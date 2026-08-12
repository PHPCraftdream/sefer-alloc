//! Tests for the `mock` feature: recording call log + fail-N-th fault
//! injection. These run on any target (they never depend on the real OS
//! reservation succeeding beyond `std::alloc`).

#![cfg(feature = "mock")]

use aligned_vmem::mock::{self, Call};
use aligned_vmem::{
    decommit, decommit_lazy, page_size, recommit, reserve_aligned, try_reserve_aligned, PAGE,
};

const MIB: usize = 1024 * 1024;

#[test]
fn records_reserve_and_decommit() {
    mock::reset();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("mock reserve chains to real backend");
    let base = r.as_ptr();
    // SAFETY: base is a live reservation; decommit records only under mock.
    unsafe {
        decommit(base, 0, page_size());
        decommit_lazy(base, page_size(), 2 * page_size());
    }
    let calls = mock::drain();
    assert_eq!(calls.len(), 3, "reserve + decommit + decommit_lazy");
    assert!(matches!(
        calls[0],
        Call::Reserve {
            size,
            align,
            ..
        } if size == 2 * MIB && align == 2 * MIB
    ));
    assert!(matches!(calls[1], Call::Decommit { start: 0, .. }));
    assert!(matches!(calls[2], Call::DecommitLazy { start, .. } if start == PAGE));
    // V9: Drop records Release, so after this point r will drop and we'll see Release.
    // For this test, explicitly drop r to see the Release before checking.
    drop(r);
    let calls_after_drop = mock::drain();
    assert_eq!(calls_after_drop.len(), 1, "release via Drop");
    assert!(matches!(calls_after_drop[0], Call::Release { .. }));
    // Drain clears the log.
    assert!(mock::drain().is_empty());
}

/// V9 fix: Drop records Release calls so RAII path is visible.
#[test]
fn drop_records_release() {
    mock::reset();
    {
        let _r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
        // Reservation goes out of scope here, triggering Drop.
    }
    let calls = mock::drain();
    println!("drop_records_release: calls = {:?}", calls);
    assert_eq!(calls.len(), 2, "Reserve + Release (via Drop)");
    assert!(
        matches!(calls[0], Call::Reserve { size, align, .. } if size == 2 * MIB && align == 2 * MIB)
    );
    // Don't check the exact reservation_len - it can be larger due to over-reserve.
    assert!(matches!(calls[1], Call::Release { .. }));
}

#[test]
fn fail_next_reserve_injects_oom() {
    mock::reset();
    mock::fail_next_reserve(2);
    assert!(
        reserve_aligned(MIB, MIB).is_none(),
        "1st reserve fails (armed)"
    );
    assert!(
        reserve_aligned(MIB, MIB).is_none(),
        "2nd reserve fails (armed)"
    );
    assert!(
        reserve_aligned(MIB, MIB).is_some(),
        "3rd reserve succeeds (disarmed)"
    );
    // V9: Drop records Release, so we'll see 3 Reserve + 1 Release.
    // The successful reserve is dropped at test end.
    let n = mock::drain().len();
    assert_eq!(n, 4);
}

#[test]
fn fail_next_commit_injects_recommit_failure() {
    mock::reset();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
    let base = r.as_ptr();
    mock::fail_next_commit(1);
    // SAFETY: base is a live reservation.
    unsafe {
        assert!(
            !recommit(base, 0, PAGE),
            "1st recommit fails (commit fault armed)"
        );
        assert!(
            recommit(base, 0, PAGE),
            "2nd recommit succeeds (fault consumed)"
        );
    }
    // V9: Explicitly drop to trigger Drop before draining.
    drop(r);
    let calls = mock::drain();
    // V9: Drop records Release, so we'll see 1 Reserve + 2 Recommits + 1 Release.
    assert_eq!(calls.len(), 4);
}

#[cfg(feature = "lazy-commit")]
#[test]
fn fail_next_commit_injects_commit_range_failure() {
    use aligned_vmem::{commit_range, reserve_aligned_lazy};
    mock::reset();
    let r = reserve_aligned_lazy(4 * MIB, 4 * MIB, PAGE).expect("lazy reserve");
    let base = r.as_ptr();
    mock::fail_next_commit(1);
    // SAFETY: base is a live reservation.
    unsafe {
        assert!(!commit_range(base, PAGE, 2 * PAGE), "commit fault armed");
        assert!(commit_range(base, PAGE, 2 * PAGE), "fault consumed");
    }
    // V9: Explicitly drop to trigger Drop before draining.
    drop(r);
    let calls = mock::drain();
    assert!(matches!(calls[0], Call::ReserveLazy { .. }));
    // V9: Drop records Release, so we'll see 1 ReserveLazy + 2 CommitRange + 1 Release.
    assert_eq!(calls.len(), 4);
}

/// V6 fix: constructors allow external crates to build expected call vectors.
#[test]
fn call_constructors_work_from_external_tests() {
    use aligned_vmem::mock::Call;

    // This would not compile with `Call::Reserve { size, align }` syntax
    // because of variant-level `#[non_exhaustive]`. Constructors restore
    // the natural `assert_eq!` pattern.
    let expected = [
        Call::reserve(2 * MIB, 2 * MIB),
        Call::decommit(0x1000, 0, PAGE),
        Call::release(0x1000, 2 * MIB),
    ];

    // Verify the constructors produce the same variants as the crate's internal
    // construction. Use `matches!` because direct equality with `..` is not
    // possible for non-exhaustive variants.
    assert!(
        matches!(expected[0], Call::Reserve { size, align, .. } if size == 2 * MIB && align == 2 * MIB)
    );
    assert!(
        matches!(expected[1], Call::Decommit { base, start, end, .. } if base == 0x1000 && start == 0 && end == PAGE)
    );
    assert!(
        matches!(expected[2], Call::Release { reservation, reservation_len, .. } if reservation == 0x1000 && reservation_len == 2 * MIB)
    );
}

#[cfg(feature = "huge-pages")]
#[test]
fn fail_next_reserve_injects_through_huge_path() {
    // task #716 fix (1) item (c): `reserve_aligned_huge` shares
    // `try_reserve_aligned_exact`'s reserve-fault-injection point with the
    // ordinary path, but had no direct regression test proving `Call::ReserveHuge`
    // is actually the variant recorded and that `fail_next_reserve` actually
    // fires on this specific entry point (as opposed to merely on
    // `reserve_aligned`, which every other test here exercises).
    use aligned_vmem::reserve_aligned_huge;
    mock::reset();
    mock::fail_next_reserve(1);
    assert!(
        reserve_aligned_huge(2 * MIB, 2 * MIB).is_none(),
        "1st huge reserve fails (armed)"
    );
    assert!(
        reserve_aligned_huge(2 * MIB, 2 * MIB).is_some(),
        "2nd huge reserve succeeds (fault consumed)"
    );
    let calls = mock::drain();
    // V9: Drop records Release, so we'll see 2 ReserveHuge + 1 Release.
    assert_eq!(
        calls.len(),
        3,
        "both attempts are recorded regardless of outcome, plus Release from Drop"
    );
    assert!(
        matches!(calls[0], Call::ReserveHuge { size, align, .. } if size == 2 * MIB && align == 2 * MIB),
        "must record Call::ReserveHuge, not Call::Reserve: {:?}",
        calls[0]
    );
    assert!(matches!(calls[1], Call::ReserveHuge { .. }));
}

/// task #776 (F2): a simulated mock fault used to report
/// `VmemError::last_os_error()` -- reading whatever `errno`/`GetLastError`
/// happened to be lying around from unrelated prior code, since no real
/// syscall ran. That is exactly the "code 0 ambiguity" task #713 already
/// fixed for the real-path fault-injection branch (`os_refusal_unknown_code()`
/// instead of `last_os_error()`); `mock`'s own fault takers had the same
/// defect. Proves the fix: a simulated fault's `VmemError` reports
/// `os_code() == None`, distinct from a genuine OS error code (which would be
/// `Some(_)`).
#[test]
fn simulated_fault_reports_no_os_code() {
    mock::reset();
    mock::fail_next_reserve(1);
    let err = match try_reserve_aligned(MIB, MIB) {
        Err(e) => e,
        Ok(_) => panic!("armed fault must fail"),
    };
    assert_eq!(
        err.os_code(),
        None,
        "a SIMULATED failure must not report a fabricated OS code: {err:?}"
    );
    assert!(
        !err.is_invalid_argument(),
        "a simulated OS refusal is distinct from a contract violation: {err:?}"
    );

    mock::reset();
    let r = reserve_aligned(MIB, MIB).expect("reserve");
    let base = r.as_ptr();
    mock::fail_next_commit(1);
    // SAFETY: base is a live reservation.
    let err = unsafe { aligned_vmem::try_recommit(base, 0, PAGE) }
        .expect_err("armed commit fault must fail");
    assert_eq!(
        err.os_code(),
        None,
        "a SIMULATED commit failure must not report a fabricated OS code: {err:?}"
    );
}

#[test]
fn reset_clears_faults_and_log() {
    mock::reset();
    mock::fail_next_reserve(5);
    let _ = reserve_aligned(MIB, MIB);
    mock::reset();
    // After reset the fault counter is cleared: this reserve must succeed.
    assert!(reserve_aligned(MIB, MIB).is_some());
    // V9: Drop records Release, so we'll see 1 Reserve + 1 Release.
    assert_eq!(mock::drain().len(), 2);
}
