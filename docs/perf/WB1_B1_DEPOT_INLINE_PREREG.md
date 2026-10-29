# WB-1 / B-1a: inline the depot overflow chain into its two cold roots — pre-registration

Written and committed before any measurement of this change.

## Change

Attributes only, no statement changes (`src/registry/heap_core/state/depot_paths.rs`,
`tcache_flush.rs`): `depot_park`, `depot_reload_tick`, `park_overflow`,
`reload_overflow_tick` and `depot_tick` become `#[inline(always)]`,
`reload_overflow` becomes `#[inline]` (all keep `#[cold]` where they had
it; `depot_decay` stays out of line). Every caller is already a cold,
out-of-line root (`free_magazine_full`, `refill_magazine_slow`,
`refill_magazine_slow_virgin`, the teardown `depot_reload`), so the hot
alloc/free paths do not change; the call graph in the source is unchanged,
so the P12 ghost rules hold as written.

Motivation: WB-1 callgrind (warm bulk 1024 × 16 B) puts one park at ≈ 154 Ir
and one reload at ≈ 163 Ir, 63 of each per pass, ≈ 19 Ir per pair; the
frames and re-checks of the nested out-of-line levels are part of that
(@ox estimate −3…−6 Ir per pair for B-1 as a whole).

## Arms

- `base`: `40da85c3` (`main`, includes the BT-1 harness).
- `b1`: this branch's commit carrying the change and this file.

## Measurements and acceptance (all must hold)

1. **Ir (callgrind, WSL)** — `examples/b4_pmu_probe.rs`, marginal 50 → 100
   passes, divided by 56 320 pairs: `bulk sefer 1024 16` and
   `bulk sefer 1024 64` lower in `b1` by at least 2 Ir per pair; pair16 /
   churn16 / churn64 (marginal 200k → 400k ops) not higher by more than
   0.5 Ir per op.
2. **Wall clock (paired, Windows host)** — `npm run bench:table:paired`
   (3 processes × 200 rounds, `steady`), arms run alternately
   `base, b1, base, b1`, both built with `RUSTFLAGS="-C
   llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"`;
   statistic the SeferAlloc/mimalloc ratio per cell (as in WB-1 A-1 check
   3b, derived by `scripts/wb1_a1_verdict.mjs`-style table):
   - warm bulk 16 B: the ratio of `b1` lower than `base` in both run pairs;
   - no cell's ratio higher in `b1` than in `base` by more than 3 % in both
     run pairs.
3. **iai (CI)** — the perf-gate iai set, `b1` against `base`: no bench more
   expensive by more than 10 Ir in total.

If any check fails the change is rejected and reverted; the result is
recorded either way.
