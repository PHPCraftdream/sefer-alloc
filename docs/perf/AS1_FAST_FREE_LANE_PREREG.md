# AS-1 — own-thread small free accept-only fast lane: pre-registration

Written and committed before the confirmatory data.

**Change (this branch).** `HeapCore::dealloc` first runs `dealloc_fast`
(`src/registry/heap_core/free/dealloc_fast.rs`, design A-S of the @ox memo).
The lane accepts a free only on a tier-1 memo hit, for a Small or Primordial
segment, with the same checks as `small_free_guard`: payload start, bump,
16-byte alignment and `state == encode(c, LIVE)`, class from the Layout. It
then pushes to the magazine (`cnt < FREE_PARK_CAP[c]`) or calls the
full-magazine tail. Every other case tail-calls the unchanged chain
(`dealloc_routing`), which re-derives everything, so rejects and their
counters keep the original paths. Compiled out under `hardened`.

**Mechanism (screening).** WSL disassembly of `b4_pmu_probe::sefer_dealloc`
drops from about 69 instructions with three pushes to 59 with one. In the
probe `inter` ABBA (10 repeats), the in-process sefer/mi ratio improves by
4.5–5.5 % in every scenario (pair, single-class churn, cross-class).

**Judge.** `node scripts/bench-table-paired.mjs --features production`
(3 processes × 200 rounds), aligned code
(`-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5`).
Two ABBA sets: base, as1, as1, base, then as1, base, base, as1.

**Rule (magnitude rule, derived by `node scripts/g1_magnitude_verdict.mjs`, applied to each set).**
Decided on the sefer time between the builds. GO if all of the following hold:

1. the geometric mean over the 12 cells is ≤ −0.5 %;
2. the agreed wins are ≥ 2 × the agreed losses;
3. the largest agreed loss is ≤ 3 %;
4. no cell turns into the LOSS class against mimalloc (sefer/mi > 1.03).

The variant is GO if both sets are GO. Before a merge the MT controls are
reported as well: larson/mstress T = 1, 2, 4 (`examples/malloc_macro.rs`).

## Verdict

Derived by `node scripts/g1_magnitude_verdict.mjs docs/perf/as1/set1 base as1` and `.../set2 base as1` (the per-set CSVs are copies of the `s1*`/`s2*` runs; manifests carry the commit and binary hashes).

| Cell (set 1) | ratio d pair 1 / 2 | ratio agreed | sefer time pair 1 / 2 | sefer time agreed | sefer/mi before → after |
|---|---:|---:|---:|---:|---|
| direct 16 B | -10.7 % / -8.0 % | -8.0 % | -8.8 % / -8.9 % | -8.8 % | 1.518 → 1.376 |
| direct 64 B | -13.4 % / -12.2 % | -12.2 % | -14.1 % / -12.9 % | -12.9 % | 1.081 → 0.942 |
| direct 256 B | -11.9 % / -12.0 % | -11.9 % | -13.0 % / -12.0 % | -12.0 % | 0.982 → 0.864 |
| direct 1024 B | -5.1 % / -8.3 % | -5.1 % | -5.9 % / -8.3 % | -5.9 % | 0.598 → 0.558 |
| churn 16 B | -3.4 % / -2.6 % | -2.6 % | -4.2 % / -2.6 % | -2.6 % | 0.987 → 0.958 |
| churn 64 B | -0.5 % / -2.8 % | -0.5 % | -4.3 % / -4.3 % | -4.3 % | 0.885 → 0.871 |
| churn 256 B | -5.8 % / -6.6 % | -5.8 % | -5.8 % / -3.4 % | -3.4 % | 0.654 → 0.613 |
| churn 1024 B | -6.2 % / -18.1 % | -6.2 % | -1.7 % / -3.3 % | -1.7 % | 0.351 → 0.309 |
| churn_write 16 B | -2.5 % / -0.9 % | -0.9 % | -3.3 % / -0.9 % | -0.9 % | 1.013 → 0.996 |
| churn_write 64 B | -2.5 % / -0.8 % | -0.8 % | -4.1 % / -1.7 % | -1.7 % | 1.000 → 0.983 |
| churn_write 256 B | -4.6 % / -4.9 % | -4.6 % | -9.0 % / -4.9 % | -4.9 % | 0.747 → 0.711 |
| churn_write 1024 B | +4.9 % / -2.0 % | +0.0 % | +0.8 % / -0.8 % | +0.0 % | 0.317 → 0.321 |

Sefer time (A/B of the two builds): geometric mean -5.8 %; agreed wins 59.0 pp, agreed losses 0.0 pp; largest agreed loss +0.0 %.
Ratio to mimalloc: geometric mean over 12 cells: -6.0 %; agreed wins 58.5 pp, agreed losses 0.0 pp (ratio ∞); largest agreed loss +0.0 %.

Magnitude rule: gm -5.8 % (<= -0.5 %), wins/losses 59.0/0.0 pp (>= 2x), largest loss +0.0 % (<= 3 %), new LOSS cells 0 (0) => **GO**

| Cell (set 2) | ratio d pair 1 / 2 | ratio agreed | sefer time pair 1 / 2 | sefer time agreed | sefer/mi before → after |
|---|---:|---:|---:|---:|---|
| direct 16 B | -10.1 % / -7.3 % | -7.3 % | -8.3 % / -8.3 % | -8.3 % | 1.510 → 1.379 |
| direct 64 B | -13.4 % / -13.5 % | -13.4 % | -12.1 % / -12.9 % | -12.1 % | 1.085 → 0.939 |
| direct 256 B | -11.2 % / -8.4 % | -8.4 % | -10.1 % / -7.8 % | -7.8 % | 0.966 → 0.871 |
| direct 1024 B | -7.3 % / -3.0 % | -3.0 % | -5.9 % / -3.0 % | -3.0 % | 0.594 → 0.564 |
| churn 16 B | -2.5 % / -4.2 % | -2.5 % | -0.9 % / -2.6 % | -0.9 % | 0.987 → 0.954 |
| churn 64 B | -0.9 % / -6.4 % | -0.9 % | -0.9 % / -2.7 % | -0.9 % | 0.897 → 0.864 |
| churn 256 B | -4.2 % / -2.7 % | -2.7 % | -2.6 % / -4.3 % | -2.6 % | 0.636 → 0.614 |
| churn 1024 B | -10.0 % / -15.2 % | -10.0 % | -4.1 % / -5.0 % | -4.1 % | 0.361 → 0.316 |
| churn_write 16 B | -2.5 % / +0.0 % | +0.0 % | -1.7 % / +0.0 % | +0.0 % | 1.004 → 0.991 |
| churn_write 64 B | -1.7 % / -0.8 % | -0.8 % | -0.9 % / +0.0 % | +0.0 % | 0.988 → 0.975 |
| churn_write 256 B | -7.9 % / +8.4 % | +0.0 % | -2.5 % / +9.1 % | +0.0 % | 0.721 → 0.723 |
| churn_write 1024 B | -0.3 % / -5.5 % | -0.3 % | -1.6 % / -3.8 % | -1.6 % | 0.322 → 0.313 |

Sefer time (A/B of the two builds): geometric mean -4.0 %; agreed wins 41.2 pp, agreed losses 0.0 pp; largest agreed loss +0.0 %.
Ratio to mimalloc: geometric mean over 12 cells: -5.6 %; agreed wins 49.4 pp, agreed losses 0.0 pp (ratio ∞); largest agreed loss +0.0 %.

Magnitude rule: gm -4.0 % (<= -0.5 %), wins/losses 41.2/0.0 pp (>= 2x), largest loss +0.0 % (<= 3 %), new LOSS cells 0 (0) => **GO**

Reading: **GO** in both sets, with no agreed loss in any cell. The warm batch improves the most (16 B −8.3…−8.8 %, 64 B −12…−13 %, now a win against mimalloc). Churn gains are −1…−4 %.

## MT controls (larson / mstress, `examples/malloc_macro.rs`, 30 ABBA runs per arm)

Derived by `node scripts/as1_mt_table.mjs docs/perf/as1/mt_malloc_macro_raw.txt docs/perf/AS1_FAST_FREE_LANE_mt_summary.csv`:

| Workload | T | runs | sefer before (M ops/s) | sefer after | Δ | sefer/mi before | sefer/mi after |
|---|---:|---:|---:|---:|---:|---:|---:|
| larson | 1 | 30/30 | 44.10 | 44.60 | +1.1 % | 0.789 | 0.789 |
| larson | 2 | 30/30 | 53.44 | 54.45 | +1.9 % | 1.156 | 1.177 |
| larson | 4 | 30/30 | 96.65 | 93.44 | -3.3 % | 1.184 | 1.141 |
| mstress | 1 | 30/30 | 55.68 | 53.53 | -3.9 % | 0.776 | 0.750 |
| mstress | 2 | 30/30 | 91.87 | 90.79 | -1.2 % | 0.917 | 0.917 |
| mstress | 4 | 30/30 | 140.92 | 163.84 | +16.3 % | 1.006 | 0.973 |

Throughput: higher is better; Δ = after/before − 1 of the run medians; sefer/mi > 1 means faster than mimalloc in the same run.

Run-to-run spread is about ±4 % (mimalloc in the same runs swings by up to ±14 % at T = 4). Callgrind of `g1_mt_one` at T = 1, 300 000 steps (`docs/perf/as1/mt_callgrind_totals.txt`): larson 79 358 209 → 76 686 530 Ir (−3.4 %), mstress 72 901 949 → 68 066 414 Ir (−6.6 %). Fewer instructions on both, so the mstress T = 1 −3.9 % throughput is run noise, not a cost.
