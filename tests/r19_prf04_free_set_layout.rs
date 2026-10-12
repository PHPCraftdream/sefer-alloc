//! CACHE-LINE layout of FREE-slot hint sets: 64-byte alignment, no line sharing.
#![cfg(all(feature = "alloc-global", feature = "internals"))]
#![forbid(unsafe_code)]

use core::mem::{align_of, size_of};
use sefer_alloc::registry::bootstrap::{FreeSlotHint, MAX_HEAPS};

#[test]
fn cache_line_aligned() {
    assert_eq!(align_of::<FreeSlotHint>(), 64);
    assert_eq!(size_of::<FreeSlotHint>(), 576);
}

#[test]
fn words_fit_within_aligned_size() {
    // 1 summary word + MAX_HEAPS/64 bitmap words, rounded up to whole lines.
    assert!(size_of::<FreeSlotHint>() >= (MAX_HEAPS / 64 + 1) * 8);
    assert_eq!(size_of::<FreeSlotHint>() % 64, 0);
}
