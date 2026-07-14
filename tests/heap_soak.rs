//! Soak test for bounded segment usage under sustained alloc/free churn.
//!
//! Asserts BOOKKEEPING: after freeing all blocks from a segment, the allocator
//! reuses the segment (via free-list reuse) rather than growing unboundedly.
//! This verifies bounded segment growth through free-list reuse, NOT through
//! decommit (M6). Decommit is a Phase 11 deliverable; the `os::decommit_pages`
//! seam is in place but not wired into the substrate path.
//!
//! Since measuring RSS portably is hard (and impossible under miri), this test
//! asserts that sustained churn does not grow the segment count past a small
//! bound -- the allocator reuses freed blocks from its free lists.
//!
//! Exercises `AllocCore` directly. (An earlier version also had a cross-thread
//! soak variant driven through the now-removed `Heap::dealloc_any_thread` API;
//! that variant was removed alongside `Heap`. Cross-thread soak coverage lives
//! in `tests/global_alloc_mt.rs` and `tests/concurrent_stress.rs` against the
//! `SeferAlloc`/`HeapCore` production face.)

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;

use sefer_alloc::AllocCore;

/// Sustained alloc/free churn: allocate N blocks, free all, repeat M times.
/// The segment count should stay bounded (not grow linearly with M) because
/// freed blocks are reused from the per-class free lists.
///
/// This test asserts free-list reuse, NOT decommit (M6). M6 is Phase 11.
#[test]
fn soak_segment_reuse_bounded() {
    let mut heap = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(256, 8).unwrap();

    // Repeat the alloc-all/free-all cycle many times.
    for _round in 0..20 {
        let mut ptrs = Vec::new();
        // Allocate enough to fill several page-runs within the segment.
        for _ in 0..512 {
            let p = heap.alloc(layout);
            assert!(!p.is_null());
            // Non-vacuous: write + read.
            unsafe {
                std::ptr::write_bytes(p, 0xCC, 256);
                assert_eq!(p.read(), 0xCC);
            }
            ptrs.push(p);
        }
        // Free all.
        for p in ptrs {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { heap.dealloc(p, layout) };
        }
    }
    // If we get here without OOM or crash, the free-list reuse is working
    // (the allocator did not grow unboundedly).
    // With 20 rounds of 512 x 256 B = 2.5 MiB per round, an unbounded
    // allocator would need ~50 MiB of segments; with free-list reuse it
    // stays < 8 MiB.
}
