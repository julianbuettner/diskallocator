# DiskAlloc
For really big vectors: allocate on disk.

## Motivation
If you have really big vectors, you might want to
keep them not in memory, but on disk.  
There appears to be no way arround serializing and deserializing,
but with this crate, you can allocate "memory" directly on disk!  

## How does it work?
Linux provides memory mapping of files.
This means you can have a file which is 1TB big,
map it to an address in userspace and use it as if you
would actually have 1TB of memory.

The OS swaps in and out pages as it thinks make sense,
and takes care of everything.

Vectors usually double and half in size.  
This crate first maps 512GiB of a 0B file in memory.  
If your vector resizes, DiskAlloc grows or shrinks the file
to your needs. The mapping is not guaranteed to be growable,
which is why it has to be very big from the beginning.

## Pitfalls
Doing IO can inherently fail.  
Therefore, you should use `Vec::try_reserve()` if you want to be sure
the resizing of the Vector should not crash your entire program.

Also, beware of structs keeping data on the heap.  
`Vec<String>` keeps everything on the heap, only pointers
to the strings on heap would be stored on disk, which does not
make any sense at all.  
Take a look at
[SmallString](https://docs.rs/stack-string/latest/stack_string/small_string/enum.SmallString.html)
on how to keep Strings as much as possible in the vector itself.

Also take a look at [swapvec](https://crates.io/crates/swapvec)
if you have de/serializable items and can work with limitted vector
functionality.

Use exactly one instance of `DiskAlloc` for exactly one vector.  
All optimizations are gone as soon as you use multiple vectors
on one file.

Also don't create too many `DiskAlloc` instances at once.  
Every mapping requires a address range of 512GiB, so creating
too many will result in an `OutOfMemory` error.

## Notes
If you track your application in `htop`, you
will see, that htop shows high memory usage
for your process.

It includes the data which is currently hold in RAM,
but it is still counted as file buffer (see yellow part of RAM bar).

## Usage

### Simple and most safe
```rust
#![feature(allocator_api)]
use diskallocator::DiskAlloc;

fn main() {
    let alloc = DiskAlloc::new().unwrap();
    let mut v: Vec<u32, DiskAlloc> = Vec::new_in(alloc);

    for i in 0..100 {
        v.push(i);
    }
}
```

### Advanced
```rust
#![feature(allocator_api)]
use std::{fs::OpenOptions, thread, time::Duration};

use diskallocator::DiskAlloc;

// Size 4 bytes * 16 = 64B
struct Dummy {
    _txt: [char; 16],
}

impl Dummy {
    pub fn new() -> Self {
        Self {
            _txt: [
                'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', '1', '2', '3', '4', '5', '6', '7', '8',
            ],
        }
    }
}

fn main() {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("dummy.file")
        .unwrap();
    let alloc = DiskAlloc::on_file(file).unwrap();
    let mut v: Vec<Dummy, DiskAlloc> = Vec::new_in(alloc);

    let giga = 1;
    let items_per_kb = 16;

    for _ in 0..giga * items_per_kb * 1024 * 1024 {
        v.push(Dummy::new());
    }
    println!("Check file dummy.file!");
    thread::sleep(Duration::from_secs(999999));
}
```
