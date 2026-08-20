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
//! Since task #1086 (L10) both the aligned-vmem-gates job (ci.yml) and the
//! local gate (scripts/check-all.mjs) run a `--release` +
//! `--cfg aligned_vmem_mock` row scoped to THIS test target, so the release
//! half — including the mock release oracle below — executes in CI and
//! locally (a plain whole-package `--release` row remains open as item 72
//! in `docs/CORRECTNESS_OPEN_ITEMS.md`).

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
    let mut r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();
    // A SAFE call: the panic comes from the FORWARDED free function's
    // debug_assert!, proving no pre-filter sits between the method and it.
    r.decommit(4 * ps, 2 * ps);
}

/// The method's OWN early return — out-of-bounds `end > self.len()` —
/// happens BEFORE the forward, so it must not panic even in debug: the
/// tripwire only ever sees a range that passed the bounds check.
///
/// An EMPTY range is NOT pre-checked. Task #1084/M2 corrected this comment
/// (and the `# Panics` section it mirrored), which both previously claimed
/// the method had "two early returns" including an empty-range one that
/// never existed in the code: `decommit(0, 0)` below avoids the tripwire
/// only because an empty PAGE-ALIGNED range is WELL-FORMED at the
/// free-function layer, not because this method intercepted it. The
/// distinction is pinned by `method_trips_on_an_empty_misaligned_range_
/// in_debug` below with `decommit(1, 1)` — same "empty" range, misaligned
/// endpoints, tripwire fires.
#[test]
#[cfg(debug_assertions)]
fn method_bounds_check_precedes_the_tripwire_in_debug() {
    let mut r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();
    r.decommit(0, SPAN + ps); // end > self.len(): method-level silent skip
    r.decommit(0, 0); // empty PAGE-ALIGNED range: forwarded, well-formed at the free layer
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
    let mut r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
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
///
/// Task #1084/M2 added the empty-MISALIGNED lines (`(1, 1)`, `(ps + 1,
/// ps + 1)`): the free function's release filter skips them via `start >=
/// end`, pinning the release half of the corrected `# Panics` section —
/// an empty misaligned range is a contract violation (debug: tripwire;
/// release: silent no-op), not the unconditional no-op the pre-#1084 doc
/// promised.
#[test]
#[cfg(not(debug_assertions))]
fn method_silently_skips_a_violated_range_in_release() {
    let mut r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();
    r.decommit(4 * ps, 2 * ps); // start > end
    r.decommit(1, 2 * ps); // misaligned start
    r.decommit(0, 2 * ps + 1); // misaligned end
    r.decommit(0, SPAN + ps); // out of bounds
    r.decommit(1, 1); // empty AND misaligned (M2): release-profile silent no-op
    r.decommit(ps + 1, ps + 1); // same shape at a non-zero offset
    drop(r); // still a live reservation, releasable exactly once
}

/// Mechanism oracle for the release half (mock layer): a violated
/// `Reservation::decommit` call must record NO `Call::Decommit` /
/// `Call::DecommitLazy` at all, and a well-formed call through the SAME
/// method must record one — the positive control that proves the arm
/// really exercised the method (a "no records" assert with no positive
/// control could pass vacuously if the recorder itself were broken).
/// Mock+release only: in mock+debug the tripwire panics first, and without
/// the mock cfg there is no recorder. Executed by the task #1086 (L10)
/// mock-release rows (ci.yml aligned-vmem-gates + scripts/check-all.mjs),
/// which also assert this test's name appears in the output — a typo'd
/// `--cfg aligned_vmem_mock` turns those rows red instead of compiling
/// this test silently away.
#[test]
#[cfg(all(aligned_vmem_mock, not(debug_assertions)))]
fn method_records_nothing_for_a_violated_range_in_release_mock() {
    use aligned_vmem::mock::{self, Call};

    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    mock::reset();
    let mut r = reserve_aligned(SPAN, SPAN).expect("mock reserve chains to real backend");
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
    let mut r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
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

/// `Reservation::try_decommit`'s huge-page branch always reports `Ok(_)`
/// for a well-formed range (the free `try_decommit` deliberately does not
/// report OS refusal as an error either) — since task #1180 the `Ok` payload
/// is a [`aligned_vmem::DecommitOutcome`], `Skipped` for the branch this test
/// targets (see below); before task #1180 this was a bare `Ok(())` with no
/// payload distinguishing skip from a genuine backend call, which is the
/// historical shape the "Regression note" and counter narrative below
/// describe. Which of the TWO huge-page branches it took (real backend vs.
/// skip-and-count) is platform- and kernel-dependent since task #1140, so
/// this test asserts on `[0,
/// page_size())` — a range that is well-formed but never a
/// `LINUX_HUGE_PAGE_SIZE` (2 MiB) multiple — to stay on the skip branch
/// unconditionally regardless of host kernel version. Mirrors
/// `huge_decommit_attempts_increments_on_huge_reservation` in
/// `decommit_capability.rs`; on CI runners without a hugetlb pool /
/// `SeLockMemoryPrivilege` the fallback arm runs (see that test's own
/// NOTE). The full-huge-page-aligned-range real-backend case (task #1140) has
/// its own dedicated coverage:
/// `huge_aligned_range_takes_the_real_backend_path_not_the_skip_path` and
/// `simulated_huge_flag_drives_the_same_branch_dispatch_on_any_host`, both in
/// `decommit_capability.rs`.
///
/// **Regression note (task #1140, discovered during this task's own
/// verification):** this test originally asserted on `[0, size)` (the full
/// `size == 2 MiB` span), which — since task #1140 — is now itself an
/// ELIGIBLE range on Linux/Android kernel >= 5.18: a REAL hugetlb-pool host
/// running this test would take the real-backend branch for that call
/// (correct new behavior), leaving `huge_decommit_attempts()` at 0 instead
/// of the `1` this test used to assert — a false failure of a CORRECT
/// implementation, not a real regression. At the time this note was
/// written, no CI runner in this repo configured a hugetlb pool
/// (`r.is_huge()` was false on every runner then, so the `if` arm below had
/// never actually executed in CI), so this specific failure mode was
/// latent, not yet observed in CI, but was reproduced for real on a
/// manually-provisioned WSL2/Linux kernel 6.18 host with the flag genuinely
/// synthesized via `from_raw_parts` (see `decommit_capability.rs`'s
/// `simulated_huge_flag_drives_the_same_branch_dispatch_on_any_host`) during
/// this task's own verification pass. Narrowed to `[0, page_size())` here to
/// close the gap before a real hugetlb runner ever exercises it. **Updated
/// task #1160/F4:** that hugetlb runner now exists —
/// `aligned-vmem-hugetlb-real` (`.github/workflows/ci.yml`) configures a
/// real `nr_hugepages` pool and runs this test file, so `r.is_huge()` is
/// now `true` under that job and the `if` arm below DOES execute in CI. The
/// narrowing to `[0, page_size())` performed here stays correct and
/// necessary regardless (it is what keeps this specific test pinned to the
/// skip branch on purpose); it is no longer closing a gap that has zero CI
/// coverage, only choosing which of the two branches this particular test
/// exercises.
#[test]
#[cfg(all(feature = "bench-internals", feature = "huge-pages"))]
fn method_try_decommit_huge_skip_returns_ok_and_counts() {
    use aligned_vmem::{
        huge_decommit_attempts, page_size, reserve_aligned_huge, reset_bench_internals_counters,
        DecommitOutcome,
    };

    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    reset_bench_internals_counters();

    let size = 2 * 1024 * 1024; // Linux huge-page size
    let mut r = reserve_aligned_huge(size, size).expect("huge reservation (or fallback)");
    let ps = page_size();

    if r.is_huge() {
        assert_eq!(
            r.try_decommit(0, ps),
            Ok(DecommitOutcome::Skipped),
            "huge skip must report Ok(Skipped) — OS refusal is deliberately \
             not an error, and a Rust-level skip must not be conflated with \
             a genuinely-issued-and-accepted backend call (task #1180)"
        );
        assert_eq!(
            huge_decommit_attempts(),
            1,
            "a non-2-MiB-aligned range must still increment the same skip \
             counter as Self::decommit's skip (task #1140: a genuinely \
             2-MiB-aligned range would instead take the real backend path \
             on Linux/Android kernel >= 5.18 — deliberately not exercised \
             by this call)"
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

// ── task #1084, finding M2: the empty-misaligned range ──────────────────────

/// M2 (task #1084): `r.decommit(1, 1)` — EMPTY, IN bounds, MISALIGNED — must
/// trip the free function's debug tripwire through the safe method. The
/// pre-#1084 `# Panics` section claimed "empty and out-of-bounds ranges are
/// checked by this method first and never panic"; only the out-of-bounds
/// half was true. The method forwards the empty range unfiltered, and the
/// free layer classifies it as a contract violation because `1` is not a
/// multiple of `page_size()` — the same classification `try_decommit` gives
/// it (`Err`), and the same classification a NON-empty misaligned range
/// always had. `decommit(0, 0)` (empty AND aligned) stays the well-formed
/// no-op, pinned by `method_bounds_check_precedes_the_tripwire_in_debug`.
///
/// Pinning test, not a before/after counterfactual: task #1084/M2 resolved
/// the finding by correcting the DOC to match this (intended, task #1051/
/// #1079) behavior, so observable behavior is unchanged by the fix by
/// design. The test's bite was verified at task #1084 by temporarily
/// applying each rejected alternative fix and observing this test fail:
/// (a) an empty-range pre-filter at the top of the method — fails with
/// "test did not panic as expected"; (b) widening the free function's
/// `decommit_range_is_well_formed` to bless any empty range — same failure,
/// and it would also silently change `try_decommit(1, 1)` from `Err` to
/// `Ok`, the exact M3-shaped input-validation weakening this round exists
/// to prevent. A test that passes before AND after a doc-only fix is the
/// expected shape here; what it must NOT be is vacuous, which the two
/// temporary-alternative runs demonstrate.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "violates the range contract")]
fn method_trips_on_an_empty_misaligned_range_in_debug() {
    let mut r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    r.decommit(1, 1); // empty, in bounds, misaligned endpoints
}

// ── task #1084, finding M3: validation order vs the huge-page skip ──────────

/// M3 (task #1084): on a reservation with `is_huge() == true`, a malformed
/// range must still be reported as `Err` — `Reservation::try_decommit`'s
/// range-contract validation now runs BEFORE the huge-page skip, mirroring
/// the free `try_decommit`'s validate-first order. Before the fix the huge
/// early-return sat ahead of all validation, so `r.try_decommit(1, 3)` on a
/// huge reservation returned `Ok(())` — a caller who deliberately chose
/// the fallible form to detect a malformed range was told everything was
/// fine. The method's own doc promise (`Err(VmemError::invalid_argument())`)
/// and the free function agreed; only the method deviated.
///
/// **Why this test synthesizes the huge flag instead of requesting real
/// huge pages:** `reserve_aligned_huge` grants large pages only when the
/// OS can — Windows needs SeLockMemoryPrivilege enabled AND a working
/// locked-pages quota, Linux a configured hugetlb pool. Observed on this
/// file's authoring host (2026-08-18, Windows, privilege granted and
/// enabled in-process via `AdjustTokenPrivileges`, process working set
/// raised to 64 MiB, 9.6 GiB physical memory still available):
/// `VirtualAlloc(MEM_LARGE_PAGES)` fails system-wide with
/// ERROR_WORKING_SET_QUOTA_EXCEEDED (1450), so a real grant — and with it
/// a `reserve_aligned_huge`-based test's huge arm — is unreachable on that
/// host and on every host this crate's CI runs. Such a test would silently
/// take its ordinary-fallback arm everywhere and carry no regression value
/// at all. The huge skip, however, reads ONLY the `granted_huge` flag and
/// issues no OS call, so a flag-synthesizing reservation exercises the
/// exact code path under test with no fidelity loss FOR THIS PATH. The
/// synthesis is confined to this test's own `Reservation` value; see the
/// SAFETY comment at the reconstruction call for why it is sound and why
/// production callers must not copy it.
///
/// Counterfactual: the pre-fix FAILURE was observed at task #1084 on the
/// authoring host (debug build, this test): it FAILED at the first
/// `.is_err()` assert — `try_decommit(1, 3)` on the huge-flagged
/// reservation returned `Ok(())` — and the test aborted there, before
/// calls 2-4 ever ran. The `== 3` / `== 4` counter figures below were
/// therefore NOT observed in that run — the pre-fix counter at the moment
/// of that abort could be at most 1 (the first call's increment). They
/// are DERIVED (each pre-fix call incremented the skip's counter exactly
/// once before returning `Ok`), and were separately OBSERVED for real at
/// task #1096: with the fix order temporarily reverted (skip ahead of
/// validation) and a relaxed, assert-nothing probe running the same four
/// calls, the counter read 3 after the three malformed calls and 4
/// after the well-formed fourth, all four returning `Ok(())`. Passes
/// after the fix, with the malformed calls leaving the counter at 0 and
/// the well-formed call leaving it at 1. The well-formed `Ok` +
/// counter==1 shape additionally pins, on EVERY host, what
/// `method_try_decommit_huge_skip_returns_ok_and_counts` can only pin
/// where the OS really grants large pages.
///
/// **Well-formed probe range narrowed to `[0, page_size())`, not `[0, size)`
/// (task #1140):** the well-formed call below used to target the FULL `[0,
/// size)` span, where `size == 2 MiB == LINUX_HUGE_PAGE_SIZE` — which, since
/// task #1140, is now itself an ELIGIBLE range on Linux/Android kernel >=
/// 5.18 (`Reservation::try_decommit` forwards to the real backend instead of
/// skipping for a huge-page-size-aligned range). This test's own synthetic
/// `granted_huge: true` reservation is backed by an ORDINARY (non-`MAP_HUGETLB`)
/// mapping, so the "eligible" forward is not itself unsound here (an ordinary
/// mapping accepts `MADV_DONTNEED` at any granularity), but it DOES take the
/// real-backend branch instead of the skip branch this test exists to pin —
/// observed for real on WSL2/Linux kernel 6.18 during this task's own
/// verification: the well-formed `[0, size)` call still returned `Ok(())`
/// (unchanged), but `huge_decommit_attempts()` stayed at 0 instead of
/// reaching 1, failing this test's counter assertion even though nothing
/// about the validate-before-skip ORDERING this test targets had regressed.
/// Narrowing the well-formed probe to `[0, page_size())` — page-aligned,
/// in-bounds, but never a `LINUX_HUGE_PAGE_SIZE` (2 MiB) multiple on any
/// supported host — keeps this test on the skip path unconditionally, on
/// every platform and kernel version, which is what its own M3 validate-
/// before-skip claim needs. The huge-aligned real-path case has its own
/// dedicated coverage: `huge_aligned_range_takes_the_real_backend_path_not_the_skip_path`
/// and `simulated_huge_flag_drives_the_same_branch_dispatch_on_any_host` in
/// `tests/decommit_capability.rs`.
///
/// **Gated on `huge-pages` (task #1172/M1-hybrid).** This test synthesizes
/// `granted_huge: true` via `ReservationFullParts`/`from_raw_parts`, and
/// `from_raw_parts` now `assert!`s that accepting `granted_huge: true`
/// requires this crate's own `huge-pages` feature to be enabled (closing
/// finding M2) — without the feature, the reconstruction below panics before
/// the test's own assertions run. Its sibling
/// `simulated_huge_flag_drives_the_same_branch_dispatch_on_any_host`
/// (`tests/decommit_capability.rs`) is already gated the same way.
#[test]
#[cfg(feature = "huge-pages")]
fn method_try_decommit_reports_malformed_range_on_huge_flagged_reservation() {
    use aligned_vmem::ReservationFullParts;

    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let size = 2 * 1024 * 1024;
    let ordinary = reserve_aligned(size, size).expect("reserve 2 MiB");
    // All five pointer/size fields verbatim from a live reservation; only
    // the flag is synthesized (overridden in the `ReservationFullParts::new`
    // call below).
    let parts = ordinary.into_full_parts();
    let huge_flagged = ReservationFullParts::new(
        parts.base,
        parts.len,
        parts.reservation,
        parts.reservation_len,
        parts.align,
        /* granted_huge = */ true,
    );
    // SAFETY: every pointer/size field is genuine — taken verbatim from a
    // live `reserve_aligned(2 MiB, 2 MiB)` reservation via `into_full_parts`,
    // so `base`/`reservation` are live and exclusively owned, all layout
    // invariants `from_raw_parts` asserts hold (including, since task #1172,
    // the Linux/Android 2-MiB-multiple Correctness-contract check — trivially
    // satisfied here because `size == 2 MiB` and `reserve_aligned`'s
    // exact-size fast path leaves `base == reservation`,
    // `len == reservation_len == size`), and the reconstructed handle's
    // `Drop` releases the mapping exactly once (no double release:
    // `into_full_parts` consumed the original). This test is gated on the
    // `huge-pages` feature (see this test's own `#[cfg]`) specifically
    // because task #1172's OTHER new Correctness-contract check — accepting
    // `granted_huge: true` at all requires `huge-pages` to be enabled —
    // would otherwise panic before any assertion below runs; that check is
    // satisfied here by construction of the gate, not by the values passed.
    // The deviations from `from_raw_parts`' documented `# Safety`/
    // "Correctness contract" are exactly TWO bullets, both violated by
    // `granted_huge: true` over an ordinary-page reservation:
    //
    // 1. The accuracy bullet: `granted_huge` MUST reflect whether the OS
    //    actually granted huge pages. Its stated consequence for a wrong
    //    value is a wrong `is_huge()` report and wrong downstream
    //    decommit-availability decisions — "not a crash or undefined
    //    behavior".
    // 2. The Windows commit-state compatibility bullet: `granted_huge ==
    //    true` requires creation with `MEM_RESERVE | MEM_COMMIT |
    //    MEM_LARGE_PAGES` in a single call, and an adopted reservation of
    //    unknown provenance MUST pass `false`. This ordinary reservation
    //    was not created with `MEM_LARGE_PAGES`, so on Windows this is the
    //    bullet most directly violated; its stated consequence is likewise
    //    only an `is_huge()` "value inconsistent with the reservation's
    //    actual state" — non-UB.
    //
    // Both violated bullets live in `from_raw_parts`'s "Correctness
    // contract" section (task #1172/M3 split), not its `# Safety` section —
    // that split exists precisely so this test can state, truthfully, that
    // it violates documented requirements without violating anything
    // memory-safety-relevant.
    //
    // PRIMARY justification — why neither violation can become a
    // memory-safety hazard in this crate TODAY — is the complete
    // enumeration of every reader of `granted_huge`/`is_huge()` in
    // `crates/aligned-vmem/src/` (re-derived for task #1098, not copied
    // from the review, and mechanically pinned by
    // `tests/granted_huge_reader_enumeration.rs`, which fails when the
    // reader set changes so this argument cannot silently rot):
    //
    // - `Reservation::is_huge` (src/reservation.rs) — pure query, returns
    //   the field.
    // - `Reservation::can_decommit_reclaim_and_zero` (src/reservation.rs)
    //   — pure advisory query, `platform_const && !self.is_huge()`; no
    //   syscall, no unsafe. (NOTE: the #1098 review's own enumeration
    //   missed this one; re-derivation found it. It does not change the
    //   verdict — it is a query, not an act.)
    // - three huge-skips in the decommit family — `Self::decommit`,
    //   `Self::try_decommit`, `Self::decommit_lazy` (src/reservation.rs),
    //   each `if self.is_huge() { count; return; }` — the flag can only
    //   SUPPRESS a backend OS call.
    // - `Reservation::from_raw_parts` itself (src/reservation.rs), as of
    //   task #1172's M1-hybrid: two `assert!`s, both Correctness-contract
    //   checks reading the PARAMETER (before it becomes the field) — (a)
    //   `granted_huge: true` requires the `huge-pages` feature, (b) on
    //   Linux/Android, `granted_huge: true` additionally requires `len`,
    //   `reservation_len`, `reservation`, `base`, and the offset
    //   `base - reservation` to all be 2-MiB multiples (five names listed,
    //   but only four independent checks — `reservation` and `base` both
    //   being 2-MiB multiples already implies their difference is too;
    //   task #1196/OX6-L1). Both PANIC immediately on violation — a louder
    //   failure mode than the other readers above (which only change a
    //   query result or suppress a syscall), but still not memory-unsafe:
    //   the panic happens before
    //   `Self { .. }` is constructed, so no `Reservation` with inconsistent
    //   state is ever observable. This is exactly why this test is gated on
    //   `huge-pages` and uses a `size == 2 MiB` base reservation — both
    //   asserts are satisfied by construction, so this synthesis reaches
    //   the SAME two Correctness-contract violations (1) and (2) above that
    //   it always has, without also tripping either of `from_raw_parts`'s
    //   own two new panics.
    //
    // Confirmed non-readers: `Drop` releases via `reservation`/
    // `reservation_len`/`align` only. The remaining field touches are
    // copies/observations that cannot branch on it (the `Debug` impl,
    // `into_full_parts`, `ReservationFullParts::into_reservation`,
    // `finish_reservation`).
    // A wrong flag therefore changes two pure query RESULTS, whether up to
    // three decommit backend calls are issued, and (new in task #1172)
    // whether `from_raw_parts` itself panics — never memory safety, never
    // release behavior, never an unsafe block's preconditions. SECONDARY,
    // and deliberately weaker: the rustdoc consequence quotes above say
    // "not a crash or undefined behavior" — but that is intent stated for
    // one bullet, and a future refactor that makes `is_huge()` gate
    // something real would invalidate the
    // prose without touching this test; the enumeration is what carries
    // the weight. Production callers must NOT copy this synthesis — pass
    // a truthful `granted_huge`.
    let mut r = unsafe { huge_flagged.into_reservation() };
    assert!(r.is_huge(), "synthetic flag must produce is_huge() == true");

    #[cfg(feature = "bench-internals")]
    aligned_vmem::reset_bench_internals_counters();

    let ps = page_size();
    assert!(
        r.try_decommit(1, 3).is_err(),
        "misaligned non-empty range must be Err even on a huge reservation"
    );
    assert!(
        r.try_decommit(1, 1).is_err(),
        "misaligned EMPTY range must be Err even on a huge reservation \
         (M2's range shape, M3's path)"
    );
    assert!(
        r.try_decommit(2 * ps, ps).is_err(),
        "start > end must be Err even on a huge reservation"
    );
    // Narrowed to `[0, ps)` (task #1140) — see the doc comment above this
    // test for why `[0, size)` (== `[0, 2 MiB)`) would now be an ELIGIBLE
    // range on Linux/Android kernel >= 5.18, taking the real-backend path
    // rather than the skip path this test targets. `[0, ps)` is page-aligned
    // and in-bounds (well-formed) but never a 2-MiB multiple, so it stays on
    // the skip path unconditionally, on every platform and kernel version.
    // task #1180: `Ok(DecommitOutcome::Skipped)`, not a bare `Ok(())` — see
    // this test's own module-level doc note on the historical `Ok(())` shape.
    assert_eq!(
        r.try_decommit(0, ps),
        Ok(aligned_vmem::DecommitOutcome::Skipped),
        "a well-formed range must still be Ok(Skipped) on a huge reservation \
         (skip, not error, and not conflated with a genuine backend call)"
    );

    // Mechanism oracle (bench-internals builds only): the three malformed
    // calls above must not have incremented the huge-attempt counter —
    // validation now precedes the skip. Pre-fix this read 4 — derived
    // at task #1084 (the pre-fix test aborts at the first assert, counter
    // at 1) and observed via the task-#1096 relaxed probe: every call,
    // malformed included, hit the skip's counter before returning Ok.
    #[cfg(feature = "bench-internals")]
    {
        use aligned_vmem::huge_decommit_attempts;
        assert_eq!(
            huge_decommit_attempts(),
            1,
            "only the ONE well-formed call may count as a huge attempt; \
             malformed ranges are rejected before the skip"
        );
    }
}
