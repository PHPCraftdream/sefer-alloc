//! Item 54 / V-31 (task #1058): struct-level coverage for `ReservationParts`'s
//! derived `PartialEq`/`Eq`. The existing `ReservationParts` tests in
//! `smoke.rs` compare individual FIELDS (ptr/len/align) — none exercises the
//! derived struct equality itself.

use core::ptr::NonNull;

use aligned_vmem::ReservationParts;

/// Distinct, never-dereferenced addresses — only their VALUES participate in
/// the derived `PartialEq` (raw-pointer equality compares addresses).
fn dangling_ptr(slot: usize) -> *mut u8 {
    NonNull::<u8>::dangling()
        .as_ptr()
        .wrapping_add(slot * 0x1000)
}

/// Item 54 / V-31: identical `(ptr, len, align)` must compare equal via the
/// derived `PartialEq`.
#[test]
fn reservation_parts_equal_for_identical_fields() {
    let (ptr, len, align) = (dangling_ptr(0), 2 * 4096, 4096);
    let a = ReservationParts::new(ptr, len, align);
    let b = ReservationParts::new(ptr, len, align);
    assert_eq!(a, b, "identical (ptr, len, align) must compare equal");

    // Pin the `Eq` marker half of the derive — `assert_eq!` alone exercises
    // only `PartialEq`.
    fn require_eq<T: Eq>() {}
    require_eq::<ReservationParts>();
}

/// Item 54 / V-31: differing in exactly one field (each of ptr/len/align in
/// turn) must compare unequal.
#[test]
fn reservation_parts_unequal_when_any_field_differs() {
    let ptr = dangling_ptr(0);
    let base = ReservationParts::new(ptr, 2 * 4096, 4096);

    let other_ptr = ReservationParts::new(dangling_ptr(1), 2 * 4096, 4096);
    assert_ne!(base, other_ptr, "differing ptr must compare unequal");

    let other_len = ReservationParts::new(ptr, 4 * 4096, 4096);
    assert_ne!(base, other_len, "differing len must compare unequal");

    let other_align = ReservationParts::new(ptr, 2 * 4096, 2 * 4096);
    assert_ne!(base, other_align, "differing align must compare unequal");
}
