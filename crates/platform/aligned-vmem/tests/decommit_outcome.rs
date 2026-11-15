//! Task #1180 (PUB-R2 phase 2; headline corrected task #1229/F7, and again
//! task #1219): counterfactual tests covering ALL THREE [`DecommitOutcome`]
//! variants — one `Advised`, two `Skipped`, and (since task #1219) one
//! `Refused` — each asserting the SPECIFIC variant returned (not merely
//! `is_ok()`) and each stating what a dispatch bug would have to do to
//! make that assertion pass anyway (so the test is not vacuous — see item
//! #1073's "touch the test file and rebuild before trusting a
//! counterfactual" rule, honoured by all four tests below).
//!
//! Which rows run how many of them, stated up front because the gates
//! differ: the huge-page-skip `Skipped` test is gated on `huge-pages`, and
//! the `Refused` test is gated on `all(feature = "fault-injection",
//! not(aligned_vmem_mock))` — so a default-feature row runs TWO of the four
//! (Advised + empty-range Skipped), a row with exactly one of the two gates
//! on runs THREE (`huge-pages` alone adds the huge-skip `Skipped`;
//! `fault-injection` alone, mock off — e.g. the
//! `--features "fault-injection lazy-commit"` CI row — adds the `Refused`
//! one), and only a row with BOTH (`--all-features`, or the Windows/macOS
//! explicit `lazy-commit huge-pages fault-injection bench-internals` lists —
//! in every case with the mock cfg OFF) runs all FOUR. Verified per row by
//! local runs of each combination, not derived: 2 / 3 / 3 / 4.
//!
//! The third variant, `Refused`, had ZERO test coverage anywhere in this
//! crate between task #1210 and task #1219: its only test needed
//! out-of-bounds `pointer::add` arithmetic — UB in computing the pointer,
//! not merely in dereferencing it — to reach a genuine OS refusal, so it
//! was deleted outright rather than cfg-gated a third time. Task #1219
//! restored coverage through a real-path fault-injection seam in `src/`
//! (`fault_injection::arm_fail_next_decommit`, consulted from
//! `dispatch_try_decommit` in `src/api/decommit.rs`) — NOT by reviving the
//! UB test; see [`refused_variant_is_produced_by_the_fault_injection_seam_on_both_fallible_entry_points`]
//! below for exactly what that test does and does not prove.
//!
//! Correction note, kept so the history is not re-litigated: this
//! paragraph's original headline claimed "three counterfactual tests, one
//! per `DecommitOutcome` variant" — true when `b920b29` (task #1180)
//! created this file with exactly those three tests, false ever since
//! `dcc2a2f` (task #1192) added a fourth, second-`Skipped` test, and false
//! in the opposite direction (three tests, two variants) once `1f930e2`
//! (task #1210) deleted the `Refused` one. The stale headline survived
//! every one of those edits — including `1f930e2`, the commit whose
//! stated purpose was to record the `Refused` coverage loss honestly,
//! which rewrote the `Refused` bullet below and left the headline above
//! implying the loss did not exist.
//!
//! What each test can/cannot prove on THIS host, stated up front:
//! - [`skipped_variant_is_produced_by_a_huge_page_skip`] uses the
//!   `from_raw_parts`-fabricated `granted_huge` pattern already established
//!   by `decommit_capability.rs`'s
//!   `simulated_huge_flag_drives_the_same_branch_dispatch_on_any_host` — this
//!   makes `Skipped` deterministically reproducible on ANY host (Windows
//!   included), because it does not depend on a real OS huge-page grant.
//! - [`advised_variant_is_produced_by_a_genuinely_accepted_decommit`] needs no
//!   fabrication: an ordinary, in-span, page-aligned, non-empty decommit is
//!   accepted by both Linux `madvise(MADV_DONTNEED)` and Windows
//!   `VirtualFree(MEM_DECOMMIT)`, so this is real on every platform this
//!   crate's CI runs, including this task's own Windows verification host.
//! - `refused_variant_is_produced_by_a_genuine_os_refusal` **no longer
//!   exists as a runnable test (task #1210), and its NAME is deliberately
//!   not reused by its task-#1219 replacement** — named in plain backticks,
//!   NOT as an intra-doc link, precisely because the item it would link to
//!   is the one this bullet says was deleted. It used to reach a range far
//!   outside the live `MEM_RESERVE`/`mmap` region via `far_start = 64 *
//!   1024 * 1024` on a 2 MiB reservation. That is not merely a bad bet on
//!   process layout (the framing task #1202/#1206 left it at) — computing
//!   `base.add(far_start)` on a 2 MiB allocation is Undefined Behaviour by
//!   `pointer::add`'s own contract regardless of whether the resulting
//!   pointer is ever dereferenced: `add` requires the result to remain
//!   in bounds of the SAME allocated object (or one byte past its end),
//!   and `base + 64 MiB` is ~32x past the end of a 2 MiB object. This was
//!   true on every cfg the old test compiled under, including the
//!   Windows-only row it was narrowed to by task #1199 — narrowing the
//!   `#[cfg]` changed which platforms executed the UB, not whether
//!   computing the pointer was UB. Its replacement (task #1219) does NOT
//!   exhibit UB and does NOT claim a genuine OS refusal; see
//!   [`refused_variant_is_produced_by_the_fault_injection_seam_on_both_fallible_entry_points`]
//!   for the exact split between what is proven and what is not.
//!
//! One limitation of THIS FILE under `--cfg aligned_vmem_mock`, stated so it
//! is not mistaken for coverage it does not give: with the mock backend,
//! [`advised_variant_is_produced_by_a_genuinely_accepted_decommit`] observes
//! the mock arm's unconditional `Advised` rather than a real accepted
//! syscall, so under that row it no longer distinguishes "the OS accepted"
//! from "nothing was asked". Its stated counterfactual (a dispatch collapsed
//! to always-`Skipped` fails this test) does still hold there, which is why
//! it is kept rather than gated off alongside the refusal test — but the
//! variant-to-real-OS-outcome binding is proven only on the non-mock rows.
//! - [`skipped_variant_is_produced_by_an_empty_range_on_the_free_function`]
//!   (task #1192) closes the gap the three then-existing tests left (one
//!   of which — the `Refused` one — task #1210 has since deleted): before
//!   this test, the free `try_decommit`'s `start == end` short-circuit
//!   (the `if start == end { return Ok(DecommitOutcome::Skipped); }`
//!   early-return arm of `try_decommit`, `src/api/decommit.rs` — cited by
//!   symbol and branch, not by line number, task #1229/F8: the previous
//!   line-range citation here drifted within the very wave that wrote it)
//!   had NO test coverage at all — `Skipped` was
//!   only exercised via the `huge-pages`-gated fabricated-huge-flag test
//!   above. This test needs no feature gate and no fabrication: an empty,
//!   page-aligned range is a well-formed no-op on ANY reservation, ordinary
//!   or huge, so it is the one `Skipped` source that is unconditionally real
//!   on every host and every feature configuration.

use aligned_vmem::{page_size, reserve_aligned, try_decommit, DecommitOutcome};
// Gated with the SAME cfg as the `Refused` test below (the file's own rule
// from task #1192, see the `Reservation` import note): an ungated import of
// `arm_fail_next_decommit`/`VmemError` fails `cargo clippy -p aligned-vmem
// --all-targets -- -D warnings` (the DEFAULT-features CI row) with
// `unused_imports` on exactly the rows where the test is compiled out.
#[cfg(all(feature = "fault-injection", not(aligned_vmem_mock)))]
use aligned_vmem::fault_injection::arm_fail_next_decommit;
#[cfg(all(feature = "fault-injection", not(aligned_vmem_mock)))]
use aligned_vmem::VmemError;
// `Reservation` is named ONLY by the huge-skip test below, which is gated on
// `huge-pages` — so importing it unconditionally makes the DEFAULT feature row
// fail `cargo clippy -p aligned-vmem --all-targets -- -D warnings`
// (`.github/workflows/ci.yml`'s first aligned-vmem clippy row) with
// `unused_imports`. That is not hypothetical: it shipped in `b920b29` (task
// #1180, the commit that created this file) and left `main` red on that one
// row until task #1192 measured it against a clean HEAD worktree. Gating the
// import with the same `cfg` as its only user is the minimal fix; do not
// widen it back.
#[cfg(feature = "huge-pages")]
use aligned_vmem::Reservation;

const SPAN: usize = 2 * 1024 * 1024;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// `DecommitOutcome::Advised`: a genuinely-issued, genuinely-accepted
/// backend call.
///
/// **What would make this fail if `dispatch_try_decommit` regressed:** if the
/// dispatch were changed to report `Skipped` for every call (collapsing the
/// distinction this task exists to add), this test fails immediately — it
/// does not merely check `is_ok()`, which a `Skipped`-always dispatch would
/// still satisfy.
#[test]
fn advised_variant_is_produced_by_a_genuinely_accepted_decommit() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();

    // SAFETY: `r` is live and `[ps, 2*ps)` is inside its usable span.
    let out = unsafe { try_decommit(r.as_ptr(), ps, 2 * ps) };
    assert_eq!(
        out,
        Ok(DecommitOutcome::Advised),
        "an in-span, page-aligned, non-empty range on an ordinary \
         reservation must be genuinely advised to the OS and accepted"
    );
}

/// `DecommitOutcome::Skipped`: an empty, well-formed range (`start == end`)
/// on the FREE `try_decommit` function — its `start == end` early-return
/// arm (the `if start == end { return Ok(DecommitOutcome::Skipped); }`
/// branch of `try_decommit` in `src/api/decommit.rs`, cited by symbol and
/// branch rather than line number after the previous line-range citation
/// drifted within the same wave — task #1229/F8), checked and returned
/// before `dispatch_try_decommit` is ever called. Deliberately does NOT
/// require the
/// `huge-pages` feature: this is the one `Skipped` source the free function
/// itself can produce (see the corrected doc on `DecommitOutcome::Skipped`),
/// independent of any huge-page eligibility question.
///
/// **What would make this fail if the free `try_decommit` regressed:** if the
/// `if start == end { return Ok(DecommitOutcome::Skipped); }` branch were
/// changed to fall through to `dispatch_try_decommit` instead of
/// short-circuiting (i.e. forwarding an empty range to the real backend),
/// this test would observe `Advised` or `Refused` instead of `Skipped` and
/// fail — a direct counterfactual on the exact defect class task #1192
/// exists to guard against (the rustdoc claiming the free function "always
/// forwards to the backend").
#[test]
fn skipped_variant_is_produced_by_an_empty_range_on_the_free_function() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();

    // SAFETY: `r` is live and `[ps, ps)` (empty) is trivially within its
    // usable span.
    let out = unsafe { try_decommit(r.as_ptr(), ps, ps) };
    assert_eq!(
        out,
        Ok(DecommitOutcome::Skipped),
        "an empty (start == end) well-formed range on the free try_decommit \
         must short-circuit to Skipped before any backend call"
    );
}

/// `DecommitOutcome::Skipped`: a Rust-level skip, no backend call issued —
/// reproduced via the SAME `from_raw_parts`-fabricated `granted_huge`
/// pattern `decommit_capability.rs`'s
/// `simulated_huge_flag_drives_the_same_branch_dispatch_on_any_host` already
/// established, so this is deterministic on any host (no dependency on a
/// real OS huge-page grant this task's Windows host cannot reliably obtain
/// without `SeLockMemoryPrivilege`).
///
/// Uses a range that is well-formed but NOT huge-page-size (2 MiB) aligned
/// at both endpoints (`[0, ps)`), which stays on the skip path on EVERY
/// platform and kernel version (task #1140's Linux/Android >= 5.18 carve-out
/// requires 2-MiB alignment at both ends, which `[0, ps)` does not have
/// unless `ps == 2 MiB` — not a page size this crate's supported hosts use).
///
/// **What would make this fail if `dispatch_try_decommit` regressed:** if
/// the huge-skip branch in `Reservation::try_decommit` were changed to call
/// `dispatch_try_decommit` instead of returning `Skipped` directly (the
/// exact bug this variant's own doc on `DecommitOutcome::Skipped` warns
/// against — a Rust-level skip must never reach the backend), this test
/// would observe `Advised` or `Refused` instead of `Skipped` and fail.
#[test]
#[cfg(feature = "huge-pages")]
fn skipped_variant_is_produced_by_a_huge_page_skip() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let ordinary = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB ordinary");
    let mut parts = ordinary.into_full_parts();
    // Fabricate the huge flag — sound because no unsafe operation this crate
    // performs branches on this field (see the precedent test's own comment
    // in decommit_capability.rs for the full soundness argument this test
    // reuses verbatim).
    parts.granted_huge = true;
    // SAFETY: `parts` came from a real, live `into_full_parts()` call on a
    // reservation this test still exclusively owns; only `granted_huge` was
    // mutated, every other invariant holds unchanged.
    let mut simulated_huge: Reservation = unsafe { parts.into_reservation() };
    assert!(
        simulated_huge.is_huge(),
        "sanity: fabricated flag round-tripped"
    );

    let ps = page_size();
    // [0, ps) is well-formed but not 2-MiB-aligned at `end` -- guaranteed to
    // take the skip path on every platform/kernel (mirrors
    // decommit_capability.rs's own established range choice for the same
    // reason).
    let out = simulated_huge.try_decommit(0, ps);
    assert_eq!(
        out,
        Ok(DecommitOutcome::Skipped),
        "a non-2-MiB-aligned range on a huge-flagged reservation must never \
         reach the real backend"
    );
}

/// `DecommitOutcome::Refused`, restored deterministically (task #1219) via the
/// real-path fault-injection seam — `fault_injection::arm_fail_next_decommit`,
/// consulted from `dispatch_try_decommit`'s `#[cfg(not(aligned_vmem_mock))]`
/// branch in `src/api/decommit.rs`, which replaces the syscall with an
/// injected `Err` routed through the SAME `Err(e) => Refused(e)` mapping arm a
/// real backend refusal takes.
///
/// **What this test proves, stated exactly:** the `Err(e) =>
/// DecommitOutcome::Refused(e)` mapping arm in `dispatch_try_decommit` is
/// REACHABLE from BOTH fallible entry points (the free `try_decommit` and
/// `Reservation::try_decommit`) and constructs the outcome carrying exactly
/// the error value the backend layer produced (asserted by full `PartialEq`
/// on `Ok(DecommitOutcome::Refused(VmemError::os_refusal_unknown_code()))`,
/// so an arm that drops or substitutes the payload fails); the hook is
/// one-shot and its firing does not affect the next call, which reaches the
/// REAL backend and is accepted (`Advised`, the same oracle
/// [`advised_variant_is_produced_by_a_genuinely_accepted_decommit`] relies
/// on).
///
/// **What it does NOT prove:** that any OS refused anything. No syscall ran
/// on the refused call — the fault was injected before the backend was
/// reached, which is why the payload is the no-code sentinel
/// `VmemError::os_refusal_unknown_code()` and not a captured
/// `last_os_error()`. `Refused` arising from a REAL kernel refusal
/// (`madvise` returning `-1`, `VirtualFree` returning zero) remains without
/// deterministic coverage, and every avenue for producing one from `tests/`
/// alone was checked and rejected before task #1210 deleted the old test
/// (out-of-bounds address: UB in the pointer arithmetic itself, the reason
/// the old test is gone and must not return; any in-reservation address:
/// accepted by construction, decommitting an uncommitted sub-range is a
/// documented no-op; the `granted_huge` fabrication: steers only a Rust-level
/// branch, yields `Skipped`; reserve-then-release-then-decommit: a re-mapping
/// race that can observe `Advised` against another mapping). That gap is
/// permanent unless someone finds a mechanism this list missed — the honest
/// claim of this test is reachability-and-construction of the variant, not
/// OS behaviour.
///
/// **What would make this fail if the seam regressed:** three independent
/// ways. (1) If the `#[cfg(feature = "fault-injection")]` consult were
/// removed from `dispatch_try_decommit` (or stopped firing), the first
/// assertion observes `Advised` and fails. (2) If the mapping arm were
/// changed to discard or substitute its payload, the exact-equality
/// assertions fail. (3) If `Reservation::try_decommit` stopped routing
/// through `dispatch_try_decommit`, the second assertion fails.
///
/// **Gate and rows, so this never becomes a green-and-dead test (the #1095
/// class):** gated `all(feature = "fault-injection", not(aligned_vmem_mock))`
/// — under the mock cfg `dispatch_try_decommit`'s real-path branch (the
/// hook's only call site) is compiled out and the mock arm returns `Advised`
/// unconditionally, so the test would be vacuous there. It RUNS in: the
/// `test workspace members` job's `cargo test -p aligned-vmem --all-features`
/// and `cargo test -p aligned-vmem --features "fault-injection lazy-commit"`
/// rows (`.github/workflows/ci.yml` — the latter row exists precisely
/// because a fault-injection-gated file compiled to zero tests under
/// `--all-features`-plus-mock before task #699), the `test-windows` and
/// `test-macos` jobs' explicit
/// `--features "lazy-commit huge-pages fault-injection bench-internals"`
/// rows, and `npm run check`'s `test (aligned-vmem --all-features)` step
/// (`scripts/check-all.mjs`). It does NOT run in the mock-cfg rows or any
/// default-features row.
#[test]
#[cfg(all(feature = "fault-injection", not(aligned_vmem_mock)))]
fn refused_variant_is_produced_by_the_fault_injection_seam_on_both_fallible_entry_points() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // Disarm any residue from a prior test in this binary (the hook is a
    // process-global atomic; SERIAL serializes this file's tests, but a
    // failed prior test could have left it armed).
    arm_fail_next_decommit(0);

    let mut r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();

    // Entry point 1: the FREE try_decommit, with one fault armed.
    arm_fail_next_decommit(1);
    // SAFETY: `r` is live and `[ps, 2*ps)` is inside its usable span.
    let free_path = unsafe { try_decommit(r.as_ptr(), ps, 2 * ps) };
    assert_eq!(
        free_path,
        Ok(DecommitOutcome::Refused(
            VmemError::os_refusal_unknown_code()
        )),
        "an armed decommit fault through the free try_decommit must reach \
         the Err(e) => Refused(e) mapping arm and carry the injected \
         no-code error unchanged"
    );

    // Entry point 2: Reservation::try_decommit (safe method, ordinary —
    // non-huge — reservation), with a fresh fault armed.
    arm_fail_next_decommit(1);
    let method_path = r.try_decommit(ps, 2 * ps);
    assert_eq!(
        method_path,
        Ok(DecommitOutcome::Refused(
            VmemError::os_refusal_unknown_code()
        )),
        "an armed decommit fault through Reservation::try_decommit must \
         reach the SAME mapping arm — the method routes through \
         dispatch_try_decommit, not a private copy"
    );

    // One-shot + real-backend coexistence: the hook is consumed, so the same
    // well-formed range through the free function must now reach the REAL
    // backend and be accepted — deterministically, because an in-span,
    // page-aligned, non-empty range on an ordinary reservation is accepted
    // by every real backend this crate's CI runs (the oracle the `Advised`
    // test above already relies on).
    // SAFETY: same live reservation, same in-span range.
    let after = unsafe { try_decommit(r.as_ptr(), ps, 2 * ps) };
    assert_eq!(
        after,
        Ok(DecommitOutcome::Advised),
        "after the one-shot fault is consumed, the next call must reach the \
         real backend and be accepted, proving the seam coexists with \
         rather than replaces the real path"
    );
}
