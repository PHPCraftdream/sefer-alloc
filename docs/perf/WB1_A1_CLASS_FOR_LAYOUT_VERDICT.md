# WB-1 / A-1: ceiling-indexed Layout classification — NO-GO (measurement only)

No runtime code ships. The change was built, measured against its
pre-registration (`WB1_A1_CLASS_FOR_LAYOUT_PREREG.md`, committed before any
data) and reverted in the same branch.

## What was tried

`SizeClasses::class_for_layout(layout)`: for `align <= 16` read
`SIZE2CLASS_CEIL[ceil(size / 16)]` (`SIZE2CLASS` shifted by one bucket)
instead of `class_for(size.max(16), align)`, on `HeapCore::alloc`,
`alloc_zeroed` and the own-thread small free. Measured commit `aa60d43f`
(check 3) / `a15e14b2` (check 3b), same `src/`; base `0ee09c13`.
Suggested by the WB-1 consultation (@ox) as the lowest-risk free-path cut.

## Results

| Check | Result |
|---|---|
| 1 — Ir, callgrind (WSL, `b4_pmu_probe`, marginal 50→100 passes / 56 320 pairs) | **pass**: bulk 1024 × 16 B 142.67 → 136.75 Ir/pair, × 64 B 142.71 → 136.71; pair16 / churn16 / churn64 173.8 → 167.2 Ir/op (`wb1_a1/callgrind_totals.txt`) |
| 2 — iai A/B on CI | not completed: the gate run was cancelled once checks 3 and 3b had failed |
| 3 — paired wall clock vs mimalloc (`base, a1, base, a1`) | **fail** ([check3/verdict.md](wb1_a1/check3/verdict.md)) |
| 3b — same, both arms built with function/branch alignment (Addendum 1) | **fail** ([check3b/verdict.md](wb1_a1/check3b/verdict.md)) |

The two failures agree:

- warm bulk 16 B (the target) did not get faster: its SeferAlloc/mimalloc
  ratio moved −5.6 % / +1.1 % (check 3) and +3.8 % / +1.0 % (check 3b);
- churn+write 1024 B got slower in all four run pairs: ratio +16.9 % /
  +19.1 % (check 3) and +26.7 % / +20.1 % (check 3b); SeferAlloc
  6.15–6.30 → 7.52–7.76 ns/pair.

So about 6 fewer instructions per pair did not turn into time on the target
cell, and one other cell lost about 1.3 ns per pair in every run. That
loss is reproducible with and without code alignment, so it is not
explained by function placement; its mechanism was not investigated (the
callgrind shapes measured do not include churn+write at 1024 B).

## Evidence

`docs/perf/wb1_a1/`: `callgrind_totals.txt` (raw `PROGRAM TOTALS` per run;
script `.rush`-local, arms built from `git archive` of the two commits);
`check3/` and `check3b/`: per-run paired tables (`*.md`, re-derivable byte
for byte with `node scripts/bench-table-paired.mjs --from
docs/perf/wb1_a1/<check>/<run>`), summaries (`*.csv`), manifests (commit,
binary sha256, rustc, CPU, OS, raw hashes), gzipped raw samples, and
`verdict.md`, derived by `node scripts/wb1_a1_verdict.mjs
docs/perf/wb1_a1/<check>`. Check 3b's `RUSTFLAGS` are in
`check3b/run_note.txt`.
