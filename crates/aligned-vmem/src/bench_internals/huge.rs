use core::sync::atomic::{AtomicU64, Ordering};

/// `bench-internals`: number of decommit calls made on huge-page reservations.
/// These calls are short-circuited immediately (no syscall is issued) because
/// decommit is incompatible with huge-page reservations on both Windows and Linux:
/// - On Windows, `VirtualFree(MEM_DECOMMIT)` fails on large-page regions.
/// - On Linux, `madvise` on a `MAP_HUGETLB` mapping only works at huge-page granularity,
///   so any [`page_size()`]-granular offset gets `EINVAL` and does nothing.
/// The counter reflects calls that hit this early-exit path. Added to address finding
/// R4-4 (docs/reviews/2026-08-16-aligned-vmem-independent-prerelease-audit-r4.md).
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
