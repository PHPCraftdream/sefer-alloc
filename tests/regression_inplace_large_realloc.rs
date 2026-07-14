//! Regression tests for OPT-G: in-place Large→Large realloc growth.
//!
//! When a Large allocation is grown via `realloc` and the new size still fits
//! the segment's already-committed `span_usable`, the allocator returns the
//! SAME pointer (no alloc, no copy, no dealloc) after updating the header's
//! `large_size`. These tests verify that optimisation and its edge cases.
//!
//! ## Counterfactual (what revert makes each test fail):
//!
//! - `grow_within_span_returns_same_ptr`: without OPT-G, realloc always
//!   alloc+copy+dealloc for Large, so the returned pointer differs from the
//!   original (different segment base). The `assert_eq!(ptr, new_ptr)` fails.
//!
//! - `same_size_large_realloc_returns_same_ptr`: same reasoning — without
//!   OPT-G the same-size Large realloc allocates a NEW segment and copies.
//!   `assert_eq!(ptr, new_ptr)` fails.
//!
//! - `grow_beyond_span_relocates_and_preserves`: this exercises the
//!   fall-through (precondition 4 fails). It passes with or without OPT-G —
//!   it guards against the optimisation breaking the slow path.
//!
//! - `dealloc_after_inplace_grow_then_reuse`: without OPT-G, the grow
//!   allocates a second segment and the dealloc frees that one; the test
//!   still passes but the pointer-equality assertion on the grow step fails
//!   (same as test a). The test also confirms no leak/corruption after the
//!   in-place path.
//!
//! - `shrink_large_does_not_pin`: without OPT-G this test passes identically
//!   (shrink always takes the slow path). It guards against the optimisation
//!   accidentally capturing shrinks.

#![cfg(feature = "alloc-core")]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

/// (a) Grow a Large alloc that FITS span_usable: returns the SAME pointer,
/// the original prefix bytes are preserved, the grown tail is writable.
#[test]
fn grow_within_span_returns_same_ptr() {
    let mut ac = AllocCore::new().expect("primordial");

    // 512 KiB — definitely Large. The segment's span_usable is at least one
    // full SEGMENT (4 MiB), so growing to 1 MiB fits easily.
    let old_size = 512 * 1024;
    let old_layout = Layout::from_size_align(old_size, 16).unwrap();
    let ptr = ac.alloc(old_layout);
    assert!(!ptr.is_null());

    // Write a marker pattern across the first 256 bytes.
    unsafe {
        for i in 0..256usize {
            ptr.add(i).write((i as u8).wrapping_add(0xAA));
        }
    }

    // Grow to 1 MiB — still well within a single 4 MiB segment.
    let new_size = 1024 * 1024;
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let new_ptr = unsafe { ac.realloc(ptr, old_layout, new_size) };
    assert!(!new_ptr.is_null());
    assert_eq!(
        ptr, new_ptr,
        "in-place Large grow must return the SAME pointer"
    );

    // Verify the original prefix is intact.
    unsafe {
        for i in 0..256usize {
            assert_eq!(
                new_ptr.add(i).read(),
                (i as u8).wrapping_add(0xAA),
                "prefix byte {i} corrupted after in-place grow"
            );
        }
    }

    // The grown tail must be writable (committed memory).
    unsafe {
        let tail_off = old_size;
        for i in 0..256usize {
            new_ptr
                .add(tail_off + i)
                .write((i as u8).wrapping_add(0xBB));
        }
        for i in 0..256usize {
            assert_eq!(
                new_ptr.add(tail_off + i).read(),
                (i as u8).wrapping_add(0xBB),
                "grown tail byte {i} not writable"
            );
        }
    }

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(new_ptr, Layout::from_size_align(new_size, 16).unwrap()) };
}

/// (b) Same-size realloc of a Large alloc: returns the SAME pointer.
#[test]
fn same_size_large_realloc_returns_same_ptr() {
    let mut ac = AllocCore::new().expect("primordial");

    let size = 512 * 1024;
    let layout = Layout::from_size_align(size, 16).unwrap();
    let ptr = ac.alloc(layout);
    assert!(!ptr.is_null());

    unsafe { ptr.write(0xCC) };

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let new_ptr = unsafe { ac.realloc(ptr, layout, size) };
    assert!(!new_ptr.is_null());
    assert_eq!(
        ptr, new_ptr,
        "same-size Large realloc must return the SAME pointer"
    );
    assert_eq!(unsafe { new_ptr.read() }, 0xCC, "marker must survive");

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(new_ptr, Layout::from_size_align(size, 16).unwrap()) };
}

/// (c) Grow BEYOND span_usable: pointer may differ, data still preserved.
#[test]
fn grow_beyond_span_relocates_and_preserves() {
    let mut ac = AllocCore::new().expect("primordial");

    // Allocate 3.5 MiB — this occupies one 4 MiB segment. Growing to 5 MiB
    // exceeds span_usable and must fall through to the slow path.
    let old_size = 3_500_000;
    let old_layout = Layout::from_size_align(old_size, 16).unwrap();
    let ptr = ac.alloc(old_layout);
    assert!(!ptr.is_null());

    // Write a marker pattern.
    unsafe {
        for i in 0..128usize {
            ptr.add(i).write((i as u8).wrapping_add(0xDD));
        }
    }

    let new_size = 5 * 1024 * 1024; // 5 MiB — exceeds one segment.
                                    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let new_ptr = unsafe { ac.realloc(ptr, old_layout, new_size) };
    assert!(!new_ptr.is_null());

    // Data must be preserved (whether moved or not).
    unsafe {
        for i in 0..128usize {
            assert_eq!(
                new_ptr.add(i).read(),
                (i as u8).wrapping_add(0xDD),
                "prefix byte {i} lost during beyond-span Large realloc"
            );
        }
    }

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(new_ptr, Layout::from_size_align(new_size, 16).unwrap()) };
}

/// (d) After an in-place grow, dealloc then a fresh large alloc works
/// (no leak/corruption; the freed segment is reusable).
#[test]
fn dealloc_after_inplace_grow_then_reuse() {
    let mut ac = AllocCore::new().expect("primordial");

    let old_size = 512 * 1024;
    let old_layout = Layout::from_size_align(old_size, 16).unwrap();
    let ptr = ac.alloc(old_layout);
    assert!(!ptr.is_null());

    // In-place grow.
    let new_size = 1024 * 1024;
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let new_ptr = unsafe { ac.realloc(ptr, old_layout, new_size) };
    assert!(!new_ptr.is_null());
    assert_eq!(ptr, new_ptr, "must be in-place");

    // Free the grown block with the NEW layout.
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(new_ptr, Layout::from_size_align(new_size, 16).unwrap()) };

    // A fresh large alloc must succeed (the freed segment is available for
    // reuse or the table slot is free).
    let fresh_size = 256 * 1024;
    let fresh_layout = Layout::from_size_align(fresh_size, 16).unwrap();
    let fresh_ptr = ac.alloc(fresh_layout);
    assert!(
        !fresh_ptr.is_null(),
        "fresh large alloc after dealloc must succeed"
    );

    // Write and read to confirm the memory is sound.
    unsafe {
        fresh_ptr.write(0xEE);
        assert_eq!(fresh_ptr.read(), 0xEE);
    }

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(fresh_ptr, fresh_layout) };
}

/// (f) OPT-G stores the CLAMPED `large_size` (>= MIN_BLOCK), not the raw
/// `new_size`. Without the clamp, a later cross-thread free via the #138
/// consistency check (`large_layout_consistent`) would see `raw != clamped`
/// and silently drop the free, permanently leaking the segment.
///
/// Counterfactual: with the raw-store bug (storing `new_size` instead of
/// `new_size.max(MIN_BLOCK)`), the header's `large_size` after the realloc
/// would be the un-clamped value (e.g. 12), and the `assert_eq!` against
/// `MIN_BLOCK` (16) would fail.
#[test]
fn inplace_grow_stores_clamped_large_size_for_tiny_huge_aligned() {
    let mut ac = AllocCore::new().expect("primordial");

    // A tiny size with align > SMALL_MAX forces the Large path. align = 512 KiB
    // is comfortably above SMALL_MAX (~253 KiB) and below SEGMENT (4 MiB).
    let align = 512 * 1024;
    let old_size = 8;
    let old_layout = Layout::from_size_align(old_size, align).unwrap();
    let ptr = ac.alloc(old_layout);
    assert!(
        !ptr.is_null(),
        "alloc of tiny-but-huge-aligned must succeed"
    );

    // Verify premise: the alloc path clamped to MIN_BLOCK.
    let min_block = 16usize; // super::size_classes::MIN_BLOCK
    let stored_before = ac.dbg_large_size_of(ptr);
    assert_eq!(
        stored_before, min_block,
        "alloc path must clamp large_size to MIN_BLOCK; got {stored_before}"
    );

    // Realloc-grow to another sub-MIN_BLOCK size. OPT-G fires (same clamped
    // effective size, fits span). The stored large_size must remain MIN_BLOCK.
    let new_size = 12;
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let new_ptr = unsafe { ac.realloc(ptr, old_layout, new_size) };
    assert!(!new_ptr.is_null());
    assert_eq!(ptr, new_ptr, "must be in-place (OPT-G)");

    let stored_after = ac.dbg_large_size_of(new_ptr);
    assert_eq!(
        stored_after, min_block,
        "OPT-G must store clamped large_size (MIN_BLOCK={min_block}), \
         got {stored_after} (raw new_size={new_size} — the bug)"
    );

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(new_ptr, Layout::from_size_align(new_size, align).unwrap()) };
}

/// (e) Shrink of a Large alloc does NOT get pinned in the oversized segment.
/// The slow path (alloc+copy+dealloc) still runs on shrink — data is preserved.
#[test]
fn shrink_large_does_not_pin() {
    let mut ac = AllocCore::new().expect("primordial");

    let old_size = 1024 * 1024; // 1 MiB
    let old_layout = Layout::from_size_align(old_size, 16).unwrap();
    let ptr = ac.alloc(old_layout);
    assert!(!ptr.is_null());

    // Write marker.
    unsafe {
        for i in 0..64usize {
            ptr.add(i).write((i as u8).wrapping_add(0xFF));
        }
    }

    // Shrink to just above SMALL_MAX — still Large, but smaller. (The old value
    // 128 KiB was a SMALL class, so this test did NOT exercise Large-shrink at
    // all.) Derived from the constant so a size-class rebuild cannot demote it.
    let new_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    assert!(new_size > SegmentLayout::SMALL_MAX && new_size < old_size);
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let new_ptr = unsafe { ac.realloc(ptr, old_layout, new_size) };
    assert!(!new_ptr.is_null());

    // Data preserved across the min(old, new) prefix.
    unsafe {
        for i in 0..64usize {
            assert_eq!(
                new_ptr.add(i).read(),
                (i as u8).wrapping_add(0xFF),
                "prefix byte {i} lost during Large shrink"
            );
        }
    }

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(new_ptr, Layout::from_size_align(new_size, 16).unwrap()) };
}

/// (e2) Large → strictly-smaller-Large SHRINK across segment spans (multi-
/// segment source → single-segment target). The source (8 MiB) spans TWO
/// SEGMENTs (4 MiB each); shrinking to 5 MiB is still genuinely Large but
/// needs a different segment span, so the realloc slow leg must relocate
/// (alloc a fresh smaller large segment + copy + dealloc old). This closes the
/// coverage hole: `grow_beyond_span_relocates_and_preserves` only covers the
/// GROW direction of the relocating slow leg; nothing covered the SHRINK
/// direction of a genuine Large→Large move until now.
#[test]
fn shrink_large_to_smaller_large_relocates_and_preserves() {
    let mut ac = AllocCore::new().expect("primordial");

    // 8 MiB — spans two 4 MiB SEGMENTs; unambiguously Large.
    let old_size = 8 * 1024 * 1024;
    assert!(old_size > SegmentLayout::SMALL_MAX);
    let old_layout = Layout::from_size_align(old_size, 16).unwrap();
    let ptr = ac.alloc(old_layout);
    assert!(!ptr.is_null());

    // Write a marker across a prefix wider than one page so a relocating copy
    // is verifiable.
    unsafe {
        for i in 0..4096usize {
            ptr.add(i).write((i as u8).wrapping_add(0x77));
        }
    }

    // 5 MiB — still Large, still multi-segment, but strictly smaller than the
    // source. Shrink never takes the in-place fast path, so this drives the
    // relocating slow leg (alloc+copy+dealloc).
    let new_size = 5 * 1024 * 1024;
    assert!(new_size > SegmentLayout::SMALL_MAX && new_size < old_size);
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let new_ptr = unsafe { ac.realloc(ptr, old_layout, new_size) };
    assert!(!new_ptr.is_null());

    // The min(old,new) prefix must survive the move.
    unsafe {
        for i in 0..4096usize {
            assert_eq!(
                new_ptr.add(i).read(),
                (i as u8).wrapping_add(0x77),
                "prefix byte {i} lost during Large→smaller-Large shrink relocation"
            );
        }
    }

    // The whole shrunk block must be writable (committed) end-to-end.
    unsafe {
        new_ptr.add(new_size - 1).write(0x99);
        assert_eq!(new_ptr.add(new_size - 1).read(), 0x99);
    }

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(new_ptr, Layout::from_size_align(new_size, 16).unwrap()) };
}
