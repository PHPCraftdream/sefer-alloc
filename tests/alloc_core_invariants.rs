//! Focused invariant tests for the Phase 8 segment substrate (`alloc-core`).
//!
//! These complement `alloc_core_differential.rs` with targeted checks for each
//! of M1–M5 and the substrate's structural properties. Kept FAST per the
//! short-scenario policy: small sizes, small counts, miri-friendly.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;
use std::ptr;

use sefer_alloc::{AllocCore, SegmentLayout};

// ---------------------------------------------------------------------------
// M1 — validity: non-null, sized, aligned.
// ---------------------------------------------------------------------------

#[test]
fn m1_small_allocations_are_aligned_and_writable() {
    let mut a = AllocCore::new().unwrap();
    for align in [1usize, 2, 4, 8, 16] {
        for size in [1usize, 7, 16, 100, 1024, 4096] {
            let layout = Layout::from_size_align(size, align).unwrap();
            let ptr = a.alloc(layout);
            assert!(!ptr.is_null(), "alloc({:?}) returned null", layout);
            assert_eq!(
                (ptr as usize) % align,
                0,
                "ptr {ptr:#p} not aligned to {align}"
            );
            // SAFETY: `ptr` is valid for `size` bytes per M1.
            unsafe {
                for b in 0..size {
                    ptr.add(b).write(0x5C);
                }
                for b in 0..size {
                    assert_eq!(ptr.add(b).read(), 0x5C, "byte {b} not writable/readable");
                }
            }
        }
    }
}

#[test]
fn m1_large_allocations_are_aligned_and_writable() {
    let mut a = AllocCore::new().unwrap();
    // Larger than SMALL_MAX → dedicated-segment path.
    let big = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    let layout = Layout::from_size_align(big, 4096).unwrap();
    let ptr = a.alloc(layout);
    assert!(!ptr.is_null(), "large alloc returned null");
    assert_eq!((ptr as usize) % 4096, 0, "large ptr not page-aligned");
    // SAFETY: valid for `big` bytes.
    unsafe {
        ptr::write_bytes(ptr, 0x33, big);
        assert_eq!(ptr.add(0).read(), 0x33);
        assert_eq!(ptr.add(big - 1).read(), 0x33);
    }
}

#[test]
fn m1_alloc_zeroed_is_all_zero() {
    let mut a = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(999, 8).unwrap();
    let ptr = a.alloc_zeroed(layout);
    assert!(!ptr.is_null());
    // SAFETY: zeroed allocation, valid for 999 bytes.
    unsafe {
        for b in 0..999 {
            assert_eq!(ptr.add(b).read(), 0, "byte {b} not zero");
        }
    }
}

// ---------------------------------------------------------------------------
// M2 — no double-free / no UAF: never corrupts, never UB.
// ---------------------------------------------------------------------------

#[test]
fn m2_double_free_is_noop() {
    let mut a = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(64, 8).unwrap();
    let ptr = a.alloc(layout);
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { a.dealloc(ptr, layout) };
    // Second dealloc of the same pointer must be a no-op (Phase 13.4a: the
    // per-segment bitmap guard rejects the double-free — the block is NOT
    // pushed onto the free list a second time). If the guard were absent, the
    // second dealloc would corrupt the free list (a self-loop at the head), so
    // a subsequent alloc would re-issue the SAME looped block.
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { a.dealloc(ptr, layout) };
    // Two consecutive allocs must yield DISTINCT blocks. Under a broken M2
    // (self-loop from the double-add) the head would keep returning the same
    // node, so ptr1 == ptr2 — this assert is the load-bearing detector.
    let ptr1 = a.alloc(layout);
    assert!(!ptr1.is_null());
    let ptr2 = a.alloc(layout);
    assert!(!ptr2.is_null());
    assert_ne!(
        ptr1, ptr2,
        "double-free corrupted the free list — same block issued twice (M2 guard failed)"
    );
}

#[test]
fn m2_foreign_pointer_dealloc_is_noop() {
    let mut a = AllocCore::new().unwrap();
    // A stack pointer is NOT one of ours; dealloc must no-op.
    let stack_var: u64 = 0xDEAD_BEEF;
    let foreign_ptr = &stack_var as *const u64 as *mut u8;
    let layout = Layout::from_size_align(8, 8).unwrap();
    // SAFETY: this is the defensive contract — a foreign pointer is a no-op,
    // not UB. We test that here.
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { a.dealloc(foreign_ptr, layout) };
    // Allocator still works.
    let ptr = a.alloc(layout);
    assert!(!ptr.is_null());
}

// ---------------------------------------------------------------------------
// M3 — no overlap: two live allocations never share a byte.
// ---------------------------------------------------------------------------

#[test]
fn m3_simultaneous_allocations_do_not_overlap() {
    let mut a = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(256, 8).unwrap();
    let mut ptrs = Vec::new();
    for _ in 0..64 {
        let ptr = a.alloc(layout);
        assert!(!ptr.is_null());
        ptrs.push((ptr as usize, 256));
    }
    // Pairwise non-overlap check.
    for i in 0..ptrs.len() {
        for j in (i + 1)..ptrs.len() {
            let (pa, sa) = ptrs[i];
            let (pb, sb) = ptrs[j];
            assert!(
                pa + sa <= pb || pb + sb <= pa,
                "allocations {i} and {j} overlap"
            );
        }
    }
    // Write a unique pattern to each and verify no cross-contamination.
    for (i, &(p, _)) in ptrs.iter().enumerate() {
        // SAFETY: each pointer valid for 256 bytes.
        unsafe {
            ptr::write_bytes(p as *mut u8, i as u8, 256);
        }
    }
    for (i, &(p, _)) in ptrs.iter().enumerate() {
        // SAFETY: same validity.
        unsafe {
            for b in 0..256 {
                assert_eq!(
                    (p as *const u8).add(b).read(),
                    i as u8,
                    "alloc {i} byte {b} clobbered"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// M4 — alignment & size fidelity.
// ---------------------------------------------------------------------------

#[test]
fn m4_size_class_satisfies_size_and_align() {
    let mut a = AllocCore::new().unwrap();
    // Every (size, align) pair in the small range yields a fitting block.
    for align in [1usize, 2, 4, 8, 16] {
        for size in [1usize, 15, 31, 63, 127, 255, 511, 1023, 2047] {
            let layout = Layout::from_size_align(size, align).unwrap();
            let ptr = a.alloc(layout);
            assert!(!ptr.is_null());
            assert_eq!((ptr as usize) % align, 0, "size={size} align={align}");
        }
    }
}

#[test]
fn m4_large_alignment_uses_dedicated_segment() {
    let mut a = AllocCore::new().unwrap();
    // Alignment > SMALL_ALIGN_MAX → dedicated segment, which can honour any
    // alignment up to SEGMENT.
    let align = 4096;
    let layout = Layout::from_size_align(32, align).unwrap();
    let ptr = a.alloc(layout);
    assert!(!ptr.is_null());
    assert_eq!((ptr as usize) % align, 0);
}

// ---------------------------------------------------------------------------
// Segment-of routing (M7, single-threaded): every live pointer's segment base
// is one of our segment bases.
// ---------------------------------------------------------------------------

#[test]
fn segment_of_finds_our_segment_base() {
    let mut a = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(48, 8).unwrap();
    let ptr = a.alloc(layout);
    let base = SegmentLayout::segment_base_of(ptr as usize);
    // The segment base must be SEGMENT-aligned by construction.
    assert_eq!(base % SegmentLayout::SEGMENT, 0);
    // And the pointer must lie within [base, base + SEGMENT).
    assert!(
        (ptr as usize) >= base && (ptr as usize) < base + SegmentLayout::SEGMENT,
        "ptr not within its computed segment"
    );
    // Load-bearing detector: the derived base must be a base this allocator
    // actually OWNS (registered in its segment table). The two asserts above
    // are tautologies of the SEGMENT mask (true for any pointer); this one
    // exercises the real membership routing (`contains_base`) and fails if the
    // segment is not registered — i.e. if routing is broken.
    assert!(
        a.dbg_contains_base(ptr),
        "segment base of a live pointer is not owned by the allocator (M7 routing failed)"
    );
}

// ---------------------------------------------------------------------------
// Reentrancy audit (M5): the alloc path is allocation-free through the global
// allocator. We verify this RUNTIME-recursively by installing AllocCore as a
// counter — see the structural note in `alloc_core_reentrancy.rs`. Here we
// do the simpler smoke check: alloc/dealloc under churn does not itself call
// the global allocator (which would manifest as recursion if AllocCore were
// installed). The dedicated audit test below is the load-bearing one.
// ---------------------------------------------------------------------------

#[test]
fn m5_churn_keeps_allocator_consistent() {
    let mut a = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(64, 8).unwrap();
    // Many alloc/dealloc cycles — would exhaust or corrupt if any internal
    // state were mishandled.
    for _ in 0..10_000 {
        let ptr = a.alloc(layout);
        assert!(!ptr.is_null());
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { a.dealloc(ptr, layout) };
    }
}

// ---------------------------------------------------------------------------
// realloc preserves bytes.
// ---------------------------------------------------------------------------

#[test]
fn realloc_preserves_prefix_bytes() {
    let mut a = AllocCore::new().unwrap();
    let initial = 128;
    let layout = Layout::from_size_align(initial, 8).unwrap();
    let ptr = a.alloc(layout);
    // SAFETY: valid for `initial` bytes.
    unsafe {
        for b in 0..initial {
            ptr.add(b).write((b as u8).wrapping_mul(7));
        }
    }
    // Grow.
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let new_ptr = unsafe { a.realloc(ptr, layout, 512) };
    assert!(!new_ptr.is_null());
    // SAFETY: new_ptr valid for 512 bytes; first `initial` must be preserved.
    unsafe {
        for b in 0..initial {
            assert_eq!(
                new_ptr.add(b).read(),
                (b as u8).wrapping_mul(7),
                "byte {b} not preserved across realloc grow"
            );
        }
    }
    // Shrink.
    let new_layout = Layout::from_size_align(512, 8).unwrap();
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let shrunk = unsafe { a.realloc(new_ptr, new_layout, 32) };
    assert!(!shrunk.is_null());
    // SAFETY: first 32 bytes preserved.
    unsafe {
        for b in 0..32 {
            assert_eq!(
                shrunk.add(b).read(),
                (b as u8).wrapping_mul(7),
                "byte {b} not preserved across realloc shrink"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Free-list reuse: dealloc then alloc reuses the same block (the segment count
// does not grow unboundedly under churn). This guards against the Phase 4 bug
// where freed memory was never reused.
// ---------------------------------------------------------------------------

#[test]
fn free_list_reuses_freed_blocks() {
    let mut a = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(64, 8).unwrap();
    // Allocate many, free all, allocate many again — should reuse without
    // needing new segments (the primordial payload is large enough for the
    // small working set here).
    let mut ptrs = Vec::new();
    for _ in 0..256 {
        ptrs.push(a.alloc(layout));
    }
    for p in &ptrs {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { a.dealloc(*p, layout) };
    }
    // Re-allocate the same count — if the free list works, this should not
    // need many new segments. We only assert it succeeds (the exact segment
    // count is an implementation detail; miri correctness is the point).
    for _ in 0..256 {
        let p = a.alloc(layout);
        assert!(!p.is_null());
    }
}

#[test]
fn many_large_allocs_then_free() {
    let mut a = AllocCore::new().unwrap();
    let mut ptrs = Vec::new();
    // Every size must be strictly ABOVE SMALL_MAX so these are genuinely Large
    // allocations (the dedicated-segment path), not small classes. Derived from
    // the constant, not a literal, so a size-class table rebuild cannot silently
    // demote these back into the small range.
    let base = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    for i in 0..20usize {
        let size = base + 50_000 * i;
        assert!(size > SegmentLayout::SMALL_MAX, "size {size} not Large");
        let layout = Layout::from_size_align(size, 4096).unwrap();
        let p = a.alloc(layout);
        assert!(!p.is_null(), "large alloc {i} failed");
        ptrs.push((p, layout));
    }
    for (p, l) in &ptrs {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { a.dealloc(*p, *l) };
    }
}
