//! R31-10 (task #474) — integration tests for `SeferAlloc::trim_current_thread()`,
//! the public, caller-driven heap-trim API promoted from the pre-existing
//! `dbg_trim_current_thread` test/bench-only hook (design:
//! `docs/design/R30_7_TRIM_SCAVENGE_API_DESIGN.md`).
//!
//! Four acceptance criteria from the design doc §6, AC1–AC4:
//!
//! - **AC1** — behavioral equivalence: `trim_current_thread()` produces the
//!   same empty-pool / evicted-cache state that `trim_for_recycle` produces
//!   at thread-exit (verified by the process-wide `segments_released_total`
//!   delta for the cache, and by per-heap `dbg_pooled_count()` for the pool).
//! - **AC2** — the thread keeps working afterward: alloc → trim → alloc
//!   again on the SAME thread, asserting the second allocation succeeds and
//!   both allocations' memory is correct and readable (proving TLS/registry
//!   slot survive the trim).
//! - **AC3** — no cross-thread effect: trimming thread A's heap leaves
//!   thread B's heap state intact (verified via per-arm
//!   `segments_released_total` deltas: A's trim releases A's cached span,
//!   then B's trim STILL releases B's cached span — which would be
//!   impossible if A's trim had already evicted it).
//! - **AC4** — fallback-heap no-op: calling `trim_current_thread()` before
//!   any per-thread heap is bound (fresh thread, no prior allocation) does
//!   not panic and does not corrupt fallback state.
//!
//! AC5 (the measured burst-idle-burst RSS win) is a separate gate report:
//! `docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md`.
//!
//! ## Feature gating
//!
//! `alloc-global` — required for `SeferAlloc` / `global` module to exist.
//! `alloc-decommit` — required for `trim_current_thread` itself (the
//! mechanisms it drives — small pool, large cache — are
//! `alloc-decommit`-only).
//!
//! Run:
//!   cargo test --release --features production --test r31_10_trim_current_thread_api
//!
//! With the stronger per-heap large-cache assertion (AC1):
//!   cargo test --release --features "production bench-internals" \
//!       --test r31_10_trim_current_thread_api

#![cfg(all(feature = "alloc-global", feature = "alloc-decommit"))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use sefer_alloc::global::tls_heap;
use sefer_alloc::SeferAlloc;

// ---------------------------------------------------------------------------
// AC1 — behavioral equivalence with trim_for_recycle
// ---------------------------------------------------------------------------

#[test]
fn ac1_trim_empties_pool_and_evicts_large_cache() {
    let a = SeferAlloc::new();

    // 2 MiB: comfortably Large under every feature combination this crate
    // can build (mirrors `tests/r17_8_deterministic_trim_releases_cached_large_span.rs`).
    let large = Layout::from_size_align(2 << 20, 8).unwrap();

    // Warm-up: establish baseline heap state before the measured snapshot.
    // SAFETY: valid, non-zero-size layout.
    let warmup = unsafe { a.alloc(large) };
    assert!(!warmup.is_null(), "warm-up alloc failed");
    // SAFETY: warmup valid for `large`, freed exactly once.
    unsafe { a.dealloc(warmup, large) };
    a.trim_current_thread();

    let released_before = a.stats().segments_released_total;

    // Deposit a Large span into this thread's large cache by allocating and
    // freeing it. Under `alloc-decommit` with the default unbounded budget,
    // the freed span is cached, not released.
    // SAFETY: valid, non-zero-size layout.
    let p = unsafe { a.alloc(large) };
    assert!(!p.is_null(), "measured alloc failed");
    // SAFETY: p valid for `large`, freed exactly once → cached, not released.
    unsafe { a.dealloc(p, large) };

    // Confirm the span is cached, not yet released.
    let released_after_cache = a.stats().segments_released_total;
    assert_eq!(
        released_after_cache, released_before,
        "freeing a Large span should cache it, not release it \
         (released_before={released_before}, released_after_cache={released_after_cache})"
    );

    // Trim: evicts the cache + drains the pool — the SAME work
    // `trim_for_recycle` does at thread-exit.
    a.trim_current_thread();

    let released_after_trim = a.stats().segments_released_total;
    assert!(
        released_after_trim > released_before,
        "trim_current_thread() did not evict the cached large span: \
         released before={released_before}, after trim={released_after_trim} \
         (expected after > before)"
    );

    // The small-segment pool must be empty after trim (same as
    // `trim_for_recycle` / `dbg_drain_small_pool` would leave it).
    let heap = match tls_heap::current_for_alloc() {
        tls_heap::CurrentHeap::Own(p) => p,
        tls_heap::CurrentHeap::Fallback => std::ptr::null_mut(),
    };
    assert!(
        !heap.is_null(),
        "this thread must have a bound HeapCore after the alloc above"
    );
    // SAFETY: `heap` is this thread's own live, bound `HeapCore` (just
    // resolved via `current_for_alloc()` — the alloc/trim calls above
    // already bound it).
    let pooled = unsafe { (*heap).dbg_pooled_count() };
    assert_eq!(
        pooled, 0,
        "trim_current_thread() must drain the small-segment pool to zero"
    );

    // With `bench-internals`, also directly assert the large-cache-used
    // counter is zero (the strongest per-heap check).
    #[cfg(feature = "bench-internals")]
    {
        // SAFETY: `heap` is this thread's own live, bound `HeapCore`.
        let used = unsafe { (*heap).dbg_large_cache_used() };
        assert_eq!(
            used, 0,
            "trim_current_thread() must evict the entire large cache"
        );
    }
}

// ---------------------------------------------------------------------------
// AC2 — thread continues working after trim
// ---------------------------------------------------------------------------

#[test]
fn ac2_thread_continues_working_after_trim() {
    let a = SeferAlloc::new();
    let layout = Layout::from_size_align(1024, 8).unwrap();

    // Phase 1: allocate and write a sentinel value.
    // SAFETY: valid, non-zero-size layout.
    let p1 = unsafe { a.alloc(layout) };
    assert!(!p1.is_null(), "first allocation must succeed");
    // SAFETY: p1 was just allocated with `layout`, not yet freed.
    unsafe { p1.write_volatile(0x42) };

    // Trim mid-thread-lifetime: flushes tcache/pool/cache but keeps TLS
    // binding and registry slot intact.
    a.trim_current_thread();

    // Phase 2: allocate again on the SAME thread — the TLS binding and
    // registry slot MUST survive the trim.
    // SAFETY: valid, non-zero-size layout.
    let p2 = unsafe { a.alloc(layout) };
    assert!(!p2.is_null(), "second allocation after trim must succeed");

    // The FIRST allocation's memory must still be readable (trim did not
    // corrupt or free live user blocks — it only flushes caches/pools).
    // SAFETY: p1 was allocated above, never freed, still valid.
    let val = unsafe { p1.read_volatile() };
    assert_eq!(val, 0x42, "first allocation's memory must survive trim");

    // The SECOND allocation must be writable (proves the post-trim cold-path
    // allocation produces serviceable memory).
    // SAFETY: p2 was just allocated with `layout`, not yet freed.
    unsafe { p2.write_volatile(0x99) };
    // SAFETY: p2 valid, not yet freed.
    let val2 = unsafe { p2.read_volatile() };
    assert_eq!(val2, 0x99, "second allocation must be writable");

    // SAFETY: p1, p2 valid for `layout`, each freed exactly once.
    unsafe { a.dealloc(p1, layout) };
    unsafe { a.dealloc(p2, layout) };
}

// ---------------------------------------------------------------------------
// AC3 — no cross-thread effect
// ---------------------------------------------------------------------------

/// Thread B caches a large span, waits for thread A to trim A's OWN heap,
/// then trims B's OWN heap and returns the `segments_released_total` delta.
/// If A's trim had evicted B's cache, B's delta would be zero.
#[test]
fn ac3_trim_does_not_affect_other_thread_heap() {
    let a = Arc::new(SeferAlloc::new());
    let large = Layout::from_size_align(2 << 20, 8).unwrap();

    let b_cached = Arc::new(AtomicBool::new(false));
    let a_trimmed = Arc::new(AtomicBool::new(false));

    // --- Thread B: cache a span, wait, then trim its own heap ---
    let a_for_b = Arc::clone(&a);
    let b_cached_b = Arc::clone(&b_cached);
    let a_trimmed_b = Arc::clone(&a_trimmed);
    let handle = thread::spawn(move || -> u64 {
        // SAFETY: valid, non-zero-size layout.
        let p = unsafe { a_for_b.alloc(large) };
        assert!(!p.is_null(), "thread B alloc failed");
        // SAFETY: p valid, freed once → cached in thread B's heap.
        unsafe { a_for_b.dealloc(p, large) };

        // Signal thread A that B has cached its span.
        b_cached_b.store(true, Ordering::Release);

        // Wait for thread A to trim A's own heap.
        while !a_trimmed_b.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(1));
        }

        // Thread A has now trimmed its own heap. Thread B trims its OWN heap.
        let released_before = a_for_b.stats().segments_released_total;
        a_for_b.trim_current_thread();
        let released_after = a_for_b.stats().segments_released_total;

        // Return the delta: if B's cache survived A's trim, this is > 0.
        released_after.saturating_sub(released_before)
    });

    // --- Thread A (main): cache a span, wait for B, trim A's own heap ---

    // SAFETY: valid, non-zero-size layout.
    let p = unsafe { a.alloc(large) };
    assert!(!p.is_null(), "thread A alloc failed");
    // SAFETY: p valid, freed once → cached in thread A's heap.
    unsafe { a.dealloc(p, large) };

    // Wait for thread B to cache its span.
    while !b_cached.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(1));
    }

    // Thread A trims its OWN heap (releases A's cached span).
    a.trim_current_thread();

    // Signal thread B that A has trimmed.
    a_trimmed.store(true, Ordering::Release);

    // Collect thread B's result.
    let b_delta = handle.join().expect("thread B panicked");

    assert!(
        b_delta > 0,
        "thread B's trim should STILL release B's cached span (delta={b_delta}), \
         proving thread A's trim did not evict thread B's large cache — \
         a zero delta would mean A's trim reached into B's heap"
    );
}

// ---------------------------------------------------------------------------
// AC4 — fallback-heap no-op
// ---------------------------------------------------------------------------

#[test]
fn ac4_trim_on_fallback_heap_is_safe_noop() {
    let a = Arc::new(SeferAlloc::new());

    // Spawn a FRESH thread that has NEVER allocated — its TLS has no
    // per-thread heap binding, so `trim_current_thread()` resolves to the
    // fallback heap and must be a no-op (no panic, no state corruption).
    let a_for_child = Arc::clone(&a);
    let handle = thread::spawn(move || {
        // This is the FIRST thing the thread does — no prior allocation has
        // bound a per-thread heap.
        a_for_child.trim_current_thread();

        // Verify the fallback heap was not corrupted: an allocation AFTER
        // the no-op trim must succeed and produce serviceable memory.
        let layout = Layout::from_size_align(256, 8).unwrap();
        // SAFETY: valid, non-zero-size layout.
        let p = unsafe { a_for_child.alloc(layout) };
        assert!(!p.is_null(), "alloc after no-op trim must succeed");
        // SAFETY: p was just allocated, not yet freed.
        unsafe { p.write_volatile(0xAB) };
        // SAFETY: p valid, not yet freed.
        let val = unsafe { p.read_volatile() };
        assert_eq!(val, 0xAB, "memory after no-op trim must be serviceable");
        // SAFETY: p valid for `layout`, freed exactly once.
        unsafe { a_for_child.dealloc(p, layout) };
    });

    handle
        .join()
        .expect("fresh thread panicked during no-op trim");
}
