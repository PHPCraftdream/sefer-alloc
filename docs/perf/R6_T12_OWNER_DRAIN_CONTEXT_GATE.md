# R6-T12 gate — owner-drain segment context once per scan (iai A/B): NO-GO

`bench` result, no runtime change ships. R6-T12 (`docs/agent_reviews_round6/SUMMARY.md`;
review R6-PF-5 + R17-OPT-08, `docs/perf/OPEN_ITEMS.md` item 93) proposed hoisting
the segment invariants of `reclaim_sidecar_record` (`kind_at`, payload start,
`bump_of`, the metadata views) to one computation per owner-drain scan,
retiring accepted live credits with one `sub_live(k)` per scan instead of one
`dec_live` per record, and replacing the per-record `off % block_size` with a
compile-time reciprocal. The review estimated ~10–20 Ir saved per reclaimed
record. The candidate was built and measured; it did not save Ir, so it is not
merged.

## Identity and method

- Runner: `scripts/r20_r6_ab_iai_run.mjs` (`r20-gates.yml`, PR #58 label
  `gate-run`), bench target `perf_gate_iai`, features
  `production bench-internals internals`.
- Arm A = `d39417238848098eac17713e5ef7cb5a908db074` (`main`), arm B =
  `e88c3bce3a52713571e93979450beb317cbcb8fa` (PR #58 head at measurement; the
  commit stays reachable in the PR #58 history). Each arm built in its own
  worktree and target directory. rustc 1.99.1 (58cf8f9c3 2026-10-13),
  Linux 6.17.0-1022-azure (GitHub Actions).
- Entry layer: `perf_gate_iai` cells drive `HeapCore` through the production
  alloc/free paths (`SeferAlloc`'s `#[global_allocator]` chain); the edited
  code is the owner-side sidecar drain those paths call.
- Path activation: no per-cell record counter is read by this gate. The drain
  cells below are the ones whose `Ir` moved by 5–16 % when the same drain loop
  was restructured in R6-T18 (`docs/perf/R6_T18_SIDECAR_DRAIN_UNIFY_GATE.md`),
  which shows they execute it; the record count per cell is not measured.
- Thresholds (max tolerated regression, %), set before the run: 1.0 on every
  row of the table below except `growth16_2n` / `growth16_4n`
  (informational).

## Result

Script-generated table: `docs/perf/_raw_r6_t12_verdict.log` (106 cells,
`VERDICT: PASS` — no cell over its threshold). Per-cell CSV:
`docs/perf/R6_T12_OWNER_DRAIN_CONTEXT_GATE_summary.csv`. Raw iai output:
`docs/perf/_raw_r6_t12_iai_{A,B}.log`. The table below is generated from the
CSV by `scripts/r6_t12_gate_table.mjs` (which asserts `B − A = delta` per row):

| cell | A Ir | B Ir | delta Ir | delta / A |
|---|---:|---:|---:|---:|
| `r19_prf01_publication` | 3621539 | 3622964 | +1425 | +1425 / 3621539 = 0.04 % |
| `seg_cycle_decommit_256k` | 10359137 | 10362696 | +3559 | +3559 / 10359137 = 0.03 % |
| `multiseg_cold_256k` | 3389835 | 3391106 | +1271 | +1271 / 3389835 = 0.04 % |
| `growth16_n` | 19241175 | 19267783 | +26608 | +26608 / 19241175 = 0.14 % |
| `growth16_2n` | 48924121 | 48977353 | +53232 | +53232 / 48924121 = 0.11 % |
| `growth16_4n` | 139747430 | 139853910 | +106480 | +106480 / 139747430 = 0.08 % |
| `cold_alloc_only_256x16b` | 102822 | 103014 | +192 | +192 / 102822 = 0.19 % |
| `dealloc_free_only_16b` | 65922 | 65958 | +36 | +36 / 65922 = 0.05 % |
| `small_churn_16b` | 53590 | 53587 | -3 | -3 / 53590 = -0.01 % |

cells: 106; delta > 0: 37; delta < 0: 16; delta = 0: 53

## Verdict

**NO-GO.** The thresholds pass, but the change was proposed as an `Ir`
reduction and every drain cell moved up, not down: `r19_prf01_publication`
+1425, `seg_cycle_decommit_256k` +3559, `multiseg_cold_256k` +1271. The
`growth16` series grows with the workload (+26608 / +53232 / +106480 Ir at
n / 2n / 4n), so the extra cost scales with the drain work rather than being a
fixed setup cost. The review's per-record saving does not show up in any
measured cell. The likely reason, not verified against the assembly: the
per-record values the context hoists were already cheap loads from the same
cache lines, and carrying the context through the shared loop costs more than
it saves.

The code is reverted in the same PR; only this report and its artifacts land.
Item 93 is closed as NO-GO. A division-free class divisibility helper arrives separately with R6-T06
(PR #57); a future attempt can reuse it without the per-scan context. No wall-clock measurement
was made. Not a production feature-composition change.
