use crate::error::VmemError;
#[cfg(aligned_vmem_mock)]
use crate::mock;
use crate::os::reserve_aligned_huge_raw;
use crate::Reservation;

use super::internal::{finish_reservation_huge, validate_size_align};

/// Reserve `size` bytes aligned to `align`, requesting OS **large / huge
/// pages** (Linux `MAP_HUGETLB`, Windows `MEM_LARGE_PAGES`).
/// Currently a **no-op on macOS and other non-Linux Unix** — it falls back to
/// an ordinary reservation, identical to [`reserve_aligned`](crate::api::reserve_aligned).
///
/// **Transparent-huge-page hinting (Linux `MADV_HUGEPAGE`) is not used:** it
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
/// **except on Linux with `huge-pages` enabled**: `size` and `align` must BOTH
/// additionally be multiples of the Linux huge-page size (2 MiB) — a request
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
/// **Decommit incompatibility:** on both Windows and Linux, [`decommit`](crate::api::decommit) and
/// [`decommit_lazy`](crate::api::decommit_lazy) **do not work** on huge-page reservations. On Windows,
/// `VirtualFree` with `MEM_DECOMMIT` fails on large-page regions. On Linux,
/// `MADV_DONTNEED`/`MADV_FREE` on a `MAP_HUGETLB` mapping is accepted only at
/// huge-page granularity, so any [`page_size()`](crate::page_size::page_size)-granular offset gets `EINVAL`
/// and does nothing. The behavior is therefore indistinguishable from a silent
/// no-op: the caller's RSS does not decrease, and subsequent reads return the
/// old (stale) data rather than zeroed pages. Use [`reserve_aligned`](crate::api::reserve_aligned) instead
/// if you need decommit functionality.
// Historical notes (task #776, #714, #848, #843):
//
// - task #776, F3: Linux huge-page request additionally requires both size
//   and align to be multiples of the Linux huge-page size (2 MiB), rejecting
//   PAGE-multiple requests that `reserve_aligned` accepts. This was added to
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
//   platform (Windows: VirtualFree fails; Linux: MADV_DONTNEED/MADV_FREE
//   requires huge-page granularity).
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
