//! Linux-specific test for CPU index fallback behavior.
//!
//! This file is gated on Linux (non-mock, non-miri) and is exercised by
//! CI's plain `cargo test -p numa-shim` on ubuntu-latest. A
//! `RUSTFLAGS="--cfg numa_shim_mock"` run skips this file; together both
//! configurations cover all tests (plain `--all-features` no longer activates
//! the mock at all — task #1288).

#![cfg(all(target_os = "linux", not(miri), not(numa_shim_mock)))]

use numa_shim::{current_node, current_node_resolution, linux, NodeResolution};

#[test]
fn dbg_huge_cpu_index_is_topology_unavailable() {
    // CPU 1_000_000 is far beyond any real system's CPU count; the cached
    // cpumap files will not have enough words to contain it, so
    // `cpu_to_numa_node_checked` will return `None`, which maps to
    // `TopologyUnavailable`.
    let resolution = linux::dbg_node_resolution_for_cpu(1_000_000);
    assert_eq!(resolution, NodeResolution::TopologyUnavailable);
}

#[test]
fn dbg_huge_cpu_index_current_node_is_none() {
    // Counterfactual for task #1308: before the fail-closed fix,
    // `cpu_to_numa_node` substituted `0` for lookup failure, so an unmapped
    // CPU produced `Some(0)` — indistinguishable from a genuinely resolved
    // node 0. Now it correctly returns `None`.
    assert_eq!(linux::dbg_current_node_for_cpu(1_000_000), None);
}

#[test]
fn current_node_agrees_with_resolution_mapping() {
    // Verify the documented mapping on this real host:
    // - Resolved(n) -> Some(n)
    // - TopologyUnavailable -> None
    // - Unavailable -> None
    //
    // NodeResolution is #[non_exhaustive] from outside the crate, so a
    // wildcard arm is REQUIRED (do not enumerate all variants).
    let node = current_node();
    match current_node_resolution() {
        NodeResolution::Resolved(n) => assert_eq!(node, Some(n)),
        _ => assert_eq!(node, None),
    }
}
