use crate::error::VmemError;
#[cfg(aligned_vmem_mock)]
use crate::mock;
#[cfg(not(aligned_vmem_mock))]
use crate::os::recommit_pages_impl;
use crate::page_size::page_size;

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
/// `base` must be the [`as_ptr`](crate::Reservation::as_ptr) of a live reservation
/// whose `[base+start, base+end)` range was previously decommitted.
/// `start`/`end` must be multiples of the runtime page size ([`page_size()`])
/// with `start <= end` — a violation returns `false` (task #712: an earlier
/// version of this function clamped a contract violation to the WRITE-PERMITTING
/// `true` sentinel, which already caused a real crash — see
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
/// for the incident this class of bug produces on Windows).
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
/// Same as [`recommit`].
pub unsafe fn try_recommit(base: *mut u8, start: usize, end: usize) -> Result<(), VmemError> {
    let ps = page_size();
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
