# FG-1/B1-R — cold TLS bind by reference, re-measured under the magnitude rule: pre-registration

Written and committed before the confirmatory data.

**Why a re-measurement.** FG-1/B1 (`docs/perf/FG1_B1_COLD_BIND_BYREF_PREREG.md`)
was rejected by a count rule on the ratio to mimalloc. Re-read by magnitude
(`node scripts/g1_magnitude_verdict.mjs docs/perf/fg1/b1 base b1`), the sefer
time over the two ABBA pairs shows a geometric mean of −2.4 %, agreed wins of
2.1 pp (warm batch 16 B −1.3 %, churn_write 16 B −0.8 %) and no agreed loss.
The ratio losses came from the mimalloc denominator, whose code also moves
between the two builds. The owner's rule (2026-10-30) judges by the size of
wins and losses, not their count. Because this re-read happens after the data,
the verdict must come from fresh data.

**Change.** The same patch as FG-1/B1, `src/global/tls_heap.rs`
(`bind_slow_tagged_with_config` takes `&LargeCacheConfig`), on `origin/main`
`4e97c2da`. Base: `dfb8c143`, whose production source is identical to
`4e97c2da` (docs and scripts only in between).

**Judge.** `node scripts/bench-table-paired.mjs --features production`
(3 processes × 200 rounds), aligned code
(`-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5`).
Two ABBA blocks: base, b1, b1, base, then b1, base, base, b1. The four
pairs are taken as two two-pair sets.

**Rule (derived by `node scripts/g1_magnitude_verdict.mjs`, applied to each set).**
Decided on the sefer time between builds. GO if all of the following hold:

1. the geometric mean over the 12 cells is ≤ −0.5 %;
2. the agreed wins are ≥ 2 × the agreed losses (a cell's effect counts only
   when both of its pairs agree in sign; its size is the smaller |Δ|);
3. the largest agreed loss is ≤ 3 %;
4. no cell turns into the LOSS class against mimalloc (sefer/mi > 1.03).

The variant is GO if both sets are GO.

## Verdict

Derived by `node scripts/g1_magnitude_verdict.mjs docs/perf/fg1/b1r/set1 base b1` and `.../set2 base b1` (the per-set CSVs are copies of the `s1*`/`s2*` runs).

| Cell (set 1) | ratio d pair 1 / 2 | ratio agreed | sefer time pair 1 / 2 | sefer time agreed | sefer/mi before → after |
|---|---:|---:|---:|---:|---|
| direct 16 B | -0.7 % / -2.4 % | -0.7 % | -0.7 % / -1.4 % | -0.7 % | 1.523 → 1.500 |
| direct 64 B | +2.2 % / -0.6 % | +0.0 % | +1.4 % / -2.8 % | +0.0 % | 1.069 → 1.077 |
| direct 256 B | -0.0 % / -0.7 % | -0.0 % | -1.9 % / -1.3 % | -1.3 % | 0.969 → 0.966 |
| direct 1024 B | -3.3 % / -1.3 % | -1.3 % | -2.4 % / -1.0 % | -1.0 % | 0.601 → 0.587 |
| churn 16 B | +1.7 % / +2.6 % | +1.7 % | -0.9 % / +2.6 % | +0.0 % | 0.966 → 0.987 |
| churn 64 B | -0.8 % / +3.4 % | +0.0 % | +0.0 % / +1.8 % | +0.0 % | 0.880 → 0.891 |
| churn 256 B | +6.3 % / +1.1 % | +1.1 % | +0.9 % / +0.0 % | +0.0 % | 0.611 → 0.633 |
| churn 1024 B | +4.4 % / +6.8 % | +4.4 % | -1.6 % / +1.7 % | +0.0 % | 0.336 → 0.354 |
| churn_write 16 B | +0.9 % / -0.9 % | +0.0 % | -0.8 % / +0.0 % | +0.0 % | 1.017 → 1.017 |
| churn_write 64 B | +3.3 % / +0.0 % | +0.0 % | +0.0 % / +0.9 % | +0.0 % | 0.963 → 0.979 |
| churn_write 256 B | +4.4 % / -1.3 % | +0.0 % | -8.3 % / -2.5 % | -2.5 % | 0.709 → 0.719 |
| churn_write 1024 B | +4.9 % / +3.8 % | +3.8 % | +0.8 % / +4.6 % | +0.8 % | 0.327 → 0.341 |

Sefer time (A/B of the two builds): geometric mean -0.5 %; agreed wins 5.4 pp, agreed losses 0.8 pp; largest agreed loss +0.8 %.
Ratio to mimalloc: geometric mean over 12 cells: +1.4 %; agreed wins 2.0 pp, agreed losses 11.0 pp (ratio 0.18); largest agreed loss +4.4 %.

Magnitude rule: gm -0.5 % (<= -0.5 %), wins/losses 5.4/0.8 pp (>= 2x), largest loss +0.8 % (<= 3 %), new LOSS cells 0 (0) => **NO-GO**

| Cell (set 2) | ratio d pair 1 / 2 | ratio agreed | sefer time pair 1 / 2 | sefer time agreed | sefer/mi before → after |
|---|---:|---:|---:|---:|---|
| direct 16 B | -0.7 % / +0.0 % | +0.0 % | -0.7 % / +0.0 % | +0.0 % | 1.500 → 1.495 |
| direct 64 B | -0.8 % / -3.6 % | -0.8 % | +0.0 % / -2.1 % | +0.0 % | 1.081 → 1.057 |
| direct 256 B | +0.0 % / -2.5 % | +0.0 % | +0.0 % / -0.6 % | +0.0 % | 0.969 → 0.957 |
| direct 1024 B | +0.5 % / -0.3 % | +0.0 % | +0.5 % / +0.0 % | +0.0 % | 0.592 → 0.592 |
| churn 16 B | -2.5 % / -0.0 % | -0.0 % | +0.9 % / -0.9 % | +0.0 % | 0.996 → 0.983 |
| churn 64 B | +0.0 % / -4.0 % | +0.0 % | +0.0 % / -1.7 % | +0.0 % | 0.914 → 0.895 |
| churn 256 B | +3.1 % / +5.2 % | +3.1 % | +0.9 % / -0.9 % | +0.0 % | 0.615 → 0.641 |
| churn 1024 B | +2.0 % / +0.6 % | +0.6 % | +0.8 % / +0.0 % | +0.0 % | 0.347 → 0.352 |
| churn_write 16 B | -1.7 % / +0.0 % | +0.0 % | -0.8 % / -0.8 % | -0.8 % | 1.013 → 1.004 |
| churn_write 64 B | +0.9 % / +0.8 % | +0.8 % | +0.9 % / +0.8 % | +0.8 % | 0.979 → 0.988 |
| churn_write 256 B | -1.3 % / -9.2 % | -1.3 % | +0.8 % / -0.8 % | +0.0 % | 0.691 → 0.653 |
| churn_write 1024 B | -0.5 % / +2.3 % | +0.0 % | -0.8 % / +0.8 % | +0.0 % | 0.326 → 0.329 |

Sefer time (A/B of the two builds): geometric mean -0.2 %; agreed wins 0.8 pp, agreed losses 0.8 pp; largest agreed loss +0.8 %.
Ratio to mimalloc: geometric mean over 12 cells: -0.5 %; agreed wins 2.1 pp, agreed losses 4.5 pp (ratio 0.47); largest agreed loss +3.1 %.

Magnitude rule: gm -0.2 % (<= -0.5 %), wins/losses 0.8/0.8 pp (>= 2x), largest loss +0.8 % (<= 3 %), new LOSS cells 0 (0) => **NO-GO**

Reading: the effect is real but small (sefer-time geometric mean −0.5 % and −0.2 %). The screening −2.4 % came from one slow base run (warm batch 64 B −31 % in its second pair). **NO-GO** under the magnitude rule; code under tag `archive/fg1-b1-cold-bind-byref-measured`.
