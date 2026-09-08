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
`Realloc` size.

Measured by a paired A/B (`examples/perf_probe_p4_measurements.rs`: identical
generator shape, same fixed seeds, `failure_persistence: None` pinned
explicitly — the earlier ~1.585 vs ~2.336 calibration inherited proptest's
env-var-dependent failure-persistence default, ~1.64 allocs/op of clone
overhead that masked the real gap; the regression guard
(`tests/size_strategy_avoids_boxed_value_trees.rs`) pins
`failure_persistence: None` with its threshold re-derived to 0.35 from that
paired run, enum 0.015, boxed 0.766): the fresh run after the probe's
realloc-honest corrections measured 0.000 vs 0.751 (default) / 0.750 (single)
boxing-attributable allocations per generated op (~150 per 200-op stream) —
small-count run-to-run drift on the enum arm; the ~+0.75 paired gap is the
durable signal, and the 0.35 threshold stays valid.

Memory axis, honestly: the inline representation is NOT transient — trees
live in the stream's `Vec` for its whole life (including shrinking);
`SizeValueTree` is 1152 B vs the box slot it replaced (2 words, i.e.
`2 x size_of::<usize>()` — 16 B on this 64-bit target), the per-op element
tree 4240 B, `SizeStrategy` 40 B, the `Single` arm's tree 24 B — all
re-confirmed by the corrected run and specific to this target/toolchain
(x86_64-windows, proptest 1.11.0), not portable contracts of the types. The
corrected probe (review findings P3-1/P3-2/P4-3: realloc-honest byte
counters; three separately-labeled scenarios instead of one mislabeled
"draw"; every measurement window reports its realloc count; review
rounds 6-7 follow-ups P3-1/P4-4/P4-3: S3 now runs proptest's REAL
`TestRunner::run_one` protocol under an explicitly CUSTOM
65,536-iteration budget, S1 gained a `current()` timing column whose
disjoint columns sum to the whole case, and the dependency-version line
is labeled a runtime lockfile snapshot, not build identity) reports, per
200-op stream draw, totals/peaks in heap bytes for enum vs boxed. Default
`Config`: S1 (successful draw, `new_tree` -> `current` -> drop, no
shrinking, n=64) 860,316/854,268 vs 639,120/633,072 — boxed SMALLER by
221,196 B on both axes; S2 (simplify-only walk, n=64) 848,124/848,124 vs
915,594/915,594 — enum smaller by 67,470 B on both axes, 0.0 reallocs per
window; S3 (REAL `TestRunner::run_one` shrink protocol — proptest's own
accept/reject/complicate walk — under an explicitly CUSTOM
65,536-iteration budget, NOT the default runner's resolved cases*4 =
1024; n=8, 15,547.0 evals/draw, 1.5 complicate attempts, 0 seeds
cap-truncated) 190,409,340/854,268 vs 190,471,788/916,716 — enum smaller
by 62,448 B on both axes (S3 totals are dominated by ~15.5k per-eval
`Vec<Op>` materializations; 93,288.0 reallocs/window). Single `Config`
(large_weight 0): S1 860,316/854,268 vs
470,307/464,259 (boxed smaller by 390,009 B); S2 848,124/848,124 vs
464,204/464,204 (enum smaller by 383,920 B, 0.0 reallocs); S3 (n=8,
15,886.6 evals/draw, 1.6 complicate attempts) 194,550,048/854,268 vs
194,166,234/470,454 (boxed smaller by 383,814 B, 95,325.8
reallocs/window). The previously published figures
(848,124 vs 915,594 default; 848,124 vs 464,204 single) reproduce EXACTLY
as S2 totals under the corrected counter, and S2 windows contain 0.0
reallocs — so the old counter bug (which only fired on realloc) did not
numerically affect them; what was wrong was the label: those numbers are
construction PLUS a full simplify-only walk, not a plain draw. The regime
ordering therefore flips: at a plain successful draw the boxed form uses
LESS memory at BOTH configs, because proptest's `TupleUnion` only
materializes the boxed arms' extra size-Boxes while simplifying; the
enum's inline stride is paid from construction — "the enum uses less
memory" is true only in the shrink/simplify regimes. The enum's total/peak
bytes are byte-identical across default and single configs in every
scenario (860,316/854,268 in S1, 848,124 in S2, 854,268 peak in S3) — the
`Weighted` slot dominates regardless of config — while the boxed
counterpart varies by config. Kept: bounded per drawn case, wins
allocations at every config, and boxing only the weighted arm would
reintroduce the per-draw allocation on the default path. The corrected
counters also separate attempted calls from successful ranges (a
successful realloc replaces the old size with the new one in live state,
TOTAL_BYTES counts the full new request on realloc, null calls leave live
unchanged), so every memory claim above is per-scenario and
realloc-audited.

Oracle checks are observations rather than continuous monitoring. Transient
corruption restored between checks is invisible. Each block's byte 0 carries
its fill identifier verbatim and its later bytes a mixed (non-additive)
function of the identifier and the offset — not a simple additive scheme, so
distinct identifiers' patterns are not fixed shifts of one another, and an
unshifted copy taken from the start of a live block carrying a different
identifier is caught at offset 0 regardless of prefix length. A shifted
copy is not covered by that guarantee (a shifted one-byte copy can still
coincide with the destination's marker byte), and the pattern's 2^32 period
on 64-bit platforms belongs to the positive-offset hash tail, not to the
sequence including the special-cased offset 0. Collisions remain possible
in general (the crate documentation's `Safety and oracle limits` section
lists the exact residual limits and counterexamples), and fill identifiers
cycle after 255 assignments, so two live blocks can still share a marker.
More fundamentally,
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
