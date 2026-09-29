//! Once materialised, a mostly empty 40-slot cache must still service a
//! narrow working set without losing entries or miscounting cached bytes.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "large-cache-extended",
    feature = "internals"
))]

#[path = "support/r6_bounded_large_cache.rs"]
mod bounded;

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

fn materialise_then_empty(ac: &mut AllocCore) -> Vec<(*mut u8, Layout)> {
    ac.dbg_set_large_cache_budget(None);
    let bytes = bounded::large_request();
    let live = bounded::allocate_live(ac, &[bytes; 9]);
    bounded::deposit_all(ac, live);
    assert!(ac.dbg_large_cache_extension_materialised());
    assert_eq!(bounded::occupied(ac), (8, 1));

    let held = bounded::allocate_live(ac, &[bytes; 9]);
    assert_eq!(bounded::occupied(ac), (0, 0));
    held
}

fn narrow_working_set_after_materialisation_is_correct(n: usize) {
    let mut ac = AllocCore::new().expect("primordial");
    let held = materialise_then_empty(&mut ac);
    let floor = bounded::large_request();
    let sizes: Vec<_> = (0..n).map(|i| floor + i * SegmentLayout::SEGMENT).collect();

    let warm = bounded::allocate_live(&mut ac, &sizes);
    bounded::deposit_all(&mut ac, warm);
    let physical: Vec<_> = ac.dbg_large_cache_slot_sizes()[..n]
        .iter()
        .map(|s| s.expect("warm deposit occupies base slot"))
        .collect();
    assert!(physical.windows(2).all(|w| w[0] < w[1]));

    let live = bounded::allocate_live(&mut ac, &sizes);
    assert_eq!(bounded::occupied(&ac), (0, 0));
    bounded::assert_used(&ac);
    let ptrs: Vec<_> = live.iter().map(|(p, _)| *p).collect();
    bounded::deposit_all(&mut ac, live);
    assert_eq!(bounded::occupied(&ac), (n, 0));
    bounded::assert_used(&ac);

    let again = bounded::allocate_live(&mut ac, &sizes);
    assert_eq!(bounded::occupied(&ac), (0, 0));
    for (p, _) in &again {
        assert!(
            ptrs.contains(p),
            "narrow-set hit must reuse a resident pointer"
        );
    }
    bounded::deposit_all(&mut ac, again);
    assert_eq!(ac.dbg_large_cache_total_slots(), 40);
    bounded::deposit_all(&mut ac, held);
}

#[test]
fn narrow_working_set_n1_after_materialisation_is_correct() {
    narrow_working_set_after_materialisation_is_correct(1);
}

#[test]
fn narrow_working_set_n2_after_materialisation_is_correct() {
    narrow_working_set_after_materialisation_is_correct(2);
}

#[test]
fn narrow_working_set_n4_after_materialisation_is_correct() {
    narrow_working_set_after_materialisation_is_correct(4);
}

#[test]
fn scan_bound_stays_forty_during_narrow_working_set_phase() {
    let mut ac = AllocCore::new().expect("primordial");
    let held = materialise_then_empty(&mut ac);
    let bytes = bounded::large_request();
    let live = bounded::allocate_live(&mut ac, &[bytes]);
    assert_eq!(ac.dbg_large_cache_total_slots(), 40);
    bounded::deposit_all(&mut ac, live);
    assert_eq!(ac.dbg_large_cache_total_slots(), 40);
    bounded::deposit_all(&mut ac, held);
}
