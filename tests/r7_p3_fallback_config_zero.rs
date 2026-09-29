#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::global::tls_heap;
use sefer_alloc::registry::HeapCore;
use sefer_alloc::{LargeCacheConfig, SeferAlloc};

const ZERO: LargeCacheConfig = LargeCacheConfig::new().budget_bytes(0);

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::with_config(ZERO);

struct TornGuard(*mut HeapCore);

impl TornGuard {
    fn new() -> Self {
        Self(tls_heap::dbg_mark_local_torn_for_test())
    }
}

impl Drop for TornGuard {
    fn drop(&mut self) {
        // SAFETY: this is the saved LOCAL value from this same thread; the
        // guard neither transfers it nor recycles its owning slot.
        unsafe { tls_heap::dbg_restore_local_for_test(self.0) };
    }
}

#[test]
fn configured_fallback_keeps_zero_budget_and_signals_later_conflicts() {
    let large = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let small = Layout::from_size_align(64, 8).unwrap();
    let default = SeferAlloc::new();
    let _torn = TornGuard::new();

    // SAFETY: each non-null result is freed exactly once with its layout.
    unsafe {
        let ptr = GLOBAL.alloc(large);
        assert!(!ptr.is_null());
        GLOBAL.dealloc(ptr, large);
    }
    let (budget, used) = HeapCore::dbg_with_fallback_for_test(|h| {
        (h.dbg_large_cache_budget(), h.dbg_large_cache_used())
    })
    .expect("fallback initialized");
    assert_eq!(budget, Some(0));
    assert_eq!(used, 0, "freed large span must not be cached");

    let before = GLOBAL.fallback_config_conflicts();
    // SAFETY: the zeroed block is live and reallocated/freed exactly once.
    unsafe {
        let ptr = default.alloc_zeroed(small);
        assert!(!ptr.is_null());
        assert_eq!(*ptr, 0);
        let resized = default.realloc(ptr, small, 128);
        assert!(!resized.is_null());
        default.dealloc(resized, Layout::from_size_align(128, 8).unwrap());
    }
    assert_eq!(GLOBAL.fallback_config_conflicts(), before + 2);

    #[cfg(feature = "batch-api")]
    {
        let mut out = [std::ptr::null_mut(); 2];
        // SAFETY: the filled prefix is freed with the same layout below.
        let filled = unsafe { default.alloc_batch(small, &mut out) };
        assert_eq!(filled, 2);
        // SAFETY: both entries were returned live by alloc_batch.
        unsafe { default.dealloc_batch(small, &out[..filled]) };
        assert_eq!(GLOBAL.fallback_config_conflicts(), before + 3);
    }

    let (budget, used) = HeapCore::dbg_with_fallback_for_test(|h| {
        (h.dbg_large_cache_budget(), h.dbg_large_cache_used())
    })
    .expect("fallback remains live");
    assert_eq!((budget, used), (Some(0), 0));
}
