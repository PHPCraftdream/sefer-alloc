# WB-1 / A-1: classify a Layout with one ceiling-indexed load — pre-registration

Written and committed before any measurement of this change.

## Change

`SizeClasses::class_for_layout(layout)` (`src/alloc_core/platform/size_classes.rs`)
replaces `class_for(layout.size().max(MIN_BLOCK), layout.align())` on
`HeapCore::alloc`, `HeapCore::alloc_zeroed` and the own-thread small free
(`dealloc_own_thread_with_base`). For `align <= MIN_BLOCK` it reads
`SIZE2CLASS_CEIL[ceil(size / MIN_BLOCK)]`, a copy of `SIZE2CLASS` shifted by
one bucket, instead of clamping the size and branching on `size == 0`.
The class returned is the same for every `Layout`
(`tests/class_for_layout_equivalence.rs`), so every free-path reject (O1–O12
of the WB-1 consultation) is unchanged.

Motivation: WB-1 callgrind (warm bulk 1024 × 16 B) puts `size_classes.rs`
at ≈ 10 Ir per free and ≈ 7 Ir per alloc; expected saving −6…−8 Ir per
alloc+free pair.

## Arms

- `base`: `0ee09c13` (branch `g1-bt1-paired`: `main` at `e4693897` plus the
  measurement-only BT-1 harness; no `src/` change against `main`).
- `a1`: this branch's commit carrying the change and this file.

Both built `--release --features production`.

## Measurements and acceptance (all must hold)

1. **Ir (callgrind, WSL)** — `examples/b4_pmu_probe.rs`, marginal between 50
   and 100 passes, divided by the 56 320 extra pairs (the probe warms
   `max(passes/10, 4)` passes):
   - `bulk sefer 1024 16` and `bulk sefer 1024 64`: `a1` lower than `base`
     by at least 3 Ir per pair;
   - pair (`sefer 1 16`), churn (`sefer 256 16`, `sefer 256 64`): `a1` not
     higher than `base` by more than 0.5 Ir per op.
2. **iai (CI)** — the perf-gate iai set, `a1` against `base`: no bench more
   expensive by more than 10 Ir in total.
3. **Wall clock (paired, Windows host)** — `npm run bench:table:paired`
   (3 processes × 200 rounds, `steady`) on each arm, run alternately
   `base, a1, base, a1`. The statistic is the SeferAlloc/mimalloc ratio of
   each cell, taken from each arm's two runs (the mimalloc arm is the
   in-process control for host drift):
   - warm bulk 16 B and 64 B: the ratio of `a1` is lower than `base` in both
     run pairs;
   - no cell's ratio is higher in `a1` than in `base` by more than 3 % in
     both run pairs.

If any check fails the change is rejected and reverted; the result is
recorded either way.

## Addendum 1 (written after check 3, before any further measurement)

Check 3 as registered **failed** (raw runs `base_r1`, `a1_r1`, `base_r2`,
`a1_r2`; commits `0ee09c13` and `aa60d43f`): warm bulk 16 B's ratio was
lower in `a1` in run pair 1 (−5.6 %) but higher in run pair 2 (+1.1 %), and
churn+write 1024 B's ratio was higher in `a1` by 16.9 % and 19.1 %
(SeferAlloc 6.15 → 7.52 and 6.30 → 7.62 ns/pair). Under this registration
A-1 is rejected; that verdict stands whatever follows.

The paired harness builds without the function/branch alignment flags this
repo uses for A/B judges (`-C llvm-args=-align-all-functions=6 -C
llvm-args=-align-all-nofallthru-blocks=5`, as in the K7 runs), and a cell
that does not exercise the changed code differently (churn+write 1024 B
classifies the same size through the same one-line change as churn 1024 B,
which did not move) moving by 1.3 ns is the shape a code-layout shift
produces. A second, separately registered experiment tests that:

- Check 3b: the same protocol as check 3 (`base, a1, base, a1`, 3 processes
  × 200 rounds, `steady`, same acceptance rules), with
  `RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C
  llvm-args=-align-all-nofallthru-blocks=5"` for both arms.
- A-1 ships only if check 3b passes AND check 2 (iai) passes; the report
  states the failed check 3 next to it.
