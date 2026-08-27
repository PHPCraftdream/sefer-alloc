# Holistic GO/NO-GO review — `crates/size-classes` first crates.io release

- **Reviewer:** rush2, fresh-eyes whole-picture pass (2026-08-26)
- **Mode:** READ-ONLY static review. No `cargo` build/check/test/clippy was run; no git
  writes. Every file cited below was read in full this session. External checks (crates.io
  API, git remote) are noted as such.
- **Scope:** the whole crate as a first-time evaluator: `src/lib.rs` (785 lines),
  `Cargo.toml`, `README.md`, `CHANGELOG.md`, `LICENSE-*`, `tests/builder.rs`,
  `tests/proptest_builder.rs`, `tests/common/mod.rs`, `benches/size_classes_bench.rs`,
  plus workspace/packaging context. I did **not** read the prior review reports, to keep
  this pass genuinely fresh; overlap with them, where noticed, is marked.

## Verdict

**GO.**

No P0 or P1 finding. Correctness, licensing, packaging metadata, MSRV, the
`#![forbid(unsafe_code)]` claim, and the publish mechanics all check out. Everything below
is documentation-polish (one P2, two P3s) and nits (P4s); all of them can land before or
after the 0.1.0 publish except the CHANGELOG dating, which should happen at publish time.
I found no new correctness defect — which, after 6 Sol-codex rounds, 13 Claude rounds, and
a prior 3-agent wave, is itself the expected result; what a fresh-eyes pass *did* surface
is doc-shape and ergonomics, below.

## Overall impression

This is a small, tight, genuinely well-engineered crate: 16 public items, zero
dependencies, const-evaluated everything, every precondition machine-checked with a named
panic message, and a test suite (42 `#[test]` fns, 26 `#[should_panic]`s each pinning a
distinct message substring, hand-derived golden values, three independent reference
oracles, proptest jump≡walk≡scan equivalence over three schemes) that is far above the
norm for a crate this size. As *code*, it is ready.

As *documentation*, it visibly wears its audit history. 369 of 785 lines of `lib.rs`
(47%) are rustdoc; `class_for`'s doc is ~91 lines against a 48-line body; the same
stride-vs-address caveat appears near-verbatim in six doc locations (three of them inside
`class_for` alone). A first-time reader who opens the docs.rs page, reads the crate root,
then `Params`, then `class_for`, encounters the same caveat — each time slightly expanded
— six times before they reach the lookup formula. None of it is *wrong*; it reads like
sediment, and it makes the crate harder to evaluate than the code deserves. The README is
the strongest single document (complete, correct, with the one construction recipe) and is
rot-protected by a mirrored compiled test (`tests/builder.rs:1022-1043`).

The API split (`build_table` / `build_size2class` / `SizeClasses::build`) is the right
abstraction *for this crate's purpose* — the standalone builders are individually useful
(sefer itself and the hand-built-table tests exercise them), and `SizeClasses::build` is
the one-call path. The one genuine ergonomic gap is that the lookup length `L` cannot be
computed from `Params` fields alone — the consumer must first materialize `TABLE` just to
read `TABLE[N-1]` (finding 2).

## Findings

### F1 — P2 (docs) — The stride-vs-address caveat is repeated near-verbatim in six doc locations; doc weight is disproportionate to the common path

Sites (all read this session):

1. Crate-root doc — `src/lib.rs:30-34` ("The classifier picks an align-*divisible* stride;
   align-aligned block *addresses* additionally require the caller's carve base …")
2. `Params::min_block` — `src/lib.rs:66-70` ("every stride PRESERVES whatever
   `min_block`-alignment the caller's carve base already has …")
3. `SizeClasses::build` — `src/lib.rs:525-529` ("block ADDRESSES are aligned only if the
   caller's carve base is …")
4. `class_for` prose — `src/lib.rs:651-657` ("The divisibility conjunct is a STRIDE
   property, not an address guarantee …")
5. `class_for` fast-path paragraph — `src/lib.rs:659-663` ("(Block ADDRESSES are
   `min_block`-aligned only if the carve base is — same base-alignment precondition …)")
6. `class_for` bolded precondition — `src/lib.rs:717-735` ("**The carve base must also be
   `align`-aligned.** …")

Plus near-verbatim echoes in `README.md:24-28` and `CHANGELOG.md:62-63,74-78`, and a
seventh, internal-code-comment copy at `src/lib.rs:221-233`. That is the exact
"same distinction repeated near-verbatim in 5+ places" shape: each repetition was
presumably the right local fix for one audit finding; taken together they are noise.

Related sub-issues in the same document (`class_for`, `src/lib.rs:645-735`):

- The `# Preconditions` flow is interrupted: the `align`-pow2 contract (`:681-684`) is
  followed by the `class_for(0, 0)` digression (`:686-699`), then *resumes* align
  violation behavior (`:701-715`). A first-time reader whiplashes between two different
  precondition topics interleaved.
- `build_size2class`'s doc spends `src/lib.rs:361-396` (36 lines) on the top-bucket
  sentinel and hand-built-table edge cases before the one-sentence purpose is done. The
  content is accurate (I re-derived the clamp at `:472-475`); the proportion is not.

**Why a first-time reader hits it:** the docs.rs page is the product for this crate; the
signal-to-caveat ratio inverts about halfway through `class_for`'s doc. Everything is
individually correct, so no single site is "wrong" — which is exactly why repeated
narrow-lens passes kept each one while never weighing the total.

**Recommended fix:** state the base-alignment precondition once, fully, in `class_for`'s
`# Preconditions` (it is the natural entry point), and reduce every other site to a single
intra-doc link ("see `class_for`'s base-alignment precondition") — that is what
`[`class_for`](Self::class_for)` links are for. Move the `(0,0)` case to the end of the
preconditions section. This is a pure doc refactor; nothing else changes. Reasonable to
land after 0.1.0, but it is the highest-value doc change available.

### F2 — P3 (docs + API ergonomics) — `L` is not actually derivable from `Params` alone; the crate doc overpromises, and rustdoc contains no construction recipe

- Crate doc, `src/lib.rs:45-48`: "Both [`N` and `L`] are pure functions of the [`Params`]
  — a consumer computes them as `const` expressions (see [`size2class_len`]) so nothing is
  dynamic."
- `size2class_len(max_class, min_block)` (`src/lib.rs:144`) takes `max_class` as an
  *input*, and no public `const fn` computes the geometric run's last value from
  `Params`. `L = size2class_len(table[N-1], min_block)`, so the consumer must first build
  a scratch `TABLE` via `build_table` merely to read its last entry — and then
  `SizeClasses::build` (`src/lib.rs:533-535`) rebuilds the same table internally. The
  canonical pattern (correct, but a dance) is spelled out only in `README.md:39-58` and
  `tests/common/mod.rs:26-31`: `const TABLE` → `const L = size2class_len(TABLE[N-1], …)` →
  `static SC: SizeClasses<N, L> = SizeClasses::build(PARAMS)`.

Mathematically `L` *is* a function of the params, so the doc sentence is defensible; but
"computes them as const expressions (see size2class_len)" suggests a one-step derivation
that does not exist. A first-time user working from the docs.rs page alone (which is what
crates.io serves) gets **no worked example at all** — the only example lives in the README
as a ```text fence (deliberately not a doctest, per this repo's policy) — and must invent
the TABLE-then-L dance or already have read the README.

**Recommended fix (either or both):**
1. Add a `pub const fn max_class(params: Params) -> usize` (loop the widened advance,
   ~10 lines, same checks as `build_table`'s core) so `L` is derivable in one expression
   and the scratch `TABLE` disappears from consumer code. This is also a nicer story than
   "build the table twice".
2. At minimum, copy the README's construction recipe into the crate-level rustdoc as a
   ```text block (repo policy forbids runnable fences, not text ones) and reword
   `src/lib.rs:45-48` to say `L` is derived from the built table's largest class.

### F3 — P3 (release hygiene) — Internal audit/task references and two Cyrillic fragments ship in the published tarball; CHANGELOG still says "Unreleased"

- `CHANGELOG.md:28-31` cites "task #731" and "the publication audit's P2-1/P2-2
  findings"; `CHANGELOG.md:47-49` cites "task #728"; `:73` "task #729". External readers
  cannot resolve any of these.
- `tests/builder.rs:261-262` cites
  `docs/reviews/2026-08-06-size-classes-publish-readiness-review.md §5.1` — a repo path
  that does **not** exist inside the published `.crate` tarball (tests/ and benches/ are
  packaged; `docs/` at the workspace root is not). Same shape at `builder.rs:491, :596`
  and `proptest`/bench comments (`benches/size_classes_bench.rs:43-54` cite review-2 F2).
- Two mixed Cyrillic fragments in shipped test comments: `tests/builder.rs:337`
  ("соседний непроверенный случай") and `tests/builder.rs:568` ("contrpпример" — mixed
  Latin/Cyrillic). Found by `rg -n "[А-яё]"` over the crate; those two lines are the only
  hits in shipped source.
- `CHANGELOG.md:7` reads `## 0.1.0 - Unreleased` — fine now, but should be dated when the
  version is published (the only item here that is genuinely time-locked to the release).

None of this affects behavior; all of it makes the tarball read like an internal working
tree. **Fix:** a light editorial pass (English-only, genericize or drop internal IDs —
"an earlier internal audit" is enough), and date the 0.1.0 heading at publish.

### F4 — P4 (docs) — "SEFER" is never defined for a standalone audience

`src/lib.rs:29-31` and `README.md:23-24` motivate the slow path with "SEFER's own
motivating case: `align >= 512`". On crates.io the crate's docs are read standalone;
"SEFER" is this repository's allocator, but neither the crate doc nor the README says so
(it is only inferable from the repository URL). One clause — "the author's own allocator
(`sefer-alloc`)" — fixes it.

### F5 — P4 (API) — Derived `Debug` dumps both full tables

`src/lib.rs:508` derives `Debug` on `SizeClasses`, which embeds `[usize; N]` and
`[u8; L]` — for the sefer scheme ~16 KiB + ~16 KiB of numbers. Any accidental
`dbg!`/`{:?}` print (a failed assert in a downstream test is the likeliest) floods the
output and buries the useful line. A hand-written `Debug` printing only
`(N, L, min_block, small_max, huge_threshold)` would be friendlier. The `Clone`-but-not-
`Copy` decision is well-argued (`src/lib.rs:501-507`); `Debug` just didn't get the same
treatment.

### F6 — P4 (API nits, bundled)

- **Parameter-passing inconsistency:** `build_table` takes `&Params` (`src/lib.rs:195`)
  while sibling `SizeClasses::build` takes `Params` by value (`src/lib.rs:533`).
  Harmless (`Params: Copy`) but noticeable in the README example where both appear four
  lines apart.
- **`small_align_max` is a stored field and public accessor that is always exactly
  `min_block`** (`src/lib.rs:514, :542, :606-612`) — dead configurability today,
  documented as such, tested at `builder.rs:82`, and hinted as a future knob
  (`README.md:33-35`). Fine to keep; noting because a fresh reader wonders why a field
  stores a constant.
- **`growth = (1, 1)` behaves like `(0, den)` but only the latter is documented as
  deliberately valid:** any `num <= den` makes the geometric term `<= prev`, degrading to
  the linear `min_block`-step table via the fallback at `src/lib.rs:316-325`; the doc at
  `src/lib.rs:165-169` blesses only `num == 0` explicitly. Doc completeness nit only —
  the behavior is identical and safe.

## What I verified as clean (with method)

- **`#![forbid(unsafe_code)]` is airtight.** `rg -n "unsafe" crates/size-classes/` returns
  only doc-prose matches (the word in descriptions) and the attribute itself at
  `src/lib.rs:50`; there is no `unsafe` block, no macro that could emit one, **zero
  runtime dependencies** (`Cargo.toml [dependencies]` is empty), and `forbid` (unlike
  `deny`) cannot be re-allowed by an inner attribute. Dev-dependencies (`proptest`,
  `bench-scale-tool`) never reach consumers.
- **Panic / DoS surface.** Every production `panic!`/`assert!` is construction-time
  parameter validation (`build_table`, `build_size2class`, `size2class_len` — 26 distinct
  `#[should_panic]` tests pin their messages) or standard slice-index OOB in
  `block_size` (`src/lib.rs:634-636`, documented `# Panics`). The only per-request
  methods, `class_for` and `is_huge`, cannot panic for `Layout`-derived inputs: `align`
  from `Layout` is a power of two ≥ 1 by construction, so the `need - 1` at
  `src/lib.rs:746` never underflows, and `need > small_max` (`:743`) rejects before any
  indexing. Non-pow2 `align` is `debug_assert`-only with all three failure modes honestly
  documented (`:701-715`); `class_for(0, 0)`'s release-mode wrap behavior is documented at
  `:686-699`. This is a reasonable stance for a size-arithmetic crate.
- **Correctness re-derivation (by reading, not executing).** I independently re-derived:
  the sorted-merge termination and strict-increase guarantee (`:271-333` — with the
  u128-widened advance and its two checked ops, `:302-314`); the monotone-pointer LUT
  build and top-bucket clamp (`:456-485`); fast-path divisibility (all classes are
  `min_block` multiples, `min_block` pow2 ⇒ multiple of any pow2 `align ≤ min_block`);
  slow-path jump equivalence (the next multiple strictly above `block` bounds away every
  skipped non-divisible class, and the re-seeded index strictly advances ⇒ termination);
  and the u8 index bound (`N ≤ 256`, indices `0..=255`). No defect found. The
  extreme-value cases (`min_block = 1<<62/63`, `usize::MAX` tables) are pinned by the
  `extreme64_overflow` module and 64-bit-gated tests.
- **Packaging metadata, end to end.** `repository`/`homepage` in `Cargo.toml:9-10` match
  the actual remote (`git remote -v` → `https://github.com/PHPCraftdream/sefer-alloc.git`)
  and the actual branch (`main`); `documentation = "https://docs.rs/size-classes"` is the
  standard auto-build URL and the crate needs no features for it. `license = "MIT OR
  Apache-2.0"` matches both shipped files; `LICENSE-APACHE` (201 lines, appendix present)
  and `LICENSE-MIT` (21 lines, "Copyright (c) 2026 sefer-alloc contributors") are
  byte-identical to the workspace root's (`cmp`, both clean). Description is 283 chars,
  single line. Exactly 5 keywords, each ≤ 20 chars; all three categories
  (`memory-management`, `data-structures`, `no-std::no-alloc`) are valid slugs. No
  `exclude` needed — the crate directory contains only publish-relevant files (the empty
  `.rush/` tool dir contributes nothing; cargo archives files, not empty dirs).
- **Crates.io availability (network check, read-only):** `GET /api/v1/crates/size-classes`
  → 404 — the name is free, so no collision and no existing dependents to break. The
  dev-dependency `bench-scale-tool = "0.1"` resolves: it is published (0.1.0, same owner
  `PHPCraftdream`), so `cargo publish`'s verification build can resolve dev-deps.
  `proptest = "1"` is obviously fine.
- **MSRV (`rust-version = "1.88"`).** The strongest library-side requirements are const
  `Option::expect` (`src/lib.rs:324`, const-stable 1.83) and `u128::div_ceil`
  (`src/lib.rs:303`, 1.73); dev-only code uses `usize::is_multiple_of` (1.87 — the tests
  genuinely need 1.87, so the 1.88 pin is honest, not decorative). I found no post-1.88
  syntax anywhere (no let-else, no let-chains, no edition-2024 constructs, no unstable
  const generics — only path/literal const args). Edition 2021 is consistent.
- **Test/bench quality.** 42 `#[test]` fns (38 in `builder.rs` incl. cfg-gated 64-bit and
  debug-assertion-gated ones; 4 in `proptest_builder.rs`), 26 `#[should_panic]`s each
  pinning a distinct message substring so a panic from the wrong chokepoint cannot
  satisfy the test; a hand-derived golden geometric run (`builder.rs:456-478`) guarding
  against circular-oracle drift; three independent reference implementations (reference
  table, walk, scan); the README example mirrored in a compiled test
  (`builder.rs:1022-1043`); the bench's slow-path rows protected by a path-activation
  oracle (`builder.rs:218-250`) with the `(size, align)` pairs mechanically shared via
  `tests/common/mod.rs`. The `#[cfg(debug_assertions)]` gate on the `debug_assert`
  should-panic test (`builder.rs:827`) correctly keeps `cargo test --release` green. This
  is a model small-crate suite.

## Claims I could not verify in this mode (stated plainly)

- I ran no compiler of any kind, so "it compiles / tests pass / rustdoc links resolve /
  `cargo publish --dry-run` succeeds" is **not verified** by me; the README example's
  numeric validity (no extras/geometric collision) rests on the mirrored compiled test,
  not on my execution.
- `src/lib.rs:140-142` asserts that release-profile **const-eval** arithmetic overflow
  follows the `overflow-checks` profile (i.e. wraps) for a `const fn` body, unlike literal
  const expressions. This is a real rustc quirk, but I could not re-verify it empirically
  here. The crate's own tests claim it was empirically verified
  (`tests/builder.rs:553-555` "task #1423/#1431, empirically verified"; reiterated at
  `:661-664`), and the claim is only load-bearing for a doc justification — the
  `checked_add` is unconditionally correct. If that empirical verification ever turns out
  wrong, the doc sentence overstates the hazard but nothing behavioral changes.
- `benches/size_classes_bench.rs:25` passes `env!("CARGO_MANIFEST_DIR")` into the harness;
  whether `bench-scale-tool` writes artifacts relative to it (relevant only to someone
  running `cargo bench` from the registry cache) is outside this crate and unverified.

## Scope overlaps

The production consumer (`sefer-alloc`'s path dep and the thin shim at
`src/alloc_core/size_classes.rs`, root `Cargo.toml:920`) is the assigned scope of
`size-classes-review2-consumer-integration`; I confirmed only that it exists and uses
`class_for(size.max(MIN_BLOCK), align)` at `src/alloc_core/alloc_core.rs:2252`, without
evaluating its satisfaction of the carve-base precondition. The recent fix wave is the
regression-hunt agent's scope; I noticed nothing regression-shaped.
