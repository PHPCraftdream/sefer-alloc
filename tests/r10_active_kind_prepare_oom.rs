#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::AllocCore;

static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn primordial_attach_failure_retries_with_seed_only() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    RouteDirectory::fail_next_registration_for_test();
    assert!(AllocCore::dbg_new_routed_for_test().is_none());
    let core = AllocCore::dbg_new_routed_for_test().unwrap();
    assert_eq!(core.dbg_table_count(), 1);
    assert_eq!(core.dbg_active_kind_census(), (1, 0, true));
}

#[test]
fn append_and_reuse_prepare_failure_do_not_change_membership() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let released = AllocCore::dbg_segments_released_total();
    RouteDirectory::fail_next_registration_for_test();
    assert!(core.alloc(layout).is_null());
    assert_eq!(AllocCore::dbg_segments_released_total() - released, 1);
    assert_eq!(core.dbg_table_count(), 1);
    assert_eq!(core.dbg_active_kind_census(), (1, 0, true));

    let first = core.alloc(layout);
    assert!(!first.is_null());
    assert_eq!(core.dbg_segment_id_of(first), 1);
    let first_incarnation = RouteDirectory::global()
        .lookup(first)
        .unwrap()
        .incarnation();
    assert_eq!(core.dbg_active_kind_census(), (1, 1, true));
    core.dbg_set_large_cache_budget(Some(0));
    // SAFETY: first is the one live allocation of layout, freed once.
    unsafe { core.dealloc(first, layout) };
    assert_eq!(core.dbg_table_count(), 2);
    assert_eq!(core.dbg_active_kind_census(), (1, 0, true));

    let released = AllocCore::dbg_segments_released_total();
    RouteDirectory::fail_next_registration_for_test();
    assert!(core.alloc(layout).is_null());
    assert_eq!(AllocCore::dbg_segments_released_total() - released, 1);
    assert_eq!(core.dbg_table_count(), 2);
    assert_eq!(core.dbg_active_kind_census(), (1, 0, true));
    let second = core.alloc(layout);
    assert!(!second.is_null());
    assert_eq!(core.dbg_segment_id_of(second), 1);
    let second_incarnation = RouteDirectory::global()
        .lookup(second)
        .unwrap()
        .incarnation();
    assert_ne!(second_incarnation, first_incarnation);
    assert_eq!(core.dbg_active_kind_census(), (1, 1, true));
    // SAFETY: second is the fresh live route instance, freed once.
    unsafe { core.dealloc(second, layout) };
    assert_eq!(core.dbg_active_kind_census(), (1, 0, true));
}
