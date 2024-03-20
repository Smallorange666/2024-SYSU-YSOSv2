#![no_std]
#![no_main]

extern crate lib;
use lib::*;

fn main() -> isize {
    println!("Hello, world!!!");

    233
}

entry!(main);
