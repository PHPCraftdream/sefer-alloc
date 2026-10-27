//! B4 depot (DP-4): loom shadow-model of the depot across a slot recycle.
//!
//! The depot is owner-only, non-atomic storage in `HeapSlot` (bound once as
//! `&'static mut Depot`). Its cross-thread soundness rests on the lease
//! protocol: the owner drains the depot (`flush_all_tcache` inside
//! `trim_for_recycle`) BEFORE the Release LIVE→FREE publication, and the next
//! claimant's Acquire CAS orders its depot accesses after that. Shadow model
//! by the pattern of `tests/loom_r11_ph4a_heap_lease.rs` (the real registry
//! cannot run under loom). The lines marked `ghost-bind` carry the production
//! orderings and the teardown step order; rule R8 checks them against the R1
//! rules on `src/registry/heap_registry/claim.rs` and the R3 rule on
//! `LeaseReleaseGuard::drop`.
//! Two counterfactuals must go red: a Relaxed release (loom data race on the
//! depot cell) and publishing before the drain (claimant sees parked state).
#![cfg(all(loom, feature = "alloc-global", feature = "internals"))]

use loom::cell::UnsafeCell;
use loom::sync::atomic::{AtomicU8, Ordering};
use loom::sync::Arc;
use loom::thread;
use sefer_alloc::registry::heap_slot::SlotState;

// ghost-bind: heap-lease/claim-free-live
const CLAIM: (Ordering, Ordering) = (Ordering::AcqRel, Ordering::Acquire);
// ghost-bind: heap-lease/drop-release
const DROP_LEASE: (Ordering, Ordering) = (Ordering::Release, Ordering::Relaxed);

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
    release: DROP_LEASE.0,
    drain_before_release: true,
};

/// `HeapLease`: the winning claim; its Drop is the LIVE→FREE publication.
struct Lease<'a> {
    slot: &'a Slot,
    release: Ordering,
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        assert!(self
            .slot
            .state
            .compare_exchange(
                SlotState::Live as u8,
                SlotState::Free as u8,
                self.release,
                DROP_LEASE.1,
            )
            .is_ok());
    }
}

fn claim(slot: &Slot, p: Protocol) -> Option<Lease<'_>> {
    slot.state
        .compare_exchange(
            SlotState::Free as u8,
            SlotState::Live as u8,
            CLAIM.0,
            CLAIM.1,
        )
        .ok()
        .map(|_| Lease {
            slot,
            release: p.release,
        })
}

/// `trim_for_recycle` → `flush_all_tcache`: every parked magazine is flushed.
fn trim_for_recycle(slot: &Slot) {
    slot.depot.with_mut(|d| {
        // SAFETY: the LIVE CAS winner is the only depot accessor.
        unsafe { *d = (0, 0) };
    });
}

/// One owner lifetime: claim, see an empty depot, park, reload, park again,
/// drain at recycle, release.
fn owner_lifetime(slot: &Slot, token: usize, p: Protocol) {
    let Some(lease) = claim(slot, p) else {
        return;
    };
    lease.slot.depot.with_mut(|d| {
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
        // ghost-bind: teardown/trim-before-release
        trim_for_recycle(lease.slot);
        drop(lease);
    } else {
        drop(lease);
        trim_for_recycle(slot);
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
