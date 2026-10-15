//! R6-T26 (docs/agent_reviews_round6, R6-CC-6) — TTAS + bounded exponential
//! backoff in the CAS spin loops of `ShardLock` and the fallback `LOCK`.
//!
//! Stress: several threads acquire/release each changed lock in a loop while
//! an in-critical-section flag asserts mutual exclusion (0 -> 1 -> 0, no
//! overlap) and a plain payload counter (touched only under the lock) must
//! end at exactly one increment per acquisition.
//!
//! **What this guards (behavior preservation, not a red-on-revert test):**
//! the pre-TTAS loops serialised correctly too, so reverting the rewrite does
//! not turn this red. It catches a TTAS rewrite that admits a second holder
//! (entering without a won CAS, or never re-reading the flag): the overlap
//! flag and the payload sum fail. Memory-ordering weakening is not reliably
//! caught here — the flag's own RMW synchronises holders — that needs a loom
//! model. The contended loops drive the backoff and yield arms.
//!
//! `ShardLock`/`ShardGuard` are `pub(super)`, so the lock is included by
//! path (the `#[path]` convention of `tests/r13_shard_guard_auto_traits.rs`).
//! The fallback half drives the production `with_heap` lock through the
//! `#[doc(hidden)]` hook `HeapCore::dbg_with_fallback_for_test`.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]

#[path = "../src/registry/segment_route/shard_lock.rs"]
mod shard_lock;

use core::cell::Cell;
use core::sync::atomic::{AtomicUsize, Ordering};
use sefer_alloc::registry::HeapCore;
use shard_lock::ShardLock;

const THREADS: usize = 4;
const ROUNDS: usize = 2000;
const FALLBACK_ROUNDS: usize = 256;

/// One flag per lock under test, so the two tests may run in parallel.
static IN_SHARD: AtomicUsize = AtomicUsize::new(0);
static IN_FALLBACK: AtomicUsize = AtomicUsize::new(0);

fn enter_assert_exclusive(flag: &AtomicUsize) {
    let prev = flag.swap(1, Ordering::AcqRel);
    assert_eq!(prev, 0, "mutual exclusion violated: two holders observed");
}

fn leave(flag: &AtomicUsize) {
    flag.store(0, Ordering::Release);
}

#[test]
fn shard_lock_stress_mutex_holds_under_contention() {
    let lock = ShardLock::new(0u64);
    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| {
                for _ in 0..ROUNDS {
                    {
                        let mut guard = lock.lock();
                        enter_assert_exclusive(&IN_SHARD);
                        let seen = *guard;
                        *guard = seen.wrapping_add(1);
                        leave(&IN_SHARD);
                    }
                    // The `lock_unless_exiting` face must always acquire when
                    // the process is not exiting.
                    let gave_up = Cell::new(false);
                    let guard = lock.lock_unless_exiting(|| false, || gave_up.set(true));
                    assert!(!gave_up.get(), "lock_unless_exiting refused without exit");
                    let mut guard = guard.expect("acquires when not exiting");
                    enter_assert_exclusive(&IN_SHARD);
                    let seen = *guard;
                    *guard = seen.wrapping_add(1);
                    leave(&IN_SHARD);
                }
            });
        }
    });
    assert_eq!(
        *lock.lock(),
        (THREADS * ROUNDS * 2) as u64,
        "payload sum mismatch: two holders raced on the plain counter"
    );
    assert_eq!(IN_SHARD.load(Ordering::Relaxed), 0);
}

#[test]
fn fallback_lock_stress_mutex_holds_under_contention() {
    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| {
                for _ in 0..FALLBACK_ROUNDS {
                    HeapCore::dbg_with_fallback_for_test(|_core| {
                        enter_assert_exclusive(&IN_FALLBACK);
                        leave(&IN_FALLBACK);
                    })
                    .expect("fallback with_heap returned None without process exit");
                }
            });
        }
    });
    assert_eq!(IN_FALLBACK.load(Ordering::Relaxed), 0);
}
