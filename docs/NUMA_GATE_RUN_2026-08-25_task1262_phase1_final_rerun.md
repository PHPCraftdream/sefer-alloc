# NUMA release gate Phase 1 final re-run — task #1262 — 2026-08-25

## Context
- Why: task #1262's own recorded ordering rule (item 102's E1, folded into #1262's task description) requires the Phase 1 gate run LAST, on the final pre-tag revision — after the version bump, not before it. The last Phase 1 measurement (task #1279, `docs/NUMA_GATE_RUN_2026-08-23_task1279_phase1_rerun.md`) was taken at `c427dd6`, superseded many times over by the subsequent 19+ independent-review remediation waves (items 103-118) that touched `crates/numa-shim/src/lib.rs` and its test suite repeatedly.
- This re-run executes ONLY Phase 1, on the version-bumped tree (`0.2.0`). It supersedes every prior Phase 1 result for the 0.2.0 release record. Phases 2/3/4 are unchanged from the waiver (`docs/NUMA_GATE_2026-08-23_0.2.0_phase24_waiver.md`) and `docs/NUMA_GATE_RUN_2026-08-23_task1270.md` — not re-measured here.

## Measurement identity
- Branch `main`, commit `3397d60d53f3f2cc369f2f2de2cf385b5adca174` (full SHA, `git rev-parse HEAD` verified) — one commit after the version-bump commit `cb5d35107ddb883672c7e262affa1b4075285dce` (task #1262), which fills the waiver's landing-SHA placeholder; `crates/numa-shim/Cargo.toml`'s `version` field is `0.2.0` at this commit. Clean worktree at measurement time apart from this report + its raw log.
- Host: Windows 10 Pro 10.0.19045, single-socket Intel i7-11800H; rustc 1.97.0 (2d8144b78 2026-07-07).

## Phase 1 — RAN, PASS
Command: `RUSTFLAGS="--cfg numa_shim_mock" cargo test -p numa-shim` — the canonical Phase 1 invocation since task #1288's `mock`-feature-to-cfg conversion (matching `.github/workflows/ci.yml`'s own `numa-shim-mock` job's plain mock row exactly, including its two green-and-dead sentinels `current_node_records_scripted_value` / `resolution_matches_current_node_resolved_zero`, both confirmed present in this run's output). The file was force-touched before the run to rule out a stale-cache false pass. Exit status 0.

Result: **36 passed, 0 failed.** Per-binary:

| Binary | Passed |
|---|---|
| `unittests src/lib.rs` | 0 |
| `cpumap_parser.rs` | 15 |
| `cpumap_reverse_index.rs` | 6 |
| `eintr_retry.rs` | 3 |
| `mock_dispatch.rs` | 4 |
| `node_id.rs` | 2 |
| `node_resolution.rs` | 5 |
| `node_resolution_linux.rs` | 0 (Linux-gated, correctly empty on Windows) |
| `policy_oracle_linux.rs` | 0 (Linux-gated, correctly empty on Windows) |
| `readme_examples.rs` | 0 (its four tests are `vmem-integration`-gated; not passed to this invocation) |
| `smoke.rs` | 1 (its `vmem-integration`-gated tests are not exercised here; only the plain `current_node_returns_valid_or_none` runs) |
| Doc-tests | 0 |

Comparison to the superseded task #1279 run (31 passed across 4 binaries: `cpumap_parser` 17, `mock_dispatch` 7, `node_resolution` 3, `smoke` 4): the counts have shifted in BOTH directions since — `cpumap_reverse_index.rs`, `eintr_retry.rs`, and `node_id.rs` are new test files added by later tasks (#1310/#1319/#1309 respectively) that did not exist at the #1279 measurement; `cpumap_parser`/`mock_dispatch`/`smoke`'s own per-file counts also moved as their test suites were extended/restructured across the 19+ review-remediation waves between `c427dd6` and this commit. The "expect 33" figure item 103's T4 finding recorded is itself now stale for the same reason — this report's 36 is the current, execution-confirmed ground truth, not a discrepancy to investigate.

Raw log: `docs/perf/_raw_numa_gate_p1_final_rerun_2026-08-25.log`

## Phases 2 and 4 — DID NOT RUN (both: WAIVED per `docs/NUMA_GATE_2026-08-23_0.2.0_phase24_waiver.md`, unchanged)
- Phase 2 (real Linux kernel / QEMU `-numa` VM): requires a Linux kernel, not available on this Windows host. Waived for 0.2.0 by explicit owner risk acceptance (task #1290, now bound to commit `cb5d351` per the waiver's filled-in landing SHA). No workaround attempted.
- Phase 4 (2-socket metal cloud instance): requires rented metal + cloud credentials, not available in this environment. Waived for 0.2.0 by the same record. No workaround attempted.

## Phase 3 — NOT re-run
Unchanged from `docs/NUMA_GATE_RUN_2026-08-23_task1270.md` — host-level partial (`numa_seam` 5/5, `numa_segment_id` 2/2, single-node-host `numa_alloc` 2/2, corroborating only), in-guest Hyper-V procedure still OUTSTANDING, per the waiver's own explicit non-waiver of Phase 3's remainder.

## Verdict
- Phase 1: **PASS** at `3397d60` (this run supersedes every prior Phase 1 result for the 0.2.0 release record — 36/0, 0 failed, force-recompiled, not a stale-cache pass).
- Phases 2, 4: WAIVED (owner risk acceptance, bound to commit `cb5d351`). Phase 3: PARTIAL (unchanged).
- This is task #1262's own recorded final step. Per `docs/NUMA_RELEASE_GATE.md`, publishing now proceeds under the waiver's explicit terms — not on the strength of this Phase 1 run alone.
