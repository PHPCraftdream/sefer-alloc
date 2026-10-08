# Source review round 15 manifest — commits, impact & verdicts

**Scope:** Round-15 work commits in four ranges: the initial report wave after base `b1a1e4f9ad232e7a3aef6e7b8b488c5b27eddfe9` through `021977689c89804eda909f6ec9aca64adf74c4d9`; the first CI follow-up after closing manifest `714ea5b73c0a5558913a4af5d7b8048042fdd2e6` through `6b407dd8e65a85e82f9e46091c5735fa0e1a4ebb`; the cross-target follow-up after closing manifest `d5fb531bb13b065968b014007dc8e2fef114b724` through `024c3e8f726eccb355017444b34991d9f1337d07`; and the rustfmt follow-up after `024c3e8f726eccb355017444b34991d9f1337d07` through `b24a181f6db5c5e804622b2a074a699107e1679f`. The work-commit rows are derived from:

```text
git log --reverse --format="%H|%cI|%s" b1a1e4f9ad232e7a3aef6e7b8b488c5b27eddfe9..021977689c89804eda909f6ec9aca64adf74c4d9
git log --reverse --format="%H|%cI|%s" 714ea5b73c0a5558913a4af5d7b8048042fdd2e6..6b407dd8e65a85e82f9e46091c5735fa0e1a4ebb
git log --reverse --format="%H|%cI|%s" d5fb531bb13b065968b014007dc8e2fef114b724..024c3e8f726eccb355017444b34991d9f1337d07
git log --reverse --format="%H|%cI|%s" 024c3e8f726eccb355017444b34991d9f1337d07..b24a181f6db5c5e804622b2a074a699107e1679f
```

The prior closing manifests (`714ea5b7` and `d5fb531b`) and this updated closing artifact are intentionally outside the work-commit ranges, avoiding self-reference.

## §1. Work commits

| # | SHA | Commit time (`%cI`) | Prefix | Subject | Category |
|---|---|---|---|---|---|
| 1 | `247a52ad2f81381b775b7fa955d8f7c1f7aeef74` | `2026-10-07T09:41:43+02:00` | `test` | `accept current rustc feature-probe wording` | Compile-fail harness accepts both rustc E0433 registry-resolution wordings; no runtime change |
| 2 | `c82c88c54dfae4cf65775851f829e2d7899db15a` | `2026-10-07T17:15:07+02:00` | `test` | `skip dependency-mismatched probe candidates` | Compile-fail candidate filter skips E0460 dependency-version variants; no runtime change |
| 3 | `021977689c89804eda909f6ec9aca64adf74c4d9` | `2026-10-07T17:17:26+02:00` | `docs` | `record src review round 15` | XS review report, current-state index dispositions, and changelog; no runtime source change |
| 4 | `749dbfa914657d7096e0e910b40b08bf8c3c4b36` | `2026-10-07T19:44:30+02:00` | `test` | `skip missing-dependency probe candidates` | Compile-fail candidate filter skips E0463 dependency-not-found variants; no runtime change |
| 5 | `6b407dd8e65a85e82f9e46091c5735fa0e1a4ebb` | `2026-10-07T21:00:58+02:00` | `docs` | `record R15 CI probe follow-up` | Records the first landing-CI failure, test-only correction, and current follow-up state; no runtime source change |
| 6 | `20b7c443843c76848d657c54e96bec32dde3c8d4` | `2026-10-07T22:41:15+02:00` | `test` | `gate host-rustc probe to x86_64` | Compile-fail harness excludes cross-target test binaries whose rlibs cannot be inspected by host rustc; no runtime change |
| 7 | `024c3e8f726eccb355017444b34991d9f1337d07` | `2026-10-07T22:47:27+02:00` | `docs` | `record R15 cross-target probe follow-up` | Records the E0461 cross-target failure and test-only target guard; no runtime source change |
| 8 | `b24a181f6db5c5e804622b2a074a699107e1679f` | `2026-10-07T22:55:00+02:00` | `test` | `format host-rustc probe gate` | Applies rustfmt to the cross-target guard; no behavior change |

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

- `npm run check` — **passed all 65 steps** from the isolated review worktree using its worktree-local target after the E0463/E0461 candidate-filter and target-gate fixes, plus the rustfmt correction. The process-local empty `RUSTC_WRAPPER` and `CARGO_BUILD_RUSTC_WRAPPER` values bypassed the machine-level sccache wrapper; no global Cargo configuration changed.
- Focused `r14_sidecar_owner_capability_negative` mixed-feature test — **3 passed** after all candidate-classifier and target-guard fixes.
- Temporary R15 native witnesses — **2 passed** (panic-tail destructor skip and primordial rollback counter mismatch); their source was removed. These are receipts, not permanent regressions.
- Correctness-index count check — **149** ACTIVE/TRACKED headings; **140** `[T]` headings and **140** item-to-file lookup rows.
- GitHub Actions run `37644289617` on initial landing SHA `714ea5b73c0a5558913a4af5d7b8048042fdd2e6` exposed E0463; run `37681398463` on landing SHA `d5fb531bb13b065968b014007dc8e2fef114b724` exposed E0461 from invoking host rustc on an aarch64 rlib. Test-only fixes in `749dbfa9` and `20b7c443` address both candidate-selection failures; no allocator runtime/source failure was observed.
- No Miri, Loom, Kani, NUMA runtime witness, or R15-specific performance judge was run. The unmeasured optimization hypotheses remain proposals only.

## §5. Independent verification correction (R16, 2026-10-08; append-only)

- §4's CI sentence attributes E0461 to the second run. In fact run `37644289617` on `714ea5b7` already failed both `test (aarch64-unknown-linux-gnu)` (E0461) and `test (x86_64-unknown-linux-gnu)` (E0463). `749dbfa9` fixed only E0463, so E0461 recurred in run `37681398463`; run `37704825252` on `6a0d47f6` is green.
- The `20b7c443` x86_64 gate also removed the R14-01 harness from the native macOS arm64 job (3 tests at `714ea5b7`, 0 at `6a0d47f6`). This is tracked as correctness item 175 (R16-02).
- §4's census line (149 ACTIVE/TRACKED records; 140 `[T]` headings and lookup rows) is correct. The thin index's "8 ACTIVE cards" text was not: there were 9. It is corrected in round 16 (now 10 with item 174).
- Full verdicts: the "Независимая проверка (oxx, 2026-10-08)" section at the end of `docs/reviews/2026-10-07-src-review-xs-round-15.md`.
