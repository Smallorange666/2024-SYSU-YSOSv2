#![no_std]
#![no_main]

use rand::prelude::*;
use rand_chacha::ChaCha20Rng;

use lib::{sync::Semaphore, *};

const PHILO_NUM: usize = 5;
static CHOPSTICKS_SEM: [Semaphore; PHILO_NUM] = semaphore_array![0, 1, 2, 3, 4];

fn main() -> isize {
    let mut pids = [0u16; PHILO_NUM];
    let mut rng = ChaCha20Rng::seed_from_u64(sys_time());
    for i in 0..PHILO_NUM {
        CHOPSTICKS_SEM[i].init(1);
    }

    for i in 0..PHILO_NUM {
        let pid = sys_fork();

        if pid == 0 {
            loop {
                sleep(randnum(&mut rng));
                hungry(i, &mut rng);
            }
            // sys_exit(0);
        } else {
            pids[i] = pid;
        }
    }

    for i in 0..PHILO_NUM {
        sys_wait_pid(pids[i]);
    }

    0
}

fn hungry(order: usize, rng: &mut ChaCha20Rng) {
    if order < 4 {
        sleep(randnum(rng) % 4);
        CHOPSTICKS_SEM[order].wait();
        sleep(randnum(rng) % 4);
        CHOPSTICKS_SEM[(order + 1) % PHILO_NUM].wait();
    } else {
        sleep(randnum(rng) % 4);
        CHOPSTICKS_SEM[(order + 1) % PHILO_NUM].wait();
        sleep(randnum(rng) % 4);
        CHOPSTICKS_SEM[order].wait();
    }

    sleep(randnum(rng) % 5);
    println!("Philo {} have eaten.", order + 1);
    CHOPSTICKS_SEM[order].signal();
    CHOPSTICKS_SEM[(order + 1) % PHILO_NUM].signal();
}

fn randnum(rng: &mut ChaCha20Rng) -> u64 {
    rng.gen::<u64>() % 8
}

entry!(main);
