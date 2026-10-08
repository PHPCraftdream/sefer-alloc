#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::registry::HeapRegistry;

#[test]
fn primordial_attach_failure_balances_segment_accounting() {
    let reserved_before = AllocCore::dbg_segments_reserved_total();
    let released_before = AllocCore::dbg_segments_released_total();
    RouteDirectory::fail_next_registration_for_test();
    assert!(HeapRegistry::dbg_claim_lease().is_none());
    let reserved_delta = AllocCore::dbg_segments_reserved_total() - reserved_before;
    let released_delta = AllocCore::dbg_segments_released_total() - released_before;
    assert!(
        reserved_delta > 0,
        "failure must reserve a primordial segment"
    );
    assert_eq!(
        reserved_delta, released_delta,
        "failed attach: reserved_delta={reserved_delta} released_delta={released_delta}"
    );

    let lease = HeapRegistry::dbg_claim_lease().expect("failed attachment must be retryable");
    let slot = lease.slot_index();
    let retry_reserved = AllocCore::dbg_segments_reserved_total() - reserved_before;
    let retry_released = AllocCore::dbg_segments_released_total() - released_before;
    drop(lease);
    // Lease drop publishes FREE; the registry retains the materialized heap.
    assert_eq!(retry_reserved - reserved_delta, 1);
    assert_eq!(retry_released, released_delta);
    assert_eq!(retry_reserved - retry_released, 1);
    assert_eq!(
        AllocCore::dbg_segments_reserved_total() - reserved_before,
        retry_reserved
    );
    assert_eq!(
        AllocCore::dbg_segments_released_total() - released_before,
        retry_released
    );

    let lease = HeapRegistry::dbg_claim_lease().expect("retained heap must be reclaimable");
    let reclaimed_slot = lease.slot_index();
    drop(lease);
    assert_eq!(reclaimed_slot, slot);
    assert_eq!(
        AllocCore::dbg_segments_reserved_total() - reserved_before,
        retry_reserved
    );
    assert_eq!(
        AllocCore::dbg_segments_released_total() - released_before,
        retry_released
    );
}
