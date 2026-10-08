//! R15-01: every live slot must be visited despite destructor panics.
#![cfg(feature = "experimental")]
#![allow(deprecated)]

use std::marker::PhantomData;
use std::panic::{catch_unwind, panic_any, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use sefer_alloc::EpochRegion;

struct Probe {
    slot: usize,
    panics: bool,
    drops: Arc<AtomicUsize>,
    sequence: Arc<AtomicUsize>,
    position: Arc<AtomicUsize>,
    // Send, but deliberately not UnwindSafe; the region must not add that bound.
    _not_unwind_safe: PhantomData<&'static mut ()>,
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
        self.position.store(
            self.sequence.fetch_add(1, Ordering::SeqCst),
            Ordering::SeqCst,
        );
        if self.panics {
            panic_any(self.slot);
        }
    }
}

fn check(first_panics: bool, last_panics: bool) {
    let drops = [Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))];
    let positions = [
        Arc::new(AtomicUsize::new(usize::MAX)),
        Arc::new(AtomicUsize::new(usize::MAX)),
    ];
    let sequence = Arc::new(AtomicUsize::new(0));
    let region = EpochRegion::with_capacity(2);
    // Free indices are popped: insertion 1 -> slot 1, insertion 2 -> slot 0.
    for slot in [1, 0] {
        let handle = region
            .insert(Probe {
                slot,
                panics: if slot == 0 { first_panics } else { last_panics },
                drops: Arc::clone(&drops[slot]),
                sequence: Arc::clone(&sequence),
                position: Arc::clone(&positions[slot]),
                _not_unwind_safe: PhantomData,
            })
            .unwrap_or_else(|_| panic!("two-slot insertion must succeed"));
        // Debug exposes the existing opaque handle's index without a new API.
        assert!(format!("{handle:?}").contains(&format!("index: {slot},")));
    }
    assert_eq!(region.len(), 2);
    let result = catch_unwind(AssertUnwindSafe(move || drop(region)));
    let first_drops = drops[0].load(Ordering::SeqCst);
    let later_live_drops = drops[1].load(Ordering::SeqCst);
    println!(
        "first_panics={first_panics} last_panics={last_panics} caught={} first_drops={first_drops} later_live_drops={later_live_drops}",
        result.is_err()
    );
    assert_eq!(first_drops, 1);
    assert_eq!(later_live_drops, 1, "later live slot must not leak");
    assert_eq!(positions[0].load(Ordering::SeqCst), 0);
    assert_eq!(positions[1].load(Ordering::SeqCst), 1);
    match result {
        Ok(()) => assert!(!first_panics && !last_panics),
        Err(payload) => {
            assert!(first_panics || last_panics);
            assert_eq!(
                payload.downcast_ref::<usize>(),
                Some(&if first_panics { 0 } else { 1 }),
                "resume the first panic in actual slot traversal order"
            );
        }
    }
}

#[test]
fn control_drops_both_once_in_slot_order() {
    check(false, false);
}

// Red with the old drop loop: later_live_drops=0.
#[test]
fn first_panic_still_drops_later_live_slot() {
    check(true, false);
}

#[test]
fn last_panic_preserves_earlier_cleanup_and_payload() {
    check(false, true);
}

// Red with the old drop loop: the second destructor is never reached.
#[test]
fn both_panic_drop_both_and_resume_first_without_abort() {
    check(true, true);
}
