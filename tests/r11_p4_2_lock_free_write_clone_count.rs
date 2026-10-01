//! R11 P4-2 — a successful `LockFreeRegion` write must copy O(log P) page-table
//! nodes, not all P page `Arc`s.
//!
//! Witness: count page-`Arc` clones (per-thread counter behind
//! `bench-internals`) for ONE insert and ONE remove touching the same slot, at
//! P = 16 / 256 / 4096 pages. Before the fix the count equals P; after, it is
//! bounded by the table's fan-out independent of P.

#![allow(deprecated)]
#![cfg(all(feature = "experimental", feature = "bench-internals"))]

use sefer_alloc::LockFreeRegion;

/// Upper bound on page-`Arc` clones per write: one leaf node's fan-out.
const FANOUT: usize = 16;

type R = LockFreeRegion<u32>;

/// (insert clones, remove clones) for the slot at global index 0.
fn clones_for(pages: usize) -> (usize, usize) {
    let r = R::with_pages(pages);
    R::_reset_page_arc_clones_for_tests();
    let h = r.insert(7);
    let ins = R::_page_arc_clones_for_tests();
    R::_reset_page_arc_clones_for_tests();
    assert_eq!(r.remove(h).as_deref(), Some(&7));
    let rem = R::_page_arc_clones_for_tests();
    (ins, rem)
}

#[test]
fn write_page_arc_clones_do_not_grow_with_page_count() {
    let mut rows = Vec::new();
    for p in [16usize, 256, 4096] {
        let (ins, rem) = clones_for(p);
        rows.push((p, ins, rem));
    }
    for &(p, ins, rem) in &rows {
        assert!(
            ins <= FANOUT && rem <= FANOUT,
            "P={p}: insert cloned {ins} page Arcs, remove {rem} (bound {FANOUT}); rows={rows:?}"
        );
    }
    // Same touched page => identical cost regardless of P.
    assert_eq!(rows[1].1, rows[0].1, "insert cost depends on P: {rows:?}");
    assert_eq!(rows[2].2, rows[0].2, "remove cost depends on P: {rows:?}");
}

#[test]
fn growing_insert_page_arc_clones_do_not_grow_with_page_count() {
    // Fill every slot so the next insert must append a page.
    let mut counts = Vec::new();
    for p in [16usize, 256] {
        let r = R::with_pages(p);
        for i in 0..(p * 64) {
            r.insert(u32::try_from(i).unwrap());
        }
        R::_reset_page_arc_clones_for_tests();
        let _ = r.insert(0);
        counts.push((p, R::_page_arc_clones_for_tests()));
    }
    for &(p, c) in &counts {
        assert!(
            c <= FANOUT,
            "P={p}: growing insert cloned {c} (bound {FANOUT}); {counts:?}"
        );
    }
}

#[test]
fn page_table_growth_across_height_boundaries_keeps_handles_valid() {
    // 17_000 slots > 256 pages * 64: crosses the trie's height 0 -> 1 -> 2 steps.
    let r = LockFreeRegion::<usize>::new();
    let hs: Vec<_> = (0..17_000usize).map(|i| (i, r.insert(i))).collect();
    assert_eq!(r.len(), 17_000);
    for (i, h) in &hs {
        assert_eq!(r.get(*h).as_deref(), Some(i));
    }
    for (i, h) in hs.iter().filter(|(i, _)| i % 2 == 0) {
        assert_eq!(r.remove(*h).as_deref(), Some(i));
        assert!(!r.contains(*h), "stale handle must not resolve");
    }
    let fresh: Vec<_> = (0..8_500usize)
        .map(|i| (i + 1_000_000, r.insert(i + 1_000_000)))
        .collect();
    assert_eq!(r.len(), 17_000);
    for (i, h) in &fresh {
        assert_eq!(r.get(*h).as_deref(), Some(i));
    }
    for (i, h) in hs.iter().filter(|(i, _)| i % 2 == 1) {
        assert_eq!(r.get(*h).as_deref(), Some(i));
    }
}

#[test]
fn with_pages_bulk_table_matches_incremental_lookup() {
    let r = LockFreeRegion::<u32>::with_pages(300);
    let hs: Vec<_> = (0..300u32 * 64).map(|i| (i, r.insert(i))).collect();
    for (i, h) in &hs {
        assert_eq!(r.get(*h).as_deref(), Some(i));
    }
    // Full: next insert appends page 300 (crosses 256-page boundary on bulk-built root).
    let h = r.insert(u32::MAX);
    assert_eq!(r.get(h).as_deref(), Some(&u32::MAX));
}
