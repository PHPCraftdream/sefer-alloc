# sefer-alloc — project conventions

Core instructions, mandatory for all code in this repository. They
**override** default behavior.

## File and module structure

- **One file — one export.** Each source file defines exactly one public item
  (type, trait, function). The file name matches the export. This rule is about
  *one responsibility per file*, not a literal count of `pub` tokens; the
  following categories are sanctioned exceptions (they keep a single focused
  responsibility even though the file exposes more than one public item):
  1. **doc-hidden test-only forwarders** — items that are `pub` solely because
     their enclosing module is `#[doc(hidden)]`, exposing a test hook so an
     integration test in `tests/` can reach an otherwise-internal surface (the
     established "test-only export pattern"; see the `#[doc(hidden)]` notes in
     `src/lib.rs`, `src/alloc_core/mod.rs`, `src/registry/mod.rs`,
     `src/registry/tagged_ptr.rs`). These are not stable public API.
  2. **protocol-constant clusters attached to their one primary type** — a set
     of `pub` protocol constants that belong to a single owning type and live
     with it (e.g. `RemoteFreeRing` with its `RING_CAP` / `DBG_RING_OVERFLOW`
     constants). The constants are that type's protocol, not independent
     exports in the sense of the rule.
  3. **single-file seam crates in `crates/`** — for a crate that is one file
     (e.g. `crates/vmem/src/lib.rs`, `crates/numa/src/lib.rs`,
     `crates/malloc-bench/src/lib.rs`), "the whole crate is one module"; it
     publishing several public items is normal, because the crate as a whole is
     the single focused library — that is its one responsibility.
  4. **`#[cfg(kani)]` proof harnesses in `src/`** (e.g. `src/kani_proofs.rs`) —
     Kani proof harnesses need `pub(crate)` internals (e.g.
     `crate::alloc_core::node::Node`, `crate::concurrent::hand::AtomicSlot`)
     that are invisible from `tests/` (integration tests see only `pub`), so
     they legitimately live in `src/` behind `#[cfg(kani)]` rather than in the
     `tests/` tree.
- **`mod.rs` — reexports only, no code.** The `mod.rs` file contains
  exclusively `mod`/`pub mod`/`pub use` declarations. No logic, types,
  functions, or tests belong in `mod.rs` — it only wires modules together.

## Tests

- **Put tests in a separate folder from the start.** Do not leave tests inline
  in the module file (`#[cfg(test)] mod tests` inside `src/*.rs`). Tests live in
  `tests/` (integration) with a mirrored structure; new code is written with
  tests in separate files from the very beginning, not extracted later.
- **No doctests.** Never add a runnable rustdoc code example (` ```rust `,
  ` ```compile_fail `, ` ```no_run `, or a bare ` ``` ` fence) to a doc comment
  in `src/**/*.rs` — `cargo test --doc` compiles and runs every one of them as
  its own separate test binary, and that per-example compile overhead is too
  slow across a crate this size. Illustrative snippets in doc comments must use
  a non-executed fence (` ```text `) or plain prose; the runnable version of the
  example belongs in `tests/` as a real test. Pre-existing doctests are tracked
  debt for migration (see `docs/reviews/2026-07-12-round2-remediation-plan.md`),
  not a precedent for adding more.

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
- **After each wave, if the `production` feature composition changed:**
  re-run `npm run bench:table` + `npm run iai` and commit the updated
  README.md / `docs/perf/IAI_BASELINE.md` numbers in the same PR — do not
  defer the canonical-table refresh to a later round (R13-10/task #280: the
  README wall-clock table had gone stale across two consecutive rounds of
  `production` changes before this rule existed). Cite the raw logs the
  refresh was measured from (see the raw-log policy below).
- **A wall-clock gate must report both the sub-window metric and the
  full-round criterion time for the same harness** — not the sub-window
  figure alone. If a harness times an internal region narrower than the
  whole benchmarked round (e.g. skipping setup/teardown inside the timed
  iteration), the report must also cite criterion's own full-round mean for
  that same run, and any material gap between the two axes is itself a
  result requiring explanation, not a detail to omit (R14-3/task #288: the
  `class-aware-dirty` gate's "21.71×" headline was a sub-window figure whose
  full-round improvement was actually ~11%, discovered only because three
  independent reviews asked for the missing axis).
- **Raw perf logs (`docs/perf/_raw_*.log`) are scratch by default** —
  `.gitignore`d (R13-10/task #280), never committed just because a `cargo
  bench`/`npm run iai` invocation happened to write one. The one exception:
  when a perf-gate report (`docs/perf/*.md`) cites specific `_raw_*.log`
  filenames as its evidence, `git add -f` those named files alongside the
  report so the citation is reproducible from the commit, not just from a
  re-run — see `docs/perf/R13_6_EXACT_SPAN_RESERVED_CAPACITY_PRODUCTION_GATE.md`
  and `docs/perf/R13_9_CLASS_AWARE_DIRTY_PRODUCTION_GATE.md` for the
  established pattern.

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
  `cargo test` under `production`, then
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
  named seam modules that lift it with `#![allow(unsafe_code)]`, each with a
  single documented reason to hold `unsafe`. The seams are inventoried in
  README §"Where unsafe lives — the complete list" and mirrored in the
  `src/lib.rs` header. There are two tiers of confined `unsafe`, both captured
  by a single self-verifying command (never a hardcoded count):
  `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`
  — **tier 1** is the `#![...]` (module-level) matches: named seam modules
  where `unsafe` is permitted anywhere inside the file; **tier 2** is the
  `#[...]` (item-level) matches: individual `unsafe fn` declarations (and the
  scoped `unsafe {}` blocks at their internal call sites) in files that are
  otherwise safe code, each carrying its own `# Safety` doc and per-site
  `// SAFETY:` comment. Both tiers are comment-proof: `^\s*#!?\[` requires the
  line to begin with optional whitespace then the attribute, so `//` comments
  that merely mention the attribute do not match (the unanchored
  `grep -rln 'allow(unsafe_code)' ...` form has false positives, e.g. in
  `src/lib.rs` and `src/registry/heap_overflow.rs`). Any formal audit
  compares against this command's output, and an `unsafe` token not covered by
  a tier-1 module or a tier-2 item-level allow is a hard compile error in every
  feature configuration. The sanctioned exception categories (doc-hidden
  test-only forwarders, protocol-constant clusters, single-file seam crates,
  kani proofs — listed in the "File and module structure" section above) apply
  to tier 1; tier 2 has its own rule: a single documented reason to hold
  `unsafe` applies to each item-scoped site individually, not just to seam
  modules.
- Do not bump project or dependency versions without an explicit request.
- Verification-first: every invariant (I1–I6) is covered by proptest and/or
  unit test; the core is run under miri.
