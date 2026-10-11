#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]
//! Plain-Miri coverage for two remote producers freeing 64-byte allocations
//! while the owner allocates under its exclusive HeapLease. The historical
//! file/test names are retained; the current ingress is a route sidecar bitmap,
//! not RemoteFreeRing or an intrusive deferred-free stack.
//!
//! HeapCore::dealloc routes foreign addresses through publish_foreign and a
//! RouteDirectory pin. Small/Primordial publication uses owner-recorded class
//! metadata and a terminal pending-bit fetch_or(AcqRel) in the independent
//! sidecar. Producers do not access reservation bytes or the owner-only ledger;
//! releasing the pin after publication touches only the descriptor/sidecar.
//! These tiny allocations may come from Primordial: the body neither asserts
//! RouteKind::Small nor checks that every pointer shares one segment.
//!
//! After both producers join, dbg_drain_sidecar_ingress visits the owner's
//! registered routes, cuts bitmap words and reclaims through canonical table
//! roots. Owner allocations may also consume ingress before that explicit pass.
//! This is an interpreted memory-safety exercise, not an exactly-once-count or
//! forced-overlap oracle: the start gate permits concurrency but cannot force it.
//!
//! The manifest selects plain provenance with Stacked Borrows and elevated
//! preemption, not strict provenance or Tree Borrows. Integer pointer transport
//! relies on exposed provenance. A pass covers the exercised schedules only;
//! it is not a whole-project aliasing/provenance proof.

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
// `regression_xthread_thread_free_alias_miri.rs`.)
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
fn xthread_small_ring_two_producers_push_owner_drains() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // 64 B — a typical small block, well below SMALL_MAX (~253 KiB). A
    // cross-thread free of this size routes through `dealloc_foreign_slow` →
    // `RemoteFreeRing::push` (the per-segment MPSC offset queue), NOT the
    // `deferred_large` AtomicPtr stack the existing miri-plain tests cover.
    const SMALL_SIZE: usize = 64;
    let small_layout = Layout::from_size_align(SMALL_SIZE, 8).unwrap();

    // Total blocks, split evenly between 2 producer threads. Kept tiny for
    // miri (each alloc/dealloc is individually interpreted — miri is ~1e5×
    // slower than native). All blocks come from the same segment (a 4 MiB
    // segment holds ~65 K 64-byte blocks), so both producers push into the
    // SAME `RemoteFreeRing` — the genuine multi-producer case.
    #[cfg(not(miri))]
    const N: usize = 64;
    #[cfg(miri)]
    const N: usize = 8;

    let mut lease = HeapRegistry::dbg_claim_lease().expect("HeapRegistry::claim returned None");

    // Owner pre-allocates all N blocks (same segment for 64-byte blocks).
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        let p = lease.core().alloc(small_layout);
        assert!(!p.is_null(), "owner pre-alloc returned null");
        ptrs.push(p);
    }

    // Split addresses evenly between the 2 producers. Raw pointers are
    // `!Send`; ship addresses instead (same idiom as the other xthread tests).
    let half = N / 2;
    let addrs_a: Vec<usize> = ptrs[..half].iter().map(|&p| p as usize).collect();
    let addrs_b: Vec<usize> = ptrs[half..].iter().map(|&p| p as usize).collect();

    // A gate so all three threads start their tight loops at (nearly) the same
    // instant, maximising the window in which a producer ring-push overlaps
    // the owner's `&mut HeapCore` alloc frames.
    let start = Arc::new(AtomicBool::new(false));

    // Producer A: frees its half concurrently with producer B and the owner.
    let start_a = Arc::clone(&start);
    let prod_a = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let mut remote_lease =
            HeapRegistry::dbg_claim_lease().expect("producer A HeapRegistry::claim failed");
        let remote_heap = remote_lease.core();
        while !start_a.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        // Each dealloc routes through `dealloc_routing` →
        // `dealloc_foreign_slow` → `RemoteFreeRing::push` (a CAS-reserve
        // into the owner segment's per-segment ring). Both producers push
        // concurrently into the SAME ring — the multi-producer case.
        for &addr in &addrs_a {
            // SAFETY: `addr` is a live owner allocation; this deliberate
            // cross-thread free exercises the remote ring under test.
            unsafe { remote_heap.dealloc(addr as *mut u8, small_layout) };
        }
        drop(remote_lease);
    });

    // Producer B: identical structure, independent heap claim.
    let start_b = Arc::clone(&start);
    let prod_b = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let mut remote_lease =
            HeapRegistry::dbg_claim_lease().expect("producer B HeapRegistry::claim failed");
        let remote_heap = remote_lease.core();
        while !start_b.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        for &addr in &addrs_b {
            // SAFETY: `addr` is a live owner allocation; this deliberate
            // cross-thread free exercises the remote ring under test.
            unsafe { remote_heap.dealloc(addr as *mut u8, small_layout) };
        }
        drop(remote_lease);
    });

    // Owner: release the gate, then spin small allocs. Every iteration forms a
    // protected `&mut HeapCore` over the struct whose segment metadata the
    // producers are concurrently CASing via `Node::atomic_u32_at` — the real
    // overlap whose aliasing discipline miri validates.
    start.store(true, Ordering::Release);
    let mut owner_ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        let p = lease.core().alloc(small_layout);
        assert!(!p.is_null(), "owner concurrent alloc returned null");
        owner_ptrs.push(p);
    }

    prod_a.join().unwrap();
    prod_b.join().unwrap();

    // Force-drain every owned segment's ring into its BinTable, exercising the
    // single-consumer `RemoteFreeRing::drain` path (`Node::atomic_u32_at`
    // reads of tail/head/slots). This reclaims the offsets the producers
    // pushed, making them available for the owner's future allocations and
    // satisfying miri's leak checker.
    lease.core().dbg_drain_sidecar_ingress();

    // Cleanup: free everything the owner currently holds and recycle the heap.
    for &p in &owner_ptrs {
        // SAFETY: `p` is a live owner allocation of this lease's heap.
        unsafe { lease.core().dealloc(p, small_layout) };
    }
    // Drop of the lease recycles the slot (LIVE -> FREE, Release).
    drop(lease);
}
