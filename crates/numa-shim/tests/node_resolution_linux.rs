//! Linux-specific test for CPU index fallback behavior.
//!
//! This file is gated on Linux (non-mock, non-miri) and is exercised by
//! CI's plain `cargo test -p numa-shim` on ubuntu-latest. The
//! `--all-features` run (with mock enabled) skips this file; together
//! both configurations cover all tests.

#![cfg(all(target_os = "linux", not(miri), not(feature = "mock")))]

use numa_shim::NodeResolution;

#[test]
fn dbg_huge_cpu_index_falls_back_to_zero() {
    // CPU 1_000_000 is far beyond any real system's CPU count; the cached
    // cpumap files will not have enough words to contain it, so
    // `cpu_to_numa_node_checked` will return `None`, which maps to
    // `FellBackToZero`.
    let resolution = numa_shim::linux::dbg_node_resolution_for_cpu(1_000_000);
    assert_eq!(resolution, NodeResolution::FellBackToZero);
}
