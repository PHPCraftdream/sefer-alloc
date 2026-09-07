# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - Unreleased

First release. Everything below is new in this version; nothing has shipped
before it.

### Added

- **`drive<A: RawAllocator>(alloc: &A, config: Config, ops: &[Op])`** — the one
  differential-testing loop: replays an op stream against `alloc` and a
  trivial reference model (a `Vec` of live blocks), asserting the **M1-M4
  correctness oracles** on every step — M1 validity (non-null, aligned,
  fill-byte read-back), M2 double-free-is-a-no-op (opt-in via
  `Config::double_free`, since a real `malloc` treats this as UB), M3 no
  live-block byte overlap (checked on all three block-creating paths —
  `alloc`, `alloc_zeroed`, and `realloc` — and again at run end), M4
  alignment/size fidelity, plus the `alloc_zeroed` zeroed-contract and
  `realloc` `min(old, new)`-prefix-preservation checks. Total over every
  hand-built `Op` value: zero/oversized sizes are clamped into
  `GlobalAlloc`'s own contract, so the allocator is never invoked outside it.
  Panics — the natural oracle-failure signal for both proptest and libFuzzer
  — the moment any oracle is violated, with every failure message naming the
  op index and its operands. All survivors are freed and the model dropped
  before returning, so a passing run itself proves no UAF in the teardown
  walk too.
- **`unsafe trait RawAllocator`** — the minimal four-method `alloc` /
  `alloc_zeroed` / `dealloc` / `realloc` surface `drive` is generic over,
  with a blanket impl for every `GlobalAlloc`. A plain owned allocator with
  the same four methods (e.g. `AllocCore`) can implement it directly without
  going through the `GlobalAlloc` trait at all.
- **Two front-ends over the one model**, so an oracle improvement reaches
  both at once: `op_strategy()` (feature `proptest`) — a
  `Strategy<Value = Vec<Op>>` for `cargo test` and a bounded miri run — and
  `OpStream` (feature `arbitrary`) — an `impl Arbitrary` for `cargo fuzz` /
  libFuzzer, with fuzzer-derived sizes/aligns bounded so a single input
  cannot OOM the fuzzer instead of finding a bug. The DEFAULT bounds are
  the proptest-shaped distribution (sizes `1..=128 KiB`, small-heavy 9:1;
  aligns `2^0..=2^12`), so the budget lands on allocator state space rather
  than multi-megabyte byte fills; generated values never exceed
  `Config::large_max` or `min(2^21, Config::max_align)`.
  `OpStream::arbitrary_with_config` accepts a `Config` for front-ends that
  need non-default shaping — the in-tree `global_alloc_ops` fuzz target
  passes one that restores its historical 2 MiB size / 2^21 align reach.
  Both features are additive and independent; a normal build with neither
  enabled has **zero dependencies**.
- **`no_std` by default.** The core model (`drive`, `RawAllocator`, `Op`,
  `Config`) needs only `core` + `alloc`, so this crate can
  differential-test a `no_std` allocator's own test suite without pulling
  in `std`. Verified against a real bare-metal target
  (`thumbv7em-none-eabi`) for the default build AND for the `proptest`
  front-end (declared with `default-features = false, features = ["alloc",
  "no_std"]`, which routes proptest's float samplers through
  num-traits/libm). One consequence worth knowing: without `std`, proptest
  seeds its RNG from a hardcoded constant, so a consumer relying on the
  `proptest` feature alone gets deterministic seeding; this crate's own test
  suite dev-depends on a default-featured `proptest` so its runs are randomly
  seeded. The one exception is the `arbitrary` front-end:
  `derive_arbitrary`'s generated recursion guard for the crate's internal
  `RawOp` enum unconditionally references `std::thread_local!` — an
  upstream limitation, not this crate's own choice.
- **`Config`** — the size/align/double-free knobs shaping the generators:
  weighted small/large size arms (default 9:1, small ≤ 4 KiB, large ≤
  128 KiB — the historical in-tree shape this crate unifies), power-of-two
  aligns up to `max_align` (default 4096), and `double_free` (default
  `None`, since it is a *stronger-than-`GlobalAlloc`* guarantee that a real
  system `malloc` does not provide — enabled by an unforgeable `DoubleFreeOk`
  token, see Changed below).

### Changed (breaking relative to earlier drafts of this unreleased crate)

- **`Config::double_free` is now `Option<DoubleFreeOk>` (was `bool`).** The
  M2 double-free-is-no-op oracle is undefined behaviour against any ordinary
  `GlobalAlloc` (e.g. `System`), and `drive` is a safe function — a plain
  public `bool` let 100%-safe code write
  `Config { double_free: true, ..Config::default() }` and drive a real
  double-free into `System` through the blanket `GlobalAlloc` impl. The field
  now holds an unforgeable token whose only constructor is
  `const unsafe fn DoubleFreeOk::new()`, so enabling the oracle always
  crosses an explicit `unsafe` boundary in the allocator author's own code;
  `double_free: None` (the default) leaves it off (review run 7, P0-1).

### Notes

This crate unifies three copies of the same differential op-stream + M1-M4
oracle harness that had drifted independently in-tree
(`tests/alloc_core_differential.rs`, `tests/heap_differential.rs`,
`fuzz/fuzz_targets/global_alloc_ops.rs`) — all three are now thin consumers
supplying only a `RawAllocator` adapter and their historical `Config`. It is
the correctness twin of [`malloc-bench-rs`](https://crates.io/crates/malloc-bench-rs)
(the performance side).

A negative-oracle test suite exists in this crate's own `tests/`
(`tests/oracle_negative.rs`): a deliberately-broken allocator per oracle,
pinning each failure message as behaviour.

`Op`, `Config`, and `OpStream` are exhaustive by design with public fields;
adding a field or variant is a breaking change and bumps the minor version
under 0.x.

MSRV is 1.85, bounded by proptest 1.11's declared `rust-version` — a
deliberate decision rather than a copy of the workspace's 1.88 (review
P3-23).
