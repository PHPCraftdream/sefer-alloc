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
/// `NodeId` (raw `u32` in the log). task #1311 (F6): a SUCCESSFUL call now records
/// TWO entries — the public call, then the simulated policy installation — so the
/// log reflects the real Linux backend's two-stage reserve-then-policy contract.
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
        2,
        "success path now records two entries: ReservePreferredOnNode then InstallPolicy succeeded:true"
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
    assert!(matches!(
        calls[1],
        mock::MockCall::InstallPolicy {
            node: 3,
            succeeded: true,
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

/// task #1311 (F6): scripted policy failure returns the exact error and
/// releases the reservation exactly once. The log shows the three-stage
/// sequence: reserve → policy failed → release.
#[cfg(feature = "vmem-integration")]
#[test]
fn scripted_policy_failure_returns_os_error_and_releases_exactly_once() {
    use numa_shim::{reserve_preferred_on_node, NodeId, ReserveNumaError};
    use std::io::Error;
    fresh_drain();
    mock::clear_policy_failure();
    // Script ENOMEM (errno 12) for node 3
    let original_err = Error::from_raw_os_error(12);
    let original_display = original_err.to_string();
    mock::set_policy_failure(3, original_err);
    let result = reserve_preferred_on_node(
        PAGE_4,
        PAGE,
        NodeId::new(3).expect("literal 3, not NO_NODE"),
    );
    let err = result.expect_err("scripted policy failure must return Err");
    assert!(
        matches!(err, ReserveNumaError::Os(_)),
        "expected Os error, got {:?}",
        err
    );
    let os_err = match err {
        ReserveNumaError::Os(e) => e,
        _ => unreachable!(),
    };
    assert_eq!(
        os_err.raw_os_error(),
        Some(12),
        "errno must be preserved exactly"
    );
    assert_eq!(
        os_err.to_string(),
        original_display,
        "Display representation must match the original error"
    );
    let calls = fresh_drain();
    assert_eq!(
        calls.len(),
        3,
        "exactly three records: reserve, failed policy, release"
    );
    // Index-based ordering asserts to prove the sequence
    assert!(matches!(
        calls[0],
        mock::MockCall::ReservePreferredOnNode { node: 3, .. }
    ));
    assert!(matches!(
        calls[1],
        mock::MockCall::InstallPolicy {
            node: 3,
            succeeded: false,
            ..
        }
    ));
    assert!(matches!(
        calls[2],
        mock::MockCall::PolicyFailureRelease { node: 3 }
    ));
}

/// task #1311 (F6): a policy failure scripted for one node does not affect
/// calls to a different node. The slot stays armed or is not consumed for
/// the different node.
#[cfg(feature = "vmem-integration")]
#[test]
fn policy_failure_script_for_other_node_does_not_fire() {
    use numa_shim::{reserve_preferred_on_node, NodeId};
    fresh_drain();
    mock::clear_policy_failure();
    mock::set_policy_failure(5, std::io::Error::from_raw_os_error(12));
    // Call with node 3 — should succeed
    let r = reserve_preferred_on_node(
        PAGE_4,
        PAGE,
        NodeId::new(3).expect("literal 3, not NO_NODE"),
    )
    .expect("call to node 3 must succeed");
    let calls = fresh_drain();
    assert_eq!(calls.len(), 2, "reserve and succeeded policy, no release");
    assert!(matches!(
        calls[0],
        mock::MockCall::ReservePreferredOnNode { node: 3, .. }
    ));
    assert!(matches!(
        calls[1],
        mock::MockCall::InstallPolicy {
            node: 3,
            succeeded: true,
            ..
        }
    ));
    drop(r);
    // Call again with node 3 — still succeeds (slot for node 5 still armed)
    fresh_drain();
    let r2 = reserve_preferred_on_node(
        PAGE_4,
        PAGE,
        NodeId::new(3).expect("literal 3, not NO_NODE"),
    )
    .expect("second call to node 3 must still succeed");
    let calls2 = fresh_drain();
    assert_eq!(
        calls2.len(),
        2,
        "second call still records reserve and succeeded policy"
    );
    assert!(matches!(
        calls2[1],
        mock::MockCall::InstallPolicy {
            node: 3,
            succeeded: true,
            ..
        }
    ));
    drop(r2);
}

/// task #1311 (F6): a scripted policy failure is one-shot — consumed by the
/// first matching call, after which the node behaves normally (no re-arming
/// required).
#[cfg(feature = "vmem-integration")]
#[test]
fn policy_failure_script_is_one_shot() {
    use numa_shim::{reserve_preferred_on_node, NodeId, ReserveNumaError};
    fresh_drain();
    mock::clear_policy_failure();
    mock::set_policy_failure(4, std::io::Error::from_raw_os_error(12));
    // First call to node 4 — fails with Os
    let result = reserve_preferred_on_node(
        PAGE_4,
        PAGE,
        NodeId::new(4).expect("literal 4, not NO_NODE"),
    );
    assert!(
        matches!(result, Err(ReserveNumaError::Os(_))),
        "first call must fail with Os"
    );
    let calls = fresh_drain();
    assert_eq!(calls.len(), 3, "reserve, failed policy, release");
    // Second call to node 4, no re-scripting — succeeds
    fresh_drain();
    let r2 = reserve_preferred_on_node(
        PAGE_4,
        PAGE,
        NodeId::new(4).expect("literal 4, not NO_NODE"),
    )
    .expect("second call must succeed (one-shot consumed)");
    let calls2 = fresh_drain();
    assert_eq!(calls2.len(), 2, "reserve and succeeded policy, no release");
    assert!(matches!(
        calls2[1],
        mock::MockCall::InstallPolicy {
            node: 4,
            succeeded: true,
            ..
        }
    ));
    drop(r2);
}

/// task #1311 (F6): the `InstallPolicy` record includes the complete OS
/// reservation length, which must be at least the requested size.
#[cfg(feature = "vmem-integration")]
#[test]
fn install_policy_records_the_complete_reservation_len() {
    use numa_shim::{reserve_preferred_on_node, NodeId};
    fresh_drain();
    mock::clear_policy_failure();
    let r = reserve_preferred_on_node(
        PAGE_4,
        PAGE,
        NodeId::new(7).expect("literal 7, not NO_NODE"),
    )
    .expect("reserve");
    let calls = fresh_drain();
    assert_eq!(calls.len(), 2);
    if let mock::MockCall::InstallPolicy {
        reservation_len, ..
    } = &calls[1]
    {
        assert!(
            *reservation_len >= PAGE_4,
            "reservation_len must be at least the requested size"
        );
    } else {
        panic!("second record must be InstallPolicy");
    }
    drop(r);
}
