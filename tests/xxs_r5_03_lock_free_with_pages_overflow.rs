#![allow(deprecated)]
//! xxs R5-03 (`docs/reviews/2026-09-29-091221-src-review-xxs-sol-round-5.md`):
//! `LockFreeRegion::with_pages` must reject a `page_count` whose total slot
//! count (`page_count * PAGE`) overflows `u32` before allocating. The old code
//! reserved the page table first, then multiplied unchecked (debug: panic, release:
//! wrap to 0 and a broken free list).
//!
//! Boundaries are probed through the pure `checked_total_slots` (test-only
//! forwarder), so nothing large is allocated. The single `with_pages` call uses
//! `usize::MAX`, which `Vec::with_capacity` rejects before touching the
//! allocator either way; pinning the crate's own panic message shows the new
//! check fires first.

#![cfg(all(feature = "experimental", feature = "bench-internals"))]

use sefer_alloc::LockFreeRegion;

const PAGE: usize = 64;
/// `u32::MAX / PAGE`: the largest `page_count` whose slot total fits `u32`.
const LARGEST_VALID_PAGE_COUNT: usize = 67_108_863;
/// The review's example: `page_count * PAGE == 2^32`.
const REVIEW_OVERFLOW_PAGE_COUNT: usize = 67_108_864;

#[test]
fn checked_total_slots_boundary_cases() {
    let check = LockFreeRegion::<u32>::_checked_total_slots_for_tests;
    assert_eq!(check(0, PAGE), Some(0));
    assert_eq!(check(1, PAGE), Some(64));
    assert_eq!(check(LARGEST_VALID_PAGE_COUNT, PAGE), Some(4_294_967_232));
    assert_eq!(check(LARGEST_VALID_PAGE_COUNT + 1, PAGE), None);
    assert_eq!(check(REVIEW_OVERFLOW_PAGE_COUNT, PAGE), None);
    assert_eq!(check(usize::MAX, PAGE), None);
}

#[test]
#[should_panic(expected = "with_pages: page_count * PAGE")]
fn with_pages_rejects_usize_max_before_allocating() {
    let _ = LockFreeRegion::<u32>::with_pages(usize::MAX);
}
