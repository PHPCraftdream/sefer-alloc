#![cfg(all(feature = "alloc-xthread", feature = "internals"))]

//! R2-13 (src review round 2,
//! `docs/reviews/2026-09-22-120730-src-review-xa-round-2.md`): [`HeapOverflow`]'s
//! drain must (a) publish already-completed progress when a `reclaim` callback
//! panics mid-drain, with an explicit at-least-once retry contract for the ONE
//! in-flight entry, and (b) ENFORCE its single-consumer requirement via an
//! exclusive token instead of assuming it from the call sites.
//!
//! Pre-fix mechanism (the bug): the drain cleared each successfully processed
//! slot one at a time but published `head` only after the whole loop. A panic
//! out of a LATER callback left those cleared slots in front of the stale
//! `head`, so the next drain saw `ENTRY_EMPTY_BASE` at `head` and stopped
//! immediately — the ring jammed permanently and every remaining reclaim note
//! was stranded. And nothing rejected a second concurrent/reentrant
//! `drain(&self, ...)`: a caller could re-process entries the first pass had
//! reclaimed but not yet published.
//!
//! Every test here is a genuine counterfactual: each names the assertion that
//! fails against the pre-R2-13 code (see the per-test comments).

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use sefer_alloc::registry::heap_overflow::HeapOverflow;

/// Synthetic, never-dereferenced non-null "segment base" — same contract as
/// `tests/heap_overflow_drain_return.rs`'s helper (`HeapOverflow` stores and
/// compares these as `usize` and never reads through them).
fn synthetic_base(tag: usize) -> *mut u8 {
    core::ptr::without_provenance_mut((tag + 1) * 64)
}

// Distinct packed words so each entry's deliveries are identifiable.
const P0: u32 = 100;
const P1: u32 = 101;
const P2: u32 = 102;
const P3: u32 = 103;
const P4: u32 = 104;

/// Acceptance criterion 1 (panic progress): a panic after several successful
/// callbacks must not lose the completed progress, must leave the ring
/// continuable, and new pushes must still work.
///
/// Counterfactual: against the pre-R2-13 code the `second_pass` assertion
/// fails — `head` was never published past the cleared slot 0, so the second
/// drain observed `ENTRY_EMPTY_BASE` at `head == 0` and reclaimed NOTHING
/// (`second_pass == []`, `stop2 == Some(0)`), and `third_pass` stayed empty
/// forever after (the jammed ring never drained the new pushes either).
#[test]
fn panic_mid_drain_publishes_progress_retries_in_flight_entry_and_keeps_ring_usable() {
    let ring = HeapOverflow::new_boxed_for_test();
    assert!(ring.push(synthetic_base(0), P0));
    assert!(ring.push(synthetic_base(1), P1));
    assert!(ring.push(synthetic_base(2), P2));

    let first_pass = Rc::new(RefCell::new(Vec::new()));
    {
        let recorded = Rc::clone(&first_pass);
        let result = catch_unwind(AssertUnwindSafe(|| {
            ring.try_drain(|_base, packed| {
                recorded.borrow_mut().push(packed);
                if packed == P1 {
                    panic!("reclaim callback panics on the SECOND entry");
                }
            });
        }));
        assert!(
            result.is_err(),
            "the panicking callback must propagate out of try_drain"
        );
    }
    assert_eq!(
        *first_pass.borrow(),
        vec![P0, P1],
        "P0 was fully processed (reclaimed + cleared + cursor advanced past) \
         before the panic; P1's callback ran — and recorded its delivery — \
         before panicking (that partial effect is what the retry contract \
         makes the caller responsible for)"
    );

    // The counterfactual core: the guard's unwind Drop published head = 1
    // (past the fully-processed P0) and released the token, so this drain
    // must RUN (a leaked token would return None here) and must resume at
    // the in-flight entry, in order.
    let second_pass = Rc::new(RefCell::new(Vec::new()));
    let stop2 = {
        let recorded = Rc::clone(&second_pass);
        ring.try_drain(|_base, packed| recorded.borrow_mut().push(packed))
    };
    assert_eq!(
        stop2,
        Some(3),
        "drain must continue correctly after the caught panic (head published \
         past fully-processed entries; token released by the unwind Drop)"
    );
    assert_eq!(
        *second_pass.borrow(),
        vec![P1, P2],
        "the in-flight-at-panic entry P1 is RETRIED (the explicit at-least-once \
         choice documented on try_drain), P2 was never lost, and P0 — already \
         fully processed — is NOT re-delivered"
    );

    // Acceptance criterion 1's tail: new pushes still work — the queue did
    // not jam.
    assert!(ring.push(synthetic_base(3), P3));
    assert!(ring.push(synthetic_base(4), P4));
    let third_pass = Rc::new(RefCell::new(Vec::new()));
    let stop3 = {
        let recorded = Rc::clone(&third_pass);
        ring.try_drain(|_base, packed| recorded.borrow_mut().push(packed))
    };
    assert_eq!(stop3, Some(5), "ring fully caught up to the new tail");
    assert_eq!(*third_pass.borrow(), vec![P3, P4]);
}

/// Acceptance criterion 2a (reentrancy): a drain attempt from INSIDE the
/// live outer drain's own callback — the reentrant-consumer shape the old
/// `&self` signature silently allowed — must be rejected as busy, without
/// processing anything.
///
/// Counterfactual: against the pre-R2-13 code `every_reentrant_rejected`
/// and `inner_delivered` both fail — the inner drain saw the outer's
/// not-yet-published cursor (slot 0 was still non-empty: the outer callback
/// for P0 was mid-flight) and re-processed ALL THREE entries with its own
/// callback (`reentrant == Some(3)`, `inner_delivered == [P0, P1, P2]`).
#[test]
fn reentrant_drain_inside_callback_is_rejected_not_run() {
    let ring = HeapOverflow::new_boxed_for_test();
    assert!(ring.push(synthetic_base(0), P0));
    assert!(ring.push(synthetic_base(1), P1));
    assert!(ring.push(synthetic_base(2), P2));

    let delivered = RefCell::new(Vec::new());
    let inner_delivered = RefCell::new(Vec::new());
    let every_reentrant_rejected = RefCell::new(true);

    let stop = ring.try_drain(|_base, packed| {
        // The outer drain is LIVE and holds the exclusive token for the
        // whole pass, so this attempt must be rejected as busy — without
        // running this inner callback or consuming any entry.
        let reentrant = ring.try_drain(|_b, p| inner_delivered.borrow_mut().push(p));
        *every_reentrant_rejected.borrow_mut() &= reentrant.is_none();
        delivered.borrow_mut().push(packed);
    });

    assert_eq!(
        stop,
        Some(3),
        "the outer drain must complete normally, exactly once per entry"
    );
    assert!(
        *every_reentrant_rejected.borrow(),
        "every reentrant try_drain from inside the live outer drain must be \
         rejected as busy (None)"
    );
    assert!(
        inner_delivered.borrow().is_empty(),
        "the rejected reentrant drain must not process any entry"
    );
    assert_eq!(
        *delivered.borrow(),
        vec![P0, P1, P2],
        "the outer drain delivers every entry exactly once"
    );
}

/// Acceptance criterion 2b (consumer ownership across threads): while one
/// thread's drain is live (holding the exclusive token), a concurrent
/// `try_drain` from a second thread must be rejected as busy — and once the
/// holder's drain completes, the token must be released so fresh drains are
/// accepted again.
///
/// Counterfactual: against the pre-R2-13 code `saw_only_busy` fails — the
/// second thread's `try_drain` was not rejected by anything (no token
/// existed), so one of its attempts would have SUCCEEDED and re-processed
/// entries concurrently with the first thread's in-flight drain.
///
/// No sleeps: both sides spin on atomic flags with `yield_now`, so the
/// interleaving (contender attempts strictly while the outer drain holds
/// the token inside its first callback) is scheduling-safe, and a broken
/// (non-exclusive) token makes the contender exit early with
/// `saw_only_busy == false` instead of hanging either side.
#[test]
fn concurrent_drain_from_second_thread_is_rejected_while_token_held() {
    let ring = Arc::new(HeapOverflow::new_boxed_for_test());
    assert!(ring.push(synthetic_base(0), P0));
    assert!(ring.push(synthetic_base(1), P1));
    assert!(ring.push(synthetic_base(2), P2));

    let entered = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let saw_only_busy = Arc::new(AtomicBool::new(true));

    let ring_for_thread = Arc::clone(&ring);
    let entered_for_thread = Arc::clone(&entered);
    let done_for_thread = Arc::clone(&done);
    let saw_only_busy_for_thread = Arc::clone(&saw_only_busy);
    let contender = thread::spawn(move || {
        while !entered_for_thread.load(Ordering::Acquire) {
            thread::yield_now();
        }
        // The outer drain is now live and holds the exclusive token, so
        // every one of these attempts must be rejected as busy — and a
        // rejected attempt must never run this callback or consume an
        // entry. On any (buggy) success, record it and stop early so the
        // test fails loudly instead of hanging.
        for _ in 0..256 {
            let busy =
                ring_for_thread
                    .try_drain(|_base, _packed| {
                        saw_only_busy_for_thread.store(false, Ordering::Release);
                    })
                    .is_none();
            if !busy {
                break;
            }
            thread::yield_now();
        }
        done_for_thread.store(true, Ordering::Release);
    });

    let processed = Rc::new(RefCell::new(Vec::new()));
    let stop = {
        let processed = Rc::clone(&processed);
        let entered = Arc::clone(&entered);
        let done = Arc::clone(&done);
        ring.try_drain(move |_base, packed| {
            processed.borrow_mut().push(packed);
            entered.store(true, Ordering::Release);
            while !done.load(Ordering::Acquire) {
                thread::yield_now();
            }
        })
    };

    assert_eq!(
        stop,
        Some(3),
        "the owning drain must complete normally with every entry delivered \
         exactly once"
    );
    assert_eq!(*processed.borrow(), vec![P0, P1, P2]);
    assert!(
        saw_only_busy.load(Ordering::Acquire),
        "every concurrent try_drain attempt from the second thread must have \
         been rejected as busy while the first drain held the token"
    );
    contender.join().expect("contender thread must not panic");

    // The token must be fully released by the completed drain: a fresh
    // drain is accepted again (and finds the ring empty).
    let mut after = 0usize;
    assert_eq!(ring.try_drain(|_base, _packed| after += 1), Some(3));
    assert_eq!(after, 0, "the ring must be empty after the first drain");
}
