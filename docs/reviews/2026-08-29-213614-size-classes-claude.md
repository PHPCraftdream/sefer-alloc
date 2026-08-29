# `size-classes` publication review — round 6 (claude, blind static analysis)

**Date:** 2026-08-29 21:36:14
**Reviewer:** claude (independent, blind — no prior review document under
`docs/reviews/` was read before the findings below were derived)
**Tree reviewed:** `origin/main` @ `acb8203d38ddd6806b8ef06f7520f7af956a155b`
**Mode:** read-only static analysis. No `cargo build`/`test`/`bench`/`clippy`/
`doc`/`package` was run; no file under review was edited. Every non-trivial
numeric or structural claim below was re-derived by hand from the current
source.

---

## Verdict: **GO** for publication.

Nothing found in this round blocks a `0.1.0` release. The crate's arithmetic,
overflow handling, loop termination, index bounds and const-eval semantics all
hold up under an independent re-derivation (§2). The single P2 is a
**documentation-only** defect — the prose that *justifies* the crate's headline
`try_class_for` safety guarantee cites a guard the code does not contain — and
should land before `cargo publish`, because it is on the most-read page of the
public API and it is the audit trail for a totality claim. It changes no
behavior; the guarantee itself is real, and I verified it independently.

**Finding count: P1 = 0, P2 = 1, P3 = 3, P4 = 7.**

---

## 1. Scope

Reviewed in full:

- `crates/size-classes/src/lib.rs` — every public item, all arithmetic,
  overflow behavior, const-eval semantics, loop termination, index bounds,
  all rustdoc.
- `crates/size-classes/tests/builder.rs` (1434 lines),
  `crates/size-classes/tests/common/mod.rs`,
  `crates/size-classes/tests/proptest_builder.rs`.
- `crates/size-classes/benches/size_classes_bench.rs`.
- `crates/size-classes/Cargo.toml`, `README.md`, `CHANGELOG.md`.

Consumer check:

- `src/alloc_core/size_classes.rs`, `src/alloc_core/segment_layout.rs`,
  `tests/size_classes_lookup.rs`.

Supporting read-only context: workspace `Cargo.toml`, `Cargo.lock`,
`.gitignore`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`,
and `grep` over `src/` for consumer call sites.

---

## 2. Independent verification performed (negative results worth recording)

These are checks that came back **clean**. They are recorded because a later
round should not have to redo them, and because several of them are the kind of
claim that is expensive to re-derive.

### 2.1 The SEFER table, re-derived from scratch

I rebuilt the 49-entry merged table by hand from
`min_block = 16, growth = (5, 4), geo_count = 40`, extras
`[256, 512, 1024, 2048, 4096, 6144, 8192, 12288, 16384]`:

```
 0..8   16 32 48 64 80 112 144 192 240
 9..17  256* 304 384 480 512* 608 768 960 1024*
18..26  1200 1504 1888 2048* 2368 2960 3712 4096* 4640
27..35  5808 6144* 7264 8192* 9088 11360 12288* 14208 16384*
36..48  17760 22208 27760 34704 43392 54240 67808 84768 105968
        132464 165584 206992 258752            (* = extras)
```

`max_class = 258752`, `L = 258752 / 16 + 1 = 16173`, `N = 49` — all three match
every place the repo quotes them. Every entry is a multiple of 16, the merged
table is strictly increasing, and no extra collides with a geometric value.

### 2.2 All five `JUMP_*` fixtures re-derived, including exact iteration counts

Re-derived independently of `simulate_jump_loop`, by executing `class_for`'s
slow path on paper against the table above:

| fixture | `(size, align)` | seed idx / block | iters | result |
|---|---|---|---|---|
| `JUMP_A` | (1025, 256) | 18 / 1200 | 4 (18→19→20→21) | `Some(21)` = 2048 |
| `JUMP_B` | (2049, 1024) | 22 / 2368 | 3 (22→24→25) | `Some(25)` = 4096 |
| `JUMP_MULTI` | (513, 512) | 14 / 608 | 2 (14→17) | `Some(17)` = 1024 |
| `JUMP_DENSE` | (129, 128) | 6 / 144 | 2 (6→9) | `Some(9)` = 256 |
| `JUMP_NONE` | (16385, 16384) | 36 / 17760 | 10 (36→39→41→42→43→44→45→46→47→48) | `None` |

All five match `tests/common/mod.rs`'s doc comments and the pinned tuples in
`sefer_bench_jump_rows_genuinely_exercise_the_slow_path`, including
`JUMP_NONE`'s "visits 10 of the 13 remaining classes, skipping 37, 38 and 40"
and `JUMP_A`'s "18→19→20→21 is a contiguous run, the jump skips nothing here".
The exact-count oracle is doing real work: a plain `iters >= 2` check would not
distinguish any of these.

### 2.3 The `# Memory cost` sparsity figures

`size2class_len`'s rustdoc claims buckets `888..=16172` — "15285 of the 16173
total, 94.5%" — all resolve to classes `35..=48` (14 classes).

Re-derived: bucket 887 needs `888 × 16 = 14208`, which class 34 (block 14208)
serves exactly; bucket 888 needs `889 × 16 = 14224 > 14208`, so it is the first
bucket to spill to class 35. `16172 − 888 + 1 = 15285`, and
`15285 / 16173 = 94.51%`. Both figures are **exact**, and 888 is the correct
first bucket (not off by one).

### 2.4 The "fewer classes, larger LUT" counter-example

`min_block = 8`, `growth = (3, 2)`, `geo_count = 24`, no extras — re-derived
step by step:
`8 16 24 40 64 96 144 216 328 496 744 1120 1680 2520 3784 5680 8520 12784 19176
28768 43152 64728 97096 145648`.
`max_class = 145648`, `L = 145648 / 8 + 1 = 18207`. Both exact. The comparison
holds: `18207 + 24×8 = 18399` bytes vs the 49-class scheme's `16173 + 49×8 =
16565` bytes.

### 2.5 The align-density figures in the bench / `JUMP_DENSE` doc

"128 divides ~31% of `SEFER_TABLE`'s entries vs 256's ~20%". Counted by hand
over all 49 entries: **15 divisible by 128** (256, 384, 512, 768, 1024, 2048,
3712, 4096, 6144, 8192, 9088, 12288, 14208, 16384, 43392) = 30.6%, and **10
divisible by 256** = 20.4%. Both round to the quoted figures.

### 2.6 The `geo_count` overflow boundaries

`build_table`'s `# Panics` cites `geo_count = 183` (64-bit) / `84` (32-bit) as
the first overflowing counts for `min_block = 16, growth = (5, 4)`. Closed-form
check from the verified `c₃₉ = 258752`: `c₁₈₂ ≈ 258752 × 1.25¹⁴³ ≈ 1.87e19` vs
`usize::MAX = 1.8447e19` — overflows, with `c₁₈₁ ≈ 1.49e19` fitting. On 32 bits,
`c₈₂ ≈ 3.80e9` fits and `c₈₃ ≈ 4.75e9` exceeds `4.295e9`. Both boundaries land
exactly where the doc and the `#[cfg(target_pointer_width)]`-gated test pairs
put them, with only ~1% margin — this is a tight, non-obvious claim and it is
correct.

### 2.7 The slow path: termination and jump ≡ walk (proved, not assumed)

- **`need == next_mult` exactly.** `next_idx = (block | (align−1)) >> shift`, so
  `(next_idx + 1) << shift` is the smallest `min_block` multiple strictly
  greater than `block | (align−1)`, i.e. `≥ next_mult`. Because `align` is a
  power of two `> min_block`, `next_mult` is itself a `min_block` multiple, so
  the two are **equal**. The `+1`/`−1` round-trip really is redundant.
- **Termination.** After the `next_idx >= L − 1` guard,
  `(next_idx + 1) << shift ≤ (L−1) << shift = small_max`, so
  `build_size2class`'s clamp is inert and the looked-up `need` is
  `> table[i]`. The table is strictly increasing, so the new index is
  **strictly** greater than `i` every iteration. `i` is bounded by `N`.
- **Equivalence to a step-by-1 walk.** Every class strictly between `block` and
  `next_mult` is not a multiple of `align` (by definition of `next_mult`), and
  nothing `≥ next_mult` is skipped. So the jump visits a subsequence of the
  walk with the same first hit — never more iterations, strictly fewer whenever
  it skips.
- **No-next-multiple case.** When `block | (align−1) == usize::MAX`,
  `usize::MAX >> shift ≥ small_max >> shift == L − 1`, so the existing guard
  returns `None` with no separate overflow check. Correct.

### 2.8 The index-space guard

`seed_idx = (need − 1) >> shift`. Since `small_max == (L−1) × min_block` for any
`build_table`-derived scheme, `seed_idx ≥ L − 1 ⟺ need − 1 ≥ small_max ⟺ need >
small_max`, in both directions, exactly. The rewrite from a `need >
self.small_max()` comparison to the const-generic guard is sound, and it does
let the compiler drop the `size2class[seed_idx]` bounds check.

### 2.9 `try_class_for`'s totality claim is **true** (despite the P2 below)

After the power-of-two rejection: `align ≥ 1` ⟹ `need ≥ 1` ⟹ `need − 1` cannot
underflow; `seed_idx < L − 1 < L` after the guard; `self.table[i]` is guarded by
`while i < N`; `self.size2class[next_idx]` is guarded by `next_idx < L − 1`. The
one remaining hazard is `L − 1` underflowing at `L == 0` — impossible, because
`SizeClasses`' fields are private, `build` is the only constructor, and `build`
asserts `L == size2class_len(small_max, min_block) ≥ 2` for any table whose
first class is `min_block`. **"Never panics, for any `(size, align)` pair" is
airtight.** Only its published *proof sketch* is wrong (P2-1).

### 2.10 `build_size2class` can never truncate a class index

The inner scan can only leave `class_idx == N` if `need > table[N−1]`, which the
`_ => small_max` clamp forbids. So `N == 256` is safe (max index 255 = `u8::MAX`)
and `N == 257` is correctly rejected by the count assert *before* the `L` check.
The `checked_mul` folding into the same clamp is correct: an unrepresentable
`(k+1) × min_block` certainly exceeds a representable `small_max`.

### 2.11 The `u128` rounding arithmetic

`rounded = (scaled + mask as u128) & !(mask as u128)` — the complement is taken
in `u128`, so the high 64 bits are preserved and the subsequent
`rounded <= usize::MAX as u128` assert sees the true value. No silent truncation
of the upper half. The `next <= cur` min-step fallback and its `checked_add` are
both reachable and both correct (`num ≤ den ⟹ ceil(cur·num/den) ≤ cur ⟹
round_up ≤ cur`, uniformly for the whole run — never a mixed table, so the
256-class `growth = (0, 1)` test in `builder.rs` covers the fallback fully).

### 2.12 Root-crate consumer, all three feature configurations

| config | `TABLE_LEN` | `SMALL_MAX` | `S2C_LEN` |
|---|---|---|---|
| default | 49 | 258752 | 16173 |
| `medium-classes` | 55 | 1 MiB | 65537 |
| `medium-classes-wide` | 58 | 1.75 MiB | 114689 |

All three match the in-file comments. In every configuration the `EXTRAS` list
is strictly increasing, every entry is a 16-multiple, no entry collides with the
geometric run (the geometric run tops out at 258752 < 262144 = the first medium
extra), and `N ≤ 256`. `SMALL_MAX < SEGMENT` (4 MiB) holds in all three, so the
module doc's M4 argument — "every power of two `≤ SMALL_MAX < SEGMENT` trivially
divides `SEGMENT`" — is sound in the widest configuration too (largest servable
align is `2²⁰`, which divides `2²²`).

### 2.13 The `const` re-materialization hazard is *not* realized in the consumer

`SizeClasses`' own rustdoc warns that "a `const` this size re-materializes at
every use site". `src/alloc_core/size_classes.rs` keeps `SIZE_CLASS_TABLE` as a
`const [usize; 49]` (392 B). I grepped every use in `src/`: there is **no
value-use anywhere** — the only non-doc reference is
`SegmentLayout::SIZE_CLASS_TABLE = &super::size_classes::SIZE_CLASS_TABLE`
(promoted to a single anonymous static) plus two `const` derivations
(`SMALL_CLASS_COUNT`, `SMALL_MAX`). Every hot-path query
(`SizeClasses::class_for` / `::block_size`, ~15 call sites across
`alloc_core_small*.rs` and `registry/heap_core_*.rs`) reads the `SC` **static**.
No 392-byte re-materialization occurs. Clean.

### 2.14 Publication mechanics

- `Cargo.lock` shows `bench-scale-tool 0.1.0` and `proptest` resolving from
  `registry+https://github.com/rust-lang/crates.io-index`, and the workspace root
  has **no** `[patch]` or `[replace]` section — so `cargo publish -p
  size-classes` (which builds `[[bench]]`) can resolve its dev-dependencies from
  crates.io as-is.
- CI (`.github/workflows/ci.yml`) covers the crate with: `cargo publish
  --dry-run`, a `thumbv7em-none-eabi` `--no-default-features` build (real
  `no_std` signal), `cargo test` in **both** debug and release, `cargo test
  --target i686-unknown-linux-gnu` (real 32-bit `usize` — the
  `#[cfg(target_pointer_width = "32")]` fixtures are genuinely executed),
  `cargo clippy --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo
  doc --no-deps`, an MSRV `cargo check`/`cargo test --no-run`, and `cargo bench
  --no-run`. That is a complete gate set for this crate.
- The crate declares **no Cargo features** and no
  `[package.metadata.docs.rs]`, so `CLAUDE.md`'s "doc-lint must run in the exact
  shipped feature set" rule has no gap to close here: default == `--all-features`
  == the docs.rs render.
- `#![deny(missing_docs)]` with `InvalidAlign(pub usize)` is fine — rustc's
  `missing_docs` skips *positional* fields.
- The declared `rust-version = "1.88"` is conservative. The highest feature I can
  find in `src/lib.rs` is `usize::is_multiple_of` (const-stable in **1.87**);
  `core::error::Error` is 1.81, `u128::div_ceil` is const since 1.73. Over-declaring
  is safe and matches the workspace floor — recorded as a non-finding.

### 2.15 Non-findings I explicitly considered and rejected

- **`build_size2class` accepting a hand-built table with non-`min_block`-multiple
  entries** (the `[16, 24, 32]` permanently-unreachable-class case): documented in
  the function's own rustdoc, pinned by
  `hand_built_table_with_a_non_min_block_multiple_entry_leaves_it_unreachable`,
  and unreachable through `SizeClasses::build`. Deliberate, not a defect.
- **`#[non_exhaustive] Params` + a single positional `const fn new`**: adding a
  field stays semver-minor only if `new` is kept and a *second* constructor is
  added. That is the standard reading and the claim holds; not a defect, but see
  P4-4's neighbourhood if the wording is ever revisited.
- **The three proptest schemes never exercise the min-step fallback** (all three
  have `num > den`): adequate, because §2.11 shows the fallback is
  all-or-nothing per scheme and the 256-class `(0, 1)` test in `builder.rs`
  covers it with a full-bucket scan.
- **`walk_class_for` uses `need > small_max` where production uses the
  index-space guard**: this is a *feature* of the oracle (an independent
  formulation), and §2.8 proves the two are exactly equivalent. The tight
  boundary is separately pinned by
  `class_for_slow_path_rejects_next_mult_landing_exactly_on_the_l_minus_1_boundary`.
- **`black_box(result);` vs `let _ = black_box(result);`** asymmetry in the
  bench: correct as written — `Result` is `#[must_use]`, `Option` is not.

---

## 3. Findings

### P1 — blocking

**None.**

---

### P2 — should be fixed before `cargo publish`

#### P2-1 — `try_class_for`'s "Never panics" proof sketch cites a guard that does not exist, and is internally incoherent

**Location:** `crates/size-classes/src/lib.rs:955-961`

```rust
/// **Never panics, for any `(size, align)` pair** — ... so `need = max(size,
/// align)` is always `>= 1` past that point, every LUT index computed from it
/// stays in bounds (`need <= small_max` is checked before indexing;
/// `(need - 1) >> min_block_shift <= size2class().len() - 1` otherwise), and
/// the slow-path jump loop is bounded exactly as `class_for`'s own doc proves.
```

Two problems in one parenthetical:

1. **`need <= small_max` is not checked, anywhere.** `class_for` checks
   `seed_idx >= L - 1` — an index-space guard on a const-generic bound. The
   crate's own inline comment 70 lines above says so in as many words:

   > `lib.rs:886` — `// Index-space guard, not `need > self.small_max()`: ...`

   So the published rustdoc for the *recommended-by-default* entry point cites,
   as the reason its safety guarantee holds, a check that the same file
   explicitly documents as **not** being what the code does. That is a shipped
   self-contradiction, in the crate's most-read page.

2. **The "…; … otherwise" structure has no antecedent.** There is no dichotomy
   for "otherwise" to select between: the second clause is not an alternative
   branch, it is the actual (and only) bound. As written the sentence cannot be
   parsed into a valid proof by a reader trying to audit the totality claim.

**Why P2 and not P4:** this is not one more stale mention of a renamed guard in
an internal comment. It sits on the crate's headline safety guarantee, it is the
*justification* a reader is being asked to trust when the docs tell them to
prefer `try_class_for` over `class_for`, and it is on published API rustdoc that
goes to docs.rs. The guarantee itself is real — I proved it independently in
§2.9 — which is exactly why the wrong proof is worth fixing rather than
tolerating: a future edit "guided" by this parenthetical could remove the real
guard while believing it kept the stated one.

**Suggested replacement** (states the actual mechanism, matches `lib.rs:886`):

> …so `need = max(size, align)` is always `>= 1` past that point; the seed index
> `(need - 1) >> min_block_shift` is compared against the compile-time bound
> `L - 1` **before** any indexing, so both the seed and every slow-path re-seed
> stay strictly inside `size2class()`; and the jump loop is bounded exactly as
> `class_for`'s own doc proves.

**Cost:** one rustdoc paragraph. No behavior change, no test change.

---

### P3

#### P3-1 — Two shipped files attribute `JUMP_NONE`'s `None` to running off the end of the table — a path `lib.rs` itself documents as unreachable

**Locations:**
- `crates/size-classes/benches/size_classes_bench.rs:148` (section header) and
  `:152-155`
- `crates/size-classes/tests/common/mod.rs:68-73`

The bench says the row "takes 10 real iterations … **before exhausting the table
and returning `None`**". `common/mod.rs` says "A slow-path case that **exhausts
the table** and returns `None` … 10 jump-loop iterations **before the table
ends**".

Re-derived (§2.2): on iteration 10 the loop is sitting on `i = 48 == N - 1`
(block 258752, not 16384-divisible). It computes `next_idx = 262143 >> 4 =
16383`, which is `>= L - 1 = 16172`, and returns `None` **from inside the loop
body via the index-space guard**. `while i < N` never fails.

This is not a nitpick about phrasing, because `lib.rs:935-939` says the opposite
in the production source:

```rust
// Unreachable in practice: `self.size2class[..] <= N - 1` always ..., so the
// loop always returns from inside the body at `i == N - 1` at the latest.
```

So the crate ships a production comment proving a path is unreachable, and two
sibling files whose comments describe that exact unreachable path as the
mechanism the fixture exercises. A reader who trusts the fixture doc will
believe the trailing `None` is live code.

**Fix:** in both files, attribute the result to the guard, e.g. "…10 jump-loop
iterations; on the last class (index 48) the next multiple of 16384 is 262144,
past `small_max`, so the `next_idx >= L - 1` guard returns `None` — the
`while i < N` fallthrough is never reached." One sentence each.

---

#### P3-2 — Published rustdoc points at private test-module and bench-file names that a docs.rs reader cannot resolve

**Locations:** `crates/size-classes/src/lib.rs:155`, `:671-672`, `:950`

Three public rustdoc passages reference repository-internal artifacts:

| line | text | what it names |
|---|---|---|
| 155 | "the crate's own tests' `SEFER` fixture" | a `pub(crate)` const in `tests/common/mod.rs` |
| 671 | "already exercised by this crate's own `extreme64_overflow` test fixture" | a `#[cfg(target_pointer_width = "64")] mod` **inside** `tests/builder.rs` |
| 950 | "see the `try_class_for/*` rows in `benches/size_classes_bench.rs`" | a bench file |

None of these render as links, and none is reachable from docs.rs — the
integration-test and bench trees are not documented. The `extreme64_overflow`
one is the worst offender: it is a nested private module name with no path, so
even a reader who downloads the `.crate` tarball has to grep for it.

This is the "review-response prose baked into shipped docs" category. The
information each passage carries is fine; the *pointer* is what does not survive
publication. Each can be reworded to state the fact directly:

- `:155` — already gives the full parameterization inline; the fixture name adds
  nothing.
- `:671` — the doc already gives the exact scheme (`min_block = 1 << 62, L = 4`
  → `L * min_block == 2^64`). Drop "already exercised by … test fixture" or
  replace with "(this crate's own test suite exercises exactly this scheme)".
- `:950` — "the crate's benchmark suite has a `try_class_for/*` row for this"
  reads the same and survives the tarball boundary.

---

#### P3-3 — `class_for` is `#[inline]` and inlines the entire divisibility-jump loop into every call site (opportunity, **not** a measured win)

**Location:** `crates/size-classes/src/lib.rs:878-941`

Structural facts, all statically verifiable:

- The fast path is ~6 machine operations: `max`, `sub`, `shr`, one compare
  against a compile-time constant (`L - 1`), one `u8` load, one compare against
  `1 << shift`.
- The slow path is an unbounded-trip-count loop containing a second array
  indirection (`self.size2class[next_idx]`) and a `usize` array load
  (`self.table[i]`).
- Both live in one `#[inline]` function, so every caller that inlines the fast
  path also materializes the loop. In the root crate, `SizeClasses::class_for`
  is reached from ~7 distinct allocation entry points
  (`alloc_core.rs:2252/2253/2553`, `registry/heap_core_alloc.rs:79/548/884`,
  `registry/heap_core_dealloc_batch.rs:192`), each of which is itself on a hot
  path.
- `align > min_block` is the rare case by construction (it is the whole reason
  the fast path exists).

Splitting the loop into a `#[inline(never)] #[cold]` private helper would leave
the `#[inline]` fast path at its ~6 operations and remove the loop from every
inlined copy. The **code-size** effect is provable by inspection; the
**latency** effect is not, and I did not measure it (read-only review).

I am reporting this as an opportunity rather than a finding precisely because I
cannot substantiate a speedup. If it is acted on, it needs a bench gate — the
crate already has `class_for/small_hit` and five slow-path rows, so an A/B is
cheap. If it is declined, that is a defensible call for a 49-class table that is
always cache-hot; the reasoning should be recorded rather than re-litigated
every round.

---

### P4

#### P4-1 — "`~16.18 KiB` total" is the two tables' sum, not the object's size

**Locations:** `crates/size-classes/src/lib.rs:157-159`, `README.md:50-51`

> "…the LUT dominates **the whole object's size**, ~16.18 KiB total on a 64-bit
> target"

`16173 + 392 = 16565 B = 16.177 KiB` — that is the two *tables*. The struct also
carries `min_block_shift: u32` and `huge_threshold: usize`, so
`size_of::<SizeClasses<49, 16173>>()` is `392 + 8 + 4 + 16173 = 16577`, rounded
to the 8-byte alignment = **16584 B = 16.195 KiB**, i.e. `~16.20 KiB`.

A 19-byte (0.11%) discrepancy is immaterial in itself; it is worth a line only
because the sentence explicitly says "the whole object's size", and the two
scalar fields it silently omits are never mentioned anywhere in the memory-cost
discussion. Either quote `~16.20 KiB` for the object, or say "the two tables
alone are ~16.18 KiB".

---

#### P4-2 — Mixed em-dash and ASCII `--` inside the same rustdoc paragraphs

**Location:** `crates/size-classes/src/lib.rs` (throughout)

Counted: **53** `—` and **47** ` -- ` in `lib.rs`. They are not segregated by
context — the crate-level doc alternates within a single bullet:

```
//!   caller's classifier happens to handle silently falls through to the
//!   caller's whole-segment path — a real bug class in hand-rolled allocators
...
//!   [`SizeClasses::try_class_for`] is the checked twin -- validates `align`
```

docs.rs renders `--` literally as two hyphens, so the published page shows two
different dash styles in adjacent sentences. `README.md` (13 vs 1) and
`CHANGELOG.md` (19 vs 3) are much more consistent, which makes `lib.rs` the
outlier. Cosmetic; mechanical to fix with a single pass.

---

#### P4-3 — README's commented pseudo-signature names `InvalidAlign`, which the example's `use` line does not import

**Location:** `crates/size-classes/README.md:63, 82`

```rust
use size_classes::{build_table, size2class_len, Params, SizeClasses};
...
// SC.try_class_for(size, align) -> Result<Option<usize>, InvalidAlign>
```

The fence is ```` ```rust ````, so a reader is invited to copy it; uncommenting
the line they were pointed at does not compile. Add `InvalidAlign` to the `use`
list (and to the `declaration_lines` array in
`readme_example_lines_appear_verbatim_in_readme_md`, which pins that exact line
verbatim).

---

#### P4-4 — CHANGELOG's trait inventory omits `Debug`, which is public, hand-written, and behaviorally documented

**Location:** `crates/size-classes/CHANGELOG.md:106-109`

The entry settles `Clone`-but-not-`Copy` explicitly ("Settled before the first
release, since removing `Copy` afterwards would be a breaking change") but never
mentions that `SizeClasses` implements `Debug`, that the impl is deliberately
**not** a derive, and that its output is a summary rather than the raw arrays —
all of which are documented in `lib.rs:563-567` and pinned by
`debug_impl_prints_a_summary_not_the_raw_tables`. That is observable public
behavior a downstream snapshot test could depend on, and the CHANGELOG is where
a first release inventories its trait surface.

Adjacent, smaller: the `Debug` output exposes `min_block` but not
`min_block_shift`, even though both have public accessors.

---

#### P4-5 — `CHANGELOG.md` is still headed `## 0.1.0 - Unreleased`

**Location:** `crates/size-classes/CHANGELOG.md:7`

Standard release-commit checklist item: this must become
`## 0.1.0 - <release date>` in the publish commit. Flagged only so it is not
missed at the moment it stops being harmless.

---

#### P4-6 — `Display for InvalidAlign` hardcodes a function name the error is no longer exclusive to

**Location:** `crates/size-classes/src/lib.rs:590-598`

```rust
write!(f, "try_class_for: align ({}) must be a power of two (the Layout contract)", self.0)
```

`InvalidAlign` is a `pub` type whose `Display` prefixes a specific function
name. It is already returned by at least one other function in this repository —
`sefer_alloc::SegmentLayout::try_class_for`, which re-exports the type — and by
`SizeClasses::try_class_for` in the shim layer. Today they all happen to share
the name `try_class_for`, so the message reads correctly by coincidence; any
future producer with a different name will emit a message naming a function the
caller never invoked. Dropping the prefix (`"align (6) must be a power of two
(the Layout contract)"`) makes the message correct for every producer, and the
existing test only asserts `msg.contains('6')`, so it stays green.

---

#### P4-7 — The bench has no row at the fast/slow-path boundary itself

**Location:** `crates/size-classes/benches/size_classes_bench.rs`

Fast-path rows use `align = 1`; slow-path rows use `align ∈ {128, 256, 512,
1024, 16384}`. Nothing measures `align == min_block == 16` (the last align that
takes the fast path) or `align == 32` (the first that takes the slow path, and
the cheapest possible slow-path case — with 16 of the 49 SEFER classes already
32-divisible). Those two rows bracket the branch the whole fast/slow split
exists for, and would make the "the slow path is a different asymptotic/cost
from the fast path" claim in the section header measurable at its own boundary
rather than only far from it. Two rows, no new fixtures needed.

---

## 4. Summary table

| # | Priority | Area | One-line |
|---|---|---|---|
| P2-1 | P2 | `src/lib.rs:955` rustdoc | `try_class_for`'s "never panics" proof cites a `need <= small_max` check that does not exist; parenthetical is incoherent |
| P3-1 | P3 | bench + `tests/common` | `JUMP_NONE`'s `None` attributed to table exhaustion, a path `lib.rs` proves unreachable |
| P3-2 | P3 | `src/lib.rs` rustdoc ×3 | published docs point at private test-module / bench-file names unresolvable from docs.rs |
| P3-3 | P3 | `src/lib.rs:878` | `#[inline]` inlines the whole jump loop into every hot call site (opportunity; needs a bench gate) |
| P4-1 | P4 | `lib.rs:157`, README | "~16.18 KiB total" is the tables' sum; `size_of` is 16584 B (~16.20 KiB) |
| P4-2 | P4 | `src/lib.rs` | 53 `—` vs 47 ` -- ` mixed inside the same paragraphs |
| P4-3 | P4 | README | commented signature names `InvalidAlign`, not in the example's `use` |
| P4-4 | P4 | CHANGELOG | trait inventory omits the hand-written `Debug` |
| P4-5 | P4 | CHANGELOG | still `0.1.0 - Unreleased` (release-commit checklist) |
| P4-6 | P4 | `lib.rs:590` | `Display` hardcodes `try_class_for:`; the error has more than one producer |
| P4-7 | P4 | bench | no row at `align == 16` / `align == 32`, the fast/slow boundary itself |

---

## 5. Publication recommendation

**GO.**

The crate is correct where it matters: I could not construct any `(size, align)`
input, any `Params`, or any hand-built table that produces a wrong class, an
out-of-bounds index, a non-terminating loop, or a silently-wrapped table. Every
overflow site is checked, every check is tested, the tests are non-vacuous (the
exact-iteration-count oracle and the hand-derived golden run in particular are
doing real work), and CI covers debug, release, 32-bit, `no_std`, MSRV, clippy,
rustdoc and `publish --dry-run`.

Recommended ordering: land **P2-1** and **P3-1**/**P3-2** (all documentation,
all mechanical) plus **P4-5** in the release commit; treat **P3-3** as a
post-release perf question with a bench gate; take the remaining P4s at
convenience.
