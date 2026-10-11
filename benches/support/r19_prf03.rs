use sefer_alloc::registry::HeapCore;
use std::alloc::Layout;
use std::hint::black_box;

pub const OBJECTS: usize = 41;
pub const BYTES: usize = 4 * 1024 * 1024;

pub fn eviction_round(heap: &mut HeapCore) {
    let layout = Layout::from_size_align(BYTES, 8).unwrap();
    let mut ptrs = [std::ptr::null_mut(); OBJECTS];
    for ptr in &mut ptrs {
        *ptr = heap.alloc(layout);
        black_box(*ptr);
        assert!(!ptr.is_null(), "R2 allocation failed");
    }
    for ptr in ptrs {
        // SAFETY: each pointer is a distinct live allocation from this heap with this exact layout; ownership is transferred exactly once.
        unsafe { heap.dealloc(ptr, layout) };
    }
}

pub fn payload_round(
    heap: &mut HeapCore,
    align: usize,
    mut observe: impl FnMut(&HeapCore, *mut u8),
) {
    let layout = Layout::from_size_align(BYTES, align).unwrap();
    let ptr = heap.alloc(layout);
    black_box(ptr);
    assert!(!ptr.is_null(), "R3 allocation failed");
    observe(heap, ptr);
    // SAFETY: ptr is the live allocation just issued by this heap with this exact layout and is freed exactly once after the observer finishes.
    unsafe { heap.dealloc(ptr, layout) };
}

pub fn payload_prefill(heap: &mut HeapCore, mut observe: impl FnMut(&HeapCore, *mut u8)) {
    for i in 0..8 {
        payload_round(heap, if i % 2 == 0 { 8 } else { 65536 }, &mut observe);
    }
}
