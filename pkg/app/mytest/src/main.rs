#![no_std]
#![no_main]

use lib::*;

extern crate lib;

fn main() -> isize {
    let pid = sys_fork();

    if pid == 0 {
        test_semaphore();
    } else {
        test_spin();
        sys_wait_pid(pid);
    }

    0
}

fn test_semaphore() {
    sys_wait_pid(sys_spawn("test_sem"));
}

fn test_spin() {
    sys_wait_pid(sys_spawn("test_spin"));
}

entry!(main);
