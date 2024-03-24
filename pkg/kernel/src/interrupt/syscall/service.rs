use core::alloc::Layout;

use super::SyscallArgs;
use crate::proc;
use crate::proc::*;
use crate::runtime::get_uefi_runtime_for_sure;

pub fn spawn_process(args: &SyscallArgs) -> usize {
    // get app name by args
    // - core::str::from_utf8_unchecked
    // - core::slice::from_raw_parts
    let name = unsafe {
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(
            args.arg0 as *const u8,
            args.arg1 as usize,
        ))
    };
    // spawn the process by name
    let ret = proc::spawn(name);
    // handle spawn error, return 0 if failed
    if ret.is_none() {
        return 0;
    }
    // return pid as usize
    ret.unwrap().0 as usize
}

pub fn sys_write(args: &SyscallArgs) -> usize {
    // get buffer and fd by args
    let buf = unsafe { core::slice::from_raw_parts(args.arg1 as *const u8, args.arg2 as usize) };
    // call proc::write -> isize
    let result = proc::write(args.arg0 as u8, buf) as usize;
    // return the result as usize
    result
}

pub fn sys_read(args: &SyscallArgs) -> usize {
    let buf = unsafe { core::slice::from_raw_parts_mut(args.arg1 as *mut u8, args.arg2 as usize) };
    let result = proc::read(args.arg0 as u8, buf) as usize;
    result
}

pub fn exit_process(args: &SyscallArgs, context: &mut ProcessContext) {
    // exit process with retcode
    proc::exit(args.arg0 as isize, context);
}

pub fn list_process() {
    // list all processes
    proc::print_process_list();
}

pub fn wait_pid(args: &SyscallArgs) -> isize {
    let pid = ProcessId(args.arg0 as u16);
    if !still_alive(pid) {
        let exit_code = get_process_manager().get_exit_code(&pid).unwrap();
        println!("Process {} exited with code {}", pid, exit_code);
        return exit_code;
    } else {
        return -1;
    }
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
        return -1;
    }
}

pub fn sys_time() -> u64 {
    let uefi_runtime = get_uefi_runtime_for_sure();
    let time = uefi_runtime.get_time();
    time.hour() as u64 * 3600 + time.minute() as u64 * 60 + time.second() as u64
}

pub fn fork(context: &mut ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let manager = get_process_manager();
        // FIXME: save_current as parent
        manager.save_current(context);
        // FIXME: fork to get child
        manager.fork();
        // FIXME: push to child & parent to ready queue
        // FIXME: switch to next process
        manager.switch_next(context);
    })
}
