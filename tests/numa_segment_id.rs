//! Phase B+C smoke test: verify that `AllocCore` stamps a `node_id` in the
//! segment header when `numa-aware` is active.
//!
//! Run with:
//!   cargo test --features "alloc-core numa-aware" --test numa_segment_id
//!
//! What we check:
//! - After at least one small allocation, the segment carrying that allocation
//!   has a `node_id` field that is NOT `u32::MAX` on platforms where
//!   `current_node()` returns a real value (i.e. not `NO_NODE`), OR equals
//!   `NO_NODE_RAW` on platforms without NUMA (macOS, miri, single-node Linux)
//!   where `current_node()` returns `NO_NODE`.
//!
//! In other words: `node_id == NO_NODE` iff `current_node() == NO_NODE`.
//! This is the only claim we can make portably — we do not assert a specific
//! node number (that would require a multi-NUMA-node machine or QEMU).

#![cfg(all(feature = "alloc-core", feature = "numa-aware"))]

use core::alloc::Layout;

use sefer_alloc::alloc_core::{numa, AllocCore};

/// After allocating a small block, the block's segment must carry the same
/// `node_id` that `numa::current_node()` returned at the time of the
/// allocation.  On platforms without NUMA (`NO_NODE`) the segment header
/// retains the `NO_NODE_RAW` sentinel — both are `u32::MAX`, so the
/// comparison is trivially correct.
///
/// On a real NUMA machine (or under QEMU fake-NUMA) `current_node()` returns
/// a value < 64 and the segment's `node_id` must equal it.  On a non-NUMA
/// platform (macOS, miri, single-node Linux that reports `NO_NODE`) the test
/// accepts `NO_NODE_RAW` from the segment.
#[test]
fn small_segment_carries_node_id() {
    let mut core = AllocCore::new().expect("AllocCore::new failed");

    // Snapshot the NUMA node of the calling thread BEFORE the allocation
    // so we can compare it with what the allocator stamped.
    let expected_node = numa::current_node();

    // Allocate a small block (8 bytes, align 8 — a typical small class).
    let layout = Layout::from_size_align(8, 8).unwrap();
    let ptr = core.alloc(layout);
    assert!(!ptr.is_null(), "AllocCore::alloc returned null");

    // Retrieve the node_id stamped in the header of the segment that carries
    // this allocation.
    let stamped_node = core
        .dbg_node_id_for(ptr)
        .expect("dbg_node_id_for returned None — ptr is not from a known segment");

    // The stamped value must equal what current_node() reported.
    // On a non-NUMA platform both are u32::MAX (NO_NODE / NO_NODE_RAW).
    assert_eq!(
        stamped_node, expected_node,
        "segment node_id ({stamped_node}) != current_node() ({expected_node})"
    );

    // Cleanup.
    core.dealloc(ptr, layout);
}

/// After allocating a LARGE block (> small class limit), the dedicated large
/// segment must also carry the correct `node_id`.
#[test]
fn large_segment_carries_node_id() {
    let mut core = AllocCore::new().expect("AllocCore::new failed");

    let expected_node = numa::current_node();

    // Allocate a large block (2 MiB, align 4096 — well above all small classes).
    let layout = Layout::from_size_align(2 * 1024 * 1024, 4096).unwrap();
    let ptr = core.alloc(layout);
    assert!(!ptr.is_null(), "AllocCore::alloc returned null for large block");

    let stamped_node = core
        .dbg_node_id_for(ptr)
        .expect("dbg_node_id_for returned None for large block");

    assert_eq!(
        stamped_node, expected_node,
        "large segment node_id ({stamped_node}) != current_node() ({expected_node})"
    );

    core.dealloc(ptr, layout);
}
