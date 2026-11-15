//! Tests for the release-side `fault-injection` hook:
//! `fault_injection::arm_fail_next_release`, consulted by the checked release
//! dispatch (`try_release` / `try_release_parts` /
//! `Reservation::try_release`).
//!
//! # Why this file is gated WITHOUT `not(aligned_vmem_mock)`
//!
//! Mirrors `tests/fault_injection.rs`'s header comment with INVERTED
//! rationale: the commit/decommit hooks are consulted only from call sites
//! that the mock cfg compiles OUT, so testing them under the mock cfg would
//! be vacuous and that file excludes the combination. The RELEASE hook is
//! the opposite case: `dispatch_try_release` is compiled under EVERY cfg
//! because mock does not replace the release backend — the real platform
//! backend still runs under `aligned_vmem_mock` — so the hook stays LIVE
//! under the mock cfg, and the mock CI row exercises it against the real
//! backend. Excluding the combination here would delete real coverage, not
//! vacuity.
//!
//! # What firing proves
//!
//! `arm_fail_next_release` is consumed in API dispatch before the checked
//! backend is entered; it returns the explicit no-code sentinel. The native
//! attempts delta-zero assertion verifies no release call occurred. This
//! binary tests only that pre-syscall behavior. The separate
//! `try_release_backend_result.rs` binary tests the checked backends' shared
//! native-result classification using a no-code surrogate; neither hook proves
//! actual kernel refusal.
//!
//! # Serial lock
//!
//! The hook state is process-global and mutex-protected; every test takes
//! SERIAL so arm/fire sequences and native-counter deltas cannot interleave.

#![cfg(feature = "fault-injection")]

use aligned_vmem::fault_injection::arm_fail_next_release;
use aligned_vmem::{reserve_aligned, try_release, Reservation, PAGE};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

/// `arm_fail_next_release(1)` forces exactly the NEXT checked release to
/// return `Err` before entering the backend; the following fresh reservation
/// reaches the real backend and succeeds.
///
/// The munmap attempts delta-0 assertion is THE pre-syscall oracle: a
/// regression that consulted the hook AFTER issuing the syscall would leave
/// a nonzero delta and return an unproven/misreported outcome after release
/// may already have succeeded; it would not be safe to retry.
#[test]
fn arm_fail_next_release_forces_exactly_one_err_without_touching_the_os() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // Disarm any residue from a prior test in this binary (the hook is
    // process-global and a sibling's armed failures could otherwise leak in).
    arm_fail_next_release(0);

    let r = reserve_aligned(4 * PAGE, PAGE).expect("reserve");
    let (ptr, len, align) = r.into_parts();

    #[cfg(all(unix, not(miri), feature = "bench-internals"))]
    let before = aligned_vmem::unix_munmap_attempts();

    arm_fail_next_release(1);

    // SAFETY: (ptr, len, align) came from a live reservation; the armed
    // hook makes this call return before the backend call, so the
    // reservation is NOT released here and MUST NOT be retried (the
    // exact-once-even-on-Err contract). `into_parts` suppressed `Drop`, so
    // no implicit release runs afterwards; the test conservatively retains
    // the (unproven-release) reservation for the process lifetime, which is
    // exactly the documented caller obligation on `Err`.
    let res = unsafe { try_release(ptr, len, align) };
    let err = res.expect_err("armed release hook must force the call to fail");
    assert_eq!(
        err.os_code(),
        None,
        "an injected failure ran no syscall, so it must carry the no-code \
         sentinel: {err:?}"
    );
    assert!(
        !err.is_invalid_argument(),
        "an injected OS refusal is distinct from a contract violation: {err:?}"
    );

    #[cfg(all(unix, not(miri), feature = "bench-internals"))]
    assert_eq!(
        aligned_vmem::unix_munmap_attempts() - before,
        0,
        "the hook must be consulted BEFORE the syscall: a nonzero munmap \
         attempts delta means the syscall ran and the mapping was leaked \
         behind an Err"
    );

    // One-shot: the hook consumed and disarmed itself, so the next checked
    // release succeeds against the real backend. Catches an off-by-one that
    // fails calls forever. Note this is a DIFFERENT reservation, not a
    // retry of the triple above — the exact-once-even-on-Err contract
    // forbids reusing a triple after Err.
    let r2 = reserve_aligned(4 * PAGE, PAGE).expect("reserve after fault");
    let (ptr2, len2, align2) = r2.into_parts();
    // SAFETY: fresh live reservation from into_parts(); released exactly once.
    let res2 = unsafe { try_release(ptr2, len2, align2) };
    assert!(
        res2.is_ok(),
        "the hook is one-shot: the following release must hit the real \
         backend and succeed: {res2:?}"
    );
}

/// `arm_fail_next_release(n)` fires EXACTLY `n` times, then the hook is
/// disarmed and real releases succeed again.
///
/// Per the documented exact-once-even-on-`Err` contract, a triple must never
/// be reused after an `Err` — the two failing calls therefore each use a
/// fresh reservation (reserve, arm/confirm armed, one Err call, drop), and
/// the third (successful) call uses a third reservation. No retry on the
/// same triple anywhere.
#[test]
fn arm_fail_next_release_n_fires_exactly_n_times_then_recovers() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_next_release(0); // disarm residue from a prior test in this binary

    arm_fail_next_release(2);

    // Firing 1 of 2 (fresh reservation — never reused after the Err).
    let r1 = reserve_aligned(4 * PAGE, PAGE).expect("reserve 1");
    let (p1, l1, a1) = r1.into_parts();
    // SAFETY: fresh live reservation; the armed hook fails this call
    // pre-syscall, so nothing is released and the triple is never retried.
    let e1 = unsafe { try_release(p1, l1, a1) }.expect_err("firing 1 of 2 must fail");
    assert_eq!(e1.os_code(), None, "no syscall ran: {e1:?}");

    // Firing 2 of 2 (fresh reservation — never reused after the Err).
    let r2 = reserve_aligned(4 * PAGE, PAGE).expect("reserve 2");
    let (p2, l2, a2) = r2.into_parts();
    // SAFETY: fresh live reservation; same armed-hook reasoning as above.
    let e2 = unsafe { try_release(p2, l2, a2) }.expect_err("firing 2 of 2 must fail");
    assert_eq!(e2.os_code(), None, "no syscall ran: {e2:?}");

    // The hook disarmed itself: the third call succeeds on a fresh
    // reservation.
    let r3 = reserve_aligned(4 * PAGE, PAGE).expect("reserve 3");
    let (p3, l3, a3) = r3.into_parts();
    // SAFETY: fresh live reservation; hooks disarmed, real backend release.
    let ok = unsafe { try_release(p3, l3, a3) };
    assert!(ok.is_ok(), "after n fires the hook is disarmed: {ok:?}");
}

/// The SAFE consuming method also routes through the hook: `Err` with the
/// no-code sentinel, and the backend never attempted.
#[test]
fn reservation_try_release_reports_injected_err_and_consumes_ownership() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_next_release(0); // disarm residue from a prior test in this binary

    let r: Reservation = reserve_aligned(4 * PAGE, PAGE).expect("reserve");

    #[cfg(all(unix, not(miri), feature = "bench-internals"))]
    let before = aligned_vmem::unix_munmap_attempts();

    arm_fail_next_release(1);

    // Consumes `r` by value regardless of outcome.
    let err = r
        .try_release()
        .expect_err("armed hook must fail the method");
    assert_eq!(
        err.os_code(),
        None,
        "an injected failure ran no syscall: {err:?}"
    );
    assert!(
        !err.is_invalid_argument(),
        "an injected OS refusal is not a contract violation: {err:?}"
    );

    #[cfg(all(unix, not(miri), feature = "bench-internals"))]
    assert_eq!(
        aligned_vmem::unix_munmap_attempts() - before,
        0,
        "delta 0 proves no backend attempt happened at all: the API did not \
         retry internally after the injected Err, and ownership was consumed \
         — it returned no live handle or pointer"
    );
}
