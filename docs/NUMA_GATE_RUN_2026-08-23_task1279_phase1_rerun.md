# NUMA release gate Phase 1 re-run — task #1279 (N12 follow-up) — 2026-08-23

## Context
- Why: review finding N12 (P2) of docs/reviews/2026-08-23-183220-numa-shim-publication-readiness-review-oh.md — the task #1270 gate run's Phase 1 PASS was measured at base commit 356fb44, superseded by later changes inside crates/numa-shim/** (task #1266 added the NodeResolution public API + ~256 lines to src/lib.rs + the node_resolution.rs / node_resolution_linux.rs test files; task #1269 changed the Windows reserve path). The old report's own counts (28 passed across 4 test binaries) corroborated the staleness — node_resolution.rs's 3 tests did not exist at measurement time.
- This re-run executes ONLY Phase 1 on the current revision; it supersedes the task #1270 Phase 1 result. Everything else in the task #1270 report stands unchanged.

## Measurement identity
- Branch numa-shim/1279-n12-gate-rerun, commit c427dd6fc454ed4567665beda8454859689ee4a3 (full SHA, `git rev-parse HEAD` verified), clean worktree at measurement time (this report + the raw log are the only new files). Note: task #1275 (further Windows reserve-path work) may land in parallel on another branch — it is NOT in this measured tree; this run is a valid record of exactly c427dd6 and a future re-run will be owed if #1275 lands before the release cut.
- Host: Windows 10 Pro 10.0.19045, single-socket Intel i7-11800H; rustc 1.97.0 (2d8144b78 2026-07-07).

## Phase 1 — RAN, PASS
Command: `cargo test -p numa-shim --features mock` — identical invocation to the task #1270 Phase 1 run. Exit status 0.
Result: 31 passed, 0 failed. Per-binary: cpumap_parser 17/17, mock_dispatch 7/7, node_resolution 3/3, smoke 4/4; lib unittests 0; node_resolution_linux runs 0 tests on Windows (the file is Linux-gated by design — CI's plain `cargo test -p numa-shim` on ubuntu exercises it); doc-tests 0.
Comparison to the superseded run: 28 passed across 4 test binaries (mock_dispatch 7, cpumap_parser 17, smoke 4, lib 0) → 31 passed; the delta is exactly node_resolution.rs's 3 new tests (task #1266). This confirms review N12's prediction (5 test binaries under `--features mock` at 472fc98; the fifth binary, node_resolution_linux, compiles on Windows but is empty there by design).
Raw log: docs/perf/_raw_numa_gate_p1_rerun_2026-08-23.log

## Phases 2 and 4 — DID NOT RUN (both: OUTSTANDING, unchanged)
- Phase 2 (real Linux kernel / QEMU -numa VM): requires a Linux kernel, not available on this Windows host. OUTSTANDING for the same infrastructure reason as the task #1270 run. No workaround attempted, per the original audit's explicit warning against synthetic substitutes.
- Phase 4 (2-socket metal cloud instance, AWS c5n.metal / Azure HBv4): requires rented metal + cloud credentials, not available in this environment. OUTSTANDING for the same infrastructure reason. No workaround attempted.
- Note the owner decision review N12+F8 calls for (ship with phases 2/4 outstanding plus a release-note record, or obtain them first) remains open and is NOT made by this report.

## Phase 3 — NOT re-run
unchanged from docs/NUMA_GATE_RUN_2026-08-23_task1270.md — host-level partial (numa_seam 5/5, numa_segment_id 2/2, single-node-host numa_alloc 2/2 corroborating only), in-guest Hyper-V procedure still OUTSTANDING.

## Verdict
- Phase 1: PASS at c427dd6 (this run supersedes the stale 356fb44 Phase 1 PASS for the release record).
- Phases 2, 4: OUTSTANDING. Phase 3: PARTIAL (unchanged from task #1270).
- Per docs/NUMA_RELEASE_GATE.md: do NOT cut a 0.x.y release touching crates/numa-shim/** on the strength of this run alone.
