# G1-K5 — pre-registration (fixed before any J1′ v2 run of this variant)

Base: `main` @ `37780bc7` + G1-K1 (`g1-k1`@`69777124`, PR #125). Measurement
branch `g1-k5-meas` (harness commit cherry-picked from `51eda7b1`).
Variant (patch in `docs/perf/b4_ablate/patches/`):

- `k5_depot_mid_classes` — the magazine depot (B1-D, item 117) covers every
  class up to 1024 B instead of up to 192 B: classes 0..7 keep
  `DEPOT_K = [8,8,8,8,8,8,7,1]`, classes 8..17 (240, 256, 304, 384, 480, 512,
  608, 768, 960, 1024 B) get K = 2. Parked blocks per thread are bounded by
  241,408 B (was 64,256 B; `DEPOT_BUDGET_BYTES` 64 KiB → 240 KiB, const-
  asserted). Pointer arrays per registry slot 8 KiB → 18 KiB (virtual;
  touched only when parked). No algorithm change: the same park/reload
  paths, now reachable for these classes.

Probe numbers (ИЗМЕРЕНО before this file; WSL, `b4_pmu_probe`, sefer arm,
3 repeats, base `g1-k1`@`c0c4eacf`, K-sweep on branch `g1-k5-sweep`):
mixed 256/1024 cycles K1 = 1 / K = 1 0.863× / K = 2 0.724× / K = 4 0.739× /
K = 8 (all classes ≤ 1216 B) 0.744×; instructions 0.879× (K = 1), 0.758×
(K ≥ 2). Callgrind mixed 256/1024 49 528 816 → 37 611 992 Ir; bulk16k,
mixed 16/64, bulk64, pair16 unchanged (±0.001 %). Instructions are not time;
the verdict is the time judge.

Protocol: J1′ v2 ablation (`scripts/b4_ablate_run.mjs`, 20 processes × 2000
blocks, all 11 kernels), effect = variant median − null median
(`scripts/b4_ablate_vs_null.mjs`), 95 % CI by process bootstrap.

Thresholds (same as B1, B2, K1, with the win kernel set for this change):

1. Win: churn256 OR churn1024 effect CI upper < 0.
2. No harm: pair16, pair64, larson effect ≤ +1 % and CI upper ≤ +1.5 %.
3. No harm, other kernels: not (CI lower > 0 AND effect > +2 %).

GO = 1 ∧ 2 ∧ 3. Path activation: the judge's churn kernels share one slot
ring, so a churn256/churn1024 portion frees blocks of the other classes and
reallocates its own; with K5 those classes now park whole magazines in the
depot (callgrind: `flush_class_internal` + `try_drain_freelist_batch` self Ir
on mixed 256/1024 drop to near zero, `depot_park` / `depot_reload` appear).
Memory: the parked-bytes bound is reported, not measured as RSS (same as the
B1 depot gate). Second measurement, against mimalloc: main judge J1′ v2 on
`g1-k5`.
