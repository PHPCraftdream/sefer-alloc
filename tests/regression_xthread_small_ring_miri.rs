//! Miri-plain coverage for the multi-producer SMALL-block `RemoteFreeRing`
//! push path (audit finding G1, high severity —
//! `docs/reviews/2026-08-04-release-stabilization-audit.md`).
//!
//! ## The coverage gap this closes
//!
//! The most-scrutinised `unsafe` seam in the project — `Node::atomic_u32_at`
//! → `RemoteFreeRing::{push, drain}` (N remote producers, one owner-consumer)
//! — had ZERO miri coverage under concurrency for the SMALL-block path. The
//! existing miri-plain matrix (`regression_xthread_large_free_no_leak`,
//! `regression_xthread_thread_free_alias_miri`) covers only the LARGE
//! cross-thread path (the `deferred_large` `AtomicPtr` stack / the
//! `thread_free` aliasing guard). TSan covers data races on the ring's
//! atomics but cannot speak to Stacked/Tree Borrows aliasing or provenance —
//! exactly the class of bug miri catches.
//!
//! ## What this test does
//!
//! Two spawned producer threads concurrently free small blocks (64 B, well
//! below `SMALL_MAX`) that the owner thread pre-allocated — all from the same
//! segment, so both producers push into the SAME per-segment `RemoteFreeRing`,
//! exercising the multi-producer CAS-reserve push protocol
//! (`RemoteFreeRing::push` → `Node::atomic_u32_at`). While the producers push,
//! the owner spins its own allocations (each forming a fresh protected
//! `&mut HeapCore` frame overlapping the producers' ring writes). After both
//! producers join, the owner force-drains all rings via `dbg_drain_all_rings`,
//! exercising the single-consumer drain path
//! (`RemoteFreeRing::drain` → `Node::atomic_u32_at`).
//!
//! Run under plain-provenance miri (Stacked Borrows, non-strict — the same
//! mode as the other `miri-plain` tests), this validates that the concurrent
//! multi-producer push and the owner drain are free of aliasing/provenance UB.

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

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let heap_addr = heap as usize;

    // Owner pre-allocates all N blocks (same segment for 64-byte blocks).
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        let p = unsafe { (*heap).alloc(small_layout) };
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
        let remote_heap = HeapRegistry::claim();
        assert!(
            !remote_heap.is_null(),
            "producer A HeapRegistry::claim failed"
        );
        while !start_a.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        // Each dealloc routes through `dealloc_routing` →
        // `dealloc_foreign_slow` → `RemoteFreeRing::push` (a CAS-reserve
        // into the owner segment's per-segment ring). Both producers push
        // concurrently into the SAME ring — the multi-producer case.
        for &addr in &addrs_a {
            unsafe { (*remote_heap).dealloc(addr as *mut u8, small_layout) };
        }
        unsafe { HeapRegistry::recycle(remote_heap) };
    });

    // Producer B: identical structure, independent heap claim.
    let start_b = Arc::clone(&start);
    let prod_b = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(
            !remote_heap.is_null(),
            "producer B HeapRegistry::claim failed"
        );
        while !start_b.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        for &addr in &addrs_b {
            unsafe { (*remote_heap).dealloc(addr as *mut u8, small_layout) };
        }
        unsafe { HeapRegistry::recycle(remote_heap) };
    });

    // Owner: release the gate, then spin small allocs. Every iteration forms a
    // protected `&mut HeapCore` over the struct whose segment metadata the
    // producers are concurrently CASing via `Node::atomic_u32_at` — the real
    // overlap whose aliasing discipline miri validates.
    let heap_ptr = heap_addr as *mut sefer_alloc::registry::HeapCore;
    start.store(true, Ordering::Release);
    let mut owner_ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        let p = unsafe { (*heap_ptr).alloc(small_layout) };
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
    unsafe { (*heap_ptr).dbg_drain_all_rings() };

    // Cleanup: free everything the owner currently holds and recycle the heap.
    for &p in &owner_ptrs {
        unsafe { (*heap).dealloc(p, small_layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}
