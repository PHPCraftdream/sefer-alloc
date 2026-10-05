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
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    for size in 1..=7 {
        let layout = Layout::from_size_align(size, 1).unwrap();
        let mut blocks = [std::ptr::null_mut(); 24];
        for block in &mut blocks {
            *block = heap.alloc(layout);
            assert!(!block.is_null());
        }
        for block in blocks {
            // SAFETY: `block` is a live allocation of this heap with `layout`.
            unsafe { heap.dealloc(narrow(block), layout) };
        }
        for _ in 0..blocks.len() {
            let block = heap.alloc(layout);
            assert!(!block.is_null());
            // SAFETY: `block` is a live allocation of this heap with `layout`.
            unsafe { heap.dealloc(block, layout) };
        }
    }

    let one = Layout::from_size_align(1, 1).unwrap();
    let block = heap.alloc(one);
    assert!(!block.is_null());
    // SAFETY: the first byte of this live block is writable.
    unsafe { block.write(0x5a) };
    let grown = unsafe { heap.realloc(narrow(block), one, 7) };
    assert_eq!(grown.addr(), block.addr());
    for i in 1..6 {
        // SAFETY: successful realloc provides seven writable bytes.
        unsafe { grown.add(i).write(0) };
    }
    // SAFETY: byte six lies within the resized block.
    unsafe { grown.add(6).write(0x39) };
    let seven = Layout::from_size_align(7, 1).unwrap();
    let moved = unsafe { heap.realloc(narrow(grown), seven, 4097) };
    assert!(!moved.is_null());
    // SAFETY: the copied bytes are initialized; the new block is freed once.
    unsafe {
        assert_eq!(moved.read(), 0x5a);
        assert_eq!(moved.add(6).read(), 0x39);
        heap.dealloc(moved, Layout::from_size_align(4097, 1).unwrap());
    }

    let large = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let big = heap.alloc(large);
    assert!(!big.is_null());
    // SAFETY: the first byte of this live block is writable.
    unsafe { big.write(0x73) };
    let resized = unsafe { heap.realloc(narrow(big), large, large.size() + 1) };
    // An exact span (`exact-span-large`) has no slack for +1 byte unless reserved
    // capacity exists; `numa-aware` reserves exactly, so that grow legitimately moves.
    if cfg!(not(feature = "exact-span-large"))
        || cfg!(all(
            feature = "large-reserved-capacity",
            not(feature = "numa-aware")
        ))
    {
        assert_eq!(resized.addr(), big.addr());
    }
    // SAFETY: the preserved byte is initialized; the resized block is released once.
    unsafe {
        assert_eq!(resized.read(), 0x73);
        heap.dealloc(
            resized,
            Layout::from_size_align(large.size() + 1, 8).unwrap(),
        );
    }
    drop(lease);
}

#[cfg(feature = "batch-api")]
#[test]
fn own_batch_stages_only_physical_blocks() {
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = Layout::from_size_align(1, 1).unwrap();
    let mut blocks = [std::ptr::null_mut(); 80];
    // SAFETY: each batch slot receives a distinct live block from the exclusive heap.
    let n = heap.alloc_batch(layout, &mut blocks);
    assert_eq!(n, blocks.len());
    // SAFETY: every returned block is live and has a writable first byte.
    let narrow_blocks = blocks.map(|p| unsafe { narrow(p) });
    unsafe { heap.dealloc_batch(layout, &narrow_blocks) };
    let mut reused = [std::ptr::null_mut(); 80];
    assert_eq!(heap.alloc_batch(layout, &mut reused), reused.len());
    // SAFETY: each reused block is live.
    unsafe {
        heap.dealloc_batch(layout, &reused);
    }
    drop(lease);
}
