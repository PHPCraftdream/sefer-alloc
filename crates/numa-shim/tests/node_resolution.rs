//! Tests for the `NodeResolution` enum and `current_node_resolution()` function.
//!
//! These tests verify the new additive API that distinguishes "genuinely
//! resolved" from "fell back to node 0" on Linux, task #1266 audit finding F4.

#![cfg(feature = "mock")]

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
