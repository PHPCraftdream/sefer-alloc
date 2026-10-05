#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::AllocCore;
use std::alloc::Layout;

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
    assert_eq!(
        reused, victim,
        "actual free-list miss must discover the terminal free"
    );
    assert!(!core.dbg_is_free_for(reused));
    // SAFETY: all pointers except victim are still issued exactly once; reused
    // is victim's new allocation instance after its terminal free was consumed.
    unsafe {
        for &ptr in &issued[1..] {
            core.dealloc(ptr, layout);
        }
        core.dealloc(reused, layout);
    }
}

#[cfg(feature = "fastbin")]
#[test]
fn magazine_hit_does_not_sweep_sidecar_words() {
    use sefer_alloc::registry::HeapRegistry;
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = Layout::from_size_align(64, 16).unwrap();
    let victim = heap.alloc(layout);
    assert!(!victim.is_null());
    assert!(heap.dbg_tcache_count(heap.dbg_class_for(layout).unwrap()) > 0);
    // SAFETY: victim transfers unique ownership exactly once to its descriptor.
    assert!(unsafe { heap.dbg_publish_small_sidecar_free(victim) });
    let hit = heap.alloc(layout);
    assert!(!hit.is_null());
    assert_ne!(hit, victim);
    assert!(
        !heap.dbg_is_free_for(victim),
        "magazine hit must not consume pending records"
    );
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
    // SAFETY: hit is the only remaining issued user allocation.
    unsafe {
        heap.dealloc(hit, layout);
    }
    drop(lease);
}

#[cfg(feature = "fastbin")]
#[test]
fn magazine_refill_miss_consumes_terminal_small_once() {
    use sefer_alloc::registry::HeapRegistry;

    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = Layout::from_size_align(64, 16).unwrap();
    let class = heap.dbg_class_for(layout).unwrap();
    let want = heap.dbg_refill_n_for_class(class);
    assert!(want >= 2);

    let victim = heap.alloc(layout);
    assert!(!victim.is_null());
    let mut first_refill = vec![victim];
    for _ in 1..want {
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null());
        first_refill.push(ptr);
    }
    assert_eq!(heap.dbg_tcache_count(class), 0);

    // SAFETY: victim is uniquely issued and this is its only free transfer.
    assert!(unsafe { heap.dbg_publish_small_sidecar_free(victim) });
    assert!(!heap.dbg_is_free_for(victim));

    let mut next_refill = Vec::with_capacity(want);
    for _ in 0..want {
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null());
        next_refill.push(ptr);
    }
    assert_eq!(
        next_refill.iter().filter(|&&ptr| ptr == victim).count(),
        1,
        "a real refill miss must consume and reissue the terminal Small block once"
    );
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    for (i, &ptr) in next_refill.iter().enumerate() {
        assert!(!first_refill[1..].contains(&ptr));
        assert!(!next_refill[..i].contains(&ptr));
    }

    // SAFETY: the first victim instance was retired; all remaining pointers
    // are distinct current allocations and each is freed once.
    unsafe {
        for &ptr in &first_refill[1..] {
            heap.dealloc(ptr, layout);
        }
        for &ptr in &next_refill {
            heap.dealloc(ptr, layout);
        }
    }
    drop(lease);
}

#[cfg(all(feature = "fastbin", feature = "virgin-zero-skip"))]
#[test]
fn zeroed_only_refill_consumes_terminal_small_and_clears_reused_bytes() {
    use sefer_alloc::registry::HeapRegistry;

    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = Layout::from_size_align(64, 16).unwrap();
    let class = heap.dbg_class_for(layout).unwrap();
    let want = heap.dbg_refill_n_for_class(class);
    assert!(want >= 2);

    let victim = heap.alloc_zeroed(layout);
    assert!(!victim.is_null());
    // SAFETY: victim is a current unique allocation with at least one byte.
    unsafe { victim.write(0xA5) };
    let mut first_refill = vec![victim];
    for _ in 1..want {
        let ptr = heap.alloc_zeroed(layout);
        assert!(!ptr.is_null());
        first_refill.push(ptr);
    }
    assert_eq!(heap.dbg_tcache_count(class), 0);

    // SAFETY: victim is still uniquely issued; this is its sole free transfer.
    assert!(unsafe { heap.dbg_publish_small_sidecar_free(victim) });
    let mut next_refill = Vec::with_capacity(want);
    for _ in 0..want {
        let ptr = heap.alloc_zeroed(layout);
        assert!(!ptr.is_null());
        next_refill.push(ptr);
    }
    let reused: Vec<_> = next_refill
        .iter()
        .copied()
        .filter(|ptr| ptr.addr() == victim.addr())
        .collect();
    assert_eq!(reused.len(), 1);
    // SAFETY: reused[0] is the newly issued allocation; calloc must clear it.
    assert_eq!(unsafe { reused[0].read() }, 0);
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);

    // SAFETY: the first victim instance was retired; all other allocations
    // remain uniquely issued and are freed exactly once here.
    unsafe {
        for &ptr in &first_refill[1..] {
            heap.dealloc(ptr, layout);
        }
        for &ptr in &next_refill {
            heap.dealloc(ptr, layout);
        }
    }
    drop(lease);
}

#[cfg(feature = "alloc-decommit")]
#[test]
fn zeroed_large_cold_path_consumes_terminal_obligation_before_cache_reuse() {
    use sefer_alloc::registry::segment_route::RouteDirectory;
    use sefer_alloc::registry::HeapRegistry;
    use sefer_alloc::SegmentLayout;

    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = Layout::from_size_align(
        SegmentLayout::SMALL_MAX + SegmentLayout::PAGE,
        SegmentLayout::PAGE,
    )
    .unwrap();
    let first = heap.alloc_zeroed(layout);
    assert!(!first.is_null());
    // SAFETY: first is uniquely issued and has at least one byte.
    unsafe { first.write(0xA5) };
    let pin = RouteDirectory::global()
        .lookup(first)
        .expect("current Large route");
    // SAFETY: the pin transfers this unique Large allocation exactly once.
    assert!(unsafe { pin.publish_large() });

    let second = heap.alloc_zeroed(layout);
    assert_eq!(
        second, first,
        "cold allocation should reclaim and reuse the cached span"
    );
    // SAFETY: second is the new unique allocation; cached bytes must be cleared.
    assert_eq!(unsafe { second.read() }, 0);
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    // SAFETY: second is the only live Large instance and is freed once.
    unsafe {
        heap.dealloc(second, layout);
    }
    drop(lease);
}
