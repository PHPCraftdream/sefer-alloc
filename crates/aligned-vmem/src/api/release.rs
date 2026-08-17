use core::ptr::NonNull;

#[cfg(aligned_vmem_mock)]
use crate::mock;
use crate::os::release_reservation;
use crate::page::PAGE;
use crate::reservation_parts::ReservationParts;

/// Release a whole OS reservation obtained from [`Reservation::into_parts`](crate::Reservation::into_parts).
///
/// # Safety
///
/// `reservation`, `reservation_len` and `align` must be the three values
/// returned by [`Reservation::into_parts`](crate::Reservation::into_parts) (or, for a self-hosting caller that
/// always uses one alignment, that same alignment constant), and the
/// reservation must be released **exactly once**. The native (`munmap` /
/// `VirtualFree`) paths ignore `align`; it is consulted only by the miri
/// fallback to reconstruct the exact `Layout`.
///
/// If `reservation` is null, this function returns early and does nothing
/// (the call is a no-op). The mock recorder is also skipped in this case,
/// so a `mock`-based test's expected call log may desync if it expects a
/// record for a null pointer.
///
/// # Panics
///
/// Panics if `reservation` is non-null and `(reservation_len, align)` violates
/// the documented contract above: `reservation_len` must be non-zero and a
/// multiple of [`PAGE`], `align` must be a power of two `>= PAGE`, and the
/// pair must form a valid [`std::alloc::Layout`]. The assert runs before
/// `mock::record`, so under the `aligned_vmem_mock` cfg a contract-violating
/// call panics before it is ever recorded in the mock call log — it does not
/// appear as a `Release` entry.
///
/// A null `reservation` is unaffected by this: it remains the documented
/// no-op above and is not a panic path.
pub unsafe fn release(reservation: *mut u8, reservation_len: usize, align: usize) {
    // Historical note (task #947/G-1): before this assert existed, this doc
    // comment used to claim "the native (`munmap`/`VirtualFree`) paths ignore
    // `align`" — which was true in the sense that a contract-violating call
    // would silently "succeed" (no crash, no error) on those native backends;
    // only the `miri` fallback path (which reconstructs a `Layout` from
    // `reservation_len`/`align` to call back into `std::alloc`) would panic on
    // the same bad input, with a bare, uninformative `.expect()` message. That
    // divergence is now closed: this function validates the contract up front
    // and panics with a descriptive message on **every** backend, not only
    // under `miri`. The assert runs before `mock::record`, so under the
    // `aligned_vmem_mock` cfg a contract-violating call panics before it is
    // ever recorded in the mock call log — it does not appear as a `Release`
    // entry.
    //
    // The checked invariants are a subset of `from_raw_parts`'s checks because
    // `release` receives only `(reservation_len, align)` (not the full
    // `(base, len, reservation, reservation_len, align)` tuple), so the bounds
    // between `base` and `reservation` are uncheckable here — we validate what
    // we can and keep the same informative message style.
    if reservation.is_null() {
        return;
    }
    assert!(
        reservation_len != 0
            && reservation_len.is_multiple_of(PAGE)
            && align.is_power_of_two()
            && align >= PAGE
            && std::alloc::Layout::from_size_align(reservation_len, align).is_ok(),
        "release: \
         reservation_len must be non-zero and a multiple of PAGE; \
         align must be a power of two >= PAGE; \
         (reservation_len, align) must form a valid Layout; \
         got reservation_len={reservation_len}, align={align}"
    );

    let nn = NonNull::new(reservation).expect("checked non-null above");
    #[cfg(aligned_vmem_mock)]
    mock::record(mock::Call::Release {
        reservation: reservation.addr(),
        reservation_len,
    });
    // SAFETY: forwarded from the caller's contract above.
    unsafe { release_reservation(nn, reservation_len, align) };
}

/// Release a reservation obtained from [`Reservation::into_reservation_parts`](crate::Reservation::into_reservation_parts).
///
/// This is the typed alternative to [`release`]: it takes a [`ReservationParts`]
/// struct instead of raw parameters, preventing accidental swapping of `len` and
/// `align` (which would cause undefined behavior on the native backend and leaks
/// or crashes on Unix).
///
/// For backwards compatibility with code that uses the raw tuple form, you can
/// convert a `ReservationParts` to a tuple via [`ReservationParts::as_tuple`] and
/// call [`release`].
///
/// # Safety
///
/// `parts.ptr` must be a reservation obtained from [`Reservation::into_reservation_parts`](crate::Reservation::into_reservation_parts)
/// (or the raw [`Reservation::into_parts`](crate::Reservation::into_parts)) and must be live. The reservation must be released
/// exactly once.
pub unsafe fn release_parts(parts: ReservationParts) {
    let ReservationParts {
        ptr: reservation,
        len: reservation_len,
        align,
    } = parts;
    // Delegate to the existing release function.
    unsafe { release(reservation, reservation_len, align) };
}
