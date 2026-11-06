//! mimalloc that counts allocation calls on the calling thread (feature `alloc-count`, for benches only).
use std::{
    alloc::{GlobalAlloc, Layout},
    cell::Cell,
};

thread_local! {
    static CALLS: Cell<u64> = const { Cell::new(0) };
}

/// Allocations and reallocations made on this thread so far.
pub fn thread_allocations() -> u64 {
    CALLS.with(Cell::get)
}

/// The global allocator of a build with the `alloc-count` feature.
pub struct CountingAllocator;

fn count() {
    let _ = CALLS.try_with(|c| c.set(c.get() + 1));
}

// SAFETY: every method forwards to mimalloc with the same layout and pointer.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { mimalloc::MiMalloc.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { mimalloc::MiMalloc.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { mimalloc::MiMalloc.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        unsafe { mimalloc::MiMalloc.realloc(ptr, layout, new_size) }
    }
}
