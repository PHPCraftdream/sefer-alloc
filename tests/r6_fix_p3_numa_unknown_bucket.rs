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

    switch_node(&mut core, overflow_node);
    let mut overflow_live = alloc_n(&mut core, overflow_class, count);
    let overflow_registration = overflow_live.pop().unwrap();
    let overflow_slot = usize::try_from(core.dbg_segment_id_of(overflow_registration))
        .expect("segment slot id must fit the platform usize");
    // SAFETY: this is a live allocation made with this class's layout.
    unsafe { core.dealloc(overflow_registration, layout_for_class(overflow_class)) };
    let unknown = AllocCore::dbg_directory_node_bitmaps() - 1;
    assert_eq!(
        core.dbg_directory_node_bucket_for(overflow_node),
        Some(unknown)
    );
    assert_eq!(
        core.dbg_directory_get_bit_bucket(unknown, overflow_class, overflow_slot),
        Some(true)
    );

    switch_node(&mut core, nodes[0]);
    drive_class_to_idle(&mut core, filling_classes[0], &node0_live);
    assert_eq!(core.dbg_directory_node_bucket_for(nodes[0]), Some(unknown));

    switch_node(&mut core, overflow_node);
    let mut dedicated_live = alloc_n(&mut core, dedicated_class, 1);
    let registration = dedicated_live.pop().unwrap();
    // SAFETY: this is a live allocation made with this class's layout.
    unsafe { core.dealloc(registration, layout_for_class(dedicated_class)) };
    let dedicated_bucket = core.dbg_directory_node_bucket_for(overflow_node).unwrap();
    assert_ne!(dedicated_bucket, unknown);

    // The overflow-class bit predates the dedicated bucket. Its empty
    // transition must clear that stale unknown-bucket copy as well.
    drive_class_to_idle(&mut core, overflow_class, &overflow_live);
    assert_eq!(
        core.dbg_directory_get_bit_bucket(unknown, overflow_class, overflow_slot),
        Some(false)
    );
    assert!(core.dbg_find_segment_with_free(overflow_class).is_none());

    let class_count = AllocCore::dbg_small_class_count();
    let table_entries = usize::try_from(core.dbg_table_count())
        .expect("segment table count must fit the platform usize");
    for bucket in 0..unknown {
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
}
