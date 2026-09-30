#![cfg(all(feature = "alloc-global", feature = "alloc-xthread", feature = "internals", feature = "bench-internals"))]

use std::alloc::Layout;
use sefer_alloc::AllocCore;

#[test]
fn small_free_list_miss_consumes_terminal_publication_without_explicit_sweep() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let layout = Layout::from_size_align(16, 16).unwrap();
    // The first carve fills the substrate's 31-block refill reserve.
    let issued: Vec<_> = (0..32).map(|_| core.alloc(layout)).collect();
    assert!(issued.iter().all(|ptr| !ptr.is_null()));
    let victim = issued[0];
    // SAFETY: victim is a current issued Small block; this is its sole free.
    assert!(unsafe { core.dbg_publish_small_sidecar_free(victim) });
    assert!(!core.dbg_is_free_for(victim));
    let reused = core.alloc(layout);
    assert_eq!(reused, victim, "actual free-list miss must discover the terminal free");
    assert!(!core.dbg_is_free_for(reused));
    // SAFETY: all pointers except victim are still issued exactly once; reused
    // is victim's new allocation instance after its terminal free was consumed.
    unsafe {
        for &ptr in &issued[1..] { core.dealloc(ptr, layout); }
        core.dealloc(reused, layout);
    }
}

#[cfg(feature = "fastbin")]
#[test]
fn magazine_hit_does_not_sweep_sidecar_words() {
    use sefer_alloc::registry::HeapRegistry;
    let heap_ptr = HeapRegistry::claim();
    assert!(!heap_ptr.is_null());
    // SAFETY: this thread exclusively owns the claimed core until recycle.
    let heap = unsafe { &mut *heap_ptr };
    let layout = Layout::from_size_align(64, 16).unwrap();
    let victim = heap.alloc(layout);
    assert!(!victim.is_null());
    assert!(heap.dbg_tcache_count(heap.dbg_class_for(layout).unwrap()) > 0);
    // SAFETY: victim transfers unique ownership exactly once to its descriptor.
    assert!(unsafe { heap.dbg_publish_small_sidecar_free(victim) });
    let hit = heap.alloc(layout);
    assert!(!hit.is_null());
    assert_ne!(hit, victim);
    assert!(!heap.dbg_is_free_for(victim), "magazine hit must not consume pending records");
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
    // SAFETY: hit is the only remaining issued user allocation.
    unsafe { heap.dealloc(hit, layout); HeapRegistry::recycle(heap_ptr); }
}
