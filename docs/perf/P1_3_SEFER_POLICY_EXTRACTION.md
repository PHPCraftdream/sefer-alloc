# P1.3 — `sefer-policy` extraction (allocator POLICY out of `src/alloc_core/config`)

Plan step: `docs/plans/2026-11-15-workspace-restructure-plan.md` §5 P1.3.
Date: 2026-11-15. Branch: `p13-sefer-policy-20261115` (based on post-T1 `origin/main`).
**Behavior boundary: pure move. No runtime behavior, public API path, feature
name, or ordering changed.** Everything in this report that is a number is
produced by `scripts/policy_visibility_report.mjs` (derived, not typed); its
JSON dump is committed as `docs/perf/P1_3_visibility_widening.json`.

## 1. What moved → `crates/core/sefer-policy` (new crate, v0.1.0)

Preserving file structure (module tree inside the new crate mirrors the old
`alloc_core::config` subtree; internal `crate::alloc_core::config::…` paths
were rewritten to the new crate root `crate::…`):

- `large_cache_config.rs`, `large_cache_mode.rs`, `small_segment_pool_config.rs`
  (→ crate root)
- `medium_promotion.rs` (→ crate root; the promotion-reachability predicate:
  the `cfg!` const + the two `#[cfg]`-generator macros)
- the whole `policy/` catalog: `mod.rs`, `profile.rs`, `small_pool_policy.rs`,
  `large_cache_policy.rs`, `large_thp/` (tuple codec, curves, estimators,
  share/gradient/mode/request-group), `large_thp_observation/` (bounded mix,
  control selector, residency plan, span geometry, error, window),
  `large_thp_runtime/` (ThpTuple, tuple cell, advice, budget ledger, cadence,
  cadence/learning cells, ThpRuntime statics)
- `continuation_bounds.rs` did **NOT** move (see §2).

New crate scaffold (modeled on `sefer-region` / `once-ptr-cell`):
`Cargo.toml` (version **0.1.0 — first version, not a bump**; edition 2021;
rust-version 1.88; MIT OR Apache-2.0; readme/repository/homepage/
documentation/keywords/categories; `package.metadata.docs.rs` with
`all-features = true` + `rustdoc-args docsrs` — all-features, so no
narrower-than-all-features doc-lint CI row is owed), `README.md`,
`CHANGELOG.md`, `LICENSE-MIT`, `LICENSE-APACHE`, `src/lib.rs` with
`#![no_std]` and **`#![forbid(unsafe_code)]`** (the moved files contain zero
`unsafe` — only `core::sync::atomic` state; the unsafe inventory is
unchanged: the new crate contributes 0 unsafe sites, 0 `allow(unsafe_code)`
seams).

Dependency: `aligned-vmem = { path = …, version = "0.2", default-features =
false }` — the ONLY dependency, needed solely for `ThpAdvice::classify`'s
error type (`aligned_vmem::VmemError`), which moved verbatim with the file.
No cycle: `aligned-vmem` does not depend on `sefer-policy` or on the root
crate. No other root dependency is referenced by any moved file (grep for
`crate::`/`super::`/`crate::ghost` before the move: only
`crate::alloc_core::config::…` self-paths and doc-comment links).

### Feature mesh (behavior-preservation mechanism)

The moved files gate on NINE root-crate feature names. sefer-policy mirrors
each 1:1 (all default-off, none implying another): `alloc-decommit`,
`alloc-global`, `bench-internals`, `internals`, `large-cache-extended`,
`medium-classes`, `exact-span-large`, `large-reserved-capacity`,
`numa-aware`. The root manifest forwards each one in weak form
(`alloc-decommit = ["alloc-core", "sefer-policy?/alloc-decommit"]`, …; a weak
forward is a no-op when the optional dependency is absent, which is exactly
when the moved code is not compiled), so
every `#[cfg(feature = …)]` inside the moved files evaluates exactly as it
did in the root crate — under every root feature row (`production`,
`hardened`, `--all-features`, alloc-core-only, …). Two `#[macro_export]`
attributes were added to the `medium_promotion!` macros: `macro_rules!`
items are crate-local by default, and the root crate consumes them across
the new crate boundary; `#[macro_export]` also places them at
`sefer_policy::` root (new public macro names, documented in the crate
CHANGELOG).

## 2. What stayed in the root crate, and why

- **`continuation_bounds.rs`** — stays at the exact path
  `crate::alloc_core::config::continuation_bounds` (same `#[cfg(not(feature
  = "hardened"))]` gate, same `pub(crate)` visibility). It is NOT policy:
  it reads `segment_header` (SegmentHeader/Layout/SegmentMeta), `os::SEGMENT`,
  `fatal::fatal`, and `size_classes::MIN_BLOCK`, i.e. segment geometry for
  the owner-only free-list walk, with the raw-pointer parameter
  (`base: *mut u8`). Moving it would create the one real root→policy back-edge
  and widen a raw-pointer-taking item. **It moves to `sefer-segment` in plan
  step P2.1, as the plan already schedules.** `platform/node.rs`'s
  platform→config back-edge (layering baseline row
  `alloc_core.platform->alloc_core.config,1`) still targets it here.
- **`src/alloc_core/config/mod.rs`** — now only declares
  `mod continuation_bounds;` (unchanged gate/visibility) and re-exports the
  moved modules from the new crate
  (`pub use sefer_policy::{large_cache_config, large_cache_mode, policy,
  small_segment_pool_config};` under the unchanged `alloc-decommit` gate,
  plus the ungated `pub(crate) use sefer_policy::medium_promotion;`).
  All ~100 existing `crate::alloc_core::config::…` call sites in `src/`,
  `tests/`, `benches/`, `examples/`, and the public API
  (`sefer_alloc::{Profile, LargeCacheConfig, LargeCacheMode, SmallPoolConfig,
  SmallPoolPolicy, LargeCachePolicy, LargeThp*}` re-exports in `lib.rs`)
  keep resolving UNCHANGED, because the root re-exports the modules
  themselves, not just the items.
- **Root `Cargo.toml`** — workspace member `crates/core/sefer-policy`
  added; OPTIONAL path+version dependency
  `sefer-policy = { path = "crates/core/sefer-policy", version = "0.1", optional = true }`,
  enabled by `alloc-core` via `dep:sefer-policy` (like aligned-vmem), so the
  default build gets neither sefer-policy nor aligned-vmem through it;
  nine feature rows extended with the weak forwarding edge
  `sefer-policy?/<feature>` (list above).

## 2a. `LargeThpRequestGroup`: `#[non_exhaustive]` preserved

The enum keeps its `#[non_exhaustive]` attribute, so the public type
`sefer_alloc::LargeThpRequestGroup` keeps its contract: adding a variant stays
non-breaking and downstream matches still need a wildcard arm. Cross-crate, the
root allocator could no longer match the enum without a wildcard, so the total
group-to-index mapping lives inside `sefer-policy` as
`LargeThpRequestGroup::index()` (`#[inline]`, exhaustive match with no
wildcard; an in-crate match compiles regardless of `#[non_exhaustive]`). The two
root matches (`advise_fresh_large.rs`, `observe_large_control.rs`) and the
duplicate mapping in `thp_runtime.rs`'s private `group_index` are replaced by
`LargeThpRequestGroup::of_bytes(..).index()`; the values and all arithmetic are
unchanged. `index` is new additive public API, not a widening of an existing
item. Its boundary behaviour is asserted by
`crates/core/sefer-policy/tests/large_thp_request_group_index.rs`.

Which uses exist, that the root `src/` has no match on the enum, that the
attribute is present on the definition, and the public re-export chain with its
`cfg`/`doc(hidden)` attributes are all derived and asserted by
`scripts/policy_request_group_usage.mjs` and recorded in
[`P1_3_request_group_usage.md`](P1_3_request_group_usage.md) (generated; no
numbers are quoted here).

## 3. Visibility report (script-derived)

`node scripts/policy_visibility_report.mjs` pairs every line of every moved
file (33 files, 4732 lines) against its HEAD original under the exact
invertible move rewrites; any unexplained line aborts the script. It
asserts `raw_pointer_items == 0`.

| metric | value (script output) |
|---|---|
| files scanned / lines paired | 33 / 4732 |
| items widened to `pub` | **120** (118 `pub(crate)`, 2 `pub(super)`) |
| by category | function 63, field/other 25, const 22, re-export 5, type 5 |
| widened items touching raw pointers / allocator invariants | **0** (asserted) |

Safety audit behind the 0: the widened set is plain-data config fields
(`usize`/`Option`/enum), const resolution helpers (`resolved_*`, `dbg_*`
pure-by-value resolvers), `core::sync::atomic` cell accessors
(`ThpTupleCell`/`ThpCadenceCell`/`ThpLearningCell`/`ThpBudgetLedger` — the
atomic orderings/protocols themselves are unchanged and remain covered by
their ghost catalog rules), macro re-exports, and the five policy types.
The only raw-pointer-adjacent item in the old config tree
(`ContinuationBounds::new(base: *mut u8)`) did not move. No item required
conversion to `unsafe fn` and none stayed in the root for safety reasons.
The dbg-hook scanner catalog
(`scripts/verify-dbg-hook-safety.mjs`) was re-pointed to the new paths and
passes unchanged in content (4 entries).

Per the plan's cross-crate inlining rule: no existing `#[inline]`/`#[inline(always)]`
attribute was changed or removed, and no `#[inline]` was added to a moved
accessor. The only added `#[inline]` is on the new
`LargeThpRequestGroup::index`. Additive (non-widening) items are listed by the
same script (`additive` in the JSON): the three `impl Default` blocks, `index`,
and the `group_index` body replacement.

## 4. Tests

**No test moved.** The J13b integration family, the loom actual-source
models, and everything that builds the allocator stay in the root `tests/`
(per the task's mandate). Test-inventory gate: the set of test targets and
names is unchanged (zero renames, zero deletions — nothing to compare
differently per feature row). Deliberately NOT moved, with reasons:

- `tests/profile.rs`, `tests/large_cache_config_knobs.rs`,
  `tests/large_cache_mode.rs` etc. construct `AllocCore` /
  `dbg_*` read-back seams — allocator integration, stays.
- `tests/j13b_cadence_pure.rs`, `tests/j13b_observation_*.rs`,
  `tests/loom_thp_*.rs` mount the production files via `#[path]`
  includes with real-atomics/CAS shims — they stay in the root (loom/actual-
  source harnesses belong to the root's verification tier); their `#[path]`
  pointers were re-pointed to `crates/core/sefer-policy/src/…` and their
  path-compat shim trees updated to the new `crate::policy::…` resolution
  (behavior-identical: same real files, same model logic).
- `tests/r1_03_pool_docs_contract.rs` (`include_str!`),
  `tests/r6_t17_doc_contracts.rs`, `tests/no_stale_doc_references.rs`,
  `tests/r6_t20_medium_promotion_predicate.rs` — path constants re-pointed
  to the new locations; assertions unchanged.

## 5. J13 / static references re-pointed

- `scripts/j13b_static_reference.mjs`: `RUNTIME` path →
  `crates/core/sefer-policy/src/policy/large_thp_runtime/thp_runtime.rs`;
  the whole-file SHA-256 pin recomputed after the visibility widening +
  rustfmt (`54afe772e8aa…`); the `ADMISSION` overlay anchor was verified to
  still apply (overlay transformation executed successfully). The `FRESH`
  pin (`advise_fresh_large.rs`) is untouched and its hash verified OK.
  `scripts/classify-tests/pins.mjs` derives the hash pins from the script
  shape — no edit needed.
- `scripts/verify-dbg-hook-safety.mjs`: the 4
  `src/alloc_core/config/large_cache_config.rs::dbg_resolved_*` entries
  re-pointed; script green.
- `docs/verification/ghost-rules.json`: 61 path strings
  `src/alloc_core/config/policy/…` → `crates/core/sefer-policy/src/policy/…`
  (scope lists + cited files only; no rule content changed). 55 catalog
  rules cover files that moved (P24–P29 families and the `thp-*` rule
  families); **every touched rule needs an `Invariant-Check:` trailer on the
  landing commit** per CLAUDE.md (list in §8). GHOST_INVENTORY.md did NOT
  need regeneration: `node scripts/ghost_inventory.mjs --check` is green
  (769 claimed sites, 0 unclaimed, 0 double-claimed) because the inventory
  scans `src/**` AND `crates/*/src/**`, and the catalog paths were
  re-pointed in the same change.
- `scripts/classify-tests/features.mjs`: comment + `MARKER_PROVENANCE`
  provenance paths for `thp_tuple` / `thp_runtime` / `thp_tuple_cell`
  re-pointed. Known follow-up (documented, not silently dropped): the
  first-config-wins marker *derivation* greps `src/` only
  (`classify-tests.mjs` line 135), so a future regeneration of
  `docs/test_target_classification_20261115.csv` would drop those three
  markers unless the derive scan is widened to `crates/*/src/**`. The
  committed CSV is a P0.1 planning artifact, not CI-enforced; regeneration
  is deferred to P0.3/P0.4 which already own that CSV.
- `scripts/ci-plan.mjs`: `'sefer-policy': 'crate_sefer_policy'` added to
  `CRATE_GROUPS` (NOT in `OS_CRATES` — the crate is OS-independent), plus
  one selftest assertion (74 checks PASS).
- Shared-file edits kept minimal for the five sibling PRs: root
  `Cargo.toml` (members + dep + 9 feature forwards), `scripts/ci-plan.mjs`
  (2 lines), `.github/workflows/ci.yml` (1 output + 1 job block),
  `docs/verification/ci-decomposition-baseline.json` (regenerated digests),
  `docs/structure_limits_baseline.csv` (one row lowered 2923→2922),
  `README.md` + `docs/ARCHITECTURE.md` (crate tables). **Not touched**:
  `src/ghost`, `src/concurrent`, `src/alloc_core/platform`,
  `src/global/process_exit.rs`, `scripts/module-graph/layers.json`,
  `docs/layering_baseline.csv`, ghost-lint fixtures, and everything owned
  by P0.2/P1.1/P1.2/P1.4.

## 6. CI wiring

- New composite action `.github/actions/ci-sefer-policy-gates/action.yml`
  (modeled on `ci-sefer-region-gates`): clippy `--all-features
  --all-targets -D warnings`, clippy `--no-default-features --all-targets
  -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --all-features
  --no-deps`, `cargo publish --dry-run`. No semver-checks row yet (no
  published baseline exists for 0.1.0 — noted in the action; add it with
  the first publish). The crate has no narrow `docs.rs` feature list
  (`all-features = true`), so per CLAUDE.md no separate docs.rs-feature doc
  row is owed.
- `ci.yml`: `crate_sefer_policy` plan output declared; `sefer-policy-gates`
  job gated on it. `ci-plan.mjs` emits the output automatically from
  `CRATE_GROUPS`; `crates/core/sefer-policy/**` classifies as
  `always,crate_sefer_policy,fast,seam` (root crate depends on every
  workspace crate, unchanged semantics).
- `docs/verification/ci-decomposition-baseline.json`: regenerated by the
  script's own hashing logic (`--record` requires a clean tree, so the
  receipt was recomputed with the identical digest code and written; the
  verifier passes: 61 jobs, orchestration/commands identical). **Changed
  entries, each explained**: (1) `orchestration` digest — the jobs list
  gained one job; (2) `changes` job `steps` digest — one new plan output
  line; (3) new `sefer-policy-gates` entry. No other job changed.
- `docs/CI_TIERS.md` not edited: it documents group→job mapping for the
  OS/crate groups it lists; adding the new crate row belongs to the parent
  commit that lands the whole set (kept out to minimize shared-file churn
  for the sibling PRs — flagged as an open question in §9).

## 7. Verification performed (commands + outcomes, all local, all under `wnice -p idle -c 50 -t 4`)

Cargo (check/clippy/fmt only — no build/test/bench/run/doc execution):

| command | outcome |
|---|---|
| `cargo metadata --no-deps` | exit 0 |
| `cargo check --workspace --all-targets` | exit 0 |
| `cargo check -p sefer-policy --all-targets` / `--all-features --all-targets` | exit 0 |
| `cargo check -p sefer-alloc --features production --all-targets` | fails ONLY on `examples/sol_f1_dbg_carve_batch_negative_probe.rs` — the DELIBERATE internals negative-boundary probe that MUST fail to compile without `internals` (see `scripts/verify-internals-negative-boundary.mjs`'s own contract). Pre-existing by design, unrelated to this change; the CI-shaped rows below are the real gates. |
| `cargo check --workspace --all-features --all-targets` | exit 0 |
| `cargo clippy -p sefer-policy --all-features --all-targets -- -D warnings` | exit 0 (after adding three behavior-identical `impl Default` delegating to `new()` — clippy `new_without_default` fires cross-crate where it could not fire same-crate) |
| `cargo clippy -p sefer-policy --no-default-features --all-targets -- -D warnings` | exit 0 |
| `cargo clippy --lib --features production -- -D warnings` (CI row) | exit 0 |
| `cargo clippy --all-targets --features "production internals" -- -D warnings` (CI row) | exit 0 |
| `cargo clippy --all-targets --features "hardened medium-classes internals" -- -D warnings` (CI row) | exit 0 |
| `node scripts/fmt-check.mjs` (rustfmt gate, argv-budgeted) | exit 0 (7 moved/test files reformatted by rustfmt in the process) |

Cargo-free scripts (all green in the final sweep):

`module-graph.mjs --check-backedges` (layering lint OK — 12 back-edges
within the 12-row baseline; `layers.json` and `docs/layering_baseline.csv`
needed NO edit: the A0 table already declares `alloc_core.config` as the
future `sefer-policy` layer/boundary, and the two baseline rows still hold —
`continuation_bounds` stayed, and `platform/node.rs`'s policy read is
unchanged), `ghost_inventory.mjs --check`, `ghost_inventory_selftest.mjs`,
`verify-structure-limits.mjs`, `ci-plan.mjs --selftest`,
`verify-ci-decomposition.mjs` (+ its embedded fixtures/contracts),
`select-tests.mjs --selftest`, `verify-root-package-list.mjs`,
`verify-dbg-hook-safety.mjs`, `verify-ci-sentinels.mjs`,
`verify-loom-target-wiring.mjs`, `verify-miri-target-wiring.mjs`,
`verify-no-bare-abort.mjs`, `verify-b4-30-pin.mjs`,
`verify-evidence-registry.mjs`, `verify-perf-gate-stubs.mjs`,
`license-check-fingerprint.mjs --selftest`,
`license-check-workspaces.mjs --selftest`,
`verify-sprint-close.mjs --selftest`, and the j13b static-reference overlay
transformation executed directly (both pinned hashes verified, overlay
receipt produced).

Known pre-commit artifacts (resolve when the parent commits; NOT defects):
`verify-invariant-trailers.mjs` requires the built `ghost-lint` binary
(cargo build — forbidden locally) and a commit range; `license-check-
workspaces.mjs --check` and `classify-tests.mjs --csv` require a clean git
index (the new action/crate files are untracked and the deleted config
files are still in the index until the parent's `git add -A`/commit).

Expected CI gates (stated expectations, no local numbers):

- **ghost-zero-cost: expected delta exactly 0.** No production code path,
  layout, or ordering changed; the move is symbol-relocation only, and the
  probe exercises the allocator, not the policy layer.
- **IAI (incl. the P1.0 no-LTO row): expected delta exactly 0** against the
  pre-P1.3 base. Config reads on the Large path now cross an rlib boundary;
  the moved accessors are const-evaluable or `#[inline]`-eligible and
  thin-LTO inlines them; the no-LTO row is exactly why P1.0 exists — if a
  nonzero delta shows up there, the fix is `#[inline]` additions under
  P1.6's inline audit, not a policy change.
- **Test-inventory gate: expected zero diff** (no test merged/moved/renamed).
- **Feature powerset rows: expected green** — the forwarding mesh makes
  every `#[cfg]` evaluate as before; verified at the six strongest local
  rows above.

## 8. Affected catalog rules (for the parent's `Invariant-Check:` trailers)

Rules whose protected files moved (paths re-pointed in
`docs/verification/ghost-rules.json`; content unchanged): 55 catalog
scopes, ID families: `P24`, `P24/thp-tuple-cell`, `P25`, `P25/thp-ledger`,
`P26`, `P26/control-diagnostics`, `P26/control-ordinal`,
`P26/control-ticket-log`, `P27`, `P27/estimator-pair`,
`P27/runtime-learning-diagnostics`, `P29`, `P29/cadence-cell`,
`P29/cadence-diagnostics`, and the `thp-cadence/*`, `thp-cell/*`,
`thp-control/*`, `thp-learning/*`, `thp-ledger/*` families plus
`thp-published-store-relaxed`, `thp-reserve-unconditional-add`
(exact list derivable from `docs/verification/ghost-rules.json` by grepping
`crates/core/sefer-policy`). Trailer text: Change = path relocation only;
Why = no ordering, atomic, or protocol site changed (inventory site count
and orderings identical — `ghost_inventory.mjs --check` green);
Covered-by = the existing loom/miri/kani fixtures each rule already cites
(`tests/loom_thp_*.rs`, `tests/ghost_lint_fixtures/p27_*` — unchanged, still
green by construction).

## 9. Open questions / deferred

1. `classify-tests` derivation scope (§5): widen to `crates/*/src/**` before
   the next committed regeneration of the classification CSV (owner: P0.3).
2. `docs/CI_TIERS.md` row for `sefer-policy-gates` + a root `CHANGELOG.md`
   entry: deliberately left to the parent commit to keep shared-file churn
   minimal for the five sibling PRs rebase chain.
3. `cargo publish --dry-run` in CI will exercise the real packaging; the
   `exclude`/tarball story for `crates/core/sefer-policy` follows the
   sibling crates (no repo-private paths in any file — verified: no
   machine-private absolute paths written).
4. `#[macro_export]` puts the two `medium_promotion` macros at
   `sefer_policy::` root as a side effect of cross-crate macro re-export;
   if the owner prefers them hidden, follow-up is `#[doc(hidden)]` on the
   re-exports (cosmetic, no behavior impact).
5. Large-THP `#[path]`-included loom models now read sources from
   `crates/core/sefer-policy/src/…`; a later step could move those loom
   targets INTO the policy crate's own `tests/` (P1.5 two-level selection
   is the natural owner) — not done here because loom target wiring,
   `scripts/loom.mjs` manifests, and CI decomposition digests are shared
   surfaces the sibling PRs also touch.
