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
///
/// **This struct holds the ONLY information that can free the underlying OS
/// reservation** (task #1213/L3) — a plain `drop` of a `ReservationParts`
/// (letting it go out of scope without ever calling
/// [`release_parts`](crate::api::release_parts) or reconstructing a full
/// [`Reservation`](crate::Reservation)) silently leaks the mapping: this
/// struct has no `Drop` impl of its own. `#[must_use]` here catches an
/// ACCIDENTALLY discarded ownership token (e.g. a call to
/// [`Reservation::into_reservation_parts`](crate::Reservation::into_reservation_parts)
/// whose result is never bound to anything) at compile time; it does not
/// and cannot prevent a DELIBERATE leak (e.g. binding the result to `_` or
/// storing it and then dropping it later without acting on it).
#[must_use = "dropping `ReservationParts` leaks the reservation — release it \
              via `release_parts`, or reconstruct a `Reservation` via \
              `Reservation::from_raw_parts` and let that drop instead"]
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
    ///
    /// No message-less `#[must_use]` on this function itself (task
    /// #1213/L3): it returns `Self`, and the type now carries its own
    /// `#[must_use]` with a leak-specific message — a redundant message-less
    /// attribute on top of that is exactly what clippy's `double_must_use`
    /// lint rejects.
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
