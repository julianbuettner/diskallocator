#![feature(allocator_api)]
#![feature(pointer_byte_offsets)]
mod diskalloc;

pub use diskalloc::DiskAllocator;
