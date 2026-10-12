//! Winning FREE->LIVE through the reuse hint must clear the slot's free-set bit.
#![cfg(all(feature = "alloc-global", feature = "internals"))]
#![forbid(unsafe_code)]

use sefer_alloc::registry::{bootstrap, HeapRegistry};

#[test]
fn claim_win_clears_the_free_set_bit() {
    let reg = bootstrap::ensure();
    let first = HeapRegistry::dbg_claim_lease().expect("claim");
    let index = first.slot_index() as usize;
    let live_has_hint = reg.dbg_free_slot_hint_contains(index);
    drop(first);
    let freed_has_hint = reg.dbg_free_slot_hint_contains(index);
    // The reuse hint names the same slot, so the hint path wins it back.
    let second = HeapRegistry::dbg_claim_lease().expect("claim");
    let reclaimed_index = second.slot_index() as usize;
    let reclaimed_has_hint = reg.dbg_free_slot_hint_contains(index);
    drop(second);
    let refreed_has_hint = reg.dbg_free_slot_hint_contains(index);
    // Assert only after every lease is dropped: HeapLease::drop aborts on unwind.
    assert!(!live_has_hint, "a LIVE slot has no hint");
    assert!(freed_has_hint, "Drop publishes the FREE slot");
    assert_eq!(
        reclaimed_index, index,
        "the reuse hint must win the slot back"
    );
    assert!(
        !reclaimed_has_hint,
        "a claimed slot must not keep a stale free-set bit"
    );
    assert!(refreed_has_hint, "a second Drop publishes it again");
}
