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
//!   return `EINVAL`/`ENOMEM`. This is a REAL backend refusal on every
//!   platform, not a Windows-only trick — but it has only been executed on
//!   Windows during this task's own development; the Unix half is reasoned
//!   from `man 2 madvise`'s own documented `ENOMEM`/`EINVAL` cases for an
//!   address range that is not a valid part of the process's address space,
//!   not independently observed on a Unix CI run as part of THIS task.

use aligned_vmem::{page_size, reserve_aligned, try_decommit, DecommitOutcome, Reservation};

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
#[test]
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
