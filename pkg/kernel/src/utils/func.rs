pub fn test() -> ! {
    let mut count = 0;
    let mut counts = 0;
    let id;
    if let Some(id_env) = crate::proc::env("id") {
        id = id_env
    } else {
        id = "unknown".into()
    }

    loop {
        count += 1;
        if count == 1000 {
            count = 0;
            counts += 1;
            let numid: u32 = id.parse().expect("");
            // Print some newlines to move to the correct line
            print!("\r"); // return to the beginning of the line
            for _ in 0..numid {
                print!("\n"); //go to the numid line
            }
            print!("{:-6} => {} Tick!", id, counts);
            if numid != 0 {
                print!("\x1b[{}A", numid);
            } // go back to the first line
        }
        x86_64::instructions::hlt();
    }
}

#[inline(never)]
fn huge_stack() {
    println!("Huge stack testing...");
    let mut stack = [0u64; 0x1000];

    for (idx, item) in stack.iter_mut().enumerate() {
        *item = idx as u64;
    }

    for i in 0..stack.len() / 256 {
        println!("{:#05x} == {:#05x}", i * 256, stack[i * 256]);
    }
}

pub fn stack_test() -> ! {
    trace!("stack_test");
    huge_stack();

    crate::proc::process_exit(0)
}
