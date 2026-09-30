#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::registry::HeapCore;
use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};

#[test]
fn fallback_owner_consumes_all_remote_terminal_publications() {
    let layout = Layout::from_size_align(64, 16).unwrap();
    let (mut addresses, anchor, before) = HeapCore::dbg_with_fallback_for_test(|heap| {
        let addresses: Vec<_> = (0..257)
            .map(|_| heap.alloc(layout).expose_provenance())
            .collect();
        assert!(addresses.iter().all(|address| *address != 0));
        let anchor = heap.alloc(layout);
        assert!(!anchor.is_null());
        let before = heap.dbg_live_count_for(anchor).unwrap();
        (addresses, anchor.expose_provenance(), before)
    })
    .expect("real fallback heap initialized");
    let pending = addresses.pop().expect("one block for the busy-lock path");
    let remote_addresses = addresses.clone();
    std::thread::spawn(move || {
        for address in remote_addresses {
            // SAFETY: each current fallback allocation was uniquely transferred
            // to this thread and is freed once with its exact original Layout.
            unsafe {
                SeferAlloc::new().dealloc(std::ptr::with_exposed_provenance_mut(address), layout);
            }
        }
    })
    .join()
    .unwrap();
    // SAFETY: `pending` is the one still-issued fallback block. This hook
    // holds the fallback owner lock, forcing the foreign free to publish.
    unsafe {
        SeferAlloc::dbg_dealloc_while_fallback_lock_held(
            std::ptr::with_exposed_provenance_mut(pending),
            layout,
        );
    }
    HeapCore::dbg_with_fallback_for_test(|heap| {
        let anchor = std::ptr::with_exposed_provenance_mut(anchor);
        let pending_ptr = std::ptr::with_exposed_provenance_mut(pending);
        let class = heap.dbg_class_for(layout).unwrap();
        let after_idle = heap.dbg_live_count_for(anchor).unwrap();
        for address in &addresses {
            let ptr = std::ptr::with_exposed_provenance_mut(*address);
            assert!(
                heap.dbg_is_free_for(ptr) || heap.dbg_tcache_contains(class, ptr),
                "idle-lock free remains issued"
            );
        }
        // The 256 idle-lock frees retire synchronously. Magazine residents
        // remain bitmap-live until flushed; pre-existing magazine residents
        // can also be flushed by these frees, so live_count is not an exact
        // count of the just-freed pointers.
        assert!(after_idle < before);
        assert!(!heap.dbg_is_free_for(pending_ptr));
        assert!(!heap.dbg_tcache_contains(class, pending_ptr));
        assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
        assert!(heap.dbg_is_free_for(pending_ptr));
        assert_eq!(heap.dbg_live_count_for(anchor), Some(after_idle - 1));
        assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
        // SAFETY: anchor is the only issued allocation not transferred remotely.
        unsafe {
            heap.dealloc(anchor, layout);
        }
    })
    .expect("fallback remains initialized");
}
