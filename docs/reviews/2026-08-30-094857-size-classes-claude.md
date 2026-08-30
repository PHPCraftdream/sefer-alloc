# `crates/size-classes` — independent blind review (2026-08-30 09:48:57)

Reviewer: Claude (Opus), blind pass. No prior review report, checkpoint, or
open-items index was read; every claim below was derived from the code as it
stands at review time and re-verified by hand arithmetic against the actual
`SEFER_*` table, or against the `cargo doc` HTML rustdoc actually emits.

Files reviewed: `crates/size-classes/src/lib.rs`,
`crates/size-classes/tests/{builder.rs,proptest_builder.rs,common/mod.rs}`,
`crates/size-classes/benches/size_classes_bench.rs`,
`crates/size-classes/{README.md,CHANGELOG.md,Cargo.toml}`, plus the in-tree
consumers `src/alloc_core/size_classes.rs` and
`src/alloc_core/segment_layout.rs`.

---

## Verdict: **GO** (conditional only on the two P2 items below, both of which
are documentation/measurement-hygiene, not code)

The algorithmic core is correct. I re-derived the whole 49-entry SEFER table
by hand, replayed the fast path, the slow-path jump, both index-space guards,
the `build_size2class` monotone pointer and its overflow clamp, and every
`JUMP_*` fixture's exact iteration count — all agree with the code and with
the committed test expectations. Specifically verified with no defect found:

- The identity the `seed_idx >= L - 1` guard rests on
  (`small_max == (L - 1) * min_block`) holds for every `SizeClasses::build`
  output, and the guard is **exactly** equivalent to `need > small_max` with
  no off-by-one in either direction (`need == small_max` → `seed_idx == L-2`,
  passes; `need == small_max + 1` → `seed_idx == L-1`, rejects).
- The slow path's `(block | (align - 1)) >> shift` is the correct bucket for
  "the smallest multiple of `align` strictly greater than `block`", and
  because `align` is a power of two `> min_block`, that multiple is itself a
  `min_block` multiple, so `need` reconstructed inside `build_size2class`
  equals it **exactly** — the jump can never overshoot a divisible class.
- Termination is sound (`table[size2class[next_idx]] >= next_mult >
  table[i]` ⟹ index strictly increases), and the `usize::MAX` corner of
  `block | (align - 1)` is genuinely subsumed by the same guard.
- `try_class_for`'s "never panics, for any `(size, align)`" claim is
  airtight: `L >= 2` for every constructible `SizeClasses` (because
  `build_table` forces `table[0] == min_block`), so `L - 1` cannot underflow,
  and every index is bounds-checked against a compile-time constant before
  use.
- All five `JUMP_*` fixtures' documented seed classes, block sizes, iteration
  counts, skipped indices, and results are correct (A=4→`Some(21)`,
  B=3→`Some(25)`, MULTI=2→`Some(17)`, DENSE=2→`Some(9)`,
  NONE=10→`None`, skipping 37/38/40).
- The `# Memory cost` figures are arithmetically correct: `L = 16173`,
  `table` 392 B, `table + size2class` 16565 B ≈ 16.18 KiB,
  `size_of::<SizeClasses<49, 16173>>()` 16584 B ≈ 16.20 KiB; the `min_block =
  8` / 24-class counter-example really does reach `max_class = 145648` and
  `L = 18207` (I replayed all 24 geometric steps).
- The `geo_count = 182`/`183` (64-bit) and `83`/`84` (32-bit) overflow
  boundaries, and the "roughly the last half-dozen steps" widened-arithmetic
  window (`c_175 … c_181`, seven steps), are both correct.
- The bench's align-density figures are correct: 15/49 = 30.6% of
  `SEFER_TABLE` is 128-divisible ("~31%"), 10/49 = 20.4% is 256-divisible
  ("~20%").
- `RUSTDOCFLAGS="-D warnings" cargo doc -p size-classes --no-deps` is clean.

**No P1.** I could not construct any `(size, align)` for which `class_for`
returns a non-fitting class, a non-smallest fitting class, or a spurious
`None`. The two P2s are: one genuine hole in the `# Preconditions` contract
enumeration, and one benchmark row whose measurement cannot support the
mechanism it is named for.

---

## P1 — blocking correctness/safety

None.

---

## P2 — important

### P2-1 — `class_for`'s `# Preconditions` enumeration leaves `align == 0, size >= 1` undefined by omission

**Location:** `crates/size-classes/src/lib.rs:811-828` (specifically the
sentence beginning at line 814, and the one beginning at line 819).

**What's wrong.** The `# Preconditions` section enumerates the
release-profile behavior for a contract violation in exactly two buckets:

```
/// With `debug_assertions` off, the behavior for a non-zero
/// non-power-of-two `align` is UNSPECIFIED: ...
...
/// The `align == 0, size == 0` corner does NOT panic, but only with
/// `overflow-checks` ALSO off ...
```

The word **`non-zero`** in the first sentence carves `align == 0` out of the
"unspecified" clause entirely, and the second sentence then covers only the
`size == 0` half of that carve-out. The remaining case — `align == 0` with
`size >= 1` — is described by neither sentence, in a section that is
otherwise exhaustively case-split by profile.

**Why it matters.** This is the crate's single most contract-sensitive doc
section: it is the only place a caller who deliberately uses the unchecked
variant learns what a violation costs. The omitted case is not a hypothetical
— it is what an FFI shim or a hand-rolled `Layout` most plausibly produces
(`align` defaulted to `0` for "no alignment requirement", with a real `size`).
Actual behavior today: `need = max(size, 0) = size`, no underflow, `align (0)
<= min_block` is **true**, so the fast path returns
`Some(smallest class >= size)` — a confident "fits" answer for a request that
is unsatisfiable by the function's own documented predicate (`block % 0` is
undefined). A reader who applies the two documented sentences literally
concludes "`align == 0` is the corner covered by sentence two", and sentence
two is about `size == 0`.

**Suggested fix.** Widen the first sentence and re-scope the second, e.g.:

```
/// With `debug_assertions` off, the behavior for ANY non-power-of-two
/// `align` -- including `0` -- is UNSPECIFIED: ...
/// The one non-power-of-two input with a SPECIFIED outcome is the
/// `align == 0, size == 0` corner, which does NOT panic, but only with
/// `overflow-checks` ALSO off ...
```

That is a two-word edit plus one clause, and it closes the hole without
adding a case.

---

### P2-2 — the `jump_vs_walk` bench pair is measured on the one fixture where the jump skips **zero** classes

**Location:** `crates/size-classes/benches/size_classes_bench.rs:113-145`
(the `jump_vs_walk_a_jump` / `jump_vs_walk_a_walk` pair), with the fixture at
`crates/size-classes/tests/common/mod.rs:54` and the reference walk at
`crates/size-classes/tests/common/mod.rs:93-119`.

**What's wrong.** `JUMP_A = (1025, 256)` seeds class 18 (block 1200) and the
jump visits 18 → 19 → 20 → 21. A step-by-1 walk visits 18, 19, 20, 21 —
**the identical set**. The bench's own comment says so ("the jump skips
nothing here"), which is commendable honesty, but the consequence is that the
row named `jump_vs_walk` measures *no jump-vs-walk difference at all*. What
it does measure is the three primitive differences the comment also lists:
`walk_class_for` uses a runtime-divisor `is_multiple_of` where `class_for`
uses a bitmask, indexes `&[usize]` (bounds-checked) where `class_for` indexes
a fixed-size array field, and recomputes `min_block.trailing_zeros()` per
call.

**Why it matters.** "Skips whole runs of non-divisible classes instead of
stepping by one" is the crate's headline mechanism — it is in the crate
doc (`src/lib.rs:24-26`), the README (lines 17-21), the CHANGELOG (lines
62-68), and `class_for`'s own rustdoc (lines 793-796). The claim is *proved*
by iteration count (the `sefer_bench_jump_rows_genuinely_exercise_the_slow_path`
oracle in `tests/builder.rs:404-467`), which is real evidence — but the one
wall-clock row that exists to corroborate it is structurally incapable of
showing it, and its two arms are not otherwise comparable. A future reader
who sees a delta on this row will attribute it to the jump. This is exactly
the "measure the mechanism you claim, with apples-to-apples arms" shape this
repository already codifies for gate reports.

**Suggested fix.** Two independent changes, both small:

1. Use `JUMP_NONE = (16385, 16384)` for the jump-vs-walk pair (or add it as
   a second pair). There the jump takes 10 iterations and the walk takes 13,
   and the jump genuinely skips classes 37, 38 and 40 — a real skip on a
   fixture the oracle already pins.
2. Give the bench a local walk implementation that uses the **same**
   primitives as `class_for` (bitmask divisibility, `&[usize; N]`, a
   precomputed shift), so the only difference between the arms is
   jump-vs-step. Do **not** change `common::walk_class_for` itself for this
   — its use of a real division is deliberate independence for the proptest
   oracle in `tests/proptest_builder.rs`, and making it mirror production
   would weaken that oracle. A bench-local variant keeps both properties.

---

## P3 — quality / perf / smell

### P3-1 — the LUT's flat byte-per-bucket shape is ~94.5% dead weight, and `L`'s publicness freezes it at 0.1.0

**Location:** `crates/size-classes/src/lib.rs:686-693` (the stability note on
`size2class()`), `:145-167` (`# Memory cost`), `:565-576` (the struct).

The crate's own docs already state the arithmetic: for the realistic scheme,
buckets `888..=16172` — 15285 of 16173, 94.5% — resolve to only 14 distinct
class indices, and the `SizeClasses` object is ~16.20 KiB of which ~16.17 KiB
is the LUT. A two-level shape (a dense `u8` LUT for buckets below some
cutoff, e.g. 16 KiB → 1024 buckets → 1 KiB, plus a bounded scan or binary
search over the ~14 remaining classes above it) would cut the embedded object
roughly 10×, at the cost of one extra fast-path branch and a ≤4-iteration
binary search on the rare large-size path.

**Speculative — I could not benchmark this.** What a benchmark *would* need
to show: (a) no measurable regression on `class_for/small_hit` and
`class_for/at_min_block_align_fast`; (b) a bounded, acceptable regression on
`class_for/near_small_max_*`; (c) the object-size and `.rodata` win. My
expectation is that latency is a wash (the sparse tail is cold and never
touched by small allocations, so today's 16 KiB costs little L1 pressure in
practice) and the real win is binary size / RSS, not speed.

**Why it belongs in this review anyway:** `L` is a *public const generic*, so
this shape is part of the frozen 0.1.0 type signature. The `size2class()`
doc already concedes "a future layout change ... would very likely require a
breaking release regardless". If this is going to be reconsidered at all, the
cheapest moment is before the first publish, not after. Either do the
experiment now, or record an explicit "shape frozen for 0.x, revisit at 1.0"
decision so it isn't rediscovered every round.

### P3-2 — `class_for` is `#[inline]` and inlines the entire jump loop into every hot call site

**Location:** `crates/size-classes/src/lib.rs:854-916`.

The fast path is ~6 instructions; the slow-path jump loop is a full loop with
two array indexings and two branches. `#[inline]` on the whole function means
every `class_for` call site in a consumer gets both. For sefer, whose
`AllocCore`/`HeapCore` entry points call this on every allocation with
`align <= 16`, that is icache footprint spent on a path essentially never
taken.

Suggested shape: keep `#[inline] pub const fn class_for` as the fast path,
move the loop into a private `#[inline(never)] const fn class_for_slow`. Both
remain `const fn`; `#[inline(never)]` on a `const fn` is legal.

**Speculative — needs a bench gate.** A benchmark would need to show the
existing `class_for/small_hit` row unchanged (or better) and
`class_for/large_align_slow_path*` no worse than a call's overhead. It also
needs a *consumer-side* measurement (icache effects don't show up in a
microbenchmark that calls one function in a tight loop — the bench harness
here will look neutral-to-worse even if the change is a real win in sefer).
That is the honest reason to leave it alone absent a real end-to-end gate.

### P3-3 — the hot-field adjacency in `SizeClasses`'s layout is real but accidental and undocumented

**Location:** `crates/size-classes/src/lib.rs:565-576`.

Fields are declared `table` (align 8), `size2class` (align 1),
`min_block_shift` (align 4), `huge_threshold` (align 8). Under `repr(Rust)`,
rustc's current field-reordering heuristic (largest alignment first) will lay
this out as `table | huge_threshold | min_block_shift | size2class`, putting
`min_block_shift` at offset 400 and `size2class[0]` at 404 for `N = 49` —
i.e. the two things `class_for`'s fast path loads share a single 64-byte
cache line for buckets 0..~43 (sizes up to ~700 B). That is very close to
optimal, and it is **luck**, not design: the source declaration order is the
opposite.

The hazard is concrete: this repository has a precedent (R32-5) of adding
`#[repr(C)]` to pin a hot layout. Doing that here, in declaration order,
would place `min_block_shift` roughly `L` bytes (~16 KiB) *after*
`size2class[0]`, turning one cache line into two for every fast-path call.

Suggested fix (no code change strictly required): add a one-line comment next
to the field block recording that (a) the fast path loads `min_block_shift`
and the low end of `size2class`, (b) the current adjacency is a consequence
of the default reordering, and (c) if anyone ever adds `#[repr(C)]`, the
declaration order must be reordered to `min_block_shift, size2class, table,
huge_threshold` first. Verifiable with `-Zprint-type-sizes` — I reasoned this
from the layout algorithm, I did not measure it.

### P3-4 — published rustdoc points readers at files docs.rs does not render

**Location:** `crates/size-classes/src/lib.rs:239-241` ("is exactly the
widened-arithmetic case this crate's `CHANGELOG.md` describes") and
`:63` ("Runnable form with concrete values in the crate's `README.md`").

Neither `CHANGELOG.md` nor `README.md` is pulled into rustdoc (there is no
`#![doc = include_str!(..)]`, deliberately, to stay doctest-free per the
project's no-doctests rule). A docs.rs reader therefore cannot follow either
pointer from where it is written. The README case is mild (crates.io renders
it as the landing page); the CHANGELOG case is a genuine dead end — and the
sentence it supports ("the next class fits even though the intermediate `cur
* num` product does not") is fully stated *in that same paragraph*, so the
CHANGELOG reference adds nothing but a broken trail.

Suggested fix: delete the `CHANGELOG.md` cross-reference (the sentence stands
without it); reword the README pointer to name the repository path
(`crates/size-classes/README.md`) rather than implying a link.

### P3-5 — `# Memory cost`'s causal clause is wrong about *why* the LUT is sparse

**Location:** `crates/size-classes/src/lib.rs:162-167`.

> "the sparsity this scaling implies is large: buckets `888..=16172` ... all
> resolve to just the 14 largest classes (indices `35..=48`), **because above
> `~14 KiB` the geometric spacing widens to `~1.25×`** while the LUT's own
> resolution stays a flat `min_block`."

Two problems with the bolded clause, both checkable against the table:

1. The geometric ratio does not *widen to* 1.25× above 14 KiB — 1.25× is the
   nominal ratio of the *whole* run, and the effective ratio at the **bottom**
   of the table is strictly *larger* (16→32 is 2.0×, 32→48 is 1.5×, 48→64 is
   1.33×). What actually changes at ~14 KiB is that the nine `extras` (256 …
   16384) run out, so local spacing stops being densified by them.
2. Even that is not the cause of the 94.5% figure. I re-derived the same
   scheme with `extras = []`: buckets `888..=16172` then resolve to 13
   classes instead of 14, and the percentage is unchanged. The sparsity is
   driven **entirely** by the second half of the sentence — geometric class
   growth against a flat `min_block` LUT resolution — and would be there with
   no extras at all.

Suggested fix: drop the `because` clause and keep only the true half, e.g.
"... resolve to just the 14 largest classes (indices `35..=48`): class sizes
grow geometrically while the LUT's resolution stays a flat `min_block`, so
the top decade of the size range consumes most of the buckets."

### P3-6 — several published-rustdoc passages are single sentences 6-8 lines long with three levels of nesting

**Locations:** `crates/size-classes/src/lib.rs:93-108` (`Params::extras`),
`:145-167` (`size2class_len`'s `# Memory cost`), `:550-563` (the `SizeClasses`
type doc).

These read as accreted review responses rather than as reference
documentation. Concrete examples:

- `Params::extras` lines 95-104 are **one sentence** running six lines with
  three subordinate clauses ("… panics identically in `const` evaluation
  (compile error) and at runtime in `build_table`, which also checks
  disjointness from the geometric run at its own chokepoint (the merged
  table must itself be strictly increasing); `build_size2class` keeps the
  same check as defense-in-depth for a hand-built table that bypasses
  `build_table` entirely"). Every fact is correct; the packaging is not
  reference prose.
- `# Memory cost` lines 155-162 are one sentence spanning eight lines with a
  four-clause parenthetical nested inside it.
- The `Copy`/`Debug` rationale at `:550-563` is *design-decision
  justification aimed at a reviewer*, and it is already stated — same
  substance, more compactly — at `CHANGELOG.md:104-112`. A user of the type
  needs "not `Copy`; clone explicitly" and "`Debug` prints a summary, not the
  tables"; the *why we chose this* belongs in the CHANGELOG entry that
  already carries it.

Suggested fix: split the long sentences at their semicolons; move the
`Copy`/`Debug` rationale to the CHANGELOG (where it is duplicated already)
and leave a one-line statement of the *behavior* in the type doc. The file is
currently 460 doc lines + 133 inline-comment lines against 330 lines of code
(62% comment); the target of that trimming should be redundancy, not
information.

### P3-7 — `tests/proptest_builder.rs` const-evaluates the full 16 KiB SEFER scheme to reach one 25-line helper

**Location:** `crates/size-classes/tests/proptest_builder.rs:20-21`, pulling
`crates/size-classes/tests/common/mod.rs:31-81`.

`proptest_builder.rs` uses exactly one item from `common` —
`walk_class_for` — but `mod common;` compiles the whole file, including
`static SEFER_SC: SizeClasses<49, 16173>` (line 48) and `SEFER_TABLE` (line
43). Statics are const-evaluated and emitted whether or not they are
referenced, so this test binary pays a full 16173-bucket monotone-pointer
const-eval and carries a ~16 KiB unused `.rodata` blob. The `#![allow(dead_code)]`
at line 21 exists precisely to silence the symptom.

Suggested fix: split `walk_class_for` into its own
`tests/common/walk.rs` (or `tests/common/mod.rs` keeping only the helper, with
the SEFER fixtures in a sibling `tests/common/sefer.rs`), so
`proptest_builder.rs` imports only what it uses. `builder.rs` and the bench
import both. This also removes the need for the blanket `allow(dead_code)`.

---

## P4 — minor / cosmetic

### P4-1 — a confirmed docs.rs rendering defect: "`let a = b;` -cheap syntax"

**Location:** `crates/size-classes/src/lib.rs:553-554`.

```
/// breakdown), and `Copy` would give a full-object duplicate a `let a = b;`
/// -cheap syntax. `Clone` keeps explicit duplication available while forcing
```

Markdown joins the soft-wrapped lines with a space, so the rendered output is
"… duplicate a `let a = b;` -cheap syntax." with a stray space before the
hyphen. Confirmed against the actual generated HTML
(`target/doc/size_classes/struct.SizeClasses.html`:
`<code>let a = b;</code>\n-cheap syntax.`). Intent is the compound adjective
"``let a = b;``-cheap".

Fix: rewrap so the hyphen stays on the same source line as the closing
backtick, e.g. `... duplicate a\n/// \`let a = b;\`-cheap syntax.` I scanned
all rendered pages for this class of soft-wrap defect; this is the **only**
one (the two other hits — `Params::new(..);` / `— the construction path` and
`build_size2class` / `-- see that function's doc` — are intentional dashes
and render correctly).

### P4-2 — mixed `--` and `—` in source renders as two *different* glyphs on every docs.rs page

**Location:** throughout `crates/size-classes/src/lib.rs` (44 ASCII `--` and
44 U+2014 `—`, an exact 50/50 split).

This is not merely a source-style inconsistency: rustdoc enables smart
punctuation, so `--` renders as an **en dash** (U+2013) and `—` renders as an
**em dash** (U+2014). Counted in the generated HTML: `index.html` 6 en / 9
em, `struct.SizeClasses.html` 9 en / 22 em, `fn.build_size2class.html` 12 en
/ 2 em — frequently within the same paragraph. Visually inconsistent
typography on the published page.

Fix: pick one and normalize (mechanically: `--` → `—`, or the reverse).

### P4-3 — the Termination comment names `next_mult`, a variable that no longer exists

**Location:** `crates/size-classes/src/lib.rs:882`.

```
// Termination: `next_mult > block` ⟹ the looked-up class index is
```

`next_mult` was materialized by an earlier version of the loop; the current
code computes the bucket index directly from `block | (align - 1)` and never
names the multiple. A reader grepping the function for `next_mult` finds
nothing. The concept is defined ten lines below (lines 891-894) but under a
different phrasing.

Fix: `// Termination: the smallest multiple of \`align\` strictly greater than
\`block\` is > \`block\`, so the looked-up class index is strictly greater than
\`i\` ...`

### P4-4 — `CHANGELOG.md` is still `## 0.1.0 - Unreleased`

**Location:** `crates/size-classes/CHANGELOG.md:7`. Needs a real date at the
release commit. (Flagging for the checklist, not as a defect in current
state.)

### P4-5 — `min_block()` and `small_align_max()` are byte-identical accessors

**Location:** `crates/size-classes/src/lib.rs:706-708` and `:723-725` — both
are `1usize << self.min_block_shift`. This is a deliberate API decision (two
concepts that happen to coincide today, so a future `small_align_max` knob
can decouple them without a rename), and both docs say so. Worth one explicit
note in `CHANGELOG.md`'s accessor list that the two are *documented to be
equal today and not promised to stay equal*, so a consumer that caches one
does not silently assume the other.

### P4-6 — README's `# Memory cost` cites a 49-class scheme two paragraphs above its own 45-class example

**Location:** `crates/size-classes/README.md:49-53` vs `:69-77`.

The memory-cost paragraph says "A realistic scheme (`min_block = 16`, 49
classes …) gives `L = 16173`", while the `## Example` immediately below
builds a 45-class scheme (`GEO_COUNT = 40` + 5 extras) that *also* gives
`L = 16173` (the extras don't extend `max_class`). Both statements are true;
a reader will nonetheless try to reconcile "49" with the 45 they can count.

Fix: either use the example's own numbers ("45 classes"), or say explicitly
"a scheme like the one below but with nine extras — note `L` is unchanged,
because `extras` below `max_class` do not extend the LUT".

### P4-7 — no test drives `class_for` with `min_block == 1` (shift 0)

`MAX_PARAMS` (`tests/builder.rs:1256-1259`) builds a `min_block = 1`,
256-class table, but only ever calls `build_size2class` on it directly —
never `SizeClasses::build` / `class_for`. Every scheme that reaches
`class_for` uses `min_block ∈ {16, 8, 64, 1<<62}`. The `shift == 0` case
(where `(need - 1) >> 0 == need - 1` and the fast path serves only
`align == 1`) is therefore untested end-to-end. I believe it is correct;
it is simply uncovered.

Fix: build `SizeClasses<MAX_N, MAX_L>` from `MAX_PARAMS` and add a handful of
`class_for` assertions (including one slow-path `align >= 2` case) to
`exactly_256_classes_build_and_index_up_to_255`.

### P4-8 — the README's headline usage is `.unwrap().unwrap()`

**Location:** `crates/size-classes/README.md:87` (mirrored at
`tests/builder.rs:1408`).

`SC.try_class_for(100, 8).unwrap().unwrap()` is the first call a reader sees.
`Result<Option<usize>, InvalidAlign>` is the honest signature and I am **not**
suggesting changing it — but a double unwrap is a poor advertisement for the
recommended-by-default entry point. Showing the idiomatic shape once
(`match SC.try_class_for(size, align) { Ok(Some(i)) => …, Ok(None) => large
path, Err(e) => … }`) would demonstrate the API instead of bypassing it.

### P4-9 — `Params`'s `#[non_exhaustive]` rationale omits the one mechanism that makes it true

**Location:** `crates/size-classes/src/lib.rs:74-76` ("`#[non_exhaustive]`, so
a future policy field is a semver-minor addition rather than a breaking one").

The claim holds, but not for the reason stated. `#[non_exhaustive]` keeps the
*struct* minor-compatible; it does nothing for the *constructor*, and
`Params::new` takes all five fields positionally, so extending it would be
breaking. The reason a future field is actually settable by a downstream
consumer is that the fields are **`pub`** — `#[non_exhaustive]` blocks struct
literals and FRU but not field assignment, so
`const P: Params = { let mut p = Params::new(..); p.new_field = x; p };`
works, in `const` context, cross-crate. That mechanism is load-bearing for
the semver claim and is documented nowhere.

Fix: one sentence — "a future field is set by assigning to it on a `let mut`
binding (the fields are `pub`; `#[non_exhaustive]` blocks only struct-literal
construction and FRU), so adding one does not break `Params::new`'s callers."

### P4-10 — the full sweep's `sizes` vector re-lists ~1/3 of its boundary points

**Location:** `crates/size-classes/tests/builder.rs:188-191`. `boundary_points`
is chained with `(SMALL_STEP_CEIL + 1..=SEFER_MAX + 1).step_by(SEFER_MIN_BLOCK)`,
which enumerates every size ≡ 1 (mod 16) above 8192 — so every boundary point
that happens to be ≡ 1 (mod 16) (e.g. `9089 = 9088 + 1`) is tested twice.
Harmless, just wasted iterations. A `sort_unstable(); dedup();` on the
collected vector removes it.

### P4-11 — `CHANGELOG.md` describes `build_size2class`'s guard as covering "the merged table"

**Location:** `crates/size-classes/CHANGELOG.md:34-36` — "a machine-checked
global-monotonicity/disjointness pass over **the merged table**". "Merged
table" is `build_table` vocabulary; `build_size2class` receives an arbitrary
`table` argument, which is exactly the point of the sentence's own second
half ("for a table a caller assembles by hand"). Reword to "over the supplied
`table`".

---

## Notes on things I checked and found **correct** (recorded so a later round
does not re-open them)

- `size2class_len`'s overflow parenthetical ("reachable only for `min_block ==
  1` and `max_class == usize::MAX`") is exactly right: for `min_block >= 2`
  the quotient is bounded by `usize::MAX / 2`.
- `build_size2class`'s `N <= u8::MAX as usize + 1` bound and its "class_idx
  never reaches N" argument are both correct; 256 classes are representable,
  257 are rejected.
- The `Some(v) if v < small_max => v, _ => small_max` clamp affects **only**
  bucket `L - 1` for any `L`, including hand-built tables whose `small_max`
  is not a `min_block` multiple — and for those, the doc's claim that the top
  bucket "can be the correct, reachable answer for sizes in
  `((L - 1) * min_block, small_max]`" is precisely scoped (the word "can" is
  doing correct work; it is not always right for such tables).
- `simulate_jump_loop` (`tests/builder.rs:384-402`) uses `next_mult >
  small_max` where production uses `next_idx >= L - 1`. These are equivalent
  under the `small_max == (L-1) * min_block` invariant, so the simulation is
  a *genuinely independent formulation* of the guard, not a copy — a strength,
  not a defect.
- `class_for_slow_path_rejects_next_mult_landing_exactly_on_the_l_minus_1_boundary`'s
  claim that a fault-injected `>= L` **infinite-loops** on its fixture is
  correct: `size2class[3]` clamps back to class 2, which is never
  32-divisible, so `i` cycles at 2 forever.
- `pub(crate) static SIZE2CLASS: [u8; S2C_LEN] = *SC.size2class();`
  (`src/alloc_core/size_classes.rs:272`) is legal — a `static` initializer may
  *read* another `static` (a `const` may not), and this is a `static`.
- `bench-scale-tool = "0.1"` resolves from crates.io
  (`Cargo.lock` checksum present), so the dev-dependency does not block
  publication; `[lints] workspace = true` inherits only `unexpected_cfgs`
  entries (`loom`/`kani`/…) that this crate never sets — harmless, though the
  crate gains nothing from the inheritance either.
