//! Task #1180 (PUB-R2 phase 2; headline corrected task #1229/F7): three
//! counterfactual tests covering TWO of the THREE [`DecommitOutcome`]
//! variants — one `Advised` and two `Skipped` (the huge-page-skip one is
//! gated on `huge-pages`, so a default-feature row runs only two of the
//! three) — each asserting the SPECIFIC variant returned (not merely
//! `is_ok()`) and each stating what a dispatch bug would have to do to
//! make that assertion pass anyway (so the test is not vacuous — see item
//! #1073's "touch the test file and rebuild before trusting a
//! counterfactual" rule, honoured by all three tests below).
//!
//! The third variant, `Refused`, has ZERO test coverage in this file and
//! none deterministic anywhere in this crate (task #1210): its only test
//! needed out-of-bounds `pointer::add` arithmetic — UB in computing the
//! pointer, not merely in dereferencing it — to reach a genuine OS
//! refusal, so it was deleted outright rather than cfg-gated a third
//! time. Restoring that coverage requires a `src/` seam and is tracked as
//! task #1219; see
//! [`refused_variant_has_no_deterministic_coverage_in_this_file`] below
//! for the full record of every avenue checked and rejected.
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
//!   exists as a runnable test (task #1210)** — named in plain backticks,
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
//!   computing the pointer was UB. See
//!   [`refused_variant_has_no_deterministic_coverage_in_this_file`] for
//!   what replaces it and why real `Refused` coverage needs a seam this
//!   file's scope does not include.
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

/// `DecommitOutcome::Refused` has **no deterministic test coverage in this
/// file** (task #1210). This is a marker/doc function, not a test — it
/// exists so the gap has one grep-able name instead of living only in a
/// commit message.
///
/// # Why the old test was removed outright, not merely re-gated
///
/// The previous `refused_variant_is_produced_by_a_genuine_os_refusal`
/// (removed by task #1210) called
/// `try_decommit(r.as_ptr(), far_start, far_end)` with `far_start = 64 *
/// 1024 * 1024` against a `SPAN = 2 * 1024 * 1024` reservation. Internally
/// that free function's Windows backend
/// (`decommit_pages_impl`, `src/os/windows.rs`) computes
/// `base.add(start)` before ever calling `VirtualFree`. `pointer::add`'s own
/// contract requires the resulting pointer to stay within the bounds of the
/// SAME allocated object (or one byte past its end) — `base + 64 MiB` is
/// ~32x past the end of a 2 MiB object, so **computing that pointer is
/// Undefined Behaviour**, independent of whether `VirtualFree` ever
/// dereferences it. `VirtualFree` genuinely never dereferences the address
/// (it only manipulates kernel page-table state) — that part of the old
/// `# Safety` comment was factually correct about the memory-ACCESS — but
/// it does not make the pointer ARITHMETIC defined; `add`'s contract is
/// violated the moment it executes, before any OS call happens. Task #1199
/// had already narrowed this test's `#[cfg]` to Windows-only after a
/// same-shape address landed inside a live mapping on Linux in CI
/// (`dff7b1d`) and got genuinely decommitted instead of refused; that
/// narrowing changed which platform's ABI happened to make the bet usually
/// pay off, but never addressed that the address computation itself was
/// already unsound on every platform, including the one the test kept
/// running on. So the fix here is not a tighter `#[cfg]` (a third
/// narrowing after #1197 and #1199 would repeat the same mistake) — it is
/// deleting the out-of-bounds arithmetic instead of trying to gate it to a
/// cfg where it happens not to get caught.
///
/// # Why a real replacement needs a seam this file's scope does not have
///
/// Every avenue available from `tests/decommit_outcome.rs` alone (no edits
/// to `src/`, which is out of scope for this file) was checked and rejected:
///
/// - **A far-but-in-bounds address is not available.** The whole point of
///   `Refused` is "the backend was genuinely called and the OS declined it";
///   any address inside the live reservation's own `MEM_RESERVE` region is
///   accepted by `VirtualFree(MEM_DECOMMIT)` (decommitting an
///   already-uncommitted sub-range is a documented safe no-op — see
///   `decommit_pages_impl`'s own doc comment in `src/os/windows.rs`), so it
///   cannot produce `Refused`.
/// - **The `granted_huge`-fabrication pattern** already used by
///   [`skipped_variant_is_produced_by_a_huge_page_skip`] above only steers
///   which RUST-LEVEL branch runs (`Reservation::try_decommit`'s huge-skip
///   check reads `self.granted_huge`, which carries no OS-visible effect —
///   see that test's own doc for why fabricating it is sound). It cannot
///   make the OS itself refuse a call, because a huge-flagged reservation on
///   Windows takes the unconditional Rust-level skip (never reaches the
///   backend at all — `Reservation::decommit`'s doc, "on Windows, decommit
///   NEVER works" — `src/reservation.rs`), so that path produces `Skipped`,
///   not a genuine `Refused`.
/// - **The withdrawn "reserve, release, then decommit the freed address"
///   idea** (recorded as a candidate in the superseded version of this
///   comment, and independently flagged by the orchestrator's own task #1210
///   brief as WITHDRAWN) carries a re-mapping race: between `release` and
///   the following `try_decommit`, another thread/allocation in this same
///   process could re-`VirtualAlloc` over the freed address range, so the
///   call could just as easily observe a genuine `Advised` against someone
///   else's fresh mapping. Less reliable than a controlled backend result,
///   not a fix.
/// - **`fault_injection.rs`'s `arm_fail_next`/`arm_fail_at`** only intercept
///   the real COMMIT path (`crate::try_commit_range`, gated on
///   `lazy-commit`) — see that module's own doc, "the next `n` real commit
///   calls fail" — there is no decommit-side equivalent to arm.
/// - **The `aligned_vmem_mock` backend's `Call::Decommit` recording**
///   (`src/mock.rs`) always resolves to `DecommitOutcome::Advised`
///   unconditionally by design (`dispatch_try_decommit`'s mock arm,
///   `src/api/decommit.rs`) — no scripted-failure hook exists for it, unlike
///   `fail_next_commit`/`fail_next_reserve`.
///
/// Every option that would produce a genuine `Refused` deterministically —
/// a decommit-side fault-injection hook mirroring `fault_injection.rs`'s
/// commit-side one, or a scripted `Call::Decommit` failure in `mock.rs`
/// mirroring `fail_next_commit` — requires adding a hook to `src/`. That is
/// out of scope for this file (owned by a sibling agent in this round) and
/// is reported to the orchestrator as a request rather than implemented
/// here.
///
/// **Consequence, stated so it is not mistaken for coverage:** the
/// `Refused` variant has NO deterministic test coverage anywhere in this
/// crate as of task #1210. This is strictly worse than the pre-#1210 state
/// in coverage terms (which had non-deterministic, UB-tainted Windows-only
/// coverage) but is the correct trade: a test that exhibits Undefined
/// Behaviour to pass is not coverage, it is a hazard that happened to not
/// yet have visibly misbehaved on this host.
#[allow(dead_code)]
fn refused_variant_has_no_deterministic_coverage_in_this_file() {}
