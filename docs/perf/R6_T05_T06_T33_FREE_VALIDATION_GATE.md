# R6-T05 / R6-T06 / R6-T33 gate — own-thread free and free-list link validation (iai A/B)

`fix(perf)` change, no speedup claimed. PR #57 adds validation on two hot
paths of the default build and this gate measures its price:

- own-thread Small free (`dealloc_own_base.rs`): a `BlockKind::Unknown`
  segment is a no-op (R6-T33); the offset must be a multiple of the supplied
  class stride (`SizeClasses::divides`, R6-T05); under `hardened` the owner
  class map must also name the same class;
- free-list pops (`alloc_core_small_impl.rs`): every link is checked once
  for range `[meta end, bump)` and `MIN_BLOCK` alignment, a bad link is
  `fatal` (R6-T06); class-stride divisibility of a link stays in hardened's
  `valid_free_node`.

## Identity and method

- Runner: `scripts/r20_r6_ab_iai_run.mjs` (`r20-gates.yml`, run 38871705522,
  PR #57 label `gate-run`), bench target `perf_gate_iai`, features
  `production bench-internals internals`.
- Arm A = `d39417238848098eac17713e5ef7cb5a908db074` (`main`), arm B =
  `49d3dc9ccfd3026c268880b1c146e26aea54a172` (PR #57 head at measurement).
  Each arm built in its own worktree and target directory. rustc 1.99.1
  (58cf8f9c3 2026-10-13), Linux 6.17.0-1022-azure (GitHub Actions).
- Entry layer: `perf_gate_iai` cells drive `HeapCore` through the production
  alloc/free chain (`SeferAlloc`'s `#[global_allocator]`), which is where the
  checks live.
- Path activation: `dealloc_free_only_*` / `small_churn_*` run own-thread
  frees through the guarded branch; `recycle_alloc_only_256x16b` refills the
  magazine from the free list (R24-5 split), which runs the link check.
- Thresholds (max tolerated regression, %), set before the first run: 0.5 on
  `alloc_magazine_hit_only_16b` and `large_alloc_free_cycle`; 1.0 on the
  other eight rows below.

## Result

Script-generated table: `docs/perf/_raw_r6_t05_t06_verdict.log` (106 cells,
`VERDICT: PASS`). Per-cell CSV:
`docs/perf/R6_T05_T06_T33_FREE_VALIDATION_GATE_summary.csv`. Raw iai output:
`docs/perf/_raw_r6_t05_t06_iai_{A,B}.log`. Table generated from the CSV by
`scripts/r6_gate_table.mjs` (asserts `B − A = delta` per row):

| cell | A Ir | B Ir | delta Ir | delta / A |
|---|---:|---:|---:|---:|
| `dealloc_free_only_16b` | 65922 | 66050 | +128 | +128 / 65922 = 0.19 % |
| `dealloc_free_only_16b_n8` | 59018 | 59034 | +16 | +16 / 59018 = 0.03 % |
| `dealloc_free_only_16b_n16` | 59498 | 59530 | +32 | +32 / 59498 = 0.05 % |
| `small_churn_16b` | 53590 | 53718 | +128 | +128 / 53590 = 0.24 % |
| `small_churn_16b_2n` | 59094 | 59350 | +256 | +256 / 59094 = 0.43 % |
| `cold_alloc_only_256x16b` | 102822 | 102822 | 0 | 0 / 102822 = 0.00 % |
| `recycle_alloc_only_256x16b` | 192813 | 193403 | +590 | +590 / 192813 = 0.31 % |
| `alloc_magazine_hit_only_16b` | 53169 | 53297 | +128 | +128 / 53169 = 0.24 % |
| `cold_alloc_free_256x16b` | 137156 | 137668 | +512 | +512 / 137156 = 0.37 % |
| `large_alloc_free_cycle` | 46726 | 46726 | 0 | 0 / 46726 = 0.00 % |

cells: 106; delta > 0: 45; delta < 0: 4; delta = 0: 57

## How the default-build checks were chosen

Earlier PR #57 heads failed this gate; those runs are cited for the decision
only, their logs are not committed:

- exact class-map check on every own-thread free, per-link `kind_at` /
  `bump` / division (run 38857577637): `recycle_alloc_only_256x16b` +11.12 %,
  `dealloc_free_only_16b` +3.30 %, `alloc_magazine_hit_only_16b` +4.09 %;
- per-call link bounds with a reciprocal divisibility test, class-map check
  still on (run 38860816130): `recycle_alloc_only_256x16b` +7.43 %, free
  cells unchanged (~33 Ir per free from the class-map lookup chain);
- class-map check moved to `hardened` by the owner's decision, stride
  divisibility on free (run 38864912199): free cells under 0.5 %,
  `recycle_alloc_only_256x16b` +3.29 % (links validated in both batch passes);
- one validation per link (runs 38867379112, 38869064135):
  `recycle_alloc_only_256x16b` +1.58 % / +1.56 % (~10 Ir per link from the
  divisibility test).

The last step moved link divisibility to `hardened` as well, giving the
table above.

## Verdict

PASS on the pre-set thresholds. The default build pays +128 Ir on
`dealloc_free_only_16b` (0.19 %) and +590 Ir on `recycle_alloc_only_256x16b`
(0.31 %). What the default build still accepts (an aligned wrong-class free;
an in-segment, aligned forged link) is item 247 in
`docs/correctness-open-items/TRACKED_correctness_residuals.md` and the README
"Honest limitations". No wall-clock measurement was made. Not a production
feature-composition change.
