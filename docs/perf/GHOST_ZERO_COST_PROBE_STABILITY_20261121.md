# Ghost zero-cost probe stability — measurement only (2026-11-21)

**GO: use `pa` as the ghost-zero-cost measurement harness.** This is not an
allocator speedup or a change to production code, features, gate policy, or
thresholds. `pc` also qualifies but has higher control-pair (`ctl`) Ir in the
generated results and adds complexity: it moves `Vec::push` into a non-inlined
`record` helper and the window check, eviction index, and `swap_remove` into a
non-inlined `evict` helper. `pa` leaves those operations inside the phase loops.
The instruction counts do not establish a wall-time difference.

## Workload and measurement

The old probe's known false reds on P2.2/P2.3 were historically diagnosed as
register-selection changes in a monolithic `main`. This matrix measures total
Callgrind Ir, including the harness, and does not establish per-function
causality.

**Entry point under test:** The deterministic single-thread load is unchanged.
The probe calls `GlobalAlloc` explicitly on a local `SeferAlloc`, the gate's
allocator layer, not a real global allocator; there is no global allocator
override or maintenance thread:

- Growth: allocate 16,384 × 16 B, then free them all.
- Churn: 65,536 allocations of sizes 16–2048 B from a fixed xorshift sequence,
  with a live window of 256 blocks; drain the window at the end.
- Large: allocate and free 1 MiB with 64 B alignment.

`pa` puts growth, churn, and Large in `#[inline(never)]` functions and passes
arguments through `black_box`. Those phases are part of the measured candidate
body. The landing probe differs from the measured `pa` source only in its
header comments.

The experiment crossed four probe variants (`old`, `pa`, `pb`, `pc`) with four
pinned base/head pairs (`ctl`, `p22v0`, `p23v0`, `p23v4`), one sample per arm.
The pinned workflow lists all pair SHAs; `ctl` uses the same SHA on both sides.
Each job staged the same probe source on both sides and built in release mode
with `--locked --features production`. The workflow driver set
`RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"`.
It recorded total Callgrind Ir and compared gate-normalized `.text`.
`text_equal=false` is expected on extraction pairs and is diagnostic there;
the self-pair control must match. The qualification below is for harness
selection, not a substitute for the gate's exact proof on this PR.

## Results

<!-- BEGIN GENERATED GHOST PROBE RESULTS -->

Fixed historical measurements: Callgrind instruction counts (Ir), one sample per arm. These do not establish relative runtime performance.

| Probe | Pair | Ir base | Ir head | Signed change (Δ/base) | Text equal |
|---|---|---:|---:|---:|---|
| old | ctl | 10225643 | 10225643 | +0.000000000000000% (+0/10225643) | true |
| old | p22v0 | 10225234 | 10267777 | +0.416058938113299% (+42543/10225234) | false |
| old | p23v0 | 10225975 | 10275190 | +0.481274401707417% (+49215/10225975) | false |
| old | p23v4 | 10225975 | 10225643 | -0.003246634184026% (-332/10225975) | false |
| pa | ctl | 10412630 | 10412630 | +0.000000000000000% (+0/10412630) | true |
| pa | p22v0 | 10412221 | 10407838 | -0.042094765372345% (-4383/10412221) | false |
| pa | p23v0 | 10415173 | 10413040 | -0.020479736630395% (-2133/10415173) | false |
| pa | p23v4 | 10415173 | 10412630 | -0.024416301102248% (-2543/10415173) | false |
| pb | ctl | 10493793 | 10493793 | +0.000000000000000% (+0/10493793) | true |
| pb | p22v0 | 10491189 | 10423452 | -0.645656083404846% (-67737/10491189) | false |
| pb | p23v0 | 10496336 | 10428654 | -0.644815486089622% (-67682/10496336) | false |
| pb | p23v4 | 10496336 | 10493793 | -0.024227501863507% (-2543/10496336) | false |
| pc | ctl | 12132705 | 12132705 | +0.000000000000000% (+0/12132705) | true |
| pc | p22v0 | 12132272 | 12127900 | -0.036036119203394% (-4372/12132272) | false |
| pc | p23v0 | 12135248 | 12133102 | -0.017684022609179% (-2146/12135248) | false |
| pc | p23v4 | 12135248 | 12132705 | -0.020955484387299% (-2543/12135248) | false |

**Calibration passes.** old: p22v0 +0.416058938113299% (+42543/10225234) (expected Δ +42346 ± 1000 (inclusive)); p23v0 +0.481274401707417% (+49215/10225975) (expected Δ +49215 exactly); p23v4 -0.003246634184026% (-332/10225975) (expected Δ -332 exactly). All 4 ctl pairs have Δ +0 and text_equal=true.

Qualification uses the exact, unrounded ratio: absolute Δ/base ≤ 1/1000 (0.1%).

- **pa: qualifies.** All 4 pairs meet the absolute 0.1% threshold.
- **pb: rejected.** Outside the absolute 0.1% threshold on p22v0 -0.645656083404846% (-67737/10491189), p23v0 -0.644815486089622% (-67682/10496336).
- **pc: qualifies.** All 4 pairs meet the absolute 0.1% threshold. On ctl, pc observed Ir 12132705 vs pa 10412630 (higher).

<!-- END GENERATED GHOST PROBE RESULTS -->

This is one x86_64 Linux series, with no repeats per arm. It does not establish
repeatability across platforms or toolchains, or measure allocator wall time.

## Provenance and reproduction

Measurement source: `f6aef10dc301f18683e97b2fc032404cc4efdf2a`,
[`.github/workflows/p209-zc-profile.yml` at that SHA](https://github.com/PHPCraftdream/sefer-alloc/blob/f6aef10dc301f18683e97b2fc032404cc4efdf2a/.github/workflows/p209-zc-profile.yml).
[Run 44928351811](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/44928351811),
attempt 1, succeeded on 2026-11-21 (UTC).

Verbatim version lines from **Print toolchain and runner versions**, confirmed
via the jobs API across all 16 jobs (for example, `pa / ctl`, job `136919301113`):

```text
Runner image: ubuntu26 20261116.202.1
PRETTY_NAME="Ubuntu 26.04.1 LTS"
rustc 1.100.0 (0ef1bc386 2026-11-09)
binary: rustc
commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a
commit-date: 2026-11-09
host: x86_64-unknown-linux-gnu
release: 1.100.0
LLVM version: 23.1.5
cargo 1.100.0 (3acc79c0c 2026-11-05)
v24.22.3
valgrind-3.26.0
GNU objdump (GNU Binutils for Ubuntu) 2.46
```

To repeat the measurement, rerun the original pinned GitHub Actions workflow
from the linked run, not the driver locally. The recorded shared runner image
and tool versions describe this attempt; they are not pins on future reruns.
Temporary measurement files (the workflow, driver and candidate sources on the
measurement branch, scratch builds, and Callgrind outputs) are not shipped in
this PR.

Evidence: [raw job summaries](_raw_ghost_zero_cost_probe_stability_20261121.log)
(with a jobs API recipe for the full logs) and
[`docs/perf/GHOST_ZERO_COST_PROBE_STABILITY_20261121_summary.csv`](GHOST_ZERO_COST_PROBE_STABILITY_20261121_summary.csv).
To regenerate and check the report from the saved raw evidence, and run the
report tests without Cargo, use these commands from the repository root:

```sh
node scripts/ghost_probe_stability_report.mjs --write
node scripts/ghost_probe_stability_report.mjs --check
node --test tests/ghost_probe_stability_report.test.mjs
```

## Scope of the change

- **Evidence registry:** None of the 54 literal source scopes or 62 JSON recipes
  examined matches the probe, `scripts/ghost_zero_cost*.mjs`, or any changed
  file. This change makes no evidence rows stale and requires no EV-1 changes.
  The evidence verifier checks committed scope pathspecs, not unstaged changes;
  the strict local status was `INCOMPLETE (strict), exit 0` for unrelated
  unpublished identities and pending cells, with no `evidence-src-stale`.
- **Ghost protocol:** The gate probe is a measurement example, not ghost
  protocol code. No catalog protection, ordering site, ordering inventory,
  or `docs/verification/` material changes; no `Invariant-Check:` trailer is
  needed. `docs/verification/` contains CI decomposition job metadata and step
  digests, not a probe pin. No open noise-specific item was found, so both
  indexes are unchanged.
- **Proof:** For this change, the gate requires a full-range exact proof
  (`proof_required=true`, `proof_mode=exact`). The ghost-zero-cost CI job runs
  that proof for this change.
  On the uncommitted worktree,
  `node scripts/ghost_zero_cost.mjs --base origin/main --allow-declared-runtime --classify-only`
  reported:

  ```text
  [ghost-zero-cost] exception does not apply: the original full-range zero-cost proof is required
  [ghost-zero-cost] classification: proof_mode=exact proof_required=true skipped=false
  ```

- **CI routing:** For the changed files, the planner selects `always=true` and
  `fast=true`; `seam`, `os`, every `crate_*`, and `full` are false.
  `node scripts/ci-plan.mjs --selftest` passed 105 checks.
- **Structure:** The limits remain 7 entries per source directory and 1,000
  lines per code file. `node scripts/verify-structure-limits.mjs` reported:

  ```text
  [verify-structure-limits] OK: self-test 21 cases; 4 directories and 28 files grandfathered by docs/structure_limits_baseline.csv, none new, none grown
  ```
