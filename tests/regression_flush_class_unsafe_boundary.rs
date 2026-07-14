//! Regression (R6-MS-3, round5 `memory_safety_review` R5-MS-3): the doc-hidden
//! `AllocCore::flush_class` unsafe boundary and the `class_idx` release-bounds
//! gap in `BinTable::head`/`dbg_freelist_head_for`.
//!
//! Two hardened properties pinned here:
//!
//! (1) UNSAFE BOUNDARY — `flush_class` is now `pub unsafe fn` (was a *safe*
//!     `pub fn` taking caller-controlled raw pointers and raw-reading segment
//!     metadata — `SegmentMeta`/`BinTable`/bitmap/`bump`/`kind` — with no
//!     `contains_base` membership check before the raw access). The compile
//!     boundary is exercised by the `unsafe {}` call below: removing the
//!     wrapper is a compile error. This was confirmed during this task's
//!     development by temporarily reverting the `unsafe` keyword and observing
//!     `cargo check` go red on the call site.
//!
//! (2) RELEASE-MODE `class_idx` BOUNDS — `dbg_freelist_head_for` (safe) takes a
//!     raw caller-controlled `class_idx`. Pre-fix its only guard was
//!     `BinTable::head`'s `debug_assert!`, which compiles out under the
//!     `production` profile, so an out-of-range index raw-read `heads + c*4`
//!     out of bounds. Both `dbg_freelist_head_for` (pre-check) and
//!     `BinTable::head`/`set_head` (release no-op) now bound it; this test
//!     drives the safe entry point and asserts an out-of-range class returns
//!     `FREE_LIST_NULL` (`u32::MAX`) rather than reading OOB.
//!
//! ## Counterfactual (RED without the fix)
//!
//! Pre-fix, `dbg_freelist_head_for(p, 63)` in a release build read
//! `heads + 63*4` out of bounds (the reserved Phase-13.4b BinTable footprint,
//! zeroed by the OS → returned `0`); in a debug build `BinTable::head`'s
//! `debug_assert!` panicked. Post-fix it deterministically returns `u32::MAX`
//! in both profiles because `dbg_freelist_head_for`'s release pre-check fires
//! BEFORE reaching `BinTable::head`.

#![cfg(feature = "alloc-core")]

use core::alloc::Layout;

use sefer_alloc::alloc_core::AllocCore;

/// An out-of-range class index. `SMALL_CLASS_COUNT` is `pub(crate)` (= 49), so
/// `63` is comfortably past the last valid class and past the BinTable
/// footprint the pre-fix code raw-read. Mirrors the constant in
/// `regression_reclaim_offset_garbled_class.rs`.
const OOB_CLASS: usize = 63;

#[test]
fn flush_class_is_unsafe_fn_boundary_compiles_only_in_unsafe_block() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(16, 8).unwrap();
    let class = ac
        .dbg_layout_class_for(layout)
        .expect("16-byte layout must map to a small class");
    let p = ac.alloc(layout);
    assert!(!p.is_null(), "alloc returned null");
    let blocks = [p];
    // SAFETY (R6-MS-3): `p` is a fresh live allocation of `class` owned by this
    // core, freed exactly once here. The wrapper is load-bearing: removing the
    // `unsafe` keyword (reverting R6-MS-3) makes this call site a compile error,
    // verified during development.
    unsafe { ac.flush_class(class, &blocks) };

    // The allocator must remain fully usable after the flush (no corrupted free
    // list). A fresh alloc must still succeed.
    let q = ac.alloc(layout);
    assert!(!q.is_null(), "alloc after flush returned null");
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `q` is a live
    // allocation made with the matching layout, freed exactly once here.
    unsafe { ac.dealloc(q, layout) };
}

#[test]
fn dbg_freelist_head_for_rejects_out_of_range_class_without_oob_read() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(16, 8).unwrap();
    let class = ac
        .dbg_layout_class_for(layout)
        .expect("16-byte layout must map to a small class");
    let p = ac.alloc(layout);
    assert!(!p.is_null(), "alloc returned null");

    // Free `p` so its class's freelist is NON-empty (head != FREE_LIST_NULL).
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `p` is a live
    // allocation made with the matching layout, freed exactly once here.
    unsafe { ac.dealloc(p, layout) };

    // The in-range class's freelist is non-empty here → its head is a real
    // segment offset, NOT `u32::MAX`. Proving this first means the `u32::MAX`
    // asserted for `OOB_CLASS` below is the bounds check firing, NOT a
    // coincidentally-empty list.
    let in_range_head = ac.dbg_freelist_head_for(p, class);
    assert_ne!(
        in_range_head,
        u32::MAX,
        "in-range class head must be non-empty after the free above"
    );

    // Out-of-range class: the R6-MS-3 release-bounds guard returns
    // `FREE_LIST_NULL` (`u32::MAX`) instead of raw-reading `heads + OOB*4` out
    // of bounds. Pre-fix this returned `0` (release) or panicked (debug).
    let oob_head = ac.dbg_freelist_head_for(p, OOB_CLASS);
    assert_eq!(
        oob_head,
        u32::MAX,
        "out-of-range class_idx must short-circuit to FREE_LIST_NULL, not OOB-read"
    );

    // The allocator must remain fully usable — the bounds guard must not have
    // corrupted any state.
    let q = ac.alloc(layout);
    assert!(!q.is_null(), "alloc after OOB-class probe returned null");
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — live, matching, once.
    unsafe { ac.dealloc(q, layout) };
}
