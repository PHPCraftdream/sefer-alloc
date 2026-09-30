//! Reduced-geometry model of owner-only credits versus terminal bitmap cuts.
//! Credits are atomic only so the negative control can model the forbidden
//! producer mutation; the positive protocol has exactly one owner mutator.
#![cfg(loom)]

use loom::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

fn credit_lifetime(producer_retires_credit: bool) {
    loom::model(move || {
        let pending = Arc::new(AtomicU64::new(0));
        let credits = Arc::new(AtomicUsize::new(1));
        let reservation_live = Arc::new(AtomicBool::new(true));
        let producer_pending = Arc::clone(&pending);
        let producer_credits = Arc::clone(&credits);
        let producer_live = Arc::clone(&reservation_live);
        let producer = thread::spawn(move || {
            if producer_retires_credit {
                producer_credits.fetch_sub(1, Ordering::Relaxed);
            }
            thread::yield_now();
            assert!(
                producer_live.load(Ordering::Acquire),
                "reservation released before terminal publication"
            );
            producer_pending.fetch_or(1, Ordering::AcqRel);
            // After the terminal RMW the producer touches only the independent
            // sidecar/pin. In particular it never reads producer_live again.
        });

        let detached = pending.swap(0, Ordering::AcqRel);
        // Detach alone does not retire an instance. It remains credited until
        // logical free-list insertion/mark-free commits on the owning thread.
        if !producer_retires_credit {
            assert_eq!(credits.load(Ordering::Relaxed), 1);
            if detached != 0 {
                assert_eq!(credits.fetch_sub(1, Ordering::Relaxed), 1);
            }
        }
        if credits.load(Ordering::Relaxed) == 0 {
            reservation_live.store(false, Ordering::Release);
        }
        producer.join().unwrap();
        let later = pending.swap(0, Ordering::AcqRel);
        assert_eq!(detached | later, 1);
        assert_eq!(detached & later, 0);
        if !producer_retires_credit && later != 0 {
            assert_eq!(credits.fetch_sub(1, Ordering::Relaxed), 1);
        }
        assert_eq!(credits.load(Ordering::Relaxed), 0);
    });
}

#[test]
fn unpublished_and_detached_instances_hold_the_reservation_credit() {
    credit_lifetime(false);
}

#[test]
#[should_panic(expected = "reservation released before terminal publication")]
fn negative_producer_retirement_allows_premature_release() {
    credit_lifetime(true);
}
