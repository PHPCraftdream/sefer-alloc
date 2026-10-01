#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::global::tls_heap;
use sefer_alloc::registry::HeapCore;
use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

#[test]
fn default_fallback_can_cache_a_freed_large_span() {
    let layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let saved = tls_heap::dbg_mark_local_torn_for_test();
    // SAFETY: the returned pointer is freed once with its original layout.
    unsafe {
        let ptr = GLOBAL.alloc(layout);
        assert!(!ptr.is_null());
        GLOBAL.dealloc(ptr, layout);
    }
    // SAFETY: saved came from this thread's LOCAL and its slot is still live.
    unsafe { tls_heap::dbg_restore_local_for_test(saved) };
    let (budget, used) = HeapCore::dbg_with_fallback_for_test(|h| {
        (h.dbg_large_cache_budget(), h.dbg_large_cache_used())
    })
    .expect("fallback initialized");
    // `large-cache-extended` resolves the default to a finite budget.
    if cfg!(feature = "large-cache-extended") {
        assert!(budget.is_some_and(|b| b >= layout.size()));
    } else {
        assert_eq!(budget, None);
    }
    assert!(used > 0, "default policy should retain the freed span");
}
