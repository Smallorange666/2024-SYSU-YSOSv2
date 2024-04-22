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
                println!("\"run /path/to/your/app \" to run the app");
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
                let buf = &mut [0u8; 1024];
                sys_read(fd, buf);
                println!(
                    "{}",
                    core::str::from_utf8(buf).unwrap_or("Failed to read file")
                );
                sys_close_file(fd);
            }
            "run" => {
                let path = command.next().unwrap();
                let name: vec::Vec<&str> = path.rsplit('/').collect();
                let pid = sys_spawn(path);
                if pid == 0 {
                    println!("Failed to run app: {}", name[0]);
                    continue;
                } else {
                    sys_stat();
                    println!("{} exited with {}", name[0], sys_wait_pid(pid));
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
