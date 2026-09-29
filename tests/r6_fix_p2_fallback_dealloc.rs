//! R6-02: fallback-owned frees from a fallback-using thread should reclaim
//! directly, while a nested free must avoid reacquiring the fallback lock.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals",
    not(miri)
))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::global::tls_heap;
use sefer_alloc::registry::HeapCore;
use sefer_alloc::SeferAlloc;

#[test]
fn fallback_frees_reclaim_directly_and_nested_remote_frees_survive() {
    std::thread::spawn(|| {
        let allocator = SeferAlloc::new();
        let layout = Layout::from_size_align(64, 8).expect("valid layout");
        let saved = tls_heap::dbg_mark_local_torn_for_test();

        // TORN forces the production alloc entry through the fallback.
        // SAFETY: the layout is valid and non-zero.
        let direct = unsafe { allocator.alloc(layout) };
        assert!(!direct.is_null());
        assert_eq!(
            HeapCore::dbg_with_fallback_for_test(|heap| heap.dbg_owner_id_for(direct)),
            Some(Some(
                sefer_alloc::alloc_core::segment_header::OWNER_ID_FALLBACK
            ))
        );

        // Same-thread fallback-owned free must be recognized under the
        // fallback lock and reclaimed without waiting for another operation.
        // SAFETY: `direct` is live and was allocated above with this layout.
        unsafe { allocator.dealloc(direct, layout) };
        assert_eq!(
            HeapCore::dbg_with_fallback_for_test(|heap| heap.dbg_is_free_for(direct)),
            Some(true)
        );

        // This free re-enters GlobalAlloc::dealloc while the fallback lock is
        // held. It must use foreign routing rather than recursively lock;
        // the subsequent owner drain proves the legal free was retained.
        // SAFETY: valid non-zero layout.
        let nested = unsafe { allocator.alloc(layout) };
        assert!(!nested.is_null());
        // SAFETY: `nested` is a live allocation from this allocator.
        unsafe { SeferAlloc::dbg_dealloc_while_fallback_lock_held(nested, layout) };
        assert_eq!(
            HeapCore::dbg_with_fallback_for_test(|heap| {
                heap.dbg_drain_all_rings();
                heap.dbg_is_free_for(nested)
            }),
            Some(true)
        );

        // SAFETY: returned by this thread's TORN test hook; restore it on the
        // same thread before the worker exits.
        unsafe { tls_heap::dbg_restore_local_for_test(saved) };
    })
    .join()
    .expect("fallback regression worker panicked");
}
