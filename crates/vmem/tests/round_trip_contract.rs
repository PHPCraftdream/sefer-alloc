// Test for round-trip contract: Reservation::into_full_parts -> ReservationFullParts::into_reservation
// verifies that parts produced by the crate satisfy the documented contract.
//
// Purpose: R5-2 — ensure that a producer/consumer round-trip through from_raw_parts
// does not panic on values the crate itself produces, even on hosts where PAGE (4 KiB)
// differs from runtime page_size() (e.g., 16 KiB on Apple Silicon).

use aligned_vmem::{page_size, reserve_aligned, Reservation, ReservationFullParts, PAGE};

#[test]
fn into_full_parts_round_trip_satisfies_contract() {
    // Use the smallest meaningful size and alignment: one PAGE-aligned PAGE.
    // This is the shape where PAGE and page_size() divergence is most visible.
    let size = PAGE;
    let align = PAGE;

    // Reserve using the normal safe API.
    let reservation = reserve_aligned(size, align).expect("reserve_aligned should succeed");

    // Extract the full parts — this is what a round-trip consumer receives.
    let parts: ReservationFullParts = reservation.into_full_parts();

    // CRITICAL: Verify that the produced parts satisfy the documented contract
    // on THIS host, which may have page_size() != PAGE.
    //
    // The contract has two tiers of requirements (R5-2 fix):
    //
    // Tier 1 (logical lengths, checked by from_raw_parts assert):
    // - `len` is a non-zero multiple of PAGE (not necessarily page_size()).
    // - `reservation_len` is a non-zero multiple of PAGE (not necessarily page_size()).
    //
    // Tier 2 (addresses and operations, NOT checked by from_raw_parts assert):
    // - `base` is aligned to align AND to page_size() (not just PAGE).
    // - `reservation` is aligned to page_size().
    //
    // The test verifies BOTH tiers because the round-trip must succeed on the
    // current host without panicking.
    let runtime_page_size = page_size();

    // Tier 1: logical lengths — only PAGE multiple is guaranteed.
    assert!(
        parts.len != 0 && parts.len.is_multiple_of(PAGE),
        "len ({}) must be non-zero and a multiple of PAGE ({})",
        parts.len,
        PAGE
    );
    assert!(
        parts.reservation_len != 0 && parts.reservation_len.is_multiple_of(PAGE),
        "reservation_len ({}) must be non-zero and a multiple of PAGE ({})",
        parts.reservation_len,
        PAGE
    );
    assert!(
        parts.reservation_len >= parts.len + (parts.base.addr() - parts.reservation.addr()),
        "reservation_len ({}) must be >= len ({}) + (base - reservation) ({})",
        parts.reservation_len,
        parts.len,
        parts.base.addr() - parts.reservation.addr()
    );

    // Tier 2: addresses and operations — page_size() alignment is required.
    assert!(
        parts.base.addr().is_multiple_of(runtime_page_size),
        "base ({}) must be aligned to runtime page_size() ({}) on this host, not just PAGE ({})",
        parts.base.addr(),
        runtime_page_size,
        PAGE
    );
    assert!(
        parts.base.addr().is_multiple_of(align),
        "base ({}) must be aligned to align ({})",
        parts.base.addr(),
        align
    );
    assert!(
        parts.reservation.addr().is_multiple_of(runtime_page_size),
        "reservation ({}) must be aligned to runtime page_size() ({})",
        parts.reservation.addr(),
        runtime_page_size
    );

    // Documented: reservation_len may under-report the actual OS mapping size
    // on hosts where page_size() > PAGE (e.g., 16 KiB macOS). This is acceptable:
    // the crate passes reservation_len directly to munmap on Unix, which rounds
    // its length argument up to the page size the same way mmap did.
    // On Windows, VirtualFree(MEM_RELEASE) ignores the length argument entirely.
    //
    // The critical invariant is that reservation_len is at least sufficient to
    // cover the usable span: reservation_len >= len + (base - reservation).
    // This was verified above.

    // Finally, reconstruct the reservation via the unsafe from_raw_parts path.
    // If any of the verified invariants were wrong, this would panic.
    let expected_base = parts.base;
    let expected_len = parts.len;
    let expected_reservation = parts.reservation;
    let expected_reservation_len = parts.reservation_len;
    let expected_align = parts.align;
    let expected_granted_huge = parts.granted_huge;
    let reconstructed: Reservation = unsafe { parts.into_reservation() };

    // Verify that the reconstruction preserves the key observables.
    assert_eq!(
        reconstructed.as_ptr(),
        expected_base,
        "base should be preserved"
    );
    assert_eq!(reconstructed.len(), expected_len, "len should be preserved");
    assert_eq!(
        reconstructed.reservation_ptr(),
        expected_reservation,
        "reservation_ptr should be preserved"
    );
    assert_eq!(
        reconstructed.reservation_len(),
        expected_reservation_len,
        "reservation_len should be preserved"
    );
    assert_eq!(
        reconstructed.align(),
        expected_align,
        "align should be preserved"
    );
    assert_eq!(
        reconstructed.is_huge(),
        expected_granted_huge,
        "granted_huge should be preserved"
    );

    // The reconstructed reservation is dropped here via RAII.
}

#[test]
fn reconstructed_reservation_is_actually_usable() {
    // Verify that a reconstructed reservation is actually usable for decommit/recommit.
    // This requires a sufficiently large span: at least 2 * page_size() to have a valid
    // non-empty decommit range on ANY host.
    let runtime_page_size = page_size();
    let size = 4 * runtime_page_size;
    let align = runtime_page_size;

    // Reserve using the normal safe API.
    let reservation = reserve_aligned(size, align).expect("reserve_aligned should succeed");

    // Extract and reconstruct.
    let parts: ReservationFullParts = reservation.into_full_parts();
    let expected_base = parts.base;
    let expected_len = parts.len;
    let expected_reservation = parts.reservation;
    let expected_reservation_len = parts.reservation_len;
    let reconstructed: Reservation = unsafe { parts.into_reservation() };

    // Verify reconstruction preserves key fields.
    assert_eq!(reconstructed.as_ptr(), expected_base);
    assert_eq!(reconstructed.len(), expected_len);
    assert_eq!(reconstructed.reservation_ptr(), expected_reservation);
    assert_eq!(reconstructed.reservation_len(), expected_reservation_len);

    // On platforms with reclaim+zero semantics, decommit and recommit a sub-range.
    if Reservation::decommit_reclaims_and_zeroes() {
        let decommit_start = runtime_page_size;
        let decommit_end = 2 * runtime_page_size;

        // This must be a valid non-empty range within bounds on ANY host.
        assert!(
            decommit_end > decommit_start,
            "decommit range must be non-empty"
        );
        assert!(
            decommit_end <= reconstructed.len(),
            "decommit range must fit within len"
        );

        // Decommit the sub-range.
        reconstructed.decommit(decommit_start, decommit_end);

        // Recommit it.
        assert!(
            reconstructed.recommit(decommit_start, decommit_end),
            "recommit should succeed for decommitted range {}..{}",
            decommit_start,
            decommit_end
        );
    }

    // The reconstructed reservation is dropped here via RAII.
}
