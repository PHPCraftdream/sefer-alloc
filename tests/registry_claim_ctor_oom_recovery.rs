//! R6-T03: a constructor OOM must not monopolise `reuse_hint`, and the claim
//! loop must keep going through the recovery scan instead of giving up.
#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::registry::bootstrap::{self, MAX_HEAPS};
use sefer_alloc::registry::heap_registry::HeapRegistry;

#[test]
fn ctor_oom_releases_slot_to_bitmap_not_hint_and_recovers() {
    // Fresh process: the first claim mints slot 0 and holds it LIVE.
    let mut lease0 = HeapRegistry::dbg_claim_lease().expect("first claim");
    assert_eq!(lease0.slot_index(), 0);

    // Second claim: hint/bitmap are quiet, so it bumps slot 1 — whose
    // constructor now fails by injection.
    HeapRegistry::dbg_set_inject_ctor_oom(true);
    let failed = HeapRegistry::dbg_claim_lease();
    HeapRegistry::dbg_set_inject_ctor_oom(false);
    assert!(failed.is_none(), "constructor OOM surfaces as None");

    // Counterfactual (pre-R6-T03): `push_back_after_oom` wrote the failed
    // slot into `reuse_hint`, so the hint read 1 here and the bitmap stayed
    // empty — both asserts are red on the old code.
    assert_eq!(HeapRegistry::dbg_reuse_hint(), MAX_HEAPS as u32);
    assert!(HeapRegistry::dbg_free_slot_hint_popcount() >= 1);

    // The claim loop's recovery scan excludes the failed slot, finds nothing
    // initialised (slot 0 is LIVE), and gives up; the NEXT claim retries the
    // published slot and its constructor now succeeds.
    let mut lease1 = HeapRegistry::dbg_claim_lease().expect("retry after ctor OOM");
    assert_eq!(lease1.slot_index(), 1);
    assert!(sefer_alloc::registry::heap_registry::dbg_slot_initialised(
        1
    ));
    drop(lease1);
    drop(lease0);
}
