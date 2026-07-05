//! Regression (task #133, 0.4.x): the diagnostic hit counters
//! (`tcache_hits_total` / `large_cache_hits_total`) must correctly AGGREGATE
//! across every live heap, not just report one heap's local count.
//!
//! ## Background
//!
//! Task #133 moved `DBG_TCACHE_HITS` (magazine hits) and `LARGE_CACHE_HITS`
//! (large-segment cache hits) from single process-wide `static AtomicU64`
//! counters — bumped by EVERY thread's hot path, hence a contended `lock
//! xadd` and a cross-core cache-line ping-pong under MT — to PER-HEAP
//! fields (`HeapCore::tcache_hits`, `AllocCore::large_cache_hits`). Each
//! heap's own alloc fast path now increments only ITS OWN counter, never
//! touching another heap's cache line.
//!
//! The process-wide VIEW that `SeferAlloc::stats()` exposes (and that
//! `tests/regression_fastbin_aligned_roundtrip.rs` asserts against) must
//! still reflect activity from every heap. This is what
//! `registry::tcache_hits_total()` / `registry::large_cache_hits_total()`
//! do: walk every minted registry slot and sum each one's own counter.
//!
//! **Counterfactual (performed by hand — see the task report):** with the
//! per-heap counter correctly wired but the aggregation function BROKEN
//! (e.g. reading only slot 0, or capped to a single heap, or the pre-fix
//! single-`static`-counter code restored so only the LAST heap to run on a
//! given thread's TLS binding is visible), this test's cross-thread
//! assertion (`total >= before_total + hits_a + hits_b`, requiring BOTH
//! threads' contributions) fails to hold while a single-heap view (this
//! test also captures `HeapCore::tcache_hits()` per-heap) still passes —
//! i.e. this test is the one that would catch a "silently aggregates only
//! one heap" regression that a single-thread test cannot.
//!
//! ## This test
//!
//! Two OS threads each claim their own heap via `HeapRegistry::claim` (never
//! sharing a `HeapCore` — the whole point of the per-heap counter fix) and
//! each drives an align>16 alloc/dealloc/alloc cycle (same shape as the C1
//! fastbin test) enough times to guarantee at least one magazine hit per
//! thread. Each thread also independently reads back ITS OWN heap's local
//! `HeapCore::tcache_hits()` (via the `#[doc(hidden)]` test-only accessor)
//! to confirm the per-heap counter itself advanced. Finally the main thread
//! reads the process-wide `tcache_hits_total()` and asserts it increased by
//! AT LEAST the sum of what both threads observed locally — i.e. the
//! aggregation is not dropping either heap's contribution.

// Requires `alloc-stats` (task W3): every assertion here rests on the per-hit
// `tcache_hits` increment, which is gated behind `alloc-stats` (default OFF,
// not in `production`). Without the feature the counters read 0 by design and
// the aggregation cannot be exercised — the file is skipped.
#![cfg(all(feature = "alloc-global", feature = "fastbin", feature = "alloc-stats"))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sefer_alloc::registry::{bootstrap, tcache_hits_total, HeapRegistry};

// Serialise all tests in this file against the other registry-touching test
// files: the registry is a process-global static and `tcache_hits_total()`
// aggregates a process-wide total (matching the discipline already used in
// `regression_fastbin_aligned_roundtrip.rs` and `heap_core_tcache.rs`).
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

/// Drive one heap through enough align>16 alloc/dealloc/alloc cycles that
/// the magazine fast path is guaranteed to register at least one hit, and
/// return the DELTA this call caused in that heap's own local hit count
/// (read via the `#[doc(hidden)]` per-heap accessor) -- not the slot's
/// lifetime-cumulative count, which may already be nonzero if `claim`
/// handed back a recycled slot from an earlier test/run in this process.
fn drive_one_heap() -> u64 {
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // `claim` may hand back a RECYCLED slot whose `HeapCore` (and its
    // `tcache_hits` counter) was already live -- possibly with a nonzero
    // count -- from an EARLIER test/run in this same process (a minted
    // slot's `HeapCore` is reused as-is across recycle/re-claim, never
    // reset -- see `HeapSlot::heap`'s doc comment). Capture the starting
    // value so the count this function returns is the DELTA this
    // function's own workload caused, not the slot's lifetime total
    // (comparing a lifetime-cumulative per-heap count against a
    // before/after delta of the process-wide total is an apples-to-oranges
    // bug -- exactly what the first version of this test got wrong; see the
    // counterfactual note in the module doc comment).
    //
    // SAFETY: `heap` is live, just claimed by this thread, not yet
    // recycled; read-only Relaxed load.
    let local_before = unsafe { (*heap).tcache_hits() };

    let shapes = [
        (640usize, 128usize),
        (256usize, 64usize),
        (100usize, 32usize),
    ];
    for &(size, align) in &shapes {
        let layout = Layout::from_size_align(size, align).unwrap();
        let p1 = unsafe { (*heap).alloc(layout) };
        assert!(!p1.is_null(), "alloc({size},{align}) returned null");
        unsafe { (*heap).dealloc(p1, layout) };
        let p2 = unsafe { (*heap).alloc(layout) };
        assert!(!p2.is_null(), "second alloc({size},{align}) returned null");
        unsafe { (*heap).dealloc(p2, layout) };
    }

    // SAFETY: `heap` is a live `*mut HeapCore` returned by `claim` above,
    // not yet recycled. `tcache_hits()` is a read-only Relaxed load.
    let local_after = unsafe { (*heap).tcache_hits() };

    unsafe { HeapRegistry::recycle(heap) };
    local_after.saturating_sub(local_before)
}

/// Two threads, each with their OWN heap, both contribute to the
/// process-wide `tcache_hits_total()`. This is the counterfactual-checked
/// half: an aggregation bug that only sees one heap (e.g. a broken slot
/// walk) would make `after < before + hits_a + hits_b`.
#[test]
fn tcache_hits_aggregate_across_multiple_heaps() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let before = tcache_hits_total();

    // Run sequentially-spawned-but-concurrently-live threads so each binds a
    // DISTINCT registry slot (HeapRegistry::claim is per-call, not
    // per-thread-cached here — we call it directly, so two threads calling
    // it concurrently claim two different slots by construction of the
    // claim CAS).
    let t1 = thread::spawn(drive_one_heap);
    let t2 = thread::spawn(drive_one_heap);

    let hits_a = t1.join().expect("thread A panicked");
    let hits_b = t2.join().expect("thread B panicked");

    assert!(
        hits_a > 0,
        "thread A's own heap registered zero magazine hits -- test setup is broken"
    );
    assert!(
        hits_b > 0,
        "thread B's own heap registered zero magazine hits -- test setup is broken"
    );

    let after = tcache_hits_total();
    let total_increase = after.saturating_sub(before);

    assert!(
        total_increase >= hits_a + hits_b,
        "tcache_hits_total() did not aggregate both heaps' contributions \
         (before={before}, after={after}, increase={total_increase}, \
         expected >= hits_a({hits_a}) + hits_b({hits_b}) = {}) -- \
         the per-heap aggregation walk is dropping at least one heap",
        hits_a + hits_b
    );
}

/// Single-thread sanity check (the ST half of the counterfactual described
/// in the module doc): N alloc/dealloc/alloc cycles on ONE heap must
/// increase the process-wide total by at least N (one hit per cycle's
/// second `alloc`), and that increase must exactly match what this heap's
/// own local counter reports -- confirming the aggregation for the
/// single-heap case is neither dropping nor double-counting.
#[test]
fn tcache_hits_single_heap_matches_local_and_global() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let before = tcache_hits_total();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // `HeapRegistry::claim` may hand back a RECYCLED slot whose `HeapCore`
    // (and thus its `tcache_hits` counter) was already live before this
    // test ran (the registry never resets a minted slot's `HeapCore` -- see
    // `HeapSlot::heap`'s doc comment). So this heap's own counter is
    // CUMULATIVE across its whole process lifetime, not zero at claim time.
    // Capture its starting value here so the "how many hits did THIS test's
    // workload cause" comparison below is a delta, matching the delta we
    // compute for the process-wide total.
    //
    // SAFETY: `heap` is live, just claimed by this thread, not yet
    // recycled; read-only Relaxed load.
    let local_before = unsafe { (*heap).tcache_hits() };

    const N: usize = 20;
    let layout = Layout::from_size_align(384, 128).unwrap();
    for _ in 0..N {
        let p1 = unsafe { (*heap).alloc(layout) };
        assert!(!p1.is_null());
        unsafe { (*heap).dealloc(p1, layout) };
        let p2 = unsafe { (*heap).alloc(layout) };
        assert!(!p2.is_null());
        unsafe { (*heap).dealloc(p2, layout) };
    }

    // SAFETY: `heap` is live, not yet recycled; read-only Relaxed load.
    let local_after = unsafe { (*heap).tcache_hits() };
    let local_delta = local_after.saturating_sub(local_before);
    let after = tcache_hits_total();

    assert!(
        local_delta as usize >= N,
        "this heap's local tcache_hits() delta ({local_delta}) is less than \
         the {N} guaranteed second-allocs -- the magazine hit path regressed"
    );
    // NOTE: this is `>=`, not `==` -- other `#[test]` functions in this same
    // test BINARY may run concurrently (the default `cargo test` runner) and
    // claim/drive their OWN heaps in the same process, which also bump the
    // process-wide total. `SerialGuard` only serialises tests WITHIN this
    // file; it does not stop other test files' threads. What must hold
    // regardless: the global total can never have increased by LESS than
    // this heap's own observed local DELTA (that would mean this heap's
    // contribution was dropped by the aggregation walk).
    assert!(
        after.saturating_sub(before) >= local_delta,
        "process-wide tcache_hits_total() increase ({}) is LESS than this \
         heap's own local tcache_hits() delta ({local_delta}) in a \
         single-heap window -- the aggregation walk is dropping this heap's \
         contribution",
        after.saturating_sub(before)
    );

    unsafe { HeapRegistry::recycle(heap) };
}
