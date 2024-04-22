mod context;
mod data;
mod manager;
mod paging;
mod pid;
mod process;
mod processor;
mod sync;

use crate::filesystem::get_rootfs;
use crate::memory::PAGE_SIZE;
use alloc::sync::Arc;
use alloc::vec::Vec;
pub use manager::*;
use process::*;
use storage::FileSystem;
use xmas_elf::ElfFile;

use alloc::string::{String, ToString};
pub use context::ProcessContext;
pub use data::ProcessData;
pub use paging::PageTableContext;
pub use pid::ProcessId;

use x86_64::structures::idt::PageFaultErrorCode;
use x86_64::VirtAddr;

use sync::SemaphoreResult;

// 0xffff_ff00_0000_0000 is the kernel's address space
pub const STACK_MAX: u64 = 0x0000_4000_0000_0000;

pub const STACK_MAX_PAGES: u64 = 0x100000;
pub const STACK_MAX_SIZE: u64 = STACK_MAX_PAGES * PAGE_SIZE;
pub const STACK_START_MASK: u64 = !(STACK_MAX_SIZE - 1);
// [bot..0x2000_0000_0000..top..0x3fff_ffff_ffff]
// init stack
pub const STACK_DEF_PAGE: u64 = 1;
pub const STACK_DEF_SIZE: u64 = STACK_DEF_PAGE * PAGE_SIZE;
pub const STACK_INIT_BOT: u64 = STACK_MAX - STACK_DEF_SIZE;
pub const STACK_INIT_TOP: u64 = STACK_MAX - 8;
// [bot..0xffffff0100000000..top..0xffffff01ffffffff]
// kernel stack
pub const KSTACK_MAX: u64 = 0xffff_ff02_0000_0000;
pub const KSTACK_DEF_PAGE: u64 = 512;
pub const KSTACK_DEF_SIZE: u64 = KSTACK_DEF_PAGE * PAGE_SIZE;
pub const KSTACK_INIT_BOT: u64 = KSTACK_MAX - KSTACK_DEF_SIZE;
pub const KSTACK_INIT_TOP: u64 = KSTACK_MAX - 8;

pub const KERNEL_PID: ProcessId = ProcessId(1);

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ProgramStatus {
    Running,
    Ready,
    Blocked,
    Dead,
}

/// init process manager
pub fn init(boot_info: &'static boot::BootInfo) {
    let mut kproc_data = ProcessData::new();
    trace!("Init process data: {:#?}", kproc_data);
    // set the kernel stack
    kproc_data.set_stack(VirtAddr::new(KSTACK_INIT_BOT), KSTACK_DEF_SIZE);

    trace!("Init process data: {:#?}", kproc_data);

    // kernel process
    let kproc = Process::new(
        "kernel".to_string(),
        None,
        PageTableContext::new(),
        Some(kproc_data),
    );

    // app_list
    let app_list = boot_info.loaded_apps.as_ref();
    manager::init(kproc, app_list);

    info!("Process Manager Initialized.");
}

pub fn switch(context: &mut ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        // switch to the next process
        let manager = get_process_manager();
        let pid = manager.save_current(context);
        manager.push_ready(pid);
        manager.switch_next(context);
    });
}

pub fn print_process_list() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        get_process_manager().print_process_list();
    })
}

pub fn env(key: &str) -> Option<String> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        // get current process's environment variable
        get_process_manager().current().read().env(key)
    })
}

pub fn process_exit(ret: isize) -> ! {
    x86_64::instructions::interrupts::without_interrupts(|| {
        get_process_manager().kill_current(ret);
    });

    loop {
        x86_64::instructions::hlt();
    }
}

pub fn handle_page_fault(addr: VirtAddr, err_code: PageFaultErrorCode) -> bool {
    x86_64::instructions::interrupts::without_interrupts(|| {
        get_process_manager().handle_page_fault(addr, err_code)
    })
}

pub fn list_app() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let app_list = get_process_manager().app_list();
        if app_list.is_none() {
            println!("[!] No app found in list!");
            return;
        }

        let apps = app_list
            .unwrap()
            .iter()
            .map(|app| app.name.as_str())
            .collect::<Vec<&str>>()
            .join(", ");

        // print more information like size, entry point, etc.
        println!("[+] App list: {}", apps);
        for app in app_list.unwrap() {
            println!(" {} ", app.name.as_str());
            println!("- entry: {:#x}", app.elf.header.pt2.entry_point());
            println!("- size: {} bytes", app.elf.input.len());
        }
    });
}

// pub fn spawn(name: &str) -> Option<ProcessId> {
//     let app = x86_64::instructions::interrupts::without_interrupts(|| {
//         let app_list = get_process_manager().app_list()?;
//         app_list.iter().find(|&app| app.name.eq(name))
//     })?;

//     elf_spawn(name.to_string(), &app.elf)
// }

pub fn spawn(path: &str) -> Option<ProcessId> {
    let name: Vec<&str> = path.rsplit('/').collect();
    let mut handle = get_rootfs().open_file(path).expect("");
    let mut buf = Vec::new();
    let elf = {
        handle.read_all(&mut buf).expect("");
        ElfFile::new(buf.as_slice()).unwrap()
    };
    elf_spawn(name[0].to_string(), &elf)
}

pub fn elf_spawn(name: String, elf: &ElfFile) -> Option<ProcessId> {
    let pid = x86_64::instructions::interrupts::without_interrupts(|| {
        let manager = get_process_manager();
        let process_name = name.to_lowercase();
        let parent = Arc::downgrade(&manager.current());
        let pid = manager.spawn(elf, name, Some(parent), None);

        debug!("Spawned process: {}#{}", process_name, pid);
        pid
    });

    Some(pid)
}

pub fn read(fd: u8, buf: &mut [u8]) -> isize {
    x86_64::instructions::interrupts::without_interrupts(|| get_process_manager().read(fd, buf))
}

pub fn write(fd: u8, buf: &[u8]) -> isize {
    x86_64::instructions::interrupts::without_interrupts(|| get_process_manager().write(fd, buf))
}

pub fn fork(context: &mut ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let manager = get_process_manager();
        // save_current as parent
        let pid = manager.save_current(context);
        // fork to get child
        let child = manager.fork();
        // push to child & parent to ready queue
        trace!("Process {} forked Process {}", get_pid().0, child.pid());
        manager.push_ready(child.pid());
        manager.push_ready(pid);
        // switch to next process
        manager.switch_next(context);
    })
}

pub fn exit(ret: isize, context: &mut ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let manager = get_process_manager();
        manager.wake_waiting(ret);
        manager.kill_self(ret);
        manager.switch_next(context);
    })
}

pub fn get_pid() -> ProcessId {
    processor::get_pid()
}

pub fn wait_pid(pid: ProcessId, context: &mut ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        if still_alive(pid) {
            let manager = get_process_manager();
            let now_pid = get_pid();
            manager.save_current(context);
            manager.block_proc(&now_pid);
            manager.add_waiting(pid);
            manager.switch_next(context);
        } else {
            let exit_code = get_process_manager().get_exit_code(pid).unwrap();
            context.set_rax(exit_code as usize);
        }
    });
}

#[inline]
pub fn still_alive(pid: ProcessId) -> bool {
    x86_64::instructions::interrupts::without_interrupts(|| {
        get_process_manager().is_proc_alive(&pid)
    })
}

pub fn sem_wait(key: u32, context: &mut ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let manager = get_process_manager();
        let pid = processor::get_pid();
        let ret = manager.current().write().sem_wait(key, pid);
        match ret {
            SemaphoreResult::Ok => context.set_rax(0),
            SemaphoreResult::NotExist => context.set_rax(1),
            SemaphoreResult::Block(_pid) => {
                // save, block it, then switch to next
                manager.save_current(context);
                manager.block_proc(&pid);
                manager.switch_next(context);
            }
            _ => unreachable!(),
        };
    })
}

pub fn sem_signal(key: u32, context: &mut ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let manager = get_process_manager();
        let ret = manager.current().write().sem_signal(key);
        match ret {
            SemaphoreResult::Ok => context.set_rax(0),
            SemaphoreResult::NotExist => context.set_rax(1),
            SemaphoreResult::WakeUp(pid) => manager.wake_up(pid),
            _ => unreachable!(),
        };
    })
}

pub fn new_sem(key: u32, value: usize) -> usize {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let manager = get_process_manager();
        let ret = manager.current().write().new_sem(key, value);
        if ret {
            0
        } else {
            1
        }
    })
}

pub fn remove_sem(key: u32) -> usize {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let manager = get_process_manager();
        let ret = manager.current().write().remove_sem(key);
        if ret {
            0
        } else {
            1
        }
    })
}

pub fn open_file(path: &str) -> u8 {
    x86_64::instructions::interrupts::without_interrupts(|| get_process_manager().open_file(path))
}

pub fn close_file(fd: u8) -> bool {
    x86_64::instructions::interrupts::without_interrupts(|| get_process_manager().close_file(fd))
}
