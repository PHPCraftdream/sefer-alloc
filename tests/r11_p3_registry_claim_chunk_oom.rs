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
        let core = HeapRegistry::claim();
        assert!(!core.is_null());
        // SAFETY: this thread owns the claim until its later recycle.
        assert_eq!(unsafe { (*core).id() }, expected);
        held.push(core);
    }
    assert!(!reg.dbg_chunk_is_materialised(1));
    // Two releases overwrite the one-entry hint. Reclaim the latest one,
    // leaving an older FREE heap with no hint while fresh capacity remains.
    for index in [0, 1] {
        // SAFETY: each pointer is a distinct live claim, used only here.
        unsafe { HeapRegistry::recycle(held[index]) };
    }
    let latest = HeapRegistry::claim();
    assert_eq!(latest, held[1]);
    assert_eq!(reg.dbg_slot_state(0), STATE_FREE);
    assert_eq!(reg.dbg_slot_state(1), STATE_LIVE);

    let _reset = OomReset;
    bootstrap::dbg_set_inject_chunk_oom(true);
    let recovered = HeapRegistry::claim();
    assert_eq!(recovered, held[0], "cold OOM scan must find older FREE");
    assert_eq!(bootstrap::count_for_test(), 65);
    assert!(!reg.dbg_chunk_is_materialised(1));
    bootstrap::dbg_set_inject_chunk_oom(false);

    let retried = HeapRegistry::claim();
    assert!(!retried.is_null());
    // SAFETY: the successful claim exclusively owns the initialized core.
    assert_eq!(unsafe { (*retried).id() }, 64);
    assert!(reg.dbg_chunk_is_materialised(1));
    assert_eq!(bootstrap::count_for_test(), 65);

    // SAFETY: these three live claims have not been recycled yet.
    unsafe {
        HeapRegistry::recycle(retried);
        HeapRegistry::recycle(recovered);
        HeapRegistry::recycle(latest);
    }
    for core in held.into_iter().skip(2) {
        // SAFETY: each remaining pointer is still a distinct live claim.
        unsafe { HeapRegistry::recycle(core) };
    }

    let lease = HeapRegistry::try_maintenance().expect("materialized FREE heap");
    let leased_index = lease.slot_index() as u32;
    drop(lease);
    let after_lease = HeapRegistry::claim();
    assert!(!after_lease.is_null());
    // SAFETY: the returned claim is live until the following recycle.
    assert_eq!(unsafe { (*after_lease).id() }, leased_index);
    // SAFETY: this claim has not yet been recycled.
    unsafe { HeapRegistry::recycle(after_lease) };
}
