use core::sync::atomic::{AtomicU64, Ordering};

/// `bench-internals`: total number of exact-size `mmap` attempts across this
/// crate's two distinct exact-size fast paths, sharing one counter for unified
/// hit-rate tracking:
/// - `try_reserve_aligned_exact` (32-bit Unix only — always 0 on Windows/miri
///   AND on 64-bit Unix outside the huge-page case below, since task #944's
///   finding P-1 gated that internal helper to `target_pointer_width = "32"`;
///   it is private and platform-gated, so it is named here in code font
///   rather than linked).
/// - The Linux/Android huge-page exact-size path (II-4, 2026-08-16 audit
///   finding), which uses an exact-size `mmap` with `MAP_HUGETLB` when
///   `align == LINUX_HUGE_PAGE_SIZE` (2 MiB) — this one DOES fire on 64-bit
///   Linux/Android, since the huge-page path is not gated to 32-bit. It lives
///   in the exact-size huge-path block in `unix_reserve`, a different code
///   path from `try_reserve_aligned_exact`, but is counted here rather than
///   under a separate static.
///
/// This counter increments BEFORE the `mmap` call, so it includes both alignment
/// misses and OS-level failures (e.g., OOM, MAP_HUGETLB refused). The hit-rate
/// ratio `UNIX_EXACT_RESERVE_HITS / UNIX_EXACT_RESERVE_ATTEMPTS` therefore
/// measures the combined success rate of both alignment and OS availability.
///
/// Note that the denominator conflates two different failure kinds with
/// different syscall costs: an alignment miss costs 3 syscalls (mmap + munmap +
/// mmap for the over-reserve), while an OS refusal costs only 1 (the initial
/// mmap fails, no munmap runs afterward). Denominator for
/// [`UNIX_EXACT_RESERVE_HITS`]. See the module-level "bench-internals"
/// section doc above.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static UNIX_EXACT_RESERVE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of exact-size `mmap` attempts (counted in
/// [`UNIX_EXACT_RESERVE_ATTEMPTS`]) that succeeded -- the `mmap` landed
/// already `align`-aligned, so no over-reserve fallback was needed. Covers
/// both source paths that static's doc describes: `try_reserve_aligned_exact`
/// (32-bit Unix only) and the Linux/Android huge-page exact-size path (II-4,
/// 2026-08-16 audit finding, `align == LINUX_HUGE_PAGE_SIZE`, fires on
/// 64-bit Linux/Android too).
///
/// Numerator over [`UNIX_EXACT_RESERVE_ATTEMPTS`]. See the module-level
/// "bench-internals" section doc above.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static UNIX_EXACT_RESERVE_HITS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: total number of `madvise(2)` calls issued by
/// `libc_madvise` (Unix only — always 0 on Windows/miri; that internal
/// helper is private, so it is named here in code font rather than linked).
/// Covers every `madvise` call site reachable through `decommit_pages_impl`
/// (both `DecommitKind::Eager` — `MADV_DONTNEED` — and
/// `DecommitKind::Lazy` — `MADV_FREE`/`MADV_FREE_REUSABLE`/`MADV_DONTNEED`
/// fallback). Added by task #882 as the empirical oracle for
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
/// item 48: `libc_madvise` itself discards
/// `madvise`'s return value by design (task #719 — a failure there is not a
/// memory-safety concern), so nothing else in the crate can currently tell
/// apart "the syscall itself failed" from "the syscall succeeded but Darwin's
/// advisory-only `MADV_DONTNEED` semantics did not reclaim the pages" — the
/// two competing root-cause hypotheses for item 48's macOS zero-fill gap.
/// Denominator for [`UNIX_MADVISE_SUCCESSES`].
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static UNIX_MADVISE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of [`UNIX_MADVISE_ATTEMPTS`] that returned `0`
/// (success) from `madvise(2)`, as opposed to `-1` (failure, `errno` set).
/// See [`UNIX_MADVISE_ATTEMPTS`]'s doc for the root-cause question this
/// settles. Numerator over [`UNIX_MADVISE_ATTEMPTS`].
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static UNIX_MADVISE_SUCCESSES: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: number of `munmap` calls that failed (Unix only —
/// always 0 on Windows/miri). `munmap` failures indicate a backend
/// bookkeeping problem that can turn into a silent leak/un-freed RSS
/// with zero visibility. The crate's public API is infallible (by design),
/// so failures are currently silently ignored; this counter provides at
/// least diagnostic visibility into the failure rate. Added to address
/// the finding documented in `docs/reviews/2026-08-16-aligned-vmem-fxx-prerelease-audit.md`
/// item P2-6.
///
/// Denominator for [`UNIX_MUNMAP_FAILURES`] itself, in the sense that a
/// `munmap` call is either counted here as an attempt and THEN, iff it
/// failed, ALSO counted in that static -- see this static's own doc
/// (`UNIX_MUNMAP_ATTEMPTS`, immediately below) for why a failures-only
/// counter cannot by itself distinguish "the call was attempted and
/// succeeded" from "the call site was removed and never ran at all".
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static UNIX_MUNMAP_FAILURES: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: total number of `munmap` calls attempted (Unix only —
/// always 0 on Windows/miri), regardless of outcome. Denominator for
/// [`UNIX_MUNMAP_FAILURES`], mirroring the existing
/// `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` and
/// `WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS`/`_FAILURES` attempt/outcome pairs
/// in this module.
///
/// Added (task #1189, coverage gap C2 from
/// `docs/reviews/2026-08-19-2148-aligned-vmem-publication-audit-Сол-кодекс.md`):
/// before this counter existed, `UNIX_MUNMAP_FAILURES` alone could not
/// distinguish "the real `Reservation::release`/`Drop` path called `munmap`
/// and it succeeded" from "the call site was deleted (or never reached) and
/// `munmap` was never invoked at all" -- both leave the failures counter at
/// zero. A test asserting only `unix_munmap_failures() == 0` around a
/// `Drop` is therefore VACUOUS against that regression: it cannot fail if
/// the release call disappears. Pairing this attempts counter with the
/// existing failures counter closes that gap -- a test can now assert
/// `unix_munmap_attempts()` increased by exactly the expected call count
/// (attempt/success oracle), not just that the failure count stayed at
/// zero.
///
/// `libc_munmap` (this counter's sole increment site) is called from
/// several places in `crate::os::unix` -- the real release path
/// (`release_reservation`, reached from `Reservation::Drop` and the free
/// `release`/`try_release` functions) AND internal cleanup paths inside
/// `unix_reserve`/`try_reserve_aligned_exact`/`libc_mmap` (e.g. releasing a
/// speculative huge-page mapping that missed its alignment guarantee, or an
/// address-zero grant). A caller isolating this counter around one
/// `Drop`/`release` call must ensure no concurrent reservation/release
/// activity runs in the same window (e.g. via a serializing test mutex) for
/// the delta to attribute cleanly to that one call, exactly as the existing
/// `UNIX_MADVISE_ATTEMPTS` doc already notes for `madvise`.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub(crate) static UNIX_MUNMAP_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// `bench-internals`: relaxed snapshot of the internal
/// `UNIX_EXACT_RESERVE_ATTEMPTS` counter (private storage; this accessor is
/// the public read surface). Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn unix_exact_reserve_attempts() -> u64 {
    UNIX_EXACT_RESERVE_ATTEMPTS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of the internal
/// `UNIX_EXACT_RESERVE_HITS` counter (private storage; this accessor is the
/// public read surface). Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn unix_exact_reserve_hits() -> u64 {
    UNIX_EXACT_RESERVE_HITS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of the internal `UNIX_MADVISE_ATTEMPTS`
/// counter (private storage; this accessor is the public read surface).
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn unix_madvise_attempts() -> u64 {
    UNIX_MADVISE_ATTEMPTS.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of the internal `UNIX_MADVISE_SUCCESSES`
/// counter (private storage; this accessor is the public read surface).
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn unix_madvise_successes() -> u64 {
    UNIX_MADVISE_SUCCESSES.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of `UNIX_MUNMAP_FAILURES`.
/// Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn unix_munmap_failures() -> u64 {
    UNIX_MUNMAP_FAILURES.load(Ordering::Relaxed)
}

/// `bench-internals`: relaxed snapshot of `UNIX_MUNMAP_ATTEMPTS`. Denominator
/// for [`unix_munmap_failures`]; see that static's own doc for the
/// attempt/success oracle this pairing enables. Diagnostic only.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[must_use]
pub fn unix_munmap_attempts() -> u64 {
    UNIX_MUNMAP_ATTEMPTS.load(Ordering::Relaxed)
}
