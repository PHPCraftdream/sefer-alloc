# G1 decision criterion — written before the numbers are read

Plan: `docs/plans/2026-11-15-workspace-restructure-plan.md`, §"G1 — decision on P2"
(the criterion is written into the G1 report before the numbers are read) and §5 P2.
Status of this file: criterion + input inventory only. **It records no decision and
reads no verdict into the numbers.** Per the plan: "Default when the numbers do not
show a clear benefit: stop."

Every input below names its computing command/script and its source file. Where an
input does not exist yet, it is marked **WILL BE MEASURED** with the exact command;
no number in this document is invented.

## Disclosure

Honest disclosure, per the plan requirement that this criterion be written
before the numbers are read: some first-parking-run data was seen by the parent
before these thresholds were written down. All numbers below come from the
closed integration PR #249 (P1.2+P1.3+P1.4) and its CI runs of 2026-11-15 —
this is NOT a committed `docs/perf` report; no committed report exists.

- Status: integration PR #249 was closed on 2026-11-15 based on its own CI;
  P1.3 (sefer-policy), P1.4 (sefer-platform) and the P1.6 `#[inline]`-audit step
  are parked on branch `p14-sefer-platform-20261115` pending the G1 decision;
  P1.1 (sefer-ghost, already in main via PR #247) precedes it; P1.2 (sefer-concurrent)
  lands separately as PR #250.
- The no-LTO IAI run of the P1.2+P1.3+P1.4 stack against the P1.0 baseline
  (`docs/perf/IAI_NOLTO_BASELINE_summary.csv`) covered 106 benches: 30 unchanged,
  76 with Ir growth from +0.003% to +0.5%, and one bench,
  `decomp_os_roundtrip_8x`, 1740→1996 Ir (+14.7%; the +256 Ir ≈ 8 iterations ×
  ≈32 Ir — four removed call frames across the crate boundary).
- `ghost-zero-cost` is red for P1.3/P1.4 by construction: the production `.text`
  of the Large path differs (functions `alloc_large`, `reclaim_large_segment`,
  `maybe_decay_large_cache`, `evict_one_oldest`); the probe Ir is −2231 out of
  ≈10.2 million.
- Guard against fitting the criterion to these observations: the thresholds
  below are written as DIRECTIONS and CONDITIONS (e.g. "delta 0 after P1.6",
  "strictly fewer", "stop by default"), not as numbers chosen to match these
  observations; the "Ir after P1.6 #[inline]" input remains unmeasured — it is
  exactly the input that will show whether the inline audit removes the +256 Ir
  class.

## Default rule

If the measured inputs do not show a clear benefit of the P2 core split over
lint-enforced layering (A0/A1 ratchet, `scripts/module-graph.mjs --check-backedges`),
the default is STOP: P2 is not done, layering stays enforced by the A1 lint. The
burden of evidence is on GO, not on NO-GO.

The cost side of P2 includes the ghost-zero-cost policy: any P2 step that changes
production `.text` must pass CI job `ghost-zero-cost` (normalized `.text` diff +
callgrind Ir of `examples/ghost_zero_cost_probe.rs` against the base). The only
alternative is the prefix skip in `scripts/ghost_diff.mjs` `zeroCostSkip`
(fail-closed: any doubt keeps the proof required) — and, read from the code, it
requires ALL of the following to hold:

- some commit subject declares a runtime change
  (`scripts/ghost_diff.mjs:27` `const RUNTIME = /^(?:perf\((?:runtime|opt-in)\)|fix\(perf\))!?:/;`);
- every subject has a conventional prefix and none is verification/ghost/loom/miri/kani-scoped;
- NO changed path is a build-graph input
  (`scripts/ghost_diff.mjs:30` `const BUILD_GRAPH = /^(?:(?:[^/]+\/)*Cargo\.(?:toml|lock)|(?:[^/]+\/)*build\.rs|\.cargo\/.*|rust-toolchain(?:\.toml)?)$/;`)
  — with a single exception: the ROOT `Cargo.toml`, and only when every changed
  line is the `production = [...]` composition line or a comment
  (`scripts/ghost_diff.mjs:49` `if (path === 'Cargo.toml' && productionCompositionOnly(lines)) continue;`,
  where `productionCompositionOnly` is `scripts/ghost_diff.mjs:36`);
- for every other manifest — including any `crates/**/Cargo.toml`, which ANY
  extraction into a new crate adds — the skip does NOT apply:
  `scripts/ghost_diff.mjs:50` `if (BUILD_GRAPH.test(path) || path.startsWith('src/ghost/') || path.startsWith('crates/primitives/sefer-ghost/src/')) return null;`
  returns `null` (proof required) unconditionally for such paths.

Consequently, for the P2 steps the `ghost-zero-cost` gate can be green only if
the production code (`.text`) is byte-for-byte identical, or if the owner changes
the gate policy itself (the skip rule in the script). That is the price of P2;
whether the owner accepts it is a SEPARATE owner decision and is not pre-decided
here.

## Input 1 — CI time and selected-test counts, before/after P0 and P1

| Quantity | Unit | Threshold (proposed, to be confirmed at G1) | Source / command |
| --- | --- | --- | --- |
| Full-run CI wall time per side (mean of ≥3 ordinary CI runs per side) | minutes | must drop measurably for peripheral/policy changes after P0+P1 (exact cutoff set once both sides are measured; direction already fixed: down) | `gh run view <run-id> --json jobs --job <job-id>` over `ci.yml` runs on main; several runs per side, no dedicated wall-clock campaign (plan §3) |
| Selected-test count for a representative peripheral change | tests | after P1.5 strictly fewer than the current all-target selection | current data point: a segment-directory scan change selects 369 of 484 tests (`docs/CI_TIERS.md`, "Test selection" section; first map run: 374 of 484 mapped under `--all-features`) |
| Test binaries in rows that build all tests | binaries | −34 after P0.2 pilot merges (35 of 78 `regression_*` targets merged; 20 + 23 excluded) — source: plan revision 3 fact 5 | P0.2 pilot PR; verdict file `docs/perf/P0_2_TEST_MERGE_PILOT.md` arrives with the pilot PR (NOT present in this tree as of 2026-11-15 — verified by `ls docs/perf/P0_2_TEST_MERGE_PILOT.md` → absent). Verdict GO/NO-GO is read from that file, not assumed |

**WILL BE MEASURED (after P1.5):**
- `scripts/ci-job-durations.mjs` DOES NOT EXIST (verified: `grep -r "ci-job-durations" scripts docs` → no hits on 2026-11-15). Until it exists, durations are read directly:
  `gh run view --json jobs --job <id>` per job, several runs per side (before/after P1.x merge), averaged per job; no wall-clock campaign.
- Longest jobs today, for scale only (plan §1, measured on the analysis tree):
  miri 33 + 33 + 24 + 22 min, `check-matrix` 30 min, aarch64 23 min, windows 19 min, kani 18 min.
- After P1.5 lands: same reads against a P1.5-tree run; the delta per job is the G1 input.

## Input 2 — no-LTO Ir delta of P1 (cross-crate inlining cost)

| Quantity | Unit | Threshold (proposed) | Source / command |
| --- | --- | --- | --- |
| Ir delta of each P1 extraction vs the P1.0 no-LTO baseline, per bench function | instructions (callgrind Ir, deterministic) | delta 0 per bench row after the P1.6 inline audit (audit expectation read from recorded numbers, not a CI threshold; CI threshold is `Ir=10`, `IAI_CALLGRIND_REGRESSION` in `perf-gate.yml`) | baseline: `docs/perf/IAI_NOLTO_BASELINE.md` + `docs/perf/IAI_NOLTO_BASELINE_summary.csv` (P1.0, commit `90d8793a…`, run 43712417106; metrics per bench: Ir, L1/L2/RAM hits, estimated cycles; generated by `scripts/iai-nolto-summary.mjs`); later runs: job `iai-callgrind-nolto` (perf-gate workflow_dispatch), `CARGO_PROFILE_BENCH_LTO=false` + `CARGO_PROFILE_BENCH_CODEGEN_UNITS=16` |

**Measured BEFORE this criterion was written** (first parking run, PR #249 CI,
uncommitted): see the Disclosure section above — the numbers are not duplicated
here.

**Still to be measured:**
- Ir after the P1.6 `#[inline]` audit on the parked branch — a separate run:
  dispatch `perf-gate` (`iai-callgrind-nolto`), then
  `node scripts/iai-nolto-summary.mjs`, and diff bench-by-bench against
  `docs/perf/IAI_NOLTO_BASELINE_summary.csv` and against the PR #249 numbers
  (same bench-name set enforced by the script; same-row comparison only, never
  against the thin-LTO row).
- P1.2 alone (PR #250) — moves the `experimental` tier (the `epoch`, `lock_free`,
  `sharded`, `pinning` modules) into the `sefer-concurrent` crate; the production
  build (`--features production`) does not contain that code at all, so the
  production `.text` and Ir paths are unchanged by construction; the expected
  Ir delta of 0 is confirmed by the same run (dispatch `perf-gate`
  `iai-callgrind-nolto`).

The threshold stays: **delta 0 per bench row after the P1.6 inline audit.**

## Input 3 — back-edge list of A0/A1 (what P2.0 must remove)

| Quantity | Unit | Threshold (proposed) | Source / command |
| --- | --- | --- | --- |
| Baselined upward module edges (the ratchet) and their reference counts | edges / refs | P2.0a–d must drive the ratchet to zero edge-by-edge (plan §5 P2.0); a count that cannot shrink below its declared sites is a cost datapoint against GO | `docs/layering_baseline.csv` (rows are derived: `node scripts/module-graph.mjs --emit-baseline`); check: `node scripts/module-graph.mjs --check-backedges` — green (exit 0) on this tree, 2026-11-15; A0 report: `docs/perf/MODULE_GRAPH_20261115.md` |

Known back-edge classes feeding P2.0 (plan §1, §5 P2.0): `alloc_core -> registry::segment_route`
(RouteScan/RouteDirectory/RouteRegistration/HeapIngress, ≥10 sites),
`segment_route -> global` exit state (≥8 sites in `route_directory.rs`),
`heap_core_xthread/routing.rs:92 -> global::fallback::try_with_heap`,
`segment_route -> heap_core_xthread::route_pin_cache` (≥5 sites) — all encoded as
claims in `scripts/module-graph/layers.json` §`expected_back_edges`, statuses checked
by the command above.

## Input 4 — count of items P2 would widen to `pub`

| Quantity | Unit | Threshold (proposed) | Source / command |
| --- | --- | --- | --- |
| Items whose visibility widens to `pub` at the P2 crate boundaries; of them, raw-pointer/`unsafe`-signalled items | items | no fixed numeric cutoff: the count prices the soundness-surface cost (CLAUDE.md boundary rule: `pub unsafe fn` + `# Safety`), and every signalled item is an additional owner-facing cost | `node scripts/g1-widening-count.mjs --report` → `docs/perf/G1_P2_WIDENING_COUNT.md` (derived by the script; `--check` pins it). Canonical numbers live in that file, not here |

## Inputs that do not exist yet (summary)

1. After-P1.5 CI durations and selection counts — WILL BE MEASURED (Input 1 command).
2. P0.2 GO/NO-GO verdict — arrives in `docs/perf/P0_2_TEST_MERGE_PILOT.md` with the
   pilot PR; P0.3/P0.4 run only on GO (plan rev.3 fact 5).
3. P1.3/P1.4 no-LTO Ir deltas — MEASURED in the CI of the closed PR #249
   (uncommitted; see Disclosure). NOT yet measured: Ir after P1.6 on the parked
   branch, and the P1.2-alone run (PR #250); same commands as Input 2 (dispatch
   `perf-gate` `iai-callgrind-nolto`).
4. `scripts/ci-job-durations.mjs` — does not exist; until it exists, durations come
   from `gh run view --json jobs --job <id>` (several runs per side).
