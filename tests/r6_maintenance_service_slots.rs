#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::global::reclaimer::MaintenanceScheduler;
use sefer_alloc::registry::{bootstrap, HeapRegistry};

#[test]
fn repeated_bounded_visits_reach_every_persistent_free_slot() {
    let owners = [
        HeapRegistry::claim(),
        HeapRegistry::claim(),
        HeapRegistry::claim(),
    ];
    assert!(owners.iter().all(|owner| !owner.is_null()));
    let indices = owners.map(|owner| {
        // SAFETY: every pointer is a distinct live claim, read by its owner.
        unsafe { (*owner).id() as usize }
    });
    for owner in owners {
        // SAFETY: each live claim is recycled exactly once, with no raw alias used later.
        unsafe { HeapRegistry::recycle(owner) };
    }

    let registry = bootstrap::ensure();
    let before_chunks = bootstrap::dbg_num_chunks();
    let mut visited = [false; 3];
    for _ in 0..indices.len() {
        let result = MaintenanceScheduler::visit_once(
            |lease| {
                let pos = indices
                    .iter()
                    .position(|&index| index == lease.slot_index())
                    .expect("leased one of the recycled slots");
                visited[pos] = true;
            },
            |_| panic!("fallback must not be initialised just to scan slots"),
        );
        assert!(result.slot_leased);
        assert!(!result.fallback_locked);
    }
    assert!(visited.into_iter().all(|seen| seen));
    assert_eq!(bootstrap::dbg_num_chunks(), before_chunks);
    assert!(!registry.dbg_chunk_is_materialised(1));
}
