use crate::error::VmemError;
#[cfg(aligned_vmem_mock)]
use crate::mock;
use crate::os::reserve_aligned_huge_raw;
use crate::Reservation;

use super::internal::{finish_reservation_huge, validate_size_align};

/// Reserve `size` bytes aligned to `align`, requesting OS **large / huge
/// pages** (Linux/Android `MAP_HUGETLB`, Windows `MEM_LARGE_PAGES`).
/// Currently a **no-op on macOS and other Unix that is neither Linux nor
/// Android** — it falls back to
/// an ordinary reservation, identical to [`reserve_aligned`](crate::api::reserve_aligned).
///
/// **Transparent-huge-page hinting (Linux/Android `MADV_HUGEPAGE`) is not used:** it
/// cannot affect an already-explicitly-huge `MAP_HUGETLB` mapping (the pages
/// are already huge), so issuing it would be a wasted syscall. This crate's
/// strategy is the explicit hugetlbfs path only.
///
/// Large pages reduce TLB pressure for big allocator segments. The request is
/// **best-effort**: if the OS refuses large pages (none configured, no
/// privilege), the reservation transparently falls back to ordinary pages, so
/// this never fails purely because huge pages are unavailable — it fails only
/// on a genuine reservation error (OOM) or a contract violation.
///
/// To detect whether huge pages were actually granted (as opposed to having
/// fallen back to ordinary pages), use the returned [`Reservation::is_huge`](crate::Reservation::is_huge)
/// method.
///
/// Base/align/size contract is otherwise identical to [`reserve_aligned`](crate::api::reserve_aligned),
/// **except on Linux AND Android with `huge-pages` enabled** (the check's
/// cfg is `any(target_os = "linux", target_os = "android")` + `feature =
/// "huge-pages"`): `size` and `align` must BOTH
/// additionally be multiples of the huge-page size (2 MiB) — a request
/// that only satisfies `reserve_aligned`'s own weaker `PAGE`-multiple contract
/// is rejected with `VmemError::invalid_argument()` before any syscall runs,
/// even though such a request could previously succeed there via the documented
/// ordinary-page fallback. For the failure cause use
/// [`try_reserve_aligned_huge`].
///
/// **Windows limitation:** on Windows, this function returns a reservation with
/// [`Reservation::is_huge`](crate::Reservation::is_huge) == `true` only when ALL of the following hold:
/// 1. The fast-path condition `align <= GetLargePageMinimum()` is satisfied
///    (typically `align <= 2 MiB` on x86_64)
/// 2. `size` is a multiple of the system's large-page minimum
/// 3. The calling process has `SeLockMemoryPrivilege` granted AND has
///    **enabled** it via `AdjustTokenPrivileges` (the crate does not do
///    this for you — a process with the privilege granted but not enabled
///    fails exactly like an unprivileged one and silently falls back to
///    ordinary pages)
///
/// NOTE: The widened fast-path condition (II-3, 2026-08-16 audit finding) expanded
/// the single-call ATTEMPT window from `align <= 64 KiB` to `align <= GetLargePageMinimum()`,
/// but on an unprivileged host the actual paths that SUCCEED (pass the post-call alignment
/// check) are typically still limited. When large pages are NOT granted (unprivileged),
/// `VirtualAlloc`'s alignment guarantee is only 64 KiB; in practice it typically does NOT
/// happen to land on the requested alignment, so the post-call check fails and the fast
/// path falls through to the two-call path. Practically, this means `is_huge() == true` only
/// for shapes where large pages are actually granted, which requires all three conditions
/// above to hold.
///
/// **Extra-syscall cost on unprivileged hosts:** For the widened align range
/// (`64 KiB < align <= GetLargePageMinimum()`), when large pages are requested but
/// not granted (e.g., unprivileged process, or `SeLockMemoryPrivilege` not enabled),
/// the code attempts `VirtualAlloc` with `MEM_LARGE_PAGES` (fails), retries without
/// it (succeeds with ordinary pages), and if that retry's base doesn't happen to
/// satisfy the requested alignment, the whole thing is released and falls through to
/// the two-call path. This means an unprivileged reservation in this align range
/// can cost up to 2 extra `VirtualAlloc` calls + 1 `VirtualFree` before reaching
/// the two-call path, versus before the II-3 change (which would have gone straight
/// to the two-call path for `align > 64 KiB`). This is a real, measurable behavior
/// change, not a correctness bug — the widening genuinely expands the single-call
/// attempt window, and unprivileged processes pay the extra-syscall cost for shapes
/// that now attempt but fail the fast path.
///
/// If any of these conditions fail, the function falls back to ordinary
/// pages and returns a reservation with [`Reservation::is_huge`](crate::Reservation::is_huge) == `false`.
/// On Windows, large pages (`MEM_LARGE_PAGES`) are only ever requested and
/// possibly granted via the single-call fast path; the two-call path never requests
/// large pages, so the result never has
/// [`Reservation::is_huge`](crate::Reservation::is_huge) == `true`.
///
/// **Decommit incompatibility (corrected task #1140):** on Windows,
/// [`decommit`](crate::api::decommit)/[`decommit_lazy`](crate::api::decommit_lazy) **never work** on huge-page
/// reservations — `VirtualFree` with `MEM_DECOMMIT` unconditionally fails on
/// large-page regions. [`decommit_lazy`](crate::api::decommit_lazy) never works on a huge-page
/// reservation on ANY platform — `MADV_FREE` (its Linux/Android backend) has
/// no documented HugeTLB support, unlike `MADV_DONTNEED` below.
///
/// [`decommit`](crate::api::decommit) on Linux/Android is more nuanced: it depends on BOTH the
/// requested range and the running kernel. `madvise(2)` documents that
/// `MADV_DONTNEED` gained HugeTLB support in Linux 5.18, requiring
/// `[base+start, base+end)` to be aligned to the mapping's huge page size (2
/// MiB) at both endpoints — the same alignment this function already requires
/// of `size`/`align` themselves on Linux/Android, so decommitting an entire
/// huge reservation, or any 2-MiB-granular sub-range of it, is exactly such an
/// eligible range on a >= 5.18 kernel. A `page_size()`-granular (e.g. 4 KiB)
/// but not 2-MiB-granular offset still gets `EINVAL` and does nothing, as does
/// EVERY range on a pre-5.18 kernel. [`Reservation::decommit`](crate::Reservation::decommit)/
/// [`Reservation::try_decommit`](crate::Reservation::try_decommit) (the safe methods) consult both
/// [`Reservation::is_huge`](crate::Reservation::is_huge) and the requested range to skip the ineligible
/// case before issuing the syscall; the free [`decommit`](crate::api::decommit) function has no
/// `is_huge()` to consult and issues the syscall unconditionally — see that
/// function's own doc for the precise split. Either way — an ineligible
/// range, or any range on a pre-5.18 kernel — the effect is indistinguishable
/// from a silent no-op: the caller's RSS does not decrease, and subsequent
/// reads return the old (stale) data rather than zeroed pages.
///
/// Documented per the `madvise(2)` man page, and — since task #1152 (F1) —
/// empirically exercised under a real hugetlb pool by this crate's own CI:
/// the `aligned-vmem-hugetlb-real` job (`.github/workflows/ci.yml`) hard-
/// asserts (via a path-activation oracle) that this function actually
/// received a `MAP_HUGETLB` grant, then drives a huge-page-eligible range
/// through [`Reservation::decommit`](crate::Reservation::decommit)'s eligible-huge branch. That
/// proves the eligible-range/post-5.18-kernel case genuinely returns
/// physical backing rather than silently no-opping. The ineligible-range
/// case (still a documented no-op) and every range on a pre-5.18 kernel
/// remain reasoned-from-spec, not independently exercised — CI's runner
/// image kernel version is not pinned by this crate.
///
/// Use [`reserve_aligned`](crate::api::reserve_aligned) instead if you need decommit to work
/// unconditionally, regardless of range shape, kernel version, or platform.
///
/// **Linux/Android hugetlb pool over-reserve (`align > 2 MiB`):** when huge pages
/// are actually granted through the over-reserve path — which is every
/// granted `align > 2 MiB` request (the exact-size fast path exists only
/// for `align == LINUX_HUGE_PAGE_SIZE`, 2 MiB), and an `align == 2 MiB`
/// request only when that fast path misses — the whole `size + align`-byte
/// `MAP_HUGETLB` mapping is kept for the reservation's lifetime, and the
/// Linux kernel reserves pool pages for a private hugetlb mapping's entire
/// length at
/// `mmap` time (no `MAP_NORESERVE` is passed). The exactly `align` bytes
/// of never-touched slack are therefore charged against the bounded
/// `nr_hugepages` pool until the reservation is released: a
/// `size == align == 4 MiB` workload consumes 4 pool pages per segment
/// for 2 needed (2×), reaching pool exhaustion — and the silent
/// ordinary-page fallback — with half the segments an exact charge would
/// allow. Workloads bounding the hugetlb pool should prefer
/// `align == 2 MiB` shapes, which the exact-size fast path serves with
/// zero over-reserve whenever it hits (and whose miss cost is at most
/// `align == 2 MiB` of slack, not `align > 2 MiB`). This cost is
/// REASONED-FROM-SPEC (documented kernel
/// reservation semantics; no hugetlb-configured host exists in this
/// crate's CI to measure it on) and, when huge pages are granted via
/// this over-reserve path, is deliberately not trimmed away: the
/// over-reserved mapping is then kept whole as one soundness-driven
/// unit (a single `munmap` at the mapping base), and the pool
/// trade-off has no measurable host available.
// Historical notes (task #776, #714, #848, #843):
//
// - task #776, F3: Linux huge-page request additionally requires both size
//   and align to be multiples of the huge-page size (2 MiB), rejecting
//   PAGE-multiple requests that `reserve_aligned` accepts. (Android joined
//   the same `any(target_os = "linux", target_os = "android")` cfg arm in
//   task #944/U-2, so this contract has been Linux/Android-common since
//   then.) This was added to
//   close a real `munmap` mapping leak (task #714); the trade-off is a
//   stricter contract in exchange for provable correctness.
//
// - task #848: Windows single-call fast path is the only
//   path that can grant large pages on Windows; the two-call path never
//   requests them. (For large-page requests, the fast-path condition is
//   `align <= GetLargePageMinimum()`, typically 2 MiB; for ordinary requests,
//   it is `align <= WIN_ALLOCATION_GRANULARITY`, 64 KiB.)
//
// - task #843, V4: decommit does not work on huge-page reservations on either
//   platform (Windows: VirtualFree fails; Linux/Android: MADV_DONTNEED/MADV_FREE
//   requires huge-page granularity).
//   SUPERSEDED by task #1140: this was true only for Windows and for
//   MADV_FREE (decommit_lazy) everywhere. On Linux/Android, MADV_DONTNEED
//   (eager decommit) now works for a 2-MiB-aligned range on kernel >= 5.18 —
//   see this function's own rustdoc above ("Decommit incompatibility
//   (corrected task #1140)") for the current, precise contract; this note
//   is kept only as a historical record of the pre-#1140 belief.
#[must_use]
#[cfg(feature = "huge-pages")]
#[cfg_attr(docsrs, doc(cfg(feature = "huge-pages")))]
pub fn reserve_aligned_huge(size: usize, align: usize) -> Option<Reservation> {
    try_reserve_aligned_huge(size, align).ok()
}

/// Fallible [`reserve_aligned_huge`].
#[cfg(feature = "huge-pages")]
#[cfg_attr(docsrs, doc(cfg(feature = "huge-pages")))]
pub fn try_reserve_aligned_huge(size: usize, align: usize) -> Result<Reservation, VmemError> {
    validate_size_align(size, align)?;
    #[cfg(aligned_vmem_mock)]
    if let Some(e) = mock::take_reserve_fault() {
        mock::record(mock::Call::ReserveHuge { size, align });
        return Err(e);
    }
    #[cfg(aligned_vmem_mock)]
    mock::record(mock::Call::ReserveHuge { size, align });

    // task #713: `reserve_aligned_huge_raw` now captures its own `VmemError`
    // immediately at the point of failure; this just propagates it.
    finish_reservation_huge(size, align, reserve_aligned_huge_raw(size, align))
}
