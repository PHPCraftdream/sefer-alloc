# R6-T40 gate — OPT-05 batch issue route (NO-GO) and OPT-04 unzeroed SmallSidecar storage (NULL, kept)

Runtime change that ships: OPT-04 only — `EntryHandle::new` takes the
`SmallSidecar` storage from `System.alloc` instead of `System.alloc_zeroed`
(`src/registry/segment_route/directory.rs`); the in-place constructor already
initialises every atomic, so the zeroing pass was redundant. No speedup is
claimed: the measured delta is −1 Ir per process. OPT-05 (one borrowed route
context per free-list drain batch) was built and measured, is mixed, and is not
merged. Task source: `docs/agent_reviews_round6/SUMMARY.md` R6-T40;
`docs/perf/OPEN_ITEMS.md` item 88.

## Identity and method

- Runner: `scripts/r20_r6_ab_iai_run.mjs` (`r20-gates.yml`, PR #63 label
  `gate-run`), bench target `perf_gate_iai`, features
  `production bench-internals internals`; each arm built in its own worktree
  and target directory. rustc 1.99.1 (58cf8f9c3 2026-10-13),
  Linux 6.17.0-1022-azure (GitHub Actions).
- Arm A (both runs) = `ef694302dafb11303feb39952db4ba03fe9ee062` (`main`).
- Run 1, arm B = `26843755438e986c07bc3f5507afdb127f6b56d0` (OPT-04 + OPT-05).
- Run 2, arm B = `e966076b146be49f23f15aba520afe1404b865fe` (OPT-04 only;
  OPT-05 reverted). Both SHAs stay reachable in the PR #63 history.
- Entry layer: `perf_gate_iai` cells drive `HeapCore` through the production
  alloc/free chain that `SeferAlloc`'s `#[global_allocator]` uses; the
  edited code is the `AllocCore` free-list drain (OPT-05) and Small route
  registration (OPT-04) on that chain.
- Path activation: no per-cell drain-batch or route-registration counter is
  read by this gate. OPT-05 activation is shown by the run-1 deltas themselves
  (the `recycle_*` cells, which refill from freed chains, move by −9728 Ir;
  cells without a drain change by the run-2 −1 baseline only). OPT-04's
  behaviour (zero zeroed System bytes, full initialisation) is asserted by
  `tests/r6_t40_sidecar_system_alloc.rs` and
  `tests/r11_p3_small_sidecar_routes.rs`, not by this gate.
- Thresholds (max tolerated regression, %), set before run 1: 0.5 on
  `recycle_alloc_only_256x16b`, `recycle_alloc_free_256x16b`,
  `cold_alloc_only_256x16b`, `carve_batch_only_16b`, `small_churn_16b`,
  `dealloc_free_only_16b`, `alloc_magazine_hit_only_16b`; all other cells
  informational.

## Run 1 — OPT-04 + OPT-05: VERDICT FAIL (2 of 106 cells over threshold)

Per-cell CSV: `docs/perf/R6_T40_OPT04_OPT05_GATE_run1_summary.csv`; raw iai
output: `docs/perf/_raw_r6_t40_run1_iai_{A,B}.log`. Table generated from the
CSV by `node scripts/r6_gate_table.mjs` (asserts `B − A = delta`):

| cell | A Ir | B Ir | delta Ir | delta / A |
|---|---:|---:|---:|---:|
| `recycle_alloc_only_256x16b` | 193028 | 183300 | -9728 | -9728 / 193028 = -5.04 % |
| `recycle_alloc_free_256x16b` | 228032 | 218304 | -9728 | -9728 / 228032 = -4.27 % |
| `recycle_alloc_free_256x64b` | 229694 | 219966 | -9728 | -9728 / 229694 = -4.24 % |
| `cold_alloc_only_256x16b` | 102838 | 104645 | +1807 | +1807 / 102838 = 1.76 % |
| `dealloc_free_only_16b` | 65998 | 66449 | +451 | +451 / 65998 = 0.68 % |
| `alloc_magazine_hit_only_16b` | 53237 | 53349 | +112 | +112 / 53237 = 0.21 % |
| `small_churn_16b` | 53719 | 53831 | +112 | +112 / 53719 = 0.21 % |
| `dealloc_free_only_1088_16b_n17` | 300571 | 308254 | +7683 | +7683 / 300571 = 2.56 % |
| `growth16_n` | 19448013 | 19679436 | +231423 | +231423 / 19448013 = 1.19 % |

cells: 106; delta > 0: 47; delta < 0: 26; delta = 0: 33

Reading: OPT-05 saves 9728 Ir per 256-block recycle (9728 / 256 = 38 Ir per
drained block) where the refill drains long freed chains, but adds a near-fixed
~112 Ir per refill elsewhere (112, 451, 1807, 7683 are ≈ 1, 4, 16, 68 × 113),
including cold cells whose refills find an empty free list. The cause of the
fixed cost is not attributed by this gate (candidate: codegen of the drain's
caller, since the empty-list early return precedes the route lookup). Two gated
cells regress past 0.5 %, so OPT-05 is NO-GO in this form.

## Run 2 — OPT-04 only: VERDICT PASS (106 cells)

Per-cell CSV: `docs/perf/R6_T40_OPT04_OPT05_GATE_summary.csv`; raw iai
output: `docs/perf/_raw_r6_t40_run2_iai_{A,B}.log`. Same generator:

| cell | A Ir | B Ir | delta Ir | delta / A |
|---|---:|---:|---:|---:|
| `recycle_alloc_only_256x16b` | 193028 | 193027 | -1 | -1 / 193028 = -0.00 % |
| `recycle_alloc_free_256x16b` | 228032 | 228031 | -1 | -1 / 228032 = -0.00 % |
| `recycle_alloc_free_256x64b` | 229694 | 229693 | -1 | -1 / 229694 = -0.00 % |
| `cold_alloc_only_256x16b` | 102838 | 102837 | -1 | -1 / 102838 = -0.00 % |
| `dealloc_free_only_16b` | 65998 | 65997 | -1 | -1 / 65998 = -0.00 % |
| `alloc_magazine_hit_only_16b` | 53237 | 53236 | -1 | -1 / 53237 = -0.00 % |
| `small_churn_16b` | 53719 | 53718 | -1 | -1 / 53719 = -0.00 % |
| `dealloc_free_only_1088_16b_n17` | 300571 | 300570 | -1 | -1 / 300571 = -0.00 % |
| `growth16_n` | 19448013 | 19448012 | -1 | -1 / 19448013 = -0.00 % |

cells: 106; delta > 0: 0; delta < 0: 72; delta = 0: 34

OPT-04 is Ir-neutral (−1 Ir in 72 cells, 0 in 34): under this harness the
replaced calloc did not execute a measurable zeroing pass. It is kept as a
redundancy removal with no claimed speedup (NULL).

## Disposition

- OPT-04: ships (NULL, no speedup claimed).
- OPT-05: NO-GO, reverted in `e966076b`; its witness test
  `tests/r6_t40_batch_issue_route.rs` was removed with it. Item 88 keeps OPT-05
  open with the measured mechanism: a variant that removes the per-refill fixed
  cost while keeping the 38 Ir/block saving needs its own A/B.
- OPT-09/10/11: deferred, see `docs/agent_reviews_round6/R6_T40_U25_REPORT.md`.
