#![cfg(all(
    feature = "numa-aware-mock",
    feature = "alloc-segment-directory",
    feature = "internals",
    numa_shim_mock
))]

use std::alloc::Layout;
use std::collections::HashSet;

use numa_shim::mock;
use sefer_alloc::AllocCore;

fn layout_for_class(class: usize) -> Layout {
    Layout::from_size_align(AllocCore::dbg_block_size(class), 1).unwrap()
}

fn unknown_bucket() -> usize {
    AllocCore::dbg_directory_node_bitmaps() - 1
}

fn switch_node(core: &mut AllocCore, node: u32) {
    mock::set_current_node(node);
    let _ = mock::drain();
    core.dbg_invalidate_numa_node_cache();
}

fn alloc_n(core: &mut AllocCore, class: usize, count: usize) -> Vec<*mut u8> {
    (0..count)
        .map(|_| {
            let p = core.alloc(layout_for_class(class));
            assert!(!p.is_null());
            p
        })
        .collect()
}

fn drive_class_to_idle(core: &mut AllocCore, class: usize, live: &[*mut u8]) {
    let layout = layout_for_class(class);
    let blocks_per_segment = sefer_alloc::SegmentLayout::SEGMENT / AllocCore::dbg_block_size(class);
    for &p in live {
        // SAFETY: each pointer is a live allocation returned for `layout`.
        unsafe { core.dealloc(p, layout) };
    }
    let table_count = core.dbg_table_count();
    // Each existing segment can contain at most SEGMENT / block_size blocks
    // of this class. Exhausting that many per table entry must force a fresh
    // segment; one additional allocation observes that transition.
    let table_entries =
        usize::try_from(table_count).expect("segment table count must fit the platform usize");
    let max_attempts = table_entries
        .saturating_mul(blocks_per_segment)
        .saturating_add(1);
    let mut fresh_segment = None;
    for _ in 0..max_attempts {
        let p = core.alloc(layout);
        assert!(
            !p.is_null(),
            "allocation failed while draining class {class}"
        );
        if core.dbg_table_count() != table_count {
            fresh_segment = Some(p);
            break;
        }
    }
    let p = fresh_segment.unwrap_or_else(|| {
        panic!(
            "class {class} did not reach a fresh segment within {max_attempts} \
             attempts (table entries={table_count}, blocks/segment={blocks_per_segment})"
        )
    });

    // A fresh segment can contribute no more than its physical class capacity.
    // Include the terminal observation in the bound so exactly-full segments
    // are accepted without an unbounded condition loop.
    let mut drained = false;
    for attempt in 0..=blocks_per_segment {
        if core.dbg_freelist_head_for(p, class) == u32::MAX {
            drained = true;
            break;
        }
        assert!(
            attempt < blocks_per_segment,
            "fresh segment for class {class} retained a free-list head after \
             its capacity of {blocks_per_segment} blocks was drained"
        );
        let extra = core.alloc(layout);
        assert!(
            !extra.is_null(),
            "allocation failed draining fresh class {class} segment"
        );
        assert_eq!(core.dbg_segment_id_of(extra), core.dbg_segment_id_of(p));
    }
    assert!(
        drained,
        "fresh segment for class {class} was not observed empty within its physical capacity"
    );
}

/// Allocate on `node` until a fresh segment is registered and return the
/// live blocks. Refill publishes make the fresh segment's node register its
/// directory bucket, and `small_cur` ends up pointing at that fresh
/// node-stamped segment.
fn force_fresh_segment(core: &mut AllocCore, node: u32, class: usize) -> Vec<*mut u8> {
    switch_node(core, node);
    let before = core.dbg_table_count();
    let blocks_per_segment = sefer_alloc::SegmentLayout::SEGMENT / AllocCore::dbg_block_size(class);
    let max = usize::try_from(before)
        .expect("segment table count must fit the platform usize")
        .saturating_mul(blocks_per_segment)
        .saturating_add(blocks_per_segment + 1);
    let mut live = Vec::new();
    for _ in 0..max {
        let p = core.alloc(layout_for_class(class));
        assert!(
            !p.is_null(),
            "force_fresh_segment: alloc failed for node {node}"
        );
        live.push(p);
        if core.dbg_table_count() != before {
            return live;
        }
    }
    panic!("fresh segment for node {node} class {class} not reached in {max} allocs");
}

/// Deterministically (re-)claim a dedicated directory bucket for `node`:
/// R13-2 frees the slot once the node's active bits hit 0, which a fill
/// node whose segments were fully consumed triggers earlier than this test
/// expects. A fresh node-stamped segment re-registers it: the fresh
/// segment serves a single block, and releasing that block is what flips
/// the segment's class free list empty -> non-empty, firing the publish
/// that registers the node. Retry a few times in case a release (hysteresis
/// pool overflow) wiped an earlier pass's bits.
fn reregister_node_bucket(core: &mut AllocCore, node: u32, class: usize) {
    let layout = layout_for_class(class);
    for _ in 0..4 {
        let live = force_fresh_segment(core, node, class);
        for p in live {
            // SAFETY: each pointer is a live allocation returned for `layout`.
            unsafe { core.dealloc(p, layout) };
        }
        if core.dbg_directory_node_bucket_for(node) != Some(unknown_bucket()) {
            return;
        }
    }
}

// Determinism notes (flake fix, task #2106): the two facts this test used to
// assume are NOT allocator guarantees — (1) a fill node keeps its dedicated
// bucket (R13-2 frees the slot as soon as its active bits hit 0, which a
// fully-consumed segment triggers early); (2) the overflow node's first
// allocation of a fresh class reserves a fresh node-stamped segment (step 3
// of the small path — carve from `small_cur` — is deliberately not a NUMA
// policy point, see docs/PHASE_NUMA_DESIGN.md "AllocCore"; a foreign-node
// `small_cur` with leftover bump space serves the block under the foreign
// node's bucket). Both are now forced deterministically before the asserts
// that rely on them; the asserted invariant itself is unchanged.
#[test]
fn unknown_bucket_bit_is_cleared_after_node_gets_dedicated_bucket() {
    let classes = AllocCore::dbg_small_class_count();
    assert!(classes >= 11);
    let filling_classes: [usize; 8] = std::array::from_fn(|i| classes - 1 - i);
    let overflow_class = classes - 9;
    let dedicated_class = classes - 10;
    let probe_class = classes - 11;
    let nodes = [0, 1, 2, 3, 4, 5, 6, 7];
    let overflow_node = 100;

    mock::set_current_node(nodes[0]);
    let _ = mock::drain();
    let mut core = AllocCore::new().expect("bootstrap");

    let threshold = AllocCore::dbg_directory_materialize_threshold() as usize;
    let probe = alloc_n(&mut core, probe_class, 16);
    let probe_segments: HashSet<_> = probe.iter().map(|&p| core.dbg_segment_id_of(p)).collect();
    let density = 16 / probe_segments.len().max(1);
    drive_class_to_idle(&mut core, probe_class, &probe);
    let count = ((threshold + 48) * density / filling_classes.len()).max(6);

    let mut node0_live = Vec::new();
    let mut other_live = Vec::new();
    for (i, &node) in nodes.iter().enumerate() {
        switch_node(&mut core, node);
        let class = filling_classes[i];
        let mut allocated = alloc_n(&mut core, class, count);
        let registration = allocated.pop().unwrap();
        // SAFETY: this is a live allocation made with this class's layout.
        unsafe { core.dealloc(registration, layout_for_class(class)) };
        if i == 0 {
            node0_live = allocated;
        } else {
            other_live.push((node, class, allocated));
        }
    }
    assert!(core.dbg_directory_is_materialised());

    // R13-2 frees a bucket slot the moment a node's active bits hit 0, and a
    // fill node whose `count` blocks fully consumed its segment can already
    // be idle here — nondeterministically, because it depends on how the
    // blocks spread over segments. Re-claim every slot that was freed early
    // so "all 8 dedicated slots are claimed" below is a fact, not a hope.
    let mut warmup_keepalive: Vec<*mut u8> = Vec::new();
    for (i, &node) in nodes.iter().enumerate() {
        if core.dbg_directory_node_bucket_for(node) != Some(unknown_bucket()) {
            continue;
        }
        // Best-effort: a re-registration can itself be wiped by a
        // hysteresis-pool release that fires mid-sweep (the decay is
        // wall-clock based). The attempt loop below re-checks the actual
        // postcondition (overflow node == unknown bucket) and retries the
        // whole phase, so a wiped re-registration must not panic here.
        reregister_node_bucket(&mut core, node, filling_classes[i]);
    }

    switch_node(&mut core, overflow_node);
    // Overflow-phase attempt loop: any deallocation in this test can release
    // a fully-consumed segment (pool overflow or a clock-based pool decay),
    // wiping that bucket's bits and — via R13-2 — freeing its slot. If that
    // happens before the overflow node's first publish, it legitimately
    // claims the freed slot instead of the unknown bucket. That is correct
    // allocator behavior, but it is not the scenario under test, so the
    // whole phase is retried until its precondition (all 8 dedicated slots
    // claimed at publish time) actually holds.
    let mut overflow_live: Vec<*mut u8> = Vec::new();
    let mut overflow_slot = 0usize;
    for _ in 0..8 {
        // Determinism guard: the overflow-node allocations must land in
        // segments stamped with the overflow node (so their bits go to the
        // shared unknown bucket). A leftover bump space in `small_cur`
        // (a foreign-node segment) would otherwise serve these blocks
        // silently under a foreign node's bucket. Force a fresh
        // overflow-node segment first.
        let warmup = force_fresh_segment(&mut core, overflow_node, overflow_class);
        let warmup_layout = layout_for_class(overflow_class);
        // Keep the first (possibly foreign-stamped) warmup block alive: see
        // the `warmup_keepalive` note at the end of the test.
        for p in &warmup[1..] {
            // SAFETY: live allocations of `warmup_layout` returned by `alloc`.
            unsafe { core.dealloc(*p, warmup_layout) };
        }
        warmup_keepalive.push(warmup[0]);
        // The warmup deallocations above can free a slot before the overflow
        // allocations start publishing; re-run the re-registration sweep so
        // "all 8 dedicated slots are claimed" holds at that moment.
        for (i, &node) in nodes.iter().enumerate() {
            if core.dbg_directory_node_bucket_for(node) != Some(unknown_bucket()) {
                continue;
            }
            // Best-effort: a re-registration can itself be wiped by a
            // hysteresis-pool release that fires mid-sweep (the decay is
            // wall-clock based). The attempt loop below re-checks the actual
            // postcondition (overflow node == unknown bucket) and retries the
            // whole phase, so a wiped re-registration must not panic here.
            reregister_node_bucket(&mut core, node, filling_classes[i]);
        }
        overflow_live = alloc_n(&mut core, overflow_class, count);
        let overflow_registration = overflow_live.pop().unwrap();
        overflow_slot = usize::try_from(core.dbg_segment_id_of(overflow_registration))
            .expect("segment slot id must fit the platform usize");
        // SAFETY: this is a live allocation made with this class's layout.
        unsafe { core.dealloc(overflow_registration, layout_for_class(overflow_class)) };
        if core.dbg_directory_node_bucket_for(overflow_node) == Some(unknown_bucket()) {
            break;
        }
        // A slot was freed mid-phase and the overflow node claimed it. A
        // plain release is not enough: the overflow node's free-list bits
        // keep its claimed bucket active, so no slot ever becomes free for
        // the next attempt's re-registration sweep. Drive the class fully
        // idle instead — that clears every overflow-node bit and frees its
        // slot for the retry.
        drive_class_to_idle(&mut core, overflow_class, &overflow_live);
    }
    assert_eq!(
        core.dbg_directory_node_bucket_for(overflow_node),
        Some(unknown_bucket())
    );
    assert_eq!(
        core.dbg_directory_get_bit_bucket(unknown_bucket(), overflow_class, overflow_slot),
        Some(true)
    );

    switch_node(&mut core, nodes[0]);
    drive_class_to_idle(&mut core, filling_classes[0], &node0_live);
    // A single drain pass can legally leave node 0 short of true idleness: a
    // segment of its class may retain free blocks (the drain stops at the
    // first fresh segment), or a stale-positive directory bit may survive
    // until the next scan self-heals it. Both keep the active-bit counter
    // above zero, so R13-2 correctly keeps the slot claimed. Keep driving —
    // each pass strictly consumes free blocks and/or self-heals a stale bit —
    // until the slot is actually released; only then is the invariant below
    // meaningful.
    for _ in 0..8 {
        if core.dbg_directory_node_bucket_for(nodes[0]) == Some(unknown_bucket()) {
            break;
        }
        drive_class_to_idle(&mut core, filling_classes[0], &[]);
    }
    assert_eq!(
        core.dbg_directory_node_bucket_for(nodes[0]),
        Some(unknown_bucket())
    );

    switch_node(&mut core, overflow_node);
    // Dedicated-phase attempt loop, mirroring the overflow one: a publish
    // into a node-0-stamped segment (via a foreign `small_cur` carve) or a
    // segment release can (re-)claim the last free slot right before the
    // dedicated allocation. Retry: drive node 0 back to idleness, which
    // re-frees its slot, and try again.
    let mut dedicated_live: Vec<*mut u8> = Vec::new();
    for _ in 0..8 {
        let warmup = force_fresh_segment(&mut core, overflow_node, overflow_class);
        let warmup_layout = layout_for_class(overflow_class);
        for p in &warmup[1..] {
            // SAFETY: live allocations of `warmup_layout` returned by `alloc`.
            unsafe { core.dealloc(*p, warmup_layout) };
        }
        warmup_keepalive.push(warmup[0]);
        dedicated_live = alloc_n(&mut core, dedicated_class, 1);
        let registration = dedicated_live.pop().unwrap();
        // SAFETY: this is a live allocation made with this class's layout.
        unsafe { core.dealloc(registration, layout_for_class(dedicated_class)) };
        if core.dbg_directory_node_bucket_for(overflow_node) != Some(unknown_bucket()) {
            break;
        }
        switch_node(&mut core, nodes[0]);
        drive_class_to_idle(&mut core, filling_classes[0], &[]);
        switch_node(&mut core, overflow_node);
    }
    let dedicated_bucket = core.dbg_directory_node_bucket_for(overflow_node).unwrap();
    assert_ne!(dedicated_bucket, unknown_bucket());

    // The overflow-class bit predates the dedicated bucket. Its empty
    // transition must clear that stale unknown-bucket copy as well.
    drive_class_to_idle(&mut core, overflow_class, &overflow_live);
    assert_eq!(
        core.dbg_directory_get_bit_bucket(unknown_bucket(), overflow_class, overflow_slot),
        Some(false)
    );
    assert!(core.dbg_find_segment_with_free(overflow_class).is_none());

    let class_count = AllocCore::dbg_small_class_count();
    let table_entries = usize::try_from(core.dbg_table_count())
        .expect("segment table count must fit the platform usize");
    for bucket in 0..unknown_bucket() {
        let set_bits = (0..class_count)
            .flat_map(|class| (0..table_entries).map(move |slot| (class, slot)))
            .filter(|&(class, slot)| {
                core.dbg_directory_get_bit_bucket(bucket, class, slot) == Some(true)
            })
            .count() as u32;
        assert_eq!(
            core.dbg_directory_active_bits_for_bucket(bucket),
            Some(set_bits),
            "active-bit counter diverged for bucket {bucket}"
        );
    }

    for (node, class, live) in other_live {
        switch_node(&mut core, node);
        for p in live {
            // SAFETY: each pointer remains a live allocation of this layout.
            unsafe { core.dealloc(p, layout_for_class(class)) };
        }
    }
    switch_node(&mut core, overflow_node);
    for p in dedicated_live {
        // SAFETY: each pointer remains a live allocation of this layout.
        unsafe { core.dealloc(p, layout_for_class(dedicated_class)) };
    }
    for p in warmup_keepalive {
        // SAFETY: each pointer remains a live allocation of this layout.
        unsafe { core.dealloc(p, layout_for_class(overflow_class)) };
    }
}
