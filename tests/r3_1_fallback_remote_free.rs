#![cfg(all(feature = "alloc-global", feature = "alloc-xthread", feature = "alloc-decommit", feature = "internals", feature = "bench-internals"))]

use std::alloc::{GlobalAlloc, Layout};
use sefer_alloc::registry::HeapCore;
use sefer_alloc::SeferAlloc;

#[test]
fn fallback_owner_consumes_all_remote_terminal_publications() {
    let layout = Layout::from_size_align(64, 16).unwrap();
    let (addresses, anchor, before) = HeapCore::dbg_with_fallback_for_test(|heap| {
        let addresses: Vec<_> = (0..257).map(|_| heap.alloc(layout).expose_provenance()).collect();
        assert!(addresses.iter().all(|address| *address != 0));
        let anchor = heap.alloc(layout);
        assert!(!anchor.is_null());
        let before = heap.dbg_live_count_for(anchor).unwrap();
        (addresses, anchor.expose_provenance(), before)
    }).expect("real fallback heap initialized");
    std::thread::spawn(move || {
        for address in addresses {
            // SAFETY: each current fallback allocation was uniquely transferred
            // to this thread and is freed once with its exact original Layout.
            unsafe { SeferAlloc::new().dealloc(std::ptr::with_exposed_provenance_mut(address), layout); }
        }
    }).join().unwrap();
    HeapCore::dbg_with_fallback_for_test(|heap| {
        let anchor = std::ptr::with_exposed_provenance_mut(anchor);
        assert_eq!(heap.dbg_live_count_for(anchor), Some(before));
        assert_eq!(heap.dbg_drain_sidecar_ingress(), 257);
        assert_eq!(heap.dbg_live_count_for(anchor), Some(before - 257));
        assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
        // SAFETY: anchor is the only issued allocation not transferred remotely.
        unsafe { heap.dealloc(anchor, layout); }
    }).expect("fallback remains initialized");
}
