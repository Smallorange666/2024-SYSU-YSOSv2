use alloc::string::String;
use crossbeam_queue::ArrayQueue;
use x86_64::instructions::interrupts;

once_mutex!(pub INPUT_BUFFER: ArrayQueue<u8>);

pub fn init() {
    init_INPUT_BUFFER(ArrayQueue::new(128));
    info!("Input Buffer initialized.");
}

guard_access_fn!(pub get_input_buffer(INPUT_BUFFER: ArrayQueue<u8>));

pub fn push_key(data: u8) {
    if let Some(buffer) = get_input_buffer() {
        if buffer.push(data).is_err() {
            warn!("INPUT_BUFFER is full");
        }
    }
}

pub fn try_pop_key() -> Option<u8> {
    interrupts::without_interrupts(|| get_input_buffer_for_sure().pop())
}

pub fn pop_key() -> u8 {
    loop {
        if let Some(data) = try_pop_key() {
            return data;
        }
    }
}

pub fn get_line() -> String {
    let mut line = String::with_capacity(256);
    loop {
        let ch = pop_key();

        match ch {
            13 => {
                println!();
                return line;
            }
            0x08 | 0x7F if !line.is_empty() => {
                print!("\x08\x20\x08");
                line.pop();
            }
            _ => {
                line.push(ch as char);
                print!("{}", ch as char);
            }
        }
    }
}
