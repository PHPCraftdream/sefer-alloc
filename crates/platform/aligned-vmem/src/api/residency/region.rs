/// Byte-offset/length window into a [`Reservation`](crate::Reservation)'s
/// usable span, passed to
/// [`Reservation::try_sample_residency`](crate::Reservation::try_sample_residency).
/// `offset` is relative to `as_ptr()`; `len` is a plain byte count and need
/// not be a page-size multiple — the effective sample window is the whole
/// pages fully inside the region (a region smaller than one page contributes
/// zero to both counters).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResidencyRegion {
    /// Byte offset of the region start within the usable span.
    pub offset: usize,
    /// Region length in bytes.
    pub len: usize,
}

impl ResidencyRegion {
    /// Construct a region at `offset` spanning `len` bytes.
    #[must_use]
    #[inline]
    pub const fn new(offset: usize, len: usize) -> Self {
        Self { offset, len }
    }
}
