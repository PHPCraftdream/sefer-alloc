//! Plain-Miri regression for remote Large publication concurrent with owner
//! Small allocations and their protected &mut HeapCore call frames. Historical
//! file/test names are retained; current producers do not write a thread_free
//! field in the owner core or link a deferred-free stack inside reservations.
//!
//! HeapCore::dealloc -> dealloc_routing -> publish_foreign looks up and pins an
//! independent RouteDirectory descriptor. LargeState::publish_pending performs
//! the terminal LIVE(g) -> PENDING(g) CAS on that descriptor's atomic word.
//! Reservation/header and owner-only ledger accesses are forbidden to foreign
//! producers, including after publication; pin release may touch the sidecar.
//! Owner-side drain claims pending descriptors and accesses reservations only
//! through canonical roots while holding the exclusive heap lease.
//!
//! The owner preallocates eight Large blocks, releases a start gate, and makes
//! 200 Small allocations while the remote thread frees the Large batch. After
//! join it allocates another Large block and performs local cleanup; owner
//! allocation paths and lease teardown handle pending ingress/retention policy.
//! The gate makes competing frames possible, not a guaranteed overlap. Miri
//! checks exercised memory accesses, not every schedule or reclamation count.
//!
//! The manifest selects plain provenance, Stacked Borrows and elevated
//! preemption. Address transport uses exposed provenance; this is not a
//! strict-provenance or Tree-Borrows acceptance target, nor evidence that the
//! removed thread_free mechanism still exists.

#![cfg(all(
    all(feature = "alloc-global", feature = "alloc-xthread"),
    feature = "internals"
))]

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
    const LARGE_SIZE: usize = 2 * 1024 * 1024; // > SMALL_MAX → routed to alloc_large
    let large_layout = Layout::from_size_align(LARGE_SIZE, 8).unwrap();

    // Small allocs the owner spins concurrently (OPT-C cache-hit path — forms
    // a fresh protected `&mut HeapCore` each call, does NOT re-expose
    // `thread_free`). A modest loop count: enough interleavings for miri's
    // preemptive scheduler to land a remote CAS inside a live owner frame,
    // without blowing the per-test miri budget.
    const SMALL_ITERS: usize = 200;
    const SMALL_SIZE: usize = 64;
    let small_layout = Layout::from_size_align(SMALL_SIZE, 8).unwrap();

    let mut lease = HeapRegistry::dbg_claim_lease().expect("HeapRegistry::claim returned None");

    // Owner pre-allocates the Large blocks so they are registered, owner-
    // stamped segments before the remote thread starts freeing them.
    let mut large_ptrs: Vec<*mut u8> = Vec::with_capacity(LARGE_N);
    for i in 0..LARGE_N {
        let p = lease.core().alloc(large_layout);
        assert!(!p.is_null(), "large alloc[{i}] returned null");
        large_ptrs.push(p);
    }

    // A gate so both threads start their tight loops at (nearly) the same
    // instant, maximising the window in which a remote CAS overlaps an owner
    // `&mut HeapCore` alloc frame.
    let start = Arc::new(AtomicBool::new(false));

    // Raw pointers are `!Send`; ship addresses.
    let addrs: Vec<usize> = large_ptrs.iter().map(|&p| p as usize).collect();
    let start_remote = Arc::clone(&start);

    let remote = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let mut remote_lease =
            HeapRegistry::dbg_claim_lease().expect("remote HeapRegistry::claim failed");
        let remote_heap = remote_lease.core();
        while !start_remote.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        // Each dealloc of a Large block owned by the owner heap routes through
        // `dealloc_routing` → `push_large_deferred_free` → a wildcard CAS into
        // the owner's `thread_free` — concurrently with the owner's `&mut`
        // alloc loop.
        for &addr in &addrs {
            let p = addr as *mut u8;
            // SAFETY: `p` was allocated by the owner heap with `large_layout`.
            unsafe { remote_heap.dealloc(p, large_layout) };
        }
        drop(remote_lease);
    });

    // Owner: release the gate, then spin small allocs. Every iteration forms a
    // protected `&mut HeapCore` over the struct whose `thread_free` bytes the
    // remote is CASing. These are the frames the remote write must not violate.
    start.store(true, Ordering::Release);
    let mut small_ptrs: Vec<*mut u8> = Vec::with_capacity(SMALL_ITERS);
    for _ in 0..SMALL_ITERS {
        let p = lease.core().alloc(small_layout);
        assert!(!p.is_null(), "small alloc returned null");
        small_ptrs.push(p);
    }

    remote.join().unwrap();

    // Drain the deferred-free stack the remote populated (own-thread large
    // alloc slow path runs the drain) and clean up, so miri's leak checker is
    // satisfied.
    let drain = lease.core().alloc(large_layout);
    assert!(!drain.is_null());
    // SAFETY: `drain` came from `alloc` with `large_layout` above.
    unsafe { lease.core().dealloc(drain, large_layout) };
    for &p in &small_ptrs {
        // SAFETY: `p` came from `alloc` with `small_layout` above.
        unsafe { lease.core().dealloc(p, small_layout) };
    }
    // Drop of the lease recycles the slot (LIVE -> FREE, Release).
    drop(lease);
}
