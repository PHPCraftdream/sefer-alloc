//! NUMA Phase 1 — mock-shim dispatch tests.
//!
//! These tests run on EVERY target (Windows, Linux, macOS, miri) and verify
//! that our wrapping logic invokes the platform NUMA functions with the
//! right arguments WITHOUT depending on real multi-NUMA hardware.
//!
//! Gated on the build-time cfg `numa_shim_mock`. Run with:
//!   `RUSTFLAGS="--cfg numa_shim_mock" cargo test -p numa-shim`
//! (and for the reserve_preferred_on_node tests):
//!   `RUSTFLAGS="--cfg numa_shim_mock" cargo test -p numa-shim --features vmem-integration`
//!
//! task #1306: the `bind_range` dispatch tests are gone with the API. The
//! mock now records `ReservePreferredOnNode` BEFORE validation, so the
//! error paths (`InvalidNode`, `InvalidArguments`) are assertable in the
//! call log on every host — real Err outcomes the old `Option`-returning
//! surface could not express.

#![cfg(numa_shim_mock)]

use numa_shim::{current_node, mock, NO_NODE};

#[cfg(feature = "vmem-integration")]
const PAGE: usize = 4096;
#[cfg(feature = "vmem-integration")]
const PAGE_4: usize = PAGE * 4;

fn fresh_drain() -> Vec<mock::MockCall> {
    mock::drain()
}

#[test]
fn current_node_records_scripted_value() {
    fresh_drain();
    mock::set_current_node(7);
    let n = current_node();
    assert_eq!(n, Some(7));
    let calls = fresh_drain();
    assert_eq!(calls, vec![mock::MockCall::CurrentNode(7)]);
}

#[test]
fn current_node_default_zero() {
    fresh_drain();
    mock::set_current_node(0);
    let n = current_node();
    assert_eq!(n, Some(0));
    let calls = fresh_drain();
    assert_eq!(calls, vec![mock::MockCall::CurrentNode(0)]);
}

/// task #722 (rust-intel audit §F2): `current_node`'s mock arm used to wrap
/// the scripted slot in `Some` UNCONDITIONALLY, so `set_current_node(NO_NODE)`
/// produced `Some(NO_NODE)` -- violating this function's own documented
/// "returns `Option`, never the sentinel" guarantee, and making the `None`
/// branch impossible to exercise under `numa_shim_mock` (the cfg that exists
/// precisely so CI can assert this wrapping logic). Proves the fix: scripting
/// the sentinel now yields a genuine `None`.
#[test]
fn current_node_scripted_no_node_yields_none() {
    fresh_drain();
    mock::set_current_node(NO_NODE);
    let n = current_node();
    assert_eq!(
        n, None,
        "scripting the NO_NODE sentinel must produce None, not Some(NO_NODE)"
    );
    // The call is still recorded with the raw sentinel value -- only the
    // PUBLIC return value is remapped, matching the real dispatch's own
    // record-then-remap order.
    let calls = fresh_drain();
    assert_eq!(calls, vec![mock::MockCall::CurrentNode(NO_NODE)]);
}

/// task #1306: `reserve_preferred_on_node` records its arguments through the
/// `NodeId` (raw `u32` in the log). Exactly ONE record per call — the policy
/// installation happens inside the platform backend, which the mock replaces
/// wholesale, so there is no second `BindRange`-style record anymore.
#[cfg(feature = "vmem-integration")]
#[test]
fn reserve_preferred_on_node_records_args() {
    use numa_shim::{reserve_preferred_on_node, NodeId};
    fresh_drain();
    let r = reserve_preferred_on_node(
        PAGE_4,
        PAGE,
        NodeId::new(3).expect("literal 3, not NO_NODE"),
    )
    .expect("reserve");
    let calls = fresh_drain();
    assert_eq!(
        calls.len(),
        1,
        "exactly one record per call -- the bind happens inside the backend the mock replaces"
    );
    // task #726 (rust-intel audit §C1a): the struct-like variants carry
    // field-level `#[non_exhaustive]`, so an external crate (this
    // integration test) can only pattern-match with a trailing `..`.
    assert!(matches!(
        calls[0],
        mock::MockCall::ReservePreferredOnNode {
            size: PAGE_4,
            align: PAGE,
            node: 3,
            ..
        }
    ));
    drop(r);
}

/// task #1306: the Linux single-`u64` nodemask limit is mirrored by the mock
/// so the `InvalidNode` error path is assertable on EVERY host, not only
/// Linux — and the rejected call is still recorded (record-BEFORE-validate,
/// unlike the old `BindRange` which recorded only past its short-circuit).
#[cfg(feature = "vmem-integration")]
#[test]
fn reserve_preferred_on_node_out_of_range_node_is_recorded_then_rejected() {
    use numa_shim::{reserve_preferred_on_node, NodeId, ReserveNumaError};
    fresh_drain();
    let err = reserve_preferred_on_node(
        PAGE_4,
        PAGE,
        NodeId::new(64).expect("literal 64 is not the NO_NODE sentinel"),
    )
    .expect_err("node 64 must be rejected");
    assert!(
        matches!(err, ReserveNumaError::InvalidNode),
        "expected InvalidNode, got {err:?}"
    );
    let calls = fresh_drain();
    assert_eq!(
        calls.len(),
        1,
        "the rejected call is still recorded (record-before-validate)"
    );
    assert!(matches!(
        calls[0],
        mock::MockCall::ReservePreferredOnNode { node: 64, .. }
    ));
}

/// task #1306: argument-contract violations surface as typed errors under
/// the mock too — the mock mirrors the real backends' `try_reserve_aligned`
/// error mapping (`is_invalid_argument` -> `InvalidArguments`, else `Os`),
/// so the distinction the old `Option` API collapsed is assertable here.
#[cfg(feature = "vmem-integration")]
#[test]
fn reserve_preferred_on_node_invalid_arguments_is_typed_not_none() {
    use numa_shim::{reserve_preferred_on_node, NodeId, ReserveNumaError};
    fresh_drain();
    let err = reserve_preferred_on_node(0, PAGE, NodeId::new(0).expect("literal 0, not NO_NODE"))
        .expect_err("zero size must be rejected");
    assert!(
        matches!(err, ReserveNumaError::InvalidArguments),
        "expected InvalidArguments, got {err:?}"
    );
    let calls = fresh_drain();
    assert_eq!(calls.len(), 1);
    assert!(matches!(
        calls[0],
        mock::MockCall::ReservePreferredOnNode { size: 0, .. }
    ));
}

/// task #726 (rust-intel audit §B14): under the documented
/// sefer-alloc-as-global `numa-aware-mock` scenario, `record()` is called
/// from an allocation hot path with nothing ever draining the log -- before
/// this task `CALLS` grew without bound. Confirms the fix: pushing well past
/// the module's own `CALLS_CAP` leaves the log capped rather than matching
/// the push count. This test WOULD fail against the pre-fix unbounded
/// `Vec::push` (it would observe `calls.len() == PUSHES`).
///
/// task #778 (round-closing review, F7): `CALLS_CAP` is now `pub`, so this
/// test asserts against the real constant instead of a hardcoded mirror.
#[test]
fn calls_log_is_capped_not_unbounded() {
    fresh_drain();
    const PUSHES: usize = 5000;
    const {
        assert!(
            PUSHES > mock::CALLS_CAP,
            "PUSHES must exceed CALLS_CAP for this test to be meaningful"
        )
    };
    for i in 0..PUSHES {
        mock::set_current_node((i % 64) as u32);
        let _ = current_node();
    }
    let calls = fresh_drain();
    assert!(
        calls.len() <= mock::CALLS_CAP,
        "CALLS must be capped at {}, got {}",
        mock::CALLS_CAP,
        calls.len()
    );
    assert!(
        calls.len() < PUSHES,
        "a capped log must hold fewer entries than were pushed ({} pushed, {} recorded)",
        PUSHES,
        calls.len()
    );
}
