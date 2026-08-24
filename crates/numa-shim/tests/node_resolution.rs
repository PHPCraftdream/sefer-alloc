//! Tests for the `NodeResolution` enum and `current_node_resolution()` function.
//!
//! These tests verify the new additive API that distinguishes WHY a node
//! could not be determined — current_node() now fails closed for both
//! non-Resolved outcomes (task #1308), so the enum's value is diagnostic,
//! not a way to recover a node-0 answer.

#![cfg(numa_shim_mock)]

use numa_shim::{current_node, current_node_resolution, mock, NodeResolution};

fn fresh_drain() -> Vec<mock::MockCall> {
    mock::drain()
}

#[test]
fn resolution_matches_current_node_resolved_zero() {
    fresh_drain();
    mock::set_current_node(0);
    let node = current_node();
    let resolution = current_node_resolution();
    assert_eq!(node, Some(0));
    assert_eq!(resolution, NodeResolution::Resolved(0));
}

#[test]
fn resolution_matches_current_node_resolved_nonzero() {
    fresh_drain();
    mock::set_current_node(3);
    let node = current_node();
    let resolution = current_node_resolution();
    assert_eq!(node, Some(3));
    assert_eq!(resolution, NodeResolution::Resolved(3));
}

#[test]
fn resolution_matches_current_node_unavailable() {
    fresh_drain();
    // Use the literal u32::MAX instead of widening NO_NODE's visibility.
    mock::set_current_node(u32::MAX);
    let node = current_node();
    let resolution = current_node_resolution();
    assert_eq!(node, None);
    assert_eq!(resolution, NodeResolution::Unavailable);
}

/// task #1277 (review N6): `current_node_resolution()` used to skip the
/// mock log entirely, contradicting the `mock` module's documented
/// "records every invocation" contract. Proves the fix: the call is
/// recorded, with the resolved outcome this call returned (matching
/// `CurrentNode`'s "inner value is what was returned" convention).
#[test]
fn resolution_records_in_mock_log() {
    fresh_drain();
    mock::set_current_node(3);
    let resolution = current_node_resolution();
    assert_eq!(resolution, NodeResolution::Resolved(3));
    let calls = fresh_drain();
    assert_eq!(
        calls,
        vec![mock::MockCall::CurrentNodeResolution(
            NodeResolution::Resolved(3)
        )]
    );
}

/// Same contract on the sentinel path: the recorded value is the mapped
/// `Unavailable` outcome, not the raw `NO_NODE` slot value.
#[test]
fn resolution_unavailable_records_in_mock_log() {
    fresh_drain();
    mock::set_current_node(u32::MAX);
    let resolution = current_node_resolution();
    assert_eq!(resolution, NodeResolution::Unavailable);
    let calls = fresh_drain();
    assert_eq!(
        calls,
        vec![mock::MockCall::CurrentNodeResolution(
            NodeResolution::Unavailable
        )]
    );
}
