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
