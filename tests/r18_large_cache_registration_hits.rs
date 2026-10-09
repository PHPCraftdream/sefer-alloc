#![cfg(all(
    feature = "production",
    feature = "alloc-stats",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::AllocCore;

#[test]
fn cache_registration_refusal_is_not_a_served_hit() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    core.dbg_set_large_cache_budget(None);
    let layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let first = core.alloc(layout);
    assert!(!first.is_null());
    // SAFETY: first is live with the matching layout and freed exactly once.
    unsafe { core.dealloc(first, layout) };
    let cached_bytes = core.dbg_large_cache_used();
    assert!(cached_bytes > 0);

    let hits = core.dbg_large_cache_hits();
    let reserved = AllocCore::dbg_segments_reserved_total();
    let released = AllocCore::dbg_segments_released_total();
    let hit = core.alloc(layout);
    assert!(!hit.is_null());
    assert_eq!(hit, first);
    assert_eq!(core.dbg_large_cache_hits() - hits, 1);
    assert_eq!(AllocCore::dbg_segments_reserved_total() - reserved, 0);
    assert_eq!(AllocCore::dbg_segments_released_total() - released, 0);
    assert_eq!(core.dbg_large_cache_used(), 0);
    assert!(RouteDirectory::global().lookup(hit).is_some());
    // SAFETY: hit is the newly issued live instance, freed once with its layout.
    unsafe { core.dealloc(hit, layout) };
    assert_eq!(core.dbg_large_cache_used(), cached_bytes);

    let hits = core.dbg_large_cache_hits();
    let reserved = AllocCore::dbg_segments_reserved_total();
    let released = AllocCore::dbg_segments_released_total();
    RouteDirectory::fail_next_registration_for_test();
    let fallback = core.alloc(layout);
    assert!(
        !fallback.is_null(),
        "slow fallback must serve the allocation"
    );
    assert_eq!(
        core.dbg_large_cache_hits() - hits,
        0,
        "refused cache registration must not count as a served hit"
    );
    assert_eq!(AllocCore::dbg_segments_reserved_total() - reserved, 1);
    assert_eq!(AllocCore::dbg_segments_released_total() - released, 1);
    assert_eq!(core.dbg_large_cache_used(), 0);
    assert!(RouteDirectory::global().lookup(fallback).is_some());
    assert_eq!(core.dbg_active_kind_census(), (1, 1, true));
    // SAFETY: successful fallback provides layout.size() writable bytes and is
    // freed exactly once with the original layout after checking payload access.
    unsafe {
        fallback.write_bytes(0x5a, layout.size());
        assert_eq!(fallback.read(), 0x5a);
        assert_eq!(fallback.add(layout.size() - 1).read(), 0x5a);
        core.dealloc(fallback, layout);
    }
    assert_eq!(core.dbg_large_cache_used(), cached_bytes);

    let hits = core.dbg_large_cache_hits();
    let reserved = AllocCore::dbg_segments_reserved_total();
    let released = AllocCore::dbg_segments_released_total();
    let control = core.alloc(layout);
    assert!(!control.is_null());
    assert_eq!(control, fallback);
    assert_eq!(core.dbg_large_cache_hits() - hits, 1);
    assert_eq!(AllocCore::dbg_segments_reserved_total() - reserved, 0);
    assert_eq!(AllocCore::dbg_segments_released_total() - released, 0);
    assert!(RouteDirectory::global().lookup(control).is_some());
    // SAFETY: control is live with the matching layout and freed exactly once.
    unsafe { core.dealloc(control, layout) };
    assert_eq!(core.dbg_active_kind_census(), (1, 0, true));
    assert_eq!(core.dbg_large_cache_used(), cached_bytes);
}
