#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]
use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::AtomicPtr;
use sefer_alloc::registry::segment_route::{RouteDirectory, RoutePin};
use sefer_alloc::{SeferAlloc, SegmentLayout};

fn assert_large_route_retired(ptr: *mut u8, old: &RoutePin) {
    assert!(
        !old.pending_for_test(ptr),
        "old terminal obligation was consumed"
    );
    assert!(
        RouteDirectory::global()
            .lookup(ptr)
            .is_none_or(|current| current.incarnation() != old.incarnation()),
        "the released Large incarnation must no longer be registered"
    );
}

#[test]
fn globalalloc_foreign_terminal_free_is_visible_to_strict_trim() {
    std::thread::spawn(|| {
        let allocator = SeferAlloc::new();
        for (size, align) in [
            (1, 16),
            (7, 16),
            (37, SegmentLayout::SEGMENT),
            (513, 16 * SegmentLayout::SEGMENT),
        ] {
            let layout = Layout::from_size_align(size, align).unwrap();
            // SAFETY: matching serviceable Layout; null is checked before use.
            let ptr = unsafe { allocator.alloc(layout) };
            assert!(!ptr.is_null());
            // SAFETY: one initialized byte lies within the live allocation.
            // A narrow reborrow must not authorize any metadata/slack access.
            let narrow = unsafe {
                ptr.write(0);
                &mut *ptr
            } as *mut u8;
            let transfer = AtomicPtr::new(narrow);
            let retained = RouteDirectory::global().lookup(ptr).unwrap();
            std::thread::spawn(move || {
                // SAFETY: ownership and pointer provenance move into this thread.
                // This is the allocation's only free.
                unsafe { SeferAlloc::new().dealloc(transfer.into_inner(), layout) };
            })
            .join()
            .unwrap();
            assert!(
                retained.pending_for_test(ptr),
                "actual dealloc published terminal obligation"
            );
            allocator.trim_current_thread();
            assert!(
                !retained.pending_for_test(ptr),
                "strict trim consumed the completed publication"
            );
            if align >= SegmentLayout::SEGMENT {
                assert_large_route_retired(ptr, &retained);
            }
            // A retained descriptor remains independently valid after release.
            assert!(retained.incarnation() > 0);
        }
    })
    .join()
    .unwrap();
}

#[test]
fn foreign_overaligned_realloc_preserves_prefix_and_failure_source() {
    let layout = Layout::from_size_align(37, 2 * SegmentLayout::SEGMENT).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let owner = std::thread::spawn(move || {
        let allocator = SeferAlloc::new();
        // SAFETY: exact requested Layout; null checked before access.
        let ptr = unsafe { allocator.alloc(layout) };
        assert!(!ptr.is_null());
        // SAFETY: uniquely owned live 37-byte allocation.
        unsafe { ptr.write_bytes(0x6d, 37) };
        tx.send(AtomicPtr::new(ptr)).unwrap();
        // Keep this heap OWNED so the receiver cannot reclaim the same slot
        // and accidentally turn the foreign realloc scenario into an own free.
        done_rx.recv().unwrap();
    });
    let transfer = rx.recv().unwrap();
    let ptr = transfer.into_inner();
    let source_route = RouteDirectory::global().lookup(ptr).unwrap();
    let allocator = SeferAlloc::new();
    // SAFETY: exact source Layout, unique live source; invalid new size must fail.
    assert!(unsafe { allocator.realloc(ptr, layout, usize::MAX) }.is_null());
    // SAFETY: realloc failure preserves the original allocation.
    unsafe { assert_eq!(*ptr, 0x6d) };
    // SAFETY: current source and exact Layout; successful resize consumes source.
    let grown = unsafe { allocator.realloc(ptr, layout, 8193) };
    assert!(!grown.is_null());
    assert_eq!(grown.addr() % layout.align(), 0);
    let destination_route = RouteDirectory::global().lookup(grown).unwrap();
    assert_ne!(
        source_route.owner(),
        destination_route.owner(),
        "genuinely foreign heaps"
    );
    assert!(
        source_route.pending_for_test(ptr),
        "foreign realloc publishes the old allocation"
    );
    // SAFETY: successful realloc preserves the entire initialized prefix.
    unsafe {
        for i in 0..37 {
            assert_eq!(grown.add(i).read(), 0x6d);
        }
        allocator.dealloc(
            grown,
            Layout::from_size_align(8193, layout.align()).unwrap(),
        );
    }
    assert_large_route_retired(grown, &destination_route);
    done_tx.send(()).unwrap();
    owner.join().unwrap();
    assert_large_route_retired(ptr, &source_route);
}

#[test]
fn paused_foreign_producer_does_not_block_strict_trim() {
    std::thread::spawn(|| {
        let allocator = SeferAlloc::new();
        for align in [16, SegmentLayout::SEGMENT] {
            let layout = Layout::from_size_align(1, align).unwrap();
            // SAFETY: Layout is serviceable; result checked before handoff.
            let ptr = unsafe { allocator.alloc(layout) };
            assert!(!ptr.is_null());
            let transfer = AtomicPtr::new(ptr);
            let retained = RouteDirectory::global().lookup(ptr).unwrap();
            let (ready_tx, ready_rx) = std::sync::mpsc::channel();
            let (resume_tx, resume_rx) = std::sync::mpsc::channel();
            let producer = std::thread::spawn(move || {
                let ptr = transfer.into_inner();
                let pin = RouteDirectory::global().lookup(ptr).unwrap();
                ready_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
                // SAFETY: unique live allocation has remained unpublished and
                // credited while paused; GlobalAlloc performs its terminal free.
                unsafe { SeferAlloc::new().dealloc(ptr, layout) };
                // Descriptor is independently pinned, with no reservation access.
                drop(pin);
            });
            ready_rx.recv().unwrap();
            allocator.trim_current_thread();
            assert!(!retained.pending_for_test(ptr));
            assert_eq!(
                RouteDirectory::global().lookup(ptr).unwrap().incarnation(),
                retained.incarnation(),
                "unpublished credit preserves the same route"
            );
            resume_tx.send(()).unwrap();
            producer.join().unwrap();
            assert!(retained.pending_for_test(ptr));
            allocator.trim_current_thread();
            assert!(!retained.pending_for_test(ptr));
        }
    })
    .join()
    .unwrap();
}

#[test]
fn overalign_registration_failure_rolls_back_without_consuming_source() {
    std::thread::spawn(|| {
        let allocator = SeferAlloc::new();
        let layout = Layout::from_size_align(37, SegmentLayout::SEGMENT).unwrap();
        // SAFETY: matching Layout; result checked before payload access.
        let source = unsafe { allocator.alloc(layout) };
        assert!(!source.is_null());
        // SAFETY: initialized live source belongs uniquely to this thread.
        unsafe { source.write_bytes(0x3e, 37) };
        let source_route = RouteDirectory::global().lookup(source).unwrap();
        RouteDirectory::fail_next_registration_for_test();
        // SAFETY: valid source; this grow cannot fit any initial useful window.
        // Failed destination registration must leave the original live.
        let result = unsafe { allocator.realloc(source, layout, 5 * SegmentLayout::SEGMENT) };
        assert!(result.is_null());
        assert_eq!(
            RouteDirectory::global()
                .lookup(source)
                .unwrap()
                .incarnation(),
            source_route.incarnation(),
            "failed realloc preserves the original route"
        );
        assert!(!source_route.pending_for_test(source));
        // SAFETY: failed realloc preserves all initialized source bytes.
        unsafe {
            for i in 0..37 {
                assert_eq!(source.add(i).read(), 0x3e);
            }
            allocator.dealloc(source, layout);
        }
    })
    .join()
    .unwrap();
}

#[cfg(feature = "batch-api")]
#[test]
fn foreign_batch_terminal_free_uses_actual_payload_routes() {
    std::thread::spawn(|| {
        let allocator = SeferAlloc::new();
        for align in [16, 2 * SegmentLayout::SEGMENT] {
            let layout = Layout::from_size_align(1, align).unwrap();
            let mut blocks = [core::ptr::null_mut(); 3];
            // SAFETY: serviceable nonzero Layout and output slots; all returned
            // allocations will be uniquely transferred to the foreign batch.
            let count = unsafe { allocator.alloc_batch(layout, &mut blocks) };
            assert_eq!(count, blocks.len());
            let retained = blocks.map(|ptr| RouteDirectory::global().lookup(ptr).unwrap());
            let transfers = blocks.map(AtomicPtr::new);
            std::thread::spawn(move || {
                let blocks = transfers.map(AtomicPtr::into_inner);
                // SAFETY: each live issued instance is transferred exactly once.
                unsafe { SeferAlloc::new().dealloc_batch(layout, &blocks) };
            })
            .join()
            .unwrap();
            for (pin, ptr) in retained.iter().zip(blocks) {
                assert!(pin.pending_for_test(ptr));
            }
            allocator.trim_current_thread();
            for (pin, ptr) in retained.iter().zip(blocks) {
                assert!(!pin.pending_for_test(ptr));
            }
            if align >= SegmentLayout::SEGMENT {
                for (pin, ptr) in retained.iter().zip(blocks) {
                    assert_large_route_retired(ptr, pin);
                }
            }
            // The own batch dispatch must also resolve biased canonical roots.
            // SAFETY: same valid Layout; output overwritten with fresh instances.
            assert_eq!(
                unsafe { allocator.alloc_batch(layout, &mut blocks) },
                blocks.len()
            );
            let own_retained = blocks.map(|ptr| RouteDirectory::global().lookup(ptr).unwrap());
            // SAFETY: every returned instance is live and freed once on its owner.
            unsafe { allocator.dealloc_batch(layout, &blocks) };
            if align >= SegmentLayout::SEGMENT {
                for (pin, ptr) in own_retained.iter().zip(blocks) {
                    assert_large_route_retired(ptr, pin);
                }
            }
        }
    })
    .join()
    .unwrap();
}

#[cfg(all(feature = "alloc-decommit", not(miri)))]
#[test]
fn fallback_direct_free_before_service_and_busy_terminal_publication() {
    // Fallback policy is first-initialization-wins, and service state is
    // process-global. A fresh child proves the requested zero budget really
    // governs this scenario without interference from other test threads.
    if std::env::var_os("SEFER_R8_FALLBACK_CHILD").is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "fallback_direct_free_before_service_and_busy_terminal_publication",
            ])
            .env("SEFER_R8_FALLBACK_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated real fallback free scenario");
        return;
    }
    use sefer_alloc::global::tls_heap;
    use sefer_alloc::registry::HeapCore;
    use sefer_alloc::{AllocCore, LargeCacheConfig};
    assert!(!SeferAlloc::maintenance_running());
    let allocator = SeferAlloc::with_config(LargeCacheConfig::new().budget_bytes(0));
    let saved = tls_heap::dbg_mark_local_torn_for_test();
    let layout = Layout::from_size_align(
        SegmentLayout::SMALL_MAX + SegmentLayout::PAGE,
        SegmentLayout::PAGE,
    )
    .unwrap();
    // SAFETY: TORN sends this valid Layout through the real fallback alloc path.
    let direct = unsafe { allocator.alloc(layout) };
    assert!(!direct.is_null());
    HeapCore::dbg_with_fallback_for_test(|heap| {
        assert_eq!(
            heap.dbg_large_cache_budget(),
            Some(0),
            "resolved fallback policy"
        );
    })
    .unwrap();
    let direct_pin = RouteDirectory::global().lookup(direct).unwrap();
    let released_before = AllocCore::dbg_segments_released_total();
    // SAFETY: unique current fallback allocation, with the exact Layout.
    unsafe { allocator.dealloc(direct, layout) };
    assert!(
        RouteDirectory::global().lookup(direct).is_none(),
        "idle fallback reclaims synchronously without service or future alloc"
    );
    assert_eq!(
        AllocCore::dbg_segments_released_total(),
        released_before + 1
    );
    assert!(!direct_pin.pending_for_test(direct));
    assert!(!SeferAlloc::maintenance_running());

    // SAFETY: matching Layout; null is checked before transferring ownership.
    let busy = unsafe { allocator.alloc(layout) };
    assert!(!busy.is_null());
    let busy_pin = RouteDirectory::global().lookup(busy).unwrap();
    HeapCore::dbg_with_fallback_for_test(|_| {
        // SAFETY: unique issued instance. Reentrant free cannot borrow the
        // locked fallback core; it must terminal-publish through the pin.
        unsafe { allocator.dealloc(busy, layout) };
        assert!(busy_pin.pending_for_test(busy));
    })
    .unwrap();
    assert!(RouteDirectory::global().lookup(busy).is_some());
    assert!(busy_pin.pending_for_test(busy));
    SeferAlloc::start_maintenance().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while RouteDirectory::global().lookup(busy).is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "service did not reclaim idle fallback"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(!busy_pin.pending_for_test(busy));
    // SAFETY: saved TLS state belongs to this exact thread and was never recycled.
    unsafe { tls_heap::dbg_restore_local_for_test(saved) };
}

#[cfg(miri)]
#[test]
fn busy_fallback_publication_is_disjoint_from_owner_borrow() {
    use sefer_alloc::global::tls_heap;
    use sefer_alloc::registry::HeapCore;
    #[cfg(feature = "alloc-decommit")]
    let allocator = SeferAlloc::with_config(sefer_alloc::LargeCacheConfig::new().budget_bytes(0));
    #[cfg(not(feature = "alloc-decommit"))]
    let allocator = SeferAlloc::new();
    let saved = tls_heap::dbg_mark_local_torn_for_test();
    let layout = Layout::from_size_align(1, SegmentLayout::SEGMENT).unwrap();
    // SAFETY: serviceable Layout allocated through the actual fallback path.
    let ptr = unsafe { allocator.alloc(layout) };
    assert!(!ptr.is_null());
    let pin = RouteDirectory::global().lookup(ptr).unwrap();
    HeapCore::dbg_with_fallback_for_test(|heap| {
        // SAFETY: unique live allocation. This call occurs while `heap`'s
        // exclusive owner borrow is protected; only sidecar mutation is legal.
        unsafe { allocator.dealloc(ptr, layout) };
        assert!(pin.pending_for_test(ptr));
        assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
        assert!(!pin.pending_for_test(ptr));
    })
    .unwrap();
    assert_large_route_retired(ptr, &pin);
    // SAFETY: restoring exactly this thread's saved, unrecycled TLS state.
    unsafe { tls_heap::dbg_restore_local_for_test(saved) };
}
