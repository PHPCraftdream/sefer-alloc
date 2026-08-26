//! Native (non-loom) tests for [`OncePtrCell`]: sequential correctness of the
//! fast path, init-once, OOM rollback + retry, and the sentinel/null
//! non-leak, plus a handful of concurrency/rollback regression tests (a
//! real background thread racing a panicking `init`) that need a genuine
//! `std::thread` handshake rather than loom's interleaving explorer —
//! `catch_unwind` does not compose with loom's model checker, so these
//! properties live here instead of in the loom suite.

#![cfg(not(loom))]

use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, Ordering};

use once_ptr_cell::{OncePtrCell, RollbackProbe};

#[repr(align(4))]
struct Payload {
    marker: u32,
}

fn leak(marker: u32) -> NonNull<Payload> {
    NonNull::from(Box::leak(Box::new(Payload { marker })))
}

#[test]
fn get_is_none_until_initialised() {
    let cell: OncePtrCell<Payload> = OncePtrCell::new();
    assert!(cell.get().is_none());
    assert!(!cell.dbg_is_ready());
}

#[test]
fn init_runs_once_then_fast_path() {
    let cell: OncePtrCell<Payload> = OncePtrCell::new();
    let calls = AtomicU32::new(0);

    let p1 = cell
        .get_or_try_init(|| {
            calls.fetch_add(1, Ordering::Relaxed);
            Some(leak(0x1111))
        })
        .unwrap();
    // Second call hits the fast path — no second init.
    let p2 = cell
        .get_or_try_init(|| {
            calls.fetch_add(1, Ordering::Relaxed);
            Some(leak(0x2222))
        })
        .unwrap();

    assert_eq!(p1, p2, "same published pointer");
    assert_eq!(calls.load(Ordering::Relaxed), 1, "init ran exactly once");
    assert!(cell.dbg_is_ready());
    // SAFETY: p1 is the leaked, still-live payload.
    assert_eq!(unsafe { p1.as_ref().marker }, 0x1111);

    // get() agrees.
    assert_eq!(cell.get(), Some(p1));

    // SAFETY: p1 was leaked exactly once by leak()'s Box::leak and never
    // freed since; reclaiming it here (once, at test end) pairs with that
    // leak.
    unsafe { drop(Box::from_raw(p1.as_ptr())) };
}

#[test]
fn panicking_init_rolls_back_and_subsequent_call_succeeds() {
    // If `init` unwinds instead of returning, the RollbackGuard
    // must still roll the INITIALIZING sentinel back to null -- otherwise
    // every subsequent caller (this same cell, any thread) spins forever on
    // the loser path, since nothing else will ever move the cell out of
    // INITIALIZING.
    let cell: OncePtrCell<Payload> = OncePtrCell::new();

    // First attempt: init PANICS. Catch the unwind.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cell.get_or_try_init(|| -> Option<NonNull<Payload>> {
            panic!("simulated init panic");
        })
    }));
    assert!(
        result.is_err(),
        "the panic must propagate out of get_or_try_init"
    );

    // The subsequent call must succeed -- the cell recovered. Run it on a
    // background thread and bound OUR wait with a timeout: WITHOUT the
    // RollbackGuard fix, this call spins forever on the loser path (the
    // sentinel never left INITIALIZING) -- exactly the livelock the
    // rollback guard exists to close. A bounded wait turns "hangs forever" into a reported
    // test failure instead of wedging the whole test run; the spinning
    // thread (if the bug were present) would be orphaned, not joined, and
    // reaped at process exit.
    // `NonNull<Payload>` is `!Send`, so the worker thread does not send the
    // pointer itself across the channel at all -- only a completion signal
    // (an earlier version of this test ferried the pointer's address as a
    // `usize` and reconstructed it via `with_exposed_provenance_mut`, with a
    // comment claiming this was clean under `-Zmiri-strict-provenance` --
    // false, since that flag forbids the exposed-provenance mechanism
    // entirely, as commit `ead400a`'s own message already admitted. Fetching
    // the pointer via `cell.get()` on THIS thread after the signal arrives
    // sidesteps the provenance question outright: no pointer-to-integer
    // round-trip anywhere in this test).
    let cell = std::sync::Arc::new(cell);
    let cell2 = std::sync::Arc::clone(&cell);
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let ok = cell2.get_or_try_init(|| Some(leak(0xABCD))).is_some();
        let _ = tx.send(ok);
    });

    let init_succeeded = rx.recv_timeout(std::time::Duration::from_secs(5)).expect(
        "get_or_try_init after a panicking init did not return within 5s \
         -- the INITIALIZING sentinel is stuck forever (the rollback \
         guard's livelock is back)",
    );
    assert!(init_succeeded, "init must succeed");
    // The signal proved the worker got past `get_or_try_init`; join it too,
    // so the worker's own lifetime and any panic inside it are this test's
    // business rather than something silently reaped at process exit. Only
    // reachable on the green path -- if the sentinel were stuck, the
    // `recv_timeout` above has already failed the test, and the spinning
    // worker stays deliberately unjoined (joining it would hang forever,
    // turning a reported failure back into a wedged run).
    handle
        .join()
        .expect("the worker thread must not panic on the green path");
    let p = cell.get().expect("cell is ready after the signal above");

    assert!(cell.dbg_is_ready());
    assert_eq!(cell.get(), Some(p));
    // SAFETY: p is the leaked, still-live payload.
    assert_eq!(unsafe { p.as_ref().marker }, 0xABCD);
    // SAFETY: p was leaked exactly once by leak()'s Box::leak (via the
    // background thread's init closure) and never freed since; reclaiming
    // it here (once, at test end) pairs with that leak.
    unsafe { drop(Box::from_raw(p.as_ptr())) };
}

#[test]
fn concurrent_get_or_try_init_started_before_unwind_completes_still_succeeds() {
    // What this test proves, precisely -- and what it does NOT: the loser
    // thread's call to `get_or_try_init` is issued no later than the point
    // where it observes the winner already holds the sentinel (`in_init`),
    // and the winner cannot even BEGIN its unwind until it observes the
    // loser's own "about to call" signal in return -- both of those orderings
    // are real, `Release`/`Acquire`-backed guarantees, not timing. What is
    // NOT guaranteed is which of the loser's two possible races actually
    // happens after that: the scheduler could still run the winner's entire
    // panic-unwind-rollback to completion BEFORE the loser's own
    // `get_or_try_init` call reaches its first `compare_exchange` -- in which
    // case the loser observes an already-rolled-back `null` and wins the CAS
    // itself directly, never entering the loser/spin branch at all. Either
    // way the loser must succeed within the timeout below, but only ONE of
    // the two interleavings actually exercises the spin-and-wake path this
    // test was originally written to pin down.
    //
    // An earlier version of this test and its name claimed the stronger,
    // undemonstrated property ("a loser thread that is ALREADY spinning...
    // at the exact moment the winner's init unwinds") on the strength of "a
    // panic-driven unwind is orders of magnitude slower than a spin_loop
    // iteration" -- true on average, but a probabilistic argument about
    // relative speeds is not the same thing as a proof, and does not belong
    // stated as one. Closing that gap for real needs a hook inside the
    // loser's own CAS/spin path (a non-default verification feature, an
    // internal unit target, or a loom-friendly abstraction) that this crate
    // does not have and should not grow solely to make one integration test
    // airtight -- see this crate's own repeated stance elsewhere against
    // adding test-only production surface without a stronger reason. This
    // test is renamed and reworded to state exactly the (still real, still
    // useful) property it demonstrates instead.
    let cell = std::sync::Arc::new(OncePtrCell::<Payload>::new());
    let in_init = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let loser_about_to_call = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let (cw, iw, lw) = (
        std::sync::Arc::clone(&cell),
        std::sync::Arc::clone(&in_init),
        std::sync::Arc::clone(&loser_about_to_call),
    );
    let winner = std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cw.get_or_try_init(|| -> Option<NonNull<Payload>> {
                iw.store(true, Ordering::Release);
                while !lw.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                panic!("simulated init panic with a concurrent caller in flight");
            })
        }));
        assert!(
            result.is_err(),
            "the panic must propagate out of get_or_try_init"
        );
    });

    let (tx, rx) = std::sync::mpsc::channel();
    let (cl, il, ll) = (
        std::sync::Arc::clone(&cell),
        std::sync::Arc::clone(&in_init),
        std::sync::Arc::clone(&loser_about_to_call),
    );
    let loser = std::thread::spawn(move || {
        while !il.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        ll.store(true, Ordering::Release);
        let ok = cl.get_or_try_init(|| Some(leak(0xF00D))).is_some();
        let _ = tx.send(ok);
    });

    // Bounded wait, not an unconditional join: on the failure path (rollback
    // stuck), the loser spins forever and must stay unjoined, reported as a
    // timeout rather than a wedged test run -- the exact pattern the
    // existing panic-rollback test above already establishes.
    let loser_succeeded = rx.recv_timeout(std::time::Duration::from_secs(5)).expect(
        "the loser did not return within 5s -- a concurrent caller in flight \
         when the winner's init unwound was not woken (the INITIALIZING \
         sentinel is stuck)",
    );
    assert!(
        loser_succeeded,
        "the concurrent caller must succeed after the winner's rollback -- either by \
         re-racing and winning the CAS itself, or by observing the live sentinel and \
         spinning until the winner's rollback wakes it"
    );

    // Green path only: both threads are known to have finished by now.
    winner
        .join()
        .expect("winner thread must not panic outside its own catch_unwind");
    loser.join().expect("loser thread must not panic");

    let p = cell
        .get()
        .expect("cell is ready after the loser's successful init");
    assert!(cell.dbg_is_ready());
    // SAFETY: p is the leaked, still-live payload.
    assert_eq!(unsafe { p.as_ref().marker }, 0xF00D);
    // SAFETY: p was leaked exactly once by leak()'s Box::leak (via the
    // loser's init closure) and never freed since; reclaiming it here
    // (once, at test end) pairs with that leak.
    unsafe { drop(Box::from_raw(p.as_ptr())) };
}

#[test]
#[should_panic(expected = "init returned the null/sentinel address")]
fn init_returning_the_sentinel_address_panics() {
    // A SAFE init closure can construct and return the sentinel
    // address (1) -- `NonNull` only rules out null, not this specific
    // address. Without a release-active guard this would get published as
    // if it were READY, and every reader (this thread's own fast path
    // included, plus every future caller) would misclassify it as
    // `INITIALIZING` forever, since `is_ready`/`is_empty`-style checks key
    // off the exact same address. This must be a release-active `assert!`,
    // not `debug_assert!` (which would compile out and let this test pass
    // vacuously) -- verified via a counterfactual: temporarily reverting to
    // `debug_assert!` and re-running under `--release` makes this test fail
    // (no panic occurs), confirming the check is genuinely load-bearing.
    let cell: OncePtrCell<Payload> = OncePtrCell::new();
    let sentinel = NonNull::new(core::ptr::without_provenance_mut::<Payload>(1)).unwrap();
    let _ = cell.get_or_try_init(|| Some(sentinel));
}

#[test]
fn oom_rolls_back_and_retry_succeeds() {
    let cell: OncePtrCell<Payload> = OncePtrCell::new();

    // First attempt: init returns None (OOM) → rollback → None returned.
    let first = cell.get_or_try_init(|| None);
    assert!(first.is_none(), "OOM attempt returns None");
    assert!(!cell.dbg_is_ready(), "sentinel rolled back to UNINIT");
    assert!(cell.get().is_none());

    // Retry: now succeeds and publishes.
    let p = cell.get_or_try_init(|| Some(leak(0x9999))).unwrap();
    assert!(cell.dbg_is_ready());
    assert_eq!(cell.get(), Some(p));
    // SAFETY: p is the leaked, still-live payload.
    assert_eq!(unsafe { p.as_ref().marker }, 0x9999);
    // SAFETY: p was leaked exactly once by leak()'s Box::leak on the retry
    // attempt and never freed since; reclaiming it here (once, at test end)
    // pairs with that leak.
    unsafe { drop(Box::from_raw(p.as_ptr())) };
}

#[test]
#[should_panic(expected = "OncePtrCell<T> requires align_of::<T>() >= 2")]
fn align_of_one_payload_panics_at_construction() {
    // The align_of::<T>() >= 2 guard in `new`/`default` is
    // soundness/liveness-load-bearing -- an align-1 T could publish a real
    // pointer at address 1, which every reader would misread as the
    // INITIALIZING sentinel and spin on forever. This was documented (`#
    // Panics` on OncePtrCell::new) but never tested: deleting the assert
    // previously left the whole suite green. `default()` is the doc's own
    // named runtime-panic route (the `new()` route is `const fn`, so a
    // `static` usage with an align-1 T fails to COMPILE via const-eval,
    // which is untestable here without a `compile_fail` doctest --
    // This repository bans doctests; `default()`'s runtime arm checks the exact
    // same predicate).
    let _ = OncePtrCell::<u8>::default();
}

#[test]
fn dbg_rollback_reenterable_happy_path_and_not_applicable_arm() {
    // `dbg_rollback_reenterable`'s happy-path contract (Proven + the
    // restore postcondition) used to be asserted only in the PARENT repo's
    // own integration test, which does not ship with the standalone
    // published crate. Within crates/once-ptr-cell itself the only call site
    // discarded the result -- stubbing the probe to unconditionally return
    // the not-applicable arm left the whole suite green.

    // Happy-path arm: a fresh (UNINIT) cell.
    let cell: OncePtrCell<Payload> = OncePtrCell::new();
    assert_eq!(
        cell.dbg_rollback_reenterable(),
        RollbackProbe::Proven,
        "on a fresh UNINIT cell the probe must observe and restore UNINIT"
    );
    // Restore postcondition: the cell is back to UNINIT, not left at the
    // sentinel or leaked into some other state.
    assert!(!cell.dbg_is_ready());
    assert!(cell.get().is_none());
    // A subsequent real get_or_try_init must succeed normally.
    let p = cell.get_or_try_init(|| Some(leak(0x7777))).unwrap();
    assert!(cell.dbg_is_ready());
    assert_eq!(cell.get(), Some(p));
    // SAFETY: p is the leaked, still-live payload.
    assert_eq!(unsafe { p.as_ref().marker }, 0x7777);

    // Not-applicable arm: a READY cell (the probe's entry CAS observes
    // something other than UNINIT and must not touch the cell at all).
    assert_eq!(
        cell.dbg_rollback_reenterable(),
        RollbackProbe::NotApplicable,
        "on an already-READY cell the probe is not applicable"
    );
    // Confirm the probe truly left the READY cell untouched.
    assert!(cell.dbg_is_ready());
    assert_eq!(cell.get(), Some(p));

    // SAFETY: p was leaked exactly once by leak()'s Box::leak and never
    // freed since; reclaiming it here (once, at test end) pairs with that
    // leak.
    unsafe { drop(Box::from_raw(p.as_ptr())) };
}

#[test]
fn debug_reports_the_three_states_without_a_t_debug_bound() {
    // Payload deliberately does not implement Debug -- this compiling at all
    // is part of what the test proves (no `T: Debug` bound leaks through).
    let cell: OncePtrCell<Payload> = OncePtrCell::new();
    assert_eq!(format!("{cell:?}"), "OncePtrCell(Uninit)");

    // `fmt` takes `&self`, so the init closure can format the SAME cell
    // while it still holds the sentinel -- the only single-threaded way to
    // observe `Initializing` deterministically. Without this, the impl's
    // `SENTINEL_INITIALIZING` arm has no test coverage at all: deleting it
    // (collapsing the output to `Ready(0x1)`) would leave the suite green.
    let p = cell
        .get_or_try_init(|| {
            assert_eq!(format!("{cell:?}"), "OncePtrCell(Initializing)");
            Some(leak(0xBEEF))
        })
        .unwrap();
    assert_eq!(format!("{cell:?}"), format!("OncePtrCell(Ready({p:p}))"));

    // SAFETY: p was leaked exactly once by leak()'s Box::leak and never
    // freed since; reclaiming it here (once, at test end) pairs with that
    // leak.
    unsafe { drop(Box::from_raw(p.as_ptr())) };
}

/// This does not test `#[repr(transparent)]` itself -- the compiler already
/// rejects a `PhantomData` field that would violate it, so a wrong
/// `repr(transparent)` is a compile error, not a silent bug. What this DOES
/// guard is a later, unrelated edit that widens the struct (a new field, a
/// changed field type) while somehow keeping it compiling -- e.g. a second
/// non-ZST field would break `repr(transparent)`'s own requirement and fail
/// to build, but a change to a still-single-field-but-differently-sized
/// representation would not. Pinning the size/align equality here turns
/// that class of drift into a test failure instead of a silent layout
/// change a downstream consumer would only discover by measuring it
/// themselves.
#[test]
fn layout_matches_a_single_atomic_ptr() {
    assert_eq!(
        core::mem::size_of::<OncePtrCell<Payload>>(),
        core::mem::size_of::<core::sync::atomic::AtomicPtr<Payload>>(),
        "OncePtrCell<T> must be exactly one word, matching its documented \
         layout guarantee"
    );
    assert_eq!(
        core::mem::align_of::<OncePtrCell<Payload>>(),
        core::mem::align_of::<core::sync::atomic::AtomicPtr<Payload>>(),
        "OncePtrCell<T> must have the same alignment as the AtomicPtr<T> it wraps"
    );
}
