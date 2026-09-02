//! Real-OS-thread free-list conservation test — the multi-threaded coverage
//! `loom_aba.rs` structurally cannot provide.
//!
//! `loom_aba.rs` is exhaustive but tiny: every model there spawns at most 2
//! threads over a 1-2-slot backing (`const N: usize = 2` there), and loom
//! explores every interleaving of that small model rather than running real
//! time. That bound also caps how deep the CAS-retry backoff's `spins`
//! counter (`src/imp.rs`, `push_index`/`pop_index`) can climb inside any loom model — a
//! handful of interleavings can force at most a couple of retries, nowhere
//! near `BACKOFF_SPIN_CAP` (6). This file exercises the OPPOSITE regime: a
//! fixed, modest number of REAL OS threads hammering a SHARED stack for many
//! iterations, so genuine contention forces many CAS retries per call, and
//! `spins` genuinely climbs into its higher range in practice — something no
//! existing test (loom or otherwise) does. That two-part claim is ASSERTED,
//! not assumed — at two levels. First, the test snapshots the crate's retry
//! counters (`retry_counts_for_test()`, the non-loom twin of the loom
//! suite's per-counter accessors) before the threaded phase and asserts
//! both the pop and push counters advanced after it, so a regression that
//! makes the retry PATH unreachable fails loudly instead of passing
//! vacuously — but that level alone cannot distinguish 1 retry from
//! thousands. Second, it does the same with the
//! backoff-cap-reach counters (`backoff_cap_reached_for_test()`), which
//! count only retries whose spin loop ran at FULL depth (`spins` saturated
//! at `BACKOFF_SPIN_CAP`), so a regression that caps `spins` at 0, resets
//! it per iteration, or moves its increment off the reachable path —
//! leaving the documented backoff silently inert while every retry counter
//! still advances — fails loudly too. The counters compile only under the
//! crate's `test-internals` feature (a default build of the
//! crate carries no instrumentation), so WITHOUT that feature this file
//! still runs its conservation and drain checks but the oracle assertions
//! compile out; run
//! `cargo test -p tagged-index-stack --features test-internals` for the
//! full-strength oracle. This is the committed replacement for the
//! throwaway ad hoc probe that originally measured the backoff —
//! "8 threads x 200,000 contention-shaped pop/push iterations under the
//! backoff, then drained and confirmed the exact multiset 0..64 came back",
//! a probe that was never committed. This file runs at that exact
//! 8 x 200,000 shape.
//!
//! Discipline mirrors `benches/tagged_index_stack_bench.rs`'s
//! `contention/churn` phase exactly: every thread pops WHATEVER is currently
//! on top (which may be another thread's index, under real contention) and
//! immediately re-pushes EXACTLY that value — never a locally invented index.
//! Re-pushing anything else violates `push_index`'s documented caller contract
//! ("index must NOT already be reachable from ANY stack that reads and
//! writes the same link cells") and would corrupt
//! the free-list independent of any bug this test exists to catch.
//!
//! Not a loom model (`#![cfg(not(loom))]`) — this is a normal `cargo test`
//! file exercising real OS threads at real scale, which loom cannot do (loom
//! replaces `std`'s atomics/threads with its own model-checked stand-ins and
//! only explores a small bounded state space).

#![cfg(not(loom))]

use std::thread;

use tagged_index_stack::ArrayIndexStack;

/// Same width as the bench and the rest of this crate's test suite; the fused
/// `ArrayIndexStack` owns its head and its `ArrayLinks` links together.
type Stack = ArrayIndexStack<16, { LINKS_SIZE as usize }>;

/// Number of indices in the `ArrayLinks` backing store, and the exact
/// multiset seeded onto the stack before the threaded phase. Kept modest per
/// CLAUDE.md's "Speed: short scenario by default" convention.
const LINKS_SIZE: u32 = 64;

/// Real OS threads racing the shared stack concurrently.
const NUM_THREADS: usize = 8;

/// Pop-then-repush iterations per thread. `NUM_THREADS * ITERS_PER_THREAD`
/// (1.6M total pop/push pairs) matches the 8-threads-x-200,000 shape of the
/// throwaway ad hoc probe this file replaces. Scale alone does NOT enforce
/// the retry/backoff claims — the two-level activation-oracle assertions
/// below (snapshot of `retry_counts_for_test()` AND
/// `backoff_cap_reached_for_test()` before the threaded phase, non-zero
/// delta after, both under `--features test-internals`) do; the scale just
/// makes genuine contention — and therefore retry activation at full
/// backoff depth — routine rather than a rare race. Measured cost: runs
/// in ~0.3 s under a debug `cargo test` (~0.1 s release).
const ITERS_PER_THREAD: u32 = 200_000;

/// N threads x M iterations of pop-then-immediately-repush-exactly-what-you-
/// popped against a shared, prefilled stack, followed by a full drain and an
/// exact-multiset check: the classic Treiber free-list conservation property
/// (no index lost, none duplicated) under REAL contention.
#[test]
fn conservation_under_real_thread_contention() {
    let stack = Stack::new();

    // Prefill a fresh (already-empty) stack with 0..LINKS_SIZE -- mirrors the
    // bench's `contention/churn` prefill discipline. No drain-first needed:
    // `Stack::new()` starts empty (RAD-1 lazy links).
    for i in 0..LINKS_SIZE {
        // SAFETY: fresh stack (domain 0..LINKS_SIZE); each index is in-domain and pushed exactly once.
        unsafe { stack.push(i) };
    }

    // Activation oracle (the first committed multi-threaded oracle for real
    // contention; the instrumentation counters exist only under the
    // `test-internals` feature): the whole point
    // of this file over `loom_aba.rs` is real-contention retry activation,
    // so ASSERT it at two levels — the retry counters prove the retry arms
    // are REACHED, the backoff-cap-reach counters prove `spins` genuinely
    // climbs into its higher range (at least one call per branch executed
    // its spin loop at full depth). The counters exist only under the
    // crate's `test-internals` feature (or a loom build, which this file is
    // `#![cfg(not(loom))]`-excluded from), so without the feature the
    // conservation and drain checks below still run but these oracle
    // assertions compile out — run this file with
    // `cargo test -p tagged-index-stack --features test-internals` for the
    // full-strength version. One #[test] per binary here, so the window
    // needs no MODEL_LOCK-style serialization (see `retry_counts_for_test`'s
    // doc).
    #[cfg(feature = "test-internals")]
    let (pop_retries_before, push_retries_before) = tagged_index_stack::retry_counts_for_test();
    #[cfg(feature = "test-internals")]
    let (pop_cap_before, push_cap_before) = tagged_index_stack::backoff_cap_reached_for_test();

    // Start-rendezvous barrier: without it, `s.spawn()` merely SCHEDULES a
    // thread, it does not synchronize its start against its siblings. On a
    // CI runner with few real cores (observed live: GitHub Actions'
    // `ubuntu-latest`), thread creation can be slow enough relative to this
    // loop's tiny per-iteration cost that early threads run a large chunk of
    // their 200,000 iterations before a later thread is even scheduled for
    // the first time -- collapsing what should be 8-way real contention into
    // several near-sequential runs with little to no overlap. That is
    // exactly what happened in CI run 33508623598 (job 99858613637,
    // 2026-09-01): `push`'s CAS-retry branch fired ZERO times across all
    // 1.6M iterations (before=0, after=0) -- the activation oracle below
    // caught it as designed, because a staggered start against a shared
    // stack still conserves the free-list (no thread ever needs a SECOND
    // concurrent writer to stay correct), so only the oracle -- not the
    // conservation check -- can tell "ran without contention" apart from
    // "the retry path is broken". `NUM_THREADS + 1` participants (the
    // workers plus this scope's own thread) release everyone into the
    // contended loop at approximately the same instant, the same fix shape
    // `benches/tagged_index_stack_bench.rs`'s contention phases already use
    // for their own published-timing-window rendezvous.
    let start_barrier = std::sync::Barrier::new(NUM_THREADS + 1);

    thread::scope(|s| {
        let stack = &stack;
        let start_barrier = &start_barrier;
        for _ in 0..NUM_THREADS {
            s.spawn(move || {
                start_barrier.wait();
                for _ in 0..ITERS_PER_THREAD {
                    // Pop whatever is currently on top (may belong to any
                    // thread under contention) and immediately re-push
                    // EXACTLY that value -- never a locally invented index.
                    let idx = stack.pop().expect(
                        "stack unexpectedly empty: with LINKS_SIZE prefilled \
                         indices and at most NUM_THREADS held outstanding at \
                         once, the stack can never observe fewer than \
                         LINKS_SIZE - NUM_THREADS elements",
                    );
                    // SAFETY: idx was JUST returned by pop, so it is not live; in-domain by construction.
                    unsafe { stack.push(idx) };
                }
            });
        }
        // Release all NUM_THREADS workers into their contended loop at
        // approximately the same instant (see the barrier's own doc comment
        // above for why this rendezvous is load-bearing, not cosmetic).
        start_barrier.wait();
    });

    #[cfg(feature = "test-internals")]
    {
        let (pop_retries_after, push_retries_after) = tagged_index_stack::retry_counts_for_test();
        let (pop_cap_after, push_cap_after) = tagged_index_stack::backoff_cap_reached_for_test();
        assert!(
            pop_retries_after > pop_retries_before,
            "activation oracle: `pop`'s CAS-retry branch never executed across \
             {NUM_THREADS} threads x {ITERS_PER_THREAD} contended iterations \
             (before={pop_retries_before}, after={pop_retries_after}) — the \
             contention this test exists to exercise did not happen, and its \
             conservation assertion alone cannot catch a broken retry path"
        );
        assert!(
            push_retries_after > push_retries_before,
            "activation oracle: `push`'s CAS-retry branch never executed across \
             {NUM_THREADS} threads x {ITERS_PER_THREAD} contended iterations \
             (before={push_retries_before}, after={push_retries_after}) — the \
             contention this test exists to exercise did not happen, and its \
             conservation assertion alone cannot catch a broken retry path"
        );
        assert!(
            pop_cap_after > pop_cap_before,
            "backoff-depth oracle: no `pop` retry ever reached FULL backoff \
             depth (spins saturated at BACKOFF_SPIN_CAP, a \
             1 << BACKOFF_SPIN_CAP-iteration spin loop) across {NUM_THREADS} \
             threads x {ITERS_PER_THREAD} contended iterations \
             (before={pop_cap_before}, after={pop_cap_after}) — the backoff's \
             `spins` never climbed into its higher range, exactly the \
             silently-inert-backoff regression this oracle exists to catch"
        );
        assert!(
            push_cap_after > push_cap_before,
            "backoff-depth oracle: no `push` retry ever reached FULL backoff \
             depth (spins saturated at BACKOFF_SPIN_CAP, a \
             1 << BACKOFF_SPIN_CAP-iteration spin loop) across {NUM_THREADS} \
             threads x {ITERS_PER_THREAD} contended iterations \
             (before={push_cap_before}, after={push_cap_after}) — the backoff's \
             `spins` never climbed into its higher range, exactly the \
             silently-inert-backoff regression this oracle exists to catch"
        );
    }

    // Drain and confirm the exact multiset 0..LINKS_SIZE came back: no
    // duplicate, no missing index.
    let mut drained = Vec::with_capacity(LINKS_SIZE as usize);
    while let Some(idx) = stack.pop() {
        drained.push(idx);
    }
    drained.sort_unstable();

    let expected: Vec<u32> = (0..LINKS_SIZE).collect();
    assert_eq!(
        drained, expected,
        "free-list conservation violated after {NUM_THREADS} threads x \
         {ITERS_PER_THREAD} contention-shaped pop/push iterations: drained \
         multiset does not match the prefilled 0..{LINKS_SIZE} exactly \
         (lost and/or duplicated index)"
    );
}
