//! Real-OS-thread free-list conservation test — the multi-threaded coverage
//! `loom_aba.rs` structurally cannot provide.
//!
//! `loom_aba.rs` is exhaustive but tiny: every model there explores a small
//! bounded state space rather than running real time. This file covers the
//! complementary regime: a fixed, modest number of REAL OS threads hammering
//! a SHARED stack for many iterations and asserts free-list conservation.
//! It deliberately does not require a particular CAS-loss count: a real OS
//! scheduler may serialize otherwise-correct workers. Loom models separately
//! force and assert both retry branches, while `backoff_oracle.rs` checks the
//! local backoff progression and saturation deterministically. This test
//! checks the same conservation property at real-thread scale.
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
/// (1.6M total pop/push pairs). The scale makes contention
/// likely on ordinary multi-core hosts, but no assertion depends on that
/// scheduler outcome.
const ITERS_PER_THREAD: u32 = 200_000;

/// One full contended round: `NUM_THREADS` real OS threads, each running
/// `ITERS_PER_THREAD` pop-then-immediately-repush iterations against the
/// shared stack. Pure free-list churn — every thread pops WHATEVER is
/// currently on top (which may be another thread's index, under real
/// contention) and immediately re-pushes EXACTLY that value — so it neither
/// adds nor removes anything from the stack, and running it more than once
/// cannot break the conservation check.
fn contention_round(stack: &Stack) {
    // Start-rendezvous barrier: without it, `s.spawn()` merely SCHEDULES a
    // thread, it does not synchronize its start against its siblings. On a
    // CI runner with few real cores (observed live: GitHub Actions'
    // `ubuntu-latest`), thread creation can be slow enough relative to this
    // loop's tiny per-iteration cost that early threads run a large chunk of
    // their 200,000 iterations before a later thread is even scheduled for
    // the first time -- collapsing what should be 8-way real contention into
    // several near-sequential runs with little to no overlap. That is
    // A staggered start against a shared stack still conserves the free-list
    // (no thread ever needs a SECOND
    // concurrent writer to stay correct), so only the oracle -- not the
    // conservation check -- can tell "ran without contention" apart from
    // "the retry path is broken". `NUM_THREADS + 1` participants (the
    // workers plus the calling thread) release everyone into the
    // contended loop at approximately the same instant, the same fix shape
    // `benches/tagged_index_stack_bench.rs`'s contention phases already use
    // for their own published-timing-window rendezvous.
    let start_barrier = std::sync::Barrier::new(NUM_THREADS + 1);

    thread::scope(|s| {
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
                    // SAFETY: idx was JUST returned by this stack's own pop —
                    // that one successful pop transferred publish/recycle
                    // authority for it to THIS thread, which re-pushes it
                    // synchronously without sharing it; in-domain by
                    // construction (push clause 3). 1.6M total pop/push pairs
                    // is far below the 48-bit tag's 2^48-1 budget, so this
                    // never legitimately hits TagExhausted — `.expect` is a
                    // real assertion, not a shrug.
                    unsafe { stack.push(idx) }.expect("tag budget not exhausted at this scale");
                }
            });
        }
        // Release all NUM_THREADS workers into their contended loop at
        // approximately the same instant (see the barrier's own doc comment
        // above for why this rendezvous is load-bearing, not cosmetic).
        start_barrier.wait();
    });
}

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
        // SAFETY: fresh stack (domain 0..LINKS_SIZE); each index is in-domain,
        // never pushed before, and pushed exactly once here, so its
        // publish/recycle authority is freshly minted and consumed by this
        // one call (push clause 3).
        unsafe { stack.push(i) }.expect("fresh head has tag budget");
    }

    contention_round(&stack);

    // Drain and confirm the exact multiset 0..LINKS_SIZE came back: no
    // duplicate, no missing index.
    let mut drained = Vec::with_capacity(LINKS_SIZE as usize);
    while let Some(idx) = stack.pop() {
        drained.push(idx);
    }
    drained.sort_unstable();

    let expected: Vec<u32> = (0..LINKS_SIZE).collect();
    let total_iterations = (NUM_THREADS as u64) * u64::from(ITERS_PER_THREAD);
    assert_eq!(
        drained, expected,
        "free-list conservation violated after {total_iterations} total \
         pop/push iterations ({NUM_THREADS} \
         threads x {ITERS_PER_THREAD} per round): drained multiset does not \
         match the prefilled 0..{LINKS_SIZE} exactly (lost and/or duplicated \
         index)"
    );
}
