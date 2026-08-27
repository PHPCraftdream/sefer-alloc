# size-classes: pre-publication review, round 2 (MS)

- Timestamp: 2026-08-27T11:11:15+02:00 (Europe/Berlin)
- Checked HEAD: `47d50d32398f4a7e41efe44dd40d5e5e4e17e739`
- Verdict: **NO-GO for publication from the current tree**
- Mode: read-only audit except for this report; no source/config changes; no staging or commit; no sub-agents.
- Verification constraint: no tests, build, `cargo check`, Clippy, rustdoc, benchmarks, package/publish, or any other Cargo command was run. All conclusions below are static. Existing untracked files were left untouched.
- Independence: no prior report, checkpoint, or review-history document under `docs/reviews` was opened. Historical commentary embedded in current source files was necessarily visible while reviewing those files, but was not treated as evidence.
- Rust-intel process note: the user explicitly prohibited sub-agents, so this is a single-reviewer pass. Target-file coverage is complete for this crate and the directly relevant workspace integration/configuration. Broad category fan-out was not performed; async, FFI, unsafe, crypto, and concurrent-state categories are structurally inapplicable to this synchronous `#![forbid(unsafe_code)]`, dependency-free production crate.

## Scope

Reviewed in full:

- `crates/size-classes/Cargo.toml`, `src/lib.rs`, `README.md`, `CHANGELOG.md`, licenses;
- all crate tests: `tests/builder.rs`, `tests/proptest_builder.rs`, `tests/common/mod.rs`;
- `benches/size_classes_bench.rs`;
- workspace membership, dependency/feature wiring, lockfile entries, shared lints and MSRV declarations;
- `.github/workflows/ci.yml` and `.github/workflows/release.yml` paths relevant to this crate;
- the real in-tree consumer shim `src/alloc_core/size_classes.rs`, its public-internals forwarding surface, and all production `class_for` call-site shapes found under `src/`.

Statically checked: public API/semver shape, panic and alignment contracts, integer overflow and width boundaries, table merge and LUT construction, class-selection minimality, documentation/metadata, test-oracle independence, benchmark path activation, CI/MSRV/no_std/package gates, and path-dependency consumer integration.

Not dynamically established in this pass: compilation on any target, packaged tarball contents, docs.rs rendering, benchmark numbers, CI status for HEAD, crates.io name ownership/availability, or behavior of the current resolved dev-dependency graph. Those omissions are mandated by the no-Cargo/no-test mode and must not be mistaken for green evidence.

## Findings

### P0 — publication blocker

#### P0-1. The repository's own release workflow rejects the current changelog

`crates/size-classes/CHANGELOG.md:7` is still `## 0.1.0 - Unreleased`. The real-publish guard in `.github/workflows/release.yml:215-311` requires exactly one matching version section and explicitly fails if that section contains `unreleased` (`:301-308`). Therefore a non-dry-run release of 0.1.0 from this exact tree is intentionally blocked.

Required before publishing: replace `Unreleased` with the actual release date and let the release workflow evaluate the resulting commit. This is not optional release polish; it is a hard gate in current configuration.

### P1 — high priority

#### P1-1. The public numeric documentation states an incorrect Rust const-evaluation rule

`crates/size-classes/src/lib.rs:166-174` says that release-profile const evaluation follows the profile's `overflow-checks` setting and could wrap an unchecked `+ 1`. Rust constant evaluation rejects overflowing integer arithmetic; profile-dependent wrapping is the runtime concern. Similar “release-profile const evaluation” wording is repeated around the arithmetic rationale and in test commentary.

The implementation's explicit `checked_*` operations are still correct and valuable: they protect runtime calls to these public `const fn`s and provide stable, named diagnostics. The stated reason is nevertheless false in public rustdoc and can teach consumers the wrong language rule. Correct the explanation to distinguish compile-time evaluation (overflow is rejected) from runtime evaluation in a release profile (unchecked arithmetic may wrap when overflow checks are disabled).

#### P1-2. `class_for`'s out-of-contract behavior is documented more narrowly than it actually behaves

`crates/size-classes/src/lib.rs:759-773` says that violating the power-of-two alignment precondition trips a debug assertion and is “otherwise silent” and can “only produce a wrong class choice.” This is false for at least `(size, align) == (0, 0)`: after the release-only disappearance of the debug assertion, `(need - 1)` underflows and the LUT access panics. The crate's own test commentary acknowledges this at `tests/builder.rs:1140-1146`.

This is not an in-contract classifier bug: `class_for` clearly requires a power-of-two alignment and `try_class_for` correctly rejects zero/non-power-of-two values before arithmetic. It is still a public contract contradiction. State that behavior is unspecified/out-of-contract and may return a wrong answer or panic, or make the release behavior match the stronger prose. The README's “risking the wrong answer” wording should be aligned with that decision.

### P2 — medium priority

#### P2-1. The real consumer duplicates both generated tables in static storage

The standalone `SizeClasses` value owns `table` and `size2class` (`crates/size-classes/src/lib.rs:550-557`). The root consumer then also materializes `SIZE_CLASS_TABLE` and a separate `SIZE2CLASS` static while retaining `SC` (`src/alloc_core/size_classes.rs:183-235`). The comments correctly admit that optimizer/linker deduplication is not guaranteed. For the default fixture this means a second ~16,173-byte LUT plus a second 392-byte class table can survive in the binary; medium configurations amplify it.

This is not a correctness defect and may be linker-deduplicated, but it weakens the “single source of truth”/compact reusable primitive story in the actual consumer. Prefer exposing the root's compatibility constants as references to `SC.table()`/`SC.size2class()` where possible, or evolve the crate toward a borrowed/generated-table representation. Measure binary size before choosing a redesign.

#### P2-2. First-release validation has no clean downstream consumer fixture

The integration tests import `size_classes` as an external crate and the root allocator is a genuine path-dependency consumer, which is meaningful coverage. CI also has a `cargo publish --dry-run -p size-classes` gate. However, there is no small consumer package that builds solely against the packaged artifact's public API on MSRV/no_std; the root consumer can rely on workspace resolution and a path source, and package dry-run verifies packaging/build rather than the root shim's downstream usage.

For a foundational allocator primitive, add a tiny tarball/path consumer smoke fixture after release preparation (or immediately after 0.1.0, then switch it to the registry version). It should instantiate the README scheme, call both checked and trusted classifiers, and compile on 1.88 plus a real no_std target. This is a confidence gap, not evidence of a present API failure.

### P3 — low priority / quality and maintainability

#### P3-1. README has duplicated text and a rustdoc-style link that is not a normal README link

`crates/size-classes/README.md:53-59` repeats the same `Params is #[non_exhaustive]` paragraph twice. At `:51`, ``[`size2class_len`]`` has no Markdown reference target in the README; unlike rustdoc intra-doc links, crates.io/GitHub Markdown will not resolve it to the item automatically. Remove the duplicate and use an explicit docs.rs URL (preferably versioned or `latest`).

#### P3-2. Published source is overloaded with internal review archaeology

Production rustdoc/comments, tests, and benches contain reviewer handles, audit-round names, task IDs, stale report paths, and a commit-specific benchmark citation (notably `src/lib.rs:162`, `:570-572`, `:709-710`, `:825-828`, plus many test/bench comments). Because tests and benches are tracked under the package directory and no package include/exclude policy narrows them, this material is likely to ship in the source tarball.

The rationale itself is often useful, but provenance notes dominate the code and include references unavailable to crates.io consumers. Retain invariant/why comments; move campaign history and raw benchmark provenance to maintainers' documentation or changelog entries. This reduces review noise without losing technical justification.

#### P3-3. Benchmark evidence is useful but incomplete for the performance claims

The benchmark correctly distinguishes fast hit, real jump paths, multi-jump, slow-path `None`, boundaries, and `is_huge`; tests pin that the selected jump fixtures actually activate the intended path. The `jump_vs_walk` comparison uses only `JUMP_A`, while public comments claim a ~24–45% gain based on external raw numbers not present beside the crate (`src/lib.rs:824-828`). There is also no benchmark for runtime `SizeClasses::build`, despite all builder functions being publicly callable at runtime, nor a retained baseline/gate in this crate.

Before preserving a numeric performance claim in public source, store reproducible result metadata or soften it to a qualitative claim. Add representative jump-vs-walk cases across dense, sparse, multi-jump, and no-match inputs; benchmark runtime construction only if runtime construction is intended to be supported as a performance-sensitive use case.

#### P3-4. Post-0.1 semver automation is planned but not yet present

`.github/workflows/ci.yml:721-724` intentionally omits `cargo semver-checks` because no published baseline exists. That is correct before the first release. Add the gate immediately after 0.1.0 is on crates.io; the public const-generic type, positional `Params::new`, public fields, tuple error, trait impls, and panic contracts create a nontrivial semver surface.

## Correctness assessment

No in-contract production correctness bug, unsafe occurrence, data race, allocation, dependency, or hidden platform syscall was found in the crate's production code.

The core arithmetic is defensively structured:

- `min_block`, length, class-count, table monotonicity, extras shape, denominator, and merged duplicate conditions are checked;
- geometric arithmetic is widened to `u128`, the representable `usize` result is checked, and the minimum-step fallback is checked;
- `build_size2class` uses a monotone pointer, bounds class indices to `0..=255`, and handles an unrepresentable top-bucket product by clamping to `small_max`;
- `class_for` rejects `need > small_max` before indexing, and its power-of-two jump advances strictly or returns `None` on arithmetic exhaustion;
- `try_class_for` closes the raw-alignment validation gap for arbitrary inputs;
- the carve-base alignment obligation is unusually explicit and the root consumer's Layout-derived calls satisfy the power-of-two side of the contract.

One deliberate limitation should remain prominent: `build_size2class` accepts strictly increasing hand-built tables whose entries are not multiples of `min_block`; such entries can be unreachable through bucket lookup. Current rustdoc discloses this accurately. It is a low-level API choice rather than a silent defect, but changing validation after 0.1.0 could reject previously accepted runtime input, so decide before release whether this permissiveness is intentional long-term.

## Tests and oracles (static review)

Strengths:

- an exhaustive/boundary-dense sweep checks the real SEFER fixture against an independent linear scan;
- three distinct const schemes receive property-generated `(size, align)` checks;
- the geometric sequence has hand-derived golden values, avoiding total dependence on a mirrored builder;
- 32/64-bit overflow boundaries, 256/257-class boundary, malformed inputs, LUT sentinel/OOB zones, zero alignment through the checked API, and runtime/const construction shapes are represented;
- benchmark fixtures have path-activation oracles.

Residual weaknesses:

- the “reference” table builder intentionally mirrors much of production's arithmetic and merge structure; the small golden vector helps but does not independently cover arbitrary growth ratios or extras merges;
- property tests randomize requests, not schemes, because const generics force fixed instantiations; malformed/random runtime `Params` are therefore covered mainly by selected examples;
- `proptest` is declared as broad major version `1`, so future lock refreshes can alter the test graph; MSRV CI mitigates this only when it runs.

No test result is claimed in this report.

## CI, metadata, packaging, and release assessment

Positive static evidence:

- complete basic metadata, dual licenses, `rust-version = 1.88`, edition 2021, no runtime dependencies, no features, and `#![no_std]`/`#![forbid(unsafe_code)]`;
- workspace rustfmt, crate-target Clippy with warnings denied, rustdoc warnings denied, debug and release tests, executable i686 tests, bare-metal no_std build, pinned-MSRV check/test-no-run/bench-no-run, cargo-deny, and publish-dry-run rows are configured;
- the release workflow validates tag version for tag-triggered publication, requires a dated changelog, checks CI success for the exact SHA, reruns package tests, and relies on publish verification.

Gaps/risks:

- all of the above are configuration, not evidence that HEAD passed those jobs;
- first-release semver comparison cannot exist yet;
- action/toolchain channels in ordinary CI are moving references; the release checkout action is SHA-pinned, while the toolchain installer intentionally tracks stable;
- current changelog state definitely blocks the actual release workflow.

## Consumer integration assessment

The root `sefer-alloc` consumer is real, not a dead example: `alloc-core` enables the optional versioned path dependency, the shim instantiates the exact 49-class scheme, and allocator/registry call sites classify `Layout`-derived alignments. Production call sites inspected either clamp allocation size to `MIN_BLOCK` or explicitly document the unclamped forwarding contract; alignments originate from `Layout`, satisfying the trusted `class_for` precondition. The root also statically asserts cross-crate invariants and exposes the generated table through its internal `SegmentLayout` compatibility surface.

The principal integration issue is duplicated generated storage (P2-1), not semantic drift. The test fixture remains a snapshot rather than a live import of root-private constants; root integration tests are therefore still necessary to catch future parameter drift.

## Final gate

**NO-GO on HEAD `47d50d32398f4a7e41efe44dd40d5e5e4e17e739`.**

Minimum publication actions:

1. Fix P1-1 and P1-2 so public numeric/panic contracts are accurate.
2. Stamp the 0.1.0 changelog with the actual release date, clearing P0-1.
3. Run and require the configured MSRV, no_std, debug/release/i686, Clippy, rustdoc, deny, package dry-run, and release gates on the final commit (outside this audit's prohibited mode).
4. Confirm crates.io ownership/name availability and token permissions at release time.

P2/P3 items can be scheduled, but the duplicated README paragraph should be cleaned before first publication because it is trivial and user-visible. Once the P0/P1 items are fixed and the dynamic gates are green, the core implementation appears suitable for a 0.1.0 release.
