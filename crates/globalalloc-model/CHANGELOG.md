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
  live-block byte overlap (checked incrementally and again at run end), M4
  alignment/size fidelity, plus the `alloc_zeroed` zeroed-contract and
  `realloc` `min(old, new)`-prefix-preservation checks. Panics — the natural
  oracle-failure signal for both proptest and libFuzzer — the moment any
  oracle is violated. All survivors are freed and the model dropped before
  returning, so a passing run itself proves no UAF in the teardown walk too.
- **`unsafe trait RawAllocator`** — the minimal four-method `alloc` /
  `alloc_zeroed` / `dealloc` / `realloc` surface `drive` is generic over,
  with a blanket impl for every `GlobalAlloc`. A plain owned allocator with
  the same four methods (e.g. `AllocCore`) can implement it directly without
  going through the `GlobalAlloc` trait at all.
- **Two front-ends over the one model**, so an oracle improvement reaches
  both at once: `op_strategy()` (feature `proptest`) — a
  `Strategy<Value = Vec<Op>>` for `cargo test` and a bounded miri run — and
  `OpStream` (feature `arbitrary`) — an `impl Arbitrary` for `cargo fuzz` /
  libFuzzer, with fuzzer-derived sizes/aligns bounded (`1..=2 MiB` size,
  `2^0..2^21` align) so a single input cannot OOM the fuzzer instead of
  finding a bug. Both features are additive and independent; a normal build
  with neither enabled has **zero non-dev dependencies**.
- **`no_std` by default.** The core model (`drive`, `RawAllocator`, `Op`,
  `Config`) and the `proptest` front-end need only `core` + `alloc` (a
  `Vec<Live>`/`Vec<Op>` to track live blocks and op streams), so this crate
  can differential-test a `no_std` allocator's own test suite without
  pulling in `std`. Verified against a real bare-metal target
  (`thumbv7em-none-eabi`) for the default build. The one exception: enabling
  the `arbitrary` feature pulls `std` back in, because `derive_arbitrary`'s
  generated recursion guard for the crate's internal `RawOp` enum
  unconditionally references `std::thread_local!` — an upstream limitation,
  not this crate's own choice.
- **`Config`** — the size/align/double-free knobs shaping the generators:
  weighted small/large size arms (default 9:1, small ≤ 4 KiB, large ≤
  128 KiB — the historical in-tree shape this crate unifies), power-of-two
  aligns up to `max_align` (default 4096), and `double_free` (default
  `false`, since it is a *stronger-than-`GlobalAlloc`* guarantee that a real
  system `malloc` does not provide).
- **`ranges_overlap(a, asize, b, bsize) -> bool`** — the M3 overlap
  predicate, exposed as a standalone `#[must_use]` function since it has no
  dependency on the rest of the model.

### Notes

This crate unifies three copies of the same differential op-stream + M1-M4
oracle harness that had drifted independently in-tree
(`tests/alloc_core_differential.rs`, `tests/heap_differential.rs`,
`fuzz/fuzz_targets/global_alloc_ops.rs`) — all three are now thin consumers
supplying only a `RawAllocator` adapter and their historical `Config`. It is
the correctness twin of [`malloc-bench-rs`](https://crates.io/crates/malloc-bench-rs)
(the performance side): nothing else on crates.io offers a ready
"differential-test your `GlobalAlloc` against a model with
UAF/overlap/zeroed/realloc oracles" kit.
