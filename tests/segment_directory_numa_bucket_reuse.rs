//! R13-2 (task #272): NUMA directory bucket-slot REUSE.
//!
//! ## The defect (pre-R13-2)
//!
//! `SegmentDirectory::node_ids: [u32; MAX_NODES]` (`MAX_NODES == 8`,
//! `src/alloc_core/segment_directory.rs`) is an append-only registration
//! table: `node_bucket_mut` claims the next free slot the first time a node
//! id is seen, and NEVER releases a slot afterwards — even after every
//! segment ever attributed to that node goes completely idle (every class of
//! every segment on that node empty). This means "8 buckets" bounds the
//! number of DISTINCT nodes a directory can EVER have seen over its entire
//! lifetime, not the number of nodes concurrently holding live segments. A
//! long-lived heap that migrates across NUMA nodes (or simply runs long
//! enough on a host with more than 8 nodes to eventually touch a 9th) loses
//! directory locality for that 9th node FOREVER, even though several of the
//! first 8 buckets may have been sitting completely idle for a long time.
//!
//! ## The fix (R13-2)
//!
//! `SegmentDirectory` now tracks a live "active bit count" per real-node
//! bucket (`active_bits_by_node`), maintained by `set_bit`/`clear_bit` (and
//! their `_all_nodes`/`clear_slot` siblings) on every ACTUAL bit-value
//! transition. When a bucket's count reaches 0 — every class on every
//! segment that bucket's node ever owned bits for is now empty — its
//! `node_ids` slot is freed (`NODE_SLOT_EMPTY`) and immediately eligible for
//! `node_bucket_mut` to hand to the next never-before-seen node.
//!
//! ## This test's construction — three subtleties traced by hand
//!
//! 1. **Bucket registration is dealloc-driven, not alloc-driven.**
//!    `node_bucket_mut` (and the active-bit counter behind it) is only
//!    touched on an empty->non-empty `BinTable` head transition, which is a
//!    property of `dealloc` (a fresh carve never writes a class's free-list
//!    head — see `node_bucket_mut`'s "real-time order" doc comment). Pure
//!    allocation, however many segments it creates, registers NO bucket at
//!    all — a node must free at least one block to register + occupy its
//!    bucket.
//! 2. **A freed block is fair game for ANY node's next allocation.** The
//!    foreign-fallback NUMA scan happily reuses a DIFFERENT node's free block
//!    over carving a fresh local one. If every node used the SAME size
//!    class, node K+1's very first allocation could steal node K's just-freed
//!    block. This test sidesteps that by giving EVERY node its OWN size
//!    class — no two nodes ever compete for the same free list.
//! 3. **`alloc_small`'s cold-carve path amortises via a REFILL BATCH (up to
//!    31 extra blocks, `carve_block_with_refill`'s `REFILL_BATCH` const), AND
//!    `alloc()`'s pop path only scans OTHER segments once the CURRENT one is
//!    exhausted.** Driving a class fully back to empty requires popping
//!    EVERY free block on EVERY segment the class ever touched, but (a) the
//!    normal cold-carve path creates many more free blocks than requested
//!    (each refill block is pushed onto its OWN segment's free list via an
//!    internal `dealloc_small` call, so a naive "allocate N, expect N
//!    segments' worth of free capacity" undercounts), and (b) popping stays
//!    on the single "current" segment until IT drains before the scan ever
//!    visits a different segment — so a naive "stop once the segment I just
//!    popped from shows empty" check silently leaves every OTHER segment's
//!    free blocks completely untouched (both traps were hit and diagnosed by
//!    hand while developing this test: (a) left a real leftover free-list
//!    offset after a fixed-count pop-back, and (b) left 10 of 11 segments'
//!    free lists undrained after the first segment alone emptied). This test
//!    sidesteps both by popping until `alloc()` is FORCED to carve a
//!    genuinely fresh segment (`core.dbg_table_count()` grows) — the only
//!    signal that conclusively proves nothing free remains ANYWHERE for this
//!    class (see `drive_class_to_idle`'s doc comment).
//!
//! With those three points handled, the test:
//!   1. Allocates + registers all 8 `MAX_NODES` bucket slots (one per filling
//!      node, each on its own class), leaving every bucket OCCUPIED.
//!   2. Frees ALL of node 0's blocks (registration + live) and drains its
//!      class back to genuinely empty (`FREE_LIST_NULL` on every segment
//!      node 0 ever touched, INCLUDING any refill-batch segments this test
//!      never held a direct pointer into) — its bucket slot is freed as a
//!      result.
//!   3. A 9th (and, after driving it idle too, a 10th) distinct node then
//!      allocates (on its own fresh class): pre-fix (red) it falls into the
//!      shared unknown bucket (all 8 real slots are permanently claimed);
//!      post-fix (green) it reuses the slot node 0 (then the 9th node) freed.
//!
//! Build/run:
//!   cargo test --features "numa-aware-mock alloc-segment-directory" \
//!       --test segment_directory_numa_bucket_reuse

#![cfg(all(feature = "numa-aware-mock", feature = "alloc-segment-directory"))]

use std::alloc::Layout;
use std::collections::HashSet;

use numa_shim::mock;
use sefer_alloc::AllocCore;

/// `Layout` for small class `class_idx`, using the class's exact block size
/// (guarantees `dbg_layout_class_for` round-trips back to `class_idx` and
/// this allocation touches ONLY that one class's free list).
fn layout_for_class(class_idx: usize) -> Layout {
    let size = AllocCore::dbg_block_size(class_idx);
    Layout::from_size_align(size, 1).unwrap()
}

/// Script the mock to return `node` and invalidate the per-`AllocCore` cached
/// node id so the NEXT alloc/dealloc call re-queries the (now-scripted) mock
/// instead of reusing a stale cached value from a prior node.
fn switch_to_node(core: &mut AllocCore, node: u32) {
    mock::set_current_node(node);
    let _ = mock::drain();
    core.dbg_invalidate_numa_node_cache();
}

/// Allocate `count` blocks of `class_idx` on the CURRENTLY SCRIPTED node via
/// the NORMAL `alloc()` path (segment reservation and refill amortisation
/// happen automatically). Returns the allocated pointers AND the full set of
/// distinct segment ids touched — which may be LARGER than what a naive
/// count would predict, because a cold carve's refill batch can populate a
/// segment's free list with blocks this function never directly returns a
/// pointer to (see the module doc's point 3). Discovering the true segment
/// set requires actually draining those refill blocks back out, which
/// `drive_class_to_idle` below does; this function only needs to return
/// enough LIVE, explicitly-held pointers to free later.
fn alloc_n(core: &mut AllocCore, class_idx: usize, count: usize) -> Vec<*mut u8> {
    let layout = layout_for_class(class_idx);
    let mut ptrs = Vec::with_capacity(count);
    for _ in 0..count {
        let p = core.alloc(layout);
        assert!(!p.is_null(), "allocation failed for class {class_idx}");
        ptrs.push(p);
    }
    ptrs
}

/// Free every block in `live` (draining `class_idx`'s free list on whichever
/// segments they belong to), THEN pop everything straight back out via the
/// normal `alloc()` path, discovering and draining EVERY segment this class
/// ever touches to a genuinely empty `BinTable` head (`FREE_LIST_NULL`) — see
/// the module doc's point 3 on why a guessed/fixed pop count is unsound
/// here, since a cold-carve refill batch can leave free blocks on segments
/// this test never held a direct pointer into.
///
/// Termination: `alloc()`'s pop path prefers the CURRENT segment
/// (`small_cur`) and only falls back to scanning OTHER owned segments once
/// the current one is exhausted (`find_segment_with_free`'s cross-segment
/// scan). This means a per-segment "just went to `FREE_LIST_NULL`" check is
/// NOT sufficient to stop the drain: it only proves the CURRENT segment
/// drained, not that every OTHER segment this node ever touched is ALSO
/// empty (verified by hand — an earlier version of this helper terminated
/// the instant the first-visited segment went empty, silently leaving many
/// other segments' free blocks completely untouched, so the class never
/// actually went idle). The only conclusive signal that NOTHING remains
/// anywhere for this class is `alloc()` being forced to carve a genuinely
/// FRESH segment (`core.dbg_table_count()` grows) — a carve is the cold
/// path's last resort, tried only after both the free-list pop and the full
/// cross-segment scan come up empty. So this loop pops until that happens.
///
/// CRITICAL: every block this function pops back out is **intentionally
/// NEVER freed** — it is deliberately leaked (harmless in a test process)
/// for the rest of the test. Freeing any of them back would re-populate a
/// free list and immediately re-set the very directory bit this function
/// exists to clear (verified by hand: an earlier version of this helper
/// freed everything back "for symmetry" at the end and silently undid its
/// own drain, leaving every bit set again).
///
/// SECOND-ORDER trap (also verified by hand): the fresh segment that proves
/// termination is carved via the NORMAL cold-carve path
/// (`carve_block_with_refill`), which ALSO carves a refill batch (up to 31
/// extra blocks) and pushes each onto ITS OWN (freshly-carved) segment's
/// free list via an internal `dealloc_small` call. That internal dealloc is
/// itself an empty->non-empty transition, so the very act of detecting
/// exhaustion creates ONE MORE non-empty (bit-set) segment for this class —
/// an earlier version of this function stopped right there, leaving that
/// fresh segment's bit permanently set and the bucket permanently occupied,
/// no matter how thoroughly every PRE-EXISTING segment was drained. The fix:
/// after detecting the fresh carve, drain THAT ONE segment too (scoped to
/// just it, via `dbg_freelist_head_for` on `p` — no ambiguity about which
/// segment, unlike the general cross-segment case above) until its own head
/// reads empty. This does not recurse indefinitely: a segment this test
/// created (using the largest small classes, per the module doc) has only
/// enough capacity for a handful of blocks before it too would need a
/// SEPARATE fresh carve, but by then the refill batch just carved already
/// covers every block that segment can ever hold for this class (verified:
/// popping this one segment's free list to empty here never itself demands
/// a further carve in practice for the class sizes this test uses).
fn drive_class_to_idle(core: &mut AllocCore, class_idx: usize, layout: Layout, live: &[*mut u8]) {
    for &p in live {
        unsafe { core.dealloc(p, layout) };
    }
    let table_count_before = core.dbg_table_count();
    loop {
        let p = core.alloc(layout);
        assert!(
            !p.is_null(),
            "draining class {class_idx} to idle: alloc unexpectedly failed"
        );
        if core.dbg_table_count() != table_count_before {
            // Conclusive: every PRE-EXISTING segment's free list for this
            // class is now exhausted. But `p` came from a FRESH segment
            // whose cold-carve refill batch just pushed extra blocks onto
            // ITS OWN free list (see the doc comment above) — drain that one
            // segment too, via its own head, before stopping for real.
            while core.dbg_freelist_head_for(p, class_idx) != u32::MAX {
                let extra = core.alloc(layout);
                assert!(
                    !extra.is_null(),
                    "draining the fresh segment's refill batch: alloc \
                     unexpectedly failed"
                );
                assert_eq!(
                    core.dbg_segment_id_of(extra),
                    core.dbg_segment_id_of(p),
                    "draining the fresh segment's refill batch pulled a \
                     block from a DIFFERENT segment — the refill batch is \
                     larger than expected or a second fresh segment was \
                     carved; this function's bounded assumption (module doc) \
                     does not hold for the active feature/class configuration"
                );
            }
            break;
        }
    }
}

/// The headline R13-2 regression test: fill all `MAX_NODES` (8) bucket slots
/// (each node on its OWN size class, so no two nodes ever compete for the
/// same free list), then drive node 0's class fully back to idle (freeing
/// its bucket slot) while every other node's bucket stays occupied, then
/// bring in a 9th (and 10th) distinct node.
#[test]
fn ninth_node_reuses_freed_bucket_after_earlier_node_goes_idle() {
    let small_class_count = AllocCore::dbg_small_class_count();
    // 11 distinct classes needed: 8 filling nodes + 9th + 10th + one MORE
    // reserved exclusively for the materialisation-threshold density probe,
    // so a probe-freed block is never an untracked extra free entry on a
    // class this test's idle-draining relies on being exact.
    assert!(
        small_class_count >= 11,
        "this test needs >= 11 distinct small classes (got {small_class_count})"
    );
    let filling_classes: [usize; 8] = std::array::from_fn(|i| small_class_count - 1 - i);
    let ninth_class = small_class_count - 9;
    let tenth_class = small_class_count - 10;
    let probe_class = small_class_count - 11;

    // Nodes 0..7 fill all MAX_NODES=8 real-node bucket slots. Node 0 is the
    // one this test drives back to idle and later expects to be reused.
    let filling_nodes: [u32; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
    let ninth_node = 100u32; // Distinct from every filling node.
    let tenth_node = 101u32;

    // No `AllocCore` exists yet, so there is no per-core cache to invalidate
    // — script the mock directly for this very first node (mirrors
    // `script_node` in the sibling NUMA test files).
    mock::set_current_node(filling_nodes[0]);
    let _ = mock::drain();
    let mut core = AllocCore::new().expect("bootstrap");

    // Enough allocations per class to comfortably cross the materialisation
    // threshold once accumulated across all 8 filling nodes. NOTE: a single
    // 4 MiB segment can (and does) hold blocks of MULTIPLE distinct classes
    // simultaneously (segments are not per-class), so 8 distinct classes'
    // worth of small allocations do NOT automatically create 8 distinct
    // segments — `per_class_count` must be large enough that the total
    // allocation volume across all 8 classes actually forces the segment
    // table past `DIRECTORY_MATERIALIZE_THRESHOLD` (32 by default). Measure
    // the ACTUAL blocks-per-segment density (via the reserved `probe_class`,
    // never touched by the real filling/9th/10th nodes) rather than guessing
    // — mirrors `segment_directory_numa_high_node_ids.rs`'s R12-14
    // density-probe pattern, so this test is not hardcoded to one feature
    // combination's block size.
    let threshold = AllocCore::dbg_directory_materialize_threshold() as usize;
    let probe_layout = layout_for_class(probe_class);
    const PROBE_COUNT: usize = 16;
    let mut probe_ptrs = Vec::with_capacity(PROBE_COUNT);
    let mut probe_segments: HashSet<u32> = HashSet::new();
    for _ in 0..PROBE_COUNT {
        let p = core.alloc(probe_layout);
        assert!(!p.is_null(), "probe allocation must succeed");
        probe_segments.insert(core.dbg_segment_id_of(p));
        probe_ptrs.push(p);
    }
    let density = PROBE_COUNT / probe_segments.len().max(1);
    // R13-2 CARE: the probe runs on node 0 (the current node at this point in
    // the test — `filling_nodes[0]`), so a plain "free everything" here would
    // leave a residual directory bit registered against node 0's bucket
    // (diagnosed by hand: `dbg_directory_active_bits_for_bucket` showed 1
    // leftover active bit attributed to `probe_class`, even after phase 2
    // below fully drained `filling_classes[0]` — the counter tracks
    // EVERY class in the bucket, not just the one phase 2 cares about).
    // Reuse the exact same robust idle-draining routine phase 2 uses so the
    // probe leaves node 0's bucket completely clean before phase 1 begins.
    drive_class_to_idle(&mut core, probe_class, probe_layout, &probe_ptrs);
    // Enough allocations per class, across all 8 filling classes, to
    // comfortably cross the threshold: `threshold + margin` SEGMENTS'
    // worth, spread over 8 classes, at the measured density.
    let segments_needed = threshold + 48;
    let per_class_count = (segments_needed * density / filling_classes.len().max(1)).max(6);

    // ── Phase 1: allocate + register + occupy all 8 filling-node buckets ──
    // Allocate `per_class_count` blocks per node (own class) via the NORMAL
    // alloc path, free exactly ONE (registers + occupies the bucket), keep
    // the rest live.
    let mut node0_live: Vec<*mut u8> = Vec::new();
    let mut other_live: Vec<(u32, usize, Vec<*mut u8>)> = Vec::new();
    for (i, &node) in filling_nodes.iter().enumerate() {
        switch_to_node(&mut core, node);
        let class_idx = filling_classes[i];
        let mut allocated = alloc_n(&mut core, class_idx, per_class_count);
        let layout = layout_for_class(class_idx);
        let registration_ptr = allocated.pop().expect("at least one allocated block");
        unsafe { core.dealloc(registration_ptr, layout) };
        if node == filling_nodes[0] {
            node0_live = allocated;
        } else {
            other_live.push((node, class_idx, allocated));
        }
    }

    assert!(
        core.dbg_directory_is_materialised(),
        "directory must be materialised after filling all 8 node buckets \
         (table count = {})",
        core.dbg_table_count()
    );

    let unknown_bucket = AllocCore::dbg_directory_node_bitmaps() - 1;

    // Sanity: every filling node currently holds a REAL (non-unknown) bucket
    // — otherwise this test's later "node 0 specifically went idle and freed
    // ITS bucket" assertions would not be meaningful.
    for &node in &filling_nodes {
        let bucket = core
            .dbg_directory_node_bucket_for(node)
            .expect("directory materialised");
        assert_ne!(
            bucket, unknown_bucket,
            "node {node} must hold a REAL per-node bucket right after \
             registration — a construction failure here (node aliased into \
             the unknown bucket) would make the rest of this test \
             meaningless"
        );
    }
    let node0_bucket_while_occupied = core
        .dbg_directory_node_bucket_for(filling_nodes[0])
        .expect("directory materialised");

    // ── Phase 2: drive node 0's class fully back to idle ──
    switch_to_node(&mut core, filling_nodes[0]);
    let node0_layout = layout_for_class(filling_classes[0]);
    drive_class_to_idle(&mut core, filling_classes[0], node0_layout, &node0_live);

    let node0_bucket_after_idle = core
        .dbg_directory_node_bucket_for(filling_nodes[0])
        .expect("directory materialised");
    assert_eq!(
        node0_bucket_after_idle, unknown_bucket,
        "node 0's bucket slot must have been FREED once every bit it ever \
         set went back to 0 (every segment node 0 ever touched was drained \
         to a genuinely empty free list) — a bucket index other than the \
         unknown bucket here means the R13-2 reuse mechanism did not free \
         the slot as expected, and the rest of this test cannot be \
         meaningful"
    );

    // Every OTHER filling node's bucket must remain occupied (their blocks
    // are still live) — the fix must free ONLY the genuinely idle bucket.
    for &(node, _, _) in &other_live {
        let bucket = core
            .dbg_directory_node_bucket_for(node)
            .expect("directory materialised");
        assert_ne!(
            bucket, unknown_bucket,
            "node {node}'s bucket must remain occupied — its blocks are \
             still live, so freeing it would be a false-negative bucket \
             reuse (worse than the append-only status quo)"
        );
    }

    // ── The headline assertion: bring in a 9th distinct node ──
    switch_to_node(&mut core, ninth_node);
    let mut ninth_allocated = alloc_n(&mut core, ninth_class, per_class_count);
    let ninth_layout = layout_for_class(ninth_class);
    let ninth_registration_ptr = ninth_allocated.pop().expect("at least one allocated block");
    unsafe { core.dealloc(ninth_registration_ptr, ninth_layout) };

    let ninth_bucket = core
        .dbg_directory_node_bucket_for(ninth_node)
        .expect("directory materialised");
    assert_ne!(
        ninth_bucket, unknown_bucket,
        "RED (pre-R13-2)/GREEN (post-R13-2) assertion: the 9th distinct node \
         must claim a REAL per-node bucket, not the shared unknown bucket. \
         Under the pre-fix append-only `node_ids` table, all 8 real-node \
         slots stay permanently claimed by nodes 0..7 forever (even though \
         node 0 went completely idle), so the 9th node has no free slot and \
         falls into the unknown bucket (`ninth_bucket == unknown_bucket`). \
         Post-fix, node 0's slot was freed when its active-bit counter \
         reached 0, and the 9th node reuses it."
    );
    assert_eq!(
        ninth_bucket, node0_bucket_while_occupied,
        "the 9th node should specifically reuse node 0's former bucket \
         index (the only one freed so far) — a different real bucket here \
         would indicate an unexpected extra free slot, not the mechanism \
         this test targets"
    );

    // Drive the 9th node's class back to idle too (same technique as node
    // 0), then bring in a 10th distinct node — it must ALSO be able to reuse
    // a freed bucket, reinforcing that this is a general, repeatable
    // mechanism rather than a one-shot fluke.
    switch_to_node(&mut core, ninth_node);
    drive_class_to_idle(&mut core, ninth_class, ninth_layout, &ninth_allocated);

    switch_to_node(&mut core, tenth_node);
    let mut tenth_allocated = alloc_n(&mut core, tenth_class, per_class_count);
    let tenth_layout = layout_for_class(tenth_class);
    let tenth_registration_ptr = tenth_allocated.pop().expect("at least one allocated block");
    unsafe { core.dealloc(tenth_registration_ptr, tenth_layout) };

    let tenth_bucket = core
        .dbg_directory_node_bucket_for(tenth_node)
        .expect("directory materialised");
    assert_ne!(
        tenth_bucket, unknown_bucket,
        "a 10th distinct node must ALSO be able to reuse a freed bucket \
         (the 9th node's, freed just above) rather than overflowing to the \
         unknown bucket — confirms bucket reuse is a repeatable mechanism, \
         not a one-off"
    );

    // Cleanup: free everything still live.
    for (node, class_idx, live) in other_live {
        switch_to_node(&mut core, node);
        let layout = layout_for_class(class_idx);
        for &p in &live {
            unsafe { core.dealloc(p, layout) };
        }
    }
    switch_to_node(&mut core, tenth_node);
    for &p in &tenth_allocated {
        unsafe { core.dealloc(p, tenth_layout) };
    }
}
