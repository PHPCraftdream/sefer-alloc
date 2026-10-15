//! Constructor OOM leaves materialised slots available for later claims.
#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::registry::bootstrap;
use sefer_alloc::registry::heap_registry::{
    dbg_claim_then_simulate_oom, dbg_slot_initialised, HeapRegistry,
};
use sefer_alloc::registry::heap_slot::STATE_FREE;

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
    assert_eq!(bootstrap::ensure().dbg_slot_state(old as usize), STATE_FREE);
    assert_eq!(
        bootstrap::ensure().dbg_slot_state(latest as usize),
        STATE_FREE
    );
    assert!(!dbg_slot_initialised(old));
    assert!(!dbg_slot_initialised(latest));

    let mut held = Vec::with_capacity(64);
    let mut latest_lease = HeapRegistry::dbg_claim_lease().expect("claim latest slot");
    assert_eq!(latest_lease.core().id(), latest);
    assert_eq!(latest_lease.generation(), 2);
    held.push(latest_lease);
    // The bitmap normally finds old first; remove only its advisory bit to
    // force the documented chunk-OOM recovery scan (slot state stays FREE).
    HeapRegistry::dbg_forget_free_slot_hint(old as usize);

    for expected in 2..64 {
        let mut lease = HeapRegistry::dbg_claim_lease().expect("claim fresh slot");
        assert_eq!(lease.core().id(), expected);
        held.push(lease);
    }
    assert_eq!(bootstrap::count_for_test(), 64);
    assert!(!bootstrap::ensure().dbg_chunk_is_materialised(1));

    let _reset = OomReset;
    bootstrap::dbg_set_inject_chunk_oom(true);
    let mut recovered = HeapRegistry::dbg_claim_lease().expect("recover old constructor slot");
    assert_eq!(recovered.core().id(), old);
    assert_eq!(recovered.generation(), 2);
    assert!(dbg_slot_initialised(old));
    assert_eq!(bootstrap::count_for_test(), 65);
    assert!(!bootstrap::ensure().dbg_chunk_is_materialised(1));

    bootstrap::dbg_set_inject_chunk_oom(false);
    let mut retried = HeapRegistry::dbg_claim_lease().expect("retry minted chunk index");
    assert_eq!(retried.core().id(), 64);
    assert_eq!(bootstrap::count_for_test(), 65);
    drop(retried);
    drop(recovered);
    drop(held);
}
