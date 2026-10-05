#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::registry::{HeapCore, HeapRegistry};
use sefer_alloc::SegmentLayout;
use std::sync::{Arc, Barrier};

fn miss_and_empty(heap: &mut HeapCore, class: usize, out: &mut Vec<*mut u8>) {
    let layout = Layout::from_size_align(64, 8).unwrap();
    assert_eq!(heap.dbg_tcache_count(class), 0);
    let first = heap.alloc(layout);
    assert!(!first.is_null());
    out.push(first);
    let retained = heap.dbg_tcache_count(class);
    assert!(retained > 0);
    for _ in 0..retained {
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null());
        out.push(ptr);
    }
    assert_eq!(heap.dbg_tcache_count(class), 0);
}

#[test]
fn far_large_publication_after_clean_pass_retires_with_pin_held() {
    let large_layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let small_layout = Layout::from_size_align(64, 8).unwrap();
    let class = SegmentLayout::class_for(64, 8).unwrap();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    // SAFETY-free core access: the lease exclusively owns the slot until drop.
    let heap = lease.core();
    let mut large = Vec::new();
    for _ in 0..9 {
        let ptr = heap.alloc(large_layout);
        assert!(!ptr.is_null());
        large.push(ptr);
    }
    assert_eq!(heap.dbg_table_count(), 10);
    let victim = large.pop().unwrap();
    assert_eq!(heap.dbg_active_kind_census(), (1, 9, true));
    let mut small = Vec::new();

    let ready = Arc::new(Barrier::new(2));
    let publish = Arc::new(Barrier::new(2));
    let published = Arc::new(Barrier::new(2));
    let release_pin = Arc::new(Barrier::new(2));
    let address = victim.expose_provenance();
    let producer = {
        let ready = ready.clone();
        let publish = publish.clone();
        let published = published.clone();
        let release_pin = release_pin.clone();
        std::thread::spawn(move || {
            let held = RouteDirectory::global()
                .lookup(core::ptr::with_exposed_provenance_mut(address))
                .unwrap();
            let pin = RouteDirectory::global()
                .lookup(core::ptr::with_exposed_provenance_mut(address))
                .unwrap();
            ready.wait();
            publish.wait();
            // SAFETY: victim is a single live Large issue transferred once.
            assert!(unsafe { pin.publish_large() });
            published.wait();
            release_pin.wait();
            drop(held);
        })
    };
    ready.wait();
    miss_and_empty(heap, class, &mut small);
    assert_eq!(heap.dbg_active_kind_census(), (1, 9, true));
    publish.wait();
    published.wait();
    let probes_before = HeapCore::dbg_large_sidecar_slot_inspections();
    let rescues_before = HeapCore::dbg_large_sidecar_full_rescues();
    for pass in 0..2 {
        miss_and_empty(heap, class, &mut small);
        assert_eq!(
            heap.dbg_active_kind_census().1,
            if pass == 0 { 9 } else { 8 }
        );
    }
    assert_eq!(heap.dbg_active_kind_census(), (1, 8, true));
    assert!(RouteDirectory::global().lookup(victim).is_none());
    assert_eq!(HeapCore::dbg_large_sidecar_full_rescues(), rescues_before);
    assert!(HeapCore::dbg_large_sidecar_slot_inspections() - probes_before <= 8);
    release_pin.wait();
    producer.join().unwrap();
    // SAFETY: victim was transferred to the producer; the remainder is live.
    unsafe {
        for ptr in small {
            heap.dealloc(ptr, small_layout);
        }
        for ptr in large {
            heap.dealloc(ptr, large_layout);
        }
    }
    drop(lease);
}
