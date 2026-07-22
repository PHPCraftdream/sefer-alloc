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
///
/// ## R12-14 (task #265): per-node allocation count derived from capacity
///
/// This test originally used a hardcoded `300` allocations per node (9 × 300
/// = 2700 total), matching `segment_directory_numa.rs`'s established count.
/// That is safe under `production`/`production,numa-aware-mock`, where
/// `SMALL_MAX` (~253 KiB) still packs several blocks per 4 MiB segment, so
/// 2700 allocations consume well under `MAX_SEGMENTS` (1024) live segments.
///
/// Under `--all-features` (`medium-classes-wide` raises `SMALL_MAX` to
/// 1.75 MiB), the class this test deliberately picks FOR its low block
/// density (`small_max_layout`'s doc comment: "largest small block, fewest
/// blocks per segment") degrades all the way to exactly ONE block per
/// segment (4 MiB segment payload minus metadata fits only one ~1.75 MiB
/// block). At that density 2700 allocations need ~2700 live segments —
/// 2.6× `MAX_SEGMENTS` — so the table fills and `core.alloc` legitimately
/// returns null well before the 9th node finishes (the observed
/// `assertion failed: !p.is_null()` panic). Not a directory bug: the test's
/// own fixed-300 budget was tuned for one feature combination's block
/// density and silently overflowed under a sparser one.
///
/// Fix: MEASURE the actual `SMALL_MAX` blocks-per-segment density up front
/// (a handful of probe allocations on the bootstrap node, cleaned up before
/// the real per-node loop below) and derive the per-node count from it, so
/// `nodes.len() * per_node_count` segments (worst realistic case: exactly
/// `per_node_count / measured_density` per node) never exceeds
/// [`AllocCore::dbg_max_segments`]. Capped at the original 300 so denser
/// feature combinations (`production`, where the measured density is high)
/// keep the EXACT allocation count this test was originally tuned with —
/// only the sparse-density case (`medium-classes-wide`) actually shrinks the
/// count, preserving "several segments per node" + "comfortably crosses the
/// 32-segment materialisation threshold" in every configuration.
#[test]
fn ninth_distinct_high_node_overflows_to_unknown_bucket_without_corruption() {
    let layout = small_max_layout();

    // Nodes 8 total, occupying buckets 0..8 (all of MAX_NODES), all
    // BELOW-or-AT typical MAX_NODES=8 boundary intentionally mixed with
    // high ids to also exercise the >= 8 raw-id path per node.
    let nodes: [u32; 9] = [56, 57, 58, 59, 60, 61, 62, 63, 40];

    script_node(nodes[0]);
    let mut core = AllocCore::new().expect("bootstrap");

    // R12-14: measure SMALL_MAX blocks-per-segment density with a small
    // probe batch (distinct segment count / allocation count), then derive
    // a per-node allocation count that keeps the WORST-CASE total segment
    // demand (`nodes.len() * per_node_count / density`) safely under
    // `MAX_SEGMENTS`, while never exceeding the original 300 per node.
    const PROBE_COUNT: usize = 8;
    let mut probe_ptrs = Vec::with_capacity(PROBE_COUNT);
    let mut probe_segments = std::collections::HashSet::new();
    for _ in 0..PROBE_COUNT {
        let p = core.alloc(layout);
        assert!(!p.is_null(), "probe allocation must succeed");
        probe_segments.insert(core.dbg_segment_id_of(p));
        probe_ptrs.push(p);
    }
    // Density in blocks/segment, rounded down (>= 1 by construction: at
    // least one segment was used for PROBE_COUNT blocks).
    let density = PROBE_COUNT / probe_segments.len();
    for p in probe_ptrs {
        unsafe { core.dealloc(p, layout) };
    }

    let max_segments = AllocCore::dbg_max_segments();
    let safety_margin = 16; // headroom for the bootstrap/primordial segment.
    let safe_total_segments = max_segments.saturating_sub(safety_margin);
    let per_node_count = (safe_total_segments * density / nodes.len()).min(300);
    assert!(
        per_node_count >= 8,
        "per-node allocation count collapsed to {per_node_count} \
         (max_segments={max_segments}, measured density={density} blocks/segment) \
         — too few to reliably create several segments per node or cross the \
         materialisation threshold; MAX_SEGMENTS shrank below what this test can work with"
    );

    let mut ptrs_by_node: Vec<(u32, Vec<*mut u8>)> = Vec::new();
    for &node in &nodes {
        script_node(node);
        core.dbg_invalidate_numa_node_cache();
        let mut ptrs = Vec::new();
        // Enough allocations per node (SMALL_MAX => very few blocks per
        // segment, so this reliably creates several segments per node) to
        // push the table comfortably past the materialisation threshold
        // across all 9 nodes combined, WITHOUT exceeding `MAX_SEGMENTS`
        // collectively even at the sparsest (one-block-per-segment) density
        // (see the `per_node_count` derivation above, R12-14).
        for _ in 0..per_node_count {
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
