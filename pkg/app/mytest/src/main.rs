#![no_std]
#![no_main]

use lib::*;

extern crate lib;

fn main() -> isize {
    let pid = sys_fork();

    if pid == 0 {
        sys_wait_pid(sys_spawn("app/sem"));
    } else {
        sys_wait_pid(sys_spawn("app/spin"));
        sys_wait_pid(pid);
    }

    0
}

entry!(main);
