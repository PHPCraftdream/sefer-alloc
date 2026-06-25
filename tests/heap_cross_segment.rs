//! Counterfactual regression for the Phase 12.1 segment-centric free-state
//! refactor.
//!
//! Phase 9 aggregated free blocks across ALL segments in a heap-local `bins`
//! array, so a freed block in *any* segment was reusable. The segment-centric
//! refactor (free state moved into each segment's `BinTable`) MUST preserve that
//! behaviour: after a churn that spans several 4 MiB segments and then frees
//! everything, the next round of allocations must REUSE the freed blocks
//! (bounded segment footprint) — not reserve fresh segments.
//!
//! The Phase 8–11 tests never crossed a single 4 MiB segment boundary, so a
//! refactor that only ever pops from the *current* segment's `BinTable` passes
//! them vacuously while silently leaking every non-current segment's freed
//! blocks (unbounded RSS). This test crosses several boundaries to expose that.
//!
//! It also writes the full block on every hand-out: a corrupted free-list head
//! (e.g. a cross-segment offset truncated into a `BinTable` slot) would hand out
//! an out-of-bounds / aliased block and trip here (overlap → wrong byte, or a
//! fault).
#![cfg(feature = "alloc")]

use std::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::Heap;

/// Mirror of `alloc_core::os::SEGMENT` (4 MiB). Kept as a literal because the
/// constant is crate-private; asserted indirectly by the span check below.
const SEGMENT: usize = 1 << 22;

fn seg_base(p: *mut u8) -> usize {
    (p as usize) & !(SEGMENT - 1)
}

#[test]
fn cross_segment_free_blocks_are_reused() {
    // A mid/high small class: 4 KiB blocks → ~1000 per 4 MiB segment.
    let size = 4096;
    let align = 16;
    let layout = Layout::from_size_align(size, align).unwrap();
    // Enough live blocks to span several segments at once.
    const COUNT: usize = 4000;

    let mut heap = Heap::new().unwrap();

    // --- Round 1: allocate COUNT blocks, span multiple segments. ---
    let mut r1: Vec<*mut u8> = Vec::with_capacity(COUNT);
    let mut bases1: HashSet<usize> = HashSet::new();
    for i in 0..COUNT {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "round1 alloc null at {i}");
        // SAFETY: p is a live allocation of `size` bytes we own. Touching the
        // whole block surfaces any OOB/aliased hand-out from a corrupted head.
        unsafe {
            for b in 0..size {
                p.add(b).write((i & 0xff) as u8);
            }
        }
        bases1.insert(seg_base(p));
        r1.push(p);
    }
    assert!(
        bases1.len() >= 3,
        "test precondition: churn must span >= 3 segments, spanned {}",
        bases1.len()
    );

    // --- Free everything (blocks return to their segments' BinTables). ---
    for &p in &r1 {
        heap.dealloc(p, layout);
    }

    // --- Round 2: must reuse — no allocation may land in a fresh segment. ---
    let mut new_bases = 0usize;
    for i in 0..COUNT {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "round2 alloc null at {i}");
        // SAFETY: live allocation of `size` bytes we own.
        unsafe {
            for b in 0..size {
                p.add(b).write((i & 0xff) as u8);
            }
        }
        if !bases1.contains(&seg_base(p)) {
            new_bases += 1;
        }
    }
    assert_eq!(
        new_bases, 0,
        "cross-segment reuse regression: round 2 placed {new_bases}/{COUNT} blocks \
         in fresh segments instead of reusing round-1's freed blocks \
         (round 1 spanned {} segments)",
        bases1.len()
    );
}
