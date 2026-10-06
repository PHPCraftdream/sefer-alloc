# Source review round 13: review and accepted remediation

Report and receipts: `docs/reviews/2026-10-06-src-review-sol-round-13.md`,
section "Принятые исправления после ревью".
Review source identity: `e90a3575d1af4131b301ddf9b9b49b78bac88216`.
Remediation base: `021399a34a29f21ae5b56c73c640c08ee7881040`.

## 1. Commit enumeration

Pre-remediation table derived from:

```text
git log --reverse --format="%H|%s" e90a3575d1af4131b301ddf9b9b49b78bac88216..021399a34a29f21ae5b56c73c640c08ee7881040
```

| Commit | Category | Subject |
|---|---|---|
| `b5fb4e76aadd85ed320445f4d45c935078ce53f1` | docs | record src review round 13 |
| `021399a34a29f21ae5b56c73c640c08ee7881040` | checkpoint | 2026-10-06-1422 |

The closing `fix` commit integrates all reviewed remediation slices and this
updated manifest. Its SHA is not self-referenced inside the file it creates.
Derive the complete round through that immutable closing commit from Git:

```text
git log --reverse --format="%H|%s" e90a3575d1af4131b301ddf9b9b49b78bac88216..:/"fix: close src review round 13 findings"
```

No `perf(runtime)` or `perf(opt-in)` commit in this round; no performance
promotion claim. Source changes are correctness/retention, tests and docs.

## 2. Production impact

`production` feature composition: unchanged. Defaults, Cargo.lock and project /
dependency versions: unchanged. No feature added or removed.

Production-visible correctness: ShardGuard's auto-trait constraint; two unused
private Node operations removed. Scan-counter behavior changes only with
`alloc-stats`. Experimental ShardedRegion token lifetime changes from strong TLS
retention to weak claims over out-of-line backing with cold dead-claim pruning.
No default speedup or RSS number claimed.

## 3. Verification

- Full native root suite with production/internals/alloc-stats/bench-internals/
  batch-api: Rust-harness summaries 804 passed, 0 failed, 7 ignored, before the
  single incidental substring link test was removed.
- Final focused run after doc/link cutover: 42 passed, 0 failed, including all
  eight new R13 cases, remaining documentation/inventory checks and Large drop.
- Counter observer-off and experimental observer-off controls passed;
  existing sharded/FIFO/remote cases passed.
- Five deliberate negative controls were observed failing: old counter
  increment placement, missing guard marker, no dead-claim pruning, strong TLS
  retention, and missing exit release. Implementations restored; regressions
  passed afterward. No deliberate data race executed.
- Actual installed-allocator/maintenance/typed-region/empty-directory executable
  passed; throwaway source removed after smoke.
- Clippy -D warnings: default, experimental and production library rows;
  all-features all-targets row passed. Narrow allocator feature builds passed.
- Strict public docs.rs `production` and all-features private-items rustdoc
  passed. Private links were fixed, not warning-suppressed.
- Windows workspace fmt-check runner passed.

Miri/Loom/Kani, release-profile suite, wall-clock/Ir/RSS A/B and push were not
performed. Accepted P1-box and unrelated open-item acceptance remain unchanged.

## 4. Per-item verdict

| Item | Final verdict |
|---|---|
| R13-01 all-word scan counter | SHIPPED; correctness 168 CLOSED |
| R13-02 transient token retention / cold pruning | SHIPPED; correctness 168 CLOSED |
| R13-03 terminal proofs and private link resolution | CORRECTION; correctness 168 CLOSED |
| R13-04 dead Node accessors | CORRECTION; correctness 168 CLOSED |
| R13-05 guard auto-Sync | SHIPPED; correctness 169 CLOSED |
| Accepted P1-box / interior-free residual / protocol mirror | UNCHANGED; items 164/166/167 remain |
| Optimization hypotheses and wall-clock acceptance | UNCHANGED; no promotion decision |

## 5. Artifacts

New committed raw perf logs: 0 files / 0 bytes. No perf gate run.
Correctness receipts and observed output are in the source-review report.
Six Rush task sessions used dedicated in-repo worktrees, integrated by the
parent after actual-diff review and parent execution; no agent commits or push.
