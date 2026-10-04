//! Ph3c step 1′ (B3 per-class carve cursors) — leaf-boundary classes.
//!
//! Classes whose block size does NOT divide 4096 (48, 80, 112, 144, 1536,
//! 1888, 2368, 2560, 3072) allocate enough blocks to exhaust several carve
//! runs and cross run boundaries mid-batch. Every issued block is checked
//! by ADDRESS against the segment substrate:
//!   (a) blocks of a class are pairwise disjoint `[ptr, ptr+bs)` intervals;
//!   (b) each block's segment offset is a multiple of its block size;
//!   (c) EVERY 4 KiB sub-leaf the block touches is bound to its own class
//!       (`leaf_class == class_idx + 1`, 0 = unbound must never appear).
//! Note a block MAY straddle a sub-leaf boundary inside its run (runs are
//! contiguous k leaves); it must never leave the leaves of its own run,
//! which (c) proves.
#![cfg(all(feature = "production", feature = "internals", feature = "bench-internals"))]

use sefer_alloc::{AllocCore, SegmentLayout};

const LEAF: usize = 4096;

/// Desired boundary sizes; each rounds UP to its actual size class.
const LEAF_BOUNDARY_SIZES: [usize; 9] = [48, 80, 112, 144, 1536, 1888, 2368, 2560, 3072];

/// Enough blocks to take several runs per class (max k = 4 leaves/run).
fn count_for(bs: usize) -> usize {
    (4 * LEAF * 4 / bs).max(64) + 16
}

#[test]
fn leaf_boundary_blocks_are_aligned_bound_and_disjoint() {
    let table = SegmentLayout::SIZE_CLASS_TABLE;
    let segment_mask = !(SegmentLayout::SEGMENT - 1);

    // Desired sizes round UP to their actual size class (some, e.g. 1536,
    // are not exact table entries); duplicate classes are dropped.
    let mut class_list: Vec<usize> = Vec::new();
    for &bs in &LEAF_BOUNDARY_SIZES {
        let class = SegmentLayout::try_class_for(bs, 8)
            .ok()
            .flatten()
            .expect("class");
        if !class_list.contains(&class) {
            class_list.push(class);
        }
    }

    let mut core = AllocCore::new().expect("AllocCore::new");
    let mut by_class: Vec<Vec<usize>> = vec![Vec::new(); class_list.len()];

    for (ci, &class) in class_list.iter().enumerate() {
        let bs = table[class];
        let layout = core::alloc::Layout::from_size_align(bs, 8).unwrap();
        for _ in 0..count_for(bs) {
            // SAFETY: non-zero size, valid alignment; core outlives checks.
            let ptr = core.alloc(layout);
            assert!(!ptr.is_null(), "alloc null (bs {bs})");
            let addr = ptr as usize;
            let base = addr & segment_mask;
            let off = addr - base;

            // (b) block offset is block-size aligned within the segment.
            assert_eq!(off % bs, 0, "bs {bs}: offset {off} misaligned");

            // (c) every sub-leaf touched by the block is bound to this class.
            let first_leaf = off / LEAF;
            let last_leaf = (off + bs - 1) / LEAF;
            for leaf in first_leaf..=last_leaf {
                let lc = sefer_alloc::b3p_leaf_class_of((base + leaf * LEAF) as *mut u8);
                assert_eq!(
                    lc,
                    (class + 1) as u8,
                    "bs {bs}: leaf {leaf} bound to {lc}, expected {}",
                    class + 1
                );
            }
            by_class[ci].push(addr);
        }
    }

    // (a) pairwise disjoint intervals per class.
    for (ci, &class) in class_list.iter().enumerate() {
        let bs = table[class];
        let mut addrs = by_class[ci].clone();
        addrs.sort_unstable();
        addrs.dedup();
        assert_eq!(addrs.len(), by_class[ci].len(), "bs {bs}: duplicate block address");
        for w in addrs.windows(2) {
            assert!(
                w[0] + bs <= w[1],
                "bs {bs}: blocks [0x{:x}, 0x{:x}) and 0x{:x} overlap",
                w[0],
                w[0] + bs,
                w[1]
            );
        }
    }
}
