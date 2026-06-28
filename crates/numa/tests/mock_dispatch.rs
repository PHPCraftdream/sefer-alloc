//! NUMA Phase 1 — mock-shim dispatch tests.
//!
//! These tests run on EVERY target (Windows, Linux, macOS, miri) and verify
//! that our wrapping logic invokes the platform NUMA functions with the
//! right arguments WITHOUT depending on real multi-NUMA hardware.
//!
//! Gated on `feature = "mock"`.  Run with:
//!   `cargo test -p numa-shim --features mock`
//! (and for the reserve_on_node tests):
//!   `cargo test -p numa-shim --features "mock vmem-integration"`

#![cfg(feature = "mock")]

use numa_shim::{bind_range, current_node, mock, NO_NODE};

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

#[test]
fn bind_range_no_node_short_circuits() {
    fresh_drain();
    // SAFETY: dummy non-null pointer + len, NO_NODE → function must return
    // without recording anything (and without dereferencing the pointer).
    unsafe { bind_range(0x1000 as *mut u8, 4096, NO_NODE) };
    assert!(
        fresh_drain().is_empty(),
        "NO_NODE must short-circuit before any record"
    );
}

#[test]
fn bind_range_zero_len_short_circuits() {
    fresh_drain();
    // SAFETY: len == 0 causes an early return before any OS call or record.
    unsafe { bind_range(0x1000 as *mut u8, 0, 1) };
    assert!(
        fresh_drain().is_empty(),
        "len == 0 must short-circuit before any record"
    );
}

#[test]
fn bind_range_records_args() {
    fresh_drain();
    // SAFETY: dummy pointer; mock intercepts before any dereference.
    unsafe { bind_range(0x1000 as *mut u8, 4096, 3) };
    let calls = fresh_drain();
    assert_eq!(
        calls,
        vec![mock::MockCall::BindRange {
            base: 0x1000,
            len: 4096,
            node: 3,
        }]
    );
}

#[cfg(feature = "vmem-integration")]
#[test]
fn reserve_on_node_chains_and_records() {
    use aligned_vmem::PAGE;
    fresh_drain();
    let r = numa_shim::reserve_on_node(PAGE * 4, PAGE, 2).expect("reserve");
    let calls = fresh_drain();
    assert_eq!(calls.len(), 2, "expect ReserveOnNode + BindRange");
    assert!(matches!(
        calls[0],
        mock::MockCall::ReserveOnNode {
            size: _,
            align: _,
            node: 2
        }
    ));
    assert!(matches!(
        calls[1],
        mock::MockCall::BindRange { node: 2, .. }
    ));
    drop(r);
}

#[cfg(feature = "vmem-integration")]
#[test]
fn reserve_on_node_no_node_skips_bind() {
    use aligned_vmem::PAGE;
    fresh_drain();
    let r = numa_shim::reserve_on_node(PAGE * 4, PAGE, NO_NODE).expect("reserve");
    let calls = fresh_drain();
    assert_eq!(calls.len(), 1, "only ReserveOnNode, no BindRange");
    assert!(matches!(
        calls[0],
        mock::MockCall::ReserveOnNode { node: NO_NODE, .. }
    ));
    drop(r);
}
