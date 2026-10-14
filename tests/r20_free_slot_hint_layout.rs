//! Layout contracts for the production bitmap and global registry.
#![cfg(all(feature = "alloc-global", feature = "internals"))]
#![forbid(unsafe_code)]

const MAX_HEAPS: usize = 4096;
#[allow(dead_code, unused_imports)]
#[path = "../src/registry/bootstrap/free_slot_hint.rs"]
mod real_free_slot_hint;
#[allow(dead_code, unused_imports)]
#[path = "../src/registry/bootstrap/saturation.rs"]
mod real_saturation;
use real_free_slot_hint::FreeSlotHint;
use sefer_alloc::registry::bootstrap::Registry;
use std::mem::{align_of, size_of};

#[test]
fn free_slot_hint_is_aligned_and_padded() {
    assert_eq!(align_of::<FreeSlotHint>(), 64);
    assert_eq!(size_of::<FreeSlotHint>(), 576);
    assert_eq!(size_of::<FreeSlotHint>() % 64, 0);
    assert!(size_of::<FreeSlotHint>() >= (MAX_HEAPS / 64 + 1) * size_of::<u64>());
}

#[cfg(target_pointer_width = "64")]
#[test]
fn separate_free_bitmap_does_not_grow_registry_from_baseline() {
    use sefer_alloc::registry::bootstrap::MAX_HEAPS;
    use std::mem::size_of;

    // Baseline Registry fields: MAX_HEAPS / CHUNK_SLOTS pointers, count,
    // scan_cursor, reuse_hint, then aligned SaturationHint(AtomicU64).
    const NUM_CHUNKS: usize = MAX_HEAPS / 64;
    let controls_end = NUM_CHUNKS * size_of::<usize>() + 3 * size_of::<u32>();
    let saturation_offset = (controls_end + size_of::<u64>() - 1) & !(size_of::<u64>() - 1);
    let baseline_size = saturation_offset + size_of::<u64>();
    assert_eq!(size_of::<Registry>(), baseline_size);

    let bitmap = FreeSlotHint::new();
    bitmap.publish(23);
    assert_eq!(bitmap.take_nonempty(), Some(23));
    assert_eq!(bitmap.take_nonempty(), None);
}
