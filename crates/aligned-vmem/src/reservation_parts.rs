/// The components returned by [`Reservation::into_reservation_parts`](crate::Reservation::into_reservation_parts).
///
/// A named structure (instead of a raw tuple) prevents the footgun of
/// accidentally swapping the `len` and `align` fields, which would be
/// undefined behavior on the native backend and cause leaks or crashes
/// on the Unix backend.
///
/// `ReservationParts::new` closes the `release_parts` round-trip (release a
/// reservation you only have the parts for). Reconstructing a full
/// `Reservation` via `from_raw_parts` additionally requires the usable `base`,
/// `len`, and `granted_huge` fields, which the caller must record separately —
/// `ReservationParts` alone is insufficient whenever the reservation was
/// over-reserved for alignment or when huge-page status must be preserved.
/// If you omit `granted_huge`, the reconstructed reservation will incorrectly
/// report `is_huge() == false` even if the original reservation used huge pages.
#[non_exhaustive]
#[derive(Debug, PartialEq, Eq)]
pub struct ReservationParts {
    /// The base pointer of the reservation (from [`Reservation::reservation_ptr`](crate::Reservation::reservation_ptr)).
    pub ptr: *mut u8,
    /// The length of the reservation in bytes (from [`Reservation::reservation_len`](crate::Reservation::reservation_len)).
    pub len: usize,
    /// The alignment requested at reservation time.
    pub align: usize,
}

impl ReservationParts {
    /// Construct a `ReservationParts` from its component fields.
    ///
    /// This closes the `release_parts` round-trip (release a reservation you
    /// only have the parts for). Reconstructing a full `Reservation` via
    /// `from_raw_parts` additionally requires the usable `base`, `len`, and
    /// `granted_huge` fields, which the caller must record separately —
    /// `ReservationParts` alone is insufficient whenever the reservation was
    /// over-reserved for alignment or when huge-page status must be preserved.
    #[must_use]
    #[inline]
    pub const fn new(ptr: *mut u8, len: usize, align: usize) -> Self {
        Self { ptr, len, align }
    }

    /// Convert this struct back into a raw tuple compatible with [`release`](crate::api::release).
    ///
    /// This method exists only for backwards compatibility with code that
    /// already uses the tuple form. New code should use [`release_parts`](crate::api::release_parts) instead.
    #[must_use]
    #[inline]
    pub const fn as_tuple(self) -> (*mut u8, usize, usize) {
        (self.ptr, self.len, self.align)
    }
}
