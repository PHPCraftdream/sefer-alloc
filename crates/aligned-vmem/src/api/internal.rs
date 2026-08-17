use core::ptr::NonNull;

use crate::error::VmemError;
use crate::page::PAGE;
use crate::page_size::page_size;
use crate::Reservation;

/// A contract violation (bad `size`/`align`) returns
/// [`VmemError::invalid_argument`] without touching the OS.
pub(crate) fn validate_size_align(size: usize, align: usize) -> Result<(), VmemError> {
    if size == 0 || !align.is_power_of_two() || align < PAGE || !size.is_multiple_of(PAGE) {
        return Err(VmemError::invalid_argument());
    }
    // Reject size/align combinations that would overflow `size + align`
    // internally (e.g. on the two-call path), to ensure consistent
    // classification as `invalid_argument` across all platforms rather than
    // an OS-specific refusal.
    let Some(sum) = size.checked_add(align) else {
        return Err(VmemError::invalid_argument());
    };
    // task #957 (fxx-3.3): `checked_add` above only rejects overflow past
    // `usize::MAX`, but `Layout::from_size_align` (consulted later for the
    // resulting `reservation_len`, e.g. in `release`'s G-1 assert and in
    // `from_raw_parts`'s equivalent check) additionally requires the size to
    // fit within `isize::MAX` once rounded up to `align` -- a `sum` between
    // `isize::MAX` and `usize::MAX` passes `checked_add` but would later fail
    // that `Layout` construction, turning what should be an immediate,
    // attributable `invalid_argument` here into a deferred assert/panic
    // downstream. Reject it here instead, for the same reason `checked_add`
    // is checked above: a contract violation should be classified
    // consistently and immediately, before any OS or `Layout` call.
    if sum > isize::MAX as usize {
        return Err(VmemError::invalid_argument());
    }
    Ok(())
}

/// Private helper: validate `initial_commit` for lazy reservations.
/// Only called from `try_reserve_aligned_lazy`, which is itself gated on
/// `lazy-commit` -- dead code when that feature is off.
///
/// On Windows, `VirtualAlloc(MEM_COMMIT)` operates on whole runtime pages,
/// and `try_commit_range` accepts only offsets that are multiples of `page_size()`.
/// Therefore, both `size` and `initial_commit` must be multiples of the runtime
/// page size to avoid creating unwritable tails. This is a fail-closed check:
/// reject requests that would create spans that cannot be fully committed via the
/// public API.
///
/// **Why this check is NOT `#[cfg(windows)]`-gated, even though the hazard it
/// prevents is Windows-only** (task #1037, finding R6-2, which is itself
/// scoped "platform-specific ... on Windows"): on Unix,
/// `reserve_aligned_lazy_raw` IGNORES `initial_commit` entirely and delegates
/// straight to `reserve_aligned_raw` — the whole span is committed by the
/// `mmap` itself, so an uncommittable tail cannot exist there. A Unix-only
/// caller therefore loses nothing real by this rejection and gains nothing
/// real from being allowed through.
///
/// The rejection is uniform anyway because the alternative is worse: a caller
/// developing on x86-64 Linux who passes a `PAGE`-multiple that is not a
/// `page_size()` multiple writes code that is silently broken the moment it
/// runs on Windows with a larger runtime page — and the crate would have
/// accepted it on every host they tested. Note that a per-OS `#[cfg]` would
/// NOT buy portability either: `page_size()` is a RUNTIME value, so the same
/// call already differs between a 4 KiB x86-64 host and a 16 KiB Apple
/// Silicon one. There is no portable compile-time constant to validate
/// against, which is exactly why the contract is stated in terms of
/// `page_size()` and enforced everywhere.
///
/// Concrete cost of the uniform rule, stated so it is not a surprise: on a
/// 16 KiB-page host, `reserve_aligned_lazy(size, align, PAGE)` is now
/// rejected, where before it was accepted and worked (harmlessly, since Unix
/// ignores the argument). `PAGE` is a compile-time floor, not the runtime
/// page size; callers must pass a `page_size()` multiple.
#[cfg_attr(not(feature = "lazy-commit"), allow(dead_code))]
pub(crate) fn validate_initial_commit(initial_commit: usize, size: usize) -> Result<(), VmemError> {
    let ps = page_size();
    if initial_commit == 0
        || !initial_commit.is_multiple_of(ps)
        || !size.is_multiple_of(ps)
        || initial_commit > size
    {
        return Err(VmemError::invalid_argument());
    }
    Ok(())
}

/// Private struct for raw reservation results from backend functions.
/// Named to prevent transposing `base` and `reservation` (both `NonNull<u8>`).
///
/// This is call-site convenience only: the backend functions themselves still
/// return unnamed tuples, and the struct is constructed only at the call sites
/// via `.map()`. This helps at the call site but does NOT eliminate the
/// transposition risk entirely — the two `NonNull<u8>` tuple elements are still
/// unnamed at the backend layer.
pub(crate) struct RawReservation {
    /// The aligned usable base of the reservation.
    pub(crate) base: NonNull<u8>,
    /// The underlying OS reservation start (may be lower than `base`).
    pub(crate) reservation: NonNull<u8>,
    /// Full reservation length in bytes.
    pub(crate) reservation_len: usize,
    /// Whether large/huge pages were granted (Linux `MAP_HUGETLB` / Windows `MEM_LARGE_PAGES`).
    pub(crate) granted_huge: bool,
}

/// Private helper: finish a reservation from a raw backend result.
pub(crate) fn finish_reservation(
    size: usize,
    align: usize,
    raw: Result<RawReservation, VmemError>,
) -> Result<Reservation, VmemError> {
    raw.map(|r| Reservation {
        base: r.base,
        len: size,
        reservation: r.reservation,
        reservation_len: r.reservation_len,
        align,
        granted_huge: r.granted_huge,
    })
}

/// Private helper: finish a reservation from a raw backend result (4-tuple).
/// Only called from `try_reserve_aligned_huge`, which is itself gated on
/// `huge-pages` -- dead code when that feature is off.
#[cfg_attr(not(feature = "huge-pages"), allow(dead_code))]
pub(crate) fn finish_reservation_huge(
    size: usize,
    align: usize,
    raw: Result<(NonNull<u8>, NonNull<u8>, usize, bool), VmemError>,
) -> Result<Reservation, VmemError> {
    raw.map(
        |(base, reservation, reservation_len, granted_huge)| Reservation {
            base,
            len: size,
            reservation,
            reservation_len,
            align,
            granted_huge,
        },
    )
}
