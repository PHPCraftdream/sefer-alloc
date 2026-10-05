//! Real registry chunk failure with a usable, older FREE heap.
#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::registry::bootstrap;
use sefer_alloc::registry::heap_registry::HeapRegistry;
use sefer_alloc::registry::heap_slot::{STATE_FREE, STATE_LIVE};

struct OomReset;

impl Drop for OomReset {
    fn drop(&mut self) {
        bootstrap::dbg_set_inject_chunk_oom(false);
    }
}

#[test]
fn failed_new_chunk_recovers_older_free_then_retries_minted_index() {
    let reg = bootstrap::ensure();
    let mut held = Vec::with_capacity(64);
    for expected in 0..64 {
        let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
        assert_eq!(lease.core().id(), expected);
        held.push(lease);
    }
    assert!(!reg.dbg_chunk_is_materialised(1));
    // Two releases overwrite the one-entry hint. Reclaim the latest one,
    // leaving an older FREE heap with no hint while fresh capacity remains.
    let (old0, old1) = (held[0].slot_index(), held[1].slot_index());
    for _ in 0..2 {
        // Each lease is a distinct live claim; its Drop recycles the slot.
        drop(held.remove(0));
    }
    let latest = HeapRegistry::dbg_claim_lease().expect("claim");
    assert_eq!(latest.slot_index(), old1);
    assert_eq!(reg.dbg_slot_state(0), STATE_FREE);
    assert_eq!(reg.dbg_slot_state(1), STATE_LIVE);

    let _reset = OomReset;
    bootstrap::dbg_set_inject_chunk_oom(true);
    let recovered = HeapRegistry::dbg_claim_lease().expect("claim");
    assert_eq!(
        recovered.slot_index(),
        old0,
        "cold OOM scan must find older FREE"
    );
    assert_eq!(bootstrap::count_for_test(), 65);
    assert!(!reg.dbg_chunk_is_materialised(1));
    bootstrap::dbg_set_inject_chunk_oom(false);

    let mut retried = HeapRegistry::dbg_claim_lease().expect("claim");
    // SAFETY-free core access: the lease exclusively owns the slot.
    assert_eq!(retried.core().id(), 64);
    assert!(reg.dbg_chunk_is_materialised(1));
    assert_eq!(bootstrap::count_for_test(), 65);

    // These three live leases have not been dropped yet.
    drop(retried);
    drop(recovered);
    drop(latest);
    for lease in held.into_iter().skip(2) {
        // Each remaining lease is still a distinct live claim.
        drop(lease);
    }

    let lease = HeapRegistry::dbg_try_maintenance().expect("materialized FREE heap");
    let leased_index = lease.slot_index() as u32;
    drop(lease);
    let mut after_lease = HeapRegistry::dbg_claim_lease().expect("claim");
    assert_eq!(after_lease.core().id(), leased_index);
    // The lease Drop recycles the slot; the claim has not been recycled yet.
    drop(after_lease);
}
