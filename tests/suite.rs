#![feature(allocator_api)]
use diskallocator::{self, DiskAllocator};

#[test]
fn fill_slowly() {
    let alloc = DiskAllocator::new().unwrap();
    let mut v: Vec<u64, DiskAllocator> = Vec::new_in(alloc);
    for i in 0..9999 {
        v.push(i);
    }
}
