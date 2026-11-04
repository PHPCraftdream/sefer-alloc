#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]
#![allow(unused_unsafe)] // The RoutePin API is becoming unsafe in the parent branch.

use core::alloc::Layout;

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, RouteRegistration};
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::SegmentLayout;

fn publish_small(ptr: *mut u8) {
    let pin = RouteDirectory::global().lookup(ptr).expect("issued route");
    assert!(matches!(
        pin.kind(),
        RouteKind::Small | RouteKind::Primordial
    ));
    let offset = (ptr.addr() & (SegmentLayout::SEGMENT - 1)) as u32;
    // SAFETY: ptr is one live allocation issued by this heap. This call is
    // its only ownership transfer; no old dealloc runs for ptr afterward.
    assert!(unsafe { pin.publish_small(offset) });
}

// Tests share the process-wide route directory and allocator; a block freed by
// one test can be reissued to another at the same address. Serialize them.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[test]
fn requested_sizes_one_through_seven_retire_once_and_reissue() {
    let _guard = serialize();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layouts: [_; 7] = core::array::from_fn(|i| Layout::from_size_align(i + 1, 16).unwrap());
    let ptrs = layouts.map(|layout| {
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null());
        ptr
    });
    #[cfg(feature = "alloc-decommit")]
    let before = heap.dbg_live_count_for(ptrs[0]).unwrap();
    for ptr in ptrs {
        publish_small(ptr);
    }
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 7);
    for ptr in ptrs {
        assert!(heap.dbg_is_free_for(ptr));
    }
    #[cfg(feature = "alloc-decommit")]
    assert_eq!(heap.dbg_live_count_for(ptrs[0]), Some(before - 7));
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    #[cfg(feature = "alloc-decommit")]
    assert_eq!(heap.dbg_live_count_for(ptrs[0]), Some(before - 7));

    let reissued = heap.alloc(layouts[0]);
    assert!(!reissued.is_null());
    assert!(!heap.dbg_is_free_for(reissued));
    publish_small(reissued);
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
    assert!(heap.dbg_is_free_for(reissued));
    // SAFETY: all issued allocations were retired by the owner-side drain.
    drop(lease);
}

#[test]
fn mixed_class_and_more_than_ring_capacity() {
    let _guard = serialize();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let sizes = [1, 24, 64, 128, 256, 512, 1024];
    let mut ptrs = Vec::new();
    for &size in &sizes {
        let ptr = heap.alloc(Layout::from_size_align(size, 16).unwrap());
        assert!(!ptr.is_null());
        ptrs.push(ptr);
    }
    #[cfg(feature = "alloc-decommit")]
    let before = heap.dbg_live_count_for(ptrs[0]).unwrap();
    for ptr in &ptrs {
        publish_small(*ptr);
    }
    assert_eq!(heap.dbg_drain_sidecar_ingress(), sizes.len());
    for ptr in &ptrs {
        assert!(heap.dbg_is_free_for(*ptr));
    }
    #[cfg(feature = "alloc-decommit")]
    assert_eq!(
        heap.dbg_live_count_for(ptrs[0]),
        Some(before - sizes.len() as u32)
    );

    let layout = Layout::from_size_align(16, 16).unwrap();
    let mut burst = Vec::new();
    for _ in 0..257 {
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null());
        burst.push(ptr);
    }
    for ptr in burst {
        publish_small(ptr);
    }
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 257);
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    // SAFETY: all issued allocations were retired by the owner-side drain.
    drop(lease);
}

#[test]
fn large_pending_claim_reclaims_and_cache_reissues() {
    let _guard = serialize();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = Layout::from_size_align(
        SegmentLayout::SMALL_MAX + SegmentLayout::PAGE,
        SegmentLayout::PAGE,
    )
    .unwrap();
    let first = heap.alloc(layout);
    assert!(!first.is_null());
    let old = RouteDirectory::global().lookup(first).unwrap();
    assert_eq!(old.kind(), RouteKind::Large);
    let incarnation = old.incarnation();
    // SAFETY: first is this heap's unique live Large allocation; this is its
    // only free transfer, and no old dealloc runs for it afterward.
    assert!(unsafe {
        RouteDirectory::global()
            .lookup(first)
            .unwrap()
            .publish_large()
    });
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    assert!(RouteDirectory::global().lookup(first).is_none());
    assert_eq!(old.incarnation(), incarnation);
    drop(old);

    let second = heap.alloc(layout);
    assert!(!second.is_null());
    #[cfg(feature = "alloc-decommit")]
    assert_eq!(second, first, "same-size Large cache reuse");
    let next = RouteDirectory::global().lookup(second).unwrap();
    assert_ne!(next.incarnation(), incarnation);
    // SAFETY: second is the new unique live Large instance; it is transferred
    // exactly once here and is never sent to the old dealloc path.
    assert!(unsafe { next.publish_large() });
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
    // SAFETY: both Large instances were retired by the owner-side drain.
    drop(lease);
}

#[cfg(feature = "alloc-decommit")]
#[test]
fn last_small_node_finalizes_after_route_scan() {
    let _guard = serialize();
    use sefer_alloc::{LargeCacheConfig, SmallSegmentPoolConfig};

    let config = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0));
    let mut lease = HeapRegistry::dbg_claim_lease_with_config(config).expect("claim_with_config");
    let heap = lease.core();
    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 16).unwrap();
    let mut ptrs = Vec::new();
    let mut first_small = None;
    let mut second_small = None;
    for _ in 0..(3 * SegmentLayout::SEGMENT / SegmentLayout::SMALL_MAX + 256) {
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null());
        let pin = RouteDirectory::global().lookup(ptr).unwrap();
        if pin.kind() == RouteKind::Small {
            let base = ptr.addr() & !(SegmentLayout::SEGMENT - 1);
            match first_small {
                None => first_small = Some(base),
                Some(first) if first != base => second_small = Some(base),
                _ => {}
            }
        }
        ptrs.push(ptr);
        if second_small.is_some() {
            break;
        }
    }
    let first = first_small.expect("first Small segment");
    assert!(
        second_small.is_some(),
        "first Small must cease being current"
    );
    let first_ptrs: Vec<_> = ptrs
        .iter()
        .copied()
        .filter(|ptr| ptr.addr() & !(SegmentLayout::SEGMENT - 1) == first)
        .collect();
    let old_pin = RouteDirectory::global().lookup(first_ptrs[0]).unwrap();
    let old_incarnation = old_pin.incarnation();
    for ptr in &first_ptrs {
        publish_small(*ptr);
    }
    assert_eq!(heap.dbg_drain_sidecar_ingress(), first_ptrs.len());
    heap.dbg_drain_small_pool();
    assert!(RouteDirectory::global().lookup(first_ptrs[0]).is_none());
    assert_eq!(old_pin.incarnation(), old_incarnation);
    drop(old_pin);

    for ptr in ptrs {
        if ptr.addr() & !(SegmentLayout::SEGMENT - 1) != first {
            publish_small(ptr);
        }
    }
    let _ = heap.dbg_drain_sidecar_ingress();
    // SAFETY: all issued allocations were retired by the owner-side drain.
    drop(lease);
}

#[test]
fn duplicate_detached_record_does_not_retire_another_credit() {
    let _guard = serialize();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let ptr = heap.alloc(Layout::from_size_align(16, 16).unwrap());
    assert!(!ptr.is_null());
    let class = RouteRegistration::class_at_global_address_for_test(ptr.addr()).unwrap();
    #[cfg(feature = "alloc-decommit")]
    let before = heap.dbg_live_count_for(ptr).unwrap();
    // SAFETY: the synthetic record transfers this one live allocation. This
    // probe does not release its reservation, so a stale-record probe below
    // still has valid geometry and cannot overlap reissue.
    assert!(unsafe { heap.dbg_reclaim_sidecar_record_for_test(ptr, class) });
    assert!(heap.dbg_is_free_for(ptr));
    // SAFETY: same still-mapped physical block and issued class, now free.
    assert!(!unsafe { heap.dbg_reclaim_sidecar_record_for_test(ptr, class) });
    #[cfg(feature = "alloc-decommit")]
    assert_eq!(heap.dbg_live_count_for(ptr), Some(before - 1));
    // SAFETY: no user allocation remains live.
    drop(lease);
}

#[cfg(feature = "fastbin")]
#[test]
fn detached_magazine_record_preserves_the_canonical_copy() {
    let _guard = serialize();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = Layout::from_size_align(16, 16).unwrap();
    let ptr = heap.alloc(layout);
    assert!(!ptr.is_null());
    let class = RouteRegistration::class_at_global_address_for_test(ptr.addr()).unwrap();
    // SAFETY: the sole live allocation is returned once through the local path.
    unsafe { heap.dealloc(ptr, layout) };
    assert!(
        !heap.dbg_is_free_for(ptr),
        "the local free is magazine-held"
    );
    let parked = heap.dbg_tcache_count(usize::from(class));
    #[cfg(feature = "alloc-decommit")]
    let before = heap.dbg_live_count_for(ptr);
    // SAFETY: synthetic stale record for this still-mapped magazine block;
    // no producer publication or transfer of an already-freed instance.
    assert!(!unsafe { heap.dbg_reclaim_sidecar_record_for_test(ptr, class) });
    assert_eq!(heap.dbg_tcache_count(usize::from(class)), parked);
    #[cfg(feature = "alloc-decommit")]
    assert_eq!(heap.dbg_live_count_for(ptr), before);
    let reissued = heap.alloc(layout);
    assert_eq!(reissued, ptr, "the magazine copy remains canonical");
    publish_small(reissued);
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
    // SAFETY: the reissued allocation was retired by the owner.
    drop(lease);
}

#[test]
fn public_trim_consumes_small_and_large_terminal_publications() {
    let _guard = serialize();
    use sefer_alloc::SeferAlloc;
    use std::alloc::GlobalAlloc;

    let allocator = SeferAlloc::new();
    let layouts = [1, 7, 24].map(|size| Layout::from_size_align(size, 16).unwrap());
    let ptrs = layouts.map(|layout| {
        // SAFETY: valid nonzero layout; this test retains each returned block.
        let ptr = unsafe { allocator.alloc(layout) };
        assert!(!ptr.is_null());
        ptr
    });
    let large_layout = Layout::from_size_align(
        SegmentLayout::SMALL_MAX + SegmentLayout::PAGE,
        SegmentLayout::PAGE,
    )
    .unwrap();
    // SAFETY: valid nonzero layout; the returned instance is retained below.
    let large = unsafe { allocator.alloc(large_layout) };
    assert!(!large.is_null());
    let old_pin = RouteDirectory::global().lookup(large).unwrap();
    for ptr in ptrs {
        publish_small(ptr);
    }
    // SAFETY: this pin transfers the test's unique current Large instance.
    assert!(unsafe {
        RouteDirectory::global()
            .lookup(large)
            .unwrap()
            .publish_large()
    });
    allocator.trim_current_thread();
    assert!(RouteDirectory::global().lookup(large).is_none());
    let heap_ptr = sefer_alloc::global::tls_heap::current_for_trim().unwrap();
    for ptr in ptrs {
        if RouteDirectory::global().lookup(ptr).is_some() {
            // SAFETY: this thread still owns the bound heap; ptr is only an
            // address key for the diagnostic, not a user-memory access.
            assert!(unsafe { (*heap_ptr).dbg_is_free_for(ptr) });
        }
    }
    drop(old_pin); // Descriptor cleanup remains valid after reservation release.
}

#[test]
fn tls_exit_runs_the_owner_sweep_before_recycle() {
    let _guard = serialize();
    let address = std::thread::spawn(|| {
        use std::alloc::GlobalAlloc;
        let allocator = sefer_alloc::SeferAlloc::new();
        let layout = Layout::from_size_align(
            SegmentLayout::SMALL_MAX + SegmentLayout::PAGE,
            SegmentLayout::PAGE,
        )
        .unwrap();
        // SAFETY: valid layout; the thread uniquely owns the returned instance.
        let ptr = unsafe { allocator.alloc(layout) };
        assert!(!ptr.is_null());
        let pin = RouteDirectory::global().lookup(ptr).unwrap();
        // SAFETY: the only ownership transfer of this live Large allocation.
        assert!(unsafe { pin.publish_large() });
        ptr.addr()
    })
    .join()
    .unwrap();
    let key = core::ptr::without_provenance_mut::<u8>(address);
    assert!(RouteDirectory::global().lookup(key).is_none());
}
