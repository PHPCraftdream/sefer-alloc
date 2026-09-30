use core::ptr::NonNull;
use std::alloc::{GlobalAlloc, Layout, System};

use crate::error::VmemError;
use crate::os::DecommitKind;

/// task #713: a bad `(size, align)` `Layout` combination is a caller contract
/// violation, not an OS refusal — maps to [`VmemError::invalid_argument`]. A
/// genuine `System.alloc` failure (null return) has no real
/// `errno`/`GetLastError` to read under miri; `VmemError::last_os_error()`
/// correctly yields [`VmemError::os_refusal_unknown_code`] here rather than a
/// misleading `code 0`.
#[cfg(miri)]
pub(crate) fn reserve_aligned_raw(
    size: usize,
    align: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    let layout = Layout::from_size_align(size, align).map_err(|_| VmemError::invalid_argument())?;
    // SAFETY: `layout` has non-zero size and power-of-two alignment. `System`
    // bypasses an installed global allocator, including this crate's consumer.
    let ptr = unsafe { System.alloc(layout) };
    match NonNull::new(ptr) {
        Some(base) => Ok((base, base, size)), // Never huge under miri
        None => Err(VmemError::last_os_error()),
    }
}

#[cfg(miri)]
pub(crate) unsafe fn release_reservation(
    reservation: NonNull<u8>,
    reservation_len: usize,
    align: usize,
) {
    // Constructors create this layout; raw adoption checks its shape and
    // requires the exact original layout by its unsafe caller contract.
    let layout = Layout::from_size_align(reservation_len, align).expect("release: invalid layout");
    // SAFETY: `reservation` is the original `System.alloc` pointer, still live
    // and uniquely owned, and `layout` is the exact original allocation layout.
    unsafe { System.dealloc(reservation.as_ptr(), layout) };
}

#[cfg(miri)]
// mock (task #646/F8): bypassed by the recording backend, unused when `mock`
// alone is enabled without a real decommit call site reachable.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
pub(crate) unsafe fn decommit_pages_impl(
    _base: *mut u8,
    _start: usize,
    _end: usize,
    _kind: DecommitKind,
) -> Result<(), VmemError> {
    // Miri models no RSS; decommit is a no-op that always "succeeds" (task
    // #1180, PUB-R2 phase 2: there is no real syscall under miri to refuse).
    Ok(())
}

#[cfg(miri)]
// mock (task #646/F8): see decommit_pages_impl above.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
pub(crate) unsafe fn recommit_pages_impl(
    _base: *mut u8,
    _start: usize,
    _end: usize,
) -> Result<(), VmemError> {
    Ok(())
}

#[cfg(all(miri, feature = "lazy-commit"))]
// mock (task #646/F8): `try_commit_range`'s real-path branch is compiled out
// under `mock`, so this never gets called.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
pub(crate) unsafe fn commit_range_impl(
    _base: *mut u8,
    _start: usize,
    _end: usize,
) -> Result<(), VmemError> {
    Ok(())
}

#[cfg(all(miri, feature = "lazy-commit"))]
// mock (task #646/F8): `try_reserve_aligned_lazy`'s real-path branch is
// compiled out under `mock`, so this never gets called.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
pub(crate) fn reserve_aligned_lazy_raw(
    size: usize,
    align: usize,
    _initial_commit: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    reserve_aligned_raw(size, align)
        .map(|(base, reservation, reservation_len)| (base, reservation, reservation_len))
}

#[cfg(all(miri, feature = "huge-pages"))]
pub(crate) fn reserve_aligned_huge_raw(
    size: usize,
    align: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize, bool), VmemError> {
    // Miri has no huge pages; ordinary allocation is observably identical.
    reserve_aligned_raw(size, align).map(|(base, reservation, reservation_len)| {
        (base, reservation, reservation_len, false) // Never huge under miri
    })
}
