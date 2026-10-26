# B1 — pre-registration (fixed before any J1′ v2 run of these variants)

Base: `main` @ `1667be06` (depot shipped). Measurement branch `b4-bulk`.
Variants (patches in `docs/perf/b4_ablate/patches/`):

- `b1a_flush` — `flush_class_internal` single pass: the run end is detected
  inside `flush_run`, no `Option` / per-accept counter. Same per-block guards
  (span, state byte class + state, `write_next`, FREE) in the same order.
- `b1c_free_cold` — the full-magazine free tail (depot park, R1-01 swap-top,
  overflow half-flush) moves into `#[cold] #[inline(never)]
  free_magazine_full`; the push fast path is unchanged.
- `b1ac` — both.

Callgrind (WSL, `b4_pmu_probe bulk sefer 64 16384 40`, Ir per block, ИЗМЕРЕНО
before this file): base 303.7; b1a 292.7 (flush 58.1 → 47.2); b1c 294.9
(free 81.0 → 68.0). Instructions are not time; the verdict is the time judge.

Protocol: J1′ v2 ablation (`scripts/b4_ablate_run.mjs`, 20 processes × 2000
blocks, all 11 kernels), effect = variant median − null median
(`scripts/b4_ablate_vs_null.mjs`), 95 % CI by process bootstrap.

Thresholds, per variant:

1. Win: bulk16k OR bulk64 effect CI upper < 0.
2. No harm: pair16, pair64, larson effect ≤ +1 % and CI upper ≤ +1.5 %.
3. No harm, other kernels: not (CI lower > 0 AND effect > +2 %).

GO = 1 ∧ 2 ∧ 3. A variant failing 2 or 3 is NO-GO regardless of 1. If both
single variants pass, `b1ac` decides what ships (it must pass 1–3 itself).
Path activation: every bulk portion overflows the magazine (16384 frees per
pass, flush every 8 frees — callgrind: 89 320 `flush_class_internal` calls per
720 896 blocks), so both changed paths run on bulk by construction.
