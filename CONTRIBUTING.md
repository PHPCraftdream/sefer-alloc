# Contributing to sefer-alloc

Thank you for your interest in contributing to `sefer-alloc`! This document
explains how this project is developed, what standards a contribution must meet,
and how to get started.

If you are new to the codebase, start with:

- [`README.md`](README.md) — feature overview and quick-start examples
- [`docs/DESIGN.md`](docs/DESIGN.md) — architectural decisions and data-layout
  rationale
- [`docs/INVARIANTS.md`](docs/INVARIANTS.md) — the formal invariants (I1–I7)
  that every PR must preserve
- [`docs/ALLOC_PLAN.md`](docs/ALLOC_PLAN.md) — byte-allocator roadmap

For security issues, do **not** open a public issue — see
[`SECURITY.md`](SECURITY.md) instead.


## Verification-first philosophy

`sefer-alloc` is not a typical utility crate. Its correctness guarantees extend
into the allocator tier where `unsafe` is unavoidable, memory-safety bugs have
no runtime safety net, and races corrupt the heap silently. Because of this, the
project holds every change to a higher verification bar than `cargo test` alone:

| Layer | Tool | When required |
|---|---|---|
| Unit + integration | `cargo test` | Always |
| Property-based | `proptest` via `alloc_core_differential` | Changes to core data structures |
| Sanitizers | ThreadSanitizer (`--features production`) | Any cross-thread path |
| Memory model | `loom` (`tests/loom_*.rs` — currently 14 files, e.g. `tests/loom_epoch.rs`, `tests/loom_xthread_protocol.rs`, `tests/loom_remote_ring.rs`) | New atomics or lock-free structures |
| Formal memory | `miri` | New `unsafe` blocks |
| Cross-arch | `aarch64-unknown-linux-gnu` (weak memory model) | Atomic/concurrent changes |
| Fuzzing | `cargo fuzz` targets in `fuzz/fuzz_targets/` (currently `region_ops.rs`, `global_alloc_ops.rs`, `heap_core_ops.rs`) | New allocator entry points |
| Valgrind | `valgrind --tool=memcheck` | `unsafe impl GlobalAlloc` paths |

A PR that skips a relevant layer without justification will not be merged.  This
is by design: a single hole in the verification net can manifest as a
use-after-free in production code that calls into this allocator.


## Before submitting a pull request

Work through the checklist below before opening or marking a PR ready for
review. All steps must pass locally; the CI will re-run them, but CI time is a
shared resource.

### Mandatory for every PR: `npm run check`

The single source of truth for the required pre-push commands is
`npm run check` (`scripts/check-all.mjs`) — **run it before every push**. It
runs, in order: `cargo fmt --check`, `clippy -D warnings` across every CI
clippy row (generated from `scripts/check-matrix.mjs`'s `PER_PR_ROWS` — not
hand-written, and kept byte-identical to `ci.yml`'s `clippy` job by
`tests/ci_clippy_matrix_consistency.rs`), `cargo test` across the main
feature combinations (`production internals`, `production alloc-stats
bench-internals internals`, `pinning`, `--all-features` — note there is no
bare `--features production` test step; `production` alone appears only as
a clippy row), then `npm run iai` (the deterministic judge). See that
script's own header comment for the full, current step list — it is
intentionally not duplicated here by hand, because a hand-duplicated list
is exactly the kind of second source that drifts out of sync (this file
itself used to be one such stale copy).

This repo's own white-box `tests/` suite reaches internal module paths
directly and requires the `internals` feature to compile
(`cargo test --features "production internals"`); `npm run check` already
covers this.

`npm run check` does **not** replace CI — CI additionally runs miri, loom,
ThreadSanitizer, multi-arch, `no_std`, and MSRV checks (see `.github/workflows/ci.yml`).
The layers below are the ones `npm run check` does not cover; run them
directly when your change touches the area each layer targets.

### Mandatory when touching core data structures

```sh
# Proptest differential against the reference implementation
cargo test --features alloc-core --test alloc_core_differential
```

### Mandatory when touching concurrent paths

```sh
# Loom model checking (may be slow — run with LOOM_MAX_PREEMPTIONS=2 for quick check).
# `RUSTFLAGS="--cfg loom"` is required — every tests/loom_*.rs file is
# `#![cfg(loom)]`; without it the binary builds empty and "passes" vacuously
# (0 tests run, exit 0) instead of failing loudly (found via
# docs/reviews/2026-08-06-sprint-closing-readonly-review.md finding S3).
# Pick the loom_*.rs test(s) relevant to your change — see `tests/loom_*.rs`
# for the current set (e.g. loom_epoch, loom_xthread_protocol, loom_remote_ring,
# loom_thread_free, loom_sharded — currently 14 files total).
RUSTFLAGS="--cfg loom" LOOM_MAX_PREEMPTIONS=2 cargo test --release --test loom_epoch --features experimental

# ThreadSanitizer (Linux or macOS only)
RUSTFLAGS="-Z sanitizer=thread" cargo +nightly test \
    --features production --target x86_64-unknown-linux-gnu
```

### Mandatory when adding or modifying `unsafe`

```sh
# Miri — run on invariant tests and a bounded proptest, not the full suite.
# `--test region_invariants` selects the BINARY (all 5 tests inside it run);
# a bare positional filter (`-- region_invariants`) is a substring match
# over TEST FUNCTION NAMES, none of which contain that substring, so it
# silently runs ZERO tests while still reporting green (see ci.yml's
# `miri-core` job comment for the exact same bug once found there).
cargo +nightly miri test --features alloc-core --test region_invariants
```

Cross-architecture build (weak memory model smoke-check):

```sh
cargo build --features production \
    --target aarch64-unknown-linux-gnu
```

### Recommended for allocator entry-point changes

```sh
# Fuzz targets (short run to check for immediate crashes). Current targets
# live in fuzz/fuzz_targets/: region_ops, global_alloc_ops, heap_core_ops.
cargo fuzz run heap_core_ops -- -max_total_time=60
```


## Code style and conventions

These conventions are enforced at review time and by CI.

### Module layout

- **One file, one export.** Each source file defines exactly one public item
  (type, trait, or function). The file name matches the export name.
- **`mod.rs` — re-exports only.** A `mod.rs` file contains only
  `mod` / `pub mod` / `pub use` declarations. No logic, no types, no tests.
- **Tests go in `tests/`.** Do not add `#[cfg(test)] mod tests { ... }` inside
  `src/*.rs`. Integration tests live under `tests/` mirroring the source
  structure; unit invariant tests live in `tests/` as well.
- **No doctests.** Do not add runnable rustdoc code examples (` ```rust `,
  ` ```compile_fail `, ` ```no_run `, bare ` ``` `, etc.) in `src/**/*.rs` doc
  comments — each one is compiled and run as its own separate test binary by
  `cargo test --doc`, and that per-example compile cost adds up fast across the
  crate. Illustrative code in a doc comment must use a non-executed fence
  (` ```text ` or plain prose) instead. Put the runnable version of the example
  as a real test in `tests/`. Existing doctests are pre-existing debt tracked
  for migration, not a precedent to extend.

### Safety boundaries

- The crate top-level carries `#![forbid(unsafe_code)]` in the default
  configuration and `#![deny(unsafe_code)]` with the `experimental` or
  `alloc-core` features enabled (see `src/lib.rs`'s top-level attributes).
- `unsafe` is permitted **only** inside named seam modules, split into two
  tiers — tier 1 (`#![allow(unsafe_code)]` at the module level: `unsafe` is
  permitted anywhere in the file) and tier 2 (`#[allow(unsafe_code)]` on an
  individual `unsafe fn`/`unsafe {}` site in an otherwise-safe file). The
  full, current inventory is **not** hand-duplicated here — it changes as
  the crate grows and a hand-copied list is exactly the kind of second
  source that goes stale (this file's old two-module claim, including two
  files that do not exist in this tree, was itself an instance of that).
  Get the authoritative, current list from either:
  - README.md's [Where unsafe lives — the complete
    list](README.md#where-unsafe-lives-the-complete-list) section, or
  - the self-verifying grep both this repo's `CLAUDE.md` and README use:
    `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`
- Every `unsafe` block must carry a `// SAFETY:` comment that names the
  invariants being upheld. A block with no `// SAFETY:` comment will be
  rejected.

Example:

```rust
// SAFETY: `ptr` was allocated by this allocator with `layout`,
//         and `I3-exclusive-ownership` guarantees no alias exists.
unsafe { dealloc(ptr, layout); }
```

### Feature flags

New capabilities must ship behind a feature flag that is **off by default**.
Document the flag in `Cargo.toml` with a one-line description, add it to the
feature matrix table in `README.md`, and gate it with `#[cfg(feature = "...")]`
(not env vars or build.rs).

Add benchmarks (`benches/`) for any performance-sensitive new path.

### Formatting

```sh
cargo fmt --all
```

The repository uses the default `rustfmt` settings. Unformatted code fails CI.


## Commit message format

Follow the Conventional Commits style used throughout this repository:

```
type(scope): short imperative summary

Optional body — explain the *why*, not the *what*.
Wrap at 72 characters.
```

Common types: `feat`, `fix`, `perf`, `refactor`, `test`, `bench`, `docs`, `ci`,
`chore`.

Common scopes: `core`, `concurrent`, `fuzz`, `bench`, `ci`, `docs`.

Examples from the project history:

```
feat(concurrent): epoch reclaim — drain stale slots on grace period
fix(core): off-by-one in segment boundary check (closes #42)
bench(core): add larson workload to macro-benchmark suite
```

`perf`-prefixed commits additionally follow a `perf(runtime)` /
`perf(opt-in)` / `bench` / `docs(config)` / `fix(perf)` taxonomy — see
`CLAUDE.md`'s "Active rules" section (search for "commit subject line's
conventional-commit prefix") for the current, canonical definition of each.

Breaking changes must include `!` after the scope (`feat(core)!:`) and a
`BREAKING CHANGE:` footer.


## How to add a new feature

1. Open an issue describing the use-case before writing code, unless the feature
   is trivially small.
2. Add a feature flag (default-off) to `Cargo.toml`.
3. Implement behind the flag.
4. Add tests in `tests/` — at minimum a unit test and, if applicable, a
   proptest.
5. Add a benchmark in `benches/` if the feature is performance-sensitive.
6. Update `README.md` (feature matrix table) and `CHANGELOG.md` (under
   `[0.3.0] (unreleased)`).
7. Run the full checklist above.


## Reporting a security vulnerability

See [`SECURITY.md`](SECURITY.md). Do **not** open a public GitHub issue for
security vulnerabilities.


## License

By submitting a pull request to this repository you agree that your contribution
is licensed under the terms of **MIT OR Apache-2.0**, the same dual license
covering the rest of the project.

If you are contributing on behalf of an employer, ensure you have the necessary
rights to submit the work under these terms.
