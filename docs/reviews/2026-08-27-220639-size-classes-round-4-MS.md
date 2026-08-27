# size-classes: pre-publication review, round 4 (MS)

## Verdict

**NO-GO for publication from the checked HEAD.** I found no P0 memory-safety issue and no demonstrated arithmetic misclassification for valid inputs, but the release artifact is not in a publishable state: `crates/size-classes/CHANGELOG.md` still marks `0.1.0` as `Unreleased`, and the repository's own non-dry-run release guard rejects exactly that state. Before the first release, the unchecked-alignment public API should also receive an explicit maintainer decision because publication freezes a deliberately release-dependent failure contract into the easy-to-discover method name.

Checked HEAD: `6ba9eabfc649c1e7b462d2e29044b5a5339a8460` (`2026-08-27T21:15:21+02:00`).

Review mode: read-only, single reviewer, no sub-agents. Per request, I ran **no tests, builds, `cargo check`, clippy, rustdoc, benchmarks, package/publish, or any other Cargo command**. Conclusions are static and do not assert that HEAD currently passes CI. I did not open or use reports, checkpoints, or review history under `docs/reviews`; references to earlier work that are embedded in current source comments were treated only as current code text, never as evidence.

## Scope

Reviewed from scratch:

- all production code and public items in `crates/size-classes/src/lib.rs`;
- crate metadata, version/MSRV, dependency declarations, license files, README and changelog;
- all crate-local integration/property tests, shared fixtures, panic oracles, and the benchmark target;
- integer arithmetic, overflow behavior, const/runtime parity, 32/64-bit boundaries, table/LUT invariants, alignment contracts, termination, and index bounds;
- workspace wiring, lockfile entries, release workflow, package/MSRV/no_std/i686/clippy/rustdoc/deny CI coverage;
- the real in-tree consumer (`sefer-alloc`): versioned path dependency, compatibility shim, public `SegmentLayout` forwarding API, and allocator call sites derived from `Layout`.

Async, concurrency, atomics, FFI, raw pointers, cryptography, I/O/resource lifecycle, serialization, and `Drop` hazards are not present in this crate's production code. Production is `#![no_std]`, dependency-free, and `#![forbid(unsafe_code)]`.

## Findings by priority

### P0 — none

No unsafe code, FFI, concurrency, secret handling, or externally mutable resource lifecycle exists here. For valid builder parameters and power-of-two alignment, the static arithmetic trace did not expose an out-of-bounds access, non-termination, overflow wrap, or wrong-class result.

### P1-1 — the checked release state is intentionally rejected by the release workflow

Evidence:

- `crates/size-classes/CHANGELOG.md:7` says `## 0.1.0 - Unreleased`.
- `.github/workflows/release.yml:215-308` requires exactly one version section and rejects one still marked `unreleased` for every real (non-dry-run) publish.

Impact: a publication attempt through the supported workflow cannot pass its own gate. Bypassing that gate would publish unstamped release notes and discard a deliberate repository safeguard.

Required action: replace `Unreleased` with the actual release date only when the release commit/tag is ready, then require green CI for that exact commit. This is the immediate NO-GO reason.

### P2-1 — `class_for` is the attractive API but has release-dependent behavior on an easy-to-supply argument

Evidence:

- `crates/size-classes/src/lib.rs:766-791` documents power-of-two `align` as a precondition and says violation is unspecified.
- `crates/size-classes/src/lib.rs:817-828` enforces it only with `debug_assert!`; release can return a class that fails the documented divisibility predicate, return a wrong `None`, or panic for `(0, 0)`.
- The checked API exists at `crates/size-classes/src/lib.rs:872-905`, but the shorter/default-looking name remains the unchecked one.
- The real consumer propagates this shape through public `SegmentLayout::class_for(size, align)` at `src/alloc_core/segment_layout.rs:77-94`; its allocator-internal callers are safe because their alignments come from `Layout`, but external callers of the public forwarding helper can pass arbitrary integers.

Impact: this is not UB inside `size-classes`, and the documentation is unusually explicit. Nevertheless, an allocator author can accidentally turn the wrong result into a misaligned allocation, where the downstream effect is safety-critical. Debug and release disagree, and the most discoverable API accepts raw `usize` rather than a validity-carrying type.

Recommendation before 0.1.0: make a deliberate API decision now. Strong options are (a) make `class_for` checked and give the trusted hot path an explicitly named method/type, or (b) accept `Layout`/a validated alignment newtype for the hot path. If the current split is retained, record it as an accepted first-release contract and add the checked forwarding API to `SegmentLayout`; otherwise users of the real consumer have no equivalent to `try_class_for`.

### P2-2 — standalone `build_size2class` accepts structurally unusable classes

Evidence: `crates/size-classes/src/lib.rs:430-441` explicitly permits a hand-built strictly increasing table whose entries are not multiples of `min_block`; the documented example `[16, 24, 32]` with `min_block = 16` makes class `24` permanently unreachable through the generated LUT. Validation at `lib.rs:458-500` checks non-empty, power-of-two bucket width, class-count capacity, monotonicity, and `L`, but not minimum-block divisibility or a non-zero/minimum first entry.

Impact: this is documented, so it is not an implementation/contract mismatch. It is still a public-API footgun: a builder can succeed while silently producing a lookup that cannot select every supplied class. The `SizeClasses::build` path is unaffected because `build_table` enforces divisibility.

Recommendation: either reject entries that the bucket representation cannot faithfully expose (preferable for a general-purpose builder), or rename/scope the function so its quantized-upper-bucket semantics are unmistakable. Add a checked constructor/error path if runtime-supplied tables are a future goal.

### P3-1 — property tests generate requests, not builders

`tests/proptest_builder.rs` samples `(size, align)` over three fixed schemes with 64 cases each. It does not generate `min_block`, growth ratios, geometric lengths, or valid/disjoint extras, despite the builder being the numerically difficult component. `tests/builder.rs` is strong on named overflow boundaries and has one hand-derived golden progression, but its main reference builder intentionally mirrors the production formula closely.

Impact: correlated mistakes in merge/rounding logic outside the one golden scheme can evade the present oracle. The extensive hand tests substantially reduce risk, so this is hardening rather than a demonstrated bug.

Recommendation: property-generate small valid parameter sets and compare against an independently expressed arbitrary-precision/simple reference; separately generate one-invalid-precondition-at-a-time cases. Include duplicate geometric/extra collisions, ratios below/equal/above one, and 32-bit-sized ceilings.

### P3-2 — README drift protection is manual, not mechanical

The README example is copied into `tests/builder.rs:1335-1357`, while the crate-level rustdoc contains a `text` schematic rather than including or compiling the README. The test comment says the copy is mirrored verbatim, but nothing ensures the two copies remain equal.

Impact: edits can leave a green test that no longer represents the published README. The current example is statically consistent, so this is not a present documentation error.

Recommendation: mechanically compile one source of truth (for example, include the README in crate docs with an appropriate doctest structure, or add a drift guard).

### P3-3 — benchmark evidence is narrow and not a publication gate

`benches/size_classes_bench.rs` has good black-boxing and path-activation fixtures, including fast, checked, invalid, multi-jump, slow-`None`, and boundary rows. The direct jump-vs-walk comparison uses only `JUMP_A` on the single 49-class SEFER scheme; there is no checked-in baseline or deterministic operation-count gate, and no benchmark for alternate density/class-count regimes.

Impact: the implementation's correctness does not depend on the benchmark, but the comments' performance conclusion is broader than the static artifact can prove. Wall-clock data was not run in this review.

Recommendation: treat “jump is faster” as scheme/workload-specific unless multiple table densities confirm it; retain iteration/path counters as the deterministic oracle and wall-clock as trend data.

### P3-4 — no clean-room downstream consumer of the packaged artifact

The in-tree integration is real and substantial: root `Cargo.toml:160,920` enables a versioned path dependency, `src/alloc_core/size_classes.rs` instantiates the crate, and allocator paths consume classifications derived from `Layout`. CI also has direct debug/release/i686/no_std/MSRV/clippy/rustdoc and `cargo publish --dry-run` coverage for the member. What is absent is a tiny consumer built from the produced package, outside workspace inheritance, that compiles the advertised README flow and checked/unchecked calls.

Impact: `publish --dry-run` is strong package verification, so this is not a release blocker. A clean-room consumer would specifically guard public ergonomics, normalized manifest behavior, and accidental reliance on workspace context.

## Numerical and contract assessment

- `size2class_len` validates power-of-two/non-zero `min_block` and uses checked `+1`.
- `build_table` validates `N`, non-zero geometric count/denominator, extra ordering/range/divisibility, merged strict monotonicity, widened multiply/ceil/rounding, and checked minimum-step advance.
- On currently supported 16/32/64-bit `usize`, the `u128` intermediate is sufficient for two `usize` factors; the source correctly calls out that a hypothetical 128-bit `usize` would require a different algorithm.
- `build_size2class` bounds the `u8` index domain at exactly 256 classes, checks strict monotonicity and `L`, uses a monotone pointer, and clamps an unrepresentable top-bucket product without wrap.
- For a `SizeClasses::build` product and valid power-of-two `align`, the fast path is correct because every class is a `min_block` multiple; the slow jump cannot skip a divisible class, strictly advances, and returns `None` on next-multiple overflow.
- The raw `size2class()` accessor deliberately exposes a false-sentinel window and can panic when indexed beyond it. Its rustdoc states both preconditions accurately; callers should prefer `class_for`/`try_class_for`.
- Base-address alignment remains a caller invariant. The in-tree allocator documents and implements the necessary segment/carve alignment; this crate cannot establish it from sizes alone.

No static contradiction was found between production arithmetic and the documented valid-input result: the smallest class with `block >= max(size, align)` and `block % align == 0`.

## Metadata, dependency, CI, and publication assessment

- Metadata is generally complete: name/version, edition, MSRV 1.88, SPDX license expression, description, repository/homepage/docs/readme, five keywords, valid-looking categories, and local dual-license files.
- Runtime dependency set is empty. Locked dev dependencies are `bench-scale-tool 0.1.0` and `proptest 1.11.0`; the manifest ranges are `0.1` and `1` respectively.
- The published API is small and documented under `#![deny(missing_docs)]`; `Params` is correctly `#[non_exhaustive]` with a const constructor, and `SizeClasses` intentionally avoids `Copy` for its potentially large embedded LUT.
- CI statically covers package dry-run, stable debug/release tests, executable i686 tests, bare-metal no_std build, all-target clippy with warnings denied, rustdoc with warnings denied, cargo-deny, and pinned-1.88 library/test/bench compilation. The release workflow reruns target-package tests and verifies the packaged crate before upload.
- No first-release semver baseline exists, appropriately making semver-checks inapplicable until after 0.1.0. This increases the value of settling P2-1/P2-2 before publication.
- The repository and homepage target exists at the checked time. Registry name ownership/availability is an external operational precondition and was not established by static repository inspection; verify it under the intended crates.io owner before creating the release tag.

## Release gate

Minimum conditions to change the verdict to GO:

1. Resolve or explicitly accept the P2-1 first-release API contract; add checked forwarding in the real consumer if retaining the split.
2. Decide whether P2-2 is intentional public scope or tighten the standalone LUT builder.
3. Stamp `0.1.0` in `CHANGELOG.md` with the actual release date.
4. On that exact commit, obtain green required CI and run the repository's normal package/publish-dry-run gates. This review did not execute them by explicit instruction.

P3 items may follow after release if consciously accepted, but documenting that acceptance is preferable to silently inheriting them.
