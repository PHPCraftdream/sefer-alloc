#![cfg(all(feature = "alloc-core", feature = "internals"))]

use core::alloc::Layout;
use sefer_alloc::AllocCore;

#[test]
fn standalone_drop_releases_live_and_cached_large_once() {
    let before_reserved = AllocCore::dbg_segments_reserved_total();
    let before_released = AllocCore::dbg_segments_released_total();
    let mut core = AllocCore::new().expect("primordial reservation");
    let layout = Layout::from_size_align(2 * 1024 * 1024, 8).expect("large layout");
    let mut pointers = Vec::new();
    for _ in 0..10 {
        let ptr = core.alloc(layout);
        assert!(!ptr.is_null());
        pointers.push(ptr);
    }

    for ptr in pointers.drain(..9) {
        // SAFETY: each pointer is a distinct live allocation from this core.
        unsafe { core.dealloc(ptr, layout) };
    }

    #[cfg(all(feature = "large-cache-extended", feature = "internals"))]
    assert!(core.dbg_large_cache_extension_materialised());

    let released_before_drop = AllocCore::dbg_segments_released_total();
    #[cfg(not(feature = "alloc-decommit"))]
    assert_eq!(released_before_drop - before_released, 9);
    #[cfg(all(feature = "alloc-decommit", not(feature = "large-cache-extended")))]
    assert_eq!(released_before_drop - before_released, 1);
    #[cfg(feature = "large-cache-extended")]
    assert_eq!(released_before_drop - before_released, 0);

    let reserved = AllocCore::dbg_segments_reserved_total() - before_reserved;
    assert!(reserved >= 11);
    drop(core);
    assert_eq!(
        AllocCore::dbg_segments_released_total() - before_released,
        reserved,
        "Drop must release every Large and its sidecar exactly once"
    );
}

#[test]
fn drop_uses_phase_credit_protocol() {
    let source = include_str!("../src/alloc_core/alloc_core/lifecycle.rs");
    assert!(!source.contains(".mark_large_released()"));
    assert!(source.contains(".release_cached(generation)"));
    assert!(source.contains(".claim_live()"));
    assert!(source.contains(".release_consumed(generation)"));
}
