#![warn(missing_docs)]
#![doc = include_str!("../README.md")]
#![feature(allocator_api)]
mod diskalloc;

pub use diskalloc::DiskAlloc;
