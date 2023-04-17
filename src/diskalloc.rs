use std::{
    alloc::{Allocator, Layout},
    cell::RefCell,
    fs::{File, OpenOptions},
    os::fd::AsRawFd,
    ptr::NonNull,
    sync::{Arc, Mutex},
};

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

pub struct DiskAllocator {
    alloc: Arc<Mutex<AtomDiskAlloc>>,
}

impl AtomDiskAlloc {
    pub fn new() -> Result<Self, std::io::Error> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("lols.file")?;
        let terabyte: u64 = (1024 as u64).pow(4);
        let addr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                terabyte as libc::size_t,
                libc::PROT_WRITE,
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
            mmap: addr as *mut u8,
            size: 0.into(),
        })
    }

    fn resize(&self, size: u64) -> Result<(), std::io::Error> {
        *self.size.borrow_mut() = size;
        self.file.set_len(size)
    }

    fn get_size(&self) -> u64 {
        *self.size.borrow()
    }

    unsafe fn layout_is_end_of_file(&self, ptr: NonNull<u8>, layout: &Layout) -> bool {
        let file_end = self.mmap.offset(self.get_size() as isize);
        let interval_end = ptr.as_ptr().offset(layout.size() as isize);
        file_end == interval_end
    }
}

unsafe impl Allocator for AtomDiskAlloc {
    fn allocate(
        &self,
        layout: std::alloc::Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        println!("Alloc: {:?}", layout);
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
        layout: std::alloc::Layout,
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
        todo!()
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
        ptr: NonNull<u8>,
        old_layout: std::alloc::Layout,
        new_layout: std::alloc::Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        todo!()
    }
}

impl DiskAllocator {
    pub fn new() -> Result<Self, std::io::Error> {
        Ok(Self {
            alloc: Arc::new(Mutex::new(AtomDiskAlloc::new()?)),
        })
    }
}

unsafe impl Allocator for DiskAllocator {
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
        &self
    }
}
