//! loom model-check of the **fallback-heap lazy-init state machine**
//! (`src/global/fallback.rs::heap_ptr`) — `UNINIT -> INITIALIZING -> READY`,
//! with a rollback path (`INITIALIZING -> UNINIT`) on primordial OOM.
//!
//! # Scope — what loom covers
//!
//! This harness models the pointer/state-word state-machine in
//! `heap_ptr` in isolation using `loom::sync::atomic` (NOT the real
//! `fallback::heap_ptr`, which uses `core::sync::atomic` and constructs a
//! real `HeapCore` via the OS aperture — unsuitable for loom's model
//! executor). It asserts the core liveness/safety property that motivated
//! Phase F1:
//!
//! > Across every concurrent interleaving, INCLUDING the winner hitting a
//! > simulated OOM and rolling the state back to UNINIT, every thread that
//! > calls `ensure_modeled` eventually RETURNS (no thread spins forever).
//!
//! # The bug this models (Phase F1)
//!
//! The original protocol had the loser spin `while STATE != READY`. If the
//! winner's `HeapCore::new` returns `None` (OOM), the winner rolls
//! `INITIALIZING -> UNINIT` and returns null WITHOUT ever publishing READY.
//! A loser spinning on `!= READY` then spins forever (100% CPU livelock) —
//! there is no other thread left to publish READY.
//!
//! The fix: a loser spins only `while STATE == INITIALIZING`. When it
//! observes UNINIT (the winner rolled back after OOM) it falls out of the
//! spin and RE-RACES the CAS itself, rather than waiting for a READY that
//! will never come.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --release --features "alloc-global,alloc-xthread" --test loom_fallback_init
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicU8, Ordering};
use loom::sync::Arc;
use loom::thread;

const STATE_UNINIT: u8 = 0;
const STATE_INITIALIZING: u8 = 1;
const STATE_READY: u8 = 2;

/// Model of the FIXED `heap_ptr`: a loser re-races the CAS on observing
/// UNINIT instead of spinning forever on `!= READY`.
///
/// `fail_once`: if `Some(&AtomicU8)` and the flag is 0, the FIRST thread to
/// win the CAS simulates `HeapCore::new` returning `None` (OOM) exactly
/// once (flips the flag to 1 so only one OOM is injected across the whole
/// run — otherwise every winner could hit OOM forever and the model would
/// never terminate, which is not the property under test: we want to prove
/// liveness survives A rollback, not survive an adversarial infinite-OOM
/// oracle).
fn ensure_modeled_fixed(
    state: &Arc<AtomicU8>,
    winner_count: &Arc<AtomicU8>,
    oom_injected: &Arc<AtomicU8>,
) -> bool {
    loop {
        if state.load(Ordering::Acquire) == STATE_READY {
            return true;
        }
        let won = state
            .compare_exchange(
                STATE_UNINIT,
                STATE_INITIALIZING,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok();
        if won {
            // Inject OOM exactly once across the whole model run, on the
            // first thread that wins the CAS.
            let should_oom = oom_injected
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok();
            if should_oom {
                // Simulated primordial OOM: roll back, return "null" (false)
                // WITHOUT publishing READY.
                state.store(STATE_UNINIT, Ordering::Release);
                return false;
            }
            winner_count.fetch_add(1, Ordering::Relaxed);
            state.store(STATE_READY, Ordering::Release);
            return true;
        }
        // FIXED loser: spin only while INITIALIZING; fall out (and re-race)
        // on UNINIT.
        while state.load(Ordering::Acquire) == STATE_INITIALIZING {
            thread::yield_now();
        }
    }
}

/// Model of the BROKEN original `heap_ptr`: a loser spins `while != READY`
/// — forever, if the winner rolled back to UNINIT after OOM and no other
/// thread re-attempts initialisation.
///
/// Bounded for loom (an unbounded spin would hang the model checker
/// itself); the bound is generous relative to the fixed protocol's
/// worst-case retry count, so if the broken protocol still needs the bound
/// to terminate, that IS the livelock this counterfactual demonstrates.
fn ensure_modeled_broken(
    state: &Arc<AtomicU8>,
    winner_count: &Arc<AtomicU8>,
    oom_injected: &Arc<AtomicU8>,
) -> bool {
    const MAX_SPIN_ITERS: usize = 8;
    loop {
        if state.load(Ordering::Acquire) == STATE_READY {
            return true;
        }
        let won = state
            .compare_exchange(
                STATE_UNINIT,
                STATE_INITIALIZING,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok();
        if won {
            let should_oom = oom_injected
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok();
            if should_oom {
                state.store(STATE_UNINIT, Ordering::Release);
                return false;
            }
            winner_count.fetch_add(1, Ordering::Relaxed);
            state.store(STATE_READY, Ordering::Release);
            return true;
        }
        // BROKEN loser: spin until READY — but if the winner rolled back to
        // UNINIT (OOM) and no one else re-races, READY never comes. Bounded
        // so the model checker itself terminates; exceeding the bound is
        // the livelock signature we assert against.
        for _ in 0..MAX_SPIN_ITERS {
            if state.load(Ordering::Acquire) == STATE_READY {
                return true;
            }
            thread::yield_now();
        }
        panic!(
            "loom: loser spun {MAX_SPIN_ITERS} iterations without observing READY \
             — livelock (winner rolled back to UNINIT after simulated OOM and \
             this loser never re-races the CAS)"
        );
    }
}

// ============================================================================
// Fixed protocol — liveness holds even through a winner's OOM rollback.
// ============================================================================

/// 2 threads race `ensure_modeled_fixed`. One of the two CAS-wins triggers
/// the injected OOM (rolls back to UNINIT, returns false); the other thread
/// (loser-turned-re-racer, or whichever thread retries) must still succeed
/// and reach READY. BOTH threads must terminate (loom itself guarantees
/// this — an actual infinite loop in the model would hang the checker), and
/// at least one thread must observe `true` (READY eventually reached).
#[test]
fn fallback_init_survives_oom_rollback_two_threads() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let state = Arc::new(AtomicU8::new(STATE_UNINIT));
        let winner_count = Arc::new(AtomicU8::new(0));
        let oom_injected = Arc::new(AtomicU8::new(0));

        let (s1, w1, o1) = (
            Arc::clone(&state),
            Arc::clone(&winner_count),
            Arc::clone(&oom_injected),
        );
        let t1 = thread::spawn(move || ensure_modeled_fixed(&s1, &w1, &o1));

        let (s2, w2, o2) = (
            Arc::clone(&state),
            Arc::clone(&winner_count),
            Arc::clone(&oom_injected),
        );
        let t2 = thread::spawn(move || ensure_modeled_fixed(&s2, &w2, &o2));

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        // Both threads TERMINATE (loom itself proves this — no infinite
        // loop). At least one must have succeeded (true) — the OOM is
        // injected exactly once, so the second attempt (by whichever thread
        // gets there) must succeed.
        assert!(
            r1 || r2,
            "at least one thread must eventually observe a successfully \
             initialised fallback heap after the injected OOM rolls back \
             once"
        );

        // Exactly one successful winner publishes READY (the OOM'd
        // "winner" does not count — it never published).
        let count = winner_count.load(Ordering::Relaxed);
        assert_eq!(
            count, 1,
            "exactly one thread must successfully publish READY (got {count})"
        );

        // Final state must be READY (whoever succeeded left it there).
        assert_eq!(
            state.load(Ordering::Relaxed),
            STATE_READY,
            "final state must be READY after the surviving thread succeeds"
        );
    });
}

/// 3-thread variant (tighter preemption bound to keep the state space
/// tractable) — same property, more interleavings of who wins the race
/// after the OOM rollback.
#[test]
fn fallback_init_survives_oom_rollback_three_threads() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(1);
    builder.check(|| {
        let state = Arc::new(AtomicU8::new(STATE_UNINIT));
        let winner_count = Arc::new(AtomicU8::new(0));
        let oom_injected = Arc::new(AtomicU8::new(0));

        let (s1, w1, o1) = (
            Arc::clone(&state),
            Arc::clone(&winner_count),
            Arc::clone(&oom_injected),
        );
        let (s2, w2, o2) = (
            Arc::clone(&state),
            Arc::clone(&winner_count),
            Arc::clone(&oom_injected),
        );

        let t1 = thread::spawn(move || ensure_modeled_fixed(&s1, &w1, &o1));
        let t2 = thread::spawn(move || ensure_modeled_fixed(&s2, &w2, &o2));
        let r_main = ensure_modeled_fixed(&state, &winner_count, &oom_injected);

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        assert!(
            r1 || r2 || r_main,
            "at least one of the three threads must eventually succeed"
        );
        let count = winner_count.load(Ordering::Relaxed);
        assert_eq!(count, 1, "exactly one successful publisher (got {count})");
        assert_eq!(state.load(Ordering::Relaxed), STATE_READY);
    });
}

// ============================================================================
// Counterfactual — the BROKEN (pre-fix) protocol livelocks.
// ============================================================================

/// COUNTERFACTUAL (non-vacuousness proof): with the ORIGINAL loser spin
/// (`while != READY`), an interleaving exists where the loser spins past
/// the bounded iteration cap without ever observing READY — because the
/// winner it was waiting on rolled back to UNINIT after a simulated OOM and
/// nobody else re-raced the CAS. loom must find this interleaving and the
/// injected `panic!` inside `ensure_modeled_broken` must fire.
///
/// **If this test PASSES (does not panic), the counterfactual is
/// vacuous** — loom failed to find the livelock interleaving and the
/// harness needs rework.
#[test]
#[should_panic(expected = "livelock")]
fn counterfactual_broken_protocol_livelocks_on_oom_rollback() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let state = Arc::new(AtomicU8::new(STATE_UNINIT));
        let winner_count = Arc::new(AtomicU8::new(0));
        let oom_injected = Arc::new(AtomicU8::new(0));

        let (s1, w1, o1) = (
            Arc::clone(&state),
            Arc::clone(&winner_count),
            Arc::clone(&oom_injected),
        );
        let t1 = thread::spawn(move || ensure_modeled_broken(&s1, &w1, &o1));

        let (s2, w2, o2) = (
            Arc::clone(&state),
            Arc::clone(&winner_count),
            Arc::clone(&oom_injected),
        );
        let t2 = thread::spawn(move || ensure_modeled_broken(&s2, &w2, &o2));

        let _ = t1.join();
        let _ = t2.join();
    });
}
