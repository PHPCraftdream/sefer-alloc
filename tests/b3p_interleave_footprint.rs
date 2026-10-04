//! Ph3c step 1′ (B3 per-class carve cursors) — interleaved footprint.
//!
//! Counterfactual footprint guard: interleaving the six smallest classes
//! (16/32/48/64/80/112 B round-robin) through `AllocCore` with NO frees must
//! consume at most ONE small segment. Counterfactually red on spike 093c9d23
//! (W1 C/B=4.013): the pre-fix carve cursor let runs be taken misaligned/
//! overlapping, forcing repeated small-segment reserves at the same volume.
//!
//! Segment count via `AllocCore::dbg_table_count` (the segment table's
//! high-water registered slot count): the delta from a fresh core must be 0.
#![cfg(all(feature = "production", feature = "internals", feature = "bench-internals"))]

use sefer_alloc::{AllocCore, SegmentLayout};

#[test]
fn interleave_40k_small_allocs_single_segment() {
    let sizes: [usize; 6] = [16, 32, 48, 64, 80, 112];
    // ~58.7 B avg * 40 000 ≈ 2.3 MiB of issued bytes: fits one 4 MiB segment.
    const N: usize = 40_000;

    let mut core = AllocCore::new().expect("AllocCore::new");
    let base_count = core.dbg_table_count();

    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let size = sizes[i % sizes.len()];
        let layout = core::alloc::Layout::from_size_align(size, 8).unwrap();
        // SAFETY: non-zero size, valid alignment; freed when `core` drops.
        let ptr = core.alloc(layout);
        assert!(!ptr.is_null(), "alloc null at i={i} (size {size})");
        ptrs.push(ptr);
    }
    drop(ptrs);

    let after = core.dbg_table_count();
    assert_eq!(
        after, base_count,
        "interleaved small carve footprint reserved extra segments: {} -> {}",
        base_count, after
    );
    // Sanity: the segment actually served the full volume (payload capacity).
    assert!(N * sizes.iter().sum::<usize>() / sizes.len() <= SegmentLayout::SEGMENT);
}
