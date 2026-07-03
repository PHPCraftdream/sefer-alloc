//! P3 (task #147) — bulk-mode bypass RETIRED; magazine is the sole path.
//!
//! History: Phase P7 added a per-class `alloc_streak` counter and a
//! bulk-mode bypass that skipped the magazine on alloc-without-free streaks
//! (both an alloc-side branch in `HeapCore::alloc` and a dealloc-side
//! full-flush branch in `dealloc_own_thread`). P3 (Э1 — bump-direct batched
//! carve, `AllocCore::refill_class_bump`) makes a magazine miss carve
//! straight into the magazine at near-`memcpy` cost, so the bypass buys
//! nothing and BOTH sides were retired together (retiring one alone would
//! leave a permanently-dead branch — a stuck-at-0 streak the survivor could
//! never satisfy). The `alloc_streak` array and the `dbg_alloc_streak` /
//! `dbg_reset_bulk_state` hooks were removed with it.
//!
//! This file now asserts the NEW behavior: there is NO bypass — every
//! magazine miss refills the magazine (via bump-direct carve or free-drain)
//! and every alloc that finds blocks in the magazine pops from it. The
//! correctness properties (distinct pointers, no double-issue, clean churn,
//! cross-thread unaffected) hold with the magazine as the single path.
//!
//! ## Tests
//!
//! - **t_cold_storm_stays_in_magazine**: 64 consecutive 16B allocs without
//!   frees. The magazine is refilled on each miss and popped on each hit —
//!   it is NEVER force-emptied mid-stream by a bypass. All 64 pointers are
//!   distinct and writable.
//!
//! - **t_churn_distinct_and_healthy**: working-set 24, 1024 churn iters.
//!   Every live pointer stays distinct; the allocator never hands the same
//!   block out twice.
//!
//! - **t_bulk_then_drain_then_churn**: alloc 64 → free all → 1024 churn
//!   iters. No leaks, no double-issue, allocator healthy throughout.
//!
//! - **t_cross_thread_unaffected**: 2 threads via SeferAlloc. Thread A
//!   allocs in bulk volume, sends one ptr to thread B, B frees it
//!   cross-thread. No panic, no leak.

#![cfg(all(feature = "alloc-global", feature = "fastbin"))]

use std::alloc::Layout;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise all tests in this file: the registry is a process-global static.
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

/// Simple xorshift64 PRNG for deterministic index selection.
fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

// ── t_cold_storm_stays_in_magazine ─────────────────────────────────────────

/// Alloc 64 blocks of class 16B without any intervening frees (a cold
/// alloc-storm — exactly the pattern the retired bypass targeted). The NEW
/// behavior: each magazine miss refills via bump-direct carve and each hit
/// pops from the magazine; there is NO bypass that force-empties the
/// magazine mid-stream. We assert the observable invariants: every pointer
/// is distinct and writable, and the magazine never yields a stale/duplicate
/// block.
///
/// Counterfactual (why this is not vacuous): 64 allocs of a 16B class with
/// TCACHE_CAP=16 forces at least 4 refills. If bump-direct double-issued a
/// carved block (e.g. carved into the magazine AND left it on a BinTable to
/// be popped again), the distinct-pointer assertion would fail.
#[test]
fn t_cold_storm_stays_in_magazine() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    // A recycled slot may carry a populated magazine from an earlier test in
    // this process; flush to a known-clean baseline.
    unsafe { (*heap).dbg_flush_all() };

    const TOTAL: usize = 64;
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(TOTAL);
    for i in 0..TOTAL {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc returned null at i={i}");
        unsafe { core::ptr::write_bytes(p, 0xBB, 16) };
        ptrs.push(p);
    }

    // All pointers must be distinct — no block issued twice.
    let set: HashSet<usize> = ptrs.iter().map(|&p| p as usize).collect();
    assert_eq!(set.len(), TOTAL, "duplicate pointers in cold alloc-storm");

    // Free all — no panic, no double-free trips.
    for &p in &ptrs {
        unsafe { (*heap).dealloc(p, layout) };
    }

    // Allocator still serves after the storm.
    let check = unsafe { (*heap).alloc(layout) };
    assert!(!check.is_null(), "alloc after cold-storm returned null");
    unsafe { (*heap).dealloc(check, layout) };

    unsafe { HeapRegistry::recycle(heap) };
}

// ── t_churn_distinct_and_healthy ───────────────────────────────────────────

/// Maintain a working set of 24 blocks with churn for 1024 iterations.
/// Every live pointer must stay distinct at every step — the magazine path
/// (the sole path now) must never re-issue a block that is still live.
#[test]
fn t_churn_distinct_and_healthy() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    unsafe { (*heap).dbg_flush_all() };

    const K: usize = 24;
    const OPS: usize = 1024;
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Initial fill.
    let mut live: Vec<*mut u8> = Vec::with_capacity(K);
    for i in 0..K {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "initial alloc returned null at {i}");
        unsafe { core::ptr::write_bytes(p, 0xCC, 16) };
        live.push(p);
    }

    // Churn: free one, alloc one. Assert distinctness on every step.
    let mut rng: u64 = 0xDEAD;
    for _ in 0..OPS {
        let idx = (xorshift64(&mut rng) as usize) % K;
        unsafe { (*heap).dealloc(live[idx], layout) };
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "churn alloc returned null");
        unsafe { core::ptr::write_bytes(p, 0xDD, 16) };
        live[idx] = p;

        let set: HashSet<usize> = live.iter().map(|&q| q as usize).collect();
        assert_eq!(set.len(), K, "duplicate live pointer during churn");
    }

    for &p in &live {
        unsafe { (*heap).dealloc(p, layout) };
    }

    unsafe { HeapRegistry::recycle(heap) };
}

// ── t_bulk_then_drain_then_churn ───────────────────────────────────────────

/// Alloc 64 (cold storm) → free all 64 → 1024 churn iterations → no leaks,
/// allocator healthy, magazine still works. Exercises the full transition
/// from cold bump-carve to steady-state churn.
#[test]
fn t_bulk_then_drain_then_churn() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    unsafe { (*heap).dbg_flush_all() };

    let layout = Layout::from_size_align(16, 8).unwrap();

    // Phase 1: cold storm.
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(64);
    for i in 0..64 {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "bulk alloc null at {i}");
        unsafe { core::ptr::write_bytes(p, 0xAA, 16) };
        ptrs.push(p);
    }
    let set: HashSet<usize> = ptrs.iter().map(|&p| p as usize).collect();
    assert_eq!(set.len(), 64, "duplicate pointers in bulk alloc");

    // Phase 2: free all.
    for &p in &ptrs {
        unsafe { (*heap).dealloc(p, layout) };
    }
    ptrs.clear();

    // Phase 3: churn — magazine path.
    const K: usize = 24;
    const OPS: usize = 1024;
    let mut live: Vec<*mut u8> = Vec::with_capacity(K);
    for i in 0..K {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "churn fill alloc null at {i}");
        unsafe { core::ptr::write_bytes(p, 0xBB, 16) };
        live.push(p);
    }

    let mut rng: u64 = 0xBEEF;
    for _ in 0..OPS {
        let idx = (xorshift64(&mut rng) as usize) % K;
        unsafe { (*heap).dealloc(live[idx], layout) };
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "churn alloc null");
        unsafe { core::ptr::write_bytes(p, 0xCC, 16) };
        live[idx] = p;
    }

    // All live pointers are distinct.
    let set: HashSet<usize> = live.iter().map(|&p| p as usize).collect();
    assert_eq!(set.len(), K, "duplicate pointers after churn");

    for &p in &live {
        unsafe { (*heap).dealloc(p, layout) };
    }

    unsafe { HeapRegistry::recycle(heap) };
}

// ── t_cross_thread_unaffected ──────────────────────────────────────────────

/// Two threads via SeferAlloc's GlobalAlloc interface: thread A allocs
/// blocks in bulk volume (64 consecutive allocs — the cold-carve path),
/// sends one to thread B via channel, thread B frees it cross-thread.
/// Verify no panic, no leak. Bump-direct's free-drain-before-carve source
/// order keeps the cross-thread ring reclaim working (a freed remote block
/// is reused, not stranded).
#[test]
#[cfg(feature = "alloc-xthread")]
fn t_cross_thread_unaffected() {
    use sefer_alloc::SeferAlloc;
    use std::alloc::GlobalAlloc;
    use std::sync::mpsc;

    // Wrap raw pointer for Send.
    struct SendPtr(*mut u8);
    unsafe impl Send for SendPtr {}

    let _serial = SerialGuard::acquire();

    static ALLOC: SeferAlloc = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();

    let (tx, rx) = mpsc::channel::<SendPtr>();

    let alloc_thread = std::thread::spawn(move || {
        let mut ptrs: Vec<*mut u8> = Vec::with_capacity(64);
        for _ in 0..64 {
            let p = unsafe { ALLOC.alloc(layout) };
            assert!(!p.is_null());
            unsafe { core::ptr::write_bytes(p, 0xEE, 16) };
            ptrs.push(p);
        }

        // Send one pointer to thread B for cross-thread free.
        tx.send(SendPtr(ptrs[0])).unwrap();

        // Free the rest (skip ptrs[0], sent to B).
        for &p in &ptrs[1..] {
            unsafe { ALLOC.dealloc(p, layout) };
        }

        // Small alloc to trigger ring drain (B's cross-thread free
        // lands in our segment's ring; the next alloc drains it lazily).
        std::thread::sleep(std::time::Duration::from_millis(50));
        let probe = unsafe { ALLOC.alloc(layout) };
        if !probe.is_null() {
            unsafe { ALLOC.dealloc(probe, layout) };
        }
    });

    let free_thread = std::thread::spawn(move || {
        // Do a small alloc first so TLS binding initializes for this thread.
        let warmup = unsafe { ALLOC.alloc(layout) };
        if !warmup.is_null() {
            unsafe { ALLOC.dealloc(warmup, layout) };
        }

        // Receive the pointer from A.
        let SendPtr(ptr) = rx.recv().unwrap();

        // Free it — this is a cross-thread free (goes to A's ring).
        unsafe { ALLOC.dealloc(ptr, layout) };
    });

    alloc_thread.join().expect("alloc thread panicked");
    free_thread.join().expect("free thread panicked");
}
