//! Task #145 (P1) — the exact 256 B size class.
//!
//! Before this change the small-class table jumped 240 → 304 (a 1.25×
//! geometric step rounded to `MIN_BLOCK`), so a 256 B request resolved to the
//! 304 B class: ~18% internal waste, at the exact size where mimalloc leads on
//! churn. This adds an explicit 256 B class (merged into the sorted table as
//! one new class, `SMALL_CLASS_COUNT` 48 → 49), so a 256 B request now
//! resolves to a class whose `block_size` is exactly 256.
//!
//! Two properties:
//!   1. `class_for(256, 8)` resolves to a class with `block_size == 256`
//!      (was 304). This is the direct table-shape assertion — the
//!      counterfactual: without the new class it would be 304 and this fails.
//!   2. A 256 B allocation round-trips: alloc → write every byte → read back →
//!      free, cleanly (no corruption, no double-free fault). Also checks a few
//!      aligned 256 B shapes still round-trip (Э4 "classify once" must not have
//!      broken the aligned path).

#![cfg(feature = "alloc-global")]

use sefer_alloc::SegmentLayout;

/// The 256 B request now resolves to an exact-256 class (regression: was 304).
#[test]
fn class_for_256_resolves_to_block_size_256() {
    let idx = SegmentLayout::class_for(256, 8).expect("256 B is a small class");
    let block = SegmentLayout::SIZE_CLASS_TABLE[idx];
    assert_eq!(
        block, 256,
        "class_for(256, 8) must yield a 256 B block (was 304 before task #145); \
         got block_size = {block}"
    );

    // The table must remain strictly increasing and every entry a multiple of
    // MIN_BLOCK — the new 256 entry sits between 240 and 304 and is 16×16.
    let table = SegmentLayout::SIZE_CLASS_TABLE;
    let mb = SegmentLayout::MIN_BLOCK;
    for w in table.windows(2) {
        assert!(
            w[1] > w[0],
            "table not strictly increasing at {} -> {}",
            w[0],
            w[1]
        );
    }
    for &b in table.iter() {
        assert_eq!(b % mb, 0, "class block {b} is not a multiple of MIN_BLOCK");
    }
    // 256 is present exactly once, and 240 and 304 both still exist around it.
    assert_eq!(table.iter().filter(|&&b| b == 256).count(), 1);
    assert!(table.contains(&240), "240 B geometric class went missing");
    assert!(table.contains(&304), "304 B geometric class went missing");
}

/// A 256 B allocation round-trips cleanly through the real allocator, at
/// several alignments (the exact-256 path plus Э4's aligned magazine path).
#[test]
fn alloc_256_roundtrips_and_frees() {
    use sefer_alloc::SeferAlloc;
    use std::alloc::{GlobalAlloc, Layout};

    let a = SeferAlloc::new();
    for &align in &[8usize, 16, 32, 64, 128, 256] {
        let layout = Layout::from_size_align(256, align).unwrap();
        // Churn several rounds so the magazine (fastbin) path is exercised for
        // this class, not just the first cold carve.
        for round in 0..64 {
            // SAFETY: valid non-zero layout.
            let p = unsafe { a.alloc(layout) };
            assert!(!p.is_null(), "256 B alloc (align={align}) returned null");
            assert_eq!((p as usize) % align, 0, "256 B block not {align}-aligned");
            // SAFETY: p valid for 256 bytes.
            unsafe {
                let fill = (round as u8).wrapping_add(align as u8);
                core::ptr::write_bytes(p, fill, 256);
                assert_eq!(core::ptr::read(p), fill);
                assert_eq!(core::ptr::read(p.add(255)), fill);
                a.dealloc(p, layout);
            }
        }
    }
}
