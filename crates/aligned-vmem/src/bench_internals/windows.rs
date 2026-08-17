use core::sync::atomic::{AtomicU64, Ordering};

/// `bench-internals`: number of successful `win_reserve_commit` calls that took the
/// single-call fast path (Windows only — always 0 on Unix/miri; that internal helper
/// is private and platform-gated, so it is named here in code font rather than linked).
/// Each call issues 1 syscall (`VirtualAlloc(MEM_RESERVE | MEM_COMMIT)`),
/// which the fast path uses when `align <= WIN_ALLOCATION_GRANULARITY` and `commit_len == size`.
/// When a large-page request fails, a best-effort retry with ordinary pages
/// issues a second syscall but is still counted as 1 in this counter — see the
/// retry fallback code in `win_reserve_commit`. See the module-level
/// "bench-internals" section doc above.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static WINDOWS_RESERVE_COMMIT_SINGLE_CALLS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of successful `win_reserve_commit` calls that took the
/// two-call path (Windows only — always 0 on Unix/miri; that internal helper
/// is private and platform-gated, so it is named here in code font rather than linked).
/// Each call issues exactly 2 syscalls
/// (`VirtualAlloc(MEM_RESERVE)` + `VirtualAlloc(MEM_COMMIT)`). This path is used when
/// `align > WIN_ALLOCATION_GRANULARITY` or when `commit_len != size` (the lazy-commit case).
/// NOTE: this path never requests large pages (always plain MEM_COMMIT), so there is no
/// best-effort retry on failure — only the single-call fast path has that logic
/// (task #921/V-7, which is where that code comment's own attribution points).
/// See the module-level
/// "bench-internals" section doc above.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of speculative large-page `VirtualAlloc` attempts that
/// fell back to ordinary pages and then still failed, incurring syscall cost without
/// successful fast-path completion. This counts ONLY the case where the initial
/// large-page attempt failed AND the ordinary-page retry ALSO failed (both syscalls
/// returned NULL). Alignment failures on success are tracked separately by
/// `WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES`.
///
/// This counter measures the syscall overhead of the speculative large-page retry
/// logic that `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` does NOT capture — those
/// existing counters only increment on successful fast-path completion, not on
/// the failed attempts that still paid syscall cost. A high ratio of this counter
/// to `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` indicates the speculative large-page
/// window may be too wide and should be narrowed after measuring real-world
/// alignment/privilege/size distributions on target Windows workloads.
/// See docs/perf/OPEN_ITEMS.md for the design note tracking this measurement gap.
/// Added by R4-5 (docs/reviews/2026-08-16-aligned-vmem-independent-prerelease-audit-r4.md).
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static WINDOWS_LARGE_PAGE_RETRY_FAILURES: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of `VirtualFree(..., MEM_DECOMMIT)` calls that
/// failed (Windows only — always 0 on Unix/miri). `VirtualFree(MEM_DECOMMIT)`
/// failures indicate a backend bookkeeping problem that can turn into a
/// silent leak/un-freed RSS with zero visibility. The crate's public API is
/// infallible (by design), so failures are currently silently ignored; this
/// counter provides at least diagnostic visibility into the failure rate.
/// Added to address the finding documented in
/// `docs/reviews/2026-08-16-aligned-vmem-fxx-prerelease-audit.md` item P2-6.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of `VirtualFree(..., MEM_RELEASE)` calls that
/// failed (Windows only — always 0 on Unix/miri). `VirtualFree(MEM_RELEASE)`
/// failures indicate a backend bookkeeping problem that can turn into a
/// silent leak/un-freed RSS with zero visibility. For correctly created internal
/// reservations, a release failure usually indicates a bookkeeping defect. For
/// `unsafe from_raw_parts` and external allocator handoff, a failure turns into a
/// silent leak at Drop. The crate's public API is infallible (by design), so
/// failures are currently silently ignored; this counter provides at least
/// diagnostic visibility into the failure rate. Added to address the finding
/// documented in `docs/reviews/2026-08-16-aligned-vmem-independent-prerelease-audit-r4.md`
/// item R4-7.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static WINDOWS_VIRTUALFREE_RELEASE_FAILURES: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of `VirtualFree(MEM_DECOMMIT)` attempts made.
/// Denominator for [`WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES`]. Mirrors the Unix
/// `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` pair pattern — lets tests
/// distinguish "genuinely succeeded" from "never attempted".
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: relaxed snapshot of the sum of the internal
/// `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` and
/// `WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS` counters (both paths combined;
/// private storage, this accessor is the public read surface).
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_reserve_commit_calls() -> u64 {
    WINDOWS_RESERVE_COMMIT_SINGLE_CALLS.load(Ordering::Relaxed)
        + WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of the internal
/// `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` counter (single-call path count;
/// private storage, this accessor is the public read surface).
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_reserve_commit_single_calls() -> u64 {
    WINDOWS_RESERVE_COMMIT_SINGLE_CALLS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of the internal
/// `WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS` counter (two-call path count;
/// private storage, this accessor is the public read surface).
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_reserve_commit_two_call_pairs() -> u64 {
    WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of the internal
/// `WINDOWS_LARGE_PAGE_RETRY_FAILURES` counter (speculative large-page
/// attempt failures that incurred syscall cost without successful fast-path
/// completion; private storage, this accessor is the public read surface).
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_large_page_retry_failures() -> u64 {
    WINDOWS_LARGE_PAGE_RETRY_FAILURES.load(Ordering::Relaxed)
}

/// `bench-internals`: number of fast-path attempts that requested large pages
/// (by passing `MEM_LARGE_PAGES` in `extra_commit_flags`) but the returned base
/// address was not aligned to the requested `align`, forcing a `VirtualFree`
/// and fallthrough to the two-call path.
///
/// This is EXPECTED and NOT a malfunction when `WIN_ALLOCATION_GRANULARITY <
/// align <= GetLargePageMinimum()` (the threshold the fast path uses when
/// `extra_commit_flags != 0`). `VirtualAlloc(NULL, ...)` guarantees alignment
/// only to the 64 KiB granularity, not to arbitrary larger alignments. The
/// counter grows on ordinary Windows machines without `SeLockMemoryPrivilege`
/// because the fast path accepts such alignments when large pages are requested.
/// A nonzero value is diagnostic ONLY when it appears for alignments at or
/// below `WIN_ALLOCATION_GRANULARITY` (which the kernel SHOULD honor), or when
/// the caller DOES have `SeLockMemoryPrivilege` (where large pages are actually
/// granted and SHOULD respect the alignment guarantee).
///
/// Distinct from `WINDOWS_LARGE_PAGE_RETRY_FAILURES`, which tracks failures
/// where both the initial large-page attempt AND the ordinary-page retry returned
/// NULL (allocation refused).
/// Added by R5-4 (docs/reviews/2026-08-16-aligned-vmem-prerelease-audit-r5.md).
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of large-page `VirtualAlloc` attempts that returned NULL,
/// but the unconditional retry with ordinary pages succeeded.
///
/// This counter tracks the "huge failed, plain succeeded" scenario, which is NOT
/// visible from `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` (which counts successful
/// logical completion regardless of which attempt succeeded) or
/// `WINDOWS_LARGE_PAGE_RETRY_FAILURES` (which counts only when BOTH attempts failed).
/// A high ratio of this counter to `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` indicates
/// that the large-page request window is wide relative to actual privilege/size
/// conditions, incurring an extra syscall on the cold path without benefit.
/// Added by R7-3 (docs/reviews/2026-08-16-aligned-vmem-prerelease-audit-r7.md).
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static WINDOWS_LARGE_PAGE_PLAIN_FALLBACK_SUCCESSES: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: relaxed snapshot of the internal
/// `WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES` counter (fast-path attempts that
/// requested large pages but the returned base was not aligned to the
/// requested `align`; private storage, this accessor is the public read
/// surface). See the static's doc for diagnostic interpretation (nonzero is
/// expected for `WIN_ALLOCATION_GRANULARITY < align <= GetLargePageMinimum()`
/// on machines without `SeLockMemoryPrivilege`). Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_large_page_alignment_failures() -> u64 {
    WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of the internal
/// `WINDOWS_LARGE_PAGE_PLAIN_FALLBACK_SUCCESSES` counter (large-page attempts
/// that failed, but the retry with ordinary pages succeeded; private storage,
/// this accessor is the public read surface). Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_large_page_plain_fallback_successes() -> u64 {
    WINDOWS_LARGE_PAGE_PLAIN_FALLBACK_SUCCESSES.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of `WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS`.
/// Denominator for `windows_virtualfree_decommit_failures()`. Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_virtualfree_decommit_attempts() -> u64 {
    WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of `WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES`.
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_virtualfree_decommit_failures() -> u64 {
    WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of `WINDOWS_VIRTUALFREE_RELEASE_FAILURES`.
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn windows_virtualfree_release_failures() -> u64 {
    WINDOWS_VIRTUALFREE_RELEASE_FAILURES.load(Ordering::Relaxed)
}
