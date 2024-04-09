#[macro_use]
mod macros;
#[macro_use]
mod regs;

pub mod func;
pub mod logger;
pub mod resource;
pub mod runtime;

use crate::proc::*;
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

pub fn wait(pid: ProcessId) {
    loop {
        // try to get the status of the process
        let exit_code = get_process_manager().get_exit_code(pid);
        if exit_code.is_none() {
            x86_64::instructions::hlt();
        } else {
            break;
        }
    }
}
