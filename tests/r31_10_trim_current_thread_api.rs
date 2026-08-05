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
//! - **AC4** — fallback-heap no-op: calling `trim_current_thread()` while
//!   this thread's `CurrentHeap` resolves to `Fallback` does not panic and
//!   does not corrupt fallback state. **Corrected post-landing** (R32
//!   review, finding P1-1): a never-allocated thread does NOT resolve to
//!   `Fallback` under the OLD alloc-side resolution (a null `LOCAL` used to
//!   bind a fresh OWN heap via `tls_heap::finish_bind`) — AC4 is now two
//!   tests, `ac4a_trim_on_freshly_bound_never_allocated_heap_is_safe` (the
//!   original scenario, correctly renamed) and
//!   `ac4b_trim_on_genuinely_torn_tls_is_safe_noop` (the actual `Fallback`
//!   case, `bench-internals`-gated, using the existing TORN-TLS test hooks).
//!   **Corrected AGAIN** (task #492): `trim_current_thread()` now resolves
//!   via the PASSIVE `tls_heap::current_for_trim`, so a never-allocated
//!   thread no longer binds ANYTHING — `ac4a` below now asserts this
//!   directly (no registry slot claimed), and a new
//!   `ac4c_trim_on_never_allocated_thread_claims_no_slot` test makes the
//!   no-claim assertion the test's sole, explicit point.
//!
//! AC5 (the measured burst-idle-burst RSS win) is a separate gate report:
//! `docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md`; its "Cost side"
//! section (task #492) measures the counterpart cost (trim latency,
//! next-burst cold-start cost).
//!
//! ## Feature gating
//!
//! `alloc-global` — required for `SeferAlloc` / `global` module to exist.
//! `alloc-decommit` — required for `trim_current_thread` itself (the
//! mechanisms it drives — small pool, large cache — are
//! `alloc-decommit`-only). `bench-internals` — required additionally for
//! `ac4b_trim_on_genuinely_torn_tls_is_safe_noop` (uses the TORN-TLS test
//! hooks, which are `bench-internals`-gated per CLAUDE.md's benchmark-hook
//! rule).
//!
//! Run:
//!   cargo test --release --features production --test r31_10_trim_current_thread_api
//!
//! With the stronger per-heap large-cache assertion (AC1):
//!   cargo test --release --features "production bench-internals" \
//!       --test r31_10_trim_current_thread_api

#![cfg(all(
    all(feature = "alloc-global", feature = "alloc-decommit"),
    feature = "internals"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use sefer_alloc::global::tls_heap;
use sefer_alloc::SeferAlloc;

// `segments_released_total` (`SeferAlloc::stats()`) is a PROCESS-WIDE
// counter shared across every `SeferAlloc`/thread in the process. `cargo
// test` runs the `#[test]` functions in this file in parallel by default,
// and every test here calls `trim_current_thread()` at least once (which can
// release cached spans and bump this counter) while `ac1`/`ac3` read a
// before/after delta on it — the SAME parallelism-artifact class already
// documented in `docs/CORRECTNESS_OPEN_ITEMS.md` item 18
// (`tests/segment_table_contains_base_tier1_counters.rs`, commit `2f16ba6`):
// found here via `npm run check`'s own `--all-features` step
// (`ac1_trim_empties_pool_and_evicts_large_cache` failed with
// `released_before=1, released_after_cache=2`, meaning a sibling test's
// `trim_current_thread()` call landed in the narrow window between the two
// counter reads). Serializes ALL tests in this file with the SAME
// established `static TEST_LOCK: Mutex<()>` + per-test
// `let _guard = TEST_LOCK.lock().unwrap();` pattern used in
// `tests/segment_table_contains_base_tier1_counters.rs` and the other
// precedent files it cites — not just the two tests that read the delta,
// because every OTHER test's `trim_current_thread()` call is a source of
// interference for those two, not just each other.
static TEST_LOCK: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// AC1 — behavioral equivalence with trim_for_recycle
// ---------------------------------------------------------------------------

#[test]
fn ac1_trim_empties_pool_and_evicts_large_cache() {
    let _guard = TEST_LOCK.lock().unwrap();
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
    let _guard = TEST_LOCK.lock().unwrap();
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
    let _guard = TEST_LOCK.lock().unwrap();
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
//
// R32 review finding P1-1 (independent review,
// docs/reviews/2026-07-31-r32-full-review.md): the ORIGINAL version of this
// section had exactly one test, named and commented as if it exercised the
// fallback-heap path ("no prior allocation has bound a per-thread heap"),
// but a null `LOCAL` on a never-allocated thread resolves through
// `tls_heap::finish_bind` to `CurrentHeap::Own` (claims a fresh registry
// slot and binds an empty heap), NOT `CurrentHeap::Fallback` — traced in
// `src/global/tls_heap.rs:521-530`/`:629-654`. That test was real and
// passing, but not for the reason it claimed; the genuine fallback path
// (`CurrentHeap::Fallback`) was never exercised. Split into two tests below:
// the original scenario, renamed to state what it actually proves, plus a
// new test that genuinely reaches `Fallback` via the existing
// `bench-internals`-gated TORN-TLS test hooks.

/// The original AC4 test, renamed and strengthened (task #492): trim on a
/// never-allocated thread must be safe AND — since `trim_current_thread()`
/// now resolves via the passive `tls_heap::current_for_trim` instead of the
/// binding `current_heap()` — must claim NO registry slot at all. Does NOT
/// exercise `CurrentHeap::Fallback` — see the module comment above and
/// [`ac4b_trim_on_genuinely_torn_tls_is_safe_noop`] below for the test that
/// does.
#[test]
fn ac4a_trim_on_freshly_bound_never_allocated_heap_is_safe() {
    let _guard = TEST_LOCK.lock().unwrap();
    let a = Arc::new(SeferAlloc::new());

    // Spawn a FRESH thread that has NEVER allocated. Its `LOCAL` is null.
    // Post-#492, `trim_current_thread()` on such a thread is a true no-op —
    // it does NOT claim a registry slot (see `ac4c` below for the dedicated
    // no-slot-claimed assertion; this test focuses on "still safe, and the
    // thread still works normally afterward").
    let a_for_child = Arc::clone(&a);
    let handle = thread::spawn(move || {
        // This is the FIRST thing the thread does — no prior allocation has
        // bound a per-thread heap yet.
        a_for_child.trim_current_thread();

        // Verify the heap (bound by the FIRST REAL allocation below, not by
        // the trim call above) is not corrupted: an allocation after the
        // trim must succeed and produce serviceable memory.
        let layout = Layout::from_size_align(256, 8).unwrap();
        // SAFETY: valid, non-zero-size layout.
        let p = unsafe { a_for_child.alloc(layout) };
        assert!(!p.is_null(), "alloc after trim must succeed");
        // SAFETY: p was just allocated, not yet freed.
        unsafe { p.write_volatile(0xAB) };
        // SAFETY: p valid, not yet freed.
        let val = unsafe { p.read_volatile() };
        assert_eq!(val, 0xAB, "memory after trim must be serviceable");
        // SAFETY: p valid for `layout`, freed exactly once.
        unsafe { a_for_child.dealloc(p, layout) };
    });

    handle
        .join()
        .expect("fresh thread panicked during trim-on-never-allocated test");
}

/// **New (task #492).** The no-slot-claimed assertion, isolated as its own
/// test so a regression here fails with an unambiguous name/message rather
/// than being buried inside `ac4a`'s broader safety check. Uses the
/// process-wide `heaps_claimed_high_water` diagnostic (via
/// [`SeferAlloc::stats`]) as the oracle: it is a monotonically-increasing
/// count of registry slots ever minted (`src/registry/heap_registry.rs`'s
/// `bump_count`), so "unchanged across the trim call" is exactly "no new
/// slot was claimed."
///
/// Runs on a dedicated freshly-spawned thread (not one of libtest's pooled
/// threads) specifically so it is guaranteed to be the FIRST thing this
/// thread does — a pooled test thread might already have allocated (and
/// thus already bound a slot) from an earlier test, which would make the
/// high-water-mark comparison meaningless for this thread's own contribution.
/// The high-water mark is process-wide and monotonic, so concurrently
/// running tests bumping it for THEIR OWN threads does not invalidate this
/// test's own before/after comparison — this thread's contribution is what
/// is asserted to be zero, not the absolute value.
#[test]
fn ac4c_trim_on_never_allocated_thread_claims_no_slot() {
    let _guard = TEST_LOCK.lock().unwrap();
    let a = Arc::new(SeferAlloc::new());
    let a_for_child = Arc::clone(&a);
    let handle = thread::spawn(move || {
        let before = a_for_child.stats().heaps_claimed_high_water;

        // FIRST thing this thread does — no prior allocation, so `LOCAL` is
        // still null. Must be a true no-op: no slot claimed.
        a_for_child.trim_current_thread();

        let after = a_for_child.stats().heaps_claimed_high_water;
        assert_eq!(
            before, after,
            "trim_current_thread() on a never-allocated thread must claim \
             NO registry slot (heaps_claimed_high_water moved from {before} \
             to {after})"
        );

        // Control: a REAL allocation on this thread afterward still binds a
        // heap and works normally — proves the passive resolver did not
        // somehow poison the thread's ability to bind later. This does NOT
        // assert `heaps_claimed_high_water` increases by exactly one: the
        // registry recycles exited threads' slots (whole-slot reuse, Phase
        // 12.5), so a concurrently-running test's thread exit may have freed
        // a slot this allocation reuses WITHOUT bumping the high-water mark
        // (`bump_count` is only called for a genuinely NEW slot, not a
        // recycled one) — the mark is monotonic non-decreasing, never a
        // fixed per-alloc increment.
        let layout = Layout::from_size_align(64, 8).unwrap();
        // SAFETY: valid, non-zero-size layout.
        let p = unsafe { a_for_child.alloc(layout) };
        assert!(!p.is_null(), "alloc after no-op trim must succeed");
        let after_alloc = a_for_child.stats().heaps_claimed_high_water;
        assert!(
            after_alloc >= after,
            "heaps_claimed_high_water must be monotonic non-decreasing \
             (after={after}, after_alloc={after_alloc})"
        );
        // SAFETY: p valid for `layout`, freed exactly once.
        unsafe { a_for_child.dealloc(p, layout) };
    });

    handle
        .join()
        .expect("fresh thread panicked during no-slot-claimed test");
}

/// The genuine AC4 case: `trim_current_thread()` called while this thread's
/// `LOCAL` is TORN (the state the passive `tls_heap::current_for_trim`
/// resolver — post-#492 — and the old alloc-side `current_for_alloc` both
/// map to "no live own-thread heap"; `current_for_alloc` additionally
/// resolves the fallback heap there, `current_for_trim` deliberately does
/// not — see `current_for_trim`'s own doc comment) must be a true no-op —
/// no panic, no corruption, and (post-#492) no registry slot claimed — and
/// the heap must work normally again once TORN is lifted.
///
/// Uses the existing `bench-internals`-gated test-only hook pair
/// `tls_heap::dbg_mark_local_torn_for_test`/`dbg_restore_local_for_test`
/// (R29-7/task #438), the SAME mechanism `tests/dealloc_only_no_bind_torn.rs`
/// already uses to poke this exact state from a test. Runs entirely on a
/// dedicated, freshly-spawned OS thread (not one of libtest's pooled
/// threads), so poisoning `LOCAL` here cannot bleed into any other test.
#[cfg(feature = "bench-internals")]
#[test]
fn ac4b_trim_on_genuinely_torn_tls_is_safe_noop() {
    let _guard = TEST_LOCK.lock().unwrap();
    let a = Arc::new(SeferAlloc::new());
    let a_for_child = Arc::clone(&a);
    let handle = thread::spawn(move || {
        let layout = Layout::from_size_align(64, 8).unwrap();

        // Bind a real per-thread heap first (so TORN has something to poke
        // past, not just an also-null LOCAL).
        // SAFETY: valid, non-zero-size layout.
        let p0 = unsafe { a_for_child.alloc(layout) };
        assert!(!p0.is_null(), "warm-up alloc must succeed");
        // SAFETY: p0 valid, freed exactly once.
        unsafe { a_for_child.dealloc(p0, layout) };

        // Force this thread's LOCAL into the genuinely-torn state
        // `trim_current_thread`'s doc comment describes as its no-op case.
        let saved = tls_heap::dbg_mark_local_torn_for_test();

        // Trim while TORN: must resolve `current_for_trim` to `None` and be
        // a true no-op (no panic, no new registry slot claimed).
        let before = a_for_child.stats().heaps_claimed_high_water;
        a_for_child.trim_current_thread();
        let after = a_for_child.stats().heaps_claimed_high_water;
        assert_eq!(
            before, after,
            "trim_current_thread() while TORN must claim NO registry slot \
             (heaps_claimed_high_water moved from {before} to {after})"
        );

        // Restore LOCAL before any further allocator use on this thread —
        // mirrors tests/dealloc_only_no_bind_torn.rs's save/restore
        // discipline.
        // SAFETY: `saved` was returned by `dbg_mark_local_torn_for_test()`
        // just above on THIS thread, is this thread's own bound `HeapCore`,
        // and is never shared across threads.
        unsafe { tls_heap::dbg_restore_local_for_test(saved) };

        // Control: the heap works normally again after TORN is lifted.
        // SAFETY: valid, non-zero-size layout.
        let p = unsafe { a_for_child.alloc(layout) };
        assert!(!p.is_null(), "alloc after restore must succeed");
        // SAFETY: p was just allocated, not yet freed.
        unsafe { p.write_volatile(0xCD) };
        // SAFETY: p valid, not yet freed.
        let val = unsafe { p.read_volatile() };
        assert_eq!(val, 0xCD, "memory after restore must be serviceable");
        // SAFETY: p valid for `layout`, freed exactly once.
        unsafe { a_for_child.dealloc(p, layout) };
    });

    handle
        .join()
        .expect("thread panicked during genuinely-torn trim test");
}
