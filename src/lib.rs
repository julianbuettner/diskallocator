#![warn(missing_docs)]
#![doc = include_str!("../README.md")]
#![feature(allocator_api)]
#![feature(pointer_byte_offsets)]
mod diskalloc;

pub use diskalloc::DiskAlloc;
