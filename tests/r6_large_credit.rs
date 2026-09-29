#![cfg(all(
    feature = "alloc-core",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::Layout;

use sefer_alloc::AllocCore;

const LIVE_1: u64 = (1 << 3) | 2;
#[cfg(feature = "alloc-decommit")]
const LIVE_2: u64 = (2 << 3) | 2;
#[cfg(feature = "alloc-decommit")]
const CACHED_1: u64 = (1 << 3) | 5;

#[test]
fn own_free_releases_registered_large_without_cache() {
    let mut core = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let ptr = core.alloc(layout);
    assert!(!ptr.is_null());
    assert_eq!(
        core.terminal_header_snapshot_for_test(ptr).map(|s| s.1),
        Some(LIVE_1)
    );
    #[cfg(feature = "alloc-decommit")]
    core.dbg_set_large_cache_budget(Some(0));
    // SAFETY: ptr is the current live allocation and layout is unchanged.
    unsafe { core.dealloc(ptr, layout) };
    assert_eq!(core.terminal_header_snapshot_for_test(ptr), None);
    #[cfg(feature = "alloc-decommit")]
    assert_eq!(core.dbg_large_cache_used(), 0);
}

#[cfg(feature = "alloc-decommit")]
#[test]
fn cache_hit_creates_new_instance_credit() {
    let mut core = AllocCore::new().expect("primordial");
    core.dbg_set_large_cache_budget(None);
    let layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let first = core.alloc(layout);
    assert!(!first.is_null());
    assert_eq!(
        core.terminal_header_snapshot_for_test(first).map(|s| s.1),
        Some(LIVE_1)
    );
    // SAFETY: first is live and freed once with its original layout.
    unsafe { core.dealloc(first, layout) };
    assert_eq!(core.terminal_header_snapshot_for_test(first), None);
    assert_eq!(core.terminal_cached_large_state_for_test(), Some(CACHED_1));

    let second = core.alloc(layout);
    assert_eq!(second, first);
    assert_eq!(
        core.terminal_header_snapshot_for_test(second).map(|s| s.1),
        Some(LIVE_2)
    );
    // SAFETY: second is a new live instance, freed once with its layout.
    unsafe { core.dealloc(second, layout) };
    assert_eq!(
        core.terminal_cached_large_state_for_test(),
        Some((2 << 3) | 5)
    );
}
