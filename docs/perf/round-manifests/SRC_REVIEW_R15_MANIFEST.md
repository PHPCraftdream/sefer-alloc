# Source review round 15 manifest — commits, impact & verdicts

**Scope:** Round-15 work commits after base `b1a1e4f9ad232e7a3aef6e7b8b488c5b27eddfe9` through `021977689c89804eda909f6ec9aca64adf74c4d9`. The table is derived from:

```text
git log --reverse --format="%H|%cI|%s" b1a1e4f9ad232e7a3aef6e7b8b488c5b27eddfe9..021977689c89804eda909f6ec9aca64adf74c4d9
```

This manifest is the separate closing artifact and is intentionally outside that work-commit range, avoiding a self-referential row.

## §1. Work commits

| # | SHA | Commit time (`%cI`) | Prefix | Subject | Category |
|---|---|---|---|---|---|
| 1 | `247a52ad2f81381b775b7fa955d8f7c1f7aeef74` | `2026-10-07T09:41:43+02:00` | `test` | `accept current rustc feature-probe wording` | Compile-fail harness accepts both rustc E0433 registry-resolution wordings; no runtime change |
| 2 | `c82c88c54dfae4cf65775851f829e2d7899db15a` | `2026-10-07T17:15:07+02:00` | `test` | `skip dependency-mismatched probe candidates` | Compile-fail candidate filter skips E0460 dependency-version variants; no runtime change |
| 3 | `021977689c89804eda909f6ec9aca64adf74c4d9` | `2026-10-07T17:17:26+02:00` | `docs` | `record src review round 15` | XS review report, current-state index dispositions, and changelog; no runtime source change |

## §2. Default-feature and measurement impact

- `production` composition, defaults, dependency versions, and `Cargo.lock` are unchanged. No shipping or opt-in allocator algorithm changed; this round has no `perf(runtime)` or `perf(opt-in)` commit.
- The existing `npm run check` verification includes its deterministic IAI step (85 benches). Those results were not used to support an R15 finding, optimization hypothesis, or performance verdict. No R15-specific wall-clock, Ir, RSS, promotion, or GO/NO-GO result is claimed.
- Raw perf logs committed: **0 files, 0 bytes**. No performance report or summary CSV is owed because no verdict in this round rests on a measured performance number.

## §3. Findings and final verdicts

| Item | Final verdict | Evidence / limit |
|---|---|---|
| R15-01 / correctness item 172 | **OPEN, P3** | Parent's temporary native unwind witness observed the panicking destructor run once and the later live value's destructor not run. No source fix or permanent regression is included. |
| R15-02 / correctness item 173 | **OPEN, P3** | Parent's temporary registration-refusal witness asserted reserved delta +1 / released delta 0. RAII releases the mapping; only the root diagnostic counter is missed. No source fix or permanent regression is included. |
| R15-03 / correctness item 154 | **OPEN, P4** | Source documentation overstates shard routing/async-lock guarantees and mislabels live production route-directory wiring. The broader dedicated prose-cleanup trigger remains open. |
| Correctness item 157 | **CLOSED/SUPERSEDED** | The legacy intrusive-spill mechanism and its described counters/observer are absent from current root `src/`; full closure narrative is in `docs/correctness-open-items/RESOLVED.md`. This does not decide whether a future current-sidecar metric is needed. |
| Correctness item 158 | **OPEN** | All-bucket `publish_empty` and current-node-only post-drain sync clearing are distinguished; NUMA runtime witness and scan-cost result remain open. |
| Accepted P1-box / correctness item 164 | **OPEN, unchanged** | No Miri acceptance or source change. |
| Epoch collector / correctness item 171 | **OPEN, separate pre-existing issue** | No Miri rerun or false-positive classification. |
| Existing performance-index items | **LEAVE** | No R15 measurement discharges their triggers; current verdicts and next triggers are unchanged. |

## §4. Final verification

- `npm run check` — **passed all 65 steps** from the isolated review worktree using its worktree-local target. The process-local empty `RUSTC_WRAPPER` and `CARGO_BUILD_RUSTC_WRAPPER` variables bypassed the machine-level sccache wrapper; no global Cargo configuration changed.
- Focused `r14_sidecar_owner_capability_negative` mixed-feature test — **3 passed** after both candidate-classifier fixes.
- Temporary R15 native witnesses — **2 passed** (panic-tail destructor skip and primordial rollback counter mismatch); their source was removed. These are receipts, not permanent regressions.
- Correctness-index count check — **149** ACTIVE/TRACKED headings; **140** `[T]` headings and **140** item-to-file lookup rows.
- No Miri, Loom, Kani, NUMA runtime witness, or R15-specific performance judge was run. The unmeasured optimization hypotheses remain proposals only.
