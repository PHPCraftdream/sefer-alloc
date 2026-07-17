//! Tests for the `mock` feature: recording call log + fail-N-th fault
//! injection. These run on any target (they never depend on the real OS
//! reservation succeeding beyond `std::alloc`).

#![cfg(feature = "mock")]

use aligned_vmem::mock::{self, Call};
use aligned_vmem::{decommit, decommit_lazy, recommit, reserve_aligned, PAGE};

const MIB: usize = 1024 * 1024;

#[test]
fn records_reserve_and_decommit() {
    mock::reset();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("mock reserve chains to real backend");
    let base = r.as_ptr();
    // SAFETY: base is a live reservation; decommit records only under mock.
    unsafe {
        decommit(base, 0, PAGE);
        decommit_lazy(base, PAGE, 2 * PAGE);
    }
    let calls = mock::drain();
    assert_eq!(calls.len(), 3, "reserve + decommit + decommit_lazy");
    assert!(matches!(
        calls[0],
        Call::Reserve {
            size,
            align
        } if size == 2 * MIB && align == 2 * MIB
    ));
    assert!(matches!(calls[1], Call::Decommit { start: 0, .. }));
    assert!(matches!(calls[2], Call::DecommitLazy { start, .. } if start == PAGE));
    // Drain clears the log.
    assert!(mock::drain().is_empty());
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
    // Three Reserve calls were recorded regardless of outcome.
    let n = mock::drain().len();
    assert_eq!(n, 3);
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
    let calls = mock::drain();
    // reserve + 2 recommits.
    assert_eq!(calls.len(), 3);
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
    let calls = mock::drain();
    assert!(matches!(calls[0], Call::ReserveLazy { .. }));
    assert_eq!(calls.len(), 3);
}

#[test]
fn reset_clears_faults_and_log() {
    mock::reset();
    mock::fail_next_reserve(5);
    let _ = reserve_aligned(MIB, MIB);
    mock::reset();
    // After reset the fault counter is cleared: this reserve must succeed.
    assert!(reserve_aligned(MIB, MIB).is_some());
    // And only the post-reset call is in the log.
    assert_eq!(mock::drain().len(), 1);
}
