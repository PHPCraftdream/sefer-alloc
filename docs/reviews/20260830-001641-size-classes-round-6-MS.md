# size-classes pre-publication audit — round 6 (MS)

## Verdict

**NO-GO in the current working tree.** The production arithmetic and classifier look carefully designed and I found no demonstrated wrong answer for an in-contract input, no unsafe code, and no runtime dependency risk. Publication is nevertheless blocked by two concrete release-gate failures: the benchmark discards `Option` values returned by `black_box` in a way that should trip `unused_must_use` under the crate's `clippy -D warnings` CI row, and the 0.1.0 changelog is still marked `Unreleased`, which the release workflow explicitly rejects.

After those blockers are fixed and the prescribed package/test/doc/clippy jobs are actually run, the crate is a reasonable first-release candidate. This report cannot supply that dynamic evidence because the requested mode forbade all Cargo commands and all tests/builds.

## Audit identity and constraints

- Timestamp: `2026-08-30 00:16:41` Europe/Berlin.
- Repository: `D:\dev\rust\sefer-alloc`.
- Final checked HEAD: `c7f4f092a81c24fd0406489f831d262d7bf09229`.
- Concurrency note: the audit began at HEAD `acb8203d38ddd6806b8ef06f7520f7af956a155b` with pre-existing unstaged edits in `crates/size-classes/{CHANGELOG.md,README.md,src/lib.rs,tests/builder.rs,tests/common/mod.rs,benches/size_classes_bench.rs}`. Other workspace actors advanced HEAD during this read-only audit, first incorporating changes in those six files and then landing an unrelated commit. The target crate was clean at final verification on `c7f4f092a81c24fd0406489f831d262d7bf09229`; no target/release-scope diff existed from the immediately preceding checked HEAD, and both blocking constructs remain present. This audit did not change or stage those files.
- Read-only mode: no source/config edits. This report is the only write.
- No tests, compilation, `cargo check`, clippy, rustdoc, benchmarks, package/publish, or any other Cargo command were run.
- No sub-agents were used, per request.
- No prior reports, checkpoints, or review history under `docs/reviews` was read. Findings are based on current code/configuration only.
- The broad Rust review checklist normally recommends module fan-out. Because sub-agents were explicitly forbidden, this is a bounded single-context pass. Async, concurrency, unsafe/FFI, security/crypto, and Drop/RAII modules were applicability-scanned rather than delegated/deep-divided: the target crate has no async, threads, atomics, locks, FFI, cryptography, I/O, resource lifecycle, or unsafe blocks. Data/numerics, public API, dependencies/metadata, testing, performance, and semantic-contract concerns received the detailed pass.

## Scope

Reviewed statically:

- `crates/size-classes/Cargo.toml`, README, changelog, and both license files;
- all production code in `crates/size-classes/src/lib.rs`;
- all crate tests and shared test helpers;
- the crate benchmark and its harness dependency configuration;
- workspace membership, dependency declaration/lockfile resolution, lint inheritance, MSRV declaration, no-std claims, CI package/test/clippy/rustdoc/MSRV/32-bit/no-std rows, and release workflow;
- the real in-tree consumer integration in `src/alloc_core/size_classes.rs`, `SegmentLayout`, and allocator classification call sites;
- public contracts for construction, panic behavior, raw LUT access, checked/unchecked classification, alignment/base-address obligations, huge-size policy, and representation cost;
- arithmetic boundaries: zero, one, maximum class, one past maximum, `usize` overflow, 32/64-bit behavior, class-index `u8` capacity, slow-path termination, and power-of-two alignment assumptions;
- test-oracle independence, boundary selection, property-test domains, benchmark path activation, and benchmark claim fidelity.

Not dynamically established in this audit:

- that the current dirty tree compiles;
- that tests pass in debug/release/i686;
- that rustdoc and clippy are warning-clean;
- that the no-std target builds;
- actual packaged tarball contents or isolated package verification;
- benchmark numbers or generated-code/bounds-check claims;
- current crates.io/docs.rs/GitHub network state.

## Findings

### P1 — publication blockers

#### P1-1: benchmark ignores must-use `Option` results; strict CI should reject it

`crates/size-classes/benches/size_classes_bench.rs` stores `class_for`/`walk_class_for` results, then ends the closure with statements of the form:

```rust
black_box(result);
```

This occurs for `Option<usize>` results at lines 38, 75, 80, 105, 112, 135, 146, 163, 178, 191, 214, 220, and 228. `Option` is `#[must_use]`; the outer `black_box` returns the same `Option`, which is then discarded. The two `try_class_for` rows at lines 52 and 62 already use the warning-clean form `let _ = black_box(result);`, making the inconsistency especially visible.

The crate-specific CI row at `.github/workflows/ci.yml:1883` runs `cargo clippy -p size-classes --all-targets -- -D warnings`, and `--all-targets` includes this bench. Therefore the current benchmark is expected to fail that release-relevant gate on `unused_must_use`. This is a static conclusion; execution was forbidden.

Recommended fix: consume every must-use benchmark result explicitly (`let _ = black_box(result);`) or return it from the closure if the harness contract supports that. Apply consistently, including any future fallible/optional benchmark row.

#### P1-2: release workflow rejects the current changelog

`crates/size-classes/CHANGELOG.md:7` is `## 0.1.0 - Unreleased`. The real-publish workflow requires exactly one matching version section and then rejects a section containing `unreleased` (`.github/workflows/release.yml:273-308`). Consequently a non-dry-run `size-classes-v0.1.0` release cannot pass the repository's own guard in the current state.

Recommended fix: only at release time, replace `Unreleased` with the actual release date and ensure the tag version equals `Cargo.toml`'s `0.1.0`. Do not bypass the guard.

### P2 — important pre-1.0 decisions and evidence gaps

#### P2-1: `class_for` is a safe, attractively named API with a release-silent logical precondition

`SizeClasses::class_for` only `debug_assert!`s that `align` is a nonzero power of two (`src/lib.rs:881-885`). With debug assertions off, invalid alignment can return an incorrect `Some`/`None`; with `align == 0`, behavior also varies with overflow-check settings. The docs explain this unusually thoroughly and `try_class_for` provides the correct total API, so this is not memory unsafety and not an undocumented bug.

It remains an API misuse hazard: users discover the shorter `class_for` name first, and a safe function with an ordinary name does not visually advertise that release builds stop validating its logical contract. Before the first release, consider naming the trusted variant `class_for_unchecked`/`class_for_trusted_align`, or making `class_for` checked and giving the hot variant the explicit trusted name. If the present split is retained, the README's recommendation of `try_class_for` should stay prominent and examples should actually call it.

#### P2-2: there is no registry-shaped external consumer test yet

The root allocator is a substantial real consumer: it instantiates the crate, classifies actual `Layout`-derived alignments, exposes a checked public `SegmentLayout::try_class_for`, and documents/proves the separate carve-base alignment obligation. This is good integration evidence.

However, the root manifest uses `size-classes = { path = "crates/size-classes", version = "0.1", optional = true }` (`Cargo.toml:920`). The crate's package gate tests its own isolated package, but there is no tiny external consumer built against a registry-resolved `size-classes = "0.1"`; before first publication that is naturally impossible. After 0.1.0 lands, add a consumer/package gate that does not rely on the workspace path and exercises `Params`, generic derivation, checked classification, and no-std use. This is not a first-publication blocker, but it is the remaining gap in “real consumer” evidence.

#### P2-3: the public raw-LUT/const-generic representation is intentionally semver-sticky

`SizeClasses<N, L>` exposes both const generics and `size2class() -> &[u8; L]`; `build_size2class` is also public. The docs correctly warn that a future hybrid/compressed LUT will likely require a breaking release (`src/lib.rs:692-702`). The current flat table is about 16 KiB for the representative scheme and can become much larger as `max_class / min_block` grows.

This is not a correctness defect, but 0.1.0 is the cheapest point to decide whether consumers really need the concrete LUT shape. If classification and table inspection are sufficient, a slice-returning accessor and/or a less representation-bearing public type would preserve more optimization freedom. If raw const-array access is a core product feature, keep it and treat later representation changes as major-version work.

### P3 — quality, performance, and CI improvements

#### P3-1: the jump-vs-walk benchmark does not isolate the jump optimization

The benchmark itself candidly documents that `JUMP_A` takes four iterations in both implementations and that the two arms differ in division, bounds checks, fixed-array versus slice indexing, and repeated shift derivation (`benches/size_classes_bench.rs:115-131`). Thus it is a useful end-to-end comparison but cannot support a causal claim that “jump ahead” itself is faster. The dedicated path-activation tests are good, but iteration-count evidence and wall-clock causal evidence remain different.

Recommendation: add a fixture where jump and walk visit materially different numbers of classes while holding divisibility primitives and lookup representation as equal as practical, or rename/report the current pair strictly as an implementation-level comparison. Keep wall-clock as a trend, not a noisy shared-CI hard gate.

#### P3-2: benchmark inputs are constants and may overstate branch-predicted steady-state speed

Every row repeatedly classifies one fixed pair. `black_box` prevents trivial value propagation but does not model a realistic distribution of sizes/alignments or branch-history effects. Add a batched deterministic input corpus covering common small hits, boundary buckets, several valid alignments, successful jumps, and `None`; report both fixed-case latency and mixed-workload throughput. This would better characterize real allocator consumption without replacing the focused microbench rows.

#### P3-3: test oracles are strong but the table-builder oracle still mirrors production closely

Classifier correctness has a genuinely independent linear scan, broad deterministic boundary sweep, three property-tested schemes, and direct fit assertions. Numeric edge tests cover 32/64-bit overflow, top-bucket overflow, 256/257 classes, interleaving extras, and slow-path termination. Panic tests consistently include `expected` strings. These are strong points.

The `reference_table` oracle nevertheless repeats the same growth formula and sorted-merge control flow as production. Golden-value and edge tests reduce the circularity risk, but a future shared misunderstanding could still satisfy both. Improvement options: generate random runtime parameter sets and compare against a mathematically simpler sort/dedup reference, add mutation testing for arithmetic/guard operators, and pin a small set of hand-derived tables for ratios below/equal/above one and extras at every merge position.

#### P3-4: CI toolchain/action inputs are not fully reproducible

The crate's MSRV job pins Rust 1.88 and compiles library, tests, and bench targets, which is good. The main package/test/doc/clippy jobs use `dtolnay/rust-toolchain@stable`, while actions are referenced by movable tags such as `actions/checkout@v5`. This means the release evidence for the same source commit can change as stable Rust or action tags move.

Recommendation: pin security-sensitive Actions by commit SHA (with a readable version comment) and record the exact stable toolchain used by successful release CI. Keep the explicit 1.88 MSRV rows separately.

#### P3-5: README example advertises the checked API but does not execute it

The README imports `InvalidAlign`, then only declares constants/statics and describes calls in comments. The mirrored test proves construction and calls unchecked `class_for`; the exhaustive suite separately proves `try_class_for`, so correctness coverage exists, but the primary user example does not demonstrate the recommended path or error handling.

Recommendation: make the example a complete runnable snippet with one successful `try_class_for` call, one invalid-alignment rejection, and a `block_size` assertion. Prefer a rustdoc-included/compiled example over the current one-directional textual drift guard.

### P4 — observations, not blockers

- Production code is `#![no_std]`, dependency-free, and `#![forbid(unsafe_code)]`. No unsafe block, FFI, atomic ordering, synchronization, allocation, I/O, panic-across-boundary, or resource lifecycle issue exists in the target production crate.
- The geometric step uses widened `u128` arithmetic, checked rounding/addition, and an explicit `usize` range assertion. The min-step fallback is checked. On currently relevant <=64-bit `usize` targets this avoids intermediate-product false rejection and release wrapping.
- `build_table` validates power-of-two `min_block`, nonzero denominator/count, exact `N`, extras alignment/lower bound/order, geometric overflow, and merged-table strictness. Duplicate geometric/extra classes are rejected at the builder boundary.
- `build_size2class` validates nonempty/strictly increasing input, exact `L`, power-of-two bucket size, and the precise 256-class `u8` index capacity. Its top-bucket multiplication uses checked arithmetic and a mathematically valid clamp.
- The slow-path jump is monotone: every non-divisible class advances to a strictly greater required multiple and then to a strictly greater table index; guards prevent sentinel cycling and out-of-bounds access. No termination defect was found for a valid power-of-two alignment.
- `try_class_for` validates alignment before arithmetic. For valid alignment `need >= 1`; index-space guards precede LUT indexing, including extreme `usize` inputs. Its “never panics for any `(size, align)`” claim is consistent with the reviewed construction invariants.
- Alignment semantics are correctly separated: class-size divisibility preserves alignment but cannot create a correctly aligned carve base. The root consumer explicitly establishes segment/base/block placement conditions rather than pretending the size crate can prove pointer alignment.
- The huge threshold is correctly a caller policy and independent of class lookup.
- Metadata is generally publication-grade: version/edition/MSRV/license/readme/repository/homepage/docs/keywords/categories are present; dual license files exist; runtime dependencies are empty; lockfile records registry checksums for dev dependencies; no git/path runtime dependency is shipped by this crate.
- CI coverage is broad on paper: package dry-run, debug/release tests, executable i686 tests, bare-metal no-std build, all-target clippy with denied warnings, warning-denied rustdoc, and MSRV compilation of library/tests/bench. The first-release lack of semver baseline is correctly acknowledged; add semver checking after 0.1.0 exists on crates.io.

## Publication gate checklist

- [ ] Fix the ignored must-use benchmark results and obtain a clean crate-specific `clippy --all-targets -D warnings` result.
- [ ] Consolidate `CHANGELOG.md` into a dated 0.1.0 section only when the release date is real.
- [ ] Run the existing debug, release, i686, no-std, rustdoc, MSRV, bench-compilation, and package-dry-run CI rows on the exact release commit.
- [ ] Confirm the package tarball contains README, changelog, licenses, source, tests/common helper, and benchmark, and that isolated verification succeeds.
- [ ] Confirm the release commit's complete main CI run is green, as required by `release.yml`.
- [ ] Tag `size-classes-v0.1.0` only after version/date/CI gates match.
- [ ] After publication, add semver-baseline and registry-shaped consumer checks.

## Final decision

**NO-GO for publication from the audited working tree.** The core implementation appears numerically sound for its documented in-contract domain, but the benchmark warning issue and unreleased changelog make the present artifact fail its own publication process. Reassess as GO only after both are corrected and the prohibited-in-this-audit Cargo/CI gates run successfully on the exact release commit.
