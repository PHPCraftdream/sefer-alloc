//! R2-3 regression: `gen_at` rejects an out-of-range payload offset via a
//! RELEASE-surviving index guard.
//!
//! `gen_at(base, off)` (and `bump_gen`, `init_gen_table_in_place`) materialise
//! an atomic view into the segment's generation table at
//! `base + gen_table_off + (off >> MIN_BLOCK_SHIFT)` and load/RMW it. They
//! carried only a `debug_assert!(idx < GEN_TABLE_FOOTPRINT)` guard, which
//! COMPILES OUT in release builds — so in a release build an out-of-range `off`
//! read/wrote past the table unguarded (round2 finding R2-3 / cleanup#2).
//!
//! ## The fix
//!
//! The `debug_assert!` is now a release-surviving `assert!`. This module is
//! `#![forbid(unsafe_code)]` AND `gen_at`/`bump_gen` have internal callers
//! across OTHER `forbid` files (`alloc_core_small.rs`, `bootstrap.rs`,
//! `heap_core.rs`) that cannot host `unsafe` blocks — so the
//! `heap_registry`-style `unsafe fn` discipline (T1, commit ce887e5) cannot
//! apply. A runtime index guard is the soundness fix; base validity stays the
//! caller's contract (documented), exactly as for the `Node` seam primitives
//! these delegate to.
//!
//! ## RED→GREEN
//!
//! In a RELEASE build: before the fix `debug_assert!` compiled out, so
//! `gen_at(base, SEGMENT)` read one-past the table (the address stays inside
//! the mapped segment, so it returned a garbage byte rather than crashing) —
//! RED. After the fix the `assert!` panics — GREEN. Debug-only distinction is
//! impossible (`debug_assert!` already panicked in debug); run with `--release`.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-xthread",
    feature = "hardened"
))]

use core::alloc::Layout;

use sefer_alloc::alloc_core::segment_header::gen_at;
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::SegmentLayout;

/// `gen_at` panics on an offset whose table index is exactly at the boundary:
/// `off == SEGMENT` ⇒ `idx == SEGMENT >> MIN_BLOCK_SHIFT == GEN_TABLE_FOOTPRINT`
/// ⇒ one past the last cell.
#[test]
fn gen_at_rejects_out_of_range_offset() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(64, 8).unwrap();
    let p = ac.alloc(layout);
    assert!(!p.is_null());

    let base = SegmentLayout::segment_base_of(p as usize) as *mut u8;

    // Non-regression: a VALID, in-range offset is accepted (does not panic).
    let valid_off = (p as usize) - (base as usize);
    let _ = unsafe { gen_at(base, valid_off) };

    // `off == SEGMENT` ⇒ `idx == GEN_TABLE_FOOTPRINT` (out of range). The
    // would-be read address `gen_table_off + GEN_TABLE_FOOTPRINT` stays inside
    // the mapped segment, so pre-fix release returned a garbage byte; the
    // release-surviving `assert!` now panics.
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = unsafe { gen_at(base, SegmentLayout::SEGMENT) };
    }));
    assert!(
        r.is_err(),
        "gen_at must panic on an out-of-range offset (R2-3 release guard); got {r:?}"
    );

    ac.dealloc(p, layout);
}
