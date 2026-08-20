use crate::error::VmemError;

/// The observed result of one [`try_decommit`](crate::try_decommit) /
/// [`Reservation::try_decommit`](crate::Reservation::try_decommit) call —
/// **task #1180 (PUB-R2 phase 2)**, replacing the pre-#1180
/// `Result<(), VmemError>` return, which reported only whether the
/// CALLER'S ARGUMENTS were valid, never what the OS actually did (or was
/// even asked to do).
///
/// The outer `Result<DecommitOutcome, VmemError>` still reports a caller
/// contract violation exactly as before (`Err(VmemError::invalid_argument())`
/// for a malformed range, `Err(VmemError::os_refusal_unknown_code())` if the
/// one-time OS page-size query failed) — see
/// [`try_decommit`](crate::try_decommit)'s own `# Errors` section. What is
/// new is the `Ok` payload: three variants that distinguish "nothing was
/// asked of the OS" from "the OS was asked and refused" from "the OS was
/// asked and accepted", where the pre-#1180 signature collapsed all three
/// into the same `Ok(())`.
///
/// **None of the three variants is a claim about physical memory having
/// actually been reclaimed.** Decommit is best-effort by nature (see
/// [`Reservation::decommit_reclaims_and_zeroes`](crate::Reservation::decommit_reclaims_and_zeroes)
/// for which platforms guarantee reclaim+zero-fill at all) — this type
/// answers "what did this call do", not "what did it accomplish".
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecommitOutcome {
    /// No backend call was made — a Rust-level skip, decided before any
    /// syscall. Two sources, both a well-formed range:
    ///
    /// - **An empty range** (`start == end`), on either
    ///   [`try_decommit`](crate::try_decommit) or
    ///   [`Reservation::try_decommit`](crate::Reservation::try_decommit) — a
    ///   deliberate no-op, checked before any huge-page eligibility question
    ///   even applies.
    /// - **A well-formed, in-span, non-empty range on a huge-page
    ///   reservation** ([`Reservation::is_huge`](crate::Reservation::is_huge)
    ///   == `true`) that does not take the Linux/Android kernel >= 5.18
    ///   huge-aligned real-backend path — see
    ///   [`Reservation::decommit`](crate::Reservation::decommit)'s "Huge-page
    ///   granularity" doc for the exact eligibility split (Windows: always;
    ///   Linux/Android: only a range that is NOT huge-page-size-aligned at
    ///   both endpoints, or when the `huge-pages` feature is off). Only
    ///   [`Reservation::try_decommit`](crate::Reservation::try_decommit) has
    ///   an [`is_huge()`](crate::Reservation::is_huge) to consult, so this
    ///   second source is exclusive to it — the free
    ///   [`try_decommit`](crate::try_decommit) function has no such
    ///   eligibility check and, for a non-empty range, always forwards to the
    ///   backend.
    Skipped,
    /// The backend call was made and the kernel/OS **accepted** it — Linux
    /// `madvise(2)` returned `0`, or Windows `VirtualFree(MEM_DECOMMIT)`
    /// returned nonzero (success).
    ///
    /// **Does NOT mean the physical pages were actually returned to the
    /// OS**, let alone that a subsequent access re-faults zeroed memory —
    /// that gap between "the kernel accepted the advice" and "the kernel
    /// acted on the advice as this crate's docs describe" is exactly what
    /// [`Reservation::decommit_reclaims_and_zeroes`](crate::Reservation::decommit_reclaims_and_zeroes)
    /// and this type's own module-level doc already qualify, and remains
    /// open per task #1174 — this variant does not close it, and must not
    /// be read as though it does. On Darwin/the four BSDs, `MADV_DONTNEED`
    /// is well known to return `0` while the pages stay resident (advisory
    /// semantics) — `Advised` there is expected and unremarkable, not a
    /// stronger signal than the platform actually gives.
    Advised,
    /// The backend call was made and the kernel/OS **refused** it — Linux/Android
    /// `madvise(2)` returned `-1` (e.g. `EINVAL` on a pre-5.18 kernel
    /// receiving a HugeTLB range, or any other kernel-side rejection), or
    /// Windows `VirtualFree(MEM_DECOMMIT)` returned zero (failure, e.g.
    /// `GetLastError()` on a large-page region). Carries
    /// [`VmemError::last_os_error`] captured immediately after the failing
    /// call, same capture-timing contract as every other OS-refusal error in
    /// this crate.
    Refused(VmemError),
}

impl DecommitOutcome {
    /// `true` for [`DecommitOutcome::Skipped`].
    #[must_use]
    #[inline]
    pub const fn is_skipped(&self) -> bool {
        matches!(self, DecommitOutcome::Skipped)
    }

    /// `true` for [`DecommitOutcome::Advised`].
    #[must_use]
    #[inline]
    pub const fn is_advised(&self) -> bool {
        matches!(self, DecommitOutcome::Advised)
    }

    /// `true` for [`DecommitOutcome::Refused`].
    #[must_use]
    #[inline]
    pub const fn is_refused(&self) -> bool {
        matches!(self, DecommitOutcome::Refused(_))
    }
}
