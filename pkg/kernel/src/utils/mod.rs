#[macro_use]
mod macros;
#[macro_use]
mod regs;

pub mod func;
pub mod logger;

use crate::proc::*;
use alloc::format;
pub use macros::*;
pub use regs::*;

pub const fn get_ascii_header() -> &'static str {
    concat!(
        r"
    __  __      __  _____            ____  _____
    \ \/ /___ _/ /_/ ___/___  ____  / __ \/ ___/
     \  / __ `/ __/\__ \/ _ \/ __ \/ / / /\__ \
     / / /_/ / /_ ___/ /  __/ / / / /_/ /___/ /
    /_/\__,_/\__//____/\___/_/ /_/\____//____/

                                           v",
        env!("CARGO_PKG_VERSION")
    )
}

pub fn new_test_thread(id: &str) -> ProcessId {
    trace!("New test thread: {}", id);
    let mut proc_data = ProcessData::new();
    proc_data.set_env("id", id);

    spawn_kernel_thread(func::test, format!("#{}_test", id), Some(proc_data))
}

pub fn new_stack_test_thread() {
    trace!("new_stack_test_thread");
    let pid = spawn_kernel_thread(func::stack_test, alloc::string::String::from("stack"), None);

    // wait for progress exit
    wait(pid);
    trace!("stack_test_thread exit");
}

fn wait(pid: ProcessId) {
    loop {
        // try to get the status of the process
        let exit_code = get_process_manager().get_exit_code(&pid);

        if exit_code.is_some() {
            x86_64::instructions::hlt();
        } else {
            break;
        }
    }
}
