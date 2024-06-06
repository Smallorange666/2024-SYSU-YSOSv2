#![no_std]
#![no_main]

use lib::*;

extern crate lib;

const PAGE_SIZE: usize = 0x1000;
const HEAP_START: usize = 0x2000_0000_0000;

fn main() -> isize {
    let heap_end = sys_brk(None).unwrap();

    println!("Try to allocate new heap");
    let new_heap_end = sys_brk(Some(heap_end + PAGE_SIZE * 10)).expect("Failed to allocate heap");

    println!("Try to write new heap");
    for i in heap_end..new_heap_end {
        let ptr = i as *mut u8;
        unsafe {
            *ptr = 1;
        }
    }

    println!("Try to read new heap");
    for i in heap_end..new_heap_end {
        let ptr = i as *mut u8;
        unsafe {
            assert_eq!(*ptr, 1);
        }
    }

    println!("Try to deallocate heap");
    sys_brk(Some(heap_end)).expect("Failed to deallocate heap");

    println!("Try to clean up the heap");
    sys_brk(Some(HEAP_START)).expect("Failed to clean up the heap");

    0
}

entry!(main);
