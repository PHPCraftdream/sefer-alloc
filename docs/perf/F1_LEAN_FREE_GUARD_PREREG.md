# F-1 — lean small-free guard: pre-registration

Written and committed before the confirmatory data.

**Change (this branch).** `small_free_guard` (`src/registry/heap_core/free/dealloc_own_base.rs`)
reads the state byte first and folds the misalignment and byte checks into one
branch; the metadata-offset (`payload_start`) reject and the reject-reason split
move into an inlined classifier on the mismatch branch, in the former order.
The accepted set is unchanged: the state bytes of offsets below the payload
start lie inside the never-written prefix of the state map (zero), so an
`encode(c, LIVE)` match already implies `off >= payload_start`; the whole map is
committed with the metadata. The bump check stays on the hot path.

**Mechanism evidence (screening, WSL perf/callgrind, `b4_pmu_probe inter`).**
The J1 churn kernels share one slot ring, so churn16 is a cross-class load
(free 64/256 B, alloc 16 B). There sefer pays ~108–112 Ir per alloc+free pair
against mimalloc's 70–96; free alone is 68–70 Ir against 23–25. The base
`sefer_dealloc` saves three registers on entry; this variant saves one.

**Judge.** `node scripts/bench-table-paired.mjs --features production`
(3 processes × 200 rounds), aligned code
(`-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5`),
order base, f1c, f1c, base. Per cell and pair, d = (f1c sefer/mi) / (base sefer/mi) − 1.

**Rule.** A cell is WIN if d < −1.5 % in both pairs, LOSS if d > +1.5 % in both
pairs, otherwise no change. GO if at least one WIN and no LOSS; otherwise NO-GO.

## Verdict (derived by `node scripts/f1_verdict.mjs docs/perf/f1 docs/perf/F1_LEAN_FREE_GUARD_summary.csv`)

Measured trees: base `dfb8c143` (origin/main), f1c `49377f54` (manifests in `docs/perf/f1/`).

| Cell | sefer ns before (r1 / r2) | sefer ns after (r1 / r2) | mimalloc ns | sefer/mi before | sefer/mi after | d pair 1 / pair 2 | cell verdict | class before → after |
|---|---:|---:|---:|---:|---:|---:|---|---|
| direct 16 B | 7.28 / 7.18 | 7.37 / 7.28 | 4.79 | 1.503 | 1.538 | +2.4 % / +2.4 % | LOSS | LOSS → LOSS |
| direct 64 B | 7.03 / 6.98 | 6.84 / 6.69 | 6.40 | 1.091 | 1.061 | -2.0 % / -3.5 % | WIN | LOSS → LOSS |
| direct 256 B | 8.01 / 7.91 | 7.91 / 7.81 | 8.00 | 0.994 | 0.985 | -1.8 % / -0.0 % | no change | par → par |
| direct 1024 B | 10.40 / 10.16 | 10.64 / 10.50 | 17.02 | 0.605 | 0.620 | +3.5 % / +1.6 % | LOSS | win → win |
| churn 16 B | 5.86 / 5.76 | 5.86 / 5.81 | 6.01 | 0.975 | 0.964 | -2.4 % / +0.0 % | no change | par → win |
| churn 64 B | 5.81 / 5.81 | 5.81 / 5.76 | 6.71 | 0.876 | 0.853 | -2.8 % / -2.3 % | WIN | win → win |
| churn 256 B | 5.91 / 5.76 | 5.96 / 5.81 | 9.52 | 0.596 | 0.636 | +8.2 % / +5.2 % | LOSS | win → win |
| churn 1024 B | 5.91 / 6.01 | 5.96 / 5.91 | 19.08 | 0.294 | 0.332 | +19.1 % / +6.4 % | LOSS | win → win |
| churn_write 16 B | 5.96 / 5.86 | 5.91 / 5.96 | 5.87 | 1.004 | 1.013 | -0.8 % / +2.5 % | no change | par → par |
| churn_write 64 B | 6.01 / 5.81 | 6.01 / 5.91 | 6.08 | 0.972 | 0.980 | +0.0 % / +1.7 % | no change | par → par |
| churn_write 256 B | 6.10 / 6.05 | 6.10 / 6.15 | 9.06 | 0.671 | 0.681 | -7.5 % / +10.5 % | no change | win → win |
| churn_write 1024 B | 6.88 / 6.84 | 6.84 / 6.98 | 20.30 | 0.336 | 0.342 | +0.5 % / +3.1 % | no change | win → win |

WIN cells: 2 of 12; LOSS cells: 4 of 12. Verdict: **NO-GO**.

ns = median over 3 processes of the per-process minimum ns/pair; sefer/mi = mean of the two runs per arm; class: < 0.97 win, > 1.03 LOSS, else par.

Reading: the four LOSS cells are direct 16 B (sefer itself +1.2 %, a real loss), and churn 256 / churn 1024 / direct 1024, where the sefer time is flat and the ratio moves with the mimalloc denominator. By the pre-registered rule the variant is **NO-GO**; the rule is not re-read after the data. Code kept under tag `archive/f1-lean-free-guard-measured`.
