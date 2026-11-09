// expect: anchor (before) not found
use core::ptr::NonNull;

use super::super::unix::libc_munmap;
use crate::error::VmemError;
#[cfg(feature = "fault-injection")]
use crate::fault_injection;

/// Checked (fallible) Unix release backend: report `munmap(2)`'s own outcome.
///
/// On ALL unix targets this module compiles for — Linux, Android, the Darwin
/// family, and the BSDs — success is `munmap(2)`'s own `0` return and `Err`
/// is its `-1` return with the immediately-captured `errno`. A well-formed
/// aligned mapping failing `munmap` is not an expected runtime condition; on
/// `Err` the caller must treat the mapping as NOT proven released (it may be
/// fully, partially, or not released — the states are indistinguishable
/// through this API).
///
/// # Safety
///
/// Forwarded caller contract (the same one `release_reservation` documents):
/// `reservation` must be the start of a live mmap'd region of
/// `reservation_len` bytes, being unmapped exactly once.
pub(crate) unsafe fn release_reservation_checked(
    reservation: NonNull<u8>,
    reservation_len: usize,
    _align: usize,
) -> Result<(), VmemError> {
    #[cfg(feature = "bench-internals")]
    let (reservation_addr, release_begin) = (
        reservation.as_ptr().addr(),
        None,
    );
    #[cfg(feature = "fault-injection")]
    let surrogate = fault_injection::should_fail_release_backend_result();
    #[cfg(not(feature = "fault-injection"))]
    let surrogate = false;
    let (ret, captured_error) = if surrogate {
        // Simulate a nonzero munmap result at its classification seam. No
        // syscall ran, so never read/fabricate last_os_error; mapping leaks.
        (-1, VmemError::os_refusal_unknown_code())
    } else {
        // SAFETY: forwarded live mmap reservation, released exactly once.
        let ret = unsafe { libc_munmap(reservation.as_ptr(), reservation_len) };
        // Capture immediately on actual failure, before any other operation.
        let error = if ret == 0 {
            None
        } else {
            Some(VmemError::last_os_error())
        };
        (
            ret,
            error.unwrap_or_else(VmemError::os_refusal_unknown_code),
        )
    };
    // Native errno, if any, has already been captured; only copied keys remain.
    #[cfg(feature = "bench-internals")]
    crate::bench_internals::advice::AdviceCapture::release_end(
        release_begin,
        reservation_addr,
        reservation_len,
        ret == 0,
    );
    if ret == 0 {
        Ok(())
    } else {
        Err(captured_error)
    }
}
