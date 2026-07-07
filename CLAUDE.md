# sefer-alloc — project conventions

Core instructions, mandatory for all code in this repository. They
**override** default behavior.

## File and module structure

- **One file — one export.** Each source file defines exactly one public item
  (type, trait, function). The file name matches the export.
- **`mod.rs` — reexports only, no code.** The `mod.rs` file contains
  exclusively `mod`/`pub mod`/`pub use` declarations. No logic, types,
  functions, or tests belong in `mod.rs` — it only wires modules together.

## Tests

- **Put tests in a separate folder from the start.** Do not leave tests inline
  in the module file (`#[cfg(test)] mod tests` inside `src/*.rs`). Tests live in
  `tests/` (integration) with a mirrored structure; new code is written with
  tests in separate files from the very beginning, not extracted later.

## Phased delivery

- **Every phase is delivered with tests** — code without tests is not considered
  a completed phase.
- **Between phases: run tests and commit.** Before moving to the next phase,
  run `cargo test` (and miri/loom where applicable), confirm everything is
  green, and commit that phase. These are explicitly sanctioned commits between
  phases (the general prohibition "do not commit without being asked" is lifted
  by the user for phase boundaries). Push — only on a separate explicit request.
- **After each phase — ZERO-TRUST review.** Before committing a phase
  (especially if the code was written by a sub-agent): personally read the
  entire diff line by line; rerun the tests yourself (do not trust the agent's
  claim "tests passed"); verify the tests are not vacuous (would they fail
  without the fix — counterfactual); run an adversarial audit by rust-intel
  categories (rust-cc-audit / code-review); look for out-of-scope edits,
  TODO/placeholder, half-wired features. Commit — only after personal
  verification. An agent's statement is a claim, not a receipt.

## Speed: short scenario by default

- **Tests and benchmarks must run as fast as possible.** Long runs slow down
  the cycle too much.
- **Benchmarks (criterion):** fast profile — `sample_size(10)` + short
  `warm_up_time`/`measurement_time` (the entire suite in a few seconds). Numbers
  are rough, but the relative order of containers is visible.
- **proptest:** modest number of cases by default (around 64) — this is a
  smoke-check for conformance, not exhaustive fuzzing.
- **miri:** run on specific invariant tests (`region_invariants`) and a tiny
  bounded proptest, not the full suite.
- **Heavy/exhaustive runs (large N, many cases, CPU-hours of fuzz,
  multi-arch) — that is Phase 5 hardening**, not the everyday cycle.

## Before every push: `npm run check`

- **Run `npm run check` before pushing, every time.** It runs the fast subset
  of what CI runs — `cargo fmt --check`, `clippy -D warnings` across all three
  CI feature-matrix entries (`""`, `--features experimental`, `--all-features`),
  `cargo test` under `production` and `production alloc-runfreelist`, then
  `npm run iai` (the deterministic judge) — and fails fast at the first red
  step (`scripts/check-all.mjs`). It does NOT replace CI (CI additionally runs
  miri, loom, TSan, multi-arch, no_std, MSRV) but it catches the most common
  drift class before a push, not after.
- **Why this rule exists:** a push in this session shipped 17 commits with a
  red CI (rustfmt drift accumulated across several phases, plus two CI
  workflow jobs still pointing at test files/features deleted by an earlier
  task) — discovered only by watching the Actions run *after* pushing.
  `npm run check` is the command that would have caught all of it first.
- **`npm run bench:table`** — the companion canonical wall-clock comparison
  table (SeferAlloc vs mimalloc vs System, fixed ns/op units, fixed bench
  set) for whenever comparative numbers are asked for. Exists because ad-hoc
  benchmark tables varied in units/subset/format run to run, once causing a
  spurious "20ns → 40ns regression" that was actually just µs-per-batch vs
  ns-per-op confusion.

## Active rules (from the plan/methodology)

- `#![forbid(unsafe_code)]` for the upper world; `unsafe` is allowed only in
  one documented module `hand` (phases 3b/4) behind a feature flag.
- Do not bump project or dependency versions without an explicit request.
- Verification-first: every invariant (I1–I6) is covered by proptest and/or
  unit test; the core is run under miri.
