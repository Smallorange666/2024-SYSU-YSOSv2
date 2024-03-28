#![no_std]
#![no_main]

use lib::{sync::Semaphore, *};

extern crate lib;

const THREAD_COUNT: usize = 3;
const MAX_LEN: usize = 20;

static SEM: [Semaphore; THREAD_COUNT + 1] = semaphore_array![0, 1, 2, 3];

fn main() -> isize {
    let mut pids = [0u16; THREAD_COUNT];
    SEM[0].init(1);
    SEM[1].init(0);
    SEM[2].init(3);
    SEM[3].init(0);

    for i in 0..THREAD_COUNT {
        let pid = sys_fork();

        if pid == 0 {
            if i == 0 {
                loop {
                    print_0();
                }
            } else if i == 1 {
                loop {
                    print_1();
                }
            } else {
                for _ in 0..MAX_LEN {
                    print_2();
                }
                sys_exit(0);
            }
        } else {
            pids[i] = pid;
        }
    }

    for i in 0..THREAD_COUNT {
        sys_wait_pid(pids[i]);
    }

    0
}

fn print_0() {
    SEM[0].wait();
    SEM[3].wait();
    print!("<");
    SEM[1].signal();
    SEM[2].signal();
}

fn print_1() {
    SEM[1].wait();
    SEM[3].wait();
    print!(">");
    SEM[0].signal();
    SEM[2].signal();
}

fn print_2() {
    SEM[2].wait();
    SEM[2].wait();
    SEM[2].wait();
    print!("_");

    SEM[3].signal();
    SEM[3].signal();
    SEM[3].signal();
}

entry!(main);
