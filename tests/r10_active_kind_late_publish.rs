#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::{AllocCore, SegmentLayout};
use std::sync::{Arc, Barrier};

#[test]
fn small_publication_after_empty_word_cut_keeps_membership() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let layout = Layout::from_size_align(64, 8).unwrap();
    let ptr = core.alloc(layout);
    assert!(!ptr.is_null());
    assert_eq!(core.dbg_active_kind_census(), (1, 0, true));
    let before = core.dbg_live_count_for(ptr).unwrap();
    let address = ptr.expose_provenance();
    let offset = (address & (SegmentLayout::SEGMENT - 1)) as u32;
    let ready = Arc::new(Barrier::new(2));
    let go = Arc::new(Barrier::new(2));
    let foreign = {
        let ready = ready.clone();
        let go = go.clone();
        std::thread::spawn(move || {
            let pin = RouteDirectory::global()
                .lookup(core::ptr::with_exposed_provenance_mut(address))
                .expect("issued Small route");
            ready.wait();
            go.wait();
            // SAFETY: ptr is uniquely issued, and this is its sole free transfer.
            unsafe { pin.publish_small(offset) }
        })
    };
    ready.wait();
    assert_eq!(core.dbg_find_segment_with_free(0), None);
    assert_eq!(core.dbg_active_kind_census(), (1, 0, true));
    assert!(!core.dbg_is_free_for(ptr));
    go.wait();
    assert!(foreign.join().unwrap());
    assert_eq!(core.dbg_find_segment_with_free(0), None);
    assert!(core.dbg_is_free_for(ptr));
    assert_eq!(core.dbg_live_count_for(ptr), Some(before - 1));
    assert_eq!(core.dbg_active_kind_census(), (1, 0, true));
}

#[test]
fn large_publication_after_failed_claim_retires_once() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let ptr = core.alloc(layout);
    assert!(!ptr.is_null());
    assert_eq!(core.dbg_active_kind_census(), (1, 1, true));
    let address = ptr.expose_provenance();
    let ready = Arc::new(Barrier::new(2));
    let go = Arc::new(Barrier::new(2));
    let foreign = {
        let ready = ready.clone();
        let go = go.clone();
        std::thread::spawn(move || {
            let pin = RouteDirectory::global()
                .lookup(core::ptr::with_exposed_provenance_mut(address))
                .expect("issued Large route");
            ready.wait();
            go.wait();
            // SAFETY: ptr is the one live Large issue, transferred once.
            unsafe { pin.publish_large() }
        })
    };
    ready.wait();
    assert_eq!(core.dbg_drain_large_sidecar_ingress(), 0);
    assert_eq!(core.dbg_active_kind_census(), (1, 1, true));
    go.wait();
    assert!(foreign.join().unwrap());
    assert_eq!(core.dbg_drain_large_sidecar_ingress(), 1);
    assert_eq!(core.dbg_drain_large_sidecar_ingress(), 0);
    assert_eq!(core.dbg_active_kind_census(), (1, 0, true));
}

#[cfg(feature = "alloc-decommit")]
#[test]
fn last_small_credit_prevents_prepublication_recycle() {
    use sefer_alloc::{LargeCacheConfig, SmallSegmentPoolConfig};

    let config = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0));
    let mut core = AllocCore::dbg_new_routed_with_config_for_test(config).unwrap();
    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    let mut blocks = Vec::new();
    for _ in 0..64 {
        let ptr = core.alloc(layout);
        assert!(!ptr.is_null());
        let id = core.dbg_segment_id_of(ptr);
        blocks.push((ptr, id));
        if id == 2 {
            break;
        }
    }
    assert_eq!(core.dbg_active_kind_census(), (3, 0, true));
    let victim = blocks.iter().find(|(_, id)| *id == 1).unwrap().0;
    for &(ptr, id) in &blocks {
        if id == 1 && ptr != victim {
            // SAFETY: these id-1 blocks are live and distinct from victim.
            unsafe { core.dealloc(ptr, layout) };
        }
    }
    assert_eq!(core.dbg_live_count_for(victim), Some(1));
    let address = victim.expose_provenance();
    let offset = (address & (SegmentLayout::SEGMENT - 1)) as u32;
    let ready = Arc::new(Barrier::new(2));
    let go = Arc::new(Barrier::new(2));
    let foreign = {
        let ready = ready.clone();
        let go = go.clone();
        std::thread::spawn(move || {
            let pin = RouteDirectory::global()
                .lookup(core::ptr::with_exposed_provenance_mut(address))
                .unwrap();
            ready.wait();
            go.wait();
            // SAFETY: victim is the last unique live issue in segment 1.
            unsafe { pin.publish_small(offset) }
        })
    };
    ready.wait();
    assert_eq!(core.dbg_find_segment_with_free(0), None);
    assert_eq!(core.dbg_active_kind_census(), (3, 0, true));
    go.wait();
    assert!(foreign.join().unwrap());
    assert_eq!(core.dbg_find_segment_with_free(0), None);
    assert_eq!(core.dbg_active_kind_census(), (2, 0, true));
    for (ptr, id) in blocks {
        if id != 1 {
            // SAFETY: only id-1 allocations were freed or transferred above.
            unsafe { core.dealloc(ptr, layout) };
        }
    }
}
