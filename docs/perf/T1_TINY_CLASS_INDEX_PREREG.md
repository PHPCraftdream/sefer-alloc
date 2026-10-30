# T-1 — linear class index for sizes up to 80 B: pre-registration

Written and committed before the confirmatory data.

**Change (this branch).** `SizeClasses::class_for_small_align`
(`src/alloc_core/platform/size_classes.rs`) returns `(size - 1) >> 4` for
`size <= LINEAR_MAX = 80` instead of loading `SIZE2CLASS`; a const assert pins
`SIZE_CLASS_TABLE[i] == (i + 1) * 16` for every class below 80 B. The result is
the same class for every size, so alloc and free keep their exact semantics.

**Mechanism (screening, WSL disassembly of `b4_pmu_probe`).** For sizes up to
80 B both `sefer_alloc` and `sefer_dealloc` lose the table load (and on Linux
its GOT load) and the `classes[c]` bounds check: −3 instructions and two
dependent loads off the path to the magazine count load. Sizes above 80 B pay
one more compare and branch.

**Judge.** `node scripts/bench-table-paired.mjs --features production`
(3 processes × 200 rounds), aligned code
(`-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5`),
order base, t1, t1, base. Per cell and pair, d = (t1 sefer/mi) / (base sefer/mi) − 1.

**Rule.** A cell is WIN if d < −1.5 % in both pairs, LOSS if d > +1.5 % in both
pairs, otherwise no change. GO if at least one WIN and no LOSS; otherwise NO-GO.

## Verdict (derived by `node scripts/t1_verdict.mjs docs/perf/t1 docs/perf/T1_TINY_CLASS_INDEX_summary.csv`)

Measured trees: base `dfb8c143` (origin/main), t1 `f4b59c00` (manifests in `docs/perf/t1/`).

| Cell | sefer ns before (r1 / r2) | sefer ns after (r1 / r2) | mimalloc ns | sefer/mi before | sefer/mi after | d pair 1 / pair 2 | cell verdict | class before → after |
|---|---:|---:|---:|---:|---:|---:|---|---|
| direct 16 B | 7.18 / 7.13 | 7.76 / 7.71 | 4.72 | 1.534 | 1.617 | +4.9 % / +6.0 % | LOSS | LOSS → LOSS |
| direct 64 B | 6.84 / 6.79 | 7.23 / 7.47 | 6.35 | 1.077 | 1.153 | +4.9 % / +9.3 % | LOSS | LOSS → LOSS |
| direct 256 B | 7.57 / 7.71 | 7.81 / 7.96 | 7.87 | 0.978 | 0.994 | +0.1 % / +3.2 % | no change | par → par |
| direct 1024 B | 9.77 / 10.01 | 10.06 / 10.01 | 16.63 | 0.596 | 0.602 | +1.2 % / +0.6 % | no change | win → win |
| churn 16 B | 5.62 / 5.66 | 5.47 / 5.47 | 5.74 | 0.987 | 0.949 | -3.4 % / -4.3 % | WIN | par → win |
| churn 64 B | 5.52 / 5.66 | 5.42 / 5.42 | 6.35 | 0.884 | 0.851 | -3.3 % / -4.3 % | WIN | win → win |
| churn 256 B | 5.62 / 5.71 | 5.91 / 5.91 | 8.94 | 0.634 | 0.661 | +7.6 % / +1.2 % | no change | win → win |
| churn 1024 B | 5.86 / 5.86 | 6.20 / 6.10 | 17.55 | 0.341 | 0.344 | -6.2 % / +8.2 % | no change | win → win |
| churn_write 16 B | 5.76 / 5.81 | 5.62 / 5.62 | 5.70 | 1.013 | 0.987 | -3.4 % / -1.7 % | WIN | par → par |
| churn_write 64 B | 6.05 / 5.91 | 5.62 / 5.52 | 5.88 | 1.012 | 0.950 | -6.5 % / -5.8 % | WIN | par → win |
| churn_write 256 B | 5.81 / 6.01 | 6.10 / 6.20 | 8.50 | 0.680 | 0.744 | +2.0 % / +17.6 % | LOSS | win → win |
| churn_write 1024 B | 6.35 / 6.30 | 6.49 / 6.54 | 19.43 | 0.326 | 0.335 | -0.8 % / +6.5 % | no change | win → win |

WIN cells: 4 of 12; LOSS cells: 3 of 12. Verdict: **NO-GO**.

ns = median over 3 processes of the per-process minimum ns/pair; sefer/mi = mean of the two runs per arm; class: < 0.97 win, > 1.03 LOSS, else par.

### Screening probe (`b4_pmu_probe inter`, WSL; `node scripts/g1_probe_pair_table.mjs docs/perf/t1/probe_inter_base_vs_t1.csv` with `VBASE=a2base VX=t1`)

| Scenario | sefer before | sefer after | Δ sefer (median of pairs) | pairs a2 faster | sefer/mi before | sefer/mi after | class before → after |
|---|---:|---:|---:|---:|---:|---:|---|
| x1664 | 23.17 | 21.71 | -6.1 % | 18/20 | 1.353 | 1.274 | LOSS → LOSS |
| x16256 | 24.41 | 32.81 | +36.9 % | 0/20 | 1.399 | 1.888 | LOSS → LOSS |
| x64256 | 24.28 | 33.05 | +34.9 % | 0/20 | 1.397 | 1.898 | LOSS → LOSS |
| c16 | 21.88 | 20.69 | -5.2 % | 17/20 | 0.594 | 0.562 | win → win |
| c64 | 21.65 | 20.46 | -5.8 % | 18/20 | 0.597 | 0.566 | win → win |
| pair16 | 21.33 | 20.10 | -6.8 % | 19/20 | 1.243 | 1.174 | LOSS → LOSS |
| pair64 | 21.65 | 20.18 | -6.5 % | 20/20 | 1.207 | 1.134 | LOSS → LOSS |

sefer = median ticks/op (TSC) over runs; Δ = median over ABBA pairs of a2/base − 1 (negative = A-2 faster); sefer/mi = median of the per-run in-process ratio; class: < 0.97 win, > 1.03 LOSS, else par.

Reading: the shorter class chain is a real win on single-size churn (churn 16/64 B and churn_write 16/64 B, −3…−6.5 % in both pairs), but the size test is a new data-dependent branch on free: with frees of both sides of 80 B in random order (probe 16↔256, 64↔256) it mispredicts and costs +35 %, and the warm batch 16/64 B is slower by 5–9 % for a reason not isolated here. By the pre-registered rule the variant is **NO-GO**. Code kept under tag `archive/t1-tiny-class-index-measured`. Next trigger: a branch-free form that keeps the class off the table load.
