use core::sync::atomic::{AtomicU64, Ordering};

/// `bench-internals`: number of decommit calls made on huge-page reservations
/// that were short-circuited (no syscall issued) rather than forwarded to the
/// real backend (task #1140 narrowed this from "every huge-page decommit
/// call" to just the ineligible ones — see below).
///
/// - On Windows, EVERY decommit call on a huge-page reservation hits this
///   early-exit path and increments the counter: `VirtualFree(MEM_DECOMMIT)`
///   unconditionally fails on large-page regions, so there is no eligible
///   case to forward.
/// - On Linux/Android, only an INELIGIBLE range hits this path: a range that
///   is not huge-page-size-aligned (2 MiB) at BOTH endpoints. A range that IS
///   2-MiB-aligned at both endpoints is instead forwarded to the real
///   `madvise(MADV_DONTNEED)` backend and does **not** increment this
///   counter — even on a pre-5.18 kernel, where the kernel itself rejects
///   the call with `EINVAL` after the syscall is issued. Eligibility here is
///   decided purely by the range's 2-MiB alignment (`linux_huge_range_is_madvise_eligible`,
///   `os/unix.rs`) — this crate performs no kernel-version detection, so this
///   counter cannot distinguish "forwarded and the kernel honored it" from
///   "forwarded and the kernel rejected it with `EINVAL`"; both are real
///   backend calls that skip this early-exit path and leave the counter
///   unchanged.
///
/// The counter reflects only calls that hit the early-exit path described
/// above — not every call made on a huge-page reservation. Added to address
/// finding R4-4
/// (docs/reviews/2026-08-16-aligned-vmem-independent-prerelease-audit-r4.md);
/// narrowed for task #1140.
///
/// **Relationship to `DecommitOutcome` (task #1180): does not duplicate it.**
/// This is a `bench-internals`-gated, process-wide CUMULATIVE counter read via
/// a separate accessor call — it answers "how many huge-skip calls have
/// happened so far in this process." `DecommitOutcome::Skipped`
/// (`crate::DecommitOutcome`, `Reservation::try_decommit`'s `Ok` payload) is a
/// PER-CALL return value, always available with no feature gate, answering
/// "did THIS call skip" — for the SAME early-exit path this counter tracks
/// (both fire together: a huge-skip call always increments this counter AND
/// returns `Skipped`, from the same call site in
/// `Reservation::try_decommit`/`Reservation::decommit`). Different questions,
/// different availability, no overlap to resolve. The gap this counter's own
/// doc names above it CANNOT close on its own — "cannot distinguish forwarded-
/// and-honored from forwarded-and-`EINVAL`" — IS now closed, but only for
/// `try_decommit` callers, via `DecommitOutcome::Refused` (the eligible-forward
/// case reports `Advised`/`Refused` from the real backend's own return value,
/// not from this counter, which stays unchanged by that branch either way).
/// The infallible `Reservation::decommit` still has no such signal — it
/// discards the backend's outcome exactly as before this task.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static HUGE_DECOMMIT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: relaxed snapshot of `HUGE_DECOMMIT_ATTEMPTS`.
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn huge_decommit_attempts() -> u64 {
    HUGE_DECOMMIT_ATTEMPTS.load(Ordering::Relaxed)
}
