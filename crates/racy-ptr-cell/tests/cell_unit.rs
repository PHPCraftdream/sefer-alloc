//! Native (non-loom) single-threaded unit tests for [`RacyPtrCell`]:
//! sequential correctness of the fast path, init-once, OOM rollback + retry,
//! and the sentinel/null non-leak — the properties that do not need loom's
//! interleaving explorer.

#![cfg(not(loom))]

use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, Ordering};

use racy_ptr_cell::RacyPtrCell;

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
    // task #706: if `init` unwinds instead of returning, the RollbackGuard
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
    // sentinel never left INITIALIZING) -- exactly the livelock task #706
    // exists to close. A bounded wait turns "hangs forever" into a reported
    // test failure instead of wedging the whole test run; the spinning
    // thread (if the bug were present) would be orphaned, not joined, and
    // reaped at process exit.
    // `NonNull<Payload>` is `!Send`, so ferry the address (a plain `usize`)
    // across the channel instead and reconstruct `NonNull` on this thread.
    let cell = std::sync::Arc::new(cell);
    let cell2 = std::sync::Arc::clone(&cell);
    let (tx, rx) = std::sync::mpsc::channel();
    let _handle = std::thread::spawn(move || {
        let p = cell2
            .get_or_try_init(|| Some(leak(0xABCD)))
            .map(|p| p.as_ptr().expose_provenance());
        let _ = tx.send(p);
    });

    let addr = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect(
            "get_or_try_init after a panicking init did not return within 5s \
             -- the INITIALIZING sentinel is stuck forever (task #706's \
             livelock is back)",
        )
        .expect("init must succeed");
    // `addr` is the exposed provenance of a pointer this same process just
    // published via `expose_provenance()` above; reconstructing it via
    // `with_exposed_provenance_mut` (rather than a plain `as` cast) keeps
    // this test clean under `-Zmiri-strict-provenance` (task #774, finding
    // F2 -- mirrors task #709's identical fix one file over in
    // tests/loom_racy_ptr_cell.rs).
    let p = NonNull::new(core::ptr::with_exposed_provenance_mut::<Payload>(addr)).unwrap();

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
    // task #707: a SAFE init closure can construct and return the sentinel
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
    // task #708 (1/2): the align_of::<T>() >= 2 guard in `new`/`default` is
    // soundness/liveness-load-bearing -- an align-1 T could publish a real
    // pointer at address 1, which every reader would misread as the
    // INITIALIZING sentinel and spin on forever. This was documented (`#
    // Panics` on RacyPtrCell::new) but never tested: deleting the assert
    // previously left the whole suite green. `default()` is the doc's own
    // named runtime-panic route (the `new()` route is `const fn`, so a
    // `static` usage with an align-1 T fails to COMPILE via const-eval,
    // which is untestable here without a `compile_fail` doctest --
    // CLAUDE.md bans doctests; `default()`'s runtime arm checks the exact
    // same predicate).
    let _ = RacyPtrCell::<u8>::default();
}

#[test]
fn dbg_rollback_reenterable_happy_path_and_not_applicable_arm() {
    // task #708 (2/2): `dbg_rollback_reenterable`'s happy-path contract
    // (Some(true) + the restore postcondition) is asserted only in the
    // PARENT repo's own integration test, which does not ship with the
    // standalone published crate. Within crates/racy-ptr-cell itself the
    // only call site discards the result -- stubbing the probe to
    // unconditionally return None previously left the whole suite green.

    // Happy-path arm: a fresh (UNINIT) cell.
    let cell: RacyPtrCell<Payload> = RacyPtrCell::new();
    assert_eq!(
        cell.dbg_rollback_reenterable(),
        Some(true),
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
        None,
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
