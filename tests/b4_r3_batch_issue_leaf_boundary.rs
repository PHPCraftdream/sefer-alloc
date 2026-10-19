#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

//! B4-R3 counterfactual: a refill carve batch that crosses into a fresh
//! 256-granule class leaf must (a) roll back fully when the run's single
//! leaf promotion fails, and (b) perform exactly ONE promotion for the whole
//! crossing batch — the visible behavioral signature of the run hoist.
//!
//! Magazine refills carve whole batches, so the leaf CHANGE can happen inside
//! any refill; the spill injection is armed up front (the anchor's own leaf
//! was promoted before that) and the walk continues — same-leaf refills never
//! promote, so the injection survives — until the first refill that crosses
//! into a virgin leaf returns null with the injection consumed.

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::{RouteRegistration, SmallSidecar};
use sefer_alloc::registry::HeapRegistry;

const LEAF: usize = 4096;
const LAYOUT_16: Layout = match Layout::from_size_align(16, 16) {
    Ok(layout) => layout,
    Err(_) => panic!("bad layout"),
};

fn owner_state(anchor: *mut u8) -> (usize, u32, u32) {
    // SAFETY: the caller retains the owning heap and its live anchor
    // exclusively throughout the read.
    unsafe { SmallSidecar::owner_state_for_test(anchor.addr(), 0) }.unwrap()
}

#[test]
fn batch_issue_leaf_boundary_spill_rolls_back_and_recovers() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim lease");
    let heap = lease.core();

    // The anchor promotes its own leaf (before any injection is armed).
    let anchor = heap.alloc(LAYOUT_16);
    assert!(!anchor.is_null());
    assert_eq!(
        RouteRegistration::class_at_global_address_for_test(anchor.addr()),
        Some(0)
    );

    // Arm the spill failure; walk 16-byte carves (all kept live, so
    // everything stays bump-carved). Same-leaf refills never promote, so the
    // walk succeeds; the first refill batch that crosses into a virgin leaf
    // hits the injected failure in its single `prepare_leaf` and the whole
    // run rolls back to a graceful OOM.
    let attempts = if cfg!(feature = "fastbin") { 2 } else { 1 };
    SmallSidecar::fail_next_spills_for_test(attempts);
    let mut held: Vec<*mut u8> = vec![anchor];
    let mut crossed_leaf_end = false;
    // Pre-OOM baseline, refreshed after every successful alloc (the bump
    // advances legitimately; same-leaf allocs never promote, so the System
    // totals only move on the crossing we are waiting for).
    let mut before_owner = owner_state(anchor);
    let mut before_system = SmallSidecar::system_totals_for_test();
    let issued = loop {
        let ptr = heap.alloc(LAYOUT_16);
        if ptr.is_null() {
            assert!(
                crossed_leaf_end,
                "the walk must pass a leaf end before the injected OOM"
            );
            assert_eq!(
                SmallSidecar::spill_failures_remaining_for_test(),
                0,
                "the OOM must come from the injected spill failure"
            );
            // Full rollback: owner state and System totals unchanged.
            assert_eq!(owner_state(anchor), before_owner);
            assert_eq!(SmallSidecar::system_totals_for_test(), before_system);
            // Retry without injection.
            let retry = heap.alloc(LAYOUT_16);
            assert!(!retry.is_null(), "the retry must issue a block");
            break retry;
        }
        held.push(ptr);
        if ptr as usize % LEAF == LEAF - 16 {
            crossed_leaf_end = true;
        }
        before_owner = owner_state(anchor);
        before_system = SmallSidecar::system_totals_for_test();
    };

    assert_eq!(
        RouteRegistration::class_at_global_address_for_test(issued.addr()),
        Some(0),
        "class code published in the NEW leaf"
    );
    let after = SmallSidecar::system_totals_for_test();
    assert_eq!(
        after.0 - before_system.0,
        256,
        "exactly one 256-byte leaf promotion"
    );
    assert_eq!(after.1, before_system.1);
    assert_eq!(after.2, before_system.2);
    assert_eq!(
        after.3 - before_system.3,
        1,
        "exactly one promotion for the whole batch"
    );
    assert_eq!(after.4, before_system.4);

    // Same-leaf sanity: one more alloc inside the new leaf promotes nothing.
    let same_leaf = heap.alloc(LAYOUT_16);
    assert!(!same_leaf.is_null());
    assert_eq!(
        SmallSidecar::system_totals_for_test().3,
        after.3,
        "same-leaf alloc must not promote"
    );

    // Release everything in reverse.
    for &ptr in held.iter().rev() {
        // SAFETY: each pointer is a live, uniquely owned 16-byte block of
        // this heap.
        unsafe { heap.dealloc(ptr, LAYOUT_16) };
    }
    // SAFETY: live, uniquely owned 16-byte blocks of this heap.
    unsafe { heap.dealloc(same_leaf, LAYOUT_16) };
    unsafe { heap.dealloc(issued, LAYOUT_16) };
}
