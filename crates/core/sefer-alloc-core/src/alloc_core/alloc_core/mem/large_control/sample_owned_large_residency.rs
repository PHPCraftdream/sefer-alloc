//! Owning Large residency bridge; the sole unsafe operation temporarily
//! adopts the validated live reservation without releasing caller ownership.

#![allow(unsafe_code)]

use core::mem::ManuallyDrop;

/// Sample an exclusively owned canonical large reservation without releasing it.
///
/// The caller guarantees `reservation`/`reservation_len` identify the exact
/// live, exclusively owned, canonical reservation whose payload is already
/// terminal-free with no concurrent release; `usable_base`/`usable_span`
/// are the SEGMENT-aligned usable start/span validated upstream by the
/// bounded owner-geometry check (which proves `usable_base >= reservation`
/// and `usable_end <= reservation_end`, i.e. the `reservation_len >=
/// usable_span + (usable_base - reservation)` conjunct of
/// `from_raw_parts`). `usable_span` is a runtime-page multiple on every
/// reserve path (SEGMENT-multiple default, page-rounded exact-span,
/// page-multiple reserved-capacity growth). The align argument records the
/// alignment the reservation was created with, which is SEGMENT for all
/// three ordinary constructors. Region offsets are USABLE-BASE-relative.
/// This is crate-private because a raw pointer alone cannot prove those
/// safety invariants.
#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
pub(super) fn sample_owned_large_residency(
    reservation: *mut u8,
    reservation_len: usize,
    usable_base: *mut u8,
    usable_span: usize,
    regions: &[aligned_vmem::ResidencyRegion],
) -> Result<aligned_vmem::ResidencySample, aligned_vmem::VmemError> {
    // SAFETY: caller guarantees `reservation`/`reservation_len` identify the
    // exact live, exclusively owned, canonical reservation whose payload is
    // terminal-free with no concurrent release; `usable_base`/`usable_span`
    // are the SEGMENT-aligned usable start/span validated upstream by the
    // bounded owner-geometry check, which proves `usable_base >=
    // reservation` and `usable_end <= reservation_end`, i.e. the
    // `reservation_len >= usable_span + (usable_base - reservation)`
    // conjunct of `from_raw_parts`. `usable_span` is a runtime-page
    // multiple on every reserve path (SEGMENT-multiple default,
    // page-rounded exact-span, page-multiple reserved-capacity growth),
    // `usable_base % SEGMENT == 0` holds, and the align argument records
    // the creation alignment, which is SEGMENT for all three ordinary
    // constructors. Region offsets are USABLE-BASE-relative. Adoption
    // with base = the aligned usable start preserves the root's
    // allocation provenance.
    let owned = unsafe {
        aligned_vmem::Reservation::from_raw_parts(
            usable_base,
            usable_span,
            reservation,
            reservation_len,
            crate::alloc_core::os::SEGMENT,
            false,
        )
    };
    let owned = ManuallyDrop::new(owned);
    // SAFETY: the caller's liveness and exclusive-ownership contract above
    // remains in force for this borrow; ManuallyDrop suppresses Reservation's
    // release so the caller retains ownership after sampling.
    owned.try_sample_residency(regions)
}
