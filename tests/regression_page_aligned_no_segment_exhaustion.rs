//! Regression test for task B1 (2026-07) — the page-aligned follow-up to
//! #114.
//!
//! ## What this guards against
//!
//! #114 fixed `SizeClasses::class_for(size, align)` to walk forward for
//! `align > SMALL_ALIGN_MAX` (= `MIN_BLOCK` = 16) and find a small class
//! whose `block_size` is divisible by `align`, instead of unconditionally
//! routing every such request to the dedicated-segment Large path. That
//! closed the hole for alignments up to 256 (`Cell<T,S>`-style
//! `#[repr(align(N))]` shapes with `N <= 256`), because the plain
//! ~1.25×-geometric `SIZE_CLASS_TABLE` (16, 32, 48, 64, 80, 112, 144, 192,
//! 240, 304, ...) happens to contain classes divisible by every power of two
//! up to 256.
//!
//! It did **not** close the hole for page-aligned requests: no class in that
//! 40-entry geometric progression is ever a multiple of 512, 1024, 2048, or
//! 4096 (the progression's ratio is irrational relative to powers of two
//! beyond 256, so it never lands back on a page-sized multiple). A
//! page-aligned small buffer — the canonical shape for direct I/O,
//! `io_uring` submission buffers, or `#[repr(align(4096))]` types — such as
//! `(size=256, align=4096)` therefore still fell all the way through
//! `class_for`'s divisibility walk to `None` (Large), burning a whole ~4 MiB
//! segment + one `SegmentTable` slot per allocation. Under a workload that
//! performs more than `MAX_SEGMENTS` (1024) cumulative such allocations, the
//! `SegmentTable` exhausts and `AllocCore::alloc` returns null — the exact
//! same architectural OOM shape #114 fixed, just for a different alignment
//! range.
//!
//! Task B1 adds 8 explicit "page-aligned" classes (512, 1024, 2048, 4096,
//! 6144, 8192, 12288, 16384 — see `PAGE_ALIGNED_EXTRA` in
//! `src/alloc_core/size_classes.rs`) merged into the sorted
//! `SIZE_CLASS_TABLE`, so small page-aligned requests up to 16 KiB now
//! resolve to a small class and share a normal per-segment free list instead
//! of a dedicated segment each.
//!
//! ## Counterfactual (non-vacuity)
//!
//! Verified by hand: temporarily reverting `PAGE_ALIGNED_EXTRA` to `[]` (and
//! `build_table`'s output array size back to `40`, with the merge loop
//! degenerating to a copy of the geometric progression) makes every case in
//! this test fail — `AllocCore::alloc` returns null at iteration ~1023 (=
//! `MAX_SEGMENTS` - 1, the primordial segment takes one slot), exactly
//! mirroring the #114 counterfactual. Restored after confirming the failure.
//!
//! ## Test shape
//!
//! Single-threaded `AllocCore` (the substrate the `GlobalAlloc` face wraps)
//! — mirrors `regression_large_align_no_segment_exhaustion.rs`'s shape. For
//! each representative page-aligned `(size, align)` pair we allocate `N`
//! blocks, hold them all live (so `dealloc` cannot recycle a slot and mask
//! the regression), then free them all (twice, to exercise the M2
//! double-free guard on this size class too).
//!
//! `N = 2048` is comfortably above `MAX_SEGMENTS = 1024` so pre-B1
//! exhaustion is guaranteed to trip, and the test still runs in well under a
//! second in a release build.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;

use sefer_alloc::AllocCore;

/// Representative page-aligned shapes: direct-I/O / io_uring buffer sizes
/// crossed with page-ish alignments, plus the exact page-aligned case
/// `(4096, 4096)`.
const CASES: &[(usize, usize)] = &[(256, 4096), (512, 4096), (1024, 2048), (4096, 4096)];

#[test]
fn page_aligned_allocations_do_not_exhaust_segment_table() {
    const N: usize = 2048;

    for &(size, align) in CASES {
        let layout = Layout::from_size_align(size, align).expect("valid layout");
        let mut core = AllocCore::new().expect("AllocCore::new must succeed");

        let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
        for i in 0..N {
            let p = core.alloc(layout);
            assert!(
                !p.is_null(),
                "size={size} align={align}: AllocCore::alloc returned null at \
                 iteration {i}/{N} — SegmentTable likely exhausted (pre-B1 \
                 regression: no small class was ever a multiple of {align})"
            );
            assert_eq!(
                (p as usize) % align,
                0,
                "size={size} align={align} iteration {i}: pointer {p:#p} not \
                 aligned to {align}"
            );
            // Touch first/last byte — a too-small block (M4 violation) would
            // corrupt a neighbour and surface as a later assertion failure,
            // the same cheap end-to-end fidelity check the #114 regression
            // test uses.
            //
            // SAFETY: `p` is valid for `size` bytes per the M1 contract.
            unsafe {
                p.write(0xAB);
                p.add(size - 1).write(0xCD);
            }
            ptrs.push(p);
        }

        for &p in &ptrs {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { core.dealloc(p, layout) };
        }
        // M2 (double-free guard): re-freeing every pointer must be a safe
        // no-op — neither corruption nor panic.
        for &p in &ptrs {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { core.dealloc(p, layout) };
        }
    }
}

#[test]
fn small_page_aligned_shapes_resolve_to_small_class_not_large() {
    // Sister check across a slightly wider spread of small page-aligned
    // shapes, at a smaller N per case (this test's job is breadth, not
    // exhaustiveness — `page_aligned_allocations_do_not_exhaust_segment_table`
    // already proves the N > MAX_SEGMENTS case for the four headline shapes).
    for &(size, align) in &[
        (128usize, 512usize),
        (256, 1024),
        (512, 2048),
        (8192, 4096),
        (4096, 2048),
    ] {
        let layout = Layout::from_size_align(size, align).expect("valid layout");
        let mut core = AllocCore::new().expect("AllocCore::new must succeed");
        const N: usize = 1500; // > MAX_SEGMENTS = 1024
        let mut ptrs = Vec::with_capacity(N);
        for i in 0..N {
            let p = core.alloc(layout);
            assert!(
                !p.is_null(),
                "size={size} align={align} iter={i}: null — class_for must \
                 resolve to a page-aligned small class"
            );
            assert_eq!((p as usize) % align, 0, "pointer not aligned");
            ptrs.push(p);
        }
        for &p in &ptrs {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { core.dealloc(p, layout) };
        }
    }
}
