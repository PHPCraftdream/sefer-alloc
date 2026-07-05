//! Regression (SHOULD, reliability): `AllocCore::reclaim_offset` must SKIP a
//! garbled ring entry whose class field is out of range, NOT panic.
//!
//! The ring entry packs `class_idx` in 10 physical bits (0..1023), but only
//! `SMALL_CLASS_COUNT` (= 49) classes exist. A garbled entry — reachable via a
//! user heap-overflow that writes into a segment's metadata region where the
//! ring lives — can present `class_idx >= SMALL_CLASS_COUNT`. Before the fix,
//! `reclaim_offset` called `SizeClasses::block_size(class_idx)` (which indexes
//! `SIZE_CLASS_TABLE[class_idx]`) BEFORE any bounds guard, so an out-of-range
//! class indexed the table out of bounds and PANICKED inside the global
//! allocator's drain path → process abort. The function's own doc promises
//! "defence-in-depth against a garbled ring value (no abort — just skip)".
//!
//! ## Counterfactual (RED without the fix)
//!
//! `dbg_push_to_ring(p, class_idx)` packs `class_idx` verbatim into a raw ring
//! entry (no clamping), so pushing `class_idx = 63` fabricates exactly the
//! garbled entry. `dbg_drain_all_rings` then drains it through the real
//! `reclaim_offset`. WITHOUT the guard, the drain panics with an index-out-of-
//! bounds on `SIZE_CLASS_TABLE[63]` (49-element array) → the test aborts. WITH
//! the guard (`if class_idx >= SMALL_CLASS_COUNT { return false; }` at the top),
//! the entry is a no-op: the drain returns normally and the allocator stays
//! usable, which this test asserts by continuing to allocate afterwards.

#![cfg(all(feature = "alloc-core", feature = "alloc-xthread"))]

use core::alloc::Layout;

use sefer_alloc::alloc_core::AllocCore;

/// Any class index `>= SMALL_CLASS_COUNT (= 49)` that still fits the ring
/// entry's 10-bit class field. 63 is comfortably in range for the field and
/// out of range for the table.
const GARBLED_CLASS: usize = 63;

#[test]
fn reclaim_offset_skips_garbled_out_of_range_class_without_panic() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(8, 8).unwrap(); // class 0

    // Allocate one real block so we have a live, in-range offset to stamp the
    // garbled class onto. The OFFSET is valid; only the CLASS is garbled — this
    // isolates the class-bounds guard from the offset/alignment guards that come
    // after it in `reclaim_offset`.
    let p = ac.alloc(layout);
    assert!(!p.is_null(), "alloc returned null");

    // Fabricate the garbled ring entry: a valid offset, an out-of-range class.
    // `dbg_push_to_ring` packs the class verbatim, so this is a genuine garbled
    // packed value — exactly what a metadata-corrupting overflow could leave.
    let pushed = ac.dbg_push_to_ring(p, GARBLED_CLASS);
    assert!(
        pushed,
        "failed to push the fabricated garbled entry into the ring"
    );

    // Drain. Pre-fix this panics (index OOB on SIZE_CLASS_TABLE[63]); post-fix
    // the garbled entry is skipped and this returns cleanly.
    ac.dbg_drain_all_rings();

    // The allocator must remain fully usable after skipping the garbled entry
    // (no corruption, no poisoned free list). A fresh alloc must still succeed.
    let p2 = ac.alloc(layout);
    assert!(
        !p2.is_null(),
        "alloc after garbled-entry drain returned null"
    );

    // The garbled entry was NOT reclaimed onto any free list (it was skipped),
    // so `p` was never returned to a bin as a class-0 block via the bad path.
    // We simply confirm the allocator's own-thread free path still works, which
    // would trip on a corrupted free list.
    ac.dealloc(p2, layout);
    ac.dealloc(p, layout);
}
