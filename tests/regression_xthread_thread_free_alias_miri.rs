//! Regression guard for the H1 soundness finding
//! (`docs/reviews/2026-07-09-unsafe-soundness-review.md`).
//!
//! ## The hazard this test targets
//!
//! `HeapCore::thread_free` is (was) an INLINE `AtomicPtr<u8>` field of
//! `HeapCore`. On EVERY `alloc`/`dealloc` the owning thread materialises a
//! `&mut HeapCore` over the whole struct — including the `thread_free` bytes —
//! and that `&mut` is a *protected* `Unique` for the duration of the call
//! (`pub fn alloc(&mut self, …)`). Meanwhile a REMOTE thread that cross-thread
//! frees a Large segment owned by this heap CASes EXACTLY those same bytes,
//! reconstructing the pointer through EXPOSED provenance
//! (`Node::atomic_ptr_ref` → `with_exposed_provenance_mut` →
//! `compare_exchange`, see `deferred_large/push.rs`).
//!
//! Under Stacked/Tree Borrows (the model miri enforces) a foreign write that
//! lands inside the range of a live protected `&mut` is a protector violation
//! (UB) — the same class of conflict the project already paid to fix in W3 for
//! the diagnostic counters (there it was only a foreign *read*; here it is a
//! foreign *write*, a strictly stronger conflict).
//!
//! ## Why the EXISTING xthread miri-plain test does NOT catch this
//!
//! `regression_xthread_large_free_no_leak` is PHASE-SERIALISED: the owner
//! allocates, THEN a remote thread frees (owner passive), THEN the owner
//! allocates again. The owner's `&mut HeapCore` frame and the remote CAS never
//! overlap in time, so no protector is ever live when the remote writes.
//!
//! ## What THIS test does differently
//!
//! It runs the two frames CONCURRENTLY with a REAL overlap: an owner thread
//! spins a tight loop of SMALL allocations (each forms a fresh protected
//! `&mut HeapCore`; small allocs take the OPT-C magazine/cache-hit path that
//! does NOT re-expose `thread_free`), WHILE a remote thread frees a batch of
//! Large blocks owned by that heap — each free a wildcard CAS into the owner's
//! `thread_free`. Run under plain-provenance miri with an elevated
//! `-Zmiri-preemption-rate` (see the CI `miri-plain` job / `scripts/miri.mjs`),
//! the interpreter's scheduler will interleave a remote CAS inside a live
//! owner `alloc` frame — the exact schedule the review argues is UB.
//!
//! If miri reports a Stacked-Borrows / protector error here, H1 is confirmed
//! empirically and the fix (hoist the TFS head out of `HeapCore` into the
//! `Sync` `HeapSlot`, mirroring W3) is required. If miri stays green, the
//! schedule class is at least made VISIBLE and permanently guarded — the test
//! is kept in-tree either way.

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise against the other xthread tests in the same binary: the registry
// is a process-global static and concurrent claim/recycle churn from a sibling
// test could perturb this one. (Mirrors the SerialGuard in
// `regression_xthread_large_free_no_leak.rs`.)
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

#[test]
fn xthread_thread_free_write_overlaps_owner_alloc_mut() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // Large blocks the remote thread will free (each free = a wildcard CAS
    // into the OWNER heap's `thread_free`). Distinct bases so the double-push
    // guard does not collapse them into a single no-op — each contributes a
    // real CAS. Kept small (miri is ~1e5× slower than native; each Large alloc
    // reserves an OS segment).
    const LARGE_N: usize = 8;
    const LARGE_SIZE: usize = 512 * 1024; // > SMALL_MAX → routed to alloc_large
    let large_layout = Layout::from_size_align(LARGE_SIZE, 8).unwrap();

    // Small allocs the owner spins concurrently (OPT-C cache-hit path — forms
    // a fresh protected `&mut HeapCore` each call, does NOT re-expose
    // `thread_free`). A modest loop count: enough interleavings for miri's
    // preemptive scheduler to land a remote CAS inside a live owner frame,
    // without blowing the per-test miri budget.
    const SMALL_ITERS: usize = 200;
    const SMALL_SIZE: usize = 64;
    let small_layout = Layout::from_size_align(SMALL_SIZE, 8).unwrap();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // Owner pre-allocates the Large blocks so they are registered, owner-
    // stamped segments before the remote thread starts freeing them.
    let mut large_ptrs: Vec<*mut u8> = Vec::with_capacity(LARGE_N);
    for i in 0..LARGE_N {
        let p = unsafe { (*heap).alloc(large_layout) };
        assert!(!p.is_null(), "large alloc[{i}] returned null");
        large_ptrs.push(p);
    }

    // A gate so both threads start their tight loops at (nearly) the same
    // instant, maximising the window in which a remote CAS overlaps an owner
    // `&mut HeapCore` alloc frame.
    let start = Arc::new(AtomicBool::new(false));

    // Raw pointers are `!Send`; ship addresses.
    let addrs: Vec<usize> = large_ptrs.iter().map(|&p| p as usize).collect();
    let heap_addr = heap as usize;
    let start_remote = Arc::clone(&start);

    let remote = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        while !start_remote.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        // Each dealloc of a Large block owned by `heap` routes through
        // `dealloc_routing` → `push_large_deferred_free` → a wildcard CAS into
        // `heap.thread_free` — concurrently with the owner's `&mut` alloc loop.
        for &addr in &addrs {
            let p = addr as *mut u8;
            unsafe { (*remote_heap).dealloc(p, large_layout) };
        }
        unsafe { HeapRegistry::recycle(remote_heap) };
    });

    // Owner: release the gate, then spin small allocs. Every iteration forms a
    // protected `&mut HeapCore` over the struct whose `thread_free` bytes the
    // remote is CASing. These are the frames the remote write must not violate.
    start.store(true, Ordering::Release);
    let heap_ptr = heap_addr as *mut sefer_alloc::registry::HeapCore;
    let mut small_ptrs: Vec<*mut u8> = Vec::with_capacity(SMALL_ITERS);
    for _ in 0..SMALL_ITERS {
        let p = unsafe { (*heap_ptr).alloc(small_layout) };
        assert!(!p.is_null(), "small alloc returned null");
        small_ptrs.push(p);
    }

    remote.join().unwrap();

    // Drain the deferred-free stack the remote populated (own-thread large
    // alloc slow path runs the drain) and clean up, so miri's leak checker is
    // satisfied.
    let drain = unsafe { (*heap).alloc(large_layout) };
    assert!(!drain.is_null());
    unsafe { (*heap).dealloc(drain, large_layout) };
    for &p in &small_ptrs {
        unsafe { (*heap).dealloc(p, small_layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}
