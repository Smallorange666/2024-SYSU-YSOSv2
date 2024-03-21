use crate::{
    memory::gdt,
    proc::{self, *},
};
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
        Syscall::GetPid => {
            context.set_rax(cal_pid_from_stackframe(&context.stack_frame()) as usize)
        }
        // path: &str (ptr: arg0 as *const u8, len: arg1) -> pid: u16
        // spawn process from name
        Syscall::Spawn => context.set_rax(spawn_process(&args)),
        // ret: arg0 as isize
        // exit process with retcode
        Syscall::Exit => exit_process(&args, context),
        // pid: arg0 as u16 -> status: isize
        // check if the process is running or get retcode
        Syscall::WaitPid => context.set_rax(wait_pid(&args) as usize),

        // None
        Syscall::Stat => list_process(),
        // None
        Syscall::ListApp => proc::list_app(),

        // ----------------------------------------------------
        // NOTE: following syscall examples are implemented
        // ----------------------------------------------------

        // layout: arg0 as *const Layout -> ptr: *mut u8
        Syscall::Allocate => context.set_rax(sys_allocate(&args)),
        // ptr: arg0 as *mut u8
        Syscall::Deallocate => sys_deallocate(&args),
        // None
        // print process info
        Syscall::PrintInfo => context.set_rax(sys_print_info(&args) as usize),
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
