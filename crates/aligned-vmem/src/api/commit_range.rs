use crate::error::VmemError;
#[cfg(all(feature = "fault-injection", not(aligned_vmem_mock)))]
use crate::fault_injection;
#[cfg(aligned_vmem_mock)]
use crate::mock;
#[cfg(not(aligned_vmem_mock))]
use crate::os::commit_range_impl;
use crate::page_size::page_size;

/// Commit pages `[base + start, base + end)` within an existing reservation.
///
/// This is the incremental-commit building block: after a
/// [`reserve_aligned_lazy`](crate::api::reserve_aligned_lazy) call that left some pages reserved-but-uncommitted,
/// `commit_range` commits exactly the requested sub-range so it becomes
/// writable. On Windows this issues `VirtualAlloc(MEM_COMMIT)`; on Unix and
/// under miri the pages are already accessible, so this is a no-op that always
/// returns `true`.
///
/// `start` and `end` must be multiples of the runtime page size ([`page_size()`])
/// with `start <= end`. A well-formed no-op (empty range, `start == end`)
/// returns `true`; any other contract violation (misaligned, or `start > end`)
/// returns `false` (task #712: an earlier version of this function clamped a
/// contract violation to the WRITE-PERMITTING `true` sentinel, which already
/// caused a real crash — see
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
/// for the incident this class of bug produces on Windows).
///
/// Returns `true` if the range is now committed, `false` if the OS refused
/// (commit-charge exhaustion / true OOM) OR the offsets violated the contract
/// above. On `false` the caller MUST NOT write into the range. Never panics.
/// For the cause use [`try_commit_range`].
///
/// # Difference from [`recommit`](crate::api::recommit)
///
/// [`recommit`](crate::api::recommit) re-commits pages that were PREVIOUSLY committed and then
/// decommitted via [`decommit`](crate::api::decommit). `commit_range` commits pages that were NEVER
/// committed (reserved via the lazy path). The underlying Windows syscall is
/// the same; the semantic intent differs.
///
/// # Safety
///
/// `base` must be the [`as_ptr`](crate::Reservation::as_ptr) of a live reservation,
/// and `[base+start, base+end)` must fall within that reservation's usable span
/// (i.e. `end <= len`). The range must be currently reserved but not yet
/// committed (or already committed — recommitting is harmless on Windows).
///
/// **Concurrent calls are safe** (task #776, F14): multiple threads may call
/// `commit_range` concurrently on ranges within the SAME reservation, whether
/// the ranges overlap or not — `VirtualAlloc(MEM_COMMIT)` (Windows) is itself
/// thread-safe and idempotent, and the Unix/miri backends are no-ops (the
/// entire span is already committed eagerly on those platforms). This does
/// NOT relax the range/liveness contract above; it only states that issuing
/// several legal calls from different threads at once is not itself a new
/// hazard.
#[must_use]
#[cfg(feature = "lazy-commit")]
#[cfg_attr(docsrs, doc(cfg(feature = "lazy-commit")))]
pub unsafe fn commit_range(base: *mut u8, start: usize, end: usize) -> bool {
    // SAFETY: forwarded from the caller's contract.
    unsafe { try_commit_range(base, start, end).is_ok() }
}

/// Fallible [`commit_range`]: `Ok(())` on success (or was a well-formed no-op),
/// `Err(VmemError::invalid_argument())` if the offsets violated the contract
/// (misaligned, or `start > end`), `Err(VmemError)` carrying the OS cause on
/// genuine commit failure.
///
/// # Safety
///
/// Same as [`commit_range`].
#[cfg(feature = "lazy-commit")]
#[cfg_attr(docsrs, doc(cfg(feature = "lazy-commit")))]
pub unsafe fn try_commit_range(base: *mut u8, start: usize, end: usize) -> Result<(), VmemError> {
    let ps = page_size();
    if start > end || !start.is_multiple_of(ps) || !end.is_multiple_of(ps) {
        return Err(VmemError::invalid_argument());
    }
    if start == end {
        return Ok(());
    }
    #[cfg(aligned_vmem_mock)]
    {
        mock::record(mock::Call::CommitRange {
            base: base.addr(),
            start,
            end,
        });
        mock::take_commit_fault().map_or(Ok(()), Err)
    }
    #[cfg(not(aligned_vmem_mock))]
    {
        // Real-path fault injection (feature `fault-injection`, DISTINCT from
        // `mock`): consult the armed hooks immediately before the real
        // syscall. When neither hook is armed this is two relaxed loads that
        // branch-predict not-taken — negligible on the production path, and
        // compiled out entirely when the feature is off.
        #[cfg(feature = "fault-injection")]
        if fault_injection::should_fail_commit() {
            // task #713: this is a SIMULATED failure — no real syscall ran,
            // so `VmemError::last_os_error()` would read whatever `errno`/
            // `GetLastError` happens to be lying around from unrelated prior
            // code, not a cause tied to this call at all.
            // `os_refusal_unknown_code()` states plainly that the OS refused
            // with no (real) code to report, instead of manufacturing a
            // misleading one.
            return Err(VmemError::os_refusal_unknown_code());
        }
        // SAFETY: forwarded from the caller's contract.
        unsafe { commit_range_impl(base, start, end) }
    }
}
