#![cfg(feature = "alloc-global")]

use core::alloc::{GlobalAlloc, Layout};
use sefer_alloc::SeferAlloc;

fn assert_failed_realloc_preserves_old(size: usize, align: usize) {
    let allocator = SeferAlloc::new();
    let old_layout = Layout::from_size_align(size, align).unwrap();
    // SAFETY: valid layout; this pointer is paired with the exact layout below.
    let old_ptr = unsafe { GlobalAlloc::alloc(&allocator, old_layout) };
    assert!(
        !old_ptr.is_null(),
        "initial allocation of {size} bytes failed"
    );
    // SAFETY: old_ptr denotes a live allocation of old_layout.size() bytes.
    unsafe { core::ptr::write_bytes(old_ptr, 0xA7, size) };

    let huge = 1usize << 62;
    assert!(Layout::from_size_align(huge, align).is_ok());
    // SAFETY: old_ptr is live and old_layout exactly describes it. A null
    // result must leave that allocation valid and unchanged per GlobalAlloc.
    let result = unsafe { GlobalAlloc::realloc(&allocator, old_ptr, old_layout, huge) };
    assert!(
        result.is_null(),
        "realloc to impossible size unexpectedly succeeded"
    );
    // SAFETY: null realloc result preserves the original live allocation.
    let bytes = unsafe { core::slice::from_raw_parts(old_ptr, size) };
    assert_eq!(
        bytes,
        &vec![0xA7; size][..],
        "old allocation pattern changed after failed realloc"
    );
    // SAFETY: old_ptr remains live with its original exact layout.
    unsafe { GlobalAlloc::dealloc(&allocator, old_ptr, old_layout) };
}

#[test]
fn small_realloc_oom_keeps_old_allocation_alive() {
    assert_failed_realloc_preserves_old(64, 16);
}

#[test]
fn large_realloc_oom_keeps_old_allocation_alive() {
    assert_failed_realloc_preserves_old(2 * 1024 * 1024, 4096);
}
