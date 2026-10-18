# B4-R3 — Batch issue-touch on refill (run-level leaf hoist)

## What was hoisted

Before this change, every block issued by the two whole-batch refill paths
(`try_drain_freelist_batch`, `try_carve_batch`) ran a full per-block issue
chain: `prepare_small_issue` + `IssueTransaction::commit_prepared` = 2×
(`SegmentHeader::segment_id_at` + `base_at` + route-root compare + slot load)
plus a sidecar prepared-check and the class store.

Now one run = one refill batch in one segment. The hoist, per layer:

- **Per run (fatal route guards, once):**
  `RouteSlots::prepare_small_issue_run` (`src/alloc_core/segment/segment_table/route_slots.rs:189`) —
  live-slot, root-match and Small-sidecar guards run once; builds a
  `RoutedIssueRun` (`route_slots.rs:297`) over the sidecar's class leaves.
  `SegmentTable::prepare_small_issue_run`
  (`src/alloc_core/segment/segment_table/segment_table_impl.rs:364`) wraps it
  into an `IssueRun` (`src/alloc_core/segment/segment_table/issue_transaction.rs:133`),
  which preserves today's unrouted tautology for route-less cores.
- **Per 256-granule leaf CHANGE (the only remaining fallible step):**
  `RoutedIssueRun::prepare_for` (`route_slots.rs:306`) is two shifts + compare
  for same-leaf blocks and calls `ClassLeaves::prepare_leaf`
  (`src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs:176`)
  only on a leaf change — same guard order and fault-injection hooks as the
  existing `prepare()`. The cursor
  (`LeafCursor`, `leaf_classes.rs:33`) caches the promoted leaf's backing
  pointer; `resolve_leaf` (`leaf_classes.rs:210`) re-resolves on commit-side
  mismatch.
- **Per block (release cost):** `RoutedIssueRun::commit`
  (`route_slots.rs:333`) → `ClassLeaves::issue_in` (`leaf_classes.rs:221`) =
  the leaf-index compare plus exactly one Release store of `class + 1`.
  Call sites: `try_drain_freelist_batch`
  (`src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:560`,
  `:579`, `:631`, borrow-end at `:648`) and `try_carve_batch`
  (`:895`, `:898`, `:993`).

`IssueTransaction`, the scalar paths (`carve_block`, `pop_free`,
`prepare_current_carve`) and the producer protocol are unchanged.
`IssueTransaction::commit_prepared` was removed (no remaining callers).

## Why it is safe

- **C6 OOM ordering unchanged.** The whole prepare loop still runs before any
  mutation (decommit-recommit, lazy-commit growth, bump, live count, page
  map, output). On a leaf-change failure the run returns `Err(())`/`0`/null
  exactly as the per-block loop did, with the same full rollback and retained
  earlier promotions.
- **Fatal guards once per run, same violations abort.** A Small issue on a
  non-Small route, an empty slot, or a root mismatch aborts in
  `prepare_small_issue_run`, mirroring today's per-block prepare fatals.
- **Per-leaf preparedness is a sound weakening.** A promoted 256-granule leaf
  stays promoted for every granule in it, and per-granule class stores
  overwrite; once the leaf backing exists, any granule in it can be written.
- **Skip-prepare trap.** Commit never blindly stores: on a cursor
  None-or-different-leaf it re-resolves via `resolve_leaf`, and a
  never-promoted leaf aborts (`route_slots: batch issue commit on unprepared
  leaf`) — a release-mode trap mirroring the scalar commit's
  abort-on-violation contract. Verified by the mutant counterfactual below.
- **Borrow exclusion.** The run borrows the table (`&'a SegmentTable` →
  `&'a RouteSlots` → the route's sidecar leaves), so the borrow checker
  excludes route removal and any `&mut self` table work for the run's
  lifetime; `try_drain_freelist_batch` ends the run before its
  `&mut self` `publish_empty`.
- **Producer protocol and memory orderings untouched.** The only
  producer-visible edges remain the leaf-pointer Release-publish in
  `promote` and the class-byte Release store; `issue_in` performs the same
  store as `issue()` through a cursor resolved with the same Acquire load.

## Expected iai effect (reasoning only — no benchmarks were run)

The removed per-block work (2× id/base/root/slot validation + sidecar
prepared check) should show up in:

- `cold_alloc_free_256x16b`: ~385 Ir/pair today; target ≤ 300 Ir/pair
  (per-block overhead drops to one compare + one Release store per block).
- `cold_alloc_free_256x{64,256,1024}b`: same per-block hoist on the carve
  refill path.
- `recycle_alloc_free_256x{16,64}b`: drain-freelist refill batches get the
  same per-block reduction.

The orchestrator will measure A/B on CI.

## Mutant counterfactual

`RoutedIssueRun::prepare_for` was temporarily mutated to `if true { return
true; }` immediately after the same-leaf check (never calling
`prepare_leaf`). Running
`cargo test --features "production internals bench-internals" --test
b4_r3_batch_issue_leaf_boundary` produced:

```
running 1 test
route_slots: batch issue commit on unprepared leaf
error: test failed, to rerun pass `--test b4_r3_batch_issue_leaf_boundary`

Caused by:
  process didn't exit successfully: ... (exit code: 0xc0000409, STATUS_STACK_BUFFER_OVERRUN)
```

i.e. the skip-prepare mutant aborts on the commit-side unprepared-leaf fatal
instead of silently storing. After reverting the mutant exactly, the same
command gives:

```
running 1 test
test batch_issue_leaf_boundary_spill_rolls_back_and_recovers ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Tests

- **New:** `tests/b4_r3_batch_issue_leaf_boundary.rs` —
  `batch_issue_leaf_boundary_spill_rolls_back_and_recovers`
  (features `alloc-global`, `internals`, `bench-internals`; compiles to empty
  without them). Verifies behaviorally, across a real refill crossing into a
  virgin 256-granule leaf: the injected spill failure makes the whole batch
  fail gracefully with full rollback (owner state + System totals), the
  retry performs exactly ONE 256-byte leaf promotion for the whole crossing
  batch, class codes are published in both the new and the old leaf, and a
  same-leaf allocation promotes nothing.
- **Kept / updated:** `tests/r11_p3_small_sidecar_preflight.rs` — the
  `batch_drain` fault-injection countdown was per-block (257) and assumed the
  old per-block prepare call count; updated to 2 (run-cursor init + the
  leaf-crossing block), which lands the refusal at the same leaf-crossing
  block. Everything else in the suite (rollback assertions, scalar paths,
  carve-crossing refusal) passes unchanged.

## Local verification (Windows, `CARGO_TARGET_DIR=.../cargo-target-B`)

| Command | Exit |
| --- | --- |
| `cargo check` (default) | 0 |
| `cargo check --features production` | 0 |
| `cargo check --features "production internals bench-internals alloc-stats"` | 0 |
| `cargo check --features "production hardened internals bench-internals"` | 0 |
| `cargo check --all-features` | 0 |
| `RUSTFLAGS="--cfg loom" cargo check --test loom_sidecar_bitmap_actual --features alloc-global,alloc-xthread --features tagged-index-stack/loom` (compile-only, NOT a model run) | 0 (needs the extra `tagged-index-stack/loom` feature locally; plain invocation fails in the dev-dep's `compile_error!` — CI loom jobs cover the canonical wiring) |
| `cargo clippy --lib -- -D warnings --features "production internals bench-internals alloc-stats"` | 0 |
| `cargo clippy --all-targets -- -D warnings` (same features) | fails only in the untouched `tests/con01_exit_stranded_lock.rs` (`dead_code` on a pre-existing unused enum variant) — pre-existing, unrelated to this change |
| targeted tests ×2 feature sets (`production internals bench-internals`, `production hardened internals bench-internals`): `r11_p3_small_sidecar_issue_oom`, `r11_p3_small_sidecar_zeroed_oom`, `r11_p3_small_sidecar_preflight`, `r11_p3_small_sidecar_routes`, `r6_class_issue`, `r6_route_directory`, `r6_route_directory_global`, `r6_route_directory_model`, `b4_r3_batch_issue_leaf_boundary` | 0 (all 9 binaries pass in both sets) |
| new test without fastbin (`alloc-global alloc-xthread alloc-decommit alloc-segment-directory primordial-lazy-commit internals bench-internals`) | 0 |
| `node scripts/verify-structure-limits.mjs` | 0 |
| `rustfmt --check` on all touched files (repo-wide `cargo fmt --check` fails locally with Windows "filename too long", os error 206 — a tooling limitation, not a formatting failure) | 0 |

## What must run on CI

- miri: `miri-alloc-core`, `miri-fastbin`, `miri-plain` (new unsafe in
  `issue_in`; the run deref argument must hold under Stacked Borrows).
- loom: `loom-alloc-global` (the run borrow/leaf publication under the loom
  model; `leaf_classes.rs` compiles under loom atomics, verified compile-only).
- `test-gated-bodies` / `test-feature-isolation` rows (unrouted branches,
  hardened bodies).
- iai A/B for the cells listed above.

## iai A/B on CI (verdict: GO)

Run 39143116994 (`r20-gates`, PR #75, `scripts/r20_r6_ab_iai_run.mjs`,
bench `perf_gate_iai`, features `production bench-internals internals`,
Linux, rustc 1.99.1). A = main `e07acc59`, B = `5c100ca3` (this branch with
the orchestrator's review fixes: `IssueRun` lifetime marker for builds without
`alloc-global`, the `base_at` guard restored once per run, `cfg` on
`IssueTransaction::prepared`). Full per-cell data:
`docs/perf/B4_R3_BATCH_ISSUE_summary.csv`; table generated by
`node scripts/r6_gate_table.mjs docs/perf/B4_R3_BATCH_ISSUE_summary.csv <cells>`.

| cell | A Ir | B Ir | delta Ir | delta / A |
|---|---:|---:|---:|---:|
| `cold_alloc_free_256x16b` | 136168 | 105960 | -30208 | -30208 / 136168 = -22.18 % |
| `cold_alloc_free_256x16b_4n` | 425386 | 304554 | -120832 | -120832 / 425386 = -28.41 % |
| `cold_alloc_only_256x16b` | 101546 | 71338 | -30208 | -30208 / 101546 = -29.75 % |
| `recycle_alloc_free_256x16b` | 226605 | 169919 | -56686 | -56686 / 226605 = -25.02 % |
| `growth16_n` | 19266762 | 15400138 | -3866624 | -3866624 / 19266762 = -20.07 % |
| `dealloc_free_only_16b` | 65750 | 58198 | -7552 | -7552 / 65750 = -11.49 % |
| `dealloc_free_only_1088_16b_n17` | 294655 | 166271 | -128384 | -128384 / 294655 = -43.57 % |
| `small_churn_16b` | 53732 | 51844 | -1888 | -1888 / 53732 = -3.51 % |
| `churn_256b` | 53666 | 51778 | -1888 | -1888 / 53666 = -3.52 % |
| `mimalloc_cold_alloc_free_256x16b` | 32325 | 32325 | 0 | 0 / 32325 = 0.00 % |

cells: 106; delta > 0: 0; delta < 0: 55; delta = 0: 51

No cell regresses. `mimalloc_*` cells are the unchanged competitor control.
The cold 256×16B pair still costs 105960 Ir against mimalloc's 32325, so R3
narrows the cold-path gap but does not close it (see
`docs/perf/B1_MT_REGRESSION_ANALYSIS.md` for the remaining levers).

Known weakness: the leaf-boundary counterfactual mutant disables every leaf
prepare from the start of the test, so it does not prove that the refusal
happens exactly at the leaf crossing; the rollback and recovery assertions
themselves are unaffected.
