//! B4 depot (DP-4): loom shadow-model of the depot across a slot recycle.
//!
//! The depot is owner-only, non-atomic storage in `HeapSlot` (bound once as
//! `&'static mut Depot`). Its cross-thread soundness rests on the lease
//! protocol: the owner drains the depot (`flush_all_tcache` inside
//! `trim_for_recycle`) BEFORE the Release LIVE→FREE publication, and the next
//! claimant's Acquire CAS orders its depot accesses after that. Shadow model
//! by the pattern of `tests/loom_r11_ph4a_heap_lease.rs` (the real registry
//! cannot run under loom); the production orderings are mirrored exactly.
//! Two counterfactuals must go red: a Relaxed release (loom data race on the
//! depot cell) and publishing before the drain (claimant sees parked state).
#![cfg(all(loom, feature = "alloc-global", feature = "internals"))]

use loom::cell::UnsafeCell;
use loom::sync::atomic::{AtomicU8, Ordering};
use loom::sync::Arc;
use loom::thread;
use sefer_alloc::registry::heap_slot::SlotState;

/// Depot shadow: parked-magazine count and the top magazine's token.
struct Slot {
    state: AtomicU8,
    depot: UnsafeCell<(u8, usize)>,
}

#[derive(Clone, Copy)]
struct Protocol {
    release: Ordering,
    drain_before_release: bool,
}

const PRODUCTION: Protocol = Protocol {
    release: Ordering::Release,
    drain_before_release: true,
};

fn drain(slot: &Slot) {
    slot.depot.with_mut(|d| {
        // SAFETY: the LIVE CAS winner is the only depot accessor.
        unsafe { *d = (0, 0) };
    });
}

/// One owner lifetime: claim, see an empty depot, park, reload, park again,
/// drain at recycle, release.
fn owner_lifetime(slot: &Slot, token: usize, p: Protocol) {
    if slot
        .state
        .compare_exchange(
            SlotState::Free as u8,
            SlotState::Live as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return;
    }
    slot.depot.with_mut(|d| {
        // SAFETY: exclusive owner; Acquire on the winning CAS pairs with the
        // previous owner's Release.
        unsafe {
            assert_eq!((*d).0, 0, "a claimant must inherit an empty depot");
            *d = (1, token);
            assert_eq!(*d, (1, token), "reload returns the parked magazine");
            *d = (0, 0);
            *d = (1, token);
        }
    });
    if p.drain_before_release {
        drain(slot);
    }
    assert!(slot
        .state
        .compare_exchange(
            SlotState::Live as u8,
            SlotState::Free as u8,
            p.release,
            Ordering::Relaxed,
        )
        .is_ok());
    if !p.drain_before_release {
        drain(slot);
    }
}

fn run(p: Protocol) {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.check(move || {
        let slot = Arc::new(Slot {
            state: AtomicU8::new(SlotState::Free as u8),
            depot: UnsafeCell::new((0, 0)),
        });
        let a = {
            let s = slot.clone();
            thread::spawn(move || owner_lifetime(&s, 1, p))
        };
        let b = {
            let s = slot.clone();
            thread::spawn(move || owner_lifetime(&s, 2, p))
        };
        owner_lifetime(&slot, 3, p);
        a.join().expect("owner a");
        b.join().expect("owner b");
        assert_eq!(slot.state.load(Ordering::Acquire), SlotState::Free as u8);
        slot.depot.with(|d| {
            // SAFETY: all owners joined.
            assert_eq!(unsafe { (*d).0 }, 0, "recycled slot keeps an empty depot");
        });
    });
}

#[test]
fn depot_drained_before_release_is_seen_empty_by_next_owner() {
    run(PRODUCTION);
}

#[test]
#[should_panic]
fn negative_relaxed_release_races_on_depot() {
    run(Protocol {
        release: Ordering::Relaxed,
        ..PRODUCTION
    });
}

#[test]
#[should_panic]
fn negative_release_before_drain_leaks_parked_state() {
    run(Protocol {
        drain_before_release: false,
        ..PRODUCTION
    });
}
