//! R6-T24: a claimant that hits a slot held in MAINTENANCE must retain the
//! candidate (bounded retry) instead of minting a fresh heap.
#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use std::time::{Duration, Instant};

use sefer_alloc::registry::bootstrap;
use sefer_alloc::registry::heap_registry::HeapRegistry;

#[test]
fn claimant_hit_by_maintenance_reuses_slot_without_growing_count() {
    // Slot 0: claim and release so it is initialised + FREE, then hold it in
    // MAINTENANCE.
    let lease0 = HeapRegistry::dbg_claim_lease().expect("bind slot 0");
    assert_eq!(lease0.slot_index(), 0);
    drop(lease0);
    let maint = HeapRegistry::dbg_try_maintenance().expect("hold slot 0");

    // The claimant (another thread) hits MAINTENANCE on the hint, republishes
    // the candidate into the FREE bitmap, and retries until the lease drops.
    let handle = std::thread::spawn(|| {
        let mut lease = HeapRegistry::dbg_claim_lease().expect("claim succeeds");
        assert_eq!(lease.slot_index(), 0, "reused the maintenance slot");
        lease
    });

    // The retry loop's republish is the fix's signature: wait for the bitmap
    // to become non-empty (impossible on the pre-fix code, which `continue`d
    // straight to a fresh bump), then release the lease.
    let deadline = Instant::now() + Duration::from_secs(10);
    while HeapRegistry::dbg_free_slot_hint_popcount() == 0 {
        assert!(
            Instant::now() < deadline,
            "claimant never republished the maintenance candidate (pre-R6-T24 behaviour)"
        );
        std::hint::spin_loop();
    }
    drop(maint);

    let lease = handle.join().expect("claimant thread");
    drop(lease);
    assert_eq!(
        bootstrap::count_for_test(),
        1,
        "no replacement heap was minted (pre-fix: count grows to 2)"
    );
}
