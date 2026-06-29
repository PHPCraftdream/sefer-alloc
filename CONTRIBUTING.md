# Contributing to sefer-alloc

Thank you for your interest in contributing to `sefer-alloc`! This document
explains how this project is developed, what standards a contribution must meet,
and how to get started.

If you are new to the codebase, start with:

- [`README.md`](README.md) — feature overview and quick-start examples
- [`docs/DESIGN.md`](docs/DESIGN.md) — architectural decisions and data-layout
  rationale
- [`docs/INVARIANTS.md`](docs/INVARIANTS.md) — the formal invariants (I1–I6)
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
| Memory model | `loom` (`tests/loom_*.rs`) | New atomics or lock-free structures |
| Formal memory | `miri` | New `unsafe` blocks |
| Cross-arch | `aarch64-unknown-linux-gnu` (weak memory model) | Atomic/concurrent changes |
| Fuzzing | `cargo fuzz` targets in `fuzz/` | New allocator entry points |
| Valgrind | `valgrind --tool=memcheck` | `unsafe impl GlobalAlloc` paths |

A PR that skips a relevant layer without justification will not be merged.  This
is by design: a single hole in the verification net can manifest as a
use-after-free in production code that calls into this allocator.


## Before submitting a pull request

Work through the checklist below before opening or marking a PR ready for
review. All steps must pass locally; the CI will re-run them, but CI time is a
shared resource.

### Mandatory for every PR

```sh
# Full feature matrix — must be green
cargo test --features production

# No warnings anywhere
cargo build --all-targets --all-features
cargo clippy --features production -- -D warnings
```

### Mandatory when touching core data structures

```sh
# Proptest differential against the reference implementation
cargo test --features alloc-core --test alloc_core_differential
```

### Mandatory when touching concurrent paths

```sh
# Loom model checking (may be slow — run with LOOM_MAX_PREEMPTIONS=2 for quick check)
LOOM_MAX_PREEMPTIONS=2 cargo test --test loom_epoch --features experimental
LOOM_MAX_PREEMPTIONS=2 cargo test --test loom_reclaim --features experimental

# ThreadSanitizer (Linux or macOS only)
RUSTFLAGS="-Z sanitizer=thread" cargo +nightly test \
    --features production --target x86_64-unknown-linux-gnu
```

### Mandatory when adding or modifying `unsafe`

```sh
# Miri — run on invariant tests and a bounded proptest, not the full suite
cargo +nightly miri test --features alloc-core -- region_invariants
```

Cross-architecture build (weak memory model smoke-check):

```sh
cargo build --features production \
    --target aarch64-unknown-linux-gnu
```

### Recommended for allocator entry-point changes

```sh
# Fuzz targets (short run to check for immediate crashes)
cargo fuzz run fuzz_alloc_dealloc -- -max_total_time=60
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

### Safety boundaries

- The crate top-level carries `#![forbid(unsafe_code)]` in the default
  configuration and `#![deny(unsafe_code)]` with `experimental` or `byte`
  features enabled.
- `unsafe` is permitted **only** inside these two modules:
  - `src/concurrent/hand.rs` (gated on `experimental`)
  - `src/byte/byte_region.rs` and `src/byte/byte_allocator.rs` (gated on
    `byte`)
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

Common scopes: `core`, `concurrent`, `byte`, `fuzz`, `bench`, `ci`, `docs`.

Examples from the project history:

```
feat(concurrent): epoch reclaim — drain stale slots on grace period
fix(byte): off-by-one in segment boundary check (closes #42)
bench(core): add larson workload to macro-benchmark suite
```

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
   `[Unreleased]`).
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
