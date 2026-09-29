#![cfg(feature = "alloc-core")]

use std::alloc::Layout;

use sefer_alloc::AllocCore;

unsafe fn narrow(ptr: *mut u8) -> *mut u8 {
    // SAFETY: the caller supplies a live allocation with at least one byte.
    let byte = unsafe { &mut *ptr };
    byte as *mut u8
}

#[test]
fn one_byte_reborrows_can_be_freed_and_reused() {
    let mut core = AllocCore::new().expect("reservation");
    for size in 1..=7 {
        let layout = Layout::from_size_align(size, 1).unwrap();
        let ptr = core.alloc(layout);
        assert!(!ptr.is_null());
        // SAFETY: `ptr` is live and has at least one writable byte.
        let borrowed = unsafe { narrow(ptr) };
        // SAFETY: the reborrow starts the sole live block; the original layout
        // is exact, and no reference to the block remains in use.
        unsafe { core.dealloc(borrowed, layout) };
        let reused = core.alloc(layout);
        assert_eq!(reused.addr(), ptr.addr());
        // SAFETY: `reused` is the new, sole live allocation of this layout.
        unsafe { core.dealloc(reused, layout) };
    }
}

#[test]
fn large_one_byte_reborrow_can_be_freed() {
    let mut core = AllocCore::new().expect("reservation");
    let layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    for _ in 0..2 {
        let ptr = core.alloc(layout);
        assert!(!ptr.is_null());
        // SAFETY: `ptr` is a live Large allocation and its first byte is
        // reborrowed only until the matching single free.
        unsafe { core.dealloc(narrow(ptr), layout) };
    }
}

#[test]
fn narrow_reborrow_realloc_returns_full_physical_block() {
    let mut core = AllocCore::new().expect("reservation");
    let old = Layout::from_size_align(1, 1).unwrap();
    let ptr = core.alloc(old);
    assert!(!ptr.is_null());
    // SAFETY: `ptr` owns one writable byte.
    unsafe { ptr.write(0x6d) };
    // SAFETY: `ptr` is live, has one byte, and the exact old layout is used.
    let grown = unsafe { core.realloc(narrow(ptr), old, 7) };
    assert_eq!(grown.addr(), ptr.addr());
    // SAFETY: successful realloc returned a block with at least seven bytes.
    unsafe {
        assert_eq!(grown.read(), 0x6d);
        grown.add(6).write(0x2a);
    }
    let seven = Layout::from_size_align(7, 1).unwrap();
    // SAFETY: `grown` is live with layout `seven`; only its first byte is
    // reborrowed and no reference remains in use when ownership is returned.
    let moved = unsafe { core.realloc(narrow(grown), seven, 4097) };
    assert!(!moved.is_null());
    assert_ne!(moved.addr(), grown.addr());
    // SAFETY: realloc copied the requested seven initialized bytes.
    unsafe {
        assert_eq!(moved.read(), 0x6d);
        assert_eq!(moved.add(6).read(), 0x2a);
    }
    // SAFETY: `moved` is the sole live allocation with the new layout.
    unsafe { core.dealloc(moved, Layout::from_size_align(4097, 1).unwrap()) };
}

#[cfg(feature = "internals")]
#[test]
fn batch_flush_links_blocks_from_canonical_root() {
    let mut core = AllocCore::new().expect("reservation");
    let layout = Layout::from_size_align(1, 1).unwrap();
    let class = core.dbg_layout_class_for(layout).expect("small class");
    let mut blocks = [std::ptr::null_mut(); 4];
    assert_eq!(core.refill_class_bump(class, &mut blocks), blocks.len());
    // SAFETY: each distinct live block contains at least one byte.
    let borrowed = blocks.map(|ptr| unsafe { narrow(ptr) });
    // SAFETY: each pointer starts a distinct live block of `class`; the
    // reborrows are no longer used after ownership returns to the core.
    unsafe { core.flush_class(class, &borrowed) };
    let mut again = [std::ptr::null_mut(); 4];
    assert_eq!(
        core.refill_class(class, again.len(), &mut again),
        again.len()
    );
    for ptr in again {
        // SAFETY: each returned block is a distinct live allocation.
        unsafe { core.dealloc(ptr, layout) };
    }
}
