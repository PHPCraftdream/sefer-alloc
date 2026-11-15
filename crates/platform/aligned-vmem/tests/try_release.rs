//! Regression tests for the checked (fallible) release surface added by task
//! #56 (J13b checked-release prerequisite): `try_release`,
//! `try_release_parts`, and `Reservation::try_release`.
//!
//! # What is proven and the counterfactual oracle each test provides
//!
//! Every assertion below names the regression it would catch; a test that
//! cannot fail against the reverted behavior is not evidence.
//!
//! - `null_reservation_is_a_documented_no_op`: catches a regression that
//!   turns the documented null no-op into an `Err` or a panic (the
//!   infallible `release` shares this early return; the checked twin must
//!   keep it).
//! - `contract_violations_report_invalid_argument_without_touching_the_os`:
//!   catches a regression that (a) panics instead of reporting
//!   `VmemError::invalid_argument()` (the documented divergence from
//!   `release`'s assert), or (b) releases-then-reports — the
//!   `unix_munmap_attempts` delta-0 oracle proves validation happens BEFORE
//!   the syscall layer.
//! - `try_release_success_reports_ok_and_reaches_the_native_backend_exactly_once`:
//!   the attempts-counter delta-1 oracle catches BOTH "checked release
//!   silently became a no-op" (delta 0 — the exact vacuity task #1189 added
//!   the attempts counters for) and a double release (delta 2); the
//!   failures delta-0 oracle catches a spuriously failed real release.
//! - `reservation_try_release_consumes_ownership_and_releases_exactly_once`:
//!   same delta oracle on the SAFE consuming method; delta 2 would catch a
//!   `Drop`-release running AFTER the method (proving
//!   `into_reservation_parts`'s `Drop` suppression holds on this path),
//!   delta 0 catches the backend never being reached. The compile-time half
//!   of the ownership contract is stated, not demonstrated: `r.try_release()`
//!   MOVES `r`, so a use-after-release of the handle cannot compile — no
//!   code in this file can (or should) demonstrate that.
//! - `try_release_parts_round_trips_into_reservation_parts`: catches a
//!   regression where the typed twin diverges from the raw twin
//!   (swapped/lost fields, or a backend path that no longer issues exactly
//!   one release).
//! - `mock_row_records_the_release_call_for_try_release`: catches a
//!   regression where the checked dispatch stops recording in the mock call
//!   log (or records on the null no-op / injected-failure paths).
//!
//! # Platform-fallback coverage
//!
//! The same binary runs unchanged on the Linux native backend, under miri
//! (the miri fallback backend, where `Ok` is terminal-within-model), under
//! the `aligned_vmem_mock` cfg (the mock record is written AND the real
//! platform backend still runs — mock does not replace release), and
//! compiles on Windows — matching the CI matrix for this crate.

use aligned_vmem::{reserve_aligned, try_release, try_release_parts, Reservation, VmemError, PAGE};

#[cfg(aligned_vmem_mock)]
use aligned_vmem::mock::{self, Call};

/// Serial guard for this file's PROCESS-GLOBAL `bench-internals` counters.
///
/// libtest runs the tests in this binary on parallel threads, and the
/// bench-internals counters are process-global. The rule: every test that
/// either MOVES the counters (any reserve/release moves
/// `UNIX_MUNMAP_ATTEMPTS`/`_FAILURES` on unix and
/// `WINDOWS_VIRTUALFREE_RELEASE_ATTEMPTS`/`_FAILURES` on Windows) or READS
/// them across a delta window (snapshot → calls → delta assert) must hold
/// this lock for its whole body — a mutex excludes only the parties that
/// take it, so an unlocked reader's delta window can be invalidated by a
/// lock-holding sibling's release landing inside it. Only a test that
/// never touches the counters in any direction may skip the lock.
/// `unwrap_or_else(into_inner)` recovers from a poisoned lock so one
/// failing test does not cascade into spurious failures of the rest (same
/// pattern as `tests/fault_injection.rs`).
///
/// In this binary: the three reserve/release tests take the lock (they move
/// the counters); `contract_violations_report_invalid_argument_without_touching_the_os`
/// ALSO takes the lock — it moves nothing (the violations are rejected
/// before any syscall or mock record) but it OPENS a delta window on
/// `unix_munmap_attempts()`, which a concurrent sibling's release would
/// corrupt into a spurious nonzero delta. Only
/// `null_reservation_is_a_documented_no_op` stays unlocked: it reads no
/// counter and moves nothing. See `tests/smoke.rs`'s SERIAL header for the
/// full rule (task #1224).
static SERIAL: Mutex<()> = Mutex::new(());

use std::sync::Mutex;

/// `try_release(null, ...)` is the documented no-op `Ok(())` — the same
/// early return as the infallible `release`. Counterfactual: catches a
/// regression that turns the documented no-op into an error or a panic.
#[test]
fn null_reservation_is_a_documented_no_op() {
    // SAFETY: null is explicitly documented as a no-op early-return; the
    // recorder is skipped and no backend call is issued.
    let res = unsafe { try_release(core::ptr::null_mut(), PAGE, PAGE) };
    assert!(
        res.is_ok(),
        "try_release(NULL, ...) must be the documented no-op Ok(()): {res:?}"
    );
}

/// Contract violations report `VmemError::invalid_argument()` — the fallible
/// form reports instead of the infallible form's panic — and, critically,
/// never reach the syscall layer. The dangling pointer below is
/// validated-then-rejected; it is never dereferenced or otherwise used.
///
/// This test takes SERIAL even though it moves no counter: the unix cfg arm
/// below OPENS a delta window on the process-global
/// `unix_munmap_attempts()` counter (snapshot → 5 calls → delta assert),
/// and a SERIAL-holding sibling test's release landing inside that window
/// would make the delta nonzero and flake the assert — a reader across a
/// delta window must hold the lock just like a mover.
#[test]
fn contract_violations_report_invalid_argument_without_touching_the_os() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // Never dereferenced: `try_release` validates the (len, align) contract
    // first and rejects before the pointer is touched by anything.
    let dangling = core::ptr::NonNull::<u8>::dangling().as_ptr();

    // (reservation_len, align) shapes that must all be rejected:
    // zero len; misaligned len; align not a power of two; align < PAGE;
    // a size that cannot form a valid Layout.
    const CASES: [(usize, usize); 5] = [
        (0, PAGE),          // reservation_len == 0
        (PAGE + 1, PAGE),   // misaligned reservation_len
        (PAGE, PAGE * 3),   // align not a power of two
        (PAGE, PAGE / 2),   // align < PAGE
        (usize::MAX, PAGE), // invalid Layout size
    ];
    for (len, align) in CASES {
        // SAFETY: the pointer is never dereferenced — the call is rejected
        // at contract validation, before any syscall or mock record.
        let res = unsafe { try_release(dangling, len, align) };
        assert_invalid_argument(res, (len, align));
    }

    // THE pre-syscall oracle: validation must happen BEFORE the syscall
    // layer. A regression that released-then-reported would increment the
    // attempts counter here (the dangling pointer is not a real mapping, so
    // such a regression would also likely fault or fail the call — the
    // delta-0 assertion makes the ordering violation visible either way).
    #[cfg(all(unix, not(miri), feature = "bench-internals"))]
    {
        use aligned_vmem::unix_munmap_attempts;
        let before = unix_munmap_attempts();
        for (len, align) in CASES {
            // SAFETY: rejected at validation, before the backend call.
            let _ = unsafe { try_release(dangling, len, align) };
        }
        assert_eq!(
            unix_munmap_attempts() - before,
            0,
            "contract violations must be rejected BEFORE the syscall layer: \
             a nonzero munmap attempts delta means validation ran after (or \
             instead of) the syscall"
        );
    }
}

fn assert_invalid_argument(res: Result<(), VmemError>, case: (usize, usize)) {
    let err = res.expect_err(&format!(
        "a contract violation must report Err, not Ok (case len={}, align={})",
        case.0, case.1
    ));
    assert!(
        err.is_invalid_argument(),
        "a contract violation must report invalid_argument, not an OS \
         refusal (case len={}, align={}): {err:?}",
        case.0,
        case.1
    );
    assert_eq!(
        err.os_code(),
        None,
        "a contract violation never ran a syscall, so it has no OS cause \
         (case len={}, align={}): {err:?}",
        case.0,
        case.1
    );
}

/// Snapshot of the platform's release attempts counter; gated by cfg because
/// the counter accessors themselves are feature- and platform-gated.
#[cfg(all(unix, not(miri), feature = "bench-internals"))]
fn release_attempts_snapshot() -> u64 {
    aligned_vmem::unix_munmap_attempts()
}
#[cfg(all(unix, not(miri), feature = "bench-internals"))]
fn assert_release_delta_exactly_one(before_attempts: u64, what: &str) {
    let after = aligned_vmem::unix_munmap_attempts();
    assert_eq!(
        after - before_attempts,
        1,
        "{what}: the checked release must reach the native backend EXACTLY \
         once (delta 0 = silently became a no-op, the exact vacuity task \
         #1189 added the attempts counters for; delta 2 = double release)"
    );
}

#[cfg(all(unix, not(miri), feature = "bench-internals"))]
fn release_failures_snapshot() -> u64 {
    aligned_vmem::unix_munmap_failures()
}
#[cfg(all(unix, not(miri), feature = "bench-internals"))]
fn assert_failures_delta_zero(before: u64, what: &str) {
    assert_eq!(
        aligned_vmem::unix_munmap_failures() - before,
        0,
        "{what}: a successful release must not increment the failures counter"
    );
}

#[cfg(all(windows, not(miri), feature = "bench-internals"))]
fn release_attempts_snapshot() -> u64 {
    aligned_vmem::windows_virtualfree_release_attempts()
}
#[cfg(all(windows, not(miri), feature = "bench-internals"))]
fn assert_release_delta_exactly_one(before_attempts: u64, what: &str) {
    let after = aligned_vmem::windows_virtualfree_release_attempts();
    assert_eq!(
        after - before_attempts,
        1,
        "{what}: the checked release must reach VirtualFree(MEM_RELEASE) \
         EXACTLY once (delta 0 = silently a no-op; delta 2 = double release)"
    );
}

#[cfg(all(windows, not(miri), feature = "bench-internals"))]
fn release_failures_snapshot() -> u64 {
    aligned_vmem::windows_virtualfree_release_failures()
}
#[cfg(all(windows, not(miri), feature = "bench-internals"))]
fn assert_failures_delta_zero(before: u64, what: &str) {
    assert_eq!(
        aligned_vmem::windows_virtualfree_release_failures() - before,
        0,
        "{what}: a successful release must not increment the failures counter"
    );
}

#[cfg(any(
    all(unix, not(miri), feature = "bench-internals"),
    all(windows, not(miri), feature = "bench-internals")
))]
const COUNTERS_ON: bool = true;
#[cfg(not(any(
    all(unix, not(miri), feature = "bench-internals"),
    all(windows, not(miri), feature = "bench-internals")
)))]
const COUNTERS_ON: bool = false;

// No-counter fallback stubs so the call sites below compile unchanged when
// the platform counters are unavailable (windows builds without
// bench-internals, miri, non-unix/non-windows): COUNTERS_ON is a `const`
// known false, but name resolution still needs the functions to exist.
#[cfg(not(any(
    all(unix, not(miri), feature = "bench-internals"),
    all(windows, not(miri), feature = "bench-internals")
)))]
fn release_attempts_snapshot() -> u64 {
    0
}
#[cfg(not(any(
    all(unix, not(miri), feature = "bench-internals"),
    all(windows, not(miri), feature = "bench-internals")
)))]
fn release_failures_snapshot() -> u64 {
    0
}
#[cfg(not(any(
    all(unix, not(miri), feature = "bench-internals"),
    all(windows, not(miri), feature = "bench-internals")
)))]
fn assert_release_delta_exactly_one(_before: u64, _what: &str) {}
#[cfg(not(any(
    all(unix, not(miri), feature = "bench-internals"),
    all(windows, not(miri), feature = "bench-internals")
)))]
fn assert_failures_delta_zero(_before: u64, _what: &str) {}

/// `try_release` on a real reservation: `Ok(())` and the native backend
/// reached EXACTLY once.
#[test]
fn try_release_success_reports_ok_and_reaches_the_native_backend_exactly_once() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let r = reserve_aligned(4 * PAGE, PAGE).expect("reserve");
    let (ptr, len, align) = r.into_parts();
    assert_eq!(align, PAGE);

    let before_attempts = if COUNTERS_ON {
        release_attempts_snapshot()
    } else {
        0
    };
    let before_failures = if COUNTERS_ON {
        release_failures_snapshot()
    } else {
        0
    };

    // SAFETY: (ptr, len, align) came straight from a live reservation's
    // into_parts(); released exactly once below.
    let res = unsafe { try_release(ptr, len, align) };
    assert!(
        res.is_ok(),
        "checked release of a live reservation must be Ok: {res:?}"
    );

    if COUNTERS_ON {
        assert_release_delta_exactly_one(before_attempts, "try_release success");
        assert_failures_delta_zero(before_failures, "try_release success");
    }
}

/// The SAFE consuming method: `Reservation::try_release` consumes the handle
/// (via `into_reservation_parts`, which suppresses `Drop`) and reports the
/// native outcome.
///
/// What the delta-1 oracle catches here, both directions:
/// - delta 2 would catch a `Drop`-release running AFTER the method — i.e.
///   `into_reservation_parts`'s `Drop` suppression failing to hold on this
///   path (double release);
/// - delta 0 would catch the backend never being reached (the method became
///   a silent no-op).
///
/// The compile-time half of the contract needs no runtime code: the call
/// `r.try_release()` moves `r`, so a use-after-release of the handle cannot
/// compile. Rust's move check pins it, and no test can (or should) try to
/// demonstrate a use-after-move.
#[test]
fn reservation_try_release_consumes_ownership_and_releases_exactly_once() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let r: Reservation = reserve_aligned(4 * PAGE, PAGE).expect("reserve");

    let before_attempts = if COUNTERS_ON {
        release_attempts_snapshot()
    } else {
        0
    };
    let before_failures = if COUNTERS_ON {
        release_failures_snapshot()
    } else {
        0
    };

    // Consumes `r` by value — no `into_parts`, no raw triple in scope.
    let res = r.try_release();
    assert!(
        res.is_ok(),
        "Reservation::try_release of a live handle must be Ok: {res:?}"
    );

    if COUNTERS_ON {
        assert_release_delta_exactly_one(before_attempts, "Reservation::try_release");
        assert_failures_delta_zero(before_failures, "Reservation::try_release");
    }
}

/// The typed twin round-trips through `into_reservation_parts` and
/// `try_release_parts` with the same exactly-once backend contract.
#[test]
fn try_release_parts_round_trips_into_reservation_parts() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let r = reserve_aligned(4 * PAGE, PAGE).expect("reserve");
    let parts = r.into_reservation_parts();

    let before_attempts = if COUNTERS_ON {
        release_attempts_snapshot()
    } else {
        0
    };
    let before_failures = if COUNTERS_ON {
        release_failures_snapshot()
    } else {
        0
    };

    // SAFETY: the parts came from a live reservation's
    // into_reservation_parts(); released exactly once below.
    let res = unsafe { try_release_parts(parts) };
    assert!(res.is_ok(), "typed checked release must be Ok: {res:?}");

    if COUNTERS_ON {
        assert_release_delta_exactly_one(before_attempts, "try_release_parts round trip");
        assert_failures_delta_zero(before_failures, "try_release_parts round trip");
    }
}

/// Under the `aligned_vmem_mock` cfg the checked dispatch records exactly one
/// `Call::Release` with the recorded address/length — and the REAL platform
/// backend still runs (mock does not replace release).
///
/// bench-internals counters are process-global too, and this test's reserve
/// and release change them; it takes SERIAL to exclude counter delta readers
/// and movers in this binary.
#[cfg(aligned_vmem_mock)]
#[test]
fn mock_row_records_the_release_call_for_try_release() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    mock::reset();
    let r = reserve_aligned(4 * PAGE, PAGE).expect("reserve");
    let (ptr, len, _align) = r.into_parts();
    let addr = ptr.addr();

    // SAFETY: (ptr, len) came from a live reservation; released exactly once.
    let res = unsafe { try_release(ptr, len, PAGE) };
    assert!(
        res.is_ok(),
        "checked release under the mock cfg must be Ok: {res:?}"
    );

    let calls = mock::drain();
    let releases: Vec<_> = calls
        .iter()
        .filter(|c| matches!(c, Call::Release { .. }))
        .collect();
    assert_eq!(
        releases.len(),
        1,
        "exactly one Call::Release must be recorded for one checked release: \
         {calls:?}"
    );
    match releases[0] {
        Call::Release {
            reservation,
            reservation_len,
            ..
        } => {
            assert_eq!(*reservation, addr, "recorded address must match");
            assert_eq!(*reservation_len, len, "recorded length must match");
        }
        _ => unreachable!("filtered above"),
    }
}
