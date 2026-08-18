//! The SAFE-METHOD layer of the decommit contract, per build profile
//! (task #1079): `Reservation::decommit`, `Reservation::decommit_lazy`,
//! and the new fallible `Reservation::try_decommit`.
//!
//! Why this file exists when `try_decommit.rs` / `mock.rs` / `smoke.rs`
//! already pin contract-violation behavior: every one of those oracles
//! observes the FREE functions. The safe methods add their own forward step
//! between caller and tripwire, and that step is exactly where the (b)
//! variant of task #1079's fix (pre-filter inside the method) would have
//! disarmed the debug tripwire for every safe-API caller with no existing
//! test noticing. Task #1079 chose variant (a) — forward unfiltered and
//! document the profile split — and these tests make that choice ENFORCED
//! rather than incidental.
//!
//! Profile mechanics: `cargo test` builds tests in debug, so the
//! `#[cfg(debug_assertions)]` tests below are what CI's debug rows run; the
//! `#[cfg(not(debug_assertions))]` tests compile into `--release` runs.
//! CI has NO `--release` row for this package (item 72 in
//! `docs/CORRECTNESS_OPEN_ITEMS.md`), so the release half executes only on
//! local `cargo test --release` runs until the owner adds one.

use aligned_vmem::{page_size, reserve_aligned};

const SPAN: usize = 2 * 1024 * 1024;

/// Serializes the tests in THIS file that produce or assert mock call-log
/// entries (the mock log is process-global; sibling test binaries are
/// separate processes, but sibling tests in this binary share it). Mirrors
/// the file-local SERIAL pattern of `decommit_capability.rs`.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The debug tripwire must fire THROUGH the safe method: `Reservation::decommit`
/// forwards a violated range to the free `decommit`'s `debug_assert!`
/// instead of pre-filtering it (task #1079 decision (a)).
///
/// Counterfactual (verified at task #1079 by temporarily applying the
/// rejected fix (b) — a well-formedness pre-filter at the top of the method
/// body): this test fails with "test did not panic as expected". Deleting
/// the free function's `debug_assert!` (reverting task #1051) fails it the
/// same way.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "violates the range contract")]
fn method_trips_the_tripwire_on_a_violated_range_in_debug() {
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();
    // A SAFE call: the panic comes from the FORWARDED free function's
    // debug_assert!, proving no pre-filter sits between the method and it.
    r.decommit(4 * ps, 2 * ps);
}

/// The method's OWN two early returns — out-of-bounds `end` and the empty
/// range — happen BEFORE the forward, so they must not panic even in
/// debug: the tripwire only ever sees a range that passed the bounds
/// check. Pins the distinction the `# Panics` section draws between
/// "violated range" (panics in debug) and "empty / out of bounds"
/// (never panics).
#[test]
#[cfg(debug_assertions)]
fn method_bounds_check_precedes_the_tripwire_in_debug() {
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();
    r.decommit(0, SPAN + ps); // end > self.len(): method-level silent skip
    r.decommit(0, 0); // empty range: well-formed no-op
    r.decommit_lazy(0, SPAN + ps); // lazy twin, same bounds-first shape
}

/// `Reservation::decommit_lazy` never panics on a violated range on ANY
/// profile — the deliberate eager/lazy asymmetry settled by task #1072,
/// pinned here at the METHOD layer. Ungated: it must hold in debug (CI
/// rows) and release alike. Counterfactual: "unifying" the lazy path with
/// the eager tripwire (adding a `debug_assert!` to the free
/// `decommit_lazy`) fails this test in every debug run.
#[test]
fn lazy_method_silently_skips_a_violated_range_on_every_profile() {
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();
    r.decommit_lazy(4 * ps, 2 * ps); // start > end
    r.decommit_lazy(1, 2 * ps); // misaligned start
    r.decommit_lazy(0, 2 * ps + 1); // misaligned end
}

/// The RELEASE half of `Reservation::decommit`'s documented contract: a
/// violated range is a silent no-op — the method returns normally and the
/// reservation stays live and releasable. Runs under `cargo test --release`
/// only (CI has no such row for this package yet — see the module doc).
///
/// Counterfactual (verified at task #1079 by making the free function's
/// violation filter panic in a scratch edit): this test fails. A fix that
/// made the doc true by deleting the diagnostic would ALSO have to change
/// this test's outcome — it does not, because release behavior is unchanged
/// by the doc-only fix.
#[test]
#[cfg(not(debug_assertions))]
fn method_silently_skips_a_violated_range_in_release() {
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();
    r.decommit(4 * ps, 2 * ps); // start > end
    r.decommit(1, 2 * ps); // misaligned start
    r.decommit(0, 2 * ps + 1); // misaligned end
    r.decommit(0, SPAN + ps); // out of bounds
    drop(r); // still a live reservation, releasable exactly once
}

/// Mechanism oracle for the release half (mock layer): a violated
/// `Reservation::decommit` call must record NO `Call::Decommit` /
/// `Call::DecommitLazy` at all, and a well-formed call through the SAME
/// method must record one — the positive control that proves the arm
/// really exercised the method (a "no records" assert with no positive
/// control could pass vacuously if the recorder itself were broken).
/// Mock+release only: in mock+debug the tripwire panics first, and without
/// the mock cfg there is no recorder.
#[test]
#[cfg(all(aligned_vmem_mock, not(debug_assertions)))]
fn method_records_nothing_for_a_violated_range_in_release_mock() {
    use aligned_vmem::mock::{self, Call};

    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    mock::reset();
    let r = reserve_aligned(SPAN, SPAN).expect("mock reserve chains to real backend");
    let ps = page_size();
    let _ = mock::drain(); // discard this test's own Reserve record

    r.decommit(1, 2 * ps); // misaligned start
    r.decommit(4 * ps, 2 * ps); // start > end
    r.decommit(0, 2 * ps + 1); // misaligned end
    r.decommit_lazy(1, 2 * ps); // lazy twin: silent on every profile

    let calls = mock::drain();
    assert!(
        !calls
            .iter()
            .any(|c| matches!(c, Call::Decommit { .. } | Call::DecommitLazy { .. })),
        "a contract-violating method call must record NO Call::Decommit / \
         Call::DecommitLazy at all (silent skip, not recorded-then-rejected): \
         {calls:?}"
    );

    // Positive control: the same method DOES reach the recorder when the
    // range is well-formed.
    r.decommit(0, ps);
    let calls = mock::drain();
    assert!(
        calls
            .iter()
            .any(|c| matches!(c, Call::Decommit { start: 0, .. })),
        "a well-formed method call must record a Call::Decommit with \
         start == 0: {calls:?}"
    );
}

/// `Reservation::try_decommit` (added task #1079): every violation shape is
/// reported as `Err` — including the method-level bounds check — and
/// neither the violations nor the successes ever trip the debug tripwire
/// (that is the whole point of the fallible form). Ungated: must hold on
/// every profile; the DEBUG run is the one that proves "never panics",
/// because that is the profile where the eager path DOES panic on the same
/// inputs (`method_trips_the_tripwire_on_a_violated_range_in_debug`
/// above).
#[test]
fn method_try_decommit_reports_violations_and_never_panics() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();
    assert!(
        r.try_decommit(4 * ps, 2 * ps).is_err(),
        "start > end must be reported"
    );
    assert!(
        r.try_decommit(1, 2 * ps).is_err(),
        "misaligned start must be reported"
    );
    assert!(
        r.try_decommit(0, 2 * ps + 1).is_err(),
        "misaligned end must be reported"
    );
    assert!(
        r.try_decommit(0, SPAN + ps).is_err(),
        "end > self.len() must be reported at the method's own bounds check"
    );
    assert!(
        r.try_decommit(0, 0).is_ok(),
        "empty range is a well-formed no-op, not a violation"
    );
    assert!(
        r.try_decommit(ps, 2 * ps).is_ok(),
        "a page-aligned in-span range must succeed"
    );
}

/// `Reservation::try_decommit`'s huge-page early-exit mirrors
/// `Self::decommit`'s: skip the backend call, count the attempt, return
/// `Ok(())` (the free `try_decommit` deliberately does not report OS
/// refusal as an error either). Mirrors
/// `huge_decommit_attempts_increments_on_huge_reservation` in
/// `decommit_capability.rs`; on CI runners without a hugetlb pool /
/// `SeLockMemoryPrivilege` the fallback arm runs (see that test's own
/// NOTE).
#[test]
#[cfg(all(feature = "bench-internals", feature = "huge-pages"))]
fn method_try_decommit_huge_skip_returns_ok_and_counts() {
    use aligned_vmem::{
        huge_decommit_attempts, reserve_aligned_huge, reset_bench_internals_counters,
    };

    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    reset_bench_internals_counters();

    let size = 2 * 1024 * 1024; // Linux huge-page size
    let r = reserve_aligned_huge(size, size).expect("huge reservation (or fallback)");

    if r.is_huge() {
        assert!(
            r.try_decommit(0, size).is_ok(),
            "huge skip must report Ok — OS refusal is deliberately not an error"
        );
        assert_eq!(
            huge_decommit_attempts(),
            1,
            "the skip must increment the same counter as Self::decommit's skip"
        );
    } else {
        assert!(
            r.try_decommit(0, size).is_ok(),
            "ordinary fallback: a well-formed range must succeed"
        );
        assert_eq!(
            huge_decommit_attempts(),
            0,
            "no huge attempt may be counted for an ordinary reservation"
        );
    }
}
