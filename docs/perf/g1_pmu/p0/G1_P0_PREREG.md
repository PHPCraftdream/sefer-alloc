# G1-P0 — pre-registration (fixed before any J1′ v2 run of this variant)

Ceiling probe, NOT shipped. Base: `g1-k5`@`2bc830cc` (main `37780bc7` + G1-K1
+ G1-K5). Measurement branch `g1-p0-meas` (harness from `g1-k5-meas`, which
cherry-picked `51eda7b1`). Decision source: `docs/perf/G1_IDEAS.md` §8 on
`g1-pmu` (consultation C4 with @oh and the rush reviewer).

Variant (patch in `docs/perf/b4_ablate/patches/`):

- `p0_depot64_k1024` — class 3 (64 B) parks up to 1024 full magazines in a
  separate depot array (`BIG_K`), so a bulk16k cohort (16 384 blocks) never
  flushes to the segment free list and never drains back; every other class
  keeps its K5 depot. `PerClass::parked` widens to u16 (offset 2),
  `DEPOT_CAP` to u16. Parked blocks per thread up to 1,286,912 B. Patch
  sha256 `93f4bf8b19599fb25623fda1d4123df74a3ae339a756c79b5ed4a7119783ebca`
  (`git diff -- src` of the `g1-p0` tree).

Probe numbers (ИЗМЕРЕНО before this file; WSL, `b4_pmu_probe`, sefer arm,
7 interleaved repeats, judge alignment flags; base `g1-k5-sweep`@`760cb17e`):
bulk16k cycles 0.635×, instructions 0.627×, ns/op 0.652×; bulk64 cycles
1.008×; mixed 16/64 1.032×, mixed 256/1024 1.017× (instructions 1.002×);
control pair16 cycles 1.175× at instructions 1.000× — layout noise of the
probe between two builds. Callgrind bulk16k whole program 85 698 245 →
55 387 491 Ir; `flush_class_internal` and `try_drain_freelist_batch` leave
the top functions (activation oracle).

Protocol: J1′ v2 ablation (`scripts/b4_ablate_run.mjs`, 20 processes × 2000
blocks, all 11 kernels), effect = variant median − null median
(`scripts/b4_ablate_vs_null.mjs`), 95 % CI by process bootstrap; then the
main judge against mimalloc on the `g1-p0` tree.

Decision rule (ceiling, not a ship gate):

1. Lever confirmed: bulk16k effect ≤ −20 % and CI upper < 0 → build a
   shippable form of "transfer = 0" (C4 variant (d), or a budgeted depot by
   the owner's RSS decision), judged again as a perf(runtime) change.
2. Lever rejected: bulk16k effect > −10 % → the magazine transfer does not
   set bulk16k time; close the transfer branch.
3. Between: report, decide with @oh and the rush reviewer.

Controls (noise floor): pair16, pair64 (depot untouched), bulk64, churn64
(class-64 depot in its K5 range). Shipping needs a measured win against
mimalloc on the main judge; this probe only bounds it.
