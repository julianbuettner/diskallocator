#![feature(allocator_api)]

use diskallocator::{self, DiskAlloc};
use rand::Rng;

#[test]
fn fill_slowly() {
    let alloc = DiskAlloc::new().unwrap();
    let mut v: Vec<u64, DiskAlloc> = Vec::new_in(alloc);
    for i in 0..9999 {
        v.push(i);
    }
}

#[test]
fn random_operations() {
    let mut rng = rand::thread_rng();

    let alloc = DiskAlloc::new().unwrap();
    let mut alloc_vec: Vec<u32, _> = Vec::new_in(alloc);
    let mut usual_vec = Vec::new();

    for _ in 0..1_000_000 {
        let branch: usize = rng.gen();

        // higher number means more pushes,
        // and less differnt operations
        match branch % 32 {
            0 => {
                alloc_vec.shrink_to_fit();
                usual_vec.shrink_to_fit();
            }
            1 => {
                alloc_vec.pop();
                usual_vec.pop();
            }
            2 => {
                let reserve = rng.gen::<usize>() % 1_000_000;
                alloc_vec.reserve(reserve);
                usual_vec.reserve(reserve);
            }
            3 => {
                if usual_vec.is_empty() {
                    return;
                }
                let truncation = rng.gen::<usize>() % usual_vec.len();
                alloc_vec.truncate(truncation);
                usual_vec.truncate(truncation);
            }
            _ => {
                let value = rng.gen();
                alloc_vec.push(value);
                usual_vec.push(value);
            }
        }
        assert_eq!(alloc_vec, usual_vec);
    }
}

#[test]
fn build_big_vec() {
    let mut rng = rand::thread_rng();

    // 1GiB of u64 (8B) makes 128M elements
    let alloc = DiskAlloc::new().unwrap();
    let mut alloc_vec: Vec<u64, _> = Vec::new_in(alloc);
    let mut usual_vec: Vec<u64> = Vec::new();

    for _ in 0..128 * 1024 {
        let value: u64 = rng.gen();
        for i in 0..1024 {
            alloc_vec.push(value);
            usual_vec.push(value ^ i);
        }
    }
    assert_eq!(alloc_vec.len(), usual_vec.len());
}

#[test]
fn multi_allocs() {
    let count = 32;

    let allocator_collection: Vec<DiskAlloc> = (0..count)
        .map(|_| DiskAlloc::new().unwrap())
        .collect::<Vec<DiskAlloc>>();
    assert_eq!(allocator_collection.len(), count);
}

#[test]
fn large_memory_vec() {
    // creates a vector that will take 60g of RAM.
    let mut v = Vec::with_capacity_in(60_000_000_000, DiskAlloc::new().unwrap());
    for byte in 0_i64..60_000_000_000 {
        if byte % 1_000_000_000 == 0 {
            println!("{byte}");
        }

        v.push(byte);
    }
    for byte in 0_i64..60_000_000_000 {
        if byte % 1_000_000_000 == 0 {
            println!("{byte}");
        }

        assert_eq!(v[byte as usize], byte);
    }
}
