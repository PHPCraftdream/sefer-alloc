//! R2-3 regression: `RunStack` accessors reject an out-of-range class index via
//! a RELEASE-surviving guard.
//!
//! `RunStack::push` / `pop` / `peek` / `is_empty` (all `#[doc(hidden)] pub`)
//! carried only a `debug_assert!(class < SMALL_CLASS_COUNT)` guard, which
//! COMPILES OUT in release builds — so in a release build an out-of-range
//! `class` read/wrote past the `RunStack` region unguarded (round2 finding R2-3
//! / cleanup#2).
//!
//! ## The fix
//!
//! The `debug_assert!` is now a release-surviving `assert!`. This module is
//! `#![forbid(unsafe_code)]` AND the methods have internal callers across OTHER
//! `forbid` files (`alloc_core_small.rs`, `bootstrap.rs`,
//! `alloc_core_small_pool.rs`) that cannot host `unsafe` blocks — so the
//! `heap_registry`-style `unsafe fn` discipline (T1, commit ce887e5) cannot
//! apply. A runtime class guard is the soundness fix; base validity stays the
//! caller's contract (documented).
//!
//! ## RED→GREEN
//!
//! In a RELEASE build: before the fix `debug_assert!` compiled out, so
//! `RunStack::is_empty(base, SMALL_CLASS_COUNT)` read one-past the `RunStack`
//! region (the address stays inside the mapped segment, so it returned a bool
//! rather than crashing) — RED. After the fix the `assert!` panics — GREEN.
//! Debug-only distinction is impossible; run with `--release`.

#![cfg(feature = "alloc-runfreelist")]

use core::alloc::Layout;

use sefer_alloc::alloc_core::run_stack::RunStack;
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::SegmentLayout;

/// `RunStack::is_empty` panics on `class == SMALL_CLASS_COUNT` (one past the
/// last valid class). `SMALL_CLASS_COUNT` is `pub(crate)`; `SIZE_CLASS_TABLE`
/// is the public re-export whose `.len()` IS that count.
#[test]
fn run_stack_rejects_out_of_range_class() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(16, 8).unwrap();
    let p = ac.alloc(layout);
    assert!(!p.is_null());
    let base = SegmentLayout::segment_base_of(p as usize) as *mut u8;

    let class_count = SegmentLayout::SIZE_CLASS_TABLE.len();
    let valid_class = SegmentLayout::class_for(16, 8).expect("16/8 is a small class");

    // Non-regression: a VALID class does not panic (the segment's `RunStack` is
    // carved under `alloc-runfreelist`).
    let _ = RunStack::is_empty(base, valid_class);

    // `class == SMALL_CLASS_COUNT` (== `SIZE_CLASS_TABLE.len()`) is one past the
    // last valid class index. The would-be read address (just past the
    // `RunStack` region) stays inside the mapped segment, so pre-fix release
    // returned a bool; the release-surviving `assert!` now panics.
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = RunStack::is_empty(base, class_count);
    }));
    assert!(
        r.is_err(),
        "RunStack::is_empty must panic on class == SMALL_CLASS_COUNT (R2-3 release guard); got {r:?}"
    );

    ac.dealloc(p, layout);
}
