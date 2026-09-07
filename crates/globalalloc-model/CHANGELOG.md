# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - Unreleased

First release. Everything below is new in this version; nothing has shipped
before it.

### Added

- **`drive<A: RawAllocator>(alloc: &A, config: Config, ops: &[Op])`** — the one
  differential-testing loop: replays an op stream against `alloc` and a
  trivial reference model (a `Vec` of live blocks). It checks the **M1-M4
  correctness oracles at their observation points**: non-null allocation
  results and fill read-back; the explicitly authorized immediate double-free
  no-op; live-extent overlap on block-creating returns; alignment before
  access; zero bytes from `alloc_zeroed`; and the initialized
  `min(old, new)` realloc prefix. Live fills are checked before deallocation,
  reallocation, and each teardown deallocation. Zero and oversized sizes in
  hand-built operations are clamped into `GlobalAlloc`'s size preconditions;
  an inadmissible alignment (including zero or non-power-of-two) is rejected
  instead. A null `alloc` or `alloc_zeroed` is an oracle failure, while a null
  `realloc` is accepted and leaves the old block live. On normal return all
  survivors are freed and the model is dropped.
- **`unsafe trait RawAllocator`** — the minimal four-method `alloc` /
  `alloc_zeroed` / `dealloc` / `realloc` surface `drive` is generic over,
  with a blanket impl for every `GlobalAlloc`. A plain owned allocator with
  the same four methods (e.g. `AllocCore`) can implement it directly without
  going through the `GlobalAlloc` trait at all.
- **Two front-ends over the one model**, so an oracle improvement reaches
  both at once: `op_strategy()` (feature `proptest`) — a
  `Strategy<Value = Vec<Op>>` for `cargo test` and a bounded miri run — and
  `OpStream` (feature `arbitrary`) — an `impl Arbitrary` for `cargo fuzz` /
  libFuzzer, with fuzzer-derived sizes and alignments bounded to keep an
  individual request practical. Bounds cannot guarantee the absence of OOM:
  live requests can accumulate and allocators may impose tighter limits. The
  DEFAULT bounds are
  the proptest-shaped distribution (sizes `1..=128 KiB`, small-heavy 9:1;
  aligns `2^0..=2^12`), so the budget lands on allocator state space rather
  than multi-megabyte byte fills. In non-degenerate configurations, generated
  values do not exceed `Config::large_max` or
  `min(2^21, Config::max_align)`. With `small_max == 0`, the small arm is size
  1; with `large_max <= small_max`, the large arm is `small_max + 1` and may
  therefore exceed `large_max`.
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
- **Dedicated publication CI** — tests the default build, each front-end
  feature alone, and all features together on Windows and Linux (the
  front-end powerset; the internal `internals` feature is exercised by the
  all-features rows rather than its own matrix entry), executes the portable
  System/front-end tests with all features on 32-bit i686, checks the library
  on the exact declared Rust 1.85 MSRV, validates docs.rs-style nightly
  documentation with BOTH the exact `[package.metadata.docs.rs]` feature list
  (`proptest`, `arbitrary`) and the all-features configuration, and
  tests/lints/documents the extracted package plus a standalone path consumer.
  The repository-wide workflow remains the home of the existing bare-metal and
  Miri coverage.
- **`Config`** — the size/align/double-free knobs shaping the generators:
  weighted small/large size arms (default 9:1, small ≤ 4 KiB, large ≤
  128 KiB — the historical in-tree shape this crate unifies), power-of-two
  aligns up to `max_align` (default 4096), and `double_free` (default
  `None`, since it is a *stronger-than-`GlobalAlloc`* guarantee that a real
  system `malloc` does not provide — enabled by an unforgeable `DoubleFreeOk`
  token, see Changed below).

### Changed (breaking relative to earlier drafts of this unreleased crate)

- Arbitrary decoding uses a full-width weight sum and separate size draws;
  large weights and size ranges remain reachable. Existing fuzz bytes can
  decode differently. Zero-weight arms are also excluded during proptest
  shrinking.
- Fill checks before deallocation, reallocation and each teardown free detect
  corruption that a later destructive operation could previously hide.

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

The `proptest` front-end's size generator draws from a hand-rolled enum
`Strategy`/`ValueTree` (not `BoxedStrategy`): `TupleUnion` (the weighted-arm
case) is already non-boxing, so the only heap allocation the old `.boxed()`
form added was one `Box<dyn ValueTree>` per drawn `Alloc`/`AllocZeroed`/
`Realloc` size. Measured (`examples/perf_probe_p4_measurements.rs`): ~1.585
allocations per generated op with the enum, versus ~2.336 with `.boxed()`,
each drawn 200-op stream costing roughly one avoidable allocation per
size-bearing op under the old form.

Oracle checks are observations rather than continuous monitoring. Transient
corruption restored between checks is invisible. Each block's byte 0 carries
its fill identifier verbatim and its later bytes a mixed (non-additive)
function of the identifier and the offset — not a simple additive scheme, so
distinct identifiers' patterns are not fixed shifts of one another, and a
wrong-source copy from a live block carrying a different identifier is
caught at offset 0 regardless of prefix length. Collisions remain possible
in general (the driver's internal `pattern_byte` documentation lists the
exact residual limits), and fill identifiers cycle after 255 assignments, so
two live blocks can still share a marker. More fundamentally,
an invalid extent or genuinely uninitialized byte violates `RawAllocator`'s
safety contract: accessing it is undefined behavior natively and under Miri,
not a reliably reportable oracle failure. If an oracle panics, allocations may
be leaked deliberately because no generic cleanup can safely deallocate a set
that may contain invalid or overlapping pointers.

`Op`, `Config`, and `OpStream` are exhaustive by design with public fields;
adding a field or variant is a breaking change and bumps the minor version
under 0.x.

MSRV is 1.85, bounded by proptest 1.11's declared `rust-version` — a
deliberate decision rather than a copy of the workspace's 1.88 (review
P3-23).
