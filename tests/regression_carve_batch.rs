//! Regression / equivalence test — E1 (task W4): `AllocCore::carve_batch`.
//!
//! `carve_batch` carves a RUN of up-to-`out.len()` blocks from the current
//! small segment's bump cursor in ONE shot, hoisting the per-block `align_up`
//! div, the `bump` load/store, the `live` increment, the `is_decommitted`
//! check, and the page-map marking (per-DISTINCT-page) across the run. It must
//! be BYTE-IDENTICAL to `n` sequential `carve_block` calls.
//!
//! This test drives `carve_batch` directly (via the `dbg_carve_batch` seam) and
//! asserts the block set is well-formed, and that a mixed drain+carve refill
//! (through the live `refill_class_bump` path) round-trips cleanly — the same
//! guarantee the pre-E1 per-block carve loop gave.
//!
//! Counterfactual: if `carve_batch` mis-strided (e.g. forgot to re-align the
//! start, double-counted `live`, or marked the wrong page class), the distinct
//! / strided / page-class / round-trip assertions below fail.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::{AllocCore, SegmentLayout};

fn class_for(core: &AllocCore, size: usize, align: usize) -> usize {
    let layout = Layout::from_size_align(size, align).unwrap();
    core.dbg_layout_class_for(layout)
        .expect("expected a small class")
}

/// A batch carved from a fresh core yields N DISTINCT, `block_size`-strided
/// pointers, all inside the segment, each page-map-classified to `class_idx`.
fn carve_batch_wellformed_inner(size: usize, align: usize, n: usize) {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, size, align);
    let block_size = AllocCore::dbg_block_size(c);

    let mut buf = vec![core::ptr::null_mut::<u8>(); n];
    let got = core.dbg_carve_batch(c, &mut buf);
    assert_eq!(got, n, "carve_batch filled {got}, expected {n}");

    // 1. All non-null and DISTINCT.
    for (i, &p) in buf.iter().enumerate() {
        assert!(!p.is_null(), "buf[{i}] null");
    }
    let uniq: HashSet<usize> = buf.iter().map(|p| *p as usize).collect();
    assert_eq!(uniq.len(), n, "carve_batch returned duplicate pointers");

    // 2. Strictly `block_size`-strided within ONE segment (a bump run), and
    //    every pointer lies inside its segment (< base + SEGMENT).
    let base = SegmentLayout::segment_base_of(buf[0] as usize);
    let mut addrs: Vec<usize> = buf.iter().map(|p| *p as usize).collect();
    addrs.sort_unstable();
    for w in addrs.windows(2) {
        assert_eq!(
            w[1] - w[0],
            block_size,
            "blocks are not contiguously block_size-strided"
        );
    }
    for &p in &buf {
        assert_eq!(
            SegmentLayout::segment_base_of(p as usize),
            base,
            "carve run spanned more than one segment"
        );
        assert!(
            (p as usize) < base + SegmentLayout::SEGMENT,
            "block escaped the segment payload"
        );
    }

    // 3. Every carved block's page is dedicated to class `c` (page-map "first
    //    class wins", applied per distinct page by carve_batch).
    for &p in &buf {
        assert_eq!(
            core.dbg_page_map_class_for(p),
            Some(c),
            "carved block's page not classified to its class"
        );
    }

    // 4. Round-trip: every carved block frees cleanly and a following alloc of
    //    the same class does not corrupt (M2 bitmap intact — a carve must leave
    //    bit0=allocated; if carve_batch had wrongly touched the bitmap, the free
    //    below would be swallowed and re-alloc would hand a different set).
    let layout = Layout::from_size_align(size, align).unwrap();
    for &p in &buf {
        core.dealloc(p, layout);
    }
    // Re-alloc the same count through the public path; must reuse the freed set.
    let mut reused: HashSet<usize> = HashSet::new();
    let mut again: Vec<*mut u8> = Vec::new();
    for _ in 0..n {
        let p = core.alloc(layout);
        assert!(!p.is_null());
        reused.insert(p as usize);
        again.push(p);
    }
    assert_eq!(
        reused, uniq,
        "freed carve_batch blocks were not reused by the next allocs"
    );
    for p in again {
        core.dealloc(p, layout);
    }
}

#[test]
fn carve_batch_wellformed_16b_n16() {
    carve_batch_wellformed_inner(16, 8, 16);
}

#[test]
fn carve_batch_wellformed_16b_n256() {
    // 256 * 16 B = one page's worth of blocks; exercises the per-distinct-page
    // marking (255/256 same-page, then a page change).
    carve_batch_wellformed_inner(16, 8, 256);
}

#[test]
fn carve_batch_wellformed_64b_n64() {
    carve_batch_wellformed_inner(64, 8, 64);
}

#[test]
fn carve_batch_wellformed_medium_n8() {
    carve_batch_wellformed_inner(256, 8, 8);
}

/// A short `out` slice fills exactly `out.len()`; an empty slice fills nothing.
#[test]
fn carve_batch_len_bound() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let mut empty: [*mut u8; 0] = [];
    assert_eq!(core.dbg_carve_batch(c, &mut empty), 0);
    let mut buf = vec![core::ptr::null_mut::<u8>(); 5];
    assert_eq!(core.dbg_carve_batch(c, &mut buf), 5);
}

/// Mixed drain+carve refill (the E1 rewire in `refill_class_bump`): free some
/// blocks so a later refill drains them FIRST (source order), then bump-carves
/// the remainder via carve_batch. The result must be N distinct blocks that all
/// round-trip — proving the drain-before-carve ordering survived the E1 rewrite.
#[test]
fn mixed_drain_then_carve_refill_roundtrips() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Prime: carve 10 blocks, free 4 of them → 4 sit on the BinTable free list.
    let mut primed = vec![core::ptr::null_mut::<u8>(); 10];
    assert_eq!(core.refill_class_bump(c, &mut primed), 10);
    let freed: HashSet<usize> = primed[..4].iter().map(|p| *p as usize).collect();
    for &p in &primed[..4] {
        core.dealloc(p, layout);
    }

    // Refill 12: the first up-to-4 come from the drained free list, the rest
    // are bump-carved via carve_batch. All 12 distinct, all round-trip.
    let mut buf = vec![core::ptr::null_mut::<u8>(); 12];
    let got = core.refill_class_bump(c, &mut buf);
    assert_eq!(got, 12);
    let set: HashSet<usize> = buf.iter().map(|p| *p as usize).collect();
    assert_eq!(set.len(), 12, "mixed refill produced duplicates");
    // The 4 freed blocks must have been reused (drain-before-carve).
    assert!(
        freed.is_subset(&set),
        "drained free blocks were not reused first (source order broken)"
    );
    for &p in &buf {
        core.dealloc(p, layout);
    }
    // The still-held 6 primed blocks also free cleanly (no corruption).
    for &p in &primed[4..] {
        core.dealloc(p, layout);
    }
}
