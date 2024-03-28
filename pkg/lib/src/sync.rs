use core::{
    hint::spin_loop,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::*;

pub struct SpinLock {
    bolt: AtomicBool,
}

impl SpinLock {
    pub const fn new() -> Self {
        Self {
            bolt: AtomicBool::new(false),
        }
    }

    pub fn acquire(&self) {
        // acquire the lock, spin if the lock is not available
        loop {
            if self
                .bolt
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                spin_loop();
            } else {
                break;
            }
        }
    }

    pub fn release(&self) {
        // release the lock
        self.bolt.store(false, Ordering::SeqCst);
    }
}

unsafe impl Sync for SpinLock {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Semaphore {
    key: u32,
}

impl Semaphore {
    pub const fn new(key: u32) -> Self {
        Semaphore { key }
    }

    #[inline(always)]
    pub fn init(&self, value: usize) -> bool {
        sys_new_sem(self.key, value)
    }

    /* FIXME: other functions with syscall... */
    #[inline(always)]
    pub fn remove(&self) -> bool {
        sys_remove_sem(self.key)
    }

    #[inline(always)]
    pub fn wait(&self) -> bool {
        sys_sem_wait(self.key)
    }

    pub fn signal(&self) -> bool {
        sys_sem_signal(self.key)
    }
}

unsafe impl Sync for Semaphore {}

#[macro_export]
macro_rules! semaphore_array {
    [$($x:expr),+ $(,)?] => {
        [ $($crate::Semaphore::new($x),)* ]
    }
}
