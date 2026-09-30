#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]
#![cfg(all(feature = "alloc-xthread", feature = "internals"))]

use std::alloc::Layout;

use sefer_alloc::AllocCore;

#[test]
fn primordial_credit_survives_publication_until_owner_reclaim() {
    let mut core = AllocCore::dbg_new_routed_for_test().expect("routed primordial reservation");
    let layout = Layout::from_size_align(32, 8).unwrap();
    let ptr = core.alloc(layout);
    assert!(!ptr.is_null());
    assert_eq!(core.dbg_live_count_for(ptr), Some(1));

    // SAFETY: `ptr` is the sole live allocation with the matching class;
    // the test transfers it once to the ring and never dereferences it again.
    assert!(unsafe { core.dbg_publish_small_sidecar_free(ptr) });
    assert_eq!(core.dbg_live_count_for(ptr), Some(1));
    core.dbg_drain_sidecar_ingress();
    assert_eq!(core.dbg_live_count_for(ptr), Some(0));

    let reused = core.alloc(layout);
    assert_eq!(reused, ptr);
    assert_eq!(core.dbg_live_count_for(ptr), Some(1));
    // SAFETY: `reused` is the current allocation with its original layout.
    unsafe { core.dealloc(reused, layout) };
    assert_eq!(core.dbg_live_count_for(ptr), Some(0));
}

#[test]
fn refill_and_flush_hold_exactly_one_credit_per_block() {
    let mut core = AllocCore::new().expect("primordial reservation");
    let layout = Layout::from_size_align(64, 8).unwrap();
    let class = core.dbg_layout_class_for(layout).unwrap();
    let mut blocks = [std::ptr::null_mut(); 8];
    assert_eq!(core.refill_class_bump(class, &mut blocks), blocks.len());
    assert_eq!(core.dbg_live_count_for(blocks[0]), Some(8));
    // SAFETY: each block was issued once above with the same class; flush
    // transfers all eight unique allocations back to their owner.
    unsafe { core.flush_class(class, &blocks) };
    assert_eq!(core.dbg_live_count_for(blocks[0]), Some(0));

    let mut again = [std::ptr::null_mut(); 8];
    assert_eq!(core.refill_class_bump(class, &mut again), again.len());
    assert_eq!(core.dbg_live_count_for(again[0]), Some(8));
    // SAFETY: all entries of `again` are distinct current allocations.
    unsafe { core.flush_class(class, &again) };
    assert_eq!(core.dbg_live_count_for(again[0]), Some(0));
}

#[test]
fn realloc_keeps_one_credit_across_in_place_move_and_failure() {
    let mut core = AllocCore::new().expect("primordial reservation");
    let old = Layout::from_size_align(32, 8).unwrap();
    let ptr = core.alloc(old);
    assert!(!ptr.is_null());
    assert_eq!(core.dbg_live_count_for(ptr), Some(1));

    // SAFETY: `ptr` is the current allocation and `old` is its exact layout.
    let same = unsafe { core.realloc(ptr, old, 24) };
    assert_eq!(same, ptr);
    assert_eq!(core.dbg_live_count_for(same), Some(1));

    let shrunk = Layout::from_size_align(24, 8).unwrap();
    // SAFETY: `same` is current and `shrunk` matches the successful realloc.
    let failed = unsafe { core.realloc(same, shrunk, usize::MAX) };
    assert!(failed.is_null());
    assert_eq!(core.dbg_live_count_for(same), Some(1));

    // SAFETY: a failed realloc leaves `same` live with layout `shrunk`.
    let moved = unsafe { core.realloc(same, shrunk, 2048) };
    assert!(!moved.is_null());
    assert_ne!(moved, same);
    assert_eq!(core.dbg_live_count_for(moved), Some(1));
    // SAFETY: `moved` is the current allocation with the requested layout.
    unsafe { core.dealloc(moved, Layout::from_size_align(2048, 8).unwrap()) };
    assert_eq!(core.dbg_live_count_for(moved), Some(0));
}
