# sefer-alloc — workspace restructure plan (tests, thematic crate folders, core split)

Date: 2026-11-15. Status (2026-11-21, through `origin/main` `bf636478`): P2.0–P2.4 are merged (PRs #259, #274, #279, #280, #283; see section 5). PR #284 adds adjacent source-scanner coverage for `sefer-segment`. P2.5 (#187) ships in a PR carrying `CI: full`; see section 5 for the two acceptance requirements, neither claimed here. P1.5a/b landed and P1.5c landed the enforce criterion, not enforce enablement. P0.2 is NO-GO; P0.3/P0.4 remain closed. The original source measurement was at `main` `d093edd7`.
Revision 2 (2026-11-15, self-review): added the visibility-widening cost, the test-inventory
gate, the no-LTO baseline before P1, the explicit P2 decision gate G1, a narrower `CI: full`
scope and a corrected order (G0 does not gate A0/A1/P0).
Revision 3 (2026-11-15, after writing P1.1–P1.4 and the P0.2 pilot): leaf extraction alone does not make
CI cheaper and each extraction adds CI work, so P1.5 comes right after the P1.1–P1.4 merge; P1.3
and P1.4 landed narrower or wider than planned (facts below); the P0.2 pilot found a new
exclusion class, and P0.3/P0.4 run only on GO; steps that edit the same derived files merge as a
stack with one recompute pass and a fixed list of local cargo-free checks. Open items found in
revision 3 are recorded in the open-items indexes (`docs/CORRECTNESS_OPEN_ITEMS.md`); list:
sentinel floor, test-inventory `--ref`, process-global classifier, stale doc paths
`crates/<name>`, `NonNull` warning in hardened (no numbers here).

Revision 4 (2026-11-17, after the P1 landing and the measurement PRs): P1.3/P1.4/P1.6 are merged (PR #257, `main` `a966aa8f`); the no-LTO IAI row against the base series grew on 7 of 106 benches — maximum +0.46 % (`decomp_os_roundtrip_8x`) — and shrank on 69 (perf-gate run 43933405473). The owner-goal metric is stated honestly in section 7: 0 of 28 window runs would have been subsets even with selection enabled, and `TEST_SELECTION` is not set. The order of section 6 is replaced: process hygiene first, then P2.1 in parallel with P1.5a/b; P1.5 no longer waits for P2.2, and enforce enablement (P1.5c) comes only after P2.4. New process rules and the `TEST_SELECTION=enforce` enablement criterion are the revision-4 subsection of section 3. After the Miri sharding (PR #261, rebalanced by PR #262) the full-run wall-clock did not shrink — the critical path moved (final-run job times of PR #262's CI: test (aarch64) 1991 s, miri (core, TreeBorrows pin cache) 1888 s, check-matrix 1848 s, miri-plain (heavy) 1454 s, kani 1335 s); the next lever is check-matrix by rows and aarch64. Open items are filed in the indexes: CI critical path and Miri shard rebalance as items 155/156 of `docs/perf/OPEN_ITEMS.md`, enforce enablement as item 281 of `docs/CORRECTNESS_OPEN_ITEMS.md` (`docs/correctness-open-items/TRACKED_ci_gate_coverage.md`).

Goal: make a change pay only for the tests it can affect, and make the layering of the
allocator explicit, without changing behavior or performance.

## 1. Review of the first analysis

The first analysis (chat, 2026-11-15) is corrected here. Counts below are from the tree;
"hypothesis" marks what is not yet measured.

| Earlier claim | Verdict | Evidence |
|---|---|---|
| "562 test binaries, merge them, order-of-magnitude fewer links" | Overstated | 562 files in `tests/*.rs`, but 514 carry a file-level `#![cfg(...)]`, 79 install `#[global_allocator]`, 111 use first-config-wins APIs (`with_config`, THP tuple publication, `config_conflicts`), 33 spawn processes, 21 are loom-only, 52 targets are `harness = false`, 29 are declared `[[test]]`. These sets overlap; the mergeable pool is unknown until classified (step P0.1). |
| "merging tests is risk-free" | Wrong | CI steps grep exact test names and counts (`ci-j13b-judge-preparation`), `scripts/check-matrix.mjs` rows name targets, J13 pins hash files, README/ARCHITECTURE publish the test-file count, and the test-impact map is keyed by test target — merging makes selection coarser unless selection moves to module filters. |
| "`registry <-> global` and `alloc_core -> registry` are cycles needing trait inversion" | Mostly wrong, cheaper | Every `alloc_core -> registry` reference is `registry::segment_route` (`RouteScan`, `RouteDirectory`, `RouteRegistration`, `HeapIngress`): a lower layer sitting in the wrong module. `registry -> global` is `process_exit_in_progress` and `fallback::record_exit_lock_giveup` (8 uses in `route_directory.rs`) plus one `fallback::try_with_heap` (`heap_core_xthread/routing.rs:92`). `segment_route -> heap_core_xthread::route_pin_cache` (5) is one more back-edge. `alloc_core -> global` is a doc link only. Most of this is relocation, one call needs a hook. |
| "`concurrent` is a clean leaf" | Confirmed | Gated by `experimental` in `src/lib.rs`; no code outside `src/concurrent` uses it (one mention in `platform/node.rs` to verify as a comment). |
| "all `unsafe` can sit in one crate" (carried over from win-nice) | Does not apply | 23 seam modules (`#![allow(unsafe_code)]`) of 223 files, in `platform`, `segment`, `large`, `registry`, `global`, `concurrent`. The inventory command greps `src/ crates/` recursively and keeps working; README's list needs updating per move. |
| "thin LTO makes cross-crate inlining a non-issue" | Wrong for users | `lto = "thin"` and `codegen-units = 1` are this workspace's profile. A downstream user builds with their own profile; without LTO only `#[inline]`/generic functions inline across crates. Every hot function that ends up on a crate boundary must be `#[inline]`, and the IAI gate needs a row built without LTO. |
| "internal crates can be `publish = false`" | Wrong | The root crate and 10 of 11 members carry `version` + `path` dependencies (crates.io layout); only `ghost-lint` is `publish = false`. A published `sefer-alloc` needs every dependency published, so each extracted crate becomes a published crate. Owner decision G0. |
| "the core split is a pure move" | Incomplete | `src/` holds 1358 restricted-visibility declarations (`pub(crate)`, `pub(super)`, `pub(in ...)`) in 173 files (`rg -c 'pub\(crate\)\|pub\(super\)\|pub\(in ' src/`). Every one that is used across a new crate boundary becomes `pub` in a crate that, if published, any downstream can depend on directly. A safe function that touches allocator metadata through a raw pointer then becomes a public soundness hole (the R25-1 class). The split therefore owes a visibility audit (section 3) and this cost is an input to G0.1 and G1. Leaf extractions (P1) widen little; the core split (P2) widens most. |
| Grep-based reference counts | Approximate | `crate::x::` counting misses `super::` paths and grouped `use crate::{a, b}`. Step A0 replaces it with a real module graph. |

Still valid: the test-impact selector saves little for core changes (`docs/CI_TIERS.md`:
a segment-directory scan change selects 369 of 484 tests) because one crate holds all of the
core; the longest jobs of a full run are miri (33 + 33 + 24 + 22 min), `check-matrix` (30),
aarch64 (23), windows (19), kani (18).

### Revision 3 — facts

1. Extracting a leaf crate does not make CI cheaper by itself: `scripts/ci-plan.mjs` enables the
   root `fast` and `seam` groups for a change in any workspace crate (its selftest in the same
   file: `crates/containers/sefer-region/src/lib.rs` → `always,crate_sefer_region,fast,seam`).
   Savings for peripheral changes appear only with P1.5 (two-level test selection). Consequence
   for the order: after P1.1–P1.4 merge, P1.5 (and the G1 input measurement) goes next, not
   P0.3/P0.4.
2. Every extraction adds CI work: a new `<crate>-gates` job and rows in `ci-test-workspace`,
   `ci-msrv` and `ci-kani` (workflow files under `.github/workflows/`). The root crate gets path
   dependencies on the new internal crates, so publishing `sefer-alloc` first requires publishing
   them (leaf-first, decision G0.1).
3. P1.3 `sefer-policy`: `src/alloc_core/config/continuation_bounds.rs` is not policy (segment
   geometry, depends on `segment_header`) and stays in the root until P2; the root dependency is
   optional under `alloc-core`; 120 items became `pub` at the crate boundary, none depends on raw
   pointers (visibility report of the agent); `#[non_exhaustive]` on the public
   `LargeThpRequestGroup` is kept, the group→index mapping moved into the crate (`index()`).
4. P1.4 `sefer-platform` extracted less than planned: `fatal`, `process_exit` and the part of
   `os` that does not depend on the root. Left in the root: `node.rs` (`ContinuationBounds`,
   free-list metadata functions), `numa.rs`, `sidecar*.rs`, `size_classes.rs` and the directory
   triple of `os.rs` (`reserve_directory_sidecar`, `read_directory_class_words`,
   `read_directory_node_bucket`). Seven raw-pointer functions became `pub unsafe fn` with safe
   `pub(crate)` wrappers in the root (class R25-1). The remainder is a P2.0/P2.1 candidate.
5. P0.2 pilot: of 78 `regression_*` targets 35 were merged; 20 excluded by P0.1 blockers and 23
   by a NEW attribute — shared process-global state in the merged process (exact deltas of
   global counters, registry lifecycle, borrowed-heap identity, timeouts) or evidence-registry
   rows R4c-05..07 citing the lines. The P0.1 classifier (`scripts/classify-tests.mjs`) does not
   know this attribute — open item. Gain: −34 test binaries in rows that build all tests. The
   GO/NO-GO verdict follows the criterion written in `docs/perf/P0_2_TEST_MERGE_PILOT.md` (the
   file arrives in `main` with the pilot PR; this plan does not state the verdict). P0.3/P0.4 run
   ONLY on GO.
6. Process: steps that edit the same derived files — digests
   `docs/verification/ci-decomposition-baseline.json`, `docs/CI_TIERS.md`, crate and test counters
   in README/ARCHITECTURE/`src/lib.rs`, the test classification
   `docs/test_target_classification_20261115.csv` with its pins, fixture lock files
   `tests/compile_fail/*/Cargo.lock`, `fuzz/Cargo.lock` — merge as a stack: P1.2–P1.4 as one
   integration PR of three commits after P1.1; derived data is recomputed in ONE pass on the clean
   tree at the tip of the stack. See the rules in section 3.

## 2. Target layout

Crates grouped in thematic folders (new crates in brackets):

```text
crates/
  platform/     aligned-vmem  numa-shim  proc-memstat  proc-probe  [sefer-platform]
  primitives/   once-ptr-cell  tagged-index-stack  size-classes  [sefer-ghost]
  containers/   sefer-region  [sefer-concurrent]
  core/         [sefer-policy]  [sefer-segment]  [sefer-route]  [sefer-alloc-core]  [sefer-registry]
  verify/       ghost-lint  globalalloc-model
  bench/        malloc-bench-rs
src/            sefer-alloc: the facade (GlobalAlloc face, public features, re-exports)
```

Intended layering, bottom to top (hypothesis until A0 confirms the edges):

```text
primitives, platform
  -> sefer-policy (config + Large-THP policy)
  -> sefer-segment (header, remote bitmap, reservation state, segment table)
  -> sefer-route (route directory, shards, ingress)   [segment table registers routes: the
                                                       segment/route order is decided by A0]
  -> sefer-alloc-core (small, large, AllocCore hub)
  -> sefer-registry (heap core, heap registry, bootstrap, xthread)
  -> sefer-alloc (facade)
```

`sefer-segment` and `sefer-route` may have to be one crate if A0 shows a real mutual
dependency that relocation does not remove; the plan does not assume the split.

## 3. Gates that apply to every step

- Behavior boundary: pure move unless the step says otherwise; public API, feature names,
  error behavior and ordering are unchanged.
- Machine code: `ghost-zero-cost` (`.text` diff + Ir of `ghost_zero_cost_probe`). A step that
  changes `.text` is declared with the `fix(perf)` prefix and records the function and Ir.
- Instruction counts: `perf-gate.yml` IAI against the pre-step base, expected delta 0; from
  P1 on, one extra row built without LTO. That row's baseline is recorded on the unsplit
  tree first (step P1.0), otherwise the first extraction has nothing to be compared with.
- Test inventory: a step that merges, moves or re-gates tests (P0.2, P0.4, P1.x, P2.x)
  proves that no test disappeared: the set of test names per feature row
  (`cargo test ... -- --list` on CI, before and after) is compared by a script; a difference
  is red unless it is a rename listed in the PR. A mistyped per-module `cfg` otherwise drops
  tests silently while CI stays green.
- Visibility: a step that moves code to another crate lists, by script, every item whose
  visibility widened to `pub`. An item whose safety depends on a raw pointer or on an
  allocator invariant is `unsafe fn` with a `# Safety` contract, or stays unreachable
  (sealed/private module); the dbg-hook scanner covers the new crates.
- CI: a step that moves source, test targets or crates, or changes test selection
  (T1, P0.2, P0.3, P0.4, P1.x, P2.x) carries `CI: full` and is green on the landing SHA.
  Script/report-only steps (A0, A1, P0.1, P1.0) run the diff-selected set.
- Measured decisions (P0.2, G1): the criterion is written down before the numbers are read;
  raw data and a summary CSV are committed and tables are derived by a script. CI durations
  are read from ordinary CI runs (`gh run view --json jobs`, several runs per side); no
  dedicated wall-clock campaign is launched on GitHub.
- Repository rules: one file — one export, at most 7 entries per source directory,
  1000 lines per file, `mod.rs` re-exports only, tests in `tests/`, no doctests,
  ghost inventory, dbg-hook scanner, unsafe inventory, CI decomposition digests, open-items
  indexes, no version bump without the owner's request.
- No local builds or tests: `cargo check`, `cargo clippy`, `cargo fmt` and cargo-free scripts
  only; everything else on GitHub CI.
- Derived data in one pass (revision 3): steps that edit the same derived files (fact 6 of
  "Revision 3 — facts") are stacked; derived data is recomputed once on the clean tree at the
  tip, not per commit.
- Local cargo-free checks before every push (revision 3): `cargo metadata --locked` for every
  fixture `Cargo.lock` (`tests/compile_fail/*/Cargo.lock`, `fuzz/Cargo.lock`);
  `scripts/verify-evidence-registry.mjs --strict`; classifier output check
  (`scripts/classify-tests.mjs --check` against the committed per-file CSV and Markdown);
  `scripts/verify-structure-limits.mjs`; `scripts/verify-ci-decomposition.mjs`;
  `scripts/ci-plan.mjs --selftest` and `--check-doc`; `scripts/module-graph.mjs
  --check-backedges`; `scripts/ghost_inventory.mjs --check`;
  `scripts/verify-dbg-hook-safety.mjs`; `scripts/verify-loom-target-wiring.mjs` and
  `scripts/verify-miri-target-wiring.mjs`; `scripts/verify-ci-sentinels.mjs`.
- EV-1 reverify (revision 3): changing a test file cited by an evidence-registry row requires
  EV-1 reverify (`.github/evidence-reverify.json` plus the `gate-run` label; example: PR #247,
  rows R0-21/R0-22).
- No-LTO IAI before merge (revision 3): the no-LTO IAI row is measured on the integration
  branch against the P1.0 base (`docs/perf/IAI_NOLTO_BASELINE.md`) BEFORE the merge.

### Process rules (revision 4)
- Gate policy (ghost-zero-cost and similar) changes in a separate PR BEFORE the PR it judges. In this session the policy was changed three times inside the same PR it judged (extraction mode, `tokens_new`, layers mode); this is not done again. For P2.1–P2.4 the existing extraction mode is sufficient.
- Status (2026-11-21): the stabilized `ghost-zero-cost` probe and its [report](../perf/GHOST_ZERO_COST_PROBE_STABILITY_20261121.md) landed separately in PR #282 (`8b1cd0b3`), before P2.4 merged; gate policy and tolerances are unchanged.
- One P2 step = one PR, together with its derived data and an EV-1 reverify.
- The local ban on builds and tests holds without exceptions; feedback is accelerated by the cargo-free checks in the always-on job and by watching CI with a stop on the first red job.
- Side measurements and CI levers run only while the main path is waiting for CI.
- While a P2 step's PR is in its final CI run, nothing is merged into `main` (otherwise a derived-file conflict cancels the run) — until the process-hygiene package lands.
- `TEST_SELECTION=enforce` enablement criterion: a shadow measurement over a window of ≥ 30 ordinary runs (including red ones) shows "failed tests outside the selected subset = 0" and a subset-run share > 0; the safety net is the nightly full run of `main` and `CI: full` at sprint close.

## 4. Owner decisions (G0)

1. Publication: extracted crates are published as internal crates with no stability
   promise, or the core stays one crate (then only A0, A1, P0 and T are done and layering is
   enforced by the A1 lint, not by crates). Publishing means: each crate gets its own
   version, licence files, README and docs.rs row, is released leaf-first, and exposes as
   `pub` whatever crosses its boundary (section 1, visibility row).
2. Names of the new crates. The eight proposed names were free on crates.io on 2026-11-15
   (`sefer-platform`, `sefer-ghost`, `sefer-concurrent`, `sefer-policy`, `sefer-segment`,
   `sefer-route`, `sefer-alloc-core`, `sefer-registry`: HTTP 404 from the crates.io API).
   The folder names of section 2 are taken as agreed with this plan; T1 uses them unless the
   owner objects before T1 starts.
3. Whether `sefer-platform` is a new crate or `os`/`node`/`fatal` join `aligned-vmem`/`numa-shim`.

G0 gates P1 and P2 only. A0, A1, P0 and T1 do not wait for it.

**G0 resolved (2026-11-15; the owner delegated the open questions):**

1. The leaf crates of P1 are published as internal crates with no stability promise (they
   widen little). Whether the core is split and published (P2) is not decided here: it is
   decided at G1 from measured data.
2. The names of section 2 are used as proposed.
3. `sefer-platform` is a new crate: `os`, `node` and `fatal` are allocator-specific, while
   `aligned-vmem` and `numa-shim` are general-purpose published crates whose API should not
   grow allocator internals.

### G1 — decision on P2 (after P1)

Inputs, all measured: CI time and selected-test counts before/after P0 and P1, the no-LTO
Ir delta of P1, the back-edge list of A0/A1 and the count of items P2 would widen to `pub`.
Decision: do P2, or stop at lint-enforced layering. The owner delegated it; the criterion is
written into the G1 report before the numbers are read, and the decision with its numbers is
reported to the owner. Default when the numbers do not show a clear benefit: stop.

**G1 — Historical evidence-based STOP, superseded as current authorization by owner override (2026-11-16):** the measured inputs and historical STOP are preserved in [`docs/perf/G1_DECISION_REPORT.md`](../perf/G1_DECISION_REPORT.md). The owner override sets the normative disposition to DO: continue the core split and selector; P1.3/P1.4/P1.6 are to merge, and P1.5 plus P2.0–P2.5 are to proceed. P1.3/P1.4/P1.6 implementation is integrated in the landing stack; CI/perf remain pending, and no merge to main or measured acceleration is claimed. The extraction-policy reference is [`docs/GHOST_CHECKS_PLAN.md#extraction-ranges`](../GHOST_CHECKS_PLAN.md#extraction-ranges).

## 5. Steps

### A — foundations

- **A0. Module dependency graph.** Cargo-free script (`scripts/module-graph.mjs`) resolving
  `crate::`, `super::`, grouped `use` and re-exports to module-level edges; committed report
  with every back-edge listed by file and line. Replaces the grep counts of section 1.
- **A1. Layering lint.** Declared allowed edges (the layering of section 2) checked by the
  A0 script in CI; existing back-edges enter as a ratchet baseline that may only shrink.
  This gives "virtual crates" before any move and makes P2 mechanical.

### P0 — test binaries (no `src/` change)

- **P0.1. Classify the 562 test files**: process-global state (global allocator,
  first-config-wins, THP tuple, sidecar), process spawning, loom, `harness = false`,
  `required-features`, file-level `cfg`; commit the per-file CSV. Mergeability groups,
  relaxation rungs and pilot analysis are stdout-only (`node scripts/classify-tests.mjs --summary`).
- **P0.2. Pilot** (pilot written, landing pending, revision 3: 35 of 78 `regression_*` targets merged, 20 + 23
  excluded, −34 binaries; verdict by the criterion in `docs/perf/P0_2_TEST_MERGE_PILOT.md`,
  facts 5): merge one pure group (candidates: `regression_*` without global state)
  into one target with per-module `cfg`; measure link count and job time on CI against the
  base; decide go/no-go from the numbers.
  - Status (2026-11-17): verdict **NO-GO** (criterion (b) DOES NOT HOLD: thread-sanitizer median 96 → 122 s = +27.08 %; miri families insufficient-runs, n_after=1<3); see `docs/perf/P0_2_TEST_MERGE_PILOT.md` § Verdict (2026-11-17).
- **P0.3. Selection granularity** (only on GO of P0.2, revision 3): `select-tests` and the test-impact map address modules
  inside a merged target (test-name filters), so merging does not coarsen selection.
  - Status (2026-11-17): CLOSED — blocked by the P0.2 NO-GO; reopen trigger: a P0.2 pilot attempt passing all three criterion conditions of `docs/perf/P0_2_TEST_MERGE_PILOT.md`.
- **P0.4. Roll-out** (only on GO of P0.2, revision 3) group by group (`r6`/`r11`, `segment`, `large`, `heap`, ...), updating
  name pins: check-matrix rows, CI greps, J13 file hashes, published counts.
  - Status (2026-11-17): CLOSED — blocked by the P0.2 NO-GO; reopen trigger: a P0.2 pilot attempt passing all three criterion conditions of `docs/perf/P0_2_TEST_MERGE_PILOT.md` (the tsan growth is structural — see the Verdict's next steps; any criterion revision is an owner decision made FIRST).

### T — thematic folders for the existing crates (pure move)

- **T1.** Move the 11 crates into `crates/<theme>/`; update workspace members, path
  dependencies, `ci-plan` path classes, `select-tests` rules, structure-limit globs
  (`crates/*/src/**` becomes recursive), license/workspace scripts, CI actions and their
  decomposition digests, docs and README links. No source change.

### P1 — leaf extractions (one crate per step)

- **P1.0 No-LTO baseline**: add the IAI row built without LTO on the unsplit tree and record
  its Ir; every later extraction is compared with it. Hot paths at risk: `os` helpers used
  on the free path (P1.4) and config reads on the Large path (P1.3).
- **P1.1 `sefer-ghost`**: `ghost_assert!`/`ghost_event!`; rule R9 and the zero-cost job unchanged.
- **P1.2 `sefer-concurrent`**: `epoch`, `lock_free`, `sharded` behind `experimental`; its loom
  tests move with it and run only when it changes.
- **P1.3 `sefer-policy`**: `alloc_core/config` including the Large-THP runtime (tuple, curves,
  cadence, codec); pure policy tests become crate-local, J13b integration stays on top; the
  J13 file pins and static reference are re-pointed. Written, landing pending, revision 3: `continuation_bounds.rs`
  stays in the root (not policy), 120 items became `pub`, `index()` mapping in the crate
  (facts 3). (The wording "`alloc_core/config` including ..." is replaced by revision 3 for
  `continuation_bounds.rs`.)
  Status (2026-11-16): owner approved merge; implementation integrated in the landing stack; CI/perf pending; not claimed merged to main.
  Status (revision 4, 2026-11-17): merged into `main` `a966aa8f` via PR #257 (2026-11-16).
- **P1.4 `sefer-platform`** (or merge, decision G0.3): `os`, `node`, `fatal`, and the
  process-exit state that `segment_route` needs. (Replaced by revision 3: only `fatal`,
  `process_exit` and the root-independent part of `os` were extracted; `node.rs`, `numa.rs`,
  `sidecar*.rs`, `size_classes.rs` and the `os.rs` directory triple stay in the root until
  P2.0/P2.1; facts 4.)
  Status (2026-11-16): owner approved merge; implementation integrated in the landing stack; CI/perf pending; not claimed merged to main.
  Status (revision 4, 2026-11-17): merged into `main` `a966aa8f` via PR #257 (2026-11-16).
- **P1.2–P1.4 landing** (revision 3): one integration PR of three commits after P1.1, derived
  data recomputed once at the tip, no-LTO IAI measured before merge (section 3).
- **P1.5 Two-level selection** (next after the P1.1–P1.4 merge, together with the measurement
  of the G1 inputs, revision 3; facts 1): crate graph from `cargo metadata` first, the existing
  test-impact map inside the root crate second; miri, loom and kani run per changed crate.
  Status (2026-11-16): owner approved P1.5 to proceed; implementation not started; modeled history shows no selector savings, and actual CI/perf remain pending.
  Status (revision 4, 2026-11-17): P1.5 is split — P1.5a (Node tests/benches/examples do not trigger the full Rust test run), P1.5b (selection by the crates graph), P1.5c (enforce enablement by the section 3 criterion); order in section 6. Modeled history (PR #258): 0 of 28 window runs would have been subsets, so enforce without P1.5a/b saves nothing.
  Status (2026-11-17): P1.5a and P1.5b merged via PRs #266 (`c708ac96`) and #268 (`a28be5da`).
  Status (2026-11-18): PR #272 (`29fbbda1`) landed the P1.5c safety criterion; this does not enable enforce mode.
- **P1.6 Inline audit**: hot-path functions on the new crate boundaries are `#[inline]`;
  the no-LTO IAI row shows delta 0 against the P1.0 baseline.
  Status (2026-11-16): owner approved merge; implementation integrated in landing stack; CI/perf pending; not claimed merged to main.
  Status (revision 4, 2026-11-17): merged into `main` `a966aa8f` via PR #257 (2026-11-16).

### P2 — core split

- **P2.0 Remove back-edges inside the single crate** (A1 ratchet to zero), one PR each:
  a. move `segment_route` below `alloc_core` (it is used by `segment_table`, `small`, `sidecar_drain`);
  b. move process-exit state (`process_exit_in_progress`, `record_exit_lock_giveup`) to the platform layer;
  c. move or invert `route_pin_cache` (`segment_route -> heap_core_xthread`);
  d. replace `fallback::try_with_heap` in `heap_core_xthread/routing.rs` with a hook registered by the facade.
  Status (revision 4, 2026-11-17): P2.0a–d is open as PR #259 (A1 back-edge ratchet 12 → 8), in CI at the time of this revision; not merged.
  Status (2026-11-17, landing): merged into `main` via PR #259 (`1a684c38`); A1 ratchet 12 → 8, not zero.
- **P2.1 `sefer-segment` / `sefer-route`** (one or two crates per A0).
  Status (2026-11-18): merged via PR #274 (`b99b57c9`); segment and route were extracted together into `sefer-segment`.
- **P2.2 `sefer-alloc-core`**: `small`, `large`, the `AllocCore` hub and its diagnostics.
  Status (2026-11-19): `sefer-alloc-core` extraction merged via PR #279 (`7eb12eee`).
- **P2.3 `sefer-registry`**: heap core, heap registry, bootstrap, xthread.
  Status (2026-11-21): `sefer-registry` extraction merged via PR #280 (`fd7c675a`).
- **P2.4 Facade**: `src/` keeps `global`, feature forwarding and re-exports; Kani proofs move
  to the crates that own the internals they need.
  Status (2026-11-21): merged via PR #283 (`2246bfb1`); Kani proofs moved to `sefer-segment` and `sefer-registry`, with CI wired by owning package.
- **P2.5 Close-out (#187)**: the close-out ships in a PR carrying `CI: full`.
  - Implemented: per-crate feature-powerset wiring, the README unsafe inventory, the CHANGELOG and the round manifest. The workspace composite adds all-features rustdoc rows for workspace members and checks their exact docs.rs configurations where already declared. For those configurations, the docs.rs row is skipped only when the manifest declares `all-features = true` without rustdoc arguments.
  - Deferred: new docs.rs declarations in EV-1-scoped Cargo manifests require EV-1 reverify. Those manifests and the evidence-scoped `src/lib.rs` inventory mirror are unchanged.
  - Rustdoc find (2026-11-21): the new workspace all-features rustdoc row found a broken self intra-doc link, in `crates/core/sefer-alloc-core/src/alloc_core/platform/sidecar.rs`; the repair is a single doc-comment line, no code change. Because it touches evidence-scoped source, the PR requests EV-1 re-verification for the eight affected rows (R4a-03, R4b-01, R6a-A10, R6a-E1, R6a-E2, R6a-E3, R6a-EX2, R6a-EX4 — item 288 of `docs/CORRECTNESS_OPEN_ITEMS.md`), with the receipts to be recorded in the `ci(evidence)` commit; the reverify result is not claimed here. The item-288 follow-ups — (a) the root seam mirror, (b) docs.rs declarations in the seven manifests, (c) the pin-cache Loom harness (item 286) — remain deferred to #211.
  - Acceptance requires green full CI on the exact landing SHA (`node scripts/verify-sprint-close.mjs <sha>`) plus a separate scheduled or dispatched (`workflow_dispatch`) run of `feature-powerset` on the close-out tree covering `sefer-segment`, `sefer-alloc-core` and `sefer-registry`. The `CI: full` trailer does not schedule `feature-powerset`. Neither result is claimed here.

Historical decision (2026-11-16): the owner override approved P2.0–P2.5 to proceed.

Expected benefit, stated honestly: P0 and P1 reduce cost for peripheral, policy, container
and test-only changes. A change in `segment` or `small` will still reach most tests after
P2 — that is the allocator's architecture, not a CI defect. P2's main value is enforced
layering and smaller per-crate verification (miri, loom, kani, powerset), not test skipping.

## 6. Order

1. Land this plan in `main` (agents work in worktrees made from `origin/main` and must be
   able to read it).
2. (Replaced by revision 3: "Without G0: `A0` → `A1`, and in parallel `P0.1` → `P0.2` →
   `P0.3` → `P0.4`; `T1` and `P1.0` at any time. `T1` and the `P0` steps both touch CI path
   rules, so they are merged one after the other, not concurrently.") `A0`, `A1`, `P0.1`,
   `P0.2` (pilot), `T1` and `P1.0` are as before; `T1` and the `P0` steps still merge one after
   the other.
3. (Replaced by revision 3: "After G0: `P1.1` … `P1.4` (one crate per PR) → `P1.5`, `P1.6`.")
   Revision 3 order: `P1.1` → integration PR `P1.2`–`P1.4` (three commits) → `P1.5` (and
   measurement of the G1 inputs) → `G1`. `P1.6` stays after `P1.5`. `P0.3`/`P0.4` only on GO of
   `P0.2` (criterion in `docs/perf/P0_2_TEST_MERGE_PILOT.md`). Stacking, one-pass derived data,
   local checks, EV-1 reverify and the pre-merge no-LTO IAI row: section 3.
4. (Replaced by revision 4, 2026-11-17: "`G1` (owner decision on measured inputs) → `P2.0a–d` → `P2.1` → `P2.2` → `P2.3` → `P2.4` → `P2.5`.") Revision 4 order: the process-hygiene package (derived files without merge conflicts, cargo-free inventories, the EV-1 transfer script) → `P2.1` in parallel with `P1.5a` (a Node test/bench/example change must not run all Rust tests) → `P1.5b` (selection by the crates graph) → `P2.2` → `P2.3` → `P2.4` → `P1.5c` (enforce enablement by the section 3 criterion) → `P2.5`. `P1.5` no longer waits for `P2.2`.

## 7. Owner-goal metric (revision 4)

The owner's goal (quoted in the G1 override above) is to split everything into crates so that a change touches fewer tests. Honest state: with selection implemented but enforce off, 0 of 28 window runs would have been subsets even with selection enabled (PR #258, `docs/perf/TEST_SELECTION_ENFORCE_IMPACT.md`); the repository variable `TEST_SELECTION` is not set (checked 2026-11-17). P2 delivers enforced layering and per-crate verification; test-count reduction is delivered by P1.5 plus the enforce enablement.

Three numbers, recomputed by committed scripts, never hand-typed:
1. Subset-run share: numerator = completed window runs where `select-tests` returned a strict subset; denominator = completed window runs carrying a `test-selection` artifact. Current window value: 0 / 28 (PR #258). Recompute: `node scripts/test-selection-impact.mjs --report`.
2. Median test-core seconds per PR run: the median of the `test (core tiers)` cargo-test step seconds over `pull_request` runs in the window (per run, not the window sum). Current window value: 245 s over 15 pull_request runs (derived by the committed exports `loadInputs`/`computePerRun` of `node scripts/test-selection-impact.mjs` over the committed `docs/perf/test_selection_inputs`; no committed CLI prints this median directly; the window sum is 6669.0 s over 28 runs). Recompute the same way from `docs/perf/test_selection_inputs`.
3. Critical path of the full run: the top job elapsed-times of a full-selected completed run. Current: `docs/perf/CI_LONG_JOBS.md` (check-matrix median 1728 s, range 1650–1843, n=6). Recompute: `node scripts/ci-long-jobs.mjs`. After the Miri sharding (PRs #261/#262) the full-run time did not shrink; the critical path moved — the next lever is check-matrix by rows and aarch64.

