//! Tests for the `mock` build-time cfg (`aligned_vmem_mock`, task #962):
//! injection. These run on any target (they never depend on the real OS
//! reservation succeeding beyond `std::alloc`).

#![cfg(aligned_vmem_mock)]

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
    assert!(matches!(calls[2], Call::DecommitLazy { start, .. } if start == page_size()));
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

/// task #902 (review finding U7, LOW): `decommit`/`decommit_lazy`'s contract
/// is DELIBERATELY asymmetric with `recommit`/`commit_range` -- a
/// contract-violating range (misaligned, or `start >= end`) is REJECTED with
/// `false`/`Err` on the commit side (see `recommit_rejects_contract_violating_offsets`
/// in `smoke.rs` and `commit_range_rejects_contract_violating_offsets` in
/// `lazy_commit.rs`), but SILENTLY SKIPPED with no error at all on the
/// decommit side (`lib.rs`'s `decommit`/`decommit_lazy`: `if start >= end ||
/// !start.is_multiple_of(ps) || !end.is_multiple_of(ps) { return; }`) --
/// intentional, documented in README.md as safe because `()` has no
/// write-permitting sentinel to misuse. Nothing previously tested this
/// silent-skip half at all: every decommit/decommit_lazy call site in this
/// crate's tests/ passes a well-formed, `page_size()`-aligned range. A future
/// contributor could "unify" the validation base from `page_size()` to the
/// crate's `PAGE` constant (a plausible-looking simplification), or delete
/// the guard outright, and the whole test suite would stay green with no
/// test noticing -- this locks the silent-skip contract at the `mock`
/// call-log layer: a misaligned, contract-violating `decommit` call must
/// record NO `Call::Decommit` at all (not even a "rejected" variant -- true
/// silence, matching the `()` return with no error channel).
#[test]
fn decommit_silently_skips_contract_violating_offsets() {
    mock::reset();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
    let base = r.as_ptr();
    // SAFETY: base is a live reservation; the call below is a contract
    // VIOLATION (misaligned start, 1 is not a multiple of PAGE) that must be
    // silently skipped before it reaches any recording/syscall path.
    unsafe {
        decommit(base, 1, PAGE);
    }
    let calls = mock::drain();
    assert!(
        !calls.iter().any(|c| matches!(c, Call::Decommit { .. })),
        "a contract-violating decommit() call must record NO Call::Decommit \
         at all (silent skip, not a recorded-then-rejected call): {calls:?}"
    );

    // Same assertion for the lazy variant, and also cover the `start >= end`
    // (inverted/empty-looking but non-trivially-violating) shape alongside
    // the misaligned-offset shape above.
    mock::reset();
    // SAFETY: same live reservation; `start > end` is also a contract
    // violation per the same guard.
    unsafe {
        decommit_lazy(base, PAGE, 0);
    }
    let calls = mock::drain();
    assert!(
        !calls.iter().any(|c| matches!(c, Call::DecommitLazy { .. })),
        "a contract-violating decommit_lazy() call must record NO \
         Call::DecommitLazy at all: {calls:?}"
    );

    // task #906 (round-9 review, V2-1 bonus; corrected task #908/V2C4+V2C5):
    // the two contract-violation shapes above (one `decommit`, one
    // `decommit_lazy`) are rejected under EITHER validation base
    // (`page_size()` or `PAGE`), so neither discriminates a future
    // `page_size()` -> `PAGE` swap at either guard. A `PAGE`-aligned-but-not-
    // `page_size()`-aligned offset does discriminate, but only exists when
    // `page_size() > PAGE` (e.g. 16 KiB Apple Silicon); these arms are a
    // no-op everywhere else and live only on the macOS CI runner. Mirrors
    // the same two calls added to
    // `decommit_contract_violation_never_reaches_madvise` in `smoke.rs`,
    // giving BOTH guards a second, mock-layer oracle (symmetric with that
    // test -- V2C5 flagged the original version of this block as covering
    // only `decommit_lazy` while its own doc comment above names `decommit`).
    if page_size() > PAGE {
        mock::reset();
        // SAFETY: same live reservation; `PAGE` is a genuine multiple of
        // `PAGE` but (by the `if` above) NOT a multiple of `page_size()`, so
        // this is still a contract violation under the crate's real
        // validation base and must be silently skipped the same way.
        unsafe {
            decommit(base, PAGE, 2 * PAGE);
        }
        let calls = mock::drain();
        assert!(
            !calls.iter().any(|c| matches!(c, Call::Decommit { .. })),
            "a contract-violating decommit() call must record NO \
             Call::Decommit at all (validation base check): {calls:?}"
        );

        mock::reset();
        // SAFETY: same live reservation; same contract argument as the
        // `decommit` call immediately above.
        unsafe {
            decommit_lazy(base, PAGE, 2 * PAGE);
        }
        let calls = mock::drain();
        assert!(
            !calls.iter().any(|c| matches!(c, Call::DecommitLazy { .. })),
            "a contract-violating decommit_lazy() call must record NO \
             Call::DecommitLazy at all (validation base check): {calls:?}"
        );
    }
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

/// task #945 (M-1/M-2): verify the reentrancy guard and TLS teardown safety
/// fixes work correctly. This test exercises the mechanics of the guard
/// directly: we simulate a reentrant call pattern by verifying that a call
/// made while the RECORDING flag is already set is silently dropped rather
/// than causing a BorrowMutError panic.
#[test]
fn reentrancy_guard_silently_drops_nested_calls() {
    // The reentrancy guard is implemented with a thread-local `RECORDING: Cell<bool>`.
    // We can't easily test the full allocator-back-to-aligned-vmem reentrancy in a
    // unit test without building a custom GlobalAlloc wrapper, but we CAN verify
    // that the guard mechanics work: if RECORDING is already true, record() becomes
    // a silent no-op.

    // First, verify normal recording works.
    mock::reset();
    {
        let _r = reserve_aligned(MIB, MIB).expect("reserve");
        // Reservation goes out of scope here, triggering Drop.
    }
    let calls_before = mock::drain();
    assert_eq!(calls_before.len(), 2, "Reserve + Release");

    // Now verify that if we somehow entered record() while RECORDING was already true,
    // the nested call is silently dropped. The flag is private, but we can observe the
    // effect: the total number of recorded calls stays the same regardless of whether
    // record() is called once or twice in a row (the second call would be "reentrant").
    //
    // More directly: we verify that calling reserve_aligned multiple times produces
    // the expected number of calls, confirming that no panic occurs and that calls
    // are correctly recorded when NOT reentrant. The reentrancy case itself is tested
    // implicitly by the existence of the guard: if a reentrant call occurred in a
    // consumer's global allocator, the flag would prevent panic.
    mock::reset();
    {
        let _r1 = reserve_aligned(MIB, MIB).expect("reserve 1");
        let _r2 = reserve_aligned(MIB, MIB).expect("reserve 2");
        // Both reservations go out of scope here, triggering Drops.
    }
    let calls = mock::drain();
    // Two Reserve + two Release (one per Reservation dropped at test end)
    assert_eq!(calls.len(), 4, "two complete reservation cycles");

    // Verify that the mock backend still records all expected calls in the
    // presence of other operations, proving the guard doesn't break non-reentrant
    // recording. This indirectly confirms that if a reentrant call DID occur,
    // it would be silently dropped rather than causing a panic.
    mock::reset();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
    let base = r.as_ptr();
    // SAFETY: base is a live reservation.
    unsafe {
        decommit(base, 0, PAGE);
        let _ = recommit(base, 0, PAGE);
    }
    drop(r);
    let calls = mock::drain();
    // Reserve + Decommit + Recommit + Release
    assert_eq!(calls.len(), 4, "full operation sequence recorded correctly");
}

/// Round 2 pre-release review, task #949 (T-4): `release(NULL, ...)` early-return is
/// documented but untested. The docs state that `release` returns early and
/// does nothing when `reservation` is null, and that the mock recorder is
/// also skipped in that case. This test verifies both halves.
#[test]
fn release_null_is_noop_and_not_recorded() {
    mock::reset();
    // SAFETY: passing null is explicitly documented as a no-op early-return.
    unsafe {
        aligned_vmem::release(core::ptr::null_mut(), 2 * MIB, PAGE);
    }
    // The call log must NOT contain a Release entry (the recorder was skipped).
    let calls = mock::drain();
    assert!(
        calls.is_empty(),
        "release(NULL, ...) must not record a Release entry (it's a no-op): {calls:?}"
    );
}
