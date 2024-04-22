use super::SyscallArgs;
use crate::proc::*;
use crate::runtime::get_uefi_runtime_for_sure;
use crate::{filesystem, proc};
use core::alloc::Layout;

pub fn sys_spawn_process(args: &SyscallArgs) -> usize {
    // get app by path
    // - core::str::from_utf8_unchecked
    // - core::slice::from_raw_parts
    let path = unsafe {
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(
            args.arg0 as *const u8,
            args.arg1,
        ))
    };
    // spawn the process by name
    let ret = proc::spawn(path);
    // handle spawn error, return 0 if failed
    if ret.is_none() {
        return 0;
    }
    // return pid as usize
    ret.unwrap().0 as usize
}

pub fn sys_write(args: &SyscallArgs) -> usize {
    // get buffer and fd by args
    let buf = unsafe { core::slice::from_raw_parts(args.arg1 as *const u8, args.arg2) };
    // call proc::write -> isize
    proc::write(args.arg0 as u8, buf) as usize
}

pub fn sys_read(args: &SyscallArgs) -> usize {
    let buf = unsafe { core::slice::from_raw_parts_mut(args.arg1 as *mut u8, args.arg2) };
    proc::read(args.arg0 as u8, buf) as usize
}

pub fn sys_exit_process(args: &SyscallArgs, context: &mut ProcessContext) {
    // exit process with retcode
    proc::exit(args.arg0 as isize, context);
}

pub fn sys_list_app() {
    // list all processes
    proc::list_app();
}

pub fn sys_list_process() {
    // list all processes
    proc::print_process_list();
}

pub fn sys_list_dir(args: &SyscallArgs) {
    // get path by args
    let path = unsafe {
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(
            args.arg0 as *const u8,
            args.arg1,
        ))
    };
    filesystem::ls(path);
}

pub fn sys_wait_pid(args: &SyscallArgs, context: &mut ProcessContext) {
    let pid = ProcessId(args.arg0 as u16);
    wait_pid(pid, context);
}

pub fn sys_allocate(args: &SyscallArgs) -> usize {
    let layout = unsafe { (args.arg0 as *const Layout).as_ref().unwrap() };

    if layout.size() == 0 {
        return 0;
    }

    let ret = crate::memory::user::USER_ALLOCATOR
        .lock()
        .allocate_first_fit(*layout);

    match ret {
        Ok(ptr) => ptr.as_ptr() as usize,
        Err(_) => 0,
    }
}

pub fn sys_deallocate(args: &SyscallArgs) {
    let layout = unsafe { (args.arg1 as *const Layout).as_ref().unwrap() };

    if args.arg0 == 0 || layout.size() == 0 {
        return;
    }

    let ptr = args.arg0 as *mut u8;

    unsafe {
        crate::memory::user::USER_ALLOCATOR
            .lock()
            .deallocate(core::ptr::NonNull::new_unchecked(ptr), *layout);
    }
}

pub fn sys_print_info(args: &SyscallArgs) -> isize {
    let pid = ProcessId(args.arg0 as u16);
    if still_alive(pid) && get_process_manager().print_process_info(&pid) {
        0
    } else {
        -1
    }
}

pub fn sys_time() -> u64 {
    let uefi_runtime = get_uefi_runtime_for_sure();
    let time = uefi_runtime.get_time();
    time.hour() as u64 * 3600 + time.minute() as u64 * 60 + time.second() as u64
}

pub fn sys_fork(context: &mut ProcessContext) {
    trace!("Process {} is forking", get_pid());
    fork(context);
}

pub fn sys_sem(args: &SyscallArgs, context: &mut ProcessContext) {
    match args.arg0 {
        0 => context.set_rax(new_sem(args.arg1 as u32, args.arg2)),
        1 => context.set_rax(remove_sem(args.arg1 as u32)),
        2 => sem_signal(args.arg1 as u32, context),
        3 => sem_wait(args.arg1 as u32, context),
        _ => context.set_rax(usize::MAX),
    }
}

pub fn sys_get_pid() -> u16 {
    get_pid().0
}

pub fn sys_open_file(args: &SyscallArgs) -> usize {
    let path = unsafe {
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(
            args.arg0 as *const u8,
            args.arg1,
        ))
    };
    open_file(path) as usize
}

pub fn sys_close_file(args: &SyscallArgs) -> bool {
    let fd = args.arg0 as u8;
    close_file(fd)
}
