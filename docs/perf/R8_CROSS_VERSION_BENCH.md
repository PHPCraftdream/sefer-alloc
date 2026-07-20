# Cross-version wall-clock comparison — 0.2.1 → 0.3.0 (post-Round8 + R9-1)

**Date:** 2026-07-20
**Anchors:**

| Version | Source | Commit | Notes |
|---|---|---|---|
| **0.2.1** | tag `sefer-alloc-v0.2.1`, harness ported | `bench/0.2.1` (`5edb3d9`) | last version published on crates.io (unchanged since the R7 run) |
| **0.3.0 (current)** | `main` | `860d897` | all of Round 8 (`ffd3215..de4c4ae`) + the R9-1 miri-safety follow-up (`860d897`) |
| mimalloc / System | dev-deps / std | — | external references |

This refreshes [`R7_CROSS_VERSION_BENCH.md`](R7_CROSS_VERSION_BENCH.md) (2026-07-17, anchored at
`49046ef`, pre-Round8). The external review that opened Round 9 explicitly asked for a
fresh table because the R7 numbers pre-dated the Round 8 perf-review queue. R7 is kept
unchanged as a historical snapshot; this file is the current answer.

**Platform:** Windows 10 Pro x86-64, 16 threads, Rust 1.97.0 release, criterion fast
profile (`sample_size(10)`, short warm-up). Single noisy dev host (±15–20 %
run-to-run). Trust the *shape and order of magnitude*, not the last decimal.

## Methodology — same harness against each version's allocator

Identical to R7. The benchmark **harness** evolved across versions, so running "each
version's own harness" would conflate harness changes with allocator changes. Instead
the **current harness** (`benches/global_alloc.rs` + `scripts/bench-table.mjs`, i.e.
`npm run bench:table`) is run against each version's **allocator source**:

- **0.3.0** = current `main` (`860d897`) — the harness is native there.
- **0.2.1** predates `bench:table`, so the current harness was ported onto the 0.2.1
  tag and preserved as the reusable **`bench/0.2.1`** branch. Re-measured with
  `git checkout bench/0.2.1 && npm run bench:table`, then switched back to `main`.

mimalloc/System are the same code in both runs; their columns below are the
**0.3.0-run** reference. *Caveat vs R7:* this session's mimalloc/System arms drifted
more between the two runs than R7's did — e.g. mimalloc `Vec_push` moved
1169 → 1434 ns (+22 %) between the two runs on byte-identical mimalloc code, and
mimalloc cold-direct 1024 B moved 72.7 → 48.8 ns. That inter-run drift is the
host-noise floor for the composite/small-alloc arms this session; treat any
single-cell delta under ~1.5× as noise, and weight the multiplicative gaps (the churn
family at 64 B+, the decommit cycle, the working-set cycle) which are far above it.

The two ratio columns describe **0.3.0**: `vs 0.2.1` = the version-over-version
improvement (0.2.1 ÷ 0.3.0), `vs mimalloc` = 0.3.0 vs mimalloc (mimalloc ÷ 0.3.0).
ns/op unless noted; **lower is better**.

---

## 1. Cold-direct (`bench_direct_alloc`, no reuse — 1 alloc + 1 free per op)

| Size | Sefer 0.2.1 | Sefer 0.3.0 | mimalloc | System | vs 0.2.1 | vs mimalloc |
|---|---:|---:|---:|---:|---|---|
| 16B | 46.8 | 47.3 | 16.3 | 157.5 | 1.01× slower | 2.91× slower |
| 64B | 44.7 | 41.7 | 24.6 | 135.0 | 1.07× faster | 1.69× slower |
| 256B | 40.0 | 49.8 | 35.7 | 167.4 | 1.25× slower | 1.40× slower |
| 1024B | 37.2 | 44.7 | 72.7 | 379.9 | 1.20× slower | **1.63× faster** |

## 2. Churn, non-writing (`bench_churn_alloc`, working-set reuse — 1 free + 1 alloc per op)

| Size | Sefer 0.2.1 | Sefer 0.3.0 | mimalloc | System | vs 0.2.1 | vs mimalloc |
|---|---:|---:|---:|---:|---|---|
| 16B | 26.0 | 26.0 | 20.5 | 159.2 | ~parity | 1.27× slower |
| 64B | 35.7 | 24.7 | 28.6 | 137.1 | **1.45× faster** | **1.16× faster** |
| 256B | 44.1 | 25.2 | 40.0 | 206.8 | **1.75× faster** | **1.59× faster** |
| 1024B | 47.2 | 23.2 | 251.3 | 238.0 | **2.03× faster** | **10.83× faster** |

## 3. Churn + write (`bench_churn_alloc_write` — writes 16 B after each alloc)

| Size | Sefer 0.2.1 | Sefer 0.3.0 | mimalloc | System | vs 0.2.1 | vs mimalloc |
|---|---:|---:|---:|---:|---|---|
| 16B | 27.7 | 26.7 | 22.3 | 136.6 | 1.04× faster | 1.20× slower |
| 64B | 27.7 | 28.7 | 49.3 | 171.6 | 1.04× slower | **1.72× faster** |
| 256B | 54.7 | 27.5 | 42.5 | 213.9 | **1.99× faster** | **1.55× faster** |
| 1024B | 42.1 | 41.0 | 259.6 | 182.7 | 1.03× faster | **6.33× faster** |

## 4. Churn + teardown (`..._with_teardown` — DELIBERATE diagnostic: teardown stays inside the timed region)

The gap vs plain churn IS the segment decommit/release/re-reserve cost
(`benches/global_alloc.rs:460-469`).

| Size | Sefer 0.2.1 | Sefer 0.3.0 | mimalloc | System | vs 0.2.1 | vs mimalloc |
|---|---:|---:|---:|---:|---|---|
| 16B | 45.7 | 29.8 | 25.0 | 165.4 | **1.53× faster** | 1.19× slower |
| 64B | 42.9 | 33.2 | 29.5 | 177.6 | **1.29× faster** | 1.12× slower |
| 256B | 72.4 | 37.4 | 37.5 | 178.7 | **1.94× faster** | ~parity (1.00×) |
| 1024B | 112.6 | 84.8 | 65.0 | 184.9 | **1.33× faster** | 1.30× slower |

## 5. Vec_push (honest geometric `Vec<i64>` growth — 8 grow steps + stores per op, NOT scaled)

| Sefer 0.2.1 | Sefer 0.3.0 | mimalloc | System | vs 0.2.1 | vs mimalloc |
|---:|---:|---:|---:|---|---|
| 1117.2 | 1440.9 | 1169.4 | 2191.0 | 1.29× slower | 1.23× slower |

> **Noise flag, not a real regression.** `Vec_push` is the highest-variance arm this
> session: mimalloc's own `Vec_push` (byte-identical code in both runs) swung
> 1169.4 → 1433.7 ns (+22 %) purely from inter-run host drift, and in the 0.2.1 run
> Sefer's 1117.2 ns was *faster than mimalloc's 1433.7 ns in that same run*. The
> 0.3.0 run simply caught a noisier moment. R7 measured this same cell at
> 0.3.0 = 1032.8 ns / 0.2.1 = 1213.3 ns (0.3.0 **1.17× faster**); the structural
> position (Sefer ≈ mimalloc, both ≈ 2× faster than System) is unchanged. Do not read
> this row as a Round-8 regression.

## 6. segment_decommit_cycle (253 KiB decommit→release→re-reserve; UNSCALED — ns per batch of 34 alloc + 34 free)

| Sefer 0.2.1 | Sefer 0.3.0 | mimalloc | System | vs 0.2.1 | vs mimalloc |
|---:|---:|---:|---:|---|---|
| 547 400 | 1 875.8 | 7 183.2 | 203 470 | **~292× faster** | **3.83× faster** |

## 7. working_set_cycle (oscillating working set; UNSCALED — ns/batch, SeferAlloc-only, no mimalloc/System arm)

| Size | Sefer 0.2.1 | Sefer 0.3.0 | vs 0.2.1 |
|---|---:|---:|---|
| 16B | 303 650 | 306 200 | ~parity (1.01× slower) |
| 64B | 495 250 | 325 740 | **1.52× faster** |
| 256B | 788 360 | 316 460 | **2.49× faster** |
| 1024B | 1 262 500 | 366 810 | **3.44× faster** |

*(`pool_cap_sweep` is diagnostic-only — its signal is the `decommit_calls`
counter deltas, not criterion timing; excluded from the wall-clock comparison.
See `docs/perf/R7_POOL_CAP_PRESETS.md`.)*

---

## What moved since R7 (and what didn't)

### The structural wins are unchanged in shape, slightly smaller in magnitude

R7's headline was the small-segment pool + large cache collapsing the decommit/reuse
cycle, plus the chunked Registry cutting first-alloc commit charge. Both landed
before R7's anchor (`49046ef`) and are still present at `860d897`, so they re-appear
here in the same form:

- **segment_decommit_cycle:** ~292× faster (R7: ~318×). Same order of magnitude; the
  gap is run-to-run noise on the 0.2.1 side (547 µs vs R7's 406 µs — the OS-call
  storm this bench stress is exactly the noisy part).
- **working_set_cycle:** up to ~3.44× faster at 1024 B (R7: up to ~4.03×). Same
  monotone-by-size shape; 16 B is flat in this run (R7 had it at 1.68×). The 16 B
  cell is within-host-noise in both reports.
- **Churn family at 64 B+:** 0.3.0 still wins 64–256 B by ~1.4–2.0× and 1024 B by
  ~2.0–10.8× (vs 0.2.1) — the magazine/reuse advantage R7 reported, re-confirmed.

### What Round 8 actually changed (and why it does NOT show up as a new wall-clock win here)

Round 8 (`ffd3215..de4c4ae`) plus R9-1 (`860d897`) landed four things on top of the
R7 anchor. None of them is a new *default-bundle, measured-here* speedup, which is
why the table above looks like R7 with refreshed noise rather than a step change:

1. **Segment-directory O(1) lookup promoted to `production`** (#216 / #214 / #215).
   This is a constant-factor lookup-path improvement inside the allocator. It is real,
   but it is a single lookup per op buried under the alloc/free work the bench times,
   so it does not move the ns/op columns past the ±15–20 % noise floor.
2. **Tighter payload boundary** — a memory-layout / accounting refinement, not a
   hot-path speedup; invisible to these benches.
3. **Large-path zero-skip for `alloc_zeroed`** (#221, made miri-safe by R9-1). The
   harness measured here exercises `alloc`/`dealloc` through `GlobalAlloc`, **not**
   `alloc_zeroed`, so this win is out of scope for this table — it would show up in a
   dedicated `alloc_zeroed` bench, not in `bench_direct_alloc` / churn.
4. **Removal of decommit-on-pool-admission for `alloc-lazy-commit` small segments**
   (#223). This is gated behind the **`alloc-lazy-commit`** feature, which is **opt-in
   and NOT in the default `production` feature bundle this table measures** (the bench
   runs `cargo bench --features production`). Its gain is therefore not reflected
   here.

The honest one-line summary: **Round 8 was correctness, feature-gated opt-in, and
constant-factor/lookup work; it did not change the default-bundle hot path enough to
move these wall-clock numbers past the host-noise floor.** That is the expected
outcome, not a missed optimisation.

### Out of scope for this comparison (so a reader does not over-read the table)

- **`medium-classes`** — still feature-gated opt-in, not in the default `production`
  bundle measured here. Any medium-class wins are not in this table.
- **`alloc-lazy-commit`** — still feature-gated opt-in, not in the default `production`
  bundle measured here. The #223 decommit-on-admission removal is a real win for
  callers that opt into `alloc-lazy-commit`, but it is invisible to this `production`
  measurement.
- **`alloc_zeroed` large-path zero-skip (#221 / R9-1)** — not exercised by this
  harness (it uses `alloc`/`dealloc`, not `alloc_zeroed`).
- **First-alloc commit charge** (the R6/R7 ~128 MiB → ~6 MiB Windows commit-charge
  reduction) — a memory axis, not a speed axis; not measurable by criterion, and
  unchanged since R6/R7 (the chunked Registry is still in place). See R7's "What
  changed" section for the original measurement.

---

## Takeaways

- **0.3.0 vs 0.2.1:** faster on the reuse workloads that matter — churn (64 B–1 KiB)
  by ~1.4–2.0×, churn-with-teardown by ~1.3–1.9×, the decommit cycle **~292×**, and
  the oscillating working-set cycle up to **~3.44×**. No real regression: the
  cold-direct 256 B/1 KiB and `Vec_push` cells that look like regressions are within
  the documented inter-run host-noise floor (mimalloc's own `Vec_push` swung ±22 %
  between the two runs on identical code). The shape and order of magnitude match R7.
- **0.3.0 vs mimalloc:** wins the whole churn family at 64 B+ (up to **10.8×** at
  1024 B non-writing churn) and the decommit cycle (3.83×); loses on the cold path at
  small sizes (16–64 B, ~1.7–2.9×) — the same known scalar-`GlobalAlloc` cold-path
  gap R7 flagged, still out of scope for a batch/scoped API.
- **Versus R7:** the current version did **not** move meaningfully on the default
  wall-clock bundle. Round 8 + R9-1 were correctness / feature-gated / constant-factor
  work; the multiplicative reuse-cycle wins R7 reported are intact (re-confirmed at
  ~292× decommit, ~3.44× working-set), just refreshed for the current noise sample.
  Anything that would move this table further is behind `medium-classes` /
  `alloc-lazy-commit` (opt-in) or a future batch API (cold path), none of which this
  `production` measurement captures.
- Numbers are directional (±15–20 % host noise, with this session's mimalloc/System
  arms drifting a bit more than R7's); UNSCALED groups (6–7) are higher-variance but
  the gaps there are multiplicative, far beyond noise. Re-run
  `npm run bench:table` on both `main` and `bench/0.2.1` for fresh numbers.
