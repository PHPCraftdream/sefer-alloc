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
use sefer_alloc::registry::heap_slot::SlotState;
use sefer_alloc::registry::heap_slot::SlotState::{Free, Live};

struct Slot {
    state: AtomicU8,
    /// Exactly-one-mutator guard: `fetch_add` must always observe 0.
    active: AtomicUsize,
    /// Owner scratch — the S2 happens-before payload.
    payload: UnsafeCell<usize>,
}

/// One state CAS (`HeapSlot::cas_state`). The two lease CAS lines below are
/// marked `ghost-bind`: rule R8 checks their orderings against the R1 rules on
/// `src/registry/heap_registry/claim.rs`. Weakening the Release drop loses
/// the owner-payload write vs. next-claimant read edge → loom data race.
fn cas(slot: &Slot, from: SlotState, to: SlotState, success: Ordering, failure: Ordering) -> bool {
    slot.state
        .compare_exchange(from as u8, to as u8, success, failure)
        .is_ok()
}

/// HeapLease shadow: winning CAS grants authority; Drop is the Release
/// LIVE→FREE publication (`HeapLease::drop`).
fn owner_take_and_drop(slot: &Slot) {
    // ghost-bind: heap-lease/claim-free-live
    if !cas(slot, Free, Live, Ordering::AcqRel, Ordering::Acquire) {
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
    // ghost-bind: heap-lease/drop-release
    let won = cas(slot, Live, Free, Ordering::Release, Ordering::Relaxed);
    assert!(won, "a lease holder's LIVE -> FREE CAS cannot lose");
}

#[test]
fn lease_drop_release_pairs_with_claim_and_maintenance_acquires() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.check(|| {
        let slot = Arc::new(Slot {
            state: AtomicU8::new(SlotState::Free as u8),
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
                    SlotState::Free as u8,
                    SlotState::Maintenance as u8,
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
                    SlotState::Maintenance as u8,
                    SlotState::Free as u8,
                    Ordering::Release,
                    Ordering::Relaxed
                )
                .is_ok());
        });
        owner.join().expect("owner");
        claimant.join().expect("claimant");
        maintenance.join().expect("maintenance");
        // Every join observes the final Release publication.
        assert_eq!(slot.state.load(Ordering::Acquire), SlotState::Free as u8);
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
