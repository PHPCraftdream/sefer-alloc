use crate::Reservation;

/// The full components returned by [`Reservation::into_full_parts`].
///
/// This struct contains ALL six fields needed to reconstruct a `Reservation`
/// via [`Reservation::from_raw_parts`], eliminating the risk of metadata loss
/// during round-trip. Unlike [`ReservationParts`](crate::reservation_parts::ReservationParts), it preserves `base`, `len`,
/// and `granted_huge` in addition to the underlying reservation metadata.
///
/// This is the lossless round-trip alternative to [`ReservationParts`](crate::reservation_parts::ReservationParts). Use it
/// when you need to temporarily extract all reservation state for later
/// reconstruction.
///
/// **This struct holds the ONLY information that can free the underlying OS
/// reservation** (task #1213/L3) — it has no `Drop` impl (see the
/// "IMPORTANT" note on [`Reservation::into_full_parts`] for the full
/// explanation), so a plain `drop` of a `ReservationFullParts` — letting it
/// go out of scope without ever calling
/// [`into_reservation`](Self::into_reservation) (and then dropping the
/// resulting [`Reservation`](crate::Reservation)) or manually releasing via
/// [`release`](crate::api::release) — silently leaks the mapping.
/// `#[must_use]` here catches an ACCIDENTALLY discarded ownership token
/// (e.g. a call to
/// [`Reservation::into_full_parts`](crate::Reservation::into_full_parts)
/// whose result is never bound to anything) at compile time; it does not
/// and cannot prevent a DELIBERATE leak (e.g. binding the result to `_` or
/// storing it and then dropping it later without acting on it).
#[must_use = "dropping `ReservationFullParts` leaks the reservation — call \
              `into_reservation` and drop the resulting `Reservation`, or \
              release the `reservation`/`reservation_len`/`align` fields \
              manually via `release`"]
#[non_exhaustive]
#[derive(Debug, PartialEq, Eq)]
pub struct ReservationFullParts {
    /// The aligned usable start pointer (from [`Reservation::as_ptr`]).
    pub base: *mut u8,
    /// The usable span size in bytes (from [`Reservation::len`]).
    pub len: usize,
    /// The underlying OS reservation start (from [`Reservation::reservation_ptr`]).
    pub reservation: *mut u8,
    /// The length of the reservation in bytes (from [`Reservation::reservation_len`]).
    pub reservation_len: usize,
    /// The alignment requested at reservation time.
    pub align: usize,
    /// Whether the OS granted huge pages for this reservation (from [`Reservation::is_huge`]).
    pub granted_huge: bool,
}

impl ReservationFullParts {
    /// Construct a `ReservationFullParts` from its component fields.
    ///
    /// This is the inverse of [`Reservation::into_full_parts`]. All six fields
    /// are required to reconstruct a complete `Reservation` with no metadata loss.
    ///
    /// No message-less `#[must_use]` on this function itself (task
    /// #1213/L3): it returns `Self`, and the type now carries its own
    /// `#[must_use]` with a leak-specific message — see
    /// [`ReservationParts::new`](crate::reservation_parts::ReservationParts::new)
    /// for the identical reasoning.
    #[inline]
    pub const fn new(
        base: *mut u8,
        len: usize,
        reservation: *mut u8,
        reservation_len: usize,
        align: usize,
        granted_huge: bool,
    ) -> Self {
        Self {
            base,
            len,
            reservation,
            reservation_len,
            align,
            granted_huge,
        }
    }

    /// Reconstruct a `Reservation` from these parts.
    ///
    /// This is a convenience wrapper around [`Reservation::from_raw_parts`]
    /// that forwards all six fields. The same safety requirements apply.
    ///
    /// # Safety
    ///
    /// All six fields must satisfy the same invariants as documented for
    /// [`Reservation::from_raw_parts`]. See that function's `# Safety` section
    /// for full details.
    #[must_use]
    pub unsafe fn into_reservation(self) -> Reservation {
        // SAFETY: Delegated to the caller — same contract as `from_raw_parts`.
        unsafe {
            Reservation::from_raw_parts(
                self.base,
                self.len,
                self.reservation,
                self.reservation_len,
                self.align,
                self.granted_huge,
            )
        }
    }
}
