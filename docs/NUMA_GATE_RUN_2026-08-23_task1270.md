# NUMA release gate run — task #1270 (F8, Sol-codex audit follow-up) — 2026-08-23

## Context
- Audit finding F8 (docs/reviews/2026-08-23-164206-numa-shim-publication-audit-Sol-codex.md): docs/NUMA_RELEASE_GATE.md mandates Phase 1-4 checks before any 0.x.y release touching crates/numa-shim/**; the audit was read-only, so nothing was verified for the current revision (356fb44, branch numa-shim/1270-f8-release-gate).
- This run executes the phases reachable from this Windows host and records the remainder as explicitly open. No synthetic multi-socket / fake-kernel workarounds were attempted, per the audit's warning.

## Phase criteria (verbatim from the gate docs)
- Phase 1 — mock-shim seam (docs/NUMA_TESTING_OPTIONS.md): "add a test-only feature to numa-shim that replaces current_node / bind_range / reserve_on_node with a recording mock ... Unit tests assert that the right syscalls were issued with the right arguments." Validates: "our wrapping logic (do we call mbind with the right node? Do we honor NO_NODE short-circuit? Does reserve_on_node chain to reserve_aligned first?)". Invocation: `cargo test -p numa-shim --features mock` (tests in crates/numa-shim/tests/mock_dispatch.rs).
- Phase 2 — real Linux kernel (QEMU -numa runbook, tests/numa_alloc.rs Option A): "boot a Linux VM under QEMU with synthetic NUMA topology (-numa node,...). Inside the VM, numactl --hardware shows 2+ nodes, mbind actually binds pages, cat /proc/PID/numa_maps proves it." Invocation inside VM: `SEFER_NUMA_TEST=1 cargo test --release --features "production numa-aware" --test numa_alloc --test numa_segment_id --test numa_seam`.
- Phase 3 — Windows virtual NUMA (docs/NUMA_WINDOWS_DEV_RECIPE.md): create a Hyper-V Gen2 VM, apply `Set-VMProcessor -MaximumCountPerNumaNode 2 -MaximumCountPerNumaSocket 1 -CompatibilityForMigrationEnabled $false` + static memory; guest must show `Win32_NumaNode Count: 2`; then inside the guest run `SEFER_NUMA_TEST=1 cargo test --features "production numa-aware" --test numa_alloc --test numa_segment_id --test numa_seam`.
- Phase 4 — real multi-socket topology (docs/NUMA_RELEASE_GATE.md): AWS/Azure 2-socket metal instance; `numactl --hardware` MUST show 2 nodes; run the env-guarded NUMA suite in release mode; verify /proc/self/numa_maps physical placement.

## What ran on this host (Windows 10 Pro 10.0.19045, single-socket Intel i7-11800H, Win32_ComputerSystem NumberOfProcessors=1)

### Phase 1 — RAN, PASS
Command: `cargo test -p numa-shim --features mock`
Result: 28 passed, 0 failed across 4 test binaries — mock_dispatch 7/7 (bind_range NO_NODE/zero-len short-circuit, arg recording, current_node scripted values, capped call log), cpumap_parser 17/17, smoke 4/4, lib unittests 0.
Raw log: docs/perf/_raw_numa_gate_p1_2026-08-23.log

### Phase 3 (partial, HOST-LEVEL only) — the in-guest procedure did NOT run
The Hyper-V recipe itself could not be executed: `Get-VM` from this shell fails with "You do not have the required permission" (Hyper-V management requires an elevated/admin PowerShell; this session is non-elevated). Additionally `Win32_NumaNode` WMI/CIM class is not present on this host and `Get-NumaNode` is unavailable, so host topology introspection beyond Win32_ComputerSystem was not possible. Creating the VM, verifying guest Count:2, and running the suite inside the guest are all still OUTSTANDING.
However, the single-NUMA-safe portions of the Phase 3 test commands were run on the Windows host itself:
Command: `cargo test --features "production numa-aware internals" --test numa_seam --test numa_segment_id`
Result: numa_seam 5/5 passed, numa_segment_id 2/2 passed.
Command: `SEFER_NUMA_TEST=1 cargo test --features "production numa-aware internals" --test numa_alloc -- --nocapture`
Result: 2/2 passed, with observed output `node_a=0, node_b=0` and `observed_current_node=0, stamped_node_id=0` — i.e. this host reports a single NUMA node (node 0), so the multi-node code paths (node=1 VirtualAllocExNuma, cross-node handoff) were NOT exercised. This is corroborating evidence only, NOT a Phase 3 pass.
Note: the gate doc's invocation `--features "production numa-aware"` alone compiles 0 tests for numa_seam/numa_segment_id — both files are `#![cfg(all(feature = "numa-aware", feature = "internals"))]`-gated, so `internals` must be added. This is a documentation drift between NUMA_RELEASE_GATE.md/NUMA_WINDOWS_DEV_RECIPE.md and the actual test gating (flagged, not fixed in this task).
Raw log: docs/perf/_raw_numa_gate_p3_partial_2026-08-23.log

### Phase 2 — DID NOT RUN
Requires a Linux kernel (QEMU -numa VM or numa=fake=N boot). Not available on this Windows host; no workaround attempted.

### Phase 4 — DID NOT RUN
Requires a rented 2-socket metal cloud instance (AWS c5n.metal / Azure HBv4) with AWS/Azure credentials. Not available in this environment; no workaround attempted.

## Verdict
- Phase 1: PASS (2026-08-23, 356fb44 worktree).
- Phase 3: PARTIAL — single-NUMA-safe suites pass on the host kernel; the actual Hyper-V virtual-NUMA in-guest procedure is OUTSTANDING (needs elevated PowerShell + Hyper-V Gen2 VM).
- Phase 2: OUTSTANDING (needs Linux kernel/QEMU).
- Phase 4: OUTSTANDING (needs 2-socket metal cloud instance).
Per NUMA_RELEASE_GATE.md: do NOT cut a 0.x.y release touching crates/numa-shim/** on the strength of this run alone.

## Measurement identity
- Branch numa-shim/1270-f8-release-gate, base commit 356fb44, clean tree apart from this report and the two raw logs (this commit is the record).
- Host: Windows 10.0.19045, 11th Gen Intel i7-11800H, 1 physical processor.
