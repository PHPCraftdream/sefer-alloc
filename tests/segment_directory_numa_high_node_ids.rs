//! R12-2 (task #253): NUMA node ids >= the old direct-index `MAX_NODES`
//! clamp (8) must still preserve local-first directory locality.
//!
//! ## The design defect (pre-R12-2)
//!
//! `numa-shim` scans up to 64 real OS node ids
//! (`crates/numa/src/lib.rs::cpu_to_numa_node`, `for node in 0u32..64`), but
//! the segment directory's `node_bucket` used to map a segment's `node_id`
//! to its bucket by using the raw OS node id as a DIRECT array index clamped
//! at `MAX_NODES = 8` (`src/alloc_core/segment_directory.rs`). Every node id
//! `>= 8` fell into the SAME shared "unknown" bucket regardless of how many
//! distinct high node ids were actually observed. Because the scan-order
//! bucket list visits the unknown bucket right after the caller's own
//! (local) bucket and BEFORE any foreign real-node bucket, a thread pinned
//! to node 9 (which itself maps to the unknown bucket under the old scheme)
//! would find node 9's OWN segments in the SAME bucket as any node 10 (also
//! unknown-bucket) segments — the scan cannot distinguish "my own high-id
//! node" from "some other high-id node" at all, so on a host with segments
//! on both nodes 9 and 10, the directory can return a node-10 segment ahead
//! of an available node-9 (local) one. This defeats the entire R11-6
//! locality optimisation for any host with more than 8 NUMA nodes — the
//! exact case that optimisation matters most for.
//!
//! ## The fix (R12-2)
//!
//! `SegmentDirectory` now carries a dense `node_ids: [u32; MAX_NODES]`
//! registration table: a node id claims the next free bucket slot the first
//! time a segment on that node is registered, so `MAX_NODES` bounds the
//! number of DISTINCT nodes tracked simultaneously rather than the raw OS
//! node id value. Node ids 9 and 10 each get their OWN bucket (as long as
//! fewer than `MAX_NODES` distinct nodes are already registered), restoring
//! local-first locality exactly as R11-6 intended.
//!
//! Build/run:
//!   cargo test --features "numa-aware-mock alloc-segment-directory" \
//!       --test segment_directory_numa_high_node_ids

#![cfg(all(feature = "numa-aware-mock", feature = "alloc-segment-directory"))]

use std::alloc::Layout;

use numa_shim::mock;
use sefer_alloc::{AllocCore, SegmentLayout};

/// SMALL_MAX class — the largest small block, fewest blocks per segment, so
/// the fewest allocations are needed to create many segments (and cross the
/// 32-segment materialisation threshold).
const CLASS_SIZE: usize = SegmentLayout::SMALL_MAX;

fn small_max_layout() -> Layout {
    Layout::from_size_align(CLASS_SIZE, 1).unwrap()
}

/// Script the mock to return `node`, then clear the call log.
fn script_node(node: u32) {
    mock::set_current_node(node);
    let _ = mock::drain();
}

/// The headline R12-2 regression test: construct segments on TWO NUMA nodes
/// whose ids are BOTH `>= 8` (the pre-R12-2 direct-index `MAX_NODES` clamp),
/// with the FOREIGN node's segments appearing EARLIER in the segment table
/// than the LOCAL node's segments — mirroring
/// `segment_directory_numa::directory_returns_local_not_foreign`'s
/// construction exactly, just with high node ids.
///
/// Pre-R12-2: node 9 and node 10 both clamp to the SAME "unknown" bucket
/// (`node_id as usize >= MAX_NODES` => bucket `MAX_NODES`), so the directory
/// cannot prefer the thread's own node-9 segment over the node-10 one — this
/// test is expected to FAIL on that code (see the git-stash-verified red
/// run in the task report). Post-R12-2: node 9 and node 10 register into
/// DISTINCT dense buckets (first-seen order), so the local-first scan order
/// correctly prefers the node-9 segment.
#[test]
fn directory_returns_local_not_foreign_for_high_node_ids() {
    let foreign_node = 10u32;
    let local_node = 9u32;
    let layout = small_max_layout();

    // ── Phase 1: create foreign-node (10) segments — LOWER slot indices ──
    script_node(foreign_node);
    let mut core = AllocCore::new().expect("bootstrap");
    let mut foreign_ptrs: Vec<*mut u8> = Vec::new();
    for _ in 0..300 {
        foreign_ptrs.push(core.alloc(layout));
    }

    // ── Phase 2: create local-node (9) segments — HIGHER slot indices ──
    script_node(local_node);
    core.dbg_invalidate_numa_node_cache();
    let mut local_ptrs: Vec<*mut u8> = Vec::new();
    for _ in 0..300 {
        local_ptrs.push(core.alloc(layout));
    }

    assert!(
        core.dbg_directory_is_materialised(),
        "directory must be materialised"
    );

    // Verify the segments are actually stamped with the expected (high)
    // node ids.
    assert_eq!(
        core.dbg_node_id_for(foreign_ptrs[0]),
        Some(foreign_node),
        "foreign segments must be stamped with node {foreign_node}"
    );
    assert_eq!(
        core.dbg_node_id_for(local_ptrs[0]),
        Some(local_node),
        "local segments must be stamped with node {local_node}"
    );

    // Foreign segments must have LOWER slot indices than local segments
    // (created first => registered first => lower table slots) — otherwise
    // the test would be vacuous (a naive "first slot wins" scan would pass
    // by accident).
    let foreign_slot = core.dbg_segment_id_of(foreign_ptrs[0]);
    let local_slot = core.dbg_segment_id_of(local_ptrs[0]);
    assert!(
        foreign_slot < local_slot,
        "foreign segment (slot {foreign_slot}) must appear EARLIER in the table \
         than local (slot {local_slot}) for this test to be meaningful"
    );

    let class_idx = core
        .dbg_layout_class_for(layout)
        .expect("SMALL_MAX resolves to a class");

    // ── Phase 3: free ALL blocks — every segment now has free blocks ──
    // Both the node-10 bucket AND the node-9 bucket now have directory bits
    // set. Pre-R12-2, nodes 9 and 10 alias into the SAME unknown bucket, so
    // this phase creates a single shared bucket with both foreign and local
    // candidates mixed together — the directory cannot tell them apart and
    // is not guaranteed to prefer the local one. Post-R12-2, they occupy
    // distinct buckets and local-first scan order must win.
    let all_ptrs: Vec<*mut u8> = foreign_ptrs
        .iter()
        .chain(local_ptrs.iter())
        .copied()
        .collect();
    for &p in &all_ptrs {
        unsafe { core.dealloc(p, layout) };
    }

    // ── Phase 4: call find_segment_with_free DIRECTLY ──
    // Bypasses alloc_small's step-1 pop_free(small_cur) fast path entirely,
    // so the directory-driven lookup is ALWAYS the deciding factor.
    let seg = core
        .dbg_find_segment_with_free(class_idx)
        .expect("must find a segment with free blocks (all were just freed)");

    let seg_node = core.dbg_node_id_for(seg);
    assert_eq!(
        seg_node,
        Some(local_node),
        "directory must return a LOCAL segment (node {local_node}), not the \
         foreign one (node {foreign_node}) — both node ids are >= the legacy \
         direct-index MAX_NODES=8 clamp. A foreign return here means high \
         node ids are still aliasing into a single shared bucket (R12-2 \
         regression): the dense node_ids registration table \
         (SegmentDirectory::node_bucket_mut) is not assigning nodes 9 and 10 \
         to distinct buckets."
    );

    // Cleanup.
    for &p in &all_ptrs {
        let _ = p; // already deallocated above
    }
}

/// Sanity check: with MORE than `MAX_NODES` (8) distinct high node ids
/// actually observed, the (MAX_NODES + 1)-th distinct node correctly
/// overflows into the shared unknown bucket rather than panicking or
/// corrupting another node's bucket. This confirms the R10-6 degradation
/// path R12-2 preserves is still reachable — just now gated on genuine
/// fan-out (distinct-node count) instead of the raw node id value.
#[test]
fn ninth_distinct_high_node_overflows_to_unknown_bucket_without_corruption() {
    let layout = small_max_layout();

    // Nodes 8 total, occupying buckets 0..8 (all of MAX_NODES), all
    // BELOW-or-AT typical MAX_NODES=8 boundary intentionally mixed with
    // high ids to also exercise the >= 8 raw-id path per node.
    let nodes: [u32; 9] = [56, 57, 58, 59, 60, 61, 62, 63, 40];

    script_node(nodes[0]);
    let mut core = AllocCore::new().expect("bootstrap");

    let mut ptrs_by_node: Vec<(u32, Vec<*mut u8>)> = Vec::new();
    for &node in &nodes {
        script_node(node);
        core.dbg_invalidate_numa_node_cache();
        let mut ptrs = Vec::new();
        // Enough allocations per node (SMALL_MAX => very few blocks per
        // segment, so this reliably creates several segments per node) to
        // push the table comfortably past the materialisation threshold
        // across all 9 nodes combined. Matches the per-node allocation
        // count `segment_directory_numa.rs`'s existing tests use (300) to
        // reliably cross the 32-segment threshold.
        for _ in 0..300 {
            let p = core.alloc(layout);
            assert!(!p.is_null());
            ptrs.push(p);
        }
        ptrs_by_node.push((node, ptrs));
    }

    assert!(
        core.dbg_directory_is_materialised(),
        "directory must be materialised across the combined 9-node workload"
    );

    // Every pointer must still resolve to the CORRECT node id (no bucket
    // aliasing corrupted the segment's own stamped node_id — the header
    // stamp is independent of the directory bucket assignment).
    for (node, ptrs) in &ptrs_by_node {
        for &p in ptrs {
            assert_eq!(
                core.dbg_node_id_for(p),
                Some(*node),
                "segment's stamped node_id must be unaffected by directory \
                 bucket overflow handling"
            );
        }
    }

    // Cleanup.
    for (_, ptrs) in &ptrs_by_node {
        for &p in ptrs {
            unsafe { core.dealloc(p, layout) };
        }
    }
}
