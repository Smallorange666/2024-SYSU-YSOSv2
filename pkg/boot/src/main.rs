#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

#[macro_use]
extern crate log;
extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use arrayvec::ArrayVec;
use elf::{load_elf, map_pages, map_physical_memory};
use uefi::prelude::*;
use x86_64::registers::control::*;
use ysos_boot::*;

mod config;

const CONFIG_PATH: &str = "\\EFI\\BOOT\\boot.conf";

#[entry]
fn efi_main(image: uefi::Handle, mut system_table: SystemTable<Boot>) -> Status {
    uefi_services::init(&mut system_table).expect("Failed to initialize utilities");

    log::set_max_level(log::LevelFilter::Info);
    info!("Running UEFI bootloader...");

    let bs = system_table.boot_services();

    // Load config
    let config = {
        let mut file = open_file(bs, CONFIG_PATH);
        config::Config::parse(load_file(bs, &mut file))
    };

    info!("Config: {:#x?}", config);

    // Load ELF files
    let elf = {
        let mut file = open_file(bs, config.kernel_path);
        xmas_elf::ElfFile::new(load_file(bs, &mut file)).unwrap()
    };

    // Load apps according to config.load_apps
    let apps = if config.load_apps {
        info!("Loading apps...");
        Some(load_apps(system_table.boot_services()))
    } else {
        info!("Skip loading apps");
        None
    };

    unsafe {
        set_entry(elf.header.pt2.entry_point() as usize);
    }

    // Load MemoryMap
    let max_mmap_size = system_table.boot_services().memory_map_size();
    let mmap_storage = Box::leak(
        vec![0; max_mmap_size.map_size + 10 * max_mmap_size.entry_size].into_boxed_slice(),
    );
    let mmap = system_table
        .boot_services()
        .memory_map(mmap_storage)
        .expect("Failed to get memory map");

    let max_phys_addr = mmap
        .entries()
        .map(|m| m.phys_start + m.page_count * 0x1000)
        .max()
        .unwrap()
        .max(0x1_0000_0000); // include IOAPIC MMIO area

    // Map ELF segments, kernel stack and physical memory to virtual memory
    let mut page_table = current_page_table();

    // Root page table is read only, disable write protect (Cr0)
    unsafe { Cr0::update(|f| f.remove(Cr0Flags::WRITE_PROTECT)) }

    // Map physical memory to specific virtual address offset
    let mut frame_allocator = UEFIFrameAllocator(bs);
    map_physical_memory(
        config.physical_memory_offset,
        max_phys_addr,
        &mut page_table,
        &mut frame_allocator,
    );

    // Load and map the kernel elf file
    let kernelpages = {
        let mut ret = ArrayVec::new();
        if let Ok(kernelpage) = load_elf(
            &elf,
            config.physical_memory_offset,
            &mut page_table,
            &mut frame_allocator,
            false,
        ) {
            ret = ArrayVec::from_iter(kernelpage.into_iter());
        } else {
            panic!("Fail to load kernel elf file!")
        }
        ret
    };

    // Map kernel stack
    let (stack_start, stack_size) = if config.kernel_stack_auto_grow > 0 {
        let init_size = config.kernel_stack_auto_grow;
        let init_bottom =
            config.kernel_stack_address + (config.kernel_stack_size - init_size) * 0x1000;
        (init_bottom, init_size)
    } else {
        (config.kernel_stack_address, config.kernel_stack_size)
    };

    map_pages(
        stack_start,
        stack_size,
        &mut page_table,
        &mut frame_allocator,
        false,
    )
    .expect("");

    // Recover write protect (Cr0)
    unsafe {
        Cr0::update(|f| f.insert(Cr0Flags::WRITE_PROTECT));
    }

    free_elf(bs, elf);

    // Exit boot and jump to ELF entry
    info!("Exiting boot services...");

    let (runtime, mmap) = system_table.exit_boot_services(MemoryType::LOADER_DATA);
    // NOTE: alloc & log are no longer available

    // Construct BootInfo
    let bootinfo = BootInfo {
        memory_map: mmap.entries().copied().collect(),
        physical_memory_offset: config.physical_memory_offset,
        system_table: runtime,
        log_level: config.log_level,
        loaded_apps: apps,
        kernel_pages: kernelpages,
    };

    // Align stack to 8 bytes
    let stacktop = stack_start + stack_size * 0x1000 - 8;

    // Jump to the entry point
    unsafe {
        jump_to_entry(&bootinfo, stacktop);
    }
}
