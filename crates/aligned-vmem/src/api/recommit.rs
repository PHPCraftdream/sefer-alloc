use crate::error::VmemError;
#[cfg(aligned_vmem_mock)]
use crate::mock;
#[cfg(not(aligned_vmem_mock))]
use crate::os::recommit_pages_impl;
use crate::page_size::{page_size_or_poison, PAGE_SIZE_QUERY_FAILED};

/// Recommit pages `[base + start, base + end)` previously passed to
/// [`decommit`](crate::api::decommit). On Windows this re-commits physical pages
/// (`VirtualAlloc(MEM_COMMIT)`); on Unix re-access is implicit so this is a
/// no-op. On the Darwin family (macOS/iOS/tvOS/watchOS) specifically, whether
/// re-access reads back zeroed pages or the pre-decommit contents is not
/// guaranteed either way — see [`decommit`](crate::api::decommit)'s Darwin caveat for why.
///
/// Returns `true` if the range is now committed (or the call was a
/// well-formed no-op — an empty PAGE-ALIGNED range, `start == end`), and
/// `false` if the OS refused to
/// commit the pages (commit-charge exhaustion / true OOM) OR the offsets
/// violated the contract below. On `false` the caller MUST NOT write into
/// `[base+start, base+end)`. Never panics. For the cause use [`try_recommit`].
///
/// # Safety
///
/// - `base` must be the [`as_ptr`](crate::Reservation::as_ptr) of a live
///   reservation whose `[base+start, base+end)` range was previously
///   decommitted.
/// - **`end <= reservation.len()`** (the reservation's usable span, in
///   bytes) — this is a MANDATORY precondition of the pointer arithmetic
///   this function performs internally (`base.add(start)` in the Windows
///   backend's `recommit_pages_impl`; the Unix and miri backends are no-ops
///   but the contract is stated platform-independently), not merely a
///   functional/behavioral preference. Before task #1229/F6 this function
///   was the only range-taking free function whose `# Safety` lacked the
///   bound: [`decommit`](crate::api::decommit)'s states it in full (task
///   #1213/L2, whose wording this matches), `try_decommit` and
///   `decommit_lazy` carry it (restated in prose / by explicit
///   same-contract reference), and the
///   [`commit_range`](crate::api::commit_range) pair spells it out as
///   `end <= len`. For an `unsafe fn`, a
///   bounds requirement that determines whether pointer arithmetic is even
///   defined belongs inside `# Safety` itself, restated in full. Passing
///   `end > reservation.len()` is undefined behavior (out-of-bounds pointer
///   arithmetic), distinct from — and a strictly worse violation than —
///   the `page_size()`-multiple contract below, which merely returns
///   `false` on violation, never UB. Callers through the safe
///   [`Reservation::recommit`](crate::Reservation::recommit) /
///   [`Reservation::try_recommit`](crate::Reservation::try_recommit)
///   methods are not exposed: both bounds-check `end <= self.len()` before
///   delegating here, so the gap reaches only callers of this free
///   function directly.
/// - `start`/`end` must be multiples of the runtime page size
///   ([`page_size()`](crate::page_size)) with `start <= end` — a violation
///   returns `false` (task #712: an earlier version of this function
///   clamped a contract violation to the WRITE-PERMITTING `true` sentinel,
///   which already caused a real crash — see
///   <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
///   item 6 for the incident this class of bug produces on Windows).
#[must_use]
pub unsafe fn recommit(base: *mut u8, start: usize, end: usize) -> bool {
    // SAFETY: forwarded from the caller's contract.
    unsafe { try_recommit(base, start, end).is_ok() }
}

/// Fallible [`recommit`]: `Ok(())` if the range is now committed (or was a
/// well-formed no-op), `Err(VmemError::invalid_argument())` if the offsets
/// violated the contract (misaligned, or `start > end`), `Err(VmemError)`
/// carrying the OS cause on genuine commit failure.
///
/// # Safety
///
/// Same contract as [`recommit`], with the bound restated here rather than
/// only referenced (task #1229/F6): this function is the one that actually
/// reaches the backend — its non-mock arm calls `recommit_pages_impl`
/// directly, and [`recommit`] forwards through here — so a caller auditing
/// only this section must see it. `base` must be the
/// [`as_ptr`](crate::Reservation::as_ptr) of a live reservation whose
/// `[base+start, base+end)` range was previously decommitted, and
/// **`end <= reservation.len()`** — passing a larger `end` is undefined
/// behavior (out-of-bounds pointer arithmetic in the backend's
/// `base.add(start)`), a strictly worse violation than the
/// `page_size()`-multiple / `start <= end` contract, which merely returns
/// `Err(VmemError::invalid_argument())`, never UB.
pub unsafe fn try_recommit(base: *mut u8, start: usize, end: usize) -> Result<(), VmemError> {
    let ps = page_size_or_poison();
    // Failed OS page-size query: fail closed with the OS-side no-code error
    // (NOT `invalid_argument` — the caller's arguments are not at fault).
    // See `page_size`'s "If the one-time OS query fails" paragraph.
    if ps == PAGE_SIZE_QUERY_FAILED {
        return Err(VmemError::os_refusal_unknown_code());
    }
    if start > end || !start.is_multiple_of(ps) || !end.is_multiple_of(ps) {
        return Err(VmemError::invalid_argument());
    }
    if start == end {
        return Ok(());
    }
    #[cfg(aligned_vmem_mock)]
    {
        mock::record(mock::Call::Recommit {
            base: base.addr(),
            start,
            end,
        });
        mock::take_commit_fault().map_or(Ok(()), Err)
    }
    #[cfg(not(aligned_vmem_mock))]
    // SAFETY: forwarded from the caller's contract.
    unsafe {
        recommit_pages_impl(base, start, end)
    }
}
