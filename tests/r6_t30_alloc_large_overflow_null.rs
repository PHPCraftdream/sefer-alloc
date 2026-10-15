//! R6-T30 (R6-CR-4) — `alloc_large`'s usable-span rounding must be checked: a
//! malformed, `usize::MAX`-scale request must return null, not overflow the
//! SEGMENT (or page, under `exact-span-large`) rounding and panic in debug.
//!
//! Counterfactual: without the fix, `n_segments * SEGMENT` (feature OFF) or
//! the page-rounding multiply (feature ON) wraps — a debug panic inside the
//! allocator (the test process aborts, going red). With the fix, every
//! overflow step returns `(null, true)`.
//!
//! The sizes here cannot be expressed as a `Layout` (`from_size_align`
//! rejects sizes above `isize::MAX`), so the test drives `alloc_large`
//! directly through the internals-gated, doc(hidden) test forwarder
//! `dbg_alloc_large_for_test`.
#![cfg(all(feature = "alloc-core", feature = "internals"))]

use sefer_alloc::AllocCore;

#[test]
fn alloc_large_overflow_returns_null() {
    let mut ac = AllocCore::new().expect("primordial");
    // Just below `usize::MAX`: `hdr_aligned + size` (checked_add) still fits,
    // so the request reaches the SEGMENT rounding — where
    // `div_ceil(SEGMENT) * SEGMENT` overflows and must yield null.
    let near_max = usize::MAX - 2 * 1024 * 1024;
    let p = ac.dbg_alloc_large_for_test(near_max, 8);
    assert!(p.is_null(), "SEGMENT-rounding overflow must return null");
    // Extreme case: the very first `checked_add` overflow step.
    let p = ac.dbg_alloc_large_for_test(usize::MAX - 4096, 8);
    assert!(p.is_null(), "usize::MAX-scale request must return null");
}
