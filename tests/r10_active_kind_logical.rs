#![cfg(all(feature = "alloc-core", feature = "internals"))]

use sefer_alloc::alloc_core::SegmentHashHarness;

fn check(h: &SegmentHashHarness, small: &[bool; 4096], large: &[bool; 4096]) {
    for from in [0, 1, 62, 63, 64, 65, 126, 127, 128, 4094, 4095, 4096] {
        assert_eq!(
            h.active_next(from, false),
            (from..4096).find(|&i| small[i]),
            "small from {from}"
        );
        assert_eq!(
            h.active_next(from, true),
            (from..4096).find(|&i| large[i]),
            "large from {from}"
        );
    }
}

#[test]
fn word_boundaries_summary_empty_and_kind_reuse() {
    let mut h = SegmentHashHarness::new();
    let mut small = [false; 4096];
    let mut large = [false; 4096];
    small[0] = true;
    check(&h, &small, &large);
    for slot in [63, 64, 127, 4095] {
        h.active_set(slot, true);
        large[slot] = true;
        check(&h, &small, &large);
    }
    for slot in [64, 63, 4095, 127] {
        h.active_clear(slot, true);
        large[slot] = false;
        check(&h, &small, &large);
        h.active_set(slot, false);
        small[slot] = true;
        check(&h, &small, &large);
        h.active_clear(slot, false);
        small[slot] = false;
        check(&h, &small, &large);
    }
    assert_eq!(h.active_next(0, false), Some(0));
    assert_eq!(h.active_next(0, true), None);
}

#[test]
fn oracle_rejects_clean_bit_clear_mutant() {
    let mut h = SegmentHashHarness::new();
    let mut small = [false; 4096];
    let mut large = [false; 4096];
    small[0] = true;
    h.active_set(1, true);
    large[1] = true;
    check(&h, &small, &large);
    h.active_clear(1, true); // Mutant: no unregister occurred.
                             // The closure only reads the fixture; unwinding cannot expose partial state.
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check(&h, &small, &large)
    }))
    .is_err());
}

#[test]
fn unknown_kind_is_rejected_before_slot_and_index_mutation() {
    let mut h = SegmentHashHarness::new();
    assert!(h.unknown_registration_is_unchanged());
}
