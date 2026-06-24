# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial scaffold of the `sefer-alloc` crate.
- Single-threaded dense generational `Region<T>` with `insert` / `get` /
  `get_mut` / `remove` / `contains` / `iter` / `clear`, all `O(1)`, built under
  `#![forbid(unsafe_code)]`.
- Typed, copyable `Handle<T>` (index + generation) with hand-written
  `Copy`/`Eq`/`Hash`/`Debug` impls that hold for every `T`.
- Safety invariants I1–I5 documented (`docs/INVARIANTS.md`) and encoded as
  unit tests plus a proptest differential harness against a reference model
  (`tests/differential.rs`).
- Implementation plan with verification gates (`docs/PLAN.md`) and architecture
  notes (`docs/DESIGN.md`).
- Dual MIT / Apache-2.0 licensing; MSRV pinned to 1.88.
