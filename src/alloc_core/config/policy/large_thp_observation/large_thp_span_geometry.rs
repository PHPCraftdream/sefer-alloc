//! `LargeThpSpanGeometry` — validated live-span geometry for residency
//! sampling: the span's absolute base address, its byte size, and the
//! platform's base-page size, normalized into whole-page accounting at
//! construction.
//!
//! ## Contract
//!
//! - Explicit, CHECKED base: the geometry carries the span's absolute
//!   base address as a plain `u64` (no pointers, no dereference). The
//!   base-alignment premise is VALIDATED, not deferred to the caller: a
//!   base that is not a multiple of the base page size is rejected
//!   (`BaseNotPageAligned`) — e.g. base 1 with a 4096-byte span and
//!   4096-byte pages hosts no whole aligned page at all, so planning
//!   against it would silently misalign every window. The absolute
//!   range end is overflow-checked (`AddressOverflow`).
//! - [`checked_absolute_address`](LargeThpSpanGeometry::checked_absolute_address)
//!   accounts absolute alignment for the FUTURE owning `mincore` walk:
//!   every planned window offset is a span-relative page multiple, so
//!   `base + offset` is absolutely page-aligned by construction.
//! - The trailing partial page (`span_size_bytes % base_page_size`) is
//!   excluded from the sampleable page range and REPORTED via
//!   `trailing_partial_bytes` — never silently included in sampling.
//! - Zero sizes, non-power-of-two page sizes, and spans smaller than one
//!   full page are explicitly rejected via [`LargeThpObservationError`] —
//!   never clamped, never silently truncated. Huge `u64` sizes are fine:
//!   normalization is plain division, no multiplication overflow.
//! - No OS interaction of any kind (no mincore, no pinning, no
//!   proc/sysfs reads, no pointer dereference) — a plain validated value
//!   type over `u64` numbers.
//!
//! This module is a standalone pure component; runtime integration into
//! the allocator is a separate future task.

use super::large_thp_observation_error::LargeThpObservationError;

/// A validated, normalized live-span geometry over an ABSOLUTE address
/// range.
///
/// Total by construction: every field is computed by plain division of
/// the validated inputs, so no accessor can panic and no field can be
/// out of range. The fields are private and only reachable through the
/// normalized accessors.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LargeThpSpanGeometry {
    base_address: u64,
    span_size_bytes: u64,
    base_page_size: u64,
    full_page_count: u64,
    trailing_partial_bytes: u64,
}

impl LargeThpSpanGeometry {
    /// Validate `(base_address, span_size_bytes, base_page_size)` and
    /// normalize into whole-page accounting.
    ///
    /// Rejections, in check order: `ZeroSpanSize`, `ZeroPageSize`,
    /// `PageSizeNotPowerOfTwo` (a power of two must be nonzero and
    /// satisfy `size & (size - 1) == 0`), `BaseNotPageAligned`
    /// (`base_address % base_page_size != 0`), `AddressOverflow`
    /// (`base_address + span_size_bytes` exceeds `u64::MAX`),
    /// `NoFullPages`. Total: never panics for any input triple.
    pub const fn new(
        base_address: u64,
        span_size_bytes: u64,
        base_page_size: u64,
    ) -> Result<Self, LargeThpObservationError> {
        if span_size_bytes == 0 {
            return Err(LargeThpObservationError::ZeroSpanSize);
        }
        if base_page_size == 0 {
            return Err(LargeThpObservationError::ZeroPageSize);
        }
        if base_page_size & (base_page_size - 1) != 0 {
            return Err(LargeThpObservationError::PageSizeNotPowerOfTwo);
        }
        if !base_address.is_multiple_of(base_page_size) {
            return Err(LargeThpObservationError::BaseNotPageAligned);
        }
        if base_address.checked_add(span_size_bytes).is_none() {
            return Err(LargeThpObservationError::AddressOverflow);
        }
        let full_page_count = span_size_bytes / base_page_size;
        if full_page_count == 0 {
            return Err(LargeThpObservationError::NoFullPages);
        }
        Ok(Self {
            base_address,
            span_size_bytes,
            base_page_size,
            full_page_count,
            trailing_partial_bytes: span_size_bytes % base_page_size,
        })
    }

    /// Validates the bounded owner-geometry triple of a first-life Large
    /// mapping: the original reservation root/len and the aligned usable
    /// base. Alignment slack (root strictly below base) is valid
    /// ordinary-backend geometry; what must hold is bounding and
    /// page/root alignment, not pointer equality between root and base.
    ///
    /// Rejections, in check order: `ZeroReservationAddress` (a live
    /// reservation root is never null), `ZeroSpanSize` (zero reservation
    /// length), `ZeroPageSize`, `PageSizeNotPowerOfTwo` (the base page
    /// size and the base alignment must each be a nonzero power of two),
    /// `ReservationLenNotPageAligned` (reservation length must be a
    /// multiple of the base page size — an adoption-caller obligation),
    /// `BaseNotPageAligned` (root not page-aligned; base alignment not a
    /// multiple of the base page size; usable base not aligned to the
    /// base alignment), `BaseOutsideReservation` (base below the
    /// reservation root, or usable range end past the reservation end),
    /// and `AddressOverflow` (any checked sum exceeds `u64::MAX`).
    /// Total: never panics for any input.
    #[allow(dead_code)] // the runtime consumer is the mechanism-gated owner-free observer; the pure surface is also exercised by tests
    pub fn validate_owner_geometry(
        reservation_address: u64,
        reservation_len: u64,
        base_address: u64,
        usable_span: u64,
        base_page_size: u64,
        base_alignment: u64,
    ) -> Result<(), LargeThpObservationError> {
        if reservation_address == 0 {
            return Err(LargeThpObservationError::ZeroReservationAddress);
        }
        if reservation_len == 0 {
            return Err(LargeThpObservationError::ZeroSpanSize);
        }
        if base_page_size == 0 {
            return Err(LargeThpObservationError::ZeroPageSize);
        }
        if base_page_size & (base_page_size - 1) != 0 {
            return Err(LargeThpObservationError::PageSizeNotPowerOfTwo);
        }
        // from_raw_parts caller obligation: root and length must be
        // page-aligned / page-multiples.
        if !reservation_len.is_multiple_of(base_page_size) {
            return Err(LargeThpObservationError::ReservationLenNotPageAligned);
        }
        if !reservation_address.is_multiple_of(base_page_size) {
            return Err(LargeThpObservationError::BaseNotPageAligned);
        }
        if base_alignment == 0 || base_alignment & (base_alignment - 1) != 0 {
            return Err(LargeThpObservationError::PageSizeNotPowerOfTwo);
        }
        if !base_alignment.is_multiple_of(base_page_size) {
            return Err(LargeThpObservationError::BaseNotPageAligned);
        }
        if !base_address.is_multiple_of(base_alignment) {
            return Err(LargeThpObservationError::BaseNotPageAligned);
        }
        if base_address < reservation_address {
            return Err(LargeThpObservationError::BaseOutsideReservation);
        }
        let base_end = base_address
            .checked_add(usable_span)
            .ok_or(LargeThpObservationError::AddressOverflow)?;
        let reservation_end = reservation_address
            .checked_add(reservation_len)
            .ok_or(LargeThpObservationError::AddressOverflow)?;
        if base_end > reservation_end {
            return Err(LargeThpObservationError::BaseOutsideReservation);
        }
        Ok(())
    }

    /// The validated absolute base address (echo of the input; always a
    /// multiple of the base page size).
    #[must_use]
    #[allow(dead_code)] // pure-API accessors exercised only by the #[path]-included tests/j13b_observation_residency_plan.rs; the runtime consumes new/full_page_count only
    pub const fn base_address(&self) -> u64 {
        self.base_address
    }

    /// The absolute address of a span-relative byte offset, checked.
    ///
    /// Total: returns `AddressOverflow` instead of wrapping on overflow.
    /// For planner-produced window offsets the call cannot overflow —
    /// the whole range end was validated at construction.
    #[allow(dead_code)] // pure-API accessors exercised only by the #[path]-included tests/j13b_observation_residency_plan.rs; the runtime consumes new/full_page_count only
    pub const fn checked_absolute_address(
        &self,
        span_relative_offset: u64,
    ) -> Result<u64, LargeThpObservationError> {
        match self.base_address.checked_add(span_relative_offset) {
            Some(address) => Ok(address),
            None => Err(LargeThpObservationError::AddressOverflow),
        }
    }

    /// The validated span size in bytes (echo of the input).
    #[must_use]
    #[allow(dead_code)] // pure-API accessors exercised only by the #[path]-included tests/j13b_observation_residency_plan.rs; the runtime consumes new/full_page_count only
    pub const fn span_size_bytes(&self) -> u64 {
        self.span_size_bytes
    }

    /// The validated base page size in bytes (echo of the input).
    #[must_use]
    pub const fn base_page_size(&self) -> u64 {
        self.base_page_size
    }

    /// Whole base pages the span holds (`span_size_bytes /
    /// base_page_size`). The sampleable page range is exactly
    /// `[0, full_page_count)` — the trailing partial page is excluded.
    #[must_use]
    pub const fn full_page_count(&self) -> u64 {
        self.full_page_count
    }

    /// Trailing bytes past the last full page
    /// (`span_size_bytes % base_page_size`). Excluded from the
    /// sampleable range by contract; reported so nothing is silent.
    #[must_use]
    #[allow(dead_code)] // pure-API accessors exercised only by the #[path]-included tests/j13b_observation_residency_plan.rs; the runtime consumes new/full_page_count only
    pub const fn trailing_partial_bytes(&self) -> u64 {
        self.trailing_partial_bytes
    }
}
