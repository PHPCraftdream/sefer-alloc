# size-classes: round-5 prepublish review (MS)

## Review identity

- Timestamp: `2026-08-29T22:47:03+02:00` (Europe/Berlin).
- Checked HEAD: `2dc9d08e0080a7054de04d1400f238817117b137` (`fix(perf): rustfmt src/alloc_core/size_classes.rs's drift-guard assert (workspace-wide cargo fmt --all, missed in aca5c1e)`).
- Mode: read-only review. No tests, builds, `cargo check`, Clippy, rustdoc, benchmarks, package/publish, or any other Cargo command was run. No source/configuration was changed. This report is the only write.
- Existing unrelated worktree state observed and left untouched: `docs/checkpoints/2026-08-28-0006.md` and `docs/checkpoints/2026-08-28-1404.md` were untracked.
- Independence: prior reports, checkpoints, and review history under `docs/reviews` were not read. References to old reviews that are embedded in current source/test/CI comments were encountered as part of reviewing the current files, not followed.
- Agent mode: single reviewer, no sub-agents, as requested. The target crate and its integration were covered completely, but the rust-intel skill's preferred per-theme delegated workflow could not be used under that constraint. Its separate theme modules (`async.md`, `unsafe-and-ffi.md`, `concurrency-and-state.md`, `data-and-types.md`, `security.md`, `drop-and-raii.md`, `deps-macros-ergonomics.md`, `lifetimes-and-api.md`, `testing.md`, `semantics-and-conformance.md`) were not opened; applicable risks were checked directly against the crate.

## Scope

Reviewed:

- production code and every public item in `crates/size-classes/src/lib.rs`;
- `Cargo.toml`, README, changelog, licenses, MSRV, categories/keywords, dependency and packaging shape;
- all crate tests, shared test oracle, property tests, and benchmark source;
- workspace dependency wiring, the `sefer-alloc` compatibility shim and public `SegmentLayout` forwarders;
- size-class consumer tests in the root package;
- size-classes-specific CI, MSRV/no_std/32-bit/package gates, and release workflow.

Static-only limitations: compiler acceptance, current CI status for this HEAD, packaged archive contents, docs.rs rendering/link resolution, actual benchmark numbers, and crates.io availability were not revalidated in this round.

## Verdict

**NO-GO for publication at the checked HEAD.**

The production algorithm is carefully defended and I found no P0 memory-safety issue, no `unsafe`, no dependency/runtime supply-chain surface, and no demonstrated numerical correctness bug in valid inputs. The immediate blocker is procedural but real: version `0.1.0` is still marked `Unreleased`, and the repository's own release workflow is designed to reject it. Before the first release, maintainers should also explicitly settle the safe-by-default API naming issue described in P2-1, because changing it after publication is much more expensive.

## Findings by priority

### P0 / critical

None found.

The crate is `#![forbid(unsafe_code)]`, `no_std`, zero-runtime-dependency, synchronous, allocation-free in production, and contains no FFI, atomics, threads, crypto, raw pointers, custom `Drop`, or interior mutability.

### P1 / release blocker

#### P1-1 — `0.1.0` is still explicitly unreleased

- Evidence: `crates/size-classes/CHANGELOG.md:7` says `## 0.1.0 - Unreleased`.
- The publish workflow checks the crate-local changelog and rejects a matching section containing `unreleased` (`.github/workflows/release.yml:215-220`, `250-308`).
- Impact: a real non-dry-run publication cannot clear the repository's own release gate. Publishing outside that workflow would bypass an intentional control and ship release notes that still state the release has not happened.
- Required action: replace `Unreleased` with the actual release date immediately before tagging/publishing, then let the configured package and release gates run on that exact commit.

### P2 / should resolve before first publication

#### P2-1 — the most obvious classifier name is the contract-trusting variant

- Evidence: `SizeClasses::class_for(size, align)` is public and safe but only `debug_assert!`s that `align` is a nonzero power of two (`crates/size-classes/src/lib.rs:813-853`, `880-884`). In release, an arbitrary `usize` alignment can produce an incorrect `Some` or `None`; `try_class_for` is the total checked API (`943-978`). README correctly recommends the checked twin (`README.md:29-36`).
- This is documented, so it is not an implementation/contract mismatch. It is nevertheless a first-release API ergonomics trap: callers naturally discover `class_for` first, and the trusted function does not carry an `unchecked`-style name even though invalid input changes correctness in optimized builds.
- Impact: direct consumers not sourcing alignment from `Layout` can silently select a non-divisible stride. In an allocator consumer, misuse can contribute to returning misaligned blocks even though this crate itself remains memory-safe.
- Recommendation before `0.1.0`: decide deliberately whether the checked operation should own the primary `class_for` name and the trusted hot path use an explicitly preconditioned name (for example `class_for_layout`/`class_for_unchecked_align`). If the current API is retained, add a first-screen README example using `try_class_for`, not only comments showing `class_for`.

#### P2-2 — consumer integration is real but not crates.io-shaped

- Positive evidence: root `Cargo.toml:920` consumes `size-classes` as a versioned path dependency; `src/alloc_core/size_classes.rs` instantiates the generic builder and forwards both checked and trusted classifiers; root consumer tests compare the public `SegmentLayout` surface with an independent scan.
- Gap: CI's package gate is `cargo publish --dry-run -p size-classes` (`.github/workflows/ci.yml:711-734`), while consumer coverage builds the same workspace/path source. There is no small standalone consumer fixture resolving the produced package artifact or registry-style dependency and compiling the README usage outside workspace inheritance.
- Impact: path-based integration can be green while a normalized/package-only manifest, archive omission, or external-use assumption fails. `publish --dry-run` verifies the packaged crate itself, but not a separate downstream's API usage.
- Recommendation: after packaging, compile a minimal external consumer against the generated archive/local registry (or an equivalent isolated copied package) using the README declarations and both classifier variants. Keep the existing root allocator integration tests; they test valuable behavior that a smoke consumer would not.

### P3 / quality and performance improvements

#### P3-1 — property testing varies requests, not table-building parameters

- Evidence: `tests/proptest_builder.rs:1-15` accurately states that three schemes are hand-picked constants; only `(size, align)` is generated. The reference scans are good independent classifier oracles, but most numeric builder combinations (`growth`, `min_block`, class count, extras placement) remain examples rather than generated cases.
- The ordinary reference table builder (`tests/builder.rs:21-85`) intentionally mirrors the production rounding algorithm closely, including the same `u128` formula. Golden boundaries and overflow tests reduce correlated-oracle risk but do not eliminate it.
- Recommendation: add generated runtime tests around fixed const-generic shapes, varying valid power-of-two `min_block`, numerator/denominator (including `0`, `num <= den`, and aggressive growth), and valid interleaving extras. Compare against a mathematically independent quotient/remainder formulation or arbitrary-precision test arithmetic. Include negative generation for duplicate, unsorted, below-minimum, and non-multiple extras with explicit panic-message oracles.

#### P3-2 — the jump-vs-walk benchmark does not isolate the advertised optimization

- Evidence: the benchmark itself admits that `JUMP_A` takes four jump iterations and four naive-walk iterations, and that the arms also differ in divisibility operation, bounds-check shape, and recomputed shift (`benches/size_classes_bench.rs:97-113`). Thus the pair is a rough mixed comparison, not evidence that jumping over classes is faster.
- Existing slow-path rows cover useful paths, including a `None` path, but there is no matched jump-vs-walk pair using an input that actually skips multiple classes with otherwise equivalent helpers.
- Recommendation: add a pair based on a genuinely skipping case (the source already identifies skipped indices for `JUMP_NONE`), and make the reference walk use the same precomputed shift and power-of-two bitmask so the principal variable is class visits/LUT reseeks. Treat wall-clock as trend data, not a hard CI gate; add deterministic iteration/visit counts if a regression gate is desired.

#### P3-3 — LUT density is the main structural performance/memory tradeoff

- Evidence: the public docs correctly quantify that `L = max_class / min_block + 1` dominates object size (`src/lib.rs:141-167`, `README.md:41-54`). The realistic fixture uses about 16 KiB, while wider consumer configurations can be much larger.
- Current per-query fast path is appropriately small; no clear local micro-optimization was found statically. The larger opportunity is representation: a flat byte per minimum-block bucket spends memory proportional to maximum class size, not class count.
- Recommendation: retain the current representation for `0.1` unless measurements show cache/rodata cost matters. For a future major version, benchmark a two-level/hybrid mapping (dense small LUT plus computed/coarser upper tier). The public `L` const generic and raw `size2class()` accessor make such a change breaking, so document this as an intentionally frozen `0.1` tradeoff.

### P4 / documentation and maintenance

#### P4-1 — current consumer comments contain two stale/incorrect explanations

- `src/alloc_core/segment_layout.rs:37-42` calls the larger-alignment algorithm a bounded “divisibility-walk”; the crate now uses a jump/reseed loop. Other nearby comments use the correct term.
- `tests/size_classes_lookup.rs:331-334` says `class_for(32, 0)` would underflow `need - 1`; it would not, because `need = max(32, 0) = 32`. A debug build panics at the alignment `debug_assert!`; an ordinary release build can instead take the fast path and return an unspecified wrong answer. The true subtraction-underflow corner is `(size, align) == (0, 0)`.
- Impact: no production behavior change, but both comments teach the wrong mechanism in the real consumer integration.
- Recommendation: correct the wording in the next source change.

#### P4-2 — README example synchronization remains manually duplicated

- The executable mirror and a one-direction raw-line guard are thoughtful, and `tests/builder.rs:1403-1410` explicitly documents the remaining limitation.
- Recommendation: make the README example the single compiled source (for example, include it in crate-level docs or generate/extract one canonical snippet) when practical. This is not a publication blocker because the current declarations are statically consistent and CI compiles their mirror.

## Production code and numerical assessment

The core arithmetic is internally coherent for supported `usize` widths (32/64):

- `size2class_len` checks both the power-of-two divisor and the only reachable `+1` overflow.
- `build_table` validates cardinality, minimum block, denominator, extras shape, final strict monotonicity, and actual next-class representability. Widening `cur * num` to `u128` is sufficient for all currently supported `usize <= 64` targets; the source honestly rejects a hypothetical 128-bit-`usize` proof.
- The LUT monotone-pointer construction is `O(L + N)`, caps `N` at 256 before the `u8` cast, validates strict ordering, and folds unrepresentable top-bucket products into the documented clamp.
- For builder-produced tables, `small_max == (L - 1) * min_block`. Therefore the `seed_idx >= L - 1` guard is exactly equivalent to `need > small_max`; `need == small_max` lands at `L - 2`, while the top LUT slot is unreachable through `class_for`.
- The slow-path bitmask is valid under its power-of-two alignment precondition. Because `align > min_block` and both are powers of two, `align` is a multiple of `min_block`; the reseed index targets the first bucket that can cover the next alignment multiple and advances strictly or returns `None`.
- `try_class_for` rejects `0`/non-power-of-two alignment before arithmetic, making its “never panics for any `(size, align)`” claim credible for every constructible `SizeClasses` value (fields are private and `build` enforces invariants).

No overflow, truncation, off-by-one, nontermination, panic-on-valid-input, or smallest-fitting-class defect was found by static derivation. This conclusion is static evidence, not a substitute for the prohibited test/toolchain runs.

## Public API and contracts

The API is small and documented. `Params` is `#[non_exhaustive]` with a const constructor; `SizeClasses` deliberately avoids `Copy`; `InvalidAlign` has matching `Display`/`Error`; query methods are const and allocation-free. Panic contracts are unusually explicit. The address-vs-stride distinction is correctly prominent: divisibility of block size preserves base alignment but cannot create it.

Semver-sensitive choices intentionally exposed in `0.1.0` include `Params` public fields, `InvalidAlign(pub usize)`, const generics `N`/`L`, fixed raw array return types, the flat LUT representation, and panic behavior/messages used by tests. None is inherently wrong, but the first publication freezes consumer expectations around them; P2-1 is the only one I recommend actively reconsidering before release.

## Tests and oracles

Static test quality is strong:

- independent linear scans check smallest-fit and divisibility;
- boundary tests cover exact class edges, top bucket/sentinel, first out-of-bounds raw lookup, 256/257 class capacity, 32/64-bit overflow limits, intermediate-product widening, next-multiple overflow, malformed extras, and debug/release contract differences;
- the root allocator has additional public-forwarder and behavior-level regression coverage;
- `#[should_panic]` tests name expected messages rather than accepting arbitrary panics;
- benchmarks share fixture constants with tests and have path-activation assertions.

Primary residual weaknesses are the parameter-generation gap (P3-1), mirrored-reference correlation, and manual README synchronization (P4-2). No vacuous production-correctness oracle was found.

## CI, packaging, and release

The configured matrix is substantial: stable debug/release tests, executable i686 tests, a genuine bare-metal no_std build, crate-scoped Clippy/rustdoc, MSRV 1.88 checks including tests and bench compilation, and publish dry-run. The release workflow resolves the package, checks tag/version (for tag pushes), enforces a dated crate-local changelog, requires a successful main CI run for the exact SHA, reruns target tests, and publishes with verification enabled.

Residual observations:

- The current HEAD's CI result was not queried/verified in this static round.
- The CI package-gate actions use moving major/stable references, whereas the token-bearing release checkout is SHA-pinned. This is a workspace supply-chain posture choice, not a size-classes-specific release blocker.
- There is correctly no semver-check baseline before the first crates.io release; CI comments say to add it afterward.
- P1-1 means the configured real release path is intentionally red today.

## GO conditions

Minimum conditions to change this verdict to GO:

1. Date the `0.1.0` changelog section on the exact release commit.
2. Make and record an explicit decision on P2-1 while an API rename remains cheap.
3. Run the repository's configured CI/package/release gates on that exact commit; this review did not execute them.
4. Preferably add the isolated downstream/package consumer smoke from P2-2, or explicitly accept the current path-consumer coverage for `0.1.0`.

With item 1 unresolved, the checked HEAD is unambiguously **NO-GO**.
