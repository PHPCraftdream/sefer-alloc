#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::{AllocCore, LargeCacheConfig, SegmentLayout, SmallSegmentPoolConfig};

#[test]
fn cached_large_is_inactive_and_reissue_gets_fresh_incarnation() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    core.dbg_set_large_cache_budget(None);
    let layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let first = core.alloc(layout);
    assert!(!first.is_null());
    let old = RouteDirectory::global()
        .lookup(first)
        .unwrap()
        .incarnation();
    assert_eq!(core.dbg_active_kind_census(), (1, 1, true));
    // SAFETY: first is the single live Large issue, freed once.
    unsafe { core.dealloc(first, layout) };
    assert!(core.dbg_large_cache_used() > 0);
    assert_eq!(core.dbg_active_kind_census(), (1, 0, true));
    let second = core.alloc(layout);
    assert!(!second.is_null());
    assert_eq!(second, first);
    assert_ne!(
        RouteDirectory::global()
            .lookup(second)
            .unwrap()
            .incarnation(),
        old
    );
    assert_eq!(core.dbg_active_kind_census(), (1, 1, true));
    // SAFETY: second is the fresh unique issue, freed once.
    unsafe { core.dealloc(second, layout) };
}

#[test]
fn pooled_small_stays_registered_until_pool_drain() {
    let config = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(1));
    let mut core = AllocCore::dbg_new_routed_with_config_for_test(config).unwrap();
    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    let mut ptrs = Vec::new();
    for _ in 0..64 {
        let ptr = core.alloc(layout);
        assert!(!ptr.is_null());
        ptrs.push(ptr);
        if core.dbg_segment_id_of(ptr) == 2 {
            break;
        }
    }
    assert_eq!(core.dbg_active_kind_census(), (3, 0, true));
    let first_small: Vec<_> = ptrs
        .iter()
        .copied()
        .filter(|&ptr| core.dbg_segment_id_of(ptr) == 1)
        .collect();
    assert!(!first_small.is_empty());
    for ptr in first_small {
        // SAFETY: each selected block is live, unique, and belongs to id 1.
        unsafe { core.dealloc(ptr, layout) };
    }
    assert_eq!(core.dbg_pooled_count(), 1);
    assert_eq!(core.dbg_active_kind_census(), (3, 0, true));
    assert_eq!(core.dbg_drain_small_pool(), 1);
    assert_eq!(core.dbg_active_kind_census(), (2, 0, true));
    for ptr in ptrs {
        if core.dbg_contains_base(ptr) {
            // SAFETY: remaining registered blocks were not freed above.
            unsafe { core.dealloc(ptr, layout) };
        }
    }
    assert!(core.dbg_active_kind_census().2);
}
