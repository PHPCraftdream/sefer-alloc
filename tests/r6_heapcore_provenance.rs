#![cfg(all(feature = "alloc-global", feature = "internals"))]

use std::alloc::Layout;

use sefer_alloc::registry::{bootstrap, HeapRegistry};

unsafe fn narrow(ptr: *mut u8) -> *mut u8 {
    // SAFETY: the caller supplies a live allocation with a writable first byte.
    let byte = unsafe { &mut *ptr };
    byte as *mut u8
}

#[test]
fn own_scalar_magazine_and_realloc_use_physical_block() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());

    for size in 1..=7 {
        let layout = Layout::from_size_align(size, 1).unwrap();
        let mut blocks = [std::ptr::null_mut(); 24];
        for block in &mut blocks {
            // SAFETY: `heap` is exclusively held by this test and layout is valid.
            *block = unsafe { (*heap).alloc(layout) };
            assert!(!block.is_null());
        }
        for block in blocks {
            // SAFETY: each distinct block is live and its one-byte reborrow ends at free.
            unsafe { (*heap).dealloc(narrow(block), layout) };
        }
        for _ in 0..blocks.len() {
            // SAFETY: same heap and layout; every returned block is freed once.
            let block = unsafe { (*heap).alloc(layout) };
            assert!(!block.is_null());
            // SAFETY: this is the sole live block returned immediately above.
            unsafe { (*heap).dealloc(block, layout) };
        }
    }

    let one = Layout::from_size_align(1, 1).unwrap();
    // SAFETY: heap is live and exclusively held.
    let block = unsafe { (*heap).alloc(one) };
    assert!(!block.is_null());
    // SAFETY: the first byte of this live block is writable.
    unsafe { block.write(0x5a) };
    // SAFETY: exact old layout and sole live block; the narrow reborrow is no longer used.
    let grown = unsafe { (*heap).realloc(narrow(block), one, 7) };
    assert_eq!(grown.addr(), block.addr());
    for i in 1..6 {
        // SAFETY: successful realloc provides seven writable bytes.
        unsafe { grown.add(i).write(0) };
    }
    // SAFETY: byte six lies within the resized block.
    unsafe { grown.add(6).write(0x39) };
    let seven = Layout::from_size_align(7, 1).unwrap();
    // SAFETY: all seven bytes are initialized, layout is exact, and the old block is live.
    let moved = unsafe { (*heap).realloc(narrow(grown), seven, 4097) };
    assert!(!moved.is_null());
    // SAFETY: the copied bytes are initialized and the new block is freed once.
    unsafe {
        assert_eq!(moved.read(), 0x5a);
        assert_eq!(moved.add(6).read(), 0x39);
        (*heap).dealloc(moved, Layout::from_size_align(4097, 1).unwrap());
    }

    let large = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    // SAFETY: the heap is live and the large layout is valid.
    let big = unsafe { (*heap).alloc(large) };
    assert!(!big.is_null());
    // SAFETY: the first byte of this live block is writable.
    unsafe { big.write(0x73) };
    // SAFETY: exact old layout, live block, and no reference survives the reborrow.
    let resized = unsafe { (*heap).realloc(narrow(big), large, large.size() + 1) };
    assert_eq!(resized.addr(), big.addr());
    // SAFETY: the preserved byte is initialized; the resized block and heap are released once.
    unsafe {
        assert_eq!(resized.read(), 0x73);
        (*heap).dealloc(
            resized,
            Layout::from_size_align(large.size() + 1, 8).unwrap(),
        );
        HeapRegistry::recycle(heap);
    }
}

#[cfg(feature = "batch-api")]
#[test]
fn own_batch_stages_only_physical_blocks() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(1, 1).unwrap();
    let mut blocks = [std::ptr::null_mut(); 80];
    // SAFETY: the heap is exclusive, and each batch slot receives a distinct live block.
    let n = unsafe { (*heap).alloc_batch(layout, &mut blocks) };
    assert_eq!(n, blocks.len());
    // SAFETY: every returned block is live and has a writable first byte.
    let narrow_blocks = blocks.map(|p| unsafe { narrow(p) });
    // SAFETY: each pointer starts a distinct live block of the exact layout.
    unsafe { (*heap).dealloc_batch(layout, &narrow_blocks) };
    let mut reused = [std::ptr::null_mut(); 80];
    // SAFETY: every returned block is released once below.
    assert_eq!(
        unsafe { (*heap).alloc_batch(layout, &mut reused) },
        reused.len()
    );
    // SAFETY: each reused block is live; the heap is recycled after freeing them.
    unsafe {
        (*heap).dealloc_batch(layout, &reused);
        HeapRegistry::recycle(heap);
    }
}
