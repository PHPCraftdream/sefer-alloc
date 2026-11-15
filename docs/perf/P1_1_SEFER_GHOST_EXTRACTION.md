# P1.1 — `sefer-ghost` crate extraction (chunk D, final report)

**Status:** pure extraction, complete. Behavior boundary unchanged: public API paths
(`crate::ghost::…` resolve identically), feature names, error behavior and ordering are
untouched. Commit prefix: `build(workspace)`; CI label: `CI: full`.

## What moved

Four files moved verbatim from `src/ghost/` to
`crates/primitives/sefer-ghost/src/ghost/` (list per `git diff` of the extraction commit):

| old | new |
|---|---|
| `src/ghost/mod.rs` | `crates/primitives/sefer-ghost/src/ghost/mod.rs` |
| `src/ghost/macros.rs` | `crates/primitives/sefer-ghost/src/ghost/macros.rs` |
| `src/ghost/shadow.rs` | `crates/primitives/sefer-ghost/src/ghost/shadow.rs` |
| `src/ghost/probe.rs` | `crates/primitives/sefer-ghost/src/ghost/probe.rs` |

- `shadow.rs` and `probe.rs` are byte-identical to their pre-move forms; all macro bodies
  in `macros.rs` are the exact same text.
- Two one-line visibility edits: `mod.rs`'s macros re-export and `macros.rs`'s trailing
  re-export each changed `pub(crate) use` → `pub use`.
- `#[macro_export]` added to all five cfg'd `macro_rules!` variants in `macros.rs` —
  required by the compiler (rustc E0364): a textually-scoped macro cannot be re-exported
  `pub` across the module boundary without `#[macro_export]`.
- Root `src/lib.rs`: the two cfg'd `mod ghost;` declarations became a cfg'd re-export pair —
  `pub use sefer_ghost::ghost;` (`internals` + ghost cfg, `#[doc(hidden)]`) /
  `use sefer_ghost::ghost;` (otherwise).
- Root `Cargo.toml`: non-optional `sefer-ghost = { path = "crates/primitives/sefer-ghost", version = "0.1" }`
  dependency plus `internals = ["sefer-ghost/internals"]` forwarding.
- 15 test files: `#[path = "../src/ghost/mod.rs"]` includes repointed to the new crate path
  (count per `git diff --stat` of the test edits).
- New crate: `#![forbid(unsafe_code)]`, `#![no_std]`, zero dependencies, version 0.1.0,
  `rust-version = "1.88"`, feature `internals` (gates `probe` only), `[lints] workspace = true`
  (inherits the workspace `unexpected_cfgs` check-cfg list, which already declares
  `cfg(loom)`/`cfg(kani)`/`cfg(sefer_ghost)`).

## Visibility report (plan §3 gate в)

Derived from the `git diff` of the moved files: the only pub-widening lines in the diff are
the two `pub(crate) use` → `pub use` lines plus the five `#[macro_export]` attributes.
Exactly two items were widened at the crate boundary:

| item | before (inside sefer-alloc) | after (at `sefer_ghost::ghost`) |
|---|---|---|
| `ghost_assert!` | `pub(crate)` | `pub`, and at the `sefer-ghost` crate root via `#[macro_export]` |
| `ghost_event!` | `pub(crate)` | `pub`, and at the `sefer-ghost` crate root via `#[macro_export]` |

Both are macros that expand to nothing in production cfgs, reference no raw pointers and
carry no `unsafe` — so no `unsafe fn` + `# Safety` contract and no sealing obligation arise.
The shadow-table fns (`count`/`fail`/`overflowed`/`record`/`reset`) and `probe` were already
`pub` before the move; their visibility at the usage boundary is unchanged. The dbg-hook
scanner covers the new crate: `node scripts/verify-dbg-hook-safety.mjs` is green (verified
locally) and the crate defines no `dbg_*` hook.

## Gates and what CI confirms

- (а) `ghost-zero-cost`: normalized `.text` + Ir of `examples/ghost_zero_cost_probe.rs` vs
  base — expected delta 0; in production cfgs the macros expand to nothing and `sefer-ghost`
  compiles to an empty lib. Numbers land in CI, not here.
- (б) perf-gate IAI without LTO vs the P1.0 baseline — expected delta 0 (run manually by the
  parent; numbers land in CI, not here).
- (в) the visibility report above (derived from `git diff`, as stated).
- `ghost-guard` (ghost-lint over the moved sources): scan roots already walk
  `crates/**/src/**`; the R9 hygiene rule's `files` glob already covers the new path.
- Ghost inventory: `node scripts/ghost_inventory.mjs --check` green (verified locally —
  769 claimed sites, 10 ghost-class sites now claimed from the new path, 0 gaps/double-claims/
  dead scopes); `node scripts/ghost_inventory_mutants.mjs` 11/11 (verified locally).
  The `docs/verification/ghost-rules.json` scope edits re-point `ghost/shadow` and four
  broad-scope excludes from `src/ghost/**` to `crates/primitives/sefer-ghost/src/ghost/**`;
  no rule (R1–R9) entry changed.
- New `sefer-ghost-gates` publish job (`cargo publish --dry-run -p sefer-ghost`).
- New `ci-test-workspace` rows (plain + `--cfg sefer_ghost` test/clippy/doc + thumb `no_std`
  row) and `ci-msrv` rows (1.88 floor).
- CI decomposition digests re-recorded via `node scripts/verify-ci-decomposition.mjs` +
  `node scripts/ci-plan.mjs --selftest` + `--check-doc` (all green, verified locally);
  changed job ids: `changes` metadata, new `sefer-ghost-gates`, `test-workspace` steps,
  `msrv` steps; top-level orchestration unchanged.
- Module-graph fix: extern-anchored use-bindings now classify `crate::ghost::…` path
  references as external instead of misclassifying them as facade edges; a regression test
  was added. `node --test tests/module_graph.test.mjs tests/layering_lint.test.mjs` 32/32
  green; `node scripts/module-graph.mjs --check-backedges` reports the same 12 baselined
  rows (all verified locally).

## Verified locally

- `node scripts/ghost_inventory.mjs --check` (769 sites claimed; 10 ghost-class sites from
  the new path; 0 gaps/double-claims/dead scopes)
- `node scripts/ghost_inventory_mutants.mjs` (11/11)
- `node scripts/module-graph.mjs --check-backedges` (same 12 baselined rows)
- `node --test tests/module_graph.test.mjs tests/layering_lint.test.mjs` (32/32)
- `node scripts/verify-structure-limits.mjs`
- `node scripts/verify-dbg-hook-safety.mjs` (crate has no dbg_* hooks; the scanner's
  `crates/**` walk covers it)
- `node scripts/verify-ci-decomposition.mjs`
- `node scripts/ci-plan.mjs --selftest` and `node scripts/ci-plan.mjs --check-doc`
- `wnice cargo check`/`clippy` rows: root crate default, production, production internals;
  `sefer-ghost` plain + `RUSTFLAGS="--cfg sefer_ghost"` + clippy `-D warnings`
- `RUSTFLAGS="--cfg sefer_ghost" cargo check --features "production internals" --tests`
  (the 15 repointed test files type-check)

## Left to GitHub CI

- ghost-lint binary over the moved sources (`cargo build`/`run` forbidden locally)
- `cargo publish --dry-run -p sefer-ghost` (the new `ci-sefer-ghost-gates` job)
- the new `ci-test-workspace` / `ci-msrv` rows
- the `ghost-zero-cost` job (normalized `.text` + Ir vs base — expected delta 0)
- the perf-gate IAI no-LTO row vs the P1.0 baseline (parent runs it manually; expected
  delta 0)

## Open questions / notes

1. ghost-lint binary behavior over the new crate paths is CI-only.
2. The `#[allow(unused_imports)]` on the root `lib.rs` private re-export is kept: with the
   attribute removed, `wnice cargo check` emits `warning: unused import: sefer_ghost::ghost`
   at `src/lib.rs` under the default (non-ghost) cfg — the import exists precisely so
   `crate::ghost::…` call sites resolve, and it is unused as a value path in production.
   The stale `// no production call sites until G2` comment was replaced with a truthful
   one (`// the import is what `crate::ghost::…` call sites resolve through`).
3. No raw-log / summary-CSV obligations: this report's verdict rests on no measured
   numbers (CLAUDE.md R22-14 boundary rule); the numbers arrive via the CI gates above.
4. The affected ghost-catalog entries (`docs/verification/ghost-rules.json`) are SCOPES
   only (`ghost/shadow` + four excludes) — no rule entries changed, hence no
   Invariant-Check trailers are owed from this step.
