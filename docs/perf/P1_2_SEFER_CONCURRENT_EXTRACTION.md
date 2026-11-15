# P1.2 — `sefer-concurrent` extraction gate (statement-only, no measured numbers)

Workspace-restructure plan step P1.2 (`docs/plans/2026-11-15-workspace-restructure-plan.md` §P1):
the legacy/research concurrent tier moved out of the root crate into
`crates/containers/sefer-concurrent` (0.1.0, unpublished). This report is
statement-only per the task: no local benchmark numbers are recorded here; CI
owns the measured gates.

## What was extracted

- Source moved **verbatim** (no logic edits): `src/epoch/` (`epoch_handle.rs`,
  `epoch_region.rs`, `hand.rs`), `src/lock_free/` (`lock_free_handle.rs`,
  `lock_free_region.rs`), `src/sharded/` (`sharded_handle.rs`,
  `sharded_region.rs`), `src/pinning.rs` — now under
  `crates/containers/sefer-concurrent/src/{epoch,lock_free,sharded,pinning.rs}`.
- The epoch Kani proof harness moved as `src/kani_proofs.rs`
  (`#[cfg(kani)]`, `#[path]`-free, `pub(crate)` access to `AtomicSlot`).
- The root keeps only a re-export shim: `src/concurrent/mod.rs`
  (`pub use sefer_concurrent::{EpochHandle, EpochRegion, LockFreeHandle,
  LockFreeRegion, ShardedHandle, ShardedRegion};` plus
  `#[cfg(feature = "pinning")] pub use sefer_concurrent::PinnedRunner;`).
  `src/lib.rs`'s `pub use concurrent::{...}` and every public path behind it
  resolve unchanged.
- Root feature rewiring (root `Cargo.toml`): `experimental =
  ["std", "dep:sefer-concurrent"]`, `pinning =
  ["experimental", "sefer-concurrent?/pinning"]`,
  `internals = ["sefer-concurrent?/internals"]`,
  `bench-internals = ["…, sefer-concurrent?/bench-internals"]` — the
  same-named crate features forward from the root, so every previous feature
  combination still compiles the same code.
- Dependencies moved into the crate manifest: `arc-swap`, `crossbeam-epoch`,
  and `core_affinity` (optional, behind the crate's `pinning` feature).

## Visibility: derived verdict

`node scripts/p12-visibility-report.mjs` derives the crate-boundary picture
and asserts it (cargo-free; parses the moved sources, classifies every
item-level `pub` declaration, and compares the crate root's `pub use` set
against `git show HEAD:src/concurrent/mod.rs`):

```
p12-visibility-report: widened=0; legacy-reexports=7; internals-gated-hooks=8; bench-internals-gated-hooks=4
```

Rationale: every `pub` item on the crate boundary was already `pub` and
reachable from `sefer_alloc` under the same feature combination before the
move — the seven re-exported types, and the `_*_for_tests` hooks under the
root's `internals`/`bench-internals` features, which the root features now
forward to the crate. Nothing was widened; nothing was narrowed. The eight
`internals`-gated hooks (epoch ×2, sharded ×6, including the
`all(feature = "internals", feature = "bench-internals")` hint hook) and the
four `bench-internals`-gated hooks (lock-free ×4) keep their `#[cfg]`
attributes verbatim; a doc-comment mention of a feature name is not a gate
(the script's walk-back logic mirrors
`scripts/verify-alloc-core-dbg-internals-exhaustive.mjs`).

## Tests moved (22 files, gate mapping)

Moved from root `tests/` into `crates/containers/sefer-concurrent/tests/`:

| gate before (root) | gate after (crate) | files |
|---|---|---|
| default (`experimental`) | crate default | `epoch.rs`, `lock_free.rs`, `sharded.rs`, `sharded_remote.rs`, `pinning.rs`, `r11_epoch_false_full.rs`, `r11_p4_2_lock_free_write_clone_count.rs`, `r11_p4_3_sharded_per_region_binding.rs`, `r13_sharded_dead_region_retention.rs`, `r14_shard_late_tls_teardown.rs`, `r14_shard_prune_work_bound.rs`, `r15_01_epoch_region_drop_panic_isolation.rs`, `regression_r2_03_cross_instance_identity.rs`, `regression_r2_04_generation_setter_lockdown.rs`, `regression_r2_20_lock_free_rejected_no_cow.rs`, `regression_r2_21_epoch_drain_buffer_reuse.rs`, `xs_r3_02_sharded_region_len_docs.rs`, `xxs_r5_02_epoch_generation_setter_gated.rs`, `xxs_r5_03_lock_free_with_pages_overflow.rs` |
| `internals` (+`bench-internals` where used) | same names, crate-locally | `loom_epoch.rs`, `loom_sharded.rs`, `loom_r11_epoch_false_full.rs` (loom shadow models; crate cfg(loom) dev-dependency) |

Test names are moved, not dropped: the plan §3 test-inventory gate holds (same
test names per feature row).

What stayed in the root `tests/` and why:

- `tests/concurrent_stress.rs` — drives `sefer_alloc`'s `SyncRegion`
  (sefer-region) surface and is pinned by the ci-feature-powerset matrix;
  not concurrent-tier code.
- `tests/regression_r2_02_epoch_region_non_send_value.rs` (+ its
  `tests/compile_fail/r2_02_epoch_region_non_send_value/` fixture) — drives
  the ROOT crate's `sefer_alloc::EpochRegion` re-export (`!Send` leak
  prevention through the public re-export), preserved unchanged.

## loom / kani relocation

- The three loom shadow models (`loom_epoch`, `loom_sharded`,
  `loom_r11_epoch_false_full`) now run via `cargo test --release
  -p sefer-concurrent` in the `loom-experimental` job
  (`.github/actions/ci-loom-experimental/action.yml`) — per the plan's
  principle that a tier's models run only when the tier changes.
- `hand_proofs` moved to the crate's `src/kani_proofs.rs` and is proved by a
  dedicated `cargo kani -p sefer-concurrent` step in the kani job
  (`.github/actions/ci-kani/action.yml`) — Kani proofs live in the crate
  owning the internals they need (`AtomicSlot`).

## Expected gates (CI verifies; no local measurement)

- `ghost-zero-cost`: **no production `.text`/Ir change expected.** The tier is
  `experimental`-only and absent from production feature sets; production
  builds do not contain the moved code.
- IAI (including the P1.0 no-LTO row): **delta 0 expected** against the
  P1.0 baseline, for the same reason (the extraction touches no production
  build; P1.6 will re-audit any boundary inlining that ever matters).
- Test inventory (plan §3): same test names per feature row, moved not
  dropped — verified by the test-impact map / inventory gates.
- Wiring verifiers: `verify-loom-target-wiring`, loom/kani CI steps, and the
  checks below must stay green.

## P1.1 (`sefer-ghost`) linkage note

`src/concurrent` used **no** ghost macros and no `crate::ghost` items
(verified by grep at extraction time), so `sefer-concurrent` has **no
dependency on ghost code** and nothing must be re-linked when P1.1 lands.
The ghost-rules.json P13/P14/P15 rules and their
`tests/ghost_lint_fixtures/p14_*` / `p15_*` fixtures (which stay in the root
`tests/`) keep protecting the moved code under the updated crate paths
(`crates/containers/sefer-concurrent/src/...`, `…/tests/loom_*.rs`).

## Affected repository rules / where the move is self-checked

- `node scripts/ghost_inventory.mjs --check` (green after path re-pointing;
  summary: 769 ordering sites, 0 unclaimed, 0 claimed twice, 0 dead scopes).
- `node scripts/verify-dbg-hook-safety.mjs` (green; reviewed-surface rows and
  the self-test fixtures re-pointed to the crate paths).
- `node scripts/p12-visibility-report.mjs` (green; see above).
- `node scripts/verify-structure-limits.mjs`, `node scripts/module-graph.mjs
  --check-backedges` (untouched layering baseline).
- A1's claim-6 doc-comment pin in `scripts/module-graph/layers.json`
  (`src/alloc_core/platform/node.rs` contains `concurrent::epoch::hand`) is
  why that comment TEXT was deliberately left unchanged — the pin asserts the
  literal string; the module graph treats `concurrent` as a leaf with
  no incoming code edge, which the shim (re-exports only) preserves.
- Root `README.md` updated: the "Where unsafe lives" inventory entry, the
  workspace crate-layout tree and the crate-safety table now name
  `sefer-concurrent` (`crates/containers/sefer-concurrent`) under the
  "Research / older tiers" framing.

## Open items

- Publish ordering: the crate is 0.1.0 and unpublished; the gates job runs
  `cargo publish --dry-run -p sefer-concurrent` only. No
  `cargo semver-checks` baseline exists yet (first release will create it).
- Root `README.md` crate-layout tree and unsafe inventory updated in this
  change set; any later docs referencing `src/concurrent/...` paths must use
  the crate paths.
- Parent commits: the workspace `build(workspace)` member-list commit and the
  `CI: full` commit carry the wiring; this worktree's changes are left
  uncommitted by design.
