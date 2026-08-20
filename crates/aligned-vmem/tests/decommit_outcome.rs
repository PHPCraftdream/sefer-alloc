//! Task #1180 (PUB-R2 phase 2): three counterfactual tests, one per
//! [`DecommitOutcome`] variant, each asserting the SPECIFIC variant returned
//! (not merely `is_ok()`) and each stating what a dispatch bug would have to
//! do to make that assertion pass anyway (so the test is not vacuous — see
//! item #1073's "touch the test file and rebuild before trusting a
//! counterfactual" rule, honoured for all three below).
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
//! - [`refused_variant_is_produced_by_a_genuine_os_refusal`] uses the free
//!   `try_decommit`'s documented lack of a bounds check (unlike
//!   `Reservation::try_decommit`, which pre-checks `end > self.len()`) to
//!   reach a range far outside the live `MEM_RESERVE`/`mmap` region with a
//!   well-formed (page-aligned, in-order) offset pair — `VirtualFree` on
//!   Windows genuinely refuses this (`ERROR_INVALID_ADDRESS`, verified on
//!   this task's own Windows host: `Refused(VmemError { os_code: Some(487) })`),
//!   and Linux `madvise` on an unmapped address is equally documented to
//!   return `EINVAL`/`ENOMEM`. **Runs on Windows ONLY** —
//!   `#[cfg(all(windows, not(miri), not(aligned_vmem_mock)))]`, task #1199.
//!   The Unix half of the claim above was reasoned from `man 2 madvise` and
//!   never executed until commit `dff7b1d` reached CI, where the same Linux
//!   runner passed it twice and failed it once across three feature rows:
//!   `madvise` refuses an unmapped range as documented, but the test's
//!   "far outside" address is an arithmetic guess (`base + 64 MiB`) whose
//!   unmappedness is a property of process layout, not of the OS. So
//!   `Refused` has NO coverage on Unix, and this file does not pretend
//!   otherwise — see the test's own doc for all three exclusion reasons.
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
//!   (task #1192) closes the gap the other three left: before this test, the
//!   free `try_decommit`'s `start == end` short-circuit
//!   (`api/decommit.rs:387-389`) had NO test coverage at all — `Skipped` was
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
/// on the FREE `try_decommit` function — the short-circuit at
/// `api/decommit.rs:387-389`, checked and returned before
/// `dispatch_try_decommit` is ever called. Deliberately does NOT require the
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

/// `DecommitOutcome::Refused`: a genuinely-issued backend call that the OS
/// refuses.
///
/// Reached through the free `try_decommit`, which — unlike
/// `Reservation::try_decommit` — has no `end > self.len()` bounds check (its
/// `# Safety` contract places that obligation on the caller instead), so a
/// well-formed (page-aligned, `start <= end`) range far outside the live
/// reservation's actual span reaches the real backend and is refused by the
/// OS: `VirtualFree(MEM_DECOMMIT)` on an address outside any
/// `MEM_RESERVE`/`MEM_COMMIT` region returns 0 with `GetLastError() ==
/// ERROR_INVALID_ADDRESS` (487) on Windows (verified empirically on this
/// task's Windows development host, see the module doc above); Linux
/// `madvise(2)` documents `ENOMEM`/`EINVAL` for an address range not mapped
/// by the calling process.
///
/// **What would make this fail if `dispatch_try_decommit` regressed:** if
/// `libc_madvise`/`winapi_virtual_decommit` reverted to discarding the
/// syscall's return value (the pre-#1180 behavior this task removes), this
/// test would observe `Advised` instead of `Refused` and fail — it is a
/// direct counterfactual on the exact defect (return-value discard) this
/// task's brief describes.
///
/// # Why this runs on Windows ONLY (task #1199)
///
/// Three separate reasons, each independently sufficient — recorded together
/// because the first fix attempt (task #1197) named only the first and `main`
/// went red on the other two the moment it reached CI:
///
/// 1. **`--cfg aligned_vmem_mock`** — the mock arm of `dispatch_try_decommit`
///    (`src/api/decommit.rs`) records the call and returns `Advised`
///    unconditionally, by deliberate design. No syscall, so no refusal to
///    observe; `Refused` is unreachable by construction.
/// 2. **miri** — `src/os/miri.rs` is a third backend with no real OS, exactly
///    like the mock. Task #1197 gated on `aligned_vmem_mock` alone, i.e. it
///    enumerated ONE cfg instead of expressing the real condition, and miri
///    failed on the very next CI run.
/// 3. **Unix, the substantive one** — the address this test declares "far
///    outside the reservation" is an ARITHMETIC GUESS (`base + 64 MiB`), and
///    whether anything is mapped there is a property of the process's address
///    space layout, not of the OS contract. On `dff7b1d`'s `test workspace
///    members` job the SAME Linux runner passed it twice and failed it once:
///    ok under the default row (3 tests, log line 406), ok under
///    `--all-features` (4 tests, line 1027), FAILED under
///    `--features "fault-injection lazy-commit"` (3 tests, line 1611).
///    **The mechanism is NOT "one fewer test in the binary"** (task #1203
///    corrected that): the default row compiled the SAME three test
///    functions as the failing row and passed, so a test-count difference is
///    held constant across a pass and a fail and cannot be the cause. What
///    differs is the sequence of allocations each row performs before
///    reaching this test, hence where `base` lands and whether anything is
///    mapped 64 MiB past it. Linux `madvise` does refuse an unmapped range
///    as documented; the premise that fails is "base + 64 MiB is unmapped".
///    So on Unix this test is non-deterministic BY CONSTRUCTION, and a flaky
///    test is worse than an absent one (tasks #1030 and #1063 both cost a
///    round to flakiness).
///
/// **Consequence, stated so it is not mistaken for coverage:** the `Refused`
/// variant has NO test coverage on Unix. Filed as an open item rather than
/// papered over — a deterministic Unix refusal (e.g. reserve, release, then
/// `madvise` the just-released address under the serial lock) is plausible but
/// unverifiable from this project's Windows development host, so it is future
/// work with an owner, not a change made blind.
///
/// The gate is a `#[cfg]`, not a runtime early-return, so the test cannot
/// silently pass-by-doing-nothing on the platforms it does not cover.
#[test]
#[cfg(all(windows, not(miri), not(aligned_vmem_mock)))]
fn refused_variant_is_produced_by_a_genuine_os_refusal() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let r = reserve_aligned(SPAN, SPAN).expect("reserve 2 MiB");
    let ps = page_size();

    // Far outside the live reservation's real span (SPAN == 2 MiB); still a
    // well-formed range by the free function's own contract (start <= end,
    // both page-aligned) -- the free `try_decommit` has no bounds check to
    // reject it before it reaches the real backend.
    let far_start = 64 * 1024 * 1024;
    let far_end = far_start + ps;

    // SAFETY: this deliberately VIOLATES `try_decommit`'s `# Safety` contract
    // ("[base+start, base+end) must lie within its usable span") to observe
    // the OS's own refusal of an out-of-region decommit -- exactly the
    // "well-formed range, backend called, OS declines" case `Refused` exists
    // to report. No out-of-bounds MEMORY ACCESS occurs: `VirtualFree`/
    // `madvise` only touch kernel page-table state for the given address
    // range and return a failure code for a range outside any mapping this
    // process owns; they do not dereference the address. `r` is kept alive
    // for the duration (not dropped early), so the reservation's own valid
    // region is never disturbed by this call.
    let out = unsafe { try_decommit(r.as_ptr(), far_start, far_end) };
    match out {
        Ok(DecommitOutcome::Refused(e)) => {
            assert!(
                !e.is_invalid_argument(),
                "a backend refusal must carry an OS-side cause, not \
                 invalid_argument (which would mean the call never reached \
                 the backend at all)"
            );
        }
        other => panic!(
            "expected Ok(DecommitOutcome::Refused(_)) for a well-formed \
             range far outside the reservation's real span, got {other:?}"
        ),
    }
}
