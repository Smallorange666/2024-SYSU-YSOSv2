#![no_std]
#![allow(dead_code)]
#![feature(naked_functions)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(type_alias_impl_trait)]
#![feature(panic_info_message)]
#![feature(map_try_insert)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::result_unit_err)]

extern crate alloc;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate bitflags;
extern crate libm;

#[macro_use]
pub mod utils;
pub use utils::*;

#[macro_use]
pub mod drivers;
pub use drivers::*;

pub mod interrupt;
pub mod memory;
pub mod proc;
pub use proc::*;
pub use resource;

pub use alloc::format;
use boot::BootInfo;

pub fn init(boot_info: &'static BootInfo) {
    serial::init(); // init serial output
    logger::init(boot_info.log_level); // init logger syste
    runtime::init(boot_info); // init runtime system
    memory::address::init(boot_info);
    memory::gdt::init(); // init gdt
    memory::allocator::init(); // init kernel heap allocator
    proc::init(boot_info);
    interrupt::init(); // init interrupts
    memory::init(boot_info); // init memory manager
    x86_64::instructions::interrupts::enable();
    filesystem::init(); // init filesystem

    info!("Test stack grow.");
    grow_stack();
    info!("Stack grow test done.");

    info!("Interrupts Enabled.");
    info!("YatSenOS initialized.");
}

pub fn shutdown(boot_info: &'static BootInfo) -> ! {
    info!("YatSenOS shutting down.");
    unsafe {
        boot_info.system_table.runtime_services().reset(
            boot::ResetType::SHUTDOWN,
            boot::UefiStatus::SUCCESS,
            None,
        );
    }
}

pub fn wait(init: proc::ProcessId) {
    loop {
        if proc::still_alive(init) {
            x86_64::instructions::hlt(); // Why? Check reflection question 5
        } else {
            break;
        }
    }
}

#[no_mangle]
#[inline(never)]
pub fn grow_stack() {
    const STACK_SIZE: usize = 1024 * 16;
    const STEP: usize = 64;

    let mut array = [0u64; STACK_SIZE];
    info!("Stack: {:?}", array.as_ptr());

    // test write
    for i in (0..STACK_SIZE).step_by(STEP) {
        array[i] = i as u64;
    }

    // test read
    for i in (0..STACK_SIZE).step_by(STEP) {
        assert_eq!(array[i], i as u64);
    }
}
