#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::registry::heap_registry::{dbg_claim_then_simulate_oom, dbg_slot_initialised};
use sefer_alloc::registry::heap_slot::{STATE_EMPTY, STATE_FREE, STATE_LIVE, STATE_MAINTENANCE};
use sefer_alloc::registry::{bootstrap, HeapRegistry};

#[test]
fn failed_init_claim_maintenance_recycle_handoff() {
    let index = dbg_claim_then_simulate_oom().expect("fresh index");
    let registry = bootstrap::ensure();
    assert_eq!(registry.dbg_slot_state(index as usize), STATE_EMPTY);
    assert!(!dbg_slot_initialised(index));

    let owner = HeapRegistry::claim();
    assert!(!owner.is_null());
    // SAFETY: claim returned a live core; no other owner holds this slot.
    assert_eq!(unsafe { (*owner).id() }, index);
    assert_eq!(registry.dbg_slot_state(index as usize), STATE_LIVE);
    assert!(dbg_slot_initialised(index));

    // SAFETY: this owner was returned by claim and has not been recycled.
    unsafe { HeapRegistry::recycle(owner) };
    assert_eq!(registry.dbg_slot_state(index as usize), STATE_FREE);

    let mut lease = HeapRegistry::try_maintenance().expect("recycled slot");
    assert_eq!(lease.slot_index(), index as usize);
    assert_eq!(registry.dbg_slot_state(index as usize), STATE_MAINTENANCE);
    // SAFETY: no legacy raw core alias is used after recycle and this test
    // has no remote producer touching the core.
    assert_eq!(unsafe { lease.with_core(|core| core.id()) }, index);

    let other = HeapRegistry::claim();
    assert!(!other.is_null());
    assert_ne!(other, owner, "MAINTENANCE must defeat a claim CAS");
    // SAFETY: this is the sole claim of `other`, still live.
    unsafe { HeapRegistry::recycle(other) };
    drop(lease);
    assert_eq!(registry.dbg_slot_state(index as usize), STATE_FREE);
    assert!(!registry.dbg_chunk_is_materialised(1));
}
