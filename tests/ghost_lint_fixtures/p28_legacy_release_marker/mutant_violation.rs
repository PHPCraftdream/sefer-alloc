// expect: anchor (before) not found
//! Legacy release retains its original syscall and discarded-result behavior.
use super::super::unix::libc_munmap;
use core::ptr::NonNull;

/// # Safety
/// The original exclusively owned mmap reservation is released exactly once;
/// neither this function nor its caller may access the mapping afterwards.
pub(crate) unsafe fn release_reservation(
    reservation: NonNull<u8>,
    reservation_len: usize,
    _align: usize,
) {
    #[cfg(feature = "bench-internals")]
    let (reservation_addr, release_begin) = (
        reservation.as_ptr().addr(),
        None,
    );
    // SAFETY: on unix `reservation` is the start of the remaining mapping of length
    // `reservation_len`; `libc_munmap` requires the address/len to be a live mmap'd
    // region being unmapped once, which this call satisfies.
    let result = unsafe { libc_munmap(reservation.as_ptr(), reservation_len) };
    #[cfg(feature = "bench-internals")]
    crate::bench_internals::advice::AdviceCapture::release_end(
        release_begin,
        reservation_addr,
        reservation_len,
        result == 0,
    );
    #[cfg(not(feature = "bench-internals"))]
    let _ = result;
}
