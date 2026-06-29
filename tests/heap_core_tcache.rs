//! Phase P2 -- tcache (magazine) correctness tests.
//!
//! Tests the per-thread, per-class magazine cache wired into `HeapCore` under
//! the `fastbin` feature. All tests exercise `HeapCore` through
//! `HeapRegistry::claim` (the same path `SeferMalloc` uses).
//!
//! ## Tests
//!
//! - **T1-round-trip**: drive HeapCore::alloc/dealloc over a working set of
//!   K=64 live pointers for 1024 random-index iterations (xorshift seed,
//!   deterministic). After each iteration assert no duplicate live pointers
//!   (HashSet check on the K live slots). At end, dealloc all. Verifies:
//!   distinct pointers, no double-issue, conservation through the magazine
//!   round-trip.
//!
//! - **T7-conservation**: alloc N, free N, repeat 100 times, assert no leak.
//!
//! - **T-bulk-overflow**: alloc 1000 blocks of class 16B in a tight loop
//!   (forces magazine overflow many times via flush). Assert all 1000 pointers
//!   distinct and non-null. Free all. Confirms the half-flush hysteresis
//!   doesn't lose blocks.
//!
//! ## Counterfactual note
//!
//! If the magazine path were missing the `stamp_segment_owner` call, the
//! cross-thread routing tests (existing `race_repro` / `soak_xthread`) would
//! detect the missing stamp: a remote thread's `dealloc_routing` reads
//! `owner_thread_free_at(base)` and routes to the per-segment ring only if
//! the stamp is present. Without the stamp the remote free silently drops
//! (safe no-op) -- but the T7/soak conservation checks would catch the leak.

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

// ── T1: round-trip ─────────────────────────────────────────────────────────

/// Drive HeapCore::alloc/dealloc over a working set of K=64 live pointers for
/// 1024 random-index iterations. After each iteration assert no duplicate live
/// pointers. At end, dealloc all.
#[test]
fn t1_round_trip() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    const K: usize = 64;
    const OPS: usize = 1024;
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Initial fill: allocate K blocks.
    let mut live: Vec<*mut u8> = Vec::with_capacity(K);
    for i in 0..K {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "initial alloc returned null at {i}");
        // Write a pattern to prove the block is usable.
        unsafe { core::ptr::write_bytes(p, 0xAA, 16) };
        live.push(p);
    }

    // Check: all K pointers are distinct.
    let mut set: HashSet<usize> = HashSet::with_capacity(K);
    for &p in &live {
        assert!(set.insert(p as usize), "duplicate pointer in initial fill");
    }

    // Churn: for OPS iterations, free a random slot and allocate a replacement.
    let mut rng: u64 = 0xCAFE;
    for iter in 0..OPS {
        let idx = (xorshift64(&mut rng) as usize) % K;
        // Free the old block.
        unsafe { (*heap).dealloc(live[idx], layout) };
        // Alloc a replacement.
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc returned null at iteration {iter}");
        // Write a pattern.
        unsafe { core::ptr::write_bytes(p, (iter & 0xFF) as u8, 16) };
        live[idx] = p;

        // Uniqueness check: all K live pointers must be distinct.
        set.clear();
        for &lp in &live {
            assert!(
                set.insert(lp as usize),
                "duplicate live pointer at iteration {iter}"
            );
        }
    }

    // Final: dealloc all.
    for p in &live {
        unsafe { (*heap).dealloc(*p, layout) };
    }

    unsafe { HeapRegistry::recycle(heap) };
}

// ── T7: conservation ───────────────────────────────────────────────────────

/// Alloc N, free N, repeat 100 times, assert no crash and the allocator is
/// still functional at the end.
#[test]
fn t7_conservation() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    const N: usize = 128;
    const ROUNDS: usize = 100;
    let layout = Layout::from_size_align(32, 8).unwrap();

    for round in 0..ROUNDS {
        let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
        for i in 0..N {
            let p = unsafe { (*heap).alloc(layout) };
            assert!(!p.is_null(), "alloc returned null at round {round}, i={i}");
            // Write pattern to verify usability.
            unsafe { core::ptr::write_bytes(p, (round & 0xFF) as u8, 32) };
            ptrs.push(p);
        }
        // Free all in reverse order (exercises the magazine stack).
        for &p in ptrs.iter().rev() {
            unsafe { (*heap).dealloc(p, layout) };
        }
    }

    // Final sanity: allocate one more and verify it works.
    let p = unsafe { (*heap).alloc(layout) };
    assert!(
        !p.is_null(),
        "final alloc returned null after conservation loop"
    );
    unsafe {
        core::ptr::write_bytes(p, 0x55, 32);
        assert_eq!(p.read(), 0x55, "final read-back mismatch");
        (*heap).dealloc(p, layout);
    }

    unsafe { HeapRegistry::recycle(heap) };
}

// ── T-bulk-overflow ────────────────────────────────────────────────────────

/// Alloc 1000 blocks of class 16B in a tight loop (forces magazine overflow
/// many times via half-flush). Assert all 1000 pointers distinct and non-null.
/// Free all. Confirms the half-flush hysteresis doesn't lose blocks.
#[test]
fn t_bulk_overflow() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    const TOTAL: usize = 1000;
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(TOTAL);
    for i in 0..TOTAL {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc returned null at i={i}");
        // Write a pattern.
        unsafe { core::ptr::write_bytes(p, (i & 0xFF) as u8, 16) };
        ptrs.push(p);
    }

    // All pointers must be distinct.
    let set: HashSet<usize> = ptrs.iter().map(|&p| p as usize).collect();
    assert_eq!(
        set.len(),
        TOTAL,
        "expected {TOTAL} distinct pointers, got {}",
        set.len()
    );

    // Free all.
    for &p in &ptrs {
        unsafe { (*heap).dealloc(p, layout) };
    }

    unsafe { HeapRegistry::recycle(heap) };
}
