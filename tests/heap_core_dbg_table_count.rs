//! R32-9 (task #500) — `HeapCore::dbg_table_count` forwarder coverage.
//!
//! `AllocCore::dbg_table_count` (the segment table's high-water registered
//! slot count) already has coverage at the `AllocCore` level
//! (`tests/segment_table_o1.rs`). This file covers the NEW thin delegation
//! added at the `HeapCore` level (`src/registry/heap_core_diag.rs`), which
//! the R32-9 macro-bench harness (`benches/macro_multiseg_steady_state.rs`,
//! `examples/r32_9_macro_multiseg_steady_state_ab_gate.rs`) uses as its
//! path-activation oracle: a claimed `HeapCore` must report a count that
//! actually reflects the segments it has registered, reachable WITHOUT
//! going through `AllocCore` directly (the harness only has a `HeapCore`
//! pointer from `HeapRegistry::claim`/`claim_with_config`).
//!
//! Counterfactual: before this forwarder existed, `HeapCore` exposed no way
//! to read the segment-table count at all — a test asserting the delegated
//! value tracks 1:1 with direct large allocations through the SAME `HeapCore`
//! would fail to compile without it, and a hypothetical broken forwarder
//! (e.g. one that always returned `0`, or read some other heap's table)
//! would fail the growth assertion below.

#![cfg(feature = "alloc-global")]

use core::alloc::Layout;
use sefer_alloc::registry::{bootstrap, HeapRegistry};

#[test]
fn dbg_table_count_tracks_registered_large_segments() {
    let _ = bootstrap::ensure();
    let heap_ptr = HeapRegistry::claim();
    assert!(!heap_ptr.is_null(), "HeapRegistry::claim returned null");
    // SAFETY: `heap_ptr` was just returned by `claim` and is owned by this
    // thread until `recycle` at the end of this test; no other thread
    // touches it.
    let heap = unsafe { &mut *heap_ptr };

    let baseline = heap.dbg_table_count();

    // Each of these allocations is a dedicated Large segment (well past
    // SMALL_MAX), so registering N of them must grow the table's high-water
    // count by exactly N, and the growth must be visible through the SAME
    // `HeapCore` handle this test claimed (not some other heap's table).
    let large_size = sefer_alloc::SegmentLayout::SMALL_MAX + sefer_alloc::SegmentLayout::PAGE;
    let layout = Layout::from_size_align(large_size, sefer_alloc::SegmentLayout::PAGE).unwrap();

    const N: usize = 8;
    let mut ptrs = Vec::with_capacity(N);
    for i in 0..N {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "alloc null at i={i}");
        ptrs.push(p);
    }

    let after_alloc = heap.dbg_table_count();
    assert_eq!(
        after_alloc,
        baseline + N as u32,
        "dbg_table_count did not grow by exactly N after registering N \
         fresh Large segments through this HeapCore"
    );

    for p in ptrs {
        // SAFETY: `p` was allocated by `heap` with `layout` above, live,
        // freed exactly once here.
        unsafe { heap.dealloc(p, layout) };
    }

    // SAFETY: `heap_ptr` was returned by `claim` above, not yet recycled,
    // and no other thread touches it.
    unsafe { HeapRegistry::recycle(heap_ptr) };
}
