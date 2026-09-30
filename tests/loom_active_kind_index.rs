#![cfg(loom)]

use loom::sync::atomic::{AtomicBool, Ordering};
use loom::sync::Arc;
use loom::thread;
use std::sync::atomic::AtomicBool as ObserverBool;
use std::sync::Arc as ObserverArc;

fn model(clear_clean_bit: bool) -> bool {
    let violation_seen = ObserverArc::new(ObserverBool::new(false));
    let observer = ObserverArc::clone(&violation_seen);
    loom::model(move || {
        let observer = ObserverArc::clone(&observer);
        let pending = Arc::new(AtomicBool::new(false));
        let reservation_live = Arc::new(AtomicBool::new(true));
        let saw_live = Arc::new(AtomicBool::new(false));
        let producer_pending = Arc::clone(&pending);
        let producer_live = Arc::clone(&reservation_live);
        let producer_saw_live = Arc::clone(&saw_live);
        let producer = thread::spawn(move || {
            producer_saw_live.store(producer_live.load(Ordering::Acquire), Ordering::Release);
            thread::yield_now();
            producer_pending.fetch_or(true, Ordering::AcqRel);
            // Terminal publication is the last reservation access.
        });

        // The owner alone owns this plain (non-atomic) membership bit.
        let mut active = true;
        let mut credits = 1;
        if pending.swap(false, Ordering::AcqRel) {
            credits -= 1;
            active = false;
            reservation_live.store(false, Ordering::Release);
        } else if clear_clean_bit {
            active = false;
            reservation_live.store(false, Ordering::Release);
        }
        let removed_with_credit = !active && credits != 0;
        producer.join().unwrap();
        if active && pending.swap(false, Ordering::AcqRel) {
            credits -= 1;
            active = false;
            reservation_live.store(false, Ordering::Release);
        }
        let violation =
            removed_with_credit || !saw_live.load(Ordering::Acquire) || active || credits != 0;
        if violation {
            observer.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });
    violation_seen.load(std::sync::atomic::Ordering::Relaxed)
}

#[test]
fn owner_only_membership_survives_late_terminal_publication() {
    assert!(!model(false));
}

#[test]
fn clean_bit_clear_mutant_is_rejected() {
    assert!(model(true));
}
