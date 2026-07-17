//! Tests for the `fault-injection` feature: real-path commit fault injection,
//! DISTINCT from `mock`. These tests run against the REAL OS backend (no
//! `mock` feature): `reserve_aligned_lazy` performs a genuine reservation and
//! `commit_range` issues genuine `VirtualAlloc`/no-op-Unix commit syscalls —
//! the armed hooks only intercept the specific call(s) under test, proving
//! the fault-injection hook coexists with (and does not replace) the real
//! backend.

// `not(feature = "mock")`: under `mock`, `try_commit_range` is entirely
// replaced by the recording stub (see `crate::mock`'s doc comment) and never
// reaches the real-path hook this file tests — that combination is legal to
// compile (see `--all-features`) but produces a vacuous no-op test, which
// would be worse than not running it. These tests specifically prove the
// hook fires on the REAL backend, so they require `mock` OFF.
#![cfg(all(
    feature = "fault-injection",
    feature = "lazy-commit",
    not(feature = "mock")
))]

use aligned_vmem::fault_injection::{arm_fail_at, arm_fail_next};
use aligned_vmem::{commit_range, reserve_aligned_lazy, PAGE};
use std::sync::Mutex;

const MIB: usize = 1024 * 1024;

/// The `fault-injection` hooks are PROCESS-GLOBAL atomics; libtest runs the
/// tests in this file on parallel threads, so their arm/fire/disarm sequences
/// would otherwise interleave against the shared state (one test's disarm or
/// commit consuming another's just-armed one-shot). Every test takes this lock
/// for its whole body so the process-global hook is exercised single-threaded.
/// `unwrap_or_else(into_inner)` recovers from a poisoned lock so one failing
/// test does not cascade into spurious failures of the rest.
static SERIAL: Mutex<()> = Mutex::new(());

/// `arm_fail_next(1)` forces exactly the NEXT real `commit_range` call to
/// fail without touching the OS; the call after that succeeds normally
/// against the real backend. Non-vacuous: the reservation is real (backed by
/// the OS), and the post-fault commit genuinely makes the range writable.
#[test]
fn fail_next_forces_exactly_one_real_commit_failure() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_next(0); // disarm any residue from a prior test in this binary
    let chunk = 16 * PAGE; // 64 KiB
    let span = 4 * MIB;
    let r = reserve_aligned_lazy(span, span, chunk).expect("real lazy reserve");
    let base = r.as_ptr();

    arm_fail_next(1);

    // SAFETY: base is a live reservation; [chunk, 2*chunk) is within span.
    let first = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(!first, "armed fault must force the real commit to fail");

    // SAFETY: same range; the fault was one-shot (consumed above).
    let second = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(
        second,
        "the following commit must hit the real backend and succeed"
    );

    // Prove the range is genuinely committed now (real write, not a mock).
    // SAFETY: [chunk, 2*chunk) is committed after `second` succeeded.
    unsafe {
        base.add(chunk).write(0x5A);
        assert_eq!(base.add(chunk).read(), 0x5A);
    }
}

/// `arm_fail_at(k)` lets the first `k - 1` real commits succeed and fails
/// exactly the k-th; it is one-shot and disarms itself after firing.
/// Non-vacuous: verifies both the successes AND the one failure against the
/// real backend, and that a call after the k-th succeeds again.
#[test]
fn fail_at_fails_exactly_the_kth_real_commit() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_at(0); // disarm any residue
    let chunk = 16 * PAGE;
    let span = 4 * MIB;
    let r = reserve_aligned_lazy(span, span, chunk).expect("real lazy reserve");
    let base = r.as_ptr();

    // Fail the 2nd commit from now.
    arm_fail_at(2);

    // SAFETY: [chunk, 2*chunk) is within span.
    let c1 = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(c1, "1st commit (k=1) must succeed against the real backend");

    // SAFETY: [2*chunk, 3*chunk) is within span.
    let c2 = unsafe { commit_range(base, 2 * chunk, 3 * chunk) };
    assert!(!c2, "2nd commit (k=2) must be the forced failure");

    // One-shot: the hook disarmed itself, so the 3rd commit succeeds.
    // SAFETY: same range as the failed c2 — retrying is a valid real commit.
    let c3 = unsafe { commit_range(base, 2 * chunk, 3 * chunk) };
    assert!(
        c3,
        "3rd commit (retry after k-th) must succeed (hook disarmed)"
    );

    // Prove real committed memory is writable after the retry.
    // SAFETY: [2*chunk, 3*chunk) is committed after `c3` succeeded.
    unsafe {
        base.add(2 * chunk).write(0xA5);
        assert_eq!(base.add(2 * chunk).read(), 0xA5);
    }
}

/// `arm_fail_next` has priority over `arm_fail_at` when both are armed
/// simultaneously: the "next N" hook fires first.
#[test]
fn fail_next_has_priority_over_fail_at() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_next(0);
    arm_fail_at(0);
    let chunk = 16 * PAGE;
    let span = 4 * MIB;
    let r = reserve_aligned_lazy(span, span, chunk).expect("real lazy reserve");
    let base = r.as_ptr();

    // Arm BOTH: fail-next(1) should fire on the very next call, consuming
    // itself; fail-at(1) (also targeting "the next call") must NOT also fire
    // on that same call (it should still be armed for a LATER call).
    arm_fail_next(1);
    arm_fail_at(1);

    // SAFETY: [chunk, 2*chunk) within span.
    let c1 = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(!c1, "fail_next fires first on the 1st call");

    // fail_at(1) counts calls AFTER it was armed; this is its first observed
    // call (the fail_next branch returns before incrementing fail_at's
    // counter), so THIS call is fail_at's k=1 and must also fail.
    // SAFETY: same range, retried.
    let c2 = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(!c2, "fail_at's k=1 fires on the 2nd call");

    // Both hooks are now disarmed; the 3rd call hits the real backend.
    // SAFETY: same range.
    let c3 = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(c3, "both hooks consumed; 3rd call succeeds for real");
}

/// `arm_fail_next(0)` / `arm_fail_at(0)` are no-ops (disarm without firing):
/// a real commit proceeds normally.
#[test]
fn zero_arming_is_a_pure_disarm() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_next(0);
    arm_fail_at(0);
    let chunk = 16 * PAGE;
    let span = 2 * MIB;
    let r = reserve_aligned_lazy(span, span, chunk).expect("real lazy reserve");
    let base = r.as_ptr();

    // SAFETY: [chunk, 2*chunk) within span.
    let ok = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(ok, "disarmed hooks must not affect the real commit");
}
