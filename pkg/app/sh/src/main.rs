#![no_std]
#![no_main]

extern crate lib;
use lib::*;

fn main() -> isize {
    println!("Welcome to Smallorange's shell!");
    println!("Enter \"help\" to check more information.");

    loop {
        print!("[>] ");

        let binding = stdin().read_line();
        let mut command = binding.trim().split(' ');
        let op = command.next().unwrap();
        match op {
            "help" => {
                println!("\"la\" to list all the apps");
                println!("\"ls /path/to/your/dir \" to list all the files in directory");
                println!("\"cat /path/to/your/dir \" to check the content of the file");
                println!("\"run your_app_name\" to run the app");
                println!("\"ps\" to list all the processes");
                println!("\"info\" to print current process info");
                println!("\"exit\" to exit the shell");
            }
            "la" => {
                sys_list_app();
            }
            "ls" => {
                sys_list_dir(command.next().unwrap_or("/"));
            }
            "cat" => {
                let fd = sys_open_file(command.next().unwrap_or(""));
                sys_cat(fd);
                sys_close_file(fd);
            }
            "run" => {
                let app_name = command.next().unwrap();
                let pid = sys_spawn(app_name);
                if pid == 0 {
                    println!("Failed to run app: {}", app_name);
                    continue;
                } else {
                    sys_stat();
                    println!("{} exited with {}", app_name, sys_wait_pid(pid));
                }
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
            _ => {
                println!("Unknown command: {}", op);
            }
        }
    }
    0
}

entry!(main);
