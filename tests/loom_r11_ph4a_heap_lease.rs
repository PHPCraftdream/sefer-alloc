//! Ph4a (task #2091): loom shadow-model of the `HeapLease` owner protocol —
//! owner (FREE → LIVE CAS, payload bump, Release Drop back to FREE) racing a
//! fresh claimant (FREE → LIVE) and a maintenance taker (FREE → MAINTENANCE).
//! Shadow model by the pattern of `tests/loom_r8_maintenance_lease.rs`: the
//! real `HeapLease` cannot run under loom (registry bootstrap), so the
//! production CAS orderings are mirrored exactly.
#![cfg(all(loom, feature = "alloc-global", feature = "internals"))]

use loom::cell::UnsafeCell;
use loom::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;
use sefer_alloc::registry::heap_slot::{STATE_FREE, STATE_LIVE, STATE_MAINTENANCE};

struct Slot {
    state: AtomicU8,
    /// Exactly-one-mutator guard: `fetch_add` must always observe 0.
    active: AtomicUsize,
    /// Owner scratch — the S2 happens-before payload.
    payload: UnsafeCell<usize>,
}

/// Publication ordering of the Drop's LIVE→FREE CAS. `Release` at rest;
/// mutant M1 (task #2091 chunk C) temporarily flips this to `Relaxed` and the
/// model below must go red (owner payload write vs. next claimant read loses
/// its happens-before edge → loom data race).
const DROP_ORDERING: Ordering = Ordering::Release;

/// HeapLease shadow: winning CAS grants authority; Drop is the Release
/// LIVE→FREE publication (`HeapLease::drop`).
fn owner_take_and_drop(slot: &Slot) {
    if slot
        .state
        .compare_exchange(STATE_FREE, STATE_LIVE, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return; // lost claimant CAS — no authority, no core access
    }
    // S1: only the CAS winner runs its body.
    assert_eq!(slot.active.fetch_add(1, Ordering::Relaxed), 0);
    slot.payload.with_mut(|pointer| {
        // SAFETY: exclusive mutator (S1); Acquire on the winning CAS paired
        // with the previous Drop's Release gives S2 happens-before.
        unsafe {
            let before = *pointer;
            assert!(before <= 3, "payload monotone within bounded drops");
            *pointer = before + 1;
        }
    });
    // Drop: Release-publish the owner writes (S2), then authority ends.
    assert_eq!(slot.active.fetch_sub(1, Ordering::Relaxed), 1);
    assert!(slot
        .state
        .compare_exchange(STATE_LIVE, STATE_FREE, DROP_ORDERING, Ordering::Relaxed)
        .is_ok());
}

#[test]
fn lease_drop_release_pairs_with_claim_and_maintenance_acquires() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.check(|| {
        let slot = Arc::new(Slot {
            state: AtomicU8::new(STATE_FREE),
            active: AtomicUsize::new(0),
            payload: UnsafeCell::new(0),
        });
        let owner_slot = slot.clone();
        let owner = thread::spawn(move || owner_take_and_drop(&owner_slot));
        let claimant_slot = slot.clone();
        let claimant = thread::spawn(move || owner_take_and_drop(&claimant_slot));
        let maintenance_slot = slot.clone();
        let maintenance = thread::spawn(move || {
            if maintenance_slot
                .state
                .compare_exchange(
                    STATE_FREE,
                    STATE_MAINTENANCE,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_err()
            {
                return;
            }
            assert_eq!(maintenance_slot.active.fetch_add(1, Ordering::Relaxed), 0);
            maintenance_slot.payload.with_mut(|pointer| {
                // SAFETY: MAINTENANCE CAS winner is the exclusive mutator.
                unsafe {
                    let before = *pointer;
                    assert!(before <= 3);
                    *pointer = before + 1;
                }
            });
            assert_eq!(maintenance_slot.active.fetch_sub(1, Ordering::Relaxed), 1);
            assert!(maintenance_slot
                .state
                .compare_exchange(
                    STATE_MAINTENANCE,
                    STATE_FREE,
                    Ordering::Release,
                    Ordering::Relaxed
                )
                .is_ok());
        });
        owner.join().expect("owner");
        claimant.join().expect("claimant");
        maintenance.join().expect("maintenance");
        // Every join observes the final Release publication.
        assert_eq!(slot.state.load(Ordering::Acquire), STATE_FREE);
        assert_eq!(slot.active.load(Ordering::Relaxed), 0);
        slot.payload.with(|pointer| {
            // SAFETY: all threads joined; no lease or mutator remains.
            let completed = unsafe { *pointer };
            assert!(
                (1..=4).contains(&completed),
                "payload {completed} outside 1..=4"
            );
        });
    });
}
