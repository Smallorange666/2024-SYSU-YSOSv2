use crate::{memory::gdt, proc::*};
use alloc::format;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

// NOTE: import `ysos_syscall` package as `syscall_def` in Cargo.toml
use syscall_def::Syscall;

mod service;
use super::consts;

// write syscall service handler in `service.rs`
use service::*;

pub unsafe fn register_idt(idt: &mut InterruptDescriptorTable) {
    // register syscall handler to IDT
    // - standalone syscall stack
    // - ring 3
    idt[consts::Interrupts::Syscall as u8]
        .set_handler_fn(syscall_handler)
        .set_stack_index(gdt::SYSCALL_IST_INDEX)
        .set_privilege_level(x86_64::PrivilegeLevel::Ring3);
}

pub extern "C" fn syscall(mut context: ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        super::syscall::dispatcher(&mut context);
    });
}

as_handler!(syscall);

#[derive(Clone, Debug)]
pub struct SyscallArgs {
    pub syscall: Syscall,
    pub arg0: usize,
    pub arg1: usize,
    pub arg2: usize,
}

pub fn dispatcher(context: &mut ProcessContext) {
    let args = super::syscall::SyscallArgs::new(
        Syscall::from(context.regs.rax),
        context.regs.rdi,
        context.regs.rsi,
        context.regs.rdx,
    );
    match args.syscall {
        // fd: arg0 as u8, buf: &[u8] (ptr: arg1 as *const u8, len: arg2)
        // read from fd & return length
        Syscall::Read => context.set_rax(sys_read(&args)),
        // fd: arg0 as u8, buf: &[u8] (ptr: arg1 as *const u8, len: arg2)
        // write to fd & return length
        Syscall::Write => context.set_rax(sys_write(&args)),
        // None -> pid: u16
        // get current pid
        Syscall::GetPid => context.set_rax(sys_get_pid() as usize),
        // addr: arg0 as usize -> res: usize
        Syscall::Brk => context.set_rax(sys_brk(&args) as usize),
        // path: &str (ptr: arg0 as *const u8, len: arg1) -> pid: u16
        // spawn process from path
        Syscall::Spawn => context.set_rax(sys_spawn_process(&args)),
        // ret: arg0 as isize
        // exit process with retcode
        Syscall::Exit => sys_exit_process(&args, context),
        // pid: arg0 as u16 -> status: isize
        // block itself and wait until the process exit and be woke up
        Syscall::WaitPid => sys_wait_pid(&args, context),
        // path: &str (ptr: arg0 as *const u8, len: arg1) -> fd: u8
        // open file and return fd
        Syscall::Open => context.set_rax(sys_open_file(&args)),
        // fd: arg0 as u8 -> ret: isize
        // close file by fd
        Syscall::Close => context.set_rax(sys_close_file(&args) as usize),

        // None
        Syscall::Stat => sys_list_process(),
        // None
        Syscall::ListApp => sys_list_app(),
        // path: &str (arg0 as *const u8, arg1 as len)
        // list directory by path
        Syscall::ListDir => sys_list_dir(&args),
        // layout: arg0 as *const Layout -> ptr: *mut u8
        Syscall::Allocate => context.set_rax(sys_allocate(&args)),
        // ptr: arg0 as *mut u8
        Syscall::Deallocate => sys_deallocate(&args),
        // None
        // print process info
        Syscall::PrintInfo => context.set_rax(sys_print_info(&args) as usize),
        // get current time
        Syscall::Time => context.set_rax(sys_time() as usize),
        // None -> pid: u16 or 0 or -1
        Syscall::Fork => sys_fork(context),
        // op: u8, key: u32, val: usize -> ret: any
        Syscall::Sem => sys_sem(&args, context),
        // Unknown
        Syscall::Unknown => warn!("Unhandled syscall: {:x?}", context.regs.rax),
    }
}

impl SyscallArgs {
    pub fn new(syscall: Syscall, arg0: usize, arg1: usize, arg2: usize) -> Self {
        Self {
            syscall,
            arg0,
            arg1,
            arg2,
        }
    }
}

impl core::fmt::Display for SyscallArgs {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "SYSCALL: {:<10} (0x{:016x}, 0x{:016x}, 0x{:016x})",
            format!("{:?}", self.syscall),
            self.arg0,
            self.arg1,
            self.arg2
        )
    }
}
