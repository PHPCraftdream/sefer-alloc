//! Ph5c gap (д)-P2: GlobalAlloc `realloc` crossing the Small↔Large kind
//! boundary while the alignment stays HIGH (> 16), exercised through an
//! *installed* `#[global_allocator]` (not `AllocCore` directly — that is
//! already covered by `r8_large_alignment.rs`).
//!
//! What is actually proven:
//! - `SeferAlloc::realloc` (GlobalAlloc trait, `old_layout.align()` preserved
//!   by the trait contract) serves align = 32..1024, 4096, and 2·page_size()
//!   (> page, non-literal — `aligned_vmem::page_size()`).
//! - Content survives S→L (grow across the kind boundary) and L→S (shrink
//!   back): the move leg's `min(old, new)` copy is checked byte-for-byte.
//! - `alloc_zeroed` at high align returns zeroed memory.
//! - The kind switch genuinely happens on both legs: the Small leg uses a
//!   size far below the small-class table top (512 B) and the Large leg a
//!   size `>= SEGMENT` (the huge threshold, `src/alloc_core/platform/
//!   size_classes.rs::HUGE_THRESHOLD`), so `class_for(size, align)` returns
//!   `None` and `HeapCore` MUST take the Large leg — routing per
//!   `src/registry/heap_core/alloc/hot.rs` (high align does NOT force Large:
//!   `class_for` is align-aware, so 512 B @ align 4096 stays Small).
//! - A 256 KiB @ align 64 allocation is grown L→L/cross and shrunk L→S.
//! - Full dealloc + a fresh round-trip afterwards (allocator still healthy).

#![cfg(all(feature = "alloc-global", feature = "internals"))]

use core::alloc::GlobalAlloc;
use std::alloc::Layout;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Page size at runtime — never a literal 4096 (Ph5b §C7 rule).
fn page() -> usize {
    aligned_vmem::page_size()
}

/// A size that is unambiguously LARGE: `>= SEGMENT` (= the huge threshold,
/// `HUGE_THRESHOLD == SEGMENT`) so `class_for` returns `None` regardless of
/// feature set (medium-classes or not) and the request routes to the Large
/// leg. 4 MiB + one page keeps it strictly above SEGMENT on any page size.
fn large_size() -> usize {
    4 * 1024 * 1024 + page()
}

const SMALL_SIZE: usize = 512;

fn pattern(i: usize) -> u8 {
    (i as u8) ^ 0xa5
}

fn check_prefix(ptr: *mut u8, len: usize, what: &str) {
    // SAFETY: `ptr` is a live successful allocation of at least `len` bytes.
    unsafe {
        for i in 0..len {
            assert_eq!(ptr.add(i).read(), pattern(i), "{what}: byte {i} corrupted");
        }
    }
}

fn fill(ptr: *mut u8, len: usize) {
    // SAFETY: writable allocation of `len` bytes.
    unsafe {
        for i in 0..len {
            ptr.add(i).write(pattern(i));
        }
    }
}

#[test]
fn global_realloc_high_align_small_large_roundtrip() {
    let mut aligns: Vec<usize> = vec![32, 64, 256, 1024, 4096];
    let super_page = 2 * page();
    if super_page > 4096 {
        aligns.push(super_page);
    }

    for &align in &aligns {
        let small_layout = Layout::from_size_align(SMALL_SIZE, align).unwrap();
        // alloc_zeroed at high align must give zeros (Small kind leg).
        let ptr = unsafe { GLOBAL.alloc_zeroed(small_layout) };
        assert!(!ptr.is_null(), "alloc_zeroed align {align}");
        assert_eq!(ptr.addr() % align, 0, "align {align}");
        // SAFETY: SMALL_SIZE bytes are valid and zeroed.
        unsafe {
            for i in 0..SMALL_SIZE {
                assert_eq!(ptr.add(i).read(), 0, "alloc_zeroed align {align}");
            }
        }

        // Small → Large: grow across the kind boundary, high align preserved.
        fill(ptr, SMALL_SIZE);
        // SAFETY: live, uniquely owned, exact original Layout.
        let grown = unsafe { GLOBAL.realloc(ptr, small_layout, large_size()) };
        assert!(!grown.is_null(), "S→L realloc align {align}");
        assert_eq!(grown.addr() % align, 0, "S→L align {align}");
        check_prefix(grown, SMALL_SIZE, "S→L content");

        // Large → Small: shrink back across the boundary in the same chain.
        let large_layout = Layout::from_size_align(large_size(), align).unwrap();
        // SAFETY: live, uniquely owned, exact original Layout.
        let shrunk = unsafe { GLOBAL.realloc(grown, large_layout, SMALL_SIZE) };
        assert!(!shrunk.is_null(), "L→S realloc align {align}");
        assert_eq!(shrunk.addr() % align, 0, "L→S align {align}");
        check_prefix(shrunk, SMALL_SIZE, "L→S content");

        // SAFETY: exact issued start and Layout of the final block.
        unsafe { GLOBAL.dealloc(shrunk, small_layout) };
    }
}

#[test]
fn global_realloc_large_grow_then_shrink_to_small_high_align() {
    let align = 64;
    let start_size = 256 * 1024; // 256 KiB
    let start_layout = Layout::from_size_align(start_size, align).unwrap();
    let ptr = unsafe { GLOBAL.alloc(start_layout) };
    assert!(!ptr.is_null());
    assert_eq!(ptr.addr() % align, 0);
    fill(ptr, start_size);

    // Grow past SEGMENT (L→L in-place or cross — either is acceptable, the
    // oracle is content survival + alignment).
    let mid = large_size() + start_size;
    // SAFETY: live, uniquely owned, exact original Layout.
    let grown = unsafe { GLOBAL.realloc(ptr, start_layout, mid) };
    assert!(!grown.is_null(), "L→L grow");
    assert_eq!(grown.addr() % align, 0);
    check_prefix(grown, start_size, "L→L grow content");

    let mid_layout = Layout::from_size_align(mid, align).unwrap();
    // Shrink all the way back to a Small-class size at align 64 (L→S).
    // SAFETY: live, uniquely owned, exact original Layout.
    let shrunk = unsafe { GLOBAL.realloc(grown, mid_layout, SMALL_SIZE) };
    assert!(!shrunk.is_null(), "L→S shrink");
    assert_eq!(shrunk.addr() % align, 0);
    check_prefix(shrunk, start_size.min(SMALL_SIZE), "L→S shrink content");

    // SAFETY: exact issued start and Layout of the final block.
    unsafe { GLOBAL.dealloc(shrunk, Layout::from_size_align(SMALL_SIZE, align).unwrap()) };

    // Post-cleanup round-trip: the allocator must still be fully functional
    // at both extremes.
    for &(sz, al) in &[(SMALL_SIZE, 64usize), (large_size(), 256usize)] {
        let l = Layout::from_size_align(sz, al).unwrap();
        let p = unsafe { GLOBAL.alloc(l) };
        assert!(!p.is_null(), "round-trip size {sz} align {al}");
        assert_eq!(p.addr() % al, 0);
        fill(p, 32);
        check_prefix(p, 32, "round-trip");
        // SAFETY: exact issued start and Layout.
        unsafe { GLOBAL.dealloc(p, l) };
    }
}
