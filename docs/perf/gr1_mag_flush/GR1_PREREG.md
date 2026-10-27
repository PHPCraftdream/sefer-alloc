# GR-1 — pre-registration (fixed before any J1′ v2 run of this variant)

Base: `gh-g2` @ `eb271b34` (production machine-identical to `main` @ `64f4f2f7`:
G1/G2 ghost-only, 586/586 functions and Ir equal). Measurement branch `gr1-meas`.
Variant (patch in `docs/perf/b4_ablate/patches/`):

- `gr1_mag_flush` — the three magazine flush sites (overflow half-flush,
  cap-branch eviction, teardown `flush_all_tcache`) call
  `flush_magazine_internal`, which skips `flush_run`'s per-block span check and
  state-byte check (always true on MAG-resident blocks, invariant I-MAG, ghost
  check `mag-flush/mag-resident`). Batch-free staging keeps the checked
  `flush_class_internal`.

Callgrind (WSL, growth16 probe, ИЗМЕРЕНО before this file): base 27 268 129 Ir
at n = 131072, variant 24 810 596 (−9.0 %); n = 32768 −8.5 %, n = 65536 −8.8 %.
Instructions are not time; the verdict is the time judge.

Protocol: J1′ v2 ablation (`scripts/b4_ablate_run.mjs`, 20 processes × 2000
blocks, all 11 kernels), effect = variant median − null median
(`scripts/b4_ablate_vs_null.mjs`), 95 % CI by process bootstrap.

Thresholds:

1. Win: bulk16k OR bulk64 effect CI upper < 0.
2. No harm: pair16, pair64, larson effect ≤ +1 % and CI upper ≤ +1.5 %.
3. No harm, other kernels: not (CI lower > 0 AND effect > +2 %).

GO = 1 ∧ 2 ∧ 3. Path activation: every bulk portion overflows the magazine
(16384 frees per pass, a half-flush every 8 frees), so the changed flush runs
on bulk by construction; the ghost build's `mag-flush/mag-resident` events
count every flushed block (tests/miri_mag_flush_resident.rs).
