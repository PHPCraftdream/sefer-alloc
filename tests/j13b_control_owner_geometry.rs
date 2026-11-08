//! J13b control: production-binding pure oracle for the alignment-slack
//! owner-geometry contract. Whether a NATIVE reservation actually carries
//! alignment slack is address-luck (the kernel may hand back an
//! already-aligned region), so the discriminator here is this
//! production-compiled pure validator, not a native address expectation.
//!
//! Platform-independent: exercises `LargeThpSpanGeometry::
//! validate_owner_geometry` directly through the public
//! `alloc_core::policy` re-export. One scenario per binary (one `#[test]`).

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use sefer_alloc::alloc_core::policy::large_thp_observation::large_thp_observation_error::LargeThpObservationError;
use sefer_alloc::alloc_core::policy::large_thp_observation::large_thp_span_geometry::LargeThpSpanGeometry;

const PAGE: u64 = 4096;
const ALIGN: u64 = 4 * 1024 * 1024; // 0x400000

#[test]
fn owner_geometry_validator_accepts_slack_and_rejects_with_exact_variants() {
    // ACCEPT: slack geometry — root strictly below base, reservation spans
    // base-root plus the usable span.
    let root: u64 = 0x100000;
    let base: u64 = 0x400000;
    let usable_span: u64 = 0x800000;
    let reservation_len = (base - root) + usable_span;
    assert_eq!(
        LargeThpSpanGeometry::validate_owner_geometry(
            root,
            reservation_len,
            base,
            usable_span,
            PAGE,
            ALIGN
        ),
        Ok(()),
        "slack geometry with root strictly below an aligned base must be accepted"
    );

    // ACCEPT: equality geometry — root == base, reservation len == span.
    assert_eq!(
        LargeThpSpanGeometry::validate_owner_geometry(
            base,
            usable_span,
            base,
            usable_span,
            PAGE,
            ALIGN
        ),
        Ok(()),
        "zero-slack (root == base) geometry must be accepted"
    );

    // REJECT: root == 0 — a live reservation root is never null.
    assert_eq!(
        LargeThpSpanGeometry::validate_owner_geometry(
            0,
            reservation_len,
            base,
            usable_span,
            PAGE,
            ALIGN
        ),
        Err(LargeThpObservationError::ZeroReservationAddress),
        "null reservation root must be rejected with ZeroReservationAddress"
    );

    // REJECT: reservation_len == 0.
    assert_eq!(
        LargeThpSpanGeometry::validate_owner_geometry(root, 0, base, usable_span, PAGE, ALIGN),
        Err(LargeThpObservationError::ZeroSpanSize),
        "zero reservation length must be rejected with ZeroSpanSize"
    );

    // REJECT: reservation_len not a multiple of the base page size.
    assert_eq!(
        LargeThpSpanGeometry::validate_owner_geometry(
            root,
            reservation_len + 1,
            base,
            usable_span,
            PAGE,
            ALIGN
        ),
        Err(LargeThpObservationError::ReservationLenNotPageAligned),
        "unaligned reservation length must be rejected with ReservationLenNotPageAligned"
    );

    // REJECT: reservation root not a multiple of the base page size.
    assert_eq!(
        LargeThpSpanGeometry::validate_owner_geometry(
            root + 1,
            reservation_len,
            base,
            usable_span,
            PAGE,
            ALIGN
        ),
        Err(LargeThpObservationError::BaseNotPageAligned),
        "unaligned reservation root must be rejected with BaseNotPageAligned"
    );

    // REJECT: base not aligned to the base alignment.
    assert_eq!(
        LargeThpSpanGeometry::validate_owner_geometry(
            root,
            reservation_len,
            base + PAGE,
            usable_span,
            PAGE,
            ALIGN
        ),
        Err(LargeThpObservationError::BaseNotPageAligned),
        "base not aligned to the base alignment must be rejected with BaseNotPageAligned"
    );

    // REJECT: base below the reservation root. This input violates ONLY the
    // base<root check: root = 0x1400000 is page- and ALIGN-aligned, base =
    // 0x400000 is ALIGN-aligned, and root_end = 0x2400000 >= base_end =
    // 0xC00000, so every earlier check passes.
    let below_root = 0x1400000u64;
    let below_base = 0x400000u64;
    let below_usable = 0x800000u64;
    let below_len = 0x1000000u64;
    assert_eq!(
        LargeThpSpanGeometry::validate_owner_geometry(
            below_root,
            below_len,
            below_base,
            below_usable,
            PAGE,
            ALIGN
        ),
        Err(LargeThpObservationError::BaseOutsideReservation),
        "base below the reservation root must be rejected with BaseOutsideReservation"
    );

    // REJECT: base + usable span past the reservation end.
    let one_past_len = reservation_len + PAGE;
    assert_eq!(
        LargeThpSpanGeometry::validate_owner_geometry(
            root,
            reservation_len,
            base,
            one_past_len - (base - root),
            PAGE,
            ALIGN
        ),
        Err(LargeThpObservationError::BaseOutsideReservation),
        "usable end past the reservation end must be rejected with BaseOutsideReservation"
    );

    // REJECT: base_page_size == 0.
    assert_eq!(
        LargeThpSpanGeometry::validate_owner_geometry(
            root,
            reservation_len,
            base,
            usable_span,
            0,
            ALIGN
        ),
        Err(LargeThpObservationError::ZeroPageSize),
        "zero base page size must be rejected with ZeroPageSize"
    );

    // REJECT: base_alignment == 0 (non-power-of-two arm).
    assert_eq!(
        LargeThpSpanGeometry::validate_owner_geometry(
            root,
            reservation_len,
            base,
            usable_span,
            PAGE,
            0
        ),
        Err(LargeThpObservationError::PageSizeNotPowerOfTwo),
        "zero base alignment must be rejected with PageSizeNotPowerOfTwo"
    );

    // REJECT: base_address + usable_span overflows u64::MAX.
    let huge_base = u64::MAX - ALIGN + 1 - (u64::MAX - ALIGN + 1) % ALIGN;
    assert_eq!(
        LargeThpSpanGeometry::validate_owner_geometry(
            root,
            reservation_len,
            huge_base,
            u64::MAX,
            PAGE,
            ALIGN
        ),
        Err(LargeThpObservationError::AddressOverflow),
        "overflowing usable range end must be rejected with AddressOverflow"
    );
}
