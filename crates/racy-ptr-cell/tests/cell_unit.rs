//! Native (non-loom) single-threaded unit tests for [`RacyPtrCell`]:
//! sequential correctness of the fast path, init-once, OOM rollback + retry,
//! and the sentinel/null non-leak — the properties that do not need loom's
//! interleaving explorer.

#![cfg(not(loom))]

use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, Ordering};

use racy_ptr_cell::{RacyPtrCell, RollbackProbe};

#[repr(align(4))]
struct Payload {
    marker: u32,
}

fn leak(marker: u32) -> NonNull<Payload> {
    NonNull::from(Box::leak(Box::new(Payload { marker })))
}

#[test]
fn get_is_none_until_initialised() {
    let cell: RacyPtrCell<Payload> = RacyPtrCell::new();
    assert!(cell.get().is_none());
    assert!(!cell.dbg_is_ready());
}

#[test]
fn init_runs_once_then_fast_path() {
    let cell: RacyPtrCell<Payload> = RacyPtrCell::new();
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
    let cell: RacyPtrCell<Payload> = RacyPtrCell::new();

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
    let cell: RacyPtrCell<Payload> = RacyPtrCell::new();
    let sentinel = NonNull::new(core::ptr::without_provenance_mut::<Payload>(1)).unwrap();
    let _ = cell.get_or_try_init(|| Some(sentinel));
}

#[test]
fn oom_rolls_back_and_retry_succeeds() {
    let cell: RacyPtrCell<Payload> = RacyPtrCell::new();

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
#[should_panic(expected = "RacyPtrCell<T> requires align_of::<T>() >= 2")]
fn align_of_one_payload_panics_at_construction() {
    // The align_of::<T>() >= 2 guard in `new`/`default` is
    // soundness/liveness-load-bearing -- an align-1 T could publish a real
    // pointer at address 1, which every reader would misread as the
    // INITIALIZING sentinel and spin on forever. This was documented (`#
    // Panics` on RacyPtrCell::new) but never tested: deleting the assert
    // previously left the whole suite green. `default()` is the doc's own
    // named runtime-panic route (the `new()` route is `const fn`, so a
    // `static` usage with an align-1 T fails to COMPILE via const-eval,
    // which is untestable here without a `compile_fail` doctest --
    // This repository bans doctests; `default()`'s runtime arm checks the exact
    // same predicate).
    let _ = RacyPtrCell::<u8>::default();
}

#[test]
fn dbg_rollback_reenterable_happy_path_and_not_applicable_arm() {
    // `dbg_rollback_reenterable`'s happy-path contract (Proven + the
    // restore postcondition) used to be asserted only in the PARENT repo's
    // own integration test, which does not ship with the standalone
    // published crate. Within crates/racy-ptr-cell itself the only call site
    // discarded the result -- stubbing the probe to unconditionally return
    // the not-applicable arm left the whole suite green.

    // Happy-path arm: a fresh (UNINIT) cell.
    let cell: RacyPtrCell<Payload> = RacyPtrCell::new();
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
