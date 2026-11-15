# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - 2026-11-15

First release. Extracted from `sefer-alloc`'s `src/ghost` module verbatim
(plan step P1.1 of docs/plans/2026-11-15-workspace-restructure-plan.md).

### Added

- **`ghost_assert!` / `ghost_event!`** — the checking-build-only assertion and
  event-counter macros, now `pub` at `sefer_ghost::ghost` (previously
  `pub(crate)` inside sefer-alloc's module tree). In loom/miri/`--cfg
  sefer_ghost` builds they panic through `fail` / record into the shadow
  table; under kani the assertion is a `kani::assert` proof obligation; in
  production both expand to nothing.
- **Shadow event table** — `count`, `fail`, `overflowed`, `record`, `reset`:
  a fixed open-addressing map `(rule, key) -> i64` on `core` atomics, no
  allocation, invisible to loom's model.
- **`probe`** — self-test entry gated behind the `internals` feature.

### MSRV

- Rust 1.88.
