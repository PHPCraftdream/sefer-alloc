//! R12-04 (fxx round 12): losers of the fallback-heap init race must yield to
//! the scheduler past their tight-spin budget instead of burning whole quanta
//! while the winner sits in `INITIALIZING` (`HeapCore::new` reserves the
//! primordial segment from the OS).
//!
//! The winner is held inside `INITIALIZING` by `dbg_set_hold_fallback_init`, so
//! the losers deterministically reach the wait loop. Non-vacuous: the test
//! asserts the state is still `INITIALIZING` when the losers have waited, and
//! `dbg_fallback_init_wait_yields() > 0` is the observable that the loop yields;
//! a plain-spin loop (the pre-fix code) never increments it, so the bounded wait
//! below fails by timeout.
//!
//! Own test binary, no global allocator installed: the process-wide fallback
//! starts `UNINIT`, asserted as the precondition.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "std",
    feature = "internals",
    feature = "bench-internals"
))]

use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::global::{
    dbg_fallback_init_wait_yields, dbg_init_state, dbg_set_hold_fallback_init, STATE_INITIALIZING,
    STATE_READY, STATE_UNINIT,
};
use sefer_alloc::registry::HeapCore;

const LOSERS: usize = 4;
const WAIT: Duration = Duration::from_secs(60);

/// Clears the hold on every exit path so a failing assertion cannot wedge the
/// winner (and hang the test binary).
struct Release;

impl Drop for Release {
    fn drop(&mut self) {
        dbg_set_hold_fallback_init(false);
    }
}

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(start.elapsed() < WAIT, "timed out waiting for {what}");
        thread::yield_now();
    }
}

fn touch_fallback() -> bool {
    HeapCore::dbg_with_fallback_for_test(|_| ()).is_some()
}

#[test]
fn losers_yield_while_winner_initialises() {
    assert_eq!(
        dbg_init_state(),
        STATE_UNINIT,
        "precondition: fallback must be UNINIT or the hold is unreachable"
    );
    assert_eq!(dbg_fallback_init_wait_yields(), 0);

    dbg_set_hold_fallback_init(true);
    let release = Release;
    let winner = thread::spawn(touch_fallback);
    wait_until("the winner to enter INITIALIZING", || {
        dbg_init_state() == STATE_INITIALIZING
    });

    let losers: Vec<_> = (0..LOSERS).map(|_| thread::spawn(touch_fallback)).collect();
    wait_until("a loser to yield in the init wait loop", || {
        dbg_fallback_init_wait_yields() > 0
    });
    assert_eq!(
        dbg_init_state(),
        STATE_INITIALIZING,
        "losers must still be waiting on the held winner"
    );

    drop(release);
    assert!(
        winner.join().expect("winner panicked"),
        "winner got no heap"
    );
    for loser in losers {
        assert!(loser.join().expect("loser panicked"), "loser got no heap");
    }
    assert_eq!(dbg_init_state(), STATE_READY);
}
