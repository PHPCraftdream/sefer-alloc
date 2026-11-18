# CI check-matrix sharding — independent measurement evidence (PR #275, 2026-11-18)

Independent measurement of the unsharded `check-matrix`
(manifest-driven, non-clippy rows) CI job, recorded as historical context for
the sharding discussion in [CI_LONG_JOBS.md](CI_LONG_JOBS.md) (which is
generated separately and NOT edited here). No build/test/bench was run locally; all numbers below are
derived from the GitHub Actions job log via read-only `gh api` + a local
`python -I` parser.

## 1. Measured run (immutable identity)

| field | value |
|---|---|
| run id | 44385142496 (attempt 1, conclusion success) |
| PR / head SHA | #275 / `a1be2fe2bb7d5cd881866c6f9909b8b983c1506e` |
| job | 135087709698 — check-matrix (manifest-driven, non-clippy rows) |
| runner image | ubuntu-26.04, version 20261116.88.1 (image groups, raw lines 2-30) |
| runner OS | Ubuntu 26.04.1 LTS |
| runner CPU | not logged (log contains no lscpu/CPU identification) |
| n | 1 (single job, no repetition) |
| selection | latest completed full-selected success measured before any edits on this worktree (main merge run 44391549769 was still running when selected) |

## 2. Evidence assets

The full raw log (2,459,528 bytes; SHA-256
f1d6247fa26cfdeed9ef3f4fe62562850c9826745800f04580016e24342631a3) remains on
the GitHub job API; the repository keeps only the excerpt under the >2 MiB
policy:

```
gh api repos/PHPCraftdream/sefer-alloc/actions/jobs/135087709698/logs
```

Evidence assets (both under the recognized raw-excerpt/CSV convention,
neither is `.gitignore`d — `git check-ignore` confirms both trackable):

- [ci-check-matrix-135087709698-log-excerpt.txt](ci-check-matrix-135087709698-log-excerpt.txt) —
  minimal verbatim truncated excerpt: job-setup/runner-image/OS/image
  boundary (raw lines 1-32), setup/node/toolchain/action boundary lines,
  all 15 `##[group]Run node scripts/` guard start lines (5 pre-runner
  verify scripts, the `run-check-matrix` runner, 9 post-runner verify
  scripts), all 27 row header lines each followed by its
  `[check-matrix] OK: ...` + `freed build artefacts` cleanup line, the
  `Post job cleanup` lines and last log line, with `# TRUNCATED` markers
  and the exact reproduction command at every cut point. All 164 verbatim
  timestamp lines were re-checked byte-for-byte against the downloaded raw
  log: 0 mismatches.
- [CI_CHECK_MATRIX_SHARDING_summary.csv](CI_CHECK_MATRIX_SHARDING_summary.csv)
  (44 data rows: 27 row + 14 guard + 1 runner_step + 1 job_wall + 1 log_span)
  — machine-readable companion: kind (row/guard/runner_step/job_wall/log_span),
  run_id/job_id/attempt/head SHA, runner CPU/OS/image (CPU marked
  "not logged"), n=1, start/end timestamps, raw-log line range, duration_s,
  and the derivation method per row.

## 3. Measured timings (2026-11-18, one job)

### Row timings

Derivation (independent observed timing): each row is measured from its
verbatim row-header timestamp to its `freed build artefacts` timestamp;
each guard from `##[group]Run node scripts/` start until the next group
start or the first `Post job cleanup` line (upper bound); runner and whole
job are reported separately.

- 27 rows sum: **1492.877 s** (header->freed). Largest rows:
  test-production-misuse-checks 260.20, test-r8-production 256.04,
  test-r8-large-geometry-policies 255.48, test-r8-hardened-numa-directory
  228.10, test-r8-minimal-global 220.55, test-r8-fastbin-no-decommit 215.63.
- Runner step (`node scripts/run-check-matrix.mjs --keep-going
  --free-between-rows --kind check --kind test`, log lines 407->27036 guard
  upper bound): **1492.912 s**. No CI-reported substep precision is claimed.
- 14 verify-script guards sum: **3.982 s** (upper bounds). The 14
  verification steps do not build code; one (`verify-root-package-list.mjs`)
  invokes `cargo package --list`. In sharded CI they run in shard 1 only;
  in this unsharded run they executed once, in the single job.
- Whole job wall from GitHub API timestamps (started_at 19:54:21Z ->
  completed_at 20:19:40Z): **1519 s**; log span (first->last log timestamp)
  **1514.548 s** separately (diff ~4.5 s = pre-first-log runner allocation
  and post-log tail, outside the log's own timestamps).
- Residual outside runner+guards: 1519 − 1492.912 − 3.982 ≈ **22.1 s**
  (checkout/setup/action overhead + post-log tail). Cross-check: rows sum
  1492.877 s vs runner-step upper bound differ by 0.035 s (header->freed
  slightly undercounts the runner's own bookkeeping); no other discrepancies
  found.

## 4. Sharding model (independent LPT, seeded by the 4.000 s verify block)

Longest-processing-time packing of the 27 measured row durations, with
shard 1 seeded by the 4.000 s verify-script block and the per-shard
runner-step content taken as the packed sum:

- 3 shards: loads **[491.528, 513.690, 491.658] s** (bottleneck 513.690).
- 2 shards: peak load **748.504 s** — ~749 s, which exceeds the ~600 s
  upper end of the estimated slowest-shard wall below, so 2 shards do not
  meet the target once setup is added. **3 shards chosen.**

Expected wall of the slowest shard (estimate, not measured — the sharded CI
has not run end-to-end yet):

```
514 (bottleneck shard load)
+ ~22 (observed common runner overhead: checkout/toolchain/setup-node/post tail)
+ ~30-60 (cold-cargo variation: cold-cache build the unsharded job amortized
          once across 27 rows but each shard now pays separately)
= ~570-600 s wall for the slowest shard, ~2.5-2.7x on this 1519 s baseline, before queue.
```

No end-to-end full-CI speedup is claimed: the estimate covers only the
check-matrix job's own wall after sharding; queue depth, runner pool
concurrency and other jobs are unmeasured here.
