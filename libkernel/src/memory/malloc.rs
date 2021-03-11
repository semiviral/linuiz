use crate::cell::SyncRefCell;
use core::alloc::Layout;

pub trait MemoryAllocator {
    fn alloc(&self, layout: Layout) -> *mut u8;
    fn alloc_to(&self, frames: &crate::memory::FrameIterator) -> *mut u8;
    fn dealloc(&self, ptr: *mut u8, layout: Layout);
    fn minimum_alignment(&self) -> usize;
}

static DEFAULT_MALLOCATOR: SyncRefCell<&'static dyn MemoryAllocator> = SyncRefCell::new();

pub fn set(allocator: &'static dyn MemoryAllocator) {
    DEFAULT_MALLOCATOR.set(allocator);
}

pub fn get() -> &'static dyn MemoryAllocator {
    DEFAULT_MALLOCATOR.get().expect("no default allocator")
}
