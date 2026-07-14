//! Regression test for task #130 — `alloc_large` mishandled requests whose
//! `align >= SEGMENT` (4 MiB).
//!
//! ## What this guards against
//!
//! `alloc_large` places the returned block at `base + hdr_aligned`, where
//! `hdr_aligned = align_up(size_of::<SegmentHeader>(), align.max(PAGE))` and
//! `base` is only ever `SEGMENT`-aligned (never aligned to anything larger).
//!
//! Two failure modes, both triggered by a legal `Layout`
//! (`Layout::from_size_align(size, align)` accepts any power-of-two `align`,
//! including huge ones used for e.g. `#[repr(align(4194304))]` buffers or
//! huge-page-friendly allocations):
//!
//!   1. `align == SEGMENT`: `hdr_aligned` rounds all the way up to `SEGMENT`,
//!      so the returned block sits at `base + SEGMENT`. That address is
//!      itself `SEGMENT`-aligned, so `dealloc`'s segment-base computation
//!      (`block & !(SEGMENT-1)`) resolves to `base + SEGMENT` — a segment
//!      that was never registered (only `base` was). `dealloc` treats the
//!      pointer as foreign and no-ops, leaking the whole segment (>= 8 MiB:
//!      the one at `base` plus the one at `base + SEGMENT` worth of address
//!      space) and one `SegmentTable` slot on every such allocation. Enough
//!      repetitions exhaust `MAX_SEGMENTS` (1024) and the next large alloc
//!      aborts via `handle_alloc_error`.
//!   2. `align > SEGMENT` (e.g. `2 * SEGMENT` = 8 MiB): `base` is only
//!      4 MiB-aligned, so `base + hdr_aligned` inherits `base`'s alignment
//!      (4 MiB) rather than the requested `align`, roughly half the time.
//!      That violates the `GlobalAlloc` contract (the returned pointer must
//!      satisfy `layout.align()`), which is undefined behaviour in the
//!      caller.
//!
//! ## The fix
//!
//! `alloc_large` now rejects `align >= SEGMENT` up front by returning null —
//! a legal `GlobalAlloc`/`AllocCore` failure signal — instead of leaking or
//! misaligning. Such huge alignments are exotic; a clean failure is strictly
//! better than a silent leak-to-abort or UB.
//!
//! ## Counterfactual (non-vacuity)
//!
//! Removing the `if align >= SEGMENT { return null }` guard at the top of
//! `alloc_large` makes cases 1 and 2 below return a non-null pointer, and
//! this test fails on the `assert!(p.is_null(), ...)` assertions. Verified
//! by hand while authoring this test (guard removed → both cases returned
//! non-null → guard restored → green again).
//!
//! ## Test shape
//!
//! Single-threaded `AllocCore` (the substrate under the `GlobalAlloc` face),
//! same harness style as `regression_large_align_no_segment_exhaustion.rs`.
//! `class_for(size, align)` maps `align >= SEGMENT` to `None` (large path)
//! unconditionally, since `need = max(size, align) >= SEGMENT` is always far
//! above `SMALL_MAX` — so all three cases below genuinely exercise
//! `alloc_large`, not the small-class path.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;

use sefer_alloc::AllocCore;

/// Mirrors the crate-private `alloc_core::os::SEGMENT` constant
/// (`1 << 22` = 4 MiB). Not reachable from an integration test, so the value
/// is duplicated here; a drift would only make this test stricter/weaker,
/// never silently vacuous, since the exact boundary values are exercised
/// directly.
const SEGMENT: usize = 1 << 22;

#[test]
fn align_equal_to_segment_returns_null_not_leaked_segment() {
    let layout = Layout::from_size_align(64, SEGMENT).expect("valid layout");
    let mut core = AllocCore::new().expect("AllocCore::new must succeed");

    let p = core.alloc(layout);
    assert!(
        p.is_null(),
        "align == SEGMENT must be rejected with null (task #130) — got {p:#p}, \
         which would have leaked a mis-registered segment on dealloc"
    );
}

#[test]
fn align_greater_than_segment_returns_null() {
    let layout = Layout::from_size_align(64, 2 * SEGMENT).expect("valid layout");
    let mut core = AllocCore::new().expect("AllocCore::new must succeed");

    let p = core.alloc(layout);
    assert!(
        p.is_null(),
        "align > SEGMENT must be rejected with null (task #130) — got {p:#p}, \
         which could have been mis-aligned (GlobalAlloc contract violation)"
    );
}

#[test]
fn control_ordinary_large_alloc_with_page_scale_align_still_works() {
    // Control: align well below SEGMENT (4096 = one PAGE) with a size large
    // enough to miss every small class, so this genuinely takes the large
    // path. The guard must not affect this case at all.
    const ALIGN: usize = 4096;
    const SIZE: usize = 1 << 20; // 1 MiB — comfortably above SMALL_MAX.

    let layout = Layout::from_size_align(SIZE, ALIGN).expect("valid layout");
    let mut core = AllocCore::new().expect("AllocCore::new must succeed");

    let p = core.alloc(layout);
    assert!(
        !p.is_null(),
        "ordinary large allocation (align={ALIGN}, size={SIZE}) must succeed"
    );
    assert_eq!(
        (p as usize) % ALIGN,
        0,
        "returned pointer {p:#p} is not aligned to {ALIGN}"
    );

    // Round-trip: write to the first and last byte, then free.
    //
    // SAFETY: `p` is valid for `SIZE` bytes per the M1 contract.
    unsafe {
        p.write(0xAB);
        p.add(SIZE - 1).write(0xCD);
    }

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { core.dealloc(p, layout) };
}
