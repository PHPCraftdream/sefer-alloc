#![cfg(all(
    feature = "production",
    feature = "numa-aware-mock",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-stats",
    numa_shim_mock
))]

use core::alloc::Layout;
use numa_shim::mock;
use sefer_alloc::{AllocCore, SegmentLayout};

fn switch_node(core: &mut AllocCore, node: u32) {
    mock::set_current_node(node);
    let _ = mock::drain();
    core.dbg_invalidate_numa_node_cache();
}

#[test]
fn routed_negative_fallback_prefers_local_and_unknown_before_foreign() {
    mock::set_current_node(3);
    let _ = mock::drain();
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    core.dbg_set_large_cache_budget(Some(0));
    let large = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let mut large_ptrs = Vec::with_capacity(127);
    for _ in 0..127 {
        let ptr = core.alloc(large);
        assert!(!ptr.is_null());
        large_ptrs.push(ptr);
    }
    assert_eq!(core.dbg_table_count(), 128);
    // LIFO free-list reuse: descending retirement makes the foreign Small
    // take a lower slot than the later local Small.
    for ptr in large_ptrs.into_iter().rev() {
        // SAFETY: each ptr is a distinct live Large allocation.
        unsafe { core.dealloc(ptr, large) };
    }
    assert_eq!(core.dbg_active_kind_census(), (1, 0, true));

    let small = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    let class = SegmentLayout::class_for(SegmentLayout::SMALL_MAX, 1).unwrap();
    let mut blocks = Vec::new();
    let mut foreign = None;
    let mut unknown = None;
    let mut local = None;
    for _ in 0..96 {
        let ptr = core.alloc(small);
        assert!(!ptr.is_null());
        let id = core.dbg_segment_id_of(ptr);
        blocks.push((ptr, id));
        if id != 0 && foreign.is_none() {
            foreign = Some((ptr, id));
            switch_node(&mut core, u32::MAX);
        } else if id != 0 && Some(id) != foreign.map(|(_, slot)| slot) && unknown.is_none() {
            unknown = Some((ptr, id));
            switch_node(&mut core, 1);
        } else if id != 0
            && Some(id) != foreign.map(|(_, slot)| slot)
            && Some(id) != unknown.map(|(_, slot)| slot)
        {
            local = Some((ptr, id));
            if core.dbg_freelist_head_for(ptr, class) == u32::MAX {
                break;
            }
        }
    }
    let (foreign_ptr, foreign_slot) = foreign.expect("foreign Small route");
    let (unknown_ptr, unknown_slot) = unknown.expect("unknown Small route");
    let (local_ptr, local_slot) = local.expect("local Small route");
    assert!(foreign_slot < unknown_slot && unknown_slot < local_slot);
    assert_eq!(core.dbg_node_id_for(foreign_ptr), Some(3));
    assert_eq!(core.dbg_node_id_for(unknown_ptr), Some(u32::MAX));
    assert_eq!(core.dbg_node_id_for(local_ptr), Some(1));
    assert_eq!(core.dbg_active_kind_census(), (4, 0, true));
    assert!(core.dbg_directory_is_materialised());
    assert_eq!(core.dbg_freelist_head_for(local_ptr, class), u32::MAX);

    // SAFETY: foreign_ptr is one live block; other blocks keep this route live.
    unsafe { core.dealloc(foreign_ptr, small) };
    assert!(core.dbg_directory_force_clear_bit(class, foreign_slot as usize));
    let fallback_before = AllocCore::dbg_directory_fallback_scans();
    let probes_before = AllocCore::dbg_full_scan_slots_examined();
    let chosen = core.dbg_find_segment_with_free(class).unwrap();
    assert_eq!(core.dbg_segment_id_of(chosen), foreign_slot);
    assert_eq!(
        AllocCore::dbg_directory_fallback_scans() - fallback_before,
        1
    );
    assert_eq!(AllocCore::dbg_full_scan_slots_examined() - probes_before, 4);

    // SAFETY: local_ptr is still a live, distinct block.
    unsafe { core.dealloc(local_ptr, small) };
    assert!(core.dbg_directory_force_clear_bit(class, foreign_slot as usize));
    assert!(core.dbg_directory_force_clear_bit(class, local_slot as usize));
    let probes_before = AllocCore::dbg_full_scan_slots_examined();
    let chosen = core.dbg_find_segment_with_free(class).unwrap();
    assert_eq!(core.dbg_segment_id_of(chosen), local_slot);
    assert_eq!(AllocCore::dbg_full_scan_slots_examined() - probes_before, 4);

    // SAFETY: unknown_ptr is live and distinct from the prior victims.
    unsafe { core.dealloc(unknown_ptr, small) };
    for slot in [foreign_slot, unknown_slot, local_slot] {
        assert!(core.dbg_directory_force_clear_bit(class, slot as usize));
    }
    let chosen = core.dbg_find_segment_with_free(class).unwrap();
    assert_eq!(core.dbg_segment_id_of(chosen), unknown_slot);
    assert_eq!(core.dbg_active_kind_census(), (4, 0, true));

    for (ptr, _) in blocks {
        if ptr != foreign_ptr && ptr != local_ptr && ptr != unknown_ptr {
            // SAFETY: all other blocks remain uniquely issued.
            unsafe { core.dealloc(ptr, small) };
        }
    }
}
