//! R6-OPT-P0-1 — `SeferAlloc::dealloc` must NOT bind a registry slot (nor
//! take the fallback spinlock) for a thread whose FIRST EVER call into the
//! allocator is a `dealloc` of a foreign pointer.
//!
//! ## The defect this guards against
//!
//! Before this fix, `SeferAlloc::dealloc` unconditionally called
//! `self.current_heap()` (== `current_for_alloc`), which for a thread whose
//! TLS is null (never allocated anything of its own) called
//! `bind_slow_tagged()` -> `HeapRegistry::claim()` -> materialised a FULL
//! `HeapCore` (reserving/committing a 4 MiB primordial segment) JUST to free
//! one foreign pointer. The fix (`global::tls_heap::current_for_dealloc` +
//! `HeapCore::dealloc_foreign_routing`) routes such a dealloc directly
//! through the heap-instance-independent cross-thread routing tail, without
//! ever calling `HeapRegistry::claim`.
//!
//! ## Non-vacuous / counterfactual
//!
//! `heaps_claimed_high_water` is a monotonic, process-wide high-water mark of
//! every registry slot ever claimed (`HeapRegistry::claim`, via
//! `bump_count`) since process start — see its doc comment in
//! `registry::heap_registry`. This test snapshots it immediately BEFORE
//! spawning a fresh worker thread, has that worker's FIRST EVER allocator
//! call be a `dealloc` of a pointer produced by ANOTHER (already-bound)
//! thread, and asserts the high-water mark is UNCHANGED after the worker
//! joins. With the pre-fix `current_heap()` routing restored, this worker's
//! first `dealloc` would bind a slot (`bump_count` fires inside `claim`),
//! the high-water mark would increase by (at least) one, and the assertion
//! below would fail — this was verified by temporarily routing `dealloc`
//! back through `self.current_heap()` during development of this test (see
//! the R6-OPT-P0-1 task report).
//!
//! Per project convention: tests live in `tests/`, not inline.

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::heaps_claimed_high_water;
use sefer_alloc::SeferAlloc;

// Serialise: `heaps_claimed_high_water` is a process-global counter shared by
// every test binary running in this process; concurrent tests bumping it
// would make the "unchanged across our worker" assertion flaky.
static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

/// A fresh thread whose ONLY allocator call ever is a `dealloc` of a pointer
/// handed to it (by value, via a channel) from another, already-bound
/// thread, must not increase `heaps_claimed_high_water`.
#[test]
fn unbound_thread_dealloc_only_does_not_claim_a_slot() {
    let _serial = SerialGuard::acquire();

    let a = SeferAlloc::new();
    let layout = Layout::from_size_align(64, 8).unwrap();

    // Owner thread (this test's OS thread, driven directly through the
    // `SeferAlloc` value — not installed as `#[global_allocator]`, matching
    // `stats_reflects_activity.rs`'s established driving discipline) binds
    // its own heap and allocates the block the worker will free.
    // SAFETY: valid non-zero layout.
    let p = unsafe { a.alloc(layout) };
    assert!(!p.is_null(), "owner alloc returned null");
    let addr = p as usize;

    // Snapshot the high-water mark AFTER the owner's own bind (so the
    // owner's slot claim is not counted against the worker) and immediately
    // before spawning the worker.
    let before = heaps_claimed_high_water();

    let (tx, rx) = std::sync::mpsc::channel::<usize>();
    tx.send(addr).unwrap();

    let handle = std::thread::spawn(move || {
        // This worker's FIRST EVER call into the allocator (any allocator —
        // this thread has done nothing else) is the foreign free below.
        let addr = rx.recv().expect("owner did not send the pointer");
        let ptr = addr as *mut u8;
        let layout = Layout::from_size_align(64, 8).unwrap();
        // SAFETY: `ptr` was allocated by the owner thread with `layout` and
        // handed over by value (never touched again by the owner) — a sound
        // cross-thread free, and the exact "foreign free, never-before-bound
        // thread" shape this test targets.
        let a = SeferAlloc::new();
        unsafe { a.dealloc(ptr, layout) };
    });
    handle.join().expect("worker thread panicked");

    let after = heaps_claimed_high_water();
    assert_eq!(
        after, before,
        "a never-before-bound thread's dealloc-only first call must not \
         claim a registry slot (R6-OPT-P0-1): high-water before={before}, after={after}"
    );
}

/// Same shape, but the worker frees MANY foreign pointers (still never
/// allocating anything of its own) — proves the no-bind property holds for
/// more than a single call, not just the very first one.
#[test]
fn unbound_thread_many_dealloc_only_does_not_claim_a_slot() {
    let _serial = SerialGuard::acquire();

    let a = SeferAlloc::new();
    let layout = Layout::from_size_align(48, 8).unwrap();

    const N: usize = 32;
    let mut addrs = Vec::with_capacity(N);
    for _ in 0..N {
        // SAFETY: valid non-zero layout.
        let p = unsafe { a.alloc(layout) };
        assert!(!p.is_null(), "owner alloc returned null");
        addrs.push(p as usize);
    }

    let before = heaps_claimed_high_water();

    let handle = std::thread::spawn(move || {
        let a = SeferAlloc::new();
        let layout = Layout::from_size_align(48, 8).unwrap();
        for addr in addrs {
            let ptr = addr as *mut u8;
            // SAFETY: each `ptr` was allocated by the owner thread with
            // `layout` and handed over by value; freed exactly once here.
            unsafe { a.dealloc(ptr, layout) };
        }
    });
    handle.join().expect("worker thread panicked");

    let after = heaps_claimed_high_water();
    assert_eq!(
        after, before,
        "a never-before-bound thread performing ONLY foreign frees must not \
         claim a registry slot at any point (R6-OPT-P0-1): before={before}, after={after}"
    );
}
