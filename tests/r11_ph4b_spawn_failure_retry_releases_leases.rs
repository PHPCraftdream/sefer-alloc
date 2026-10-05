//! Ph4b (task #2092, chunk B): a `MaintenanceStartError::Spawn` failure must
//! leave NO half-activated worker behind — every registry lease stays freely
//! takeable, and an immediate retry succeeds. Single process (no child): the
//! FAIL_START injection is deterministic under `bench-internals + internals`.
//!
//! Lease-free evidence: after the failed start, `HeapRegistry::try_maintenance`
//! (doc(hidden), internals) acquires a pre-created FREE slot, its Drop returns
//! the slot to FREE, and a second `try_maintenance` succeeds again;
//! `dbg_claim_lease()` likewise hands out a claim. Then the retry
//! `start_maintenance()` returns `Ok` and the service reports running.
#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::global::{MaintenanceService, MaintenanceStartError};
use sefer_alloc::registry::bootstrap;
use sefer_alloc::registry::heap_slot::{STATE_FREE, STATE_MAINTENANCE};
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::SeferAlloc;

#[test]
fn spawn_failure_leaves_no_half_activation_and_retry_releases_leases() {
    let layout = Layout::from_size_align(2048, 16).expect("valid layout");

    // Create one initialised FREE slot: a short-lived thread claims a heap via
    // a real allocation and exits, recycling its slot (whole-slot reuse).
    let address = std::thread::spawn(move || {
        let allocator = SeferAlloc::new();
        // SAFETY: valid non-zero layout.
        let pointer = unsafe { allocator.alloc(layout) };
        assert!(!pointer.is_null(), "owner alloc returned null");
        pointer as usize
    })
    .join()
    .expect("owner exited normally");
    assert_ne!(address, 0);

    assert!(!SeferAlloc::maintenance_running());
    MaintenanceService::fail_next_start_for_test();
    assert!(
        matches!(
            SeferAlloc::start_maintenance(),
            Err(MaintenanceStartError::Spawn(_))
        ),
        "the injected start must fail with Spawn"
    );
    assert!(
        !SeferAlloc::maintenance_running(),
        "a failed spawn must not leave a half-activated service"
    );

    // No worker holds any lease: the maintenance taker acquires a FREE slot,
    // and its Drop returns that exact slot to FREE for the next taker.
    let registry = bootstrap::ensure();
    let lease = HeapRegistry::try_maintenance()
        .expect("after a Spawn failure a FREE heap must be maintainable");
    let index = lease.slot_index();
    assert_eq!(
        registry.dbg_slot_state(index),
        STATE_MAINTENANCE,
        "an acquired maintenance lease must hold its slot"
    );
    drop(lease);
    assert_eq!(
        registry.dbg_slot_state(index),
        STATE_FREE,
        "dropping the lease must publish the slot back to FREE"
    );
    let again = HeapRegistry::try_maintenance()
        .expect("the slot is freely takeable again after the lease Drop");
    drop(again);

    // The owner side is equally unaffected: a fresh typed claim is available.
    assert!(
        HeapRegistry::dbg_claim_lease().is_some(),
        "a Spawn failure must not consume any claim authority"
    );

    // Retry activation succeeds and the service is genuinely running.
    SeferAlloc::start_maintenance().expect("retry activation after Spawn failure");
    assert!(SeferAlloc::maintenance_running());
    SeferAlloc::start_maintenance().expect("repeated start stays idempotent");
}
