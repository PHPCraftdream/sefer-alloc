#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};

#[test]
fn paused_owner_fanin_retires_every_publication_exactly_once() {
    let heap_ptr = HeapRegistry::claim();
    assert!(!heap_ptr.is_null());
    // SAFETY: this thread holds exclusive ownership until recycle.
    let heap = unsafe { &mut *heap_ptr };
    let layout = Layout::from_size_align(64, 16).unwrap();
    // Larger than both retired ring tiers: descriptor publication has no queue cap.
    let blocks: Vec<_> = (0..4096)
        .map(|_| heap.alloc(layout).expose_provenance())
        .collect();
    assert!(blocks.iter().all(|address| *address != 0));
    let anchor = heap.alloc(layout);
    assert!(!anchor.is_null());
    let before = heap.dbg_live_count_for(anchor).unwrap();
    let base = heap.dbg_segment_base_of_ptr(anchor);
    assert!(blocks.iter().all(|address| heap
        .dbg_segment_base_of_ptr(std::ptr::without_provenance_mut(*address))
        == base));
    let handles: Vec<_> = blocks
        .chunks(256)
        .map(|chunk| {
            let chunk = chunk.to_vec();
            std::thread::spawn(move || {
                let allocator = SeferAlloc::new();
                for address in chunk {
                    let ptr = std::ptr::with_exposed_provenance_mut(address);
                    // SAFETY: the owner uniquely transferred these current allocations
                    // and the producer frees each once with its original Layout.
                    unsafe {
                        allocator.dealloc(ptr, layout);
                    }
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(heap.dbg_live_count_for(anchor), Some(before));
    assert_eq!(heap.dbg_drain_sidecar_ingress(), blocks.len());
    assert_eq!(
        heap.dbg_live_count_for(anchor),
        Some(before - blocks.len() as u32)
    );
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    assert_eq!(
        heap.dbg_live_count_for(anchor),
        Some(before - blocks.len() as u32)
    );
    // SAFETY: anchor is the only user allocation not terminally published.
    unsafe {
        heap.dealloc(anchor, layout);
        HeapRegistry::recycle(heap_ptr);
    }
}
