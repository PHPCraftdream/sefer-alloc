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
    let latest_core = HeapRegistry::claim();
    assert!(!latest_core.is_null());
    // SAFETY: this thread owns the live claim until its later recycle.
    assert_eq!(unsafe { (*latest_core).id() }, latest);
    held.push(latest_core);
    for expected in 2..64 {
        let core = HeapRegistry::claim();
        assert!(!core.is_null());
        // SAFETY: this thread owns each claim until its later recycle.
        assert_eq!(unsafe { (*core).id() }, expected);
        held.push(core);
    }
    assert_eq!(bootstrap::count_for_test(), 64);
    assert!(!bootstrap::ensure().dbg_chunk_is_materialised(1));

    let _reset = OomReset;
    bootstrap::dbg_set_inject_chunk_oom(true);
    let recovered = HeapRegistry::claim();
    assert!(
        !recovered.is_null(),
        "constructor retry must avoid false OOM"
    );
    // SAFETY: the recovered slot is now LIVE and exclusively owned here.
    assert_eq!(unsafe { (*recovered).id() }, old);
    assert!(dbg_slot_initialised(old));
    assert!(!bootstrap::ensure().dbg_chunk_is_materialised(1));
    bootstrap::dbg_set_inject_chunk_oom(false);

    let minted_retry = HeapRegistry::claim();
    assert!(!minted_retry.is_null());
    // SAFETY: claim returned a distinct LIVE core.
    assert_eq!(unsafe { (*minted_retry).id() }, 64);
    assert_eq!(bootstrap::count_for_test(), 65);
    // SAFETY: all these pointers are still distinct LIVE claims.
    unsafe {
        HeapRegistry::recycle(minted_retry);
        HeapRegistry::recycle(recovered);
    }
    for core in held {
        // SAFETY: this pointer has not yet been recycled.
        unsafe { HeapRegistry::recycle(core) };
    }
}
