//! Reduced-width provenance oracle plus real deferred Large release coverage.

#![cfg(feature = "internals")]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reservation {
    address: u8,
    generation: u8,
}

// Four-bit addresses deliberately wrap; generation represents the allocation
// provenance that an address-only AtomicPtr CAS cannot compare.
fn address(raw: u8) -> u8 {
    raw & 0x0f
}

fn reused_address_link(use_stale_cas: bool) -> (Reservation, Reservation) {
    let old = Reservation {
        address: address(0x1a),
        generation: 1,
    };
    let mut head = Some(old);
    let sampled = head.unwrap();
    assert_eq!(head.take(), Some(old)); // owner pops and releases old
    let current = Reservation {
        address: address(0x2a),
        generation: 2,
    };
    assert_eq!(current.address, old.address);
    head = Some(current); // a new OS reservation at the same VA
    let new_node = Reservation {
        address: 0x0b,
        generation: 1,
    };
    let predecessor = if use_stale_cas {
        assert_eq!(head.unwrap().address, sampled.address); // address CAS succeeds
        head = Some(new_node);
        sampled
    } else {
        head.replace(new_node).unwrap() // actual swap return
    };
    assert_eq!(head, Some(new_node));
    (predecessor, current)
}

#[test]
fn reduced_width_swap_preserves_actual_predecessor_provenance() {
    let (predecessor, current) = reused_address_link(false);
    assert_eq!(predecessor, current);
    let (stale, current) = reused_address_link(true);
    assert_eq!(stale.address, current.address);
    assert_ne!(stale.generation, current.generation);
}

#[test]
fn reduced_width_consumer_waits_for_actual_link() {
    let (predecessor, current) = reused_address_link(false);
    let new_node = Reservation {
        address: 0x0b,
        generation: 1,
    };
    let mut head = Some(new_node);
    let mut ready_link = None;
    let try_pop =
        |head: &mut Option<Reservation>, ready_link: Option<Reservation>| head.replace(ready_link?);
    assert_eq!(
        try_pop(&mut head, ready_link),
        None,
        "owner must not pop before link publication"
    );
    assert_eq!(head, Some(new_node));
    ready_link = Some(predecessor); // Release publication in the real stack
    assert_eq!(try_pop(&mut head, ready_link), Some(new_node));
    assert_eq!(head, Some(current)); // generation 2, not stale generation 1
}

#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]
mod real_large {
    use sefer_alloc::registry::{bootstrap, HeapRegistry, DBG_LARGE_XTHREAD_RECLAIMED};
    use sefer_alloc::{AllocCore, LargeCacheConfig};
    use std::alloc::Layout;
    use std::sync::atomic::Ordering;

    struct SendPtr(*mut u8);
    // SAFETY: each live allocation is transferred once to one remote thread;
    // the owner does not access it again before that thread has joined.
    unsafe impl Send for SendPtr {}

    impl SendPtr {
        fn into_raw(self) -> *mut u8 {
            self.0
        }
    }

    #[test]
    fn remote_large_frees_release_with_zero_cache_budget() {
        let _ = bootstrap::ensure();
        let owner_ptr = HeapRegistry::claim_with_config(LargeCacheConfig::new().budget_bytes(0));
        assert!(!owner_ptr.is_null());
        // SAFETY: this test exclusively owns the claimed heap until recycle;
        // remote threads only publish frees through their own heap instances.
        let owner = unsafe { &mut *owner_ptr };
        assert_eq!(owner.dbg_large_cache_budget(), Some(0));
        let layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
        let first = owner.alloc(layout);
        let second = owner.alloc(layout);
        assert!(!first.is_null() && !second.is_null());
        assert_ne!(first, second);
        let before_reclaim = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);

        let remote = |ptr: SendPtr| {
            std::thread::spawn(move || {
                let remote_ptr = HeapRegistry::claim();
                assert!(!remote_ptr.is_null());
                // SAFETY: ptr came from one live owner allocation and
                // ownership was transferred uniquely to this thread. Layout
                // matches and each pointer is freed exactly once.
                unsafe { (*remote_ptr).dealloc(ptr.into_raw(), layout) };
                // SAFETY: this thread owns its claimed heap and is done using it.
                unsafe { HeapRegistry::recycle(remote_ptr) };
            })
        };
        let a = remote(SendPtr(first));
        let b = remote(SendPtr(second));
        a.join().unwrap();
        b.join().unwrap();

        let before_release = AllocCore::dbg_segments_released_total();
        let replacement = owner.alloc(layout); // owner drains both deferred nodes
        assert!(!replacement.is_null());
        assert_eq!(
            DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed) - before_reclaim,
            2
        );
        assert!(AllocCore::dbg_segments_released_total() - before_release >= 2);
        assert_eq!(owner.dbg_large_cache_used(), 0);

        // SAFETY: replacement remains live in this owner and is freed once.
        unsafe { owner.dealloc(replacement, layout) };
        // SAFETY: owner_ptr is still this thread's claimed heap.
        unsafe { HeapRegistry::recycle(owner_ptr) };
    }
}
