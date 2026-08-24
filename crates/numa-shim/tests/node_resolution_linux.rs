//! Linux-specific test for CPU index fallback behavior.
//!
//! This file is gated on Linux (non-mock, non-miri) and is exercised by
//! CI's plain `cargo test -p numa-shim` on ubuntu-latest. A
//! `RUSTFLAGS="--cfg numa_shim_mock"` run skips this file; together both
//! configurations cover all tests (plain `--all-features` no longer activates
//! the mock at all — task #1288).

#![cfg(all(target_os = "linux", not(miri), not(numa_shim_mock)))]

use numa_shim::{linux, NodeResolution};

// Raw FFI declared directly in this test file — the same self-contained
// pattern `tests/policy_oracle_linux.rs` uses for its own raw `syscall(2)`
// declaration — rather than reaching into crate internals for the
// production declaration in `src/lib.rs` (which stays untouched, task
// #1332). One sample of the calling thread's current CPU is all the
// single-snapshot oracle below needs; `sched_getcpu(2)` takes no
// arguments and returns the CPU index or -1 on failure (glibc does not
// set errno for it).
extern "C" {
    fn sched_getcpu() -> core::ffi::c_int;
}

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
    // Single-snapshot oracle (task #1332, eighteenth review F5): the old
    // body called `current_node()` and `current_node_resolution()` as two
    // INDEPENDENT calls, each internally taking its own `sched_getcpu()`
    // snapshot — on a real multi-node host the scheduler is free to migrate
    // this thread between the two calls, so two individually-CORRECT
    // snapshots (`Some(0)` from the first call's CPU, `Resolved(1)` from
    // the second's) would fail the comparison as a false failure. Instead
    // take ONE `sched_getcpu()` sample here and feed that SAME value into
    // the two doc-hidden test-only forwarders, which never re-sample the
    // CPU — no scheduler migration can occur between two calls that do
    // not consult the scheduler at all.
    //
    // Verify the documented mapping for that one snapshot:
    // - Resolved(n) -> Some(n)
    // - TopologyUnavailable -> None
    // - Unavailable -> None
    //
    // NodeResolution is #[non_exhaustive] from outside the crate, so a
    // wildcard arm is REQUIRED (do not enumerate all variants).
    let raw = unsafe { sched_getcpu() };
    if raw < 0 {
        eprintln!(
            "skip: sched_getcpu() returned {raw} on this host; \
             the single-snapshot mapping oracle needs one valid CPU sample \
             (task #1332, eighteenth review F5)"
        );
        return;
    }
    let cpu = raw as u32;
    let node = linux::dbg_current_node_for_cpu(cpu);
    match linux::dbg_node_resolution_for_cpu(cpu) {
        NodeResolution::Resolved(n) => assert_eq!(node, Some(n)),
        _ => assert_eq!(node, None),
    }
}
