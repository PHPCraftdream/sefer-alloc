#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::registry::heap_registry::{dbg_claim_then_simulate_oom, dbg_slot_initialised};
use sefer_alloc::registry::heap_slot::{STATE_FREE, STATE_LIVE, STATE_MAINTENANCE};
use sefer_alloc::registry::{bootstrap, HeapRegistry};

#[test]
fn failed_init_claim_maintenance_recycle_handoff() {
    let index = dbg_claim_then_simulate_oom().expect("fresh index");
    let registry = bootstrap::ensure();
    assert_eq!(registry.dbg_slot_state(index as usize), STATE_FREE);
    assert!(!dbg_slot_initialised(index));

    let mut owner = HeapRegistry::dbg_claim_lease().expect("claim");
    assert_eq!(owner.core().id(), index);
    assert_eq!(registry.dbg_slot_state(index as usize), STATE_LIVE);
    assert!(dbg_slot_initialised(index));

    drop(owner);
    assert_eq!(registry.dbg_slot_state(index as usize), STATE_FREE);

    let mut lease = HeapRegistry::dbg_try_maintenance().expect("recycled slot");
    assert_eq!(lease.slot_index(), index as usize);
    assert_eq!(registry.dbg_slot_state(index as usize), STATE_MAINTENANCE);
    // SAFETY: no legacy raw core alias is used after recycle and this test
    // has no remote producer touching the core.
    assert_eq!(lease.dbg_with_core(|core| core.id()), index);

    let other = HeapRegistry::dbg_claim_lease().expect("claim");
    assert_ne!(
        other.slot_index(),
        index,
        "MAINTENANCE must defeat a claim CAS"
    );
    drop(other);
    drop(lease);
    assert_eq!(registry.dbg_slot_state(index as usize), STATE_FREE);
    assert!(!registry.dbg_chunk_is_materialised(1));
}
