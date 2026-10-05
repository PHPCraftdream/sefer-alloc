//! Ph4b (task #2092, chunk C): bounded miri run of the `HeapLease` teardown
//! path on the MAIN thread (the lease is `!Send`, so — as in Ph4a's
//! `r11_ph4a_lease_miri` — the lifecycle cannot cross a thread boundary):
//! `dbg_claim_lease()` → lease surface reads (slot index, generation) →
//! `drop` (the Release `LIVE → FREE` CAS + hint/claimable publication) →
//! re-claim of the SAME slot via the recycled-slot hint → drop → claim of a
//! SECOND slot. Run under both provenance models (see the CI `miri` /
//! `miri-plain` jobs).
//!
//! Note (honest scope): `HeapLease::core()` and `HeapCore::trim_for_recycle`
//! are `pub(crate)`, so the owner-side core mutation leg of the teardown is
//! NOT reachable from `tests/` through the safe API; this test exercises the
//! observable lease surface (claim authority, slot/generation monotonicity,
//! the drop publication observed by the next claim). The trim leg itself is
//! covered by the native exactly-once tests
//! (`tests/r11_ph4b_late_publication_across_recycle_exactly_once.rs`) and the
//! loom shadow model (`tests/loom_r11_ph4b_publish_recycle_drain.rs`).
//!
//! Run via `cargo +nightly miri test --features "alloc-global alloc-xthread
//! internals" --test r11_ph4b_lease_miri` (and the strict-provenance pass).
#![cfg(all(miri, feature = "alloc-global", feature = "internals"))]

use sefer_alloc::registry::{bootstrap, heap_slot, HeapRegistry};

#[test]
fn lease_teardown_lifecycle_same_thread_under_miri() {
    let reg = bootstrap::ensure();

    // Claim slot 0 on the main thread — this is the full claim authority.
    let first = HeapRegistry::dbg_claim_lease().expect("first claim");
    let index = first.slot_index() as usize;
    let first_gen = first.generation();
    assert_eq!(reg.dbg_slot_state(index), heap_slot::STATE_LIVE);

    // Teardown: HeapLease::drop publishes LIVE → FREE (Release) and plants
    // the last-recycled-slot hint, so the next claim re-claims THIS slot.
    drop(first);
    assert_eq!(reg.dbg_slot_state(index), heap_slot::STATE_FREE);

    // Re-claim via the hint: the Acquire claim CAS must observe the drop's
    // Release publication; the generation is monotone across the recycle.
    let second = HeapRegistry::dbg_claim_lease().expect("re-claim via hint");
    assert_eq!(second.slot_index() as usize, index);
    let second_gen = second.generation();
    assert!(second_gen > first_gen, "generation is monotone per recycle");
    drop(second);
    assert_eq!(reg.dbg_slot_state(index), heap_slot::STATE_FREE);

    // A further claim re-hands the SAME (only) slot via the hint — its third
    // lifecycle leg, still exactly one owner at a time.
    let third = HeapRegistry::dbg_claim_lease().expect("third claim");
    assert_eq!(third.slot_index() as usize, index);
    assert!(third.generation() > second_gen);
    drop(third);
    assert_eq!(reg.dbg_slot_state(index), heap_slot::STATE_FREE);
}
