//! Ph4a (task #2091): small bounded miri run of the `HeapLease` lifecycle on
//! a cross-thread schedule: thread A claims a lease via
//! `HeapRegistry::dbg_claim_lease()` and drops it (Release LIVE → FREE
//! publication); the main thread joins and re-claims on "another thread" —
//! the claim → drop → claim S2 happens-before pair, executed under both
//! provenance models (see CI `miri` / `miri-plain` jobs).
//!
//! `core()` is deliberately NOT exercised here: it requires `&mut HeapCore`,
//! which is not reachable from `tests/` through the safe API, and the lease
//! is `!Send` so it cannot be moved anyway; the lifecycle (claim, drop,
//! re-claim, generation monotonicity, FREE state) is the observable surface.
//!
//! Run via `cargo miri test --features "alloc-global alloc-xthread internals"
//! --test r11_ph4a_lease_miri` (and the strict-provenance pass).
#![cfg(all(miri, feature = "alloc-global", feature = "internals"))]

use std::thread;

use sefer_alloc::registry::{bootstrap, HeapRegistry};

#[test]
fn lease_claim_drop_reclaim_across_threads_under_miri() {
    let reg = bootstrap::ensure();
    // Pre-claim the slot on THIS thread so thread A's claim/drop covers a
    // reuse path (initialised publish already happened once).
    let warm = HeapRegistry::dbg_claim_lease().expect("warm claim");
    let index = warm.slot_index() as usize;
    let warm_gen = warm.generation();
    drop(warm);
    assert_eq!(
        reg.dbg_slot_state(index),
        sefer_alloc::registry::heap_slot::STATE_FREE
    );

    // Thread A: claim the lease on a DIFFERENT thread and drop it. The Drop's
    // Release CAS publishes; the join provides the cross-thread happens-before.
    let handle = thread::spawn(move || {
        let lease = HeapRegistry::dbg_claim_lease().expect("thread A claim");
        assert_eq!(lease.slot_index() as usize, index);
        lease.generation()
    });
    let a_gen = handle.join().expect("thread A");
    assert!(a_gen > warm_gen);
    assert_eq!(
        reg.dbg_slot_state(index),
        sefer_alloc::registry::heap_slot::STATE_FREE
    );

    // Main thread re-claim: the Acquire claim CAS must observe A's Release.
    let again = HeapRegistry::dbg_claim_lease().expect("re-claim after join");
    assert_eq!(again.slot_index() as usize, index);
    assert!(again.generation() > a_gen);
    drop(again);
    assert_eq!(
        reg.dbg_slot_state(index),
        sefer_alloc::registry::heap_slot::STATE_FREE
    );
}
