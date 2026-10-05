//! Chunk OOM may recover a materialised FREE slot whose constructor failed.
#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::registry::bootstrap;
use sefer_alloc::registry::heap_registry::{
    dbg_claim_then_simulate_oom, dbg_slot_initialised, HeapRegistry,
};

struct OomReset;

impl Drop for OomReset {
    fn drop(&mut self) {
        bootstrap::dbg_set_inject_chunk_oom(false);
    }
}

#[test]
fn chunk_oom_retries_materialised_uninitialised_free() {
    let old = dbg_claim_then_simulate_oom().expect("first failed constructor");
    let latest = dbg_claim_then_simulate_oom().expect("second failed constructor");
    assert_eq!((old, latest), (0, 1));
    assert!(!dbg_slot_initialised(old));
    let mut held = Vec::with_capacity(64);
    let mut latest_lease = HeapRegistry::dbg_claim_lease().expect("claim");
    // SAFETY-free core access: the lease exclusively owns the slot.
    assert_eq!(latest_lease.core().id(), latest);
    held.push(latest_lease);
    for expected in 2..64 {
        let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
        assert_eq!(lease.core().id(), expected);
        held.push(lease);
    }
    assert_eq!(bootstrap::count_for_test(), 64);
    assert!(!bootstrap::ensure().dbg_chunk_is_materialised(1));

    let _reset = OomReset;
    bootstrap::dbg_set_inject_chunk_oom(true);
    let mut recovered = HeapRegistry::dbg_claim_lease().expect("claim");
    assert!(
        dbg_slot_initialised(recovered.slot_index()),
        "constructor retry must avoid false OOM"
    );
    assert_eq!(recovered.core().id(), old);
    assert!(dbg_slot_initialised(old));
    assert!(!bootstrap::ensure().dbg_chunk_is_materialised(1));
    bootstrap::dbg_set_inject_chunk_oom(false);

    let mut minted_retry = HeapRegistry::dbg_claim_lease().expect("claim");
    assert_eq!(minted_retry.core().id(), 64);
    assert_eq!(bootstrap::count_for_test(), 65);
    // All these leases are still distinct LIVE claims.
    drop(minted_retry);
    drop(recovered);
    for lease in held {
        // This lease has not yet been dropped.
        drop(lease);
    }
}
