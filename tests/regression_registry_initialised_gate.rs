//! Regression (task #133, zero-trust follow-up): a diagnostic aggregation
//! walk over the heap registry must NEVER dereference a slot's `HeapCore`
//! before that slot's `HeapCore` is actually materialised.
//!
//! ## The bug this covers
//!
//! The first cut of the #133 per-heap-counter aggregation
//! (`tcache_hits_total` / `large_cache_hits_total` in
//! `src/registry/heap_registry.rs`) used `idx < count` (`Registry::count`,
//! bumped by `bump_count`) as its "safe to dereference `heap`" gate. That is
//! WRONG: `bump_count` runs *before* the slot's `FREE → LIVE` CAS, and
//! `HeapCore::new()` (which reserves an OS segment -- not instantaneous)
//! runs *after* that CAS but *before* `heap_ptr.write(hc)`. So there is a
//! real window where a slot index is already `< count` (and even has
//! `generation == 1`) while `slot.heap` still holds `MaybeUninit::uninit()`
//! bytes. A concurrent reader (e.g. `SeferAlloc::stats()` on another thread,
//! landing on that window) that dereferenced `heap` there would be reading
//! uninitialised memory racing a concurrent non-atomic
//! `MaybeUninit::write` -- undefined behaviour.
//!
//! ## The fix under test
//!
//! `HeapSlot::initialised: AtomicBool` is Release-stored to `true` by
//! `claim`/`claim_with_config` ONLY after `heap_ptr.write(hc)` completes.
//! The aggregation walks now Acquire-load `initialised` and `continue` past
//! any slot that has not (yet) published readiness -- see the doc comments
//! on `HeapSlot::initialised`, `tcache_hits_total`, and
//! `large_cache_hits_total` for the full happens-before argument.
//!
//! ## What this test actually checks
//!
//! A microsecond-scale race window inside `claim` cannot be hit
//! deterministically from a black-box integration test without internal
//! test hooks (there is no way to pause a claiming thread precisely between
//! its `generation.fetch_add` and `heap_ptr.write` from outside the
//! module). This test therefore does NOT prove "the race window is
//! unreachable" by itself -- it is a STRESS/regression test that:
//!
//! 1. Hammers `HeapRegistry::claim()` (which exercises the exact
//!    generation-bump-then-slow-`HeapCore::new`-then-write sequence) from
//!    many threads concurrently with many OTHER threads continuously
//!    calling the aggregation functions (`tcache_hits_total`,
//!    `large_cache_hits_total`) -- maximising the chance of landing inside
//!    the window on a real scheduler, and would reliably crash/corrupt
//!    under `cargo +nightly miri test` (which instruments every read for
//!    uninitialised-memory access and every racing non-atomic access) if
//!    the `initialised` gate were removed or the Release/Acquire pairing
//!    broken -- this is the PRIMARY value of this test; run it under miri
//!    to get a hard UB verdict, not just "didn't crash under debug/release".
//! 2. Asserts a NON-VACUOUS functional property that only holds if the
//!    gate correctly recognises every FULLY materialised heap and skips
//!    only genuinely uninitialised ones: after all claimer threads join,
//!    the aggregate must count at least as many magazine hits as the sum
//!    each individual heap observed locally (the same non-drop invariant
//!    `regression_percounter_perheap_aggregation.rs` checks in the
//!    non-racy case) -- a gate that skips a slot AFTER it was actually
//!    published (e.g. an inverted condition, or a Relaxed instead of
//!    Acquire load that lets the reader observe `initialised == true`
//!    without the HB edge and then read stale/torn `tcache` bytes) would
//!    make this assertion flaky-fail under this concurrent workload even
//!    though it passes reliably in the single-threaded tests.
//!
//! **Honesty note (per the task's counterfactual instructions):** the
//! race window this fix closes is narrow (a `HeapCore::new()` OS segment
//! reservation, microseconds) and not reliably reproducible as a
//! deterministic red/green counterfactual under a plain debug/release
//! build -- flipping the fix back out will not reliably make THIS test
//! fail on every run (it is a probabilistic stress test, not a
//! deterministic repro). The rigorous verification tool for this class of
//! bug is `cargo +nightly miri test --test regression_registry_initialised_gate`,
//! which detects the uninitialised-read/data-race directly rather than
//! relying on timing luck. See the task report for whether miri was run
//! and what it found in this environment.

#![cfg(feature = "alloc-global")]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

use sefer_alloc::registry::{bootstrap, heaps_claimed_high_water, HeapRegistry};

#[cfg(feature = "fastbin")]
use sefer_alloc::registry::tcache_hits_total;

#[cfg(feature = "alloc-decommit")]
use sefer_alloc::registry::large_cache_hits_total;

// Serialise against the other registry-touching test files in this crate
// (matches the discipline already used throughout `tests/`).
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

/// Number of concurrent claimer threads (each mints/claims a slot, does a
/// small amount of work, recycles). Kept small per the crate's "fast
/// profile" convention (CLAUDE.md): this is a smoke/stress check, not an
/// exhaustive concurrency fuzz (that is Phase 5 hardening).
const CLAIMER_THREADS: usize = 8;
/// Number of concurrent reader threads hammering the aggregation functions
/// while claimers are active -- these are the threads that would land in
/// the uninitialised-read window if the `initialised` gate were broken.
const READER_THREADS: usize = 4;
/// How many times each reader thread calls the aggregation functions.
const READER_ITERS: usize = 2000;

#[cfg(feature = "fastbin")]
fn drive_claim_and_return_local_hits() -> u64 {
    let heap = HeapRegistry::claim();
    assert!(
        !heap.is_null(),
        "HeapRegistry::claim returned null under contention"
    );

    // SAFETY: `heap` is live, just claimed by this thread, not yet recycled.
    let local_before = unsafe { (*heap).tcache_hits() };

    let layout = Layout::from_size_align(96, 32).unwrap();
    for _ in 0..8 {
        let p1 = unsafe { (*heap).alloc(layout) };
        if p1.is_null() {
            break; // benign under OOM-constrained CI runners
        }
        unsafe { (*heap).dealloc(p1, layout) };
        let p2 = unsafe { (*heap).alloc(layout) };
        if !p2.is_null() {
            unsafe { (*heap).dealloc(p2, layout) };
        }
    }

    // SAFETY: still live, not yet recycled.
    let local_after = unsafe { (*heap).tcache_hits() };
    unsafe { HeapRegistry::recycle(heap) };
    local_after.saturating_sub(local_before)
}

#[cfg(not(feature = "fastbin"))]
fn drive_claim_and_return_local_hits() -> u64 {
    let heap = HeapRegistry::claim();
    assert!(
        !heap.is_null(),
        "HeapRegistry::claim returned null under contention"
    );
    let layout = Layout::from_size_align(96, 32).unwrap();
    for _ in 0..8 {
        let p = unsafe { (*heap).alloc(layout) };
        if p.is_null() {
            break;
        }
        unsafe { (*heap).dealloc(p, layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
    0
}

/// Stress the exact race window `HeapSlot::initialised` closes: many
/// threads racing `claim()` (which walks generation-bump →
/// `HeapCore::new()` → `heap_ptr.write` → `initialised` publish) while
/// other threads continuously call the aggregation functions that
/// dereference `heap` across every minted slot.
///
/// Under `cargo +nightly miri test`, a broken/removed `initialised` gate
/// would manifest as a hard miri error (read of uninitialised bytes, or a
/// detected data race on the `MaybeUninit` write) rather than a silent
/// pass -- see the module doc comment's honesty note on why this is not a
/// deterministic red/green counterfactual under plain debug/release.
#[test]
fn concurrent_claim_and_aggregate_no_uninit_read() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let stop = Arc::new(AtomicBool::new(false));
    let reader_calls = Arc::new(AtomicU64::new(0));

    // Reader threads: hammer the aggregation walk while claimers are
    // actively minting/materialising fresh slots. Each call walks
    // `0..count`, gating every slot on `initialised` before dereferencing
    // `heap` -- exactly the code path under test.
    let mut readers = Vec::with_capacity(READER_THREADS);
    for _ in 0..READER_THREADS {
        let stop = Arc::clone(&stop);
        let reader_calls = Arc::clone(&reader_calls);
        readers.push(thread::spawn(move || {
            let mut iters = 0usize;
            while !stop.load(Ordering::Relaxed) && iters < READER_ITERS {
                #[cfg(feature = "fastbin")]
                {
                    let _ = tcache_hits_total();
                }
                #[cfg(feature = "alloc-decommit")]
                {
                    let _ = large_cache_hits_total();
                }
                let _ = heaps_claimed_high_water();
                reader_calls.fetch_add(1, Ordering::Relaxed);
                iters += 1;
            }
        }));
    }

    // Claimer threads: each claims a FRESH slot (bump_count path, the one
    // that actually exercises `HeapCore::new()`'s OS reservation -- the
    // window this test targets), does a little work, recycles.
    let mut claimers = Vec::with_capacity(CLAIMER_THREADS);
    for _ in 0..CLAIMER_THREADS {
        claimers.push(thread::spawn(drive_claim_and_return_local_hits));
    }

    let mut total_local_hits: u64 = 0;
    for c in claimers {
        total_local_hits =
            total_local_hits.saturating_add(c.join().expect("claimer thread panicked"));
    }

    stop.store(true, Ordering::Relaxed);
    for r in readers {
        r.join().expect("reader thread panicked");
    }

    // Non-vacuous functional check: readers must have actually run
    // (otherwise this test proves nothing about the race window).
    assert!(
        reader_calls.load(Ordering::Relaxed) > 0,
        "reader threads never ran a single aggregation call -- test is vacuous"
    );

    // The aggregation, called AFTER all claimers joined (so every claimed
    // slot is guaranteed materialised and `initialised == true` by now),
    // must not have dropped any live heap's contribution -- same
    // non-drop property as `regression_percounter_perheap_aggregation.rs`,
    // now checked after a genuinely concurrent claim/read workload rather
    // than a purely sequential one.
    #[cfg(feature = "fastbin")]
    {
        let final_total = tcache_hits_total();
        assert!(
            final_total >= total_local_hits,
            "process-wide tcache_hits_total() ({final_total}) is less than \
             the sum of what claimer threads observed locally ({total_local_hits}) \
             after a concurrent claim/aggregate race -- the initialised gate \
             is dropping a materialised heap's contribution"
        );
    }
}
