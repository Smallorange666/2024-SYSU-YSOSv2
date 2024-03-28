#![no_std]
#![no_main]

extern crate lib;
use lib::*;

fn main() -> isize {
    println!("Welcome to Smallorange's shell!");
    println!("Enter \"help\" to check more information.");

    loop {
        print!("[>] ");

        let op = stdin().read_line();

        match op.as_str() {
            "help" => {
                println!("\"ls\" to list all the apps");
                println!("\"app_name\" to run the app");
                println!("\"ps\" to list all the processes");
                println!("\"info\" to print current process info");
                println!("\"exit\" to exit the shell");
            }
            "ls" => {
                sys_list_app();
            }
            "ps" => {
                sys_stat();
            }
            "exit" => {
                println!("Goodbye!");
                break;
            }
            "info" => {
                sys_print_info(sys_get_pid());
            }
            // "sleep" => sleep(10),
            _ => {
                let pid = sys_spawn(op.as_str());
                if pid == 0 {
                    println!("Failed to run app: {}", op);
                    continue;
                } else {
                    println!("{} exited with {}", op.as_str(), sys_wait_pid(pid));
                }
            }
        }
    }
    0
}

entry!(main);
