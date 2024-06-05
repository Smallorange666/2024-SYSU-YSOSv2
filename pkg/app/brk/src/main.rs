#![no_std]
#![no_main]

use lib::*;

extern crate lib;

fn main() -> isize {
    let heap_start = sys_brk(None).unwrap();
    let heap_end = heap_start + 0x1000 * 8 - 8;
    println!("Heap start: {:#x}, Heap end: {:#x}", heap_start, heap_end);

    let mut ret = sys_brk(Some(heap_end)).expect("Failed to allocate heap");
    assert!(ret == heap_end, "Failed to allocate heap");

    ret = sys_brk(Some(heap_start + 0x1000 * 4 - 8)).expect("Failed to deallocate heap");
    assert!(
        ret == heap_start + 0x1000 * 4 - 8,
        "Failed to deallocate heap"
    );

    ret = sys_brk(Some(heap_start)).expect("Failed to clean up heap");
    assert!(ret == heap_start, "Failed to clean up heap");

    0
}

entry!(main);
