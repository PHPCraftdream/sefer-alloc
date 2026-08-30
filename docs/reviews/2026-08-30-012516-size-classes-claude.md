# `crates/size-classes` — independent blind review (Claude, 2026-08-30 01:25:16)

Scope: `crates/size-classes/{src/lib.rs, tests/**, benches/size_classes_bench.rs, README.md,
CHANGELOG.md, Cargo.toml}` plus the in-tree consumers `src/alloc_core/size_classes.rs` and
`src/alloc_core/segment_layout.rs`. Method: static read + hand re-derivation of every numeric
claim in the docs, tests and bench (the SEFER table, all five `JUMP_*` fixtures and their exact
iteration counts, both `geo_count` overflow boundaries, the LUT byte figures, the 94.5% sparsity
figure, the `EXTREME64` schemes). No prior review material was consulted; `docs/reviews/`,
`docs/checkpoints/` and both OPEN_ITEMS indexes were not opened.

---

## Verdict: **CONDITIONAL-GO**

The library code is, as far as I could verify, **correct**. I re-derived every arithmetic claim
in the crate's docs and tests independently and found no discrepancy: the `seed_idx >= L - 1`
index-space guard is exactly equivalent to `need > small_max`; the slow-path `(block | (align-1))
>> shift` re-seed is exactly the removed `((block | (align-1)) + 1) - 1 >> shift`; the jump loop
provably advances `i` strictly and provably terminates without ever falling through to the
trailing `None`; `try_class_for`'s "never panics, for any `(size, align)`" claim holds for every
input; the `u128` widening and the min-step fallback are both correctly checked; the documented
`geo_count` = 182/183 (64-bit) and 83/84 (32-bit) boundaries are right; the `L = 16173` /
`L = 18207` / `392 bytes` / `~16.18 KiB` / `~16.20 KiB` / `94.5%` / `15285 of 16173` figures are
all right; every `JUMP_*` fixture's stated seed class, block size, iteration count and result is
right, including the skipped indices `37, 38, 40` in `JUMP_NONE`. **No P1 findings.**

What holds the verdict back from a clean GO is not correctness but **documentation mass and the
shape of what ships**. `src/lib.rs` is now 65.3% comment (621 doc/comment lines around 330 lines
of code), and that ratio has ratcheted *upward* on every one of the last six review rounds while
the code line count stayed frozen at 330 — the docs are absorbing review dialogue rather than
converging. Two further gaps are worth closing before a first publish: an undocumented ≥`L`-byte
by-value stack materialisation in `SizeClasses::build` (in a crate whose front page advertises
`no_std`), and a shipped test file that cites four `docs/reviews/*.md` paths and two rotted
`lib.rs:NNN` line numbers that no crates.io consumer can resolve. All P2/P3 items below are
doc/test/packaging edits; none requires touching the algorithm.

---

## P1 — blocking correctness / safety

**None.** I actively hunted for one. The places I expected to find something and did not:

- `class_for`'s `L - 1` guard cannot underflow: `build_size2class` asserts
  `L == size2class_len(small_max, min_block)`, which is `>= 1` by construction, and `build_table`
  always produces `table[0] == min_block`, so `L >= 2` for any `SizeClasses` reachable through
  the public API (`src/lib.rs:897`).
- The slow-path loop terminates: for `next_idx < L - 1`, `need` at bucket `next_idx` is
  `>= next_mult > table[i]`, so the resolved index is strictly `> i`
  (`src/lib.rs:930-934`). The trailing `None` at `src/lib.rs:941` is genuinely unreachable, as its
  comment claims.
- `build_size2class`'s `(k + 1).checked_mul(min_block)` clamp is correct in both directions,
  including the `usize`-overflowing top bucket (`src/lib.rs:525-528`), and the hand-built
  `[1<<62, 2<<62, (3<<62)+2, (3<<62)+5]` regression at `tests/builder.rs:1060-1086` really does
  distinguish the fixed `[0,1,2,3]` from the pre-fix `[0,1,2,2]`.
- `build_table`'s merge cannot emit a duplicate without the monotonicity assert at
  `src/lib.rs:395-409` catching it (the `cur == extras[ei]` case emits the extra, then re-emits
  the same `cur`).

---

## P2 — important

### P2-1. `SizeClasses::build` materialises the whole scheme by value; the ≥`L`-byte stack cost is undocumented in a `no_std` crate

**Location:** `crates/size-classes/src/lib.rs:628-647` (`pub const fn build(params: Params) -> Self`),
doc at `:616-627`.

**What's wrong.** `build` returns `Self` by value. In a `static`/`const` initialiser — the only
usage the docs show — this is const-evaluated and free. But nothing in the signature, the doc, or
the type system restricts it to that context, and the crate's own test suite calls it at runtime
(`tests/builder.rs:918-920`, `extreme64_scheme_runtime()`). A runtime call materialises the entire
object on the stack: 16,584 bytes for the crate's own documented "realistic scheme", and 64 KiB+
for the in-tree consumer's `medium-classes` configuration (`S2C_LEN` = 65537 there, per
`src/alloc_core/size_classes.rs:217`).

**Why it matters.** The crate's headline claim is `no_std` (`src/lib.rs:8`, `README.md:6`,
`Cargo.toml:13` `no-std::no-alloc`), and this repo's CI actually builds it for
`thumbv7em-none-eabi` (`.github/workflows/ci.yml:1832`) — a target class where a 16 KiB stack
frame is a hard fault, not a slowdown. The doc says "Build a scheme from [`Params`] at compile
time", which *describes* the intent but does not *warn* about the alternative, and
`#![forbid(unsafe_code)]` means there is no in-place `build_into(&mut MaybeUninit<Self>)` escape
hatch a user could reach for. A `no_std` user who writes `let sc = SizeClasses::<N, L>::build(P);`
inside a function gets no diagnostic at all.

**Suggested fix.** Add two sentences to `build`'s rustdoc, immediately after the first line:
"Intended for a `static` (or `const`) initialiser, where this is const-evaluated and free. Calling
it at *runtime* materialises the entire object — at least `L` bytes, ~16 KiB for a realistic
scheme — by value on the caller's stack; on a small-stack `no_std` target, prefer a `static`."
Mirror one clause of it in `README.md`'s "Memory cost" section, which currently discusses only the
static footprint.

### P2-2. Published rustdoc is 65.3% comment and has grown on every review round with zero code change

**Location:** `crates/size-classes/src/lib.rs` (whole file).

**What's wrong.** Measured directly from git (`git show <sha>:crates/size-classes/src/lib.rs`,
counting lines whose first non-whitespace characters are `//`):

| commit | total | doc/comment | code | comment % of non-blank |
|---|---|---|---|---|
| `121d657` (extraction) | 386 | 176 | 190 | 48.1% |
| `3449ced` ("trim review-response prose") | 958 | 597 | 330 | 64.4% |
| `5341c6b` | 981 | 614 | 336 | 64.6% |
| `4c332ab` | 977 | 616 | 330 | 65.1% |
| `fcc0280` | 965 | 604 | 330 | 64.7% |
| `aca5c1e` | 980 | 619 | 330 | 65.2% |
| `HEAD` | 982 | 621 | 330 | **65.3%** |

Code has been frozen at exactly **330 lines** for five consecutive rounds while doc/comment lines
went 597 → 621. The commit whose stated purpose was to *trim* review-response prose left the file
at 64.4%, and every round since has added net prose. Concrete passages that are review dialogue
rather than API documentation, still shipped:

- `:63-64` — "(Runnable form with concrete values in the crate's `README.md`, mirrored by a
  compiled test so it cannot silently rot.)" Test-suite meta-commentary on the crate's front page.
- `:178-185` — a 8-line defence of *why* `size2class_len` bothers to check its `+ 1`, complete
  with a link to `rust-lang/rust#74823`, inside a `# Panics` section. A `# Panics` section states
  what panics, not the history of the argument about it.
- `:730-732` — `small_align_max()` is "kept as a distinct accessor NAME because it anticipates
  becoming a genuinely independent `Params` field later, not because it is a distinct value
  today." That is a design-decision record, not something a caller of `small_align_max()` needs.
- `:766-769` — `huge_threshold()`: "a caller that needs to report or log the threshold *no longer
  has to* keep its own separate copy of it." A changelog framing ("no longer") in the API doc of a
  crate that has never shipped a prior version.
- `:949-952` — "the crate's own benchmark suite has a `try_class_for/*` row for this if you want
  to measure the difference on your own target." Points a docs.rs reader at a bench row that is
  not in the rendered documentation.
- `:820-841` and `:843-854` — ~30 lines of `class_for`'s doc are a three-profile behavioural
  breakdown of `align == 0, size == 0` plus an enumeration of three wrong-answer modes, all for
  inputs the same doc has already declared out of contract two paragraphs above.
- `:657-702` — the `size2class()` accessor (a 3-line getter) carries 46 lines of doc, which
  restate "prefer `class_for`" three separate times.

**Why it matters.** docs.rs is this crate's storefront and its API surface is nine functions. A
reader landing on `class_for` has to scroll ~95 lines of prose before reaching `try_class_for`.
More structurally: the ratchet is the symptom. Each round's finding becomes a new paragraph
instead of a shorter one, so the doc is now a transcript of six reviews rather than a
specification.

**Suggested fix.** One dedicated pass with a *budget*, not another round of additions: target
≤ 55% comment (roughly −100 doc lines). Concretely — move the rationale passages above into
`CHANGELOG.md` (which already has a "why" register) or drop them; collapse `class_for`'s
out-of-contract sections into one short paragraph plus "prefer `try_class_for`"; cut
`size2class()` to ~12 lines (two preconditions + one "prefer `class_for`" + the stability note).
Then add the budget as a check so it cannot ratchet again — the awk one-liner used for the table
above is a two-line CI step next to the existing `cargo doc -p size-classes` row at
`.github/workflows/ci.yml:1884`.

---

## P3 — quality / perf / smell

### P3-1. The struct's own perf comment over-counts what the field removal saved

**Location:** `crates/size-classes/src/lib.rs:573-576`, specifically `:575`.

**What's wrong.** The comment claims that storing only `min_block_shift` "removes **two**
provably-redundant hot-path field loads". Verified against the pre-refactor source
(`git show 0453cc4^:crates/size-classes/src/lib.rs`): the removed fields were `min_block` and
`small_align_max`, and the old `class_for` body read `self.small_align_max` at exactly **one**
site and `self.min_block` at **zero** sites (`min_block` was only ever read by the `min_block()`
accessor). So one hot-path load was removed, not two. Further, that one load was traded for
`1usize << self.min_block_shift` (`:901`) — an ALU shift on a field that must be loaded anyway —
so the net change on the hot path is "one load → one shift", worth stating accurately in a comment
that exists specifically to justify a perf trade.

**Why it matters.** This is the only performance claim in the shipped source that is stated as
fact rather than as a hypothesis, and it is the one an optimiser-minded reader will check first.
(The genuine two-load saving on that path came from removing `small_max`, which this comment does
not name — see `:887-895`, where the index-space guard's own comment correctly explains it.)

**Suggested fix.** "…storing only the shift removes one hot-path field load (`small_align_max`)
and one accessor-only field (`min_block`), at the cost of re-deriving `1 << shift` at the single
site that needs it; `min_block()`/`small_align_max()` re-derive it."

### P3-2. `class_for`'s "Fast path" paragraph under-states which invariant the LUT seed depends on

**Location:** `crates/size-classes/src/lib.rs:794-797` (fast-path paragraph) and `:896-900`
(seed computation).

**What's wrong.** The doc attributes the "every class SIZE is a multiple of `min_block`" invariant
solely to the *divisibility* conjunct. It is equally load-bearing for the *size* conjunct: the LUT
resolves a bucket to "the smallest class `>= (k+1)*min_block`", i.e. to the bucket's **top**, not
to `need`. That answer is the smallest class `>= need` only because no class value can fall
strictly inside `[need, (k+1)*min_block)` — which holds precisely because every class is a
`min_block` multiple. Without it, the seed can be a *fitting but not smallest* class.

The crate already documents the phenomenon, but only in a different place and in different words:
`build_size2class`'s rustdoc at `:437-449` works it out for `table = [16, 24, 32]` (bucket
`(16, 32]` resolves straight to `32`, leaving `24` unreachable), and correctly notes that no
public constructor feeds a hand-built table into `class_for`. So this is not a bug — it is a
doc-precision gap at the exact point a future `SizeClasses::from_table` constructor would land.

**Why it matters.** `build_size2class` is deliberately exposed as a standalone building block that
"need not come from [`build_table`]" (`:438`). The next person who wires those two together —
which the crate's own API shape invites — will read `class_for`'s fast-path paragraph, see
"divisibility", conclude a non-`min_block`-multiple table is merely wasteful, and ship a
silently-non-minimal classifier.

**Suggested fix.** One clause: "…every class SIZE is a multiple of `min_block`, which does two
things: the stride divisibility check is trivially satisfied, **and** the bucket-top answer the
lookup returns is the smallest *fitting* class (no class value can lie between `need` and its
bucket's top). Both fail for a table not built by [`build_table`]."

### P3-3. `#[inline]` on `class_for` inlines the entire jump loop into every call site *(speculative — needs a bench gate)*

**Location:** `crates/size-classes/src/lib.rs:880-942`; `try_class_for` at `:971` compounds it.

**What's wrong.** `class_for` is one `#[inline]` function containing both a ~6-instruction fast
path and a variable-trip loop with two array indexings, a mask test and a re-seed. In the in-tree
consumer, `SizeClasses::class_for` is on the hot `alloc` path
(`src/alloc_core/size_classes.rs:300-302`) where — for any workload not using `align > 16` — the
loop body is 100% cold. `try_class_for` is also `#[inline]` and delegates to `class_for`, so a call
site using both gets the loop expanded twice.

**What a benchmark would show.** I cannot run one here, so this is a hypothesis, not a result. The
mechanism to test: split into `#[inline] pub const fn class_for` holding only
`need`/`seed_idx`/guard/`size2class[..]`/fast-path-return, plus an
`#[inline(never)] const fn class_for_slow(&self, seed: usize, align: usize) -> Option<usize>`
carrying the loop. Expected direction: no change on the existing
`class_for/small_hit` row (same instruction sequence), a small regression on
`class_for/large_align_slow_path*` (one added call), and — the actual target — reduced I-cache
pressure at the `AllocCore::alloc` call site, which the crate's own microbenches cannot observe at
all. The honest framing is that this is only measurable in the *consumer's* `npm run iai` Ir
numbers, not in `size_classes_bench`.

**Suggested fix.** Either do the split behind a consumer-side iai gate, or — if the gate is not
worth the round — add one line to the file noting that the current shape is a deliberate,
unmeasured choice, so the next reviewer does not re-derive the same hypothesis.

### P3-4. The flat byte-per-bucket LUT is ~13× larger than it needs to be *(speculative — needs a bench gate)*

**Location:** `crates/size-classes/src/lib.rs:462-539` (`build_size2class`), sparsity already
documented at `:163-168`.

**What's wrong.** The crate's own doc states the problem precisely: for the realistic scheme,
buckets `888..=16172` — 15285 of 16173, 94.5% — resolve to just 14 class indices, because above
~14 KiB the class spacing widens to ~1.25× while the LUT's resolution stays a flat `min_block`.
The doc concludes that the flat shape is "the only shape that stays O(1) for arbitrary `extras`"
(`:695-697`). That conclusion is stronger than the evidence supports.

**A shape that keeps exact O(1) and arbitrary `extras`.** Split the LUT at a const-derived
threshold `T`: fine buckets of `min_block` below `T`, coarse buckets of `B` above it, where `B` is
computed at const time as the largest power of two not exceeding the *minimum class gap* above
`T`. For the realistic scheme with `T = 16384`: the minimum gap above `T` is `17760 - 16384 =
1376`, so `B = 1024`. A coarse bucket then contains **at most one** class value, so
`coarse[j]` (smallest class `>= (j+1)*B`) is either the answer or exactly one too large — resolved
by one compare and one conditional decrement, branchlessly. Size: 1024 fine + 238 coarse ≈
**1262 bytes vs 16173** — ~12.8× smaller, and it fits in L1 alongside the rest of the allocator's
working set instead of contending with it.

**What a benchmark would show.** Nothing on `size_classes_bench` (a 16 KiB table stays hot when it
is the only thing in the loop — the bench's own `jump_vs_walk` comment already makes exactly this
point at `benches/size_classes_bench.rs:118-119`). The measurable effect is in the consumer under a
realistic mixed workload, as a reduction in L1d misses on `alloc`. Mark as unproven.

**Cost and caveats.** `L` is a public const generic and `size2class_len` a public `const fn`, so
this is a breaking change — which is exactly why it belongs *before* 0.1.0, not after. If it is
not taken, the honest edit is to soften `:695-697` from "the only shape that stays O(1)" to "the
simplest shape that stays O(1)", since the piecewise variant above is a counterexample to the
literal claim.

### P3-5. Shipped test file cites four repo-internal review documents and two rotted line numbers

**Location:** `crates/size-classes/tests/builder.rs:478`, `:724`, `:880`, `:1135-1136`.

**What's wrong.** `tests/` is tracked and `Cargo.toml` declares no `exclude`, so these files go
into the crates.io tarball. They contain:

- Four citations of `docs/reviews/2026-08-06-…md`, `…2026-08-07-…md` (twice) and
  `…2026-08-26-102907-…-Sol-codex.md` — paths that exist only in this repository, not in the
  published package.
- `:1135-1136` cites "§F2 (**lib.rs:408**)" and "the companion §B26 (**lib.rs:432**)" for a
  finding about `class_for`'s fit predicate. Today `src/lib.rs:408` is `i += 1;` inside
  `build_table`'s monotonicity loop and `:432` is a line of `build_size2class`'s *rustdoc*.
  `class_for` begins at `:881`. Both line references have rotted.
- More broadly, 53 lines of `tests/builder.rs` and 9 of `benches/size_classes_bench.rs` are
  review-provenance prose ("size-classes publication audit run 2 (Claude, review-2 F6)",
  "rush-tests review T4/task #1479", "MS prepublish review, task #1503 (P2-2)", …).
  `src/lib.rs` is, to its credit, entirely clean of these — the tests are where the archaeology
  accumulated instead.

**Why it matters.** A downstream reader who unpacks the `.crate` to understand a test's intent
follows a dead path. Line-number citations into a file under active edit are a guaranteed-rot
construct regardless.

**Suggested fix.** Replace the four `docs/reviews/…` paths with the *content* they stand for (one
sentence each — several of the surrounding comments already say it, making the path redundant).
Replace `(lib.rs:408)` / `(lib.rs:432)` with the item names (`class_for`'s fit predicate;
`class_for`'s slow-path bitmask). Convert the review-ID prefixes to plain statements of what the
test pins; where a task number carries real information, keep it in the *commit* message, not the
shipped source.

### P3-6. Every consumer hand-writes the same five-const derivation, and the crate documents the boilerplate rather than removing it

**Location:** `crates/size-classes/src/lib.rs:45-64` ("## Deriving lengths"), plus the identical
pattern at `README.md:66-78`, `tests/common/mod.rs:31-48`,
`tests/proptest_builder.rs:36-65` (three times), `tests/builder.rs:357-363` / `:1386-1395`,
`src/alloc_core/size_classes.rs:170-227`.

**What's wrong.** Instantiating a scheme requires the user to write `PARAMS`, `N`, `TABLE`, `L`,
`SC` in the right order and to get `N == geo_count + extras.len()` right by hand. The crate treats
this as a fact of life: the front page spends a whole section explaining that "there is no
shortcut around building `TABLE` once to read it", there is a dedicated assert for the `N`
mismatch (`:270`), a dedicated `# Panics` bullet for it (`:219`), and a dedicated test
(`tests/builder.rs:630-637`, whose own comment calls it "the single likeliest real-user error").

**Why it matters.** A `macro_rules!` removes the failure mode entirely rather than diagnosing it,
and removes one of the two `build_table` const-evaluations as a side effect:

```rust
size_classes::scheme! {
    pub static SC = Params::new(MIN_BLOCK, (5, 4), GEO_COUNT, EXTRAS, HUGE_THRESHOLD);
}
```
expanding to the same five items with `N` and `L` derived, never typed. This is a purely additive
API (a later addition is not breaking), so it is not freeze-critical — but it is the single
largest ergonomic win available, and it would let "## Deriving lengths" shrink to three lines.

**Suggested fix.** Add the macro; keep the manual form documented for users who need the
intermediate `TABLE`/`L` constants; shorten the front-page section to point at the macro first.

### P3-7. ~45 doc lines, one clamp branch and three tests exist to define a bucket `class_for` provably never reads

**Location:** `crates/size-classes/src/lib.rs:418-433` (top-bucket clamp doc), `:517-528` (the
clamp), `:668-690` (the "false sentinel" half of `size2class()`'s doc); tests
`tests/builder.rs:238-274`, `:303-315`, `:922-985`, `:1032-1087`.

**What's wrong.** For any `SizeClasses` (always `build_table`-derived, so `small_max` is always a
`min_block` multiple), the largest bucket `class_for` ever indexes is `L - 2`: `need == small_max`
gives `seed_idx == L - 2`, and anything larger is rejected by the guard. Bucket `L - 1` is
therefore dead weight — one unread byte — and yet it is the single most-documented aspect of the
crate, because it *is* reachable for the standalone `build_size2class` path with a hand-built
non-aligned `small_max`.

**Why it matters.** The complexity is real (the `checked_mul`-folds-into-the-clamp reasoning at
`:521-524` is genuinely subtle) and it is paid entirely for a code path no `SizeClasses` user can
reach. This is the kind of asymmetry that is cheap to fix pre-0.1.0 and expensive after.

**Suggested fix.** Pick one and commit to it: (a) require `small_max % min_block == 0` in
`build_size2class` (which `build_table` already guarantees, so no in-tree caller is affected) — the
top bucket becomes unambiguously a sentinel, `:430-433` and roughly half of `size2class()`'s doc
delete themselves, and the `(3<<62)+5` test at `:1032` becomes a `#[should_panic]`; or (b) keep the
permissiveness and compress the documentation to one sentence plus a pointer. Doing neither leaves
the crate's most-explained feature also its least-used one.

### P3-8. `tests/common/mod.rs` forces the 16 KiB SEFER const-eval on a consumer that only needs one function

**Location:** `crates/size-classes/tests/common/mod.rs:31-48` (heavyweight fixtures) vs `:93-119`
(`walk_class_for`), consumed at `tests/proptest_builder.rs:20-21`.

**What's wrong.** `proptest_builder.rs` imports only `walk_class_for`, but `mod common;` compiles
the whole file, and `static SEFER_SC` is const-evaluated whether referenced or not — a full
`build_table` (49 classes) plus `build_size2class` (16173 buckets) per test binary, plus the same
again for `SEFER_TABLE`. The module doc at `:15-20` notes the `#![allow(dead_code)]` consequence
but not the const-eval one.

**Suggested fix.** Split into `tests/common/mod.rs` (declaring `pub(crate) mod walk;` and
`pub(crate) mod sefer;`) so `proptest_builder.rs` imports only `walk`. Two-line change, removes a
const-eval from one of the three test binaries.

---

## P4 — minor / cosmetic

1. **`CHANGELOG.md:7`** — still `## 0.1.0 - Unreleased`. Release-commit checklist item.
2. **`README.md:54`** — the "full worked comparison" link points at
   `https://docs.rs/size-classes/latest/…`, which 404s until the first publish. Either accept it
   as a post-publish link or say so inline.
3. **`Cargo.toml:15-18`** — `[lints] workspace = true` inherits `check-cfg` declarations for
   `cfg(loom)`, `cfg(kani)`, `cfg(aligned_vmem_page_size_override)` and `cfg(numa_shim_mock)`
   (`Cargo.toml:108` at the workspace root), none of which `size-classes` uses. Publishing is not
   at risk (CI dry-runs it at `.github/workflows/ci.yml:734`, and cargo resolves the inheritance
   when packaging), but the packaged manifest advertises four sefer-internal cfg names to a
   standalone consumer, and `crates/size-classes/` cannot be built if copied out of the workspace.
   The two already-published siblings (`crates/sefer-region/Cargo.toml:27`,
   `crates/aligned-vmem/Cargo.toml:179`) both use a local `[lints.rust]` table instead. Consider
   matching them.
4. **No test pins two semver-visible trait impls.** Nothing anywhere asserts
   `InvalidAlign: core::error::Error` (only `Display`, at `tests/builder.rs:1197-1200`) or
   `SizeClasses: Clone` — the latter being a documented, deliberate decision
   (`src/lib.rs:556-562`, `CHANGELOG.md:106-109`) with no mechanical guard. Two lines:
   `fn _assert<T: core::error::Error + Clone>() {}` style static checks.
5. **`README.md:83-89`** — the example's `fn demo()` is never called, so a reader who pastes it
   into `main.rs` gets a `dead_code` warning on an example whose whole point is copy-paste. Either
   call it from a `fn main()` or inline the body.
6. **Mixed dash conventions inside one doc item.** `src/lib.rs:28` and `:30` use `—` while `:34`
   uses `--`, in the same crate-doc bullet. Same mixing recurs throughout (e.g. `:791` vs `:809`).
   Renders inconsistently on docs.rs.
7. **`src/lib.rs:26-36`** — the crate doc folds `try_class_for` onto the tail of the `class_for`
   bullet with no blank `//!` line, so markdown renders it as one paragraph. `README.md:17-36`
   gives it its own bullet. Match the README's structure; `try_class_for` is the recommended
   default and should not be a trailing sentence of another item.
8. **`src/alloc_core/size_classes.rs:98-99`** — the consumer states the growth formula as
   `round_up(prev * 5 / 4, MIN_BLOCK)`, omitting both the ceiling division and the `min_block`
   minimum step that the crate documents (`src/lib.rs:87-89`). Harmless for this specific scheme
   (with `MIN_BLOCK = 16` and `(5, 4)`, `prev * 5 / 4` is always exact and the min-step never
   fires), but it is the one place a reader would look for the formula and it disagrees with the
   authoritative copy.
9. **Triplicated provenance paragraph.** "Without it, a request whose `align` exceeds what the
   caller's classifier happens to handle silently falls through to the caller's whole-segment
   path — a real bug class in hand-rolled allocators (`sefer-alloc`'s own motivating case … `align
   >= 512`)" appears near-verbatim at `src/lib.rs:26-30`, `README.md:20-24` and
   `CHANGELOG.md:65-70`. Three copies is three places to drift; the CHANGELOG copy is the least
   load-bearing.
10. **`benches/size_classes_bench.rs:1-3`** — the header carries "(task #761)" and the
    non-sequitur "This crate previously had zero benches of its own — it is a lookup-table crate,
    so incorrect perf claims would be particularly misleading." The second clause explains a
    motivation, not the file. Trim to the run commands.
11. **`tests/builder.rs:97-99`** — a hard-wrapped identifier
    (`sefer_growth_geo_count_182_is_` / `the_last_that_fits_on_64_bit`) that no grep or IDE
    "go to test" will match. Keep test-name citations on one line even if it overflows the column.

---

## What I verified and found correct (recorded so a later round need not redo it)

- Merged SEFER table (49 entries, `max_class = 258752`) re-derived by hand from
  `min_block = 16`, `(5, 4)`, `geo_count = 40`, the nine extras — matches.
- `JUMP_A (1025,256)` seed 18/block 1200, 4 iters → `Some(21)`; `JUMP_B (2049,1024)` seed 22/2368,
  3 iters → `Some(25)`; `JUMP_MULTI (513,512)` seed 14/608, 2 iters → `Some(17)`;
  `JUMP_DENSE (129,128)` seed 6/144, 2 iters → `Some(9)`; `JUMP_NONE (16385,16384)` seed 36/17760,
  10 iters visiting `{36,39,41,42,43,44,45,46,47,48}` (skipping 37, 38, 40) → `None` via the
  `next_idx >= L - 1` guard at index 48. All five match `tests/common/mod.rs` and both the bench
  comments and the `sefer_bench_jump_rows_genuinely_exercise_the_slow_path` oracle.
- Align divisibility densities: 15/49 classes divisible by 128 (30.6%), 10/49 by 256 (20.4%) —
  matches `benches/size_classes_bench.rs:183-185`.
- `geo_count` overflow boundaries: the multiplier sequence `m_{k+1} = ceil(5*m_k/4)` compounds the
  ceiling, giving `m_k ≈ 2.7 * 1.25^k` (not `1.25^k`) — which is why 182/183 and 83/84 are right
  where a naive estimate says ~187/188. `tests/builder.rs:760-796` is correct.
- `size2class_len` figures: `258752/16 + 1 = 16173`; `145648/8 + 1 = 18207`; `49*8 = 392`;
  `392 + 16173 = 16565 B = 16.18 KiB`; `size_of::<SizeClasses<49,16173>>() = 16584 B = 16.20 KiB`
  with a 19-byte delta (`u32` + `usize` + 7 padding). All match `src/lib.rs:151-168`.
- Sparsity: class 34 is 14208 and class 35 is 16384, so buckets `>= 888` resolve to indices
  `35..=48` — 14 classes, 15285 of 16173 buckets, 94.51%. Matches `src/lib.rs:164-168`.
- `class_for`'s guard equivalence: `(need-1) >> shift >= L-1  ⟺  need > small_max`, given
  `small_max == (L-1)*min_block`; and `(block|(align-1)) >> shift >= L-1  ⟺  next_mult >
  small_max`. Both exact, including the `usize::MAX` corner.
- `EXTREME64` scheme (`min_block = 1<<62`): `class_for(2<<62, 1<<63) == Some(1)`,
  `class_for(3<<62, 1<<63) == None` via `usize::MAX >> 62 == 3 == L-1`. Matches
  `tests/builder.rs:1012-1030`.
- Hand-built `[16, 24, 32]` → `size2class == [0, 2, 2]` (class 1 unreachable). Matches both
  `src/lib.rs:443-449` and `tests/builder.rs:551-564`.
