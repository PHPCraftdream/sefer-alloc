//! `try_decommit` — the fallible twin `decommit` never had.
//!
//! Of this crate's state-changing primitives, `decommit`/`decommit_lazy` were
//! the only pair with no `try_*` form, and also the only ones that silently do
//! nothing when the range contract is violated: silent AND unreportable at the
//! same time. These tests pin the reporting half.
//!
//! The empty-range case is tested explicitly because it is the one place where
//! "does nothing" is CORRECT rather than a mistake — `decommit`'s single
//! `start >= end` early return conflates the two, and `try_decommit` must not.

use aligned_vmem::{page_size, reserve_aligned, try_decommit, DecommitOutcome, PAGE};

const SPAN: usize = 2 * 1024 * 1024;

#[test]
fn well_formed_range_succeeds() {
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();

    // SAFETY: `r` is live and `[ps, 3*ps)` is inside its usable span.
    let out = unsafe { try_decommit(r.as_ptr(), ps, 3 * ps) };
    assert!(
        out.is_ok(),
        "a page-aligned, non-empty, in-span range must succeed"
    );
    // task #1180: a fresh, in-span, page-aligned, non-empty range on an
    // ordinary (non-huge) reservation always reaches the real backend and is
    // accepted on every platform this crate supports (Linux `madvise`/
    // Windows `VirtualFree` both accept a decommit of freshly-committed
    // pages) — never `Skipped` (that requires a huge reservation) and never
    // `Refused` (there is nothing here for the OS to reject).
    assert_eq!(
        out.unwrap(),
        DecommitOutcome::Advised,
        "a well-formed range on an ordinary reservation must be genuinely \
         advised to the OS, not skipped or refused"
    );
}

/// An empty page-aligned range is a deliberate no-op, NOT a contract violation.
#[test]
fn empty_range_is_a_well_formed_no_op() {
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();

    // SAFETY: `r` is live; an empty range touches nothing.
    let a = unsafe { try_decommit(r.as_ptr(), 0, 0) };
    assert!(a.is_ok(), "empty range at offset 0");
    assert_eq!(
        a.unwrap(),
        DecommitOutcome::Skipped,
        "an empty range issues no backend call — Skipped, not Advised"
    );
    // SAFETY: as above.
    let b = unsafe { try_decommit(r.as_ptr(), 2 * ps, 2 * ps) };
    assert!(b.is_ok(), "empty range at a non-zero page-aligned offset");
    assert_eq!(b.unwrap(), DecommitOutcome::Skipped);
}

/// The empty range's MISALIGNED twin must be `Err`, not the no-op: only a
/// page-ALIGNED empty range is well-formed. Task #1094 closed the gap where
/// NOTHING pinned this — widening the free function's
/// `decommit_range_is_well_formed` to bless any empty range (the rejected
/// alternative (b) from task #1084/M2) flips exactly these calls from `Err`
/// to `Ok` while every pre-existing test in this file stays green.
#[test]
fn empty_misaligned_range_is_reported() {
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();

    // SAFETY (both): `r` is live. Each call is rejected on argument shape
    // before any OS call, so no out-of-span access can occur.
    unsafe {
        assert!(
            try_decommit(r.as_ptr(), 1, 1).is_err(),
            "empty AND misaligned (offset 0 + 1) is a contract violation, not a no-op"
        );
        assert!(
            try_decommit(r.as_ptr(), ps + 1, ps + 1).is_err(),
            "empty AND misaligned at a non-zero offset: same violation shape"
        );
    }
}

/// The three contract violations, each reported rather than swallowed.
#[test]
fn contract_violations_are_reported() {
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();

    // SAFETY (all three): `r` is live. Each call is rejected on argument shape
    // before any OS call, so no out-of-span access can occur.
    unsafe {
        assert!(
            try_decommit(r.as_ptr(), 4 * ps, 2 * ps).is_err(),
            "start > end must be reported"
        );
        assert!(
            try_decommit(r.as_ptr(), 1, 2 * ps).is_err(),
            "misaligned start must be reported"
        );
        assert!(
            try_decommit(r.as_ptr(), 0, 2 * ps + 1).is_err(),
            "misaligned end must be reported"
        );
    }
}

/// The infallible `decommit` still swallows the same violations — that is its
/// documented signature, not a bug — so this pins the DIFFERENCE between the
/// two entry points rather than asserting one is broken.
///
/// `debug_assert!` fires on those inputs in a debug build, so this test only
/// exercises `decommit` with WELL-FORMED arguments; the violating half is
/// covered above through `try_decommit`, which reports instead of aborting.
#[test]
fn decommit_and_try_decommit_agree_on_well_formed_input() {
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();

    // SAFETY: both calls target the same in-span, page-aligned range on a live
    // reservation; `decommit` is idempotent with respect to a repeat.
    unsafe {
        aligned_vmem::decommit(r.as_ptr(), ps, 2 * ps);
        assert!(try_decommit(r.as_ptr(), ps, 2 * ps).is_ok());
    }
}

/// `PAGE` is a compile-time floor, not the runtime page size. On a host where
/// they differ (16 KiB Apple Silicon), a `PAGE`-multiple offset is NOT
/// necessarily a `page_size()` multiple — and `try_decommit` must judge by the
/// runtime value, which is what the OS enforces.
#[test]
fn validation_uses_the_runtime_page_size_not_the_compile_time_floor() {
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();

    if ps == PAGE {
        // Host floor == runtime size: a `PAGE` offset is valid, and the test
        // still asserts that rather than skipping.
        // SAFETY: live reservation, in-span page-aligned range.
        // pageguard:allow — guarded by `if ps == PAGE` above: on this arm PAGE IS the runtime page size.
        assert!(unsafe { try_decommit(r.as_ptr(), PAGE, 2 * PAGE) }.is_ok());
        return;
    }

    // Runtime page is larger than `PAGE`: a single-`PAGE` offset is misaligned
    // for the OS and must be rejected.
    // SAFETY: rejected on argument shape before any OS call.
    assert!(
        unsafe { try_decommit(r.as_ptr(), PAGE, ps) }.is_err(),
        "a PAGE-multiple that is not a page_size() multiple must be rejected"
    );
}

/// The `debug_assert!` in the infallible `decommit` must actually fire, or it
/// is decoration. Gated on `debug_assertions` because it is compiled out in a
/// release-profile test run, where this test would otherwise fail by NOT
/// panicking.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "violates the range contract")]
fn decommit_debug_asserts_on_a_contract_violation() {
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();
    // SAFETY: the call panics on the argument check before touching memory.
    unsafe {
        aligned_vmem::decommit(r.as_ptr(), 4 * ps, 2 * ps);
    }
}
