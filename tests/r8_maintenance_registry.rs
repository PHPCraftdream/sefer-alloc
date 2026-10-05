//! Real registry leases/scanner: initialized-only discovery, one-step cursor
//! progression, absence of materialization and exclusion of active claimants.
#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::registry::bootstrap::{self, MAX_HEAPS};
use sefer_alloc::registry::heap_registry::{
    dbg_bump_count_without_materialising, dbg_claim_then_simulate_oom,
};
use sefer_alloc::registry::heap_slot::{STATE_FREE, STATE_LIVE, STATE_MAINTENANCE};
use sefer_alloc::registry::HeapRegistry;

#[test]
fn bounded_scan_skips_uninitialized_busy_and_absent_slots() {
    let registry = bootstrap::ensure();
    let failed = dbg_claim_then_simulate_oom().expect("initialization rollback");
    let mut cursor = failed as usize;
    assert_eq!(HeapRegistry::maintenance_pass_for_test(&mut cursor, 1), 0);
    assert_eq!(registry.dbg_slot_state(failed as usize), STATE_FREE);

    let mut owner = HeapRegistry::dbg_claim_lease().expect("claim");
    let index = owner.core().id() as usize;
    assert_eq!(
        index, failed as usize,
        "failed initialization remains claimable"
    );
    cursor = index;
    assert_eq!(HeapRegistry::maintenance_pass_for_test(&mut cursor, 1), 0);
    assert_eq!(registry.dbg_slot_state(index), STATE_LIVE);

    // Never use the owner's core again below.
    drop(owner);
    let lease = HeapRegistry::dbg_try_maintenance().expect("initialized FREE heap");
    assert_eq!(lease.slot_index(), index);
    assert_eq!(registry.dbg_slot_state(index), STATE_MAINTENANCE);
    cursor = index;
    assert_eq!(HeapRegistry::maintenance_pass_for_test(&mut cursor, 1), 0);
    let rival = HeapRegistry::dbg_claim_lease().expect("claim");
    assert_ne!(
        rival.slot_index() as usize,
        index,
        "failed MAINTENANCE claim CAS grants no authority"
    );
    assert_eq!(registry.dbg_slot_state(index), STATE_MAINTENANCE);
    drop(lease);
    cursor = index;
    assert_eq!(HeapRegistry::maintenance_pass_for_test(&mut cursor, 1), 1);
    assert_eq!(registry.dbg_slot_state(index), STATE_FREE);
    assert_eq!(cursor, (index + 1) % bootstrap::count_for_test() as usize);

    // Derive the next chunk boundary from the exposed registry dimensions.
    // Mint absent slots without materializing their chunk, then advance the
    // real scanner across them. A scan must neither call slot() nor spin on
    // an initializer that has not published a chunk.
    let chunk_slots = MAX_HEAPS / bootstrap::dbg_num_chunks();
    assert!(!registry.dbg_chunk_is_materialised(1));
    while bootstrap::count_for_test() as usize <= chunk_slots + 2 {
        dbg_bump_count_without_materialising().expect("minted numeric slot");
    }
    cursor = chunk_slots;
    assert_eq!(HeapRegistry::maintenance_pass_for_test(&mut cursor, 2), 0);
    assert_eq!(cursor, chunk_slots + 2);
    assert!(!registry.dbg_chunk_is_materialised(1));
    let before = cursor;
    assert_eq!(HeapRegistry::maintenance_pass_for_test(&mut cursor, 0), 0);
    assert_eq!(cursor, before);
    drop(rival);
}
