use std::{
    alloc::{Allocator, Layout},
    cell::RefCell,
    fs::File,
    os::fd::AsRawFd,
    ptr::NonNull,
    sync::{Arc, Mutex},
};

const STORAGE: u64 = 512 * 1024 * 1024 * 1024;

// Keep file and pointer to memorymap.
// Memory map can only be created once without changing
// addresses. So create once with multiple gigabytes
// of data and increase file size before allocating more.
struct AtomDiskAlloc {
    file: File,
    size: RefCell<u64>,
    mmap: *mut u8,
}

fn calc_byte_skip_for_alignment(first_free_addr: usize, alignment: usize) -> usize {
    (alignment - first_free_addr % alignment) % alignment
}

/// Manages the allocation of ideally one vector.  
/// Sits on top of a file, and resizes it as needed
/// by the vector.
///
/// Usage with vector:
/// ```rust
/// #![feature(allocator_api)]
/// let alloc = diskallocator::DiskAlloc::new().unwrap();
/// let data: Vec<u64, diskallocator::DiskAlloc> = Vec::new_in(alloc);
/// ```
#[derive(Clone)]
pub struct DiskAlloc {
    alloc: Arc<Mutex<AtomDiskAlloc>>,
}

impl Drop for AtomDiskAlloc {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.mmap.cast::<libc::c_void>(), STORAGE as libc::size_t);
        }
    }
}

impl AtomDiskAlloc {
    pub fn new() -> Result<Self, std::io::Error> {
        let file = tempfile::tempfile_in("/var/tmp/")?;
        Self::on_file(file)
    }

    pub fn on_file(file: File) -> Result<Self, std::io::Error> {
        #[cfg(target_os = "linux")]
        let addr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                STORAGE as libc::size_t,
                libc::PROT_WRITE | libc::PROT_READ,
                libc::MAP_SHARED_VALIDATE,
                file.as_raw_fd(),
                0,
            )
        };
        #[cfg(target_os = "macos")]
        let addr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                STORAGE as libc::size_t,
                libc::PROT_WRITE | libc::PROT_READ,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if addr == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self {
            file,
            mmap: addr.cast::<u8>(),
            size: 0.into(),
        })
    }

    fn resize(&self, size: u64) -> Result<(), std::io::Error> {
        *self.size.borrow_mut() = size;
        self.file.set_len(size)?;
        Ok(())
    }

    fn get_size(&self) -> u64 {
        *self.size.borrow()
    }

    unsafe fn layout_is_end_of_file(&self, ptr: NonNull<u8>, layout: &Layout) -> bool {
        let file_end = self.mmap.offset(self.get_size() as isize);
        let interval_end = ptr.as_ptr().add(layout.size());
        file_end == interval_end
    }
}

unsafe impl Allocator for AtomDiskAlloc {
    fn allocate(
        &self,
        layout: std::alloc::Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        let interval_start = self.get_size()
            + calc_byte_skip_for_alignment(self.get_size() as usize, layout.align()) as u64;
        let interval_end = interval_start + layout.size() as u64;
        self.resize(interval_end)
            .map_err(|_| std::alloc::AllocError)?;
        let start_ptr: *mut u8 = unsafe { self.mmap.offset(interval_start as isize) };
        let fat_ptr = unsafe { std::slice::from_raw_parts_mut(start_ptr, layout.size()) };
        Ok(NonNull::new(fat_ptr).unwrap())
    }

    fn by_ref(&self) -> &Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn allocate_zeroed(
        &self,
        _layout: std::alloc::Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        todo!()
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: std::alloc::Layout,
        new_layout: std::alloc::Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        // TODO how to handle different alignments?
        assert_eq!(old_layout.align(), new_layout.align());
        let growth = new_layout.size() - old_layout.size();

        if !self.layout_is_end_of_file(ptr, &old_layout) {
            // Can only grow at the end
            return self.allocate(new_layout);
        }
        self.resize(self.get_size() + growth as u64).unwrap();

        let fat_ptr = std::slice::from_raw_parts_mut(ptr.as_ptr(), new_layout.size());
        Ok(NonNull::new(fat_ptr).unwrap())
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: std::alloc::Layout,
        new_layout: std::alloc::Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        let fat_ptr = std::slice::from_raw_parts_mut(ptr.as_ptr(), new_layout.size());
        let success_result = Ok(NonNull::new(fat_ptr).unwrap());
        if !self.layout_is_end_of_file(ptr, &old_layout) {
            return success_result;
        }
        let shrinkage = old_layout.size() - new_layout.size();
        self.resize(self.get_size() - shrinkage as u64).unwrap();
        success_result
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: std::alloc::Layout) {
        if !self.layout_is_end_of_file(ptr, &layout) {
            // Vectors always deallocate at the end
            return;
        }
        self.resize(self.get_size() - layout.size() as u64).unwrap();
    }

    unsafe fn grow_zeroed(
        &self,
        _ptr: NonNull<u8>,
        _old_layout: std::alloc::Layout,
        _new_layout: std::alloc::Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        todo!()
    }
}

impl DiskAlloc {
    /// Create a new temporary file in `/var/tmp/`
    /// and wait for potential "memory" allocation.
    ///
    /// Might fail, if file can not be created
    /// or memory map fails.  
    /// An OutOfMemory error indicates, that
    /// no big enough address space could be found
    /// for the memory map (512GiB).
    pub fn new() -> Result<Self, std::io::Error> {
        Ok(Self {
            alloc: Arc::new(Mutex::new(AtomDiskAlloc::new()?)),
        })
    }

    /// Use custom file (must be read/write)
    /// to allocate "memory".
    ///
    /// Can be useful for debugging.
    ///
    /// Do not use same file twice or you will get
    /// memory access, bus or other unrecoverable hardware errors.
    pub fn on_file(file: File) -> Result<Self, std::io::Error> {
        Ok(Self {
            alloc: Arc::new(Mutex::new(AtomDiskAlloc::on_file(file)?)),
        })
    }
}

unsafe impl Allocator for DiskAlloc {
    fn allocate(
        &self,
        layout: std::alloc::Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        self.alloc.lock().unwrap().allocate(layout)
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: std::alloc::Layout,
        new_layout: std::alloc::Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        self.alloc.lock().unwrap().grow(ptr, old_layout, new_layout)
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old_layout: std::alloc::Layout,
        new_layout: std::alloc::Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        self.alloc
            .lock()
            .unwrap()
            .grow_zeroed(ptr, old_layout, new_layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: std::alloc::Layout) {
        self.alloc.lock().unwrap().deallocate(ptr, layout)
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: std::alloc::Layout,
        new_layout: std::alloc::Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        self.alloc
            .lock()
            .unwrap()
            .shrink(ptr, old_layout, new_layout)
    }

    fn allocate_zeroed(
        &self,
        layout: std::alloc::Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        self.alloc.lock().unwrap().allocate_zeroed(layout)
    }

    fn by_ref(&self) -> &Self
    where
        Self: Sized,
    {
        self
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn alloc_grow_shrink() {
        let allocator = AtomDiskAlloc::new().unwrap();
        assert_eq!(*allocator.size.borrow(), 0);
        let _alloc1 = allocator
            .allocate(Layout::from_size_align(64, 8).unwrap())
            .unwrap();
        assert_eq!(*allocator.size.borrow(), 64);
        let _alloc2 = allocator
            .allocate(Layout::from_size_align(64_000, 16).unwrap())
            .unwrap();
        assert_eq!(*allocator.size.borrow(), 64_064);
        let _alloc2a = unsafe {
            allocator
                .shrink(
                    NonNull::new(_alloc2.as_ptr().cast::<u8>()).unwrap(),
                    Layout::from_size_align(64_000, 16).unwrap(),
                    Layout::from_size_align(64, 16).unwrap(),
                )
                .unwrap()
        };
        assert_eq!(*allocator.size.borrow(), 128);
        let _alloc2b = unsafe {
            allocator.grow(
                NonNull::new(_alloc2a.as_ptr().cast::<u8>()).unwrap(),
                Layout::from_size_align(64, 16).unwrap(),
                Layout::from_size_align(128_000, 16).unwrap(),
            )
        };
        assert_eq!(*allocator.size.borrow(), 128_064);
    }
}
