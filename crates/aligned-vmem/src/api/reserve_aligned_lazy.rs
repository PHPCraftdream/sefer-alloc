use crate::error::VmemError;
use crate::lazy_reservation::LazyReservation;
#[cfg(aligned_vmem_mock)]
use crate::mock;
use crate::os::reserve_aligned_lazy_raw;
#[cfg(aligned_vmem_mock)]
use crate::os::reserve_aligned_raw;

use super::internal::{
    finish_reservation, validate_initial_commit, validate_size_align, RawReservation,
};

/// Reserve `size` bytes of anonymous virtual memory whose base is aligned to
/// `align`, committing ONLY the first `initial_commit` bytes — the rest is
/// reserved but NOT committed (on Windows; on Unix/miri ALL pages are committed,
/// matching the eager path).
///
/// See [`reserve_aligned`](crate::api::reserve_aligned) for the base/align contract. `initial_commit` must
/// be a non-zero multiple of the runtime [`page_size()`](crate::page_size::page_size) (not the compile-time
/// [`PAGE`](crate::page::PAGE)) and `<= size`; `size` must also be a multiple of [`page_size()`](crate::page_size::page_size).
/// Violations return `None`. This stricter contract exists because on Windows,
/// `VirtualAlloc(MEM_COMMIT)` operates on whole runtime pages and
/// `commit_range` accepts only offsets that are multiples of `page_size()`; a
/// `size` not aligned to `page_size()` would create an unwritable tail that
/// cannot be committed via the public API.
///
/// The returned [`Reservation`](crate::Reservation) frees the ENTIRE VA reservation on drop
/// regardless of how much was committed. For the failure cause use
/// [`try_reserve_aligned_lazy`].
#[must_use]
#[cfg(feature = "lazy-commit")]
#[cfg_attr(docsrs, doc(cfg(feature = "lazy-commit")))]
pub fn reserve_aligned_lazy(
    size: usize,
    align: usize,
    initial_commit: usize,
) -> Option<LazyReservation> {
    try_reserve_aligned_lazy(size, align, initial_commit).ok()
}

/// Fallible [`reserve_aligned_lazy`].
///
/// Returns a [`LazyReservation`], which carries the commit watermark alongside
/// the span. Callers that keep their own commit bookkeeping take
/// [`LazyReservation::into_reservation`] and drive the raw primitives directly.
#[cfg(feature = "lazy-commit")]
#[cfg_attr(docsrs, doc(cfg(feature = "lazy-commit")))]
pub fn try_reserve_aligned_lazy(
    size: usize,
    align: usize,
    initial_commit: usize,
) -> Result<LazyReservation, VmemError> {
    validate_size_align(size, align)?;
    validate_initial_commit(initial_commit, size)?;
    #[cfg(aligned_vmem_mock)]
    if let Some(e) = mock::take_reserve_fault() {
        mock::record(mock::Call::ReserveLazy {
            size,
            align,
            initial_commit,
        });
        return Err(e);
    }
    #[cfg(aligned_vmem_mock)]
    mock::record(mock::Call::ReserveLazy {
        size,
        align,
        initial_commit,
    });

    // Under `mock` the OS partial-commit is bypassed: `commit_range` records-
    // and-returns without touching the OS, so a genuinely partially-committed
    // Windows reservation would leave the tail unwritable and fault when the
    // consumer's mocked "commit" is a no-op. Chain to the EAGER (fully
    // committed) backend instead, so the returned span is entirely usable while
    // the mock still records the `ReserveLazy` call for assertion.
    #[cfg(aligned_vmem_mock)]
    let raw = reserve_aligned_raw(size, align);
    #[cfg(not(aligned_vmem_mock))]
    let raw = reserve_aligned_lazy_raw(size, align, initial_commit);

    // task #713: both `raw` branches now capture their own `VmemError`
    // immediately at the point of failure; this just propagates it.
    finish_reservation(
        size,
        align,
        raw.map(|(base, reservation, reservation_len)| RawReservation {
            base,
            reservation,
            reservation_len,
            granted_huge: false,
        }),
    )
    .map(|r| LazyReservation::new(r, initial_commit))
}
