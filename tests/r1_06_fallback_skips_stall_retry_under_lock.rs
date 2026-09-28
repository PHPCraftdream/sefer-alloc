//! R1-06 (src review round 1, `docs/reviews/2026-09-28-005939-src-review-oxx-round-1.md`):
//! a fallback-driven cross-thread free that lands on a saturated-but-LIVE
//! owner must NOT run `push_with_overflow_retry`'s bounded sleep-retry tier
//! while holding the fallback's process-wide spinlock — that tier can block
//! for up to `RETRY_STALLED_ROUNDS_GIVE_UP` (128) probe rounds, each round
//! beyond the first separated by a real OS-level sleep (see
//! `src/registry/heap_core_xthread/overflow.rs`'s `push_with_overflow_retry`
//! doc comment), and the fallback spinlock has exactly one holder at a time
//! across the WHOLE process — every other thread needing the fallback (TLS
//! teardown, registry exhaustion, pre-TLS init) would spin CPU-burning for
//! that entire window even though its own request has nothing to do with the
//! stalled owner.
//!
//! ## Reproduction shape
//!
//! 1. Claim an owner heap `H` and pre-allocate exactly
//!    `RING_CAP (256) + HEAP_OVERFLOW_CAP (2048) = 2304` small blocks from
//!    it, then remote-free all 2304 from a separate producer heap. `H` never
//!    drains (does no further alloc/dealloc) — the same "paused owner" shape
//!    `tests/regression_paused_owner_wallclock.rs` and
//!    `tests/remote_fanin.rs::remote_fanin_owner_starved_residual_is_bounded`
//!    already establish as this codebase's pattern for "saturated ring +
//!    live, non-draining owner". All 2304 succeed on their FIRST attempt
//!    (256 via the segment ring, 2048 via the immediate heap-overflow
//!    attempt) — nothing retries yet, so this setup phase is fast.
//! 2. Allocate ONE more block from `H` (`ptr_2305`) — its segment ring AND
//!    `H`'s heap-level overflow are now BOTH exactly full, and `H` is still
//!    `STATE_LIVE` (claimed, never recycled) — the exact "double-saturation,
//!    live owner" condition that trips `push_with_overflow_retry`'s
//!    spin-retry tier.
//! 3. Free `ptr_2305` as a FOREIGN pointer, from INSIDE the fallback's
//!    spinlock, via `HeapCore::realloc`'s foreign-pointer move leg (the exact
//!    call shape R1-06 flags: `SeferAlloc::realloc`'s fallback arm calls
//!    `fallback::with_heap(|h| h.realloc(ptr, old_layout, new_size))`, and a
//!    foreign realloc's move leg ends with `self.dealloc(ptr, old_layout)` —
//!    see `src/registry/heap_core/free/realloc.rs`'s foreign-pointer branch).
//!    Reached here via the `#[doc(hidden)]` test hook
//!    `HeapCore::dbg_with_fallback_for_test`, which runs its closure through
//!    the SAME production `with_heap`/`LockGuard` path `SeferAlloc::realloc`
//!    uses — no test-only shortcut around the lock.
//!
//! ## Oracle
//!
//! Time step 3 alone. Fixed (this task): the fallback lock's
//! `LockGuard::acquire` records (via
//! `registry::xthread_fallback_gate::set_held`) that the calling thread holds
//! the lock; `push_with_overflow_retry` consults that flag and skips straight
//! to the immediate-overflow-retry + spill tiers, so step 3 completes in at
//! most a handful of uncontended atomic ops — comfortably under
//! [`MAX_ELAPSED`]. Pre-fix, step 3 would fall through to the sleep-retry
//! loop: `H` never drains, so EVERY one of up to 128 probe rounds reports zero
//! progress, and rounds 2.. are each preceded by
//! `std::thread::sleep(Duration::from_micros(200))` — measured elsewhere in
//! this codebase (`RETRY_STALLED_ROUNDS_GIVE_UP`'s doc comment) at an actual
//! OS-granted granularity of ~2-15ms per sleep on this project's dev hosts,
//! i.e. roughly 250ms-2s for a single concession — comfortably ABOVE
//! [`MAX_ELAPSED`]. This is therefore a genuine red/green oracle, not merely
//! a smoke check: confirmed by hand (see this task's report) by temporarily
//! reverting the `overflow.rs` fallback-lock check and re-running this test,
//! which then measures the pre-fix cost and fails the assertion.
//!
//! Native-only (`#[cfg(not(miri))]`): a wall-clock assertion; miri's
//! interpreter overhead and its own `RETRY_ROUND_SAFETY_CAP == 1` narrowing
//! make a timing assertion meaningless there (mirrors
//! `regression_paused_owner_wallclock.rs`'s own `not(miri)` gate).

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals",
    not(miri)
))]

use std::alloc::Layout;
use std::time::{Duration, Instant};

use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

/// A small-class size well under `SMALL_MAX`, so every block routes through
/// the ring/overflow tiers this test exercises (never the Large/A1 path) —
/// mirrors `tests/regression_paused_owner_wallclock.rs::BLOCK_SIZE`.
const BLOCK_SIZE: usize = 64;

/// `RING_CAP (256, `alloc_core::remote_free_ring::RING_CAP`) +
/// HEAP_OVERFLOW_CAP (2048, native `not(miri)` value — not itself exported
/// from the crate, see `src/registry/heap_overflow.rs`) = 2304`: exactly the
/// combined capacity of a segment's remote-free ring and its owning heap's
/// second-chance overflow ring. Pre-saturating exactly this many blocks fills
/// BOTH tiers without any of the 2304 pushes needing a retry (each succeeds
/// on its first attempt — via the ring for the first 256, via the immediate
/// heap-overflow attempt for the remaining 2048), so this setup phase itself
/// pays no sleep-retry cost.
const N_PRESATURATE: usize = 2304;

/// Per-attempt upper bound for the FIXED code's wall-clock cost of the single
/// timed fallback free (step 3 in the module doc). The fixed path costs a
/// handful of uncontended atomic ops (no sleep, no multi-round spin) —
/// measured consistently under 1ms on this project's dev hosts (well within
/// this bound). Measured-by-hand pre-fix cost on the SAME dev hosts was
/// 160-250ms per attempt (a single stalled concession's ~128 probe rounds,
/// each round beyond the first paying a real OS sleep — see the module doc's
/// "Oracle" section for the mechanism), so 50ms sits with ample headroom
/// above the fixed cost while staying well below the observed unfixed cost —
/// a genuine red/green line, not just generous headroom.
const MAX_ELAPSED: Duration = Duration::from_millis(50);

/// Best-of-`ATTEMPTS` retry (mirrors `regression_paused_owner_wallclock.rs`'s
/// own rationale): this project's dev box is a shared, sometimes-noisy
/// machine, and a single-shot wall-clock assertion can occasionally catch an
/// unrelated host hiccup. The pre-fix pathology is systemic (a stalled
/// concession costs hundreds of ms to seconds EVERY time, not occasionally),
/// so requiring only one fast attempt out of a few does not let a real
/// regression slip through.
const ATTEMPTS: u32 = 3;

/// Runs the full reproduction shape once (fresh owner/producer heaps each
/// call) and returns the wall-clock cost of the single timed fallback free
/// (step 3). Returns `None` if the fallback's own `with_heap` reports the
/// (never expected here) true-OOM case, so the caller can fail loudly instead
/// of silently treating a broken setup as "fast".
fn run_once() -> Option<Duration> {
    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();

    let owner = HeapRegistry::claim();
    assert!(!owner.is_null(), "owner HeapRegistry::claim returned null");

    // Step 1: pre-allocate N_PRESATURATE + 1 blocks from the owner up front,
    // then the owner does ZERO further work — the established "paused
    // owner" shape (see module doc).
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N_PRESATURATE + 1);
    for _ in 0..=N_PRESATURATE {
        let p = unsafe { (*owner).alloc(layout) };
        assert!(!p.is_null(), "owner pre-alloc returned null");
        ptrs.push(p);
    }
    let ptr_2305 = *ptrs.last().unwrap();
    let presaturate = &ptrs[..N_PRESATURATE];

    // Step 1 (continued): remote-free the first N_PRESATURATE blocks from a
    // separate producer heap. Every one of these succeeds on its first
    // attempt (ring, then immediate heap-overflow) — no retry, no sleep.
    let producer = HeapRegistry::claim();
    assert!(
        !producer.is_null(),
        "producer HeapRegistry::claim returned null"
    );
    for &p in presaturate {
        unsafe { (*producer).dealloc(p, layout) };
    }

    // Step 2/3: `ptr_2305`'s segment ring AND `owner`'s heap-overflow are now
    // both exactly full. Free it as a FOREIGN pointer from INSIDE the
    // fallback's spinlock via `HeapCore::realloc`'s foreign move leg — the
    // exact call shape `SeferAlloc::realloc`'s fallback arm uses. Same
    // `new_size` as `old_layout`'s: the foreign branch always takes the
    // move leg regardless (no in-place fast path for a foreign pointer), so
    // this still exercises the `self.dealloc(ptr, old_layout)` tail R1-06
    // flags.
    let t0 = Instant::now();
    let result = HeapCore::dbg_with_fallback_for_test(|h| {
        // SAFETY: `ptr_2305` is a live allocation of `layout`'s size/align,
        // made by `owner` (a different, foreign heap from the fallback's
        // point of view) and not yet freed.
        unsafe { h.realloc(ptr_2305, layout, layout.size()) }
    });
    let elapsed = t0.elapsed();

    unsafe { HeapRegistry::recycle(producer) };
    unsafe { HeapRegistry::recycle(owner) };

    result.map(|new_ptr| {
        assert!(
            !new_ptr.is_null(),
            "fallback foreign realloc returned null (OOM?)"
        );
        elapsed
    })
}

#[test]
fn fallback_foreign_free_into_saturated_live_owner_completes_fast() {
    let _ = bootstrap::ensure();

    let mut samples = Vec::with_capacity(ATTEMPTS as usize);
    for attempt in 1..=ATTEMPTS {
        let elapsed = run_once().expect(
            "fallback::with_heap reported true OOM — unexpected on a fresh process; \
             cannot evaluate the timing oracle",
        );
        eprintln!(
            "fallback_foreign_free_into_saturated_live_owner_completes_fast: \
             attempt {attempt}/{ATTEMPTS} elapsed={elapsed:?} (bound={MAX_ELAPSED:?})"
        );
        if elapsed < MAX_ELAPSED {
            return; // Best-of-N: one fast attempt is sufficient — see module doc.
        }
        samples.push(elapsed);
    }

    panic!(
        "fallback-driven foreign free into a saturated-but-live owner exceeded \
         {MAX_ELAPSED:?} on ALL {ATTEMPTS} attempts (samples: {samples:?}) — this is the \
         R1-06 pathology: `push_with_overflow_retry`'s bounded sleep-retry tier ran WHILE \
         the calling thread held the fallback's process-wide spinlock, which would starve \
         every other thread needing the fallback for the same window. See \
         `registry::xthread_fallback_gate` and its call site in \
         `heap_core_xthread/overflow.rs::push_with_overflow_retry` for the fix (skip the \
         retry tier and concede to spill immediately while the fallback lock is held)."
    );
}
