//! Real-OS-thread free-list conservation test — the multi-threaded coverage
//! `loom_aba.rs` structurally cannot provide.
//!
//! `loom_aba.rs` is exhaustive but tiny: every model there spawns at most 2
//! threads over a 1-2-slot backing (`const N: usize = 2` there), and loom
//! explores every interleaving of that small model rather than running real
//! time. That bound also caps how deep the CAS-retry backoff's `spins`
//! counter (`src/lib.rs`, `push`/`pop`) can climb inside any loom model — a
//! handful of interleavings can force at most a couple of retries, nowhere
//! near `BACKOFF_SPIN_CAP` (6). This file exercises the OPPOSITE regime: a
//! fixed, modest number of REAL OS threads hammering a SHARED stack for many
//! iterations, so genuine contention forces many CAS retries per call, and
//! `spins` genuinely climbs into its higher range in practice — something no
//! existing test (loom or otherwise) does. This is the committed replacement
//! for the throwaway ad hoc probe cited in the round-6 backoff commit
//! (`069d187`): "8 threads x 200,000 contention-shaped pop/push iterations
//! under the backoff, then drained and confirmed the exact multiset 0..64
//! came back" — never committed. See round-7 review finding P2-2
//! (`docs/reviews/2026-08-31-100751-tagged-index-stack-review-round7-oh.md`).
//!
//! Discipline mirrors `benches/tagged_index_stack_bench.rs`'s
//! `contention/churn` phase exactly: every thread pops WHATEVER is currently
//! on top (which may be another thread's index, under real contention) and
//! immediately re-pushes EXACTLY that value — never a locally invented index.
//! Re-pushing anything else violates `push`'s documented caller contract
//! ("index must NOT already be reachable from the stack") and would corrupt
//! the free-list independent of any bug this test exists to catch.
//!
//! Not a loom model (`#![cfg(not(loom))]`) — this is a normal `cargo test`
//! file exercising real OS threads at real scale, which loom cannot do (loom
//! replaces `std`'s atomics/threads with its own model-checked stand-ins and
//! only explores a small bounded state space).

#![cfg(not(loom))]

use std::thread;

use tagged_index_stack::{ArrayLinks, TaggedIndexStack};

/// Same width as the bench and the rest of this crate's test suite.
type Stack = TaggedIndexStack<16>;

/// Number of indices in the `ArrayLinks` backing store, and the exact
/// multiset seeded onto the stack before the threaded phase. Kept modest per
/// CLAUDE.md's "Speed: short scenario by default" convention.
const LINKS_SIZE: u32 = 64;

/// Real OS threads racing the shared stack concurrently.
const NUM_THREADS: usize = 4;

/// Pop-then-repush iterations per thread. `NUM_THREADS * ITERS_PER_THREAD`
/// (80,000 total ops) is enough real contention to push the CAS-retry
/// backoff's `spins` counter well past 2-3 -- the ceiling any loom model in
/// `loom_aba.rs` can reach (those models cap out at 2 threads / 2 seeded
/// indices) -- while still running in a couple of seconds.
const ITERS_PER_THREAD: u32 = 20_000;

/// N threads x M iterations of pop-then-immediately-repush-exactly-what-you-
/// popped against a shared, prefilled stack, followed by a full drain and an
/// exact-multiset check: the classic Treiber free-list conservation property
/// (no index lost, none duplicated) under REAL contention.
#[test]
fn conservation_under_real_thread_contention() {
    let links = ArrayLinks::<{ LINKS_SIZE as usize }>::new();
    let stack = Stack::new();

    // Prefill a fresh (already-empty) stack with 0..LINKS_SIZE -- mirrors the
    // bench's `contention/churn` prefill discipline. No drain-first needed:
    // `Stack::new()` starts empty (RAD-1 lazy links).
    for i in 0..LINKS_SIZE {
        stack.push(&links, i);
    }

    thread::scope(|s| {
        let links = &links;
        let stack = &stack;
        for _ in 0..NUM_THREADS {
            s.spawn(move || {
                for _ in 0..ITERS_PER_THREAD {
                    // Pop whatever is currently on top (may belong to any
                    // thread under contention) and immediately re-push
                    // EXACTLY that value -- never a locally invented index.
                    let idx = stack.pop(links).expect(
                        "stack unexpectedly empty: with LINKS_SIZE prefilled \
                         indices and at most NUM_THREADS held outstanding at \
                         once, the stack can never observe fewer than \
                         LINKS_SIZE - NUM_THREADS elements",
                    );
                    stack.push(links, idx);
                }
            });
        }
    });

    // Drain and confirm the exact multiset 0..LINKS_SIZE came back: no
    // duplicate, no missing index.
    let mut drained = Vec::with_capacity(LINKS_SIZE as usize);
    while let Some(idx) = stack.pop(&links) {
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
