#![cfg(miri)]

use std::alloc::{GlobalAlloc, Layout, System};

use aligned_vmem::{leak_zeroed_pages, release, reserve_aligned, Reservation, PAGE};

fn offset_allocation() -> (*mut u8, *mut u8, Layout) {
    let layout = Layout::from_size_align(3 * PAGE, PAGE).expect("valid layout");
    // SAFETY: The layout is nonzero and valid; ownership passes to the caller.
    let reservation = unsafe { System.alloc(layout) };
    assert!(!reservation.is_null(), "System reservation");
    // SAFETY: The allocation covers three pages, so its second page is in bounds.
    let base = unsafe { reservation.add(PAGE) };
    (base, reservation, layout)
}

#[test]
fn adopted_offset_system_allocation_round_trips_through_drop_and_manual_release() {
    for manual in [false, true] {
        let (base, reservation, layout) = offset_allocation();
        assert_ne!(base, reservation);
        // SAFETY: `reservation` is the exact live System allocation pointer,
        // `layout` is unchanged, and the second page is exclusively owned.
        let adopted = unsafe {
            Reservation::from_raw_parts(base, PAGE, reservation, layout.size(), PAGE, false)
        };
        assert_eq!(adopted.as_ptr(), base);
        assert_eq!(adopted.reservation_ptr(), reservation);
        // SAFETY: The adopted handle exclusively owns this initialized byte.
        unsafe { adopted.as_ptr().write(0x6d) };
        // SAFETY: Same live allocation and exclusive access as above.
        assert_eq!(unsafe { adopted.as_ptr().read() }, 0x6d);

        if manual {
            let (ptr, len, align) = adopted.into_parts();
            assert_eq!((ptr, len, align), (reservation, layout.size(), PAGE));
            // SAFETY: `into_parts` transferred the sole ownership token;
            // this releases the original System pointer with its exact layout.
            unsafe { release(ptr, len, align) };
        } else {
            drop(adopted);
        }
    }
}

#[test]
fn rejected_adoption_preserves_original_system_ownership() {
    let (base, reservation, layout) = offset_allocation();
    let rejected = std::panic::catch_unwind(|| {
        // SAFETY: The underlying allocation is live and exclusively owned;
        // the deliberately short metadata is rejected before ownership transfer.
        unsafe { Reservation::from_raw_parts(base, PAGE, reservation, PAGE, PAGE, false) }
    });
    assert!(
        rejected.is_err(),
        "undersized reservation metadata must be rejected"
    );
    // SAFETY: The rejected constructor did not take ownership. `reservation`
    // is still the original System pointer and `layout` is its exact layout.
    unsafe { System.dealloc(reservation, layout) };
}

#[test]
fn eager_reservation_and_leaked_pages_have_usable_miri_backing() {
    let r = reserve_aligned(PAGE, PAGE).expect("eager System reservation");
    assert_eq!(r.as_ptr(), r.reservation_ptr());
    // SAFETY: The reservation owns at least one writable byte until dropped.
    unsafe { r.as_ptr().write(0xa5) };
    // SAFETY: Same live byte, with no intervening aliasing access.
    assert_eq!(unsafe { r.as_ptr().read() }, 0xa5);
    drop(r);

    let zeroed = leak_zeroed_pages(PAGE + 1).expect("zeroed System reservation");
    // SAFETY: The helper guarantees two rounded-up, initialized zero pages.
    let bytes = unsafe { std::slice::from_raw_parts(zeroed.as_ptr(), 2 * PAGE) };
    assert!(bytes.iter().all(|&byte| byte == 0));
    // SAFETY: Miri's backend allocated this exact two-page layout via System;
    // this test has no remaining use of `bytes` and reclaims the intentional
    // process-lifetime leak solely to satisfy Miri's leak checker.
    unsafe { release(zeroed.as_ptr(), 2 * PAGE, PAGE) };
}

#[cfg(feature = "lazy-commit")]
#[test]
fn lazy_reservation_falls_back_to_fully_usable_system_backing() {
    use aligned_vmem::{lazy_commit_is_honored, reserve_aligned_lazy};

    assert!(!lazy_commit_is_honored());
    // pageguard:allow — Miri-only System backend: its page size is the fixed PAGE constant, not the host page size.
    let r = reserve_aligned_lazy(2 * PAGE, PAGE, PAGE).expect("lazy fallback");
    // SAFETY: Under Miri the whole span, including the nominally lazy tail,
    // is backed by the original two-page System allocation.
    unsafe { r.as_ptr().add(PAGE).write(0x59) };
    // SAFETY: Same live allocation and no intervening mutation.
    assert_eq!(unsafe { r.as_ptr().add(PAGE).read() }, 0x59);
    drop(r);
}

#[cfg(feature = "huge-pages")]
#[test]
fn huge_request_falls_back_to_ordinary_system_backing() {
    use aligned_vmem::reserve_aligned_huge;

    let r = reserve_aligned_huge(PAGE, PAGE).expect("huge fallback");
    assert!(!r.is_huge());
    // SAFETY: Fallback provides a live, exclusively owned page.
    unsafe { r.as_ptr().write(0x37) };
    // SAFETY: Same live page and no intervening mutation.
    assert_eq!(unsafe { r.as_ptr().read() }, 0x37);
    drop(r);
}
