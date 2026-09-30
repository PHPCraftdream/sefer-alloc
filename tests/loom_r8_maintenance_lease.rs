//! Production-equivalent state/CAS ordering, including failed MAINTENANCE
//! claims: a failed CAS must never create a second metadata mutator.
#![cfg(all(loom, feature = "alloc-global", feature = "internals"))]

use loom::cell::UnsafeCell;
use loom::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;
use sefer_alloc::registry::heap_slot::{STATE_FREE, STATE_LIVE, STATE_MAINTENANCE};

struct Slot {
    state: AtomicU8,
    active: AtomicUsize,
    payload: UnsafeCell<usize>,
}

fn take_and_recycle(slot: &Slot, desired: u8) {
    if slot
        .state
        .compare_exchange(STATE_FREE, desired, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    assert_eq!(slot.active.fetch_add(1, Ordering::Relaxed), 0);
    slot.payload.with_mut(|pointer| {
        // SAFETY: only a successful FREE -> LIVE/MAINTENANCE CAS enters this
        // region. Its Acquire observes the previous holder's Release recycle.
        // No pointer or reference escapes the exclusive lease lifetime.
        unsafe {
            let before = *pointer;
            assert!(before == 0 || before == 1 || before == 2);
            *pointer = before + 1;
        }
    });
    assert_eq!(slot.active.fetch_sub(1, Ordering::Relaxed), 1);
    assert!(slot
        .state
        .compare_exchange(desired, STATE_FREE, Ordering::Release, Ordering::Relaxed)
        .is_ok());
}

#[test]
fn claim_maintenance_and_recycle_have_one_mutator_and_publish_payload() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.check(|| {
        let slot = Arc::new(Slot {
            state: AtomicU8::new(STATE_FREE),
            active: AtomicUsize::new(0),
            payload: UnsafeCell::new(0),
        });
        let owner_slot = slot.clone();
        let owner = thread::spawn(move || take_and_recycle(&owner_slot, STATE_LIVE));
        let maintenance_slot = slot.clone();
        let maintenance = thread::spawn(move || {
            take_and_recycle(&maintenance_slot, STATE_MAINTENANCE);
        });
        let next_owner_slot = slot.clone();
        let next_owner = thread::spawn(move || take_and_recycle(&next_owner_slot, STATE_LIVE));
        owner.join().expect("owner");
        maintenance.join().expect("maintenance");
        next_owner.join().expect("next owner");
        assert_eq!(slot.state.load(Ordering::Acquire), STATE_FREE);
        assert_eq!(slot.active.load(Ordering::Relaxed), 0);
        slot.payload.with(|pointer| {
            // SAFETY: every child has joined; no mutator or lease remains.
            let completed = unsafe { *pointer };
            assert!((1..=3).contains(&completed));
        });
    });
}
