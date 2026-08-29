# `size-classes` — independent publication review, round 5 (claude)

**Date:** 2026-08-29
**Reviewer:** claude (blind, static-analysis-only)
**Tree reviewed:** `origin/main` @ `fcc02801c554b8de48f9890a0bdaab13c47fb957` (CI-green: `CI` + `Kani verification`)
**Mode:** read-only. No `cargo build`/`test`/`bench`/`clippy`/`doc`/`package` was run; no file under review was edited. `docs/reviews/` was NOT read before the findings below were derived.

---

## Verdict

**GO for publication.**

No P1. One P2 — a flatly falsifiable claim in the root crate's *published* rustdoc
(`sefer_alloc::SegmentLayout::SMALL_ALIGN_MAX`), which is a documentation defect in the
**consumer**, not in `size-classes` itself. The `size-classes` crate's own library
surface (`src/lib.rs`, `README.md`, `CHANGELOG.md`, `Cargo.toml` metadata) is, on this
reading, arithmetically correct in every non-trivial claim I re-derived by hand — including
both `# Memory cost` worked examples, both `geo_count` overflow boundaries, all five
`JUMP_*` fixture derivations, and both alignment-density percentages. Nothing found here
blocks `cargo publish -p size-classes`; the P2 and the four P3s are all fixable in a
docs/tests/bench commit that does not touch the published library's behavior.

**Findings: P1 = 0 · P2 = 1 · P3 = 4 · P4 = 8.**

---

## Scope

| Area | Files |
| --- | --- |
| Library | `crates/size-classes/src/lib.rs` (965 lines) |
| Tests | `crates/size-classes/tests/builder.rs`, `tests/common/mod.rs`, `tests/proptest_builder.rs` |
| Bench | `crates/size-classes/benches/size_classes_bench.rs` |
| Packaging | `crates/size-classes/Cargo.toml`, `README.md`, `CHANGELOG.md` |
| Consumer | `src/alloc_core/size_classes.rs`, `src/alloc_core/segment_layout.rs` |
| Context (read-only, for cross-checking only) | root `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml`, `src/lib.rs` re-export block |

---

## Independent verification performed (negative results worth recording)

Everything in this section was re-derived from the current source by hand, on paper, before
being compared against the existing prose. All of it **checked out**; none of it is a
finding. Recorded so a sixth round does not have to redo it.

### 1. `size2class_len`'s `# Memory cost` — both worked examples are exact

- Scheme A (`min_block = 16`, `growth = (5, 4)`, `geo_count = 40`): I replayed all 40
  geometric steps by hand (`16, 32, 48, 64, 80, 112, 144, 192, 240, 304, 384, 480, 608,
  768, 960, 1200, 1504, 1888, 2368, 2960, 3712, 4640, 5808, 7264, 9088, 11360, 14208,
  17760, 22208, 27760, 34704, 43392, 54240, 67808, 84768, 105968, 132464, 165584, 206992,
  258752`). `max_class = 258752` ✔, `L = 258752/16 + 1 = 16173` ✔, `table = 49 × 8 = 392 B`
  ✔, 49 classes ✔.
- Scheme B (`min_block = 8`, `growth = (3, 2)`, `geo_count = 24`, no extras): replayed all
  24 steps (`8, 16, 24, 40, 64, 96, 144, 216, 328, 496, 744, 1120, 1680, 2520, 3784, 5680,
  8520, 12784, 19176, 28768, 43152, 64728, 97096, 145648`). `max_class = 145648` ✔,
  `L = 18207` ✔. The rustdoc's whole point — that a 24-class scheme can produce a *larger*
  object than a 49-class one — holds exactly as stated.
- The `~16.18 KiB total` figure is `(392 + 16173)/1024`; the real
  `size_of::<SizeClasses<49, 16173>>()` is ~16584 B (≈16.20 KiB) once `min_block_shift: u32`
  + `huge_threshold: usize` + tail padding are counted. The `~` covers a 0.1 % gap; not
  reported.

### 2. `build_table`'s `# Panics` overflow boundaries

Analytically re-derived, independent of the pinned tests: from class 40 = 258752, `1.25ⁿ`
growth reaches `usize::MAX` (64-bit) after ~143 more steps → class **183** is the first to
overflow ✔; on 32-bit, after ~44 more steps → class **84** ✔. The
"`cur * num` first exceeds `usize` only once `cur > usize::MAX / num`" window is
`log₁.₂₅(5) ≈ 7.2` steps, matching "roughly the last half-dozen steps" ✔. Both boundaries
are additionally pinned by `#[cfg(target_pointer_width = ...)]` tests, and CI does run the
32-bit half (`cargo test -p size-classes --target i686-unknown-linux-gnu`, `ci.yml:1869`).

### 3. All five `JUMP_*` fixtures re-derived from the merged 49-entry table

I rebuilt sefer's merged table (geometric run ∪ 9 extras) and re-walked the jump loop for
each fixture. Every documented seed index, block size, iteration count and result is exact:

| Fixture | `(size, align)` | Seed idx / block | Iterations | Result |
| --- | --- | --- | --- | --- |
| `JUMP_A` | (1025, 256) | 18 / 1200 | 4 | `Some(21)` (2048) ✔ |
| `JUMP_B` | (2049, 1024) | 22 / 2368 | 3 | `Some(25)` (4096) ✔ |
| `JUMP_MULTI` | (513, 512) | 14 / 608 | 2 | `Some(17)` (1024) ✔ |
| `JUMP_DENSE` | (129, 128) | 6 / 144 | 2 | `Some(9)` (256) ✔ |
| `JUMP_NONE` | (16385, 16384) | 36 / 17760 | 10 | `None` ✔ |

`JUMP_NONE`'s "visits 10 of the 13 remaining classes; indices 37, 38 and 40 are skipped"
is exactly right — the visited chain is 36 → 39 → 41 → 42 → 43 → 44 → 45 → 46 → 47 → 48.
The bench's "for `JUMP_A` … a naive walk ALSO takes 4 (18→19→20→21 is a contiguous run)"
is also exact.

### 4. `JUMP_DENSE`'s density percentages

Counted by hand over the 49-entry table: 15 entries are multiples of 128 (30.6 %) and 10
are multiples of 256 (20.4 %). "~31 % … vs 256's ~20 %" ✔.

### 5. `class_for`'s index-space guard and slow-path termination

- `small_max == (L − 1) · min_block` for every `build_table`-derived table (every entry is
  a `min_block` multiple), so `seed_idx ≥ L − 1 ⟺ need > small_max` **exactly**. Verified
  on the tightest fixture in the suite (`EXTREME64`, `min_block = 2⁶²`, `L = 4`), where
  `usize::MAX >> 62 == 3 == L − 1` — an equality, not a margin.
- The same identity carries the slow path: `next_idx = (block | (align−1)) >> shift`
  satisfies `next_idx ≥ L − 1 ⟺ next_mult > small_max`, and the `usize::MAX` corner
  (no representable next multiple) folds into it with no separate overflow check. The
  round-3 removal of the `checked_add(1)` / `−1` round-trip is sound.
- **Termination:** `need_bucket(next_idx) ≥ next_mult > table[i]`, so the re-seeded index
  is strictly greater than `i`; combined with `size2class[·] ≤ N − 1` this proves the loop
  always returns from inside the body. A corollary: the trailing `None` at
  `src/lib.rs:925` is provably unreachable (`i < N` never fails). Harmless — the
  `while i < N` bound is still what lets the compiler drop the `self.table[i]` bounds
  check — but see P4-7.
- **Fast-path minimality:** `seed` really is the *smallest* fitting class, because the only
  `min_block` multiple in `(need, (seed_idx+1)·min_block]` is the endpoint itself. This
  depends on `build_table`'s multiple-of-`min_block` guarantee, which the rustdoc already
  calls out for hand-built tables.

### 6. The `u128` widening in `build_table`

`v & !(mask as u128)` is the correct form — `!(mask) as u128` would truncate and silently
mis-round. Both `checked_*` panic arms
(`"cur * num overflows u128"`, `"scaled + mask overflows u128"`) are **unreachable for any
target with `usize::BITS ≤ 64`**: `(2⁶⁴−1)² < 2¹²⁸`. The comment's forward-looking note
about a hypothetical 128-bit `usize` already says as much, so this is documented dead code,
not an oversight.

### 7. Boundary/degenerate-scheme arithmetic

Re-derived and confirmed: the `N = 256` / `N = 257` `u8`-index boundary (`min_block = 1`,
`growth = (0, 1)` → table `1..=256`, `L = 257`, largest emitted index 255); the
`DOMAIN` fixture's LUT `[0, 1, 2, 2, 2]` and its three zones (valid / false-sentinel /
out-of-bounds at size 81); `class_for_slow_path_rejects_next_mult_landing_exactly_on_the_l_minus_1_boundary`'s
`[16, 32, 48]` / `L = 4` fixture landing `next_idx` on `L − 1` precisely; and the hand-built
`extreme64` table's fixed `[0, 1, 2, 3]` vs pre-fix `[0, 1, 2, 2]`.

### 8. Consumer-side scheme, all three feature configurations

| Feature set | `TABLE_LEN` | `SMALL_MAX` | `S2C_LEN` |
| --- | --- | --- | --- |
| default | 40 + 9 = 49 ✔ | 258752 (≈252.7 KiB, "~253 KiB" ✔) | 16173 ✔ |
| `medium-classes` | 40 + 15 = 55 ✔ | 1 MiB ✔ | 65537 ✔ |
| `medium-classes-wide` | 40 + 18 = 58 ✔ | 1.75 MiB ✔ | 114689 ✔ |

In all three, every `extras` entry is strictly increasing, a multiple of 16, and disjoint
from the geometric run (the smallest medium extra, 262144, is above the run's top, 258752),
so `build_table`'s merged-monotonicity check passes; and `SMALL_MAX < SEGMENT` (4 MiB) in
all three, so the module doc's M4 argument ("every power of two `≤ SMALL_MAX < SEGMENT`
trivially divides `SEGMENT`") holds under `medium-classes-wide` too. `N ≤ 256` in all
three ✔.

### 9. README example is legal input

`min_block = 16`, `growth = (5, 4)`, `geo_count = 40`, `extras = [256, 512, 1024, 2048,
4096]`: none of the five extras appears in the geometric run, so the merged table is
strictly increasing and `build_table` does not panic. `N = 45`, `L = 16173` ✔.

### 10. Packaging / CI posture

- `cargo publish --dry-run -p size-classes` (`ci.yml:734`), `cargo clippy -p size-classes
  --all-targets -- -D warnings` (`:1883`), `RUSTDOCFLAGS="-D warnings" cargo doc -p
  size-classes --no-deps` (`:1884`), debug + release test runs (`:1850`, `:1851`), 32-bit
  i686 tests (`:1869`), `thumbv7em-none-eabi` `no_std` build (`:1832`), MSRV `cargo check` /
  `cargo test --no-run` / `cargo bench --no-run` (`:2083`–`:2093`). This is the strongest
  per-crate CI matrix in the workspace; nothing missing that I could name.
- The crate declares **no features**, so CLAUDE.md's `package.metadata.docs.rs.features`
  rule does not apply (`--all-features` ≡ default ≡ docs.rs here).
- crates.io metadata is in bounds: 5 keywords (max 5), 3 valid category slugs, both
  `LICENSE-MIT`/`LICENSE-APACHE` present, `license = "MIT OR Apache-2.0"`, description
  168 chars. `bench-scale-tool = "0.1"` resolves to a real registry crate
  (`Cargo.lock:64`, checksum present), so the dev-dependency does not block publish.
- `src/lib.rs`, `README.md` and `CHANGELOG.md` contain **zero** review-ID / task-number
  citations — the round-1–4 neuroslop cleanup held. (One survives in `Cargo.toml`; see
  P3-4.)

### 11. Perf ideas considered and rejected (not reported as findings)

- **Hot-path field loads.** `SC` is an immutable `static` with no `UnsafeCell`, so
  `min_block_shift` / `L − 1` / the LUT all live in `.rodata` and LLVM constant-folds the
  scalar loads at the (already `#[inline]`) call site. There is no measurable win left in
  re-storing `min_block` alongside the shift, reordering the `align <= min_block` test
  ahead of the LUT load (both paths need the load), or replacing the `1usize << shift`
  with a cached mask.
- **Outlining the slow path** from the `#[inline]` fast path: still an unmeasured
  hypothesis, and this review ran static-only. Not re-raised.
- **The LUT's flat shape.** Quantified in P4-5, but the crate already carries an explicit
  "deliberate, but not permanently promised" stability note on `size2class()`, and `L` is a
  public const generic, so any hybrid layout is a breaking change by construction. No
  action recommended pre-0.1.0.

---

## P1 — blocking

*None.*

---

## P2 — should fix before/with publication

### P2-1 · `SegmentLayout::SMALL_ALIGN_MAX`'s published rustdoc states a falsehood: alignments well below `SMALL_MAX` *do* fall through to the large path

**Files:** `src/alloc_core/segment_layout.rs:37-44` (**`pub`** — this renders on docs.rs as
`sefer_alloc::SegmentLayout::SMALL_ALIGN_MAX`), duplicated at
`src/alloc_core/size_classes.rs:85-91` (`pub(crate)`).

> "an alignment above this value (and up to [`SMALL_MAX`](Self::SMALL_MAX)) still resolves
> to a small class via a bounded divisibility-walk slow path (see [`class_for`](Self::class_for),
> #114/B1) — **only an alignment greater than `SMALL_MAX` falls through to the
> dedicated-segment large path**."

The bolded clause is false, and falsifiable three different ways in the **default /
`production` build**:

1. **`align = 32768` never resolves, for any size.** The 49-entry table's entries at or
   above 32768 are `34704, 43392, 54240, 67808, 84768, 105968, 132464, 165584, 206992,
   258752`; none is a multiple of 32768 (nor of 65536, nor of 131072 — I checked all
   ten quotients for each). So `SegmentLayout::class_for(1, 32768) == None` even though
   `32768 ≪ SMALL_MAX == 258752`. **Three** of the seventeen power-of-two alignments in
   `(SMALL_ALIGN_MAX, SMALL_MAX]` behave this way.
2. **A `size` above the last divisible class also forces fall-through, for an `align`
   that is itself a table entry.** `class_for(16385, 16384) == None` — this is literally
   the crate's own `JUMP_NONE` fixture (`crates/size-classes/tests/common/mod.rs:73`),
   pinned to return `None` after 10 jump iterations. The doc's framing attributes
   fall-through to `align` alone and cannot express this.
3. **It is not a default-build artifact.** Under `medium-classes-wide`,
   `class_for(1048577, 1048576) == None` (the three wide extras — 1.25/1.5/1.75 MiB — are
   none of them multiples of 1 MiB), with `align = 1 MiB ≤ SMALL_MAX = 1.75 MiB`.

The same source file already states the correct, qualified form 45 lines earlier
(`src/alloc_core/size_classes.rs:42-46`): *"Alignment alone does not force Large:
`align > MIN_BLOCK` is still served by a small class **whenever one exists** whose
`block_size` is a multiple of `align`."* The constant's own doc dropped the "whenever one
exists" qualifier and hardened it into an "only".

`SegmentLayout::SMALL_MAX`'s doc (`segment_layout.rs:46-50`) is individually true — "an
allocation whose alignment exceeds this value" *does* go large — but read together with
`SMALL_ALIGN_MAX`'s "only", the pair reads as a biconditional that the code does not
implement.

**Suggested fix (doc-only, no behavior change):** replace "only an alignment greater than
`SMALL_MAX` falls through" with the already-correct wording from the module doc, e.g.
"…still resolves to a small class **whenever some class's `block_size` is a multiple of
that alignment**; when none is (or when `size` pushes `max(size, align)` past the last such
class), the request falls through to the dedicated-segment large path — as does any
alignment greater than `SMALL_MAX`." One sentence, both files.

**Not a code bug.** `class_for` returns the right answer; the root crate's own sweep test
(`tests/size_classes_lookup.rs`) compares against an independent reference and therefore
agrees with the code, not with the doc — which is exactly why four rounds of testing never
surfaced this.

---

## P3 — should fix

### P3-1 · `tests/builder.rs`'s `extreme64_overflow` module documents two `class_for` mechanisms that no longer exist

**File:** `crates/size-classes/tests/builder.rs:863-878` (module header) and
`:1016-1024` (`class_for_next_multiple_overflow_returns_none`'s body comment).

The body comment says, in the present tense:

> "`(3<<62) | ((1<<63)-1)` is usize::MAX, so **`checked_add(1)` yields None** and
> `class_for` must return None -- the same outcome the **`next_mult > small_max` clamp**
> already produces for every other out-of-range case on this path."

Neither construct is in `class_for` any more. `10fe946` ("class_for slow path drops a
redundant `checked_add(1)`/`-1` round-trip") removed the `checked_add`, and `060fa09`
replaced the `next_mult > small_max` comparison with the index-space guard
`if next_idx >= L - 1`. Today's code is:

```rust
let next_idx = (block | (align - 1)) >> self.min_block_shift;
if next_idx >= L - 1 {
    return None;
}
```

The module header (`:863-878`) compounds it: "the two `checked_*` sites `cc94a46` fixed …
`build_size2class`'s per-bucket `need` clamp and **`class_for`'s slow-path round-up**" —
the second of those two sites no longer exists as a `checked_*` call.

The test itself is still correct and still valuable (it black-box-pins `class_for(3<<62,
1<<63) == None`, which today's guard reaches by the `usize::MAX >> 62 == 3 == L − 1`
identity), so this is a stale-comment defect, not a vacuous test. But this is the **fifth**
distinct location of the same "describes a removed guard" class across rounds 2–5
(cf. tasks #1566, #1574, #1580 and P4-6 below), and it is the one place where the removed
construct is named as though it were still executing.

**Suggested fix:** rewrite the two passages against the current guard, and keep the
`Pre-fix (bare + 1) …` sentence as the explicitly-historical part it already is.

### P3-2 · Two bench rows are the same measurement — the exact defect the section's own comment says it fixed

**File:** `crates/size-classes/benches/size_classes_bench.rs:186-195`.

```rust
h.bench("class_for/near_small_max_below", || … SEFER_SC.class_for(SEFER_MAX - 1, 1) …);
h.bench("class_for/near_small_max_at",    || … SEFER_SC.class_for(SEFER_MAX,     1) …);
```

`SEFER_MAX = 258752 = 16172 × 16`. Both rows compute the identical seed index:

- `below`: `(258751 − 1) >> 4 = 258750 >> 4 = 16171`
- `at`:    `(258752 − 1) >> 4 = 258751 >> 4 = 16171`

Both then load `size2class[16171]`, both take the `align ≤ min_block` fast return, both
yield `Some(48)`. Same instruction stream, same LUT byte, same cache line — the two numbers
can differ only by measurement noise.

This is precisely the failure mode the comment directly above them (`:174-183`) says it
corrected: *"all three old rows fell through the SAME `need > small_max` branch regardless
of the 'below/at/above' label and measured nothing distinguishable."* The replacement rows
fixed the *third* row (`above_small_max_rejection` genuinely takes the early-return path)
but left `below`/`at` indistinguishable, because `SEFER_MAX` and `SEFER_MAX − 1` land in
the same `min_block`-wide bucket by construction.

The three `is_huge/near_huge_threshold_{below,at,above}` rows (`:210-223`) have the same
shape: `is_huge` is a single `size >= self.huge_threshold` compare whose result is
`black_box`ed, so all three rows measure one branch-free comparison.

**Suggested fix:** either collapse `below`/`at` into one row, or make `below` land in a
different bucket (e.g. `SEFER_MAX - MIN_BLOCK`, seed index 16170) and say in the comment
that the point is bucket locality, not a boundary. Same call for the three `is_huge` rows —
one row is enough for a `>=` compare.

### P3-3 · The README drift guard cannot detect drift in the direction its own comment claims

**File:** `crates/size-classes/tests/builder.rs:1399-1428`.

The comment promises a bidirectional guard:

> "This asserts every declaration line of the mirrored example appears verbatim … in
> README.md's own raw text, **so editing one copy without the other fails THIS test**, not
> just silently rots the published example."

But the test compares `include_str!("../README.md")` against a **third, hardcoded copy** of
the nine declaration lines living inside the test itself. There are therefore three copies:
README, the mirrored test `readme_example_compiles_and_derives_its_generics` (`:1376-1397`),
and the guard's `declaration_lines` array.

- Editing README alone → guard fails ✔ (the intended direction).
- Editing the **mirrored test** alone (say `GEO_COUNT: usize = 40` → `32`) → the mirrored
  test still compiles, the guard still finds `"const GEO_COUNT: usize = 40;"` in the
  unchanged README, and **nothing fails.** The "mirror" relationship silently breaks.

So "editing one copy without the other fails THIS test" is true for one of the two copies
and false for the other. (There is also an existing asymmetry the guard makes visible: the
guard checks for the README's `use size_classes::{…};` line, which the mirrored test does
*not* contain — the test relies on the file-level import at `:11-13`, which lists a
different set of items.)

**Suggested fix:** either derive the checked lines from one source (e.g.
`include_str!("builder.rs")` plus a delimiter pair around the mirrored block, then assert
that block is a substring of the README), or downgrade the comment to what the test
actually proves: "README still contains these nine lines."

### P3-4 · `crates/size-classes/Cargo.toml` ships review-response prose into the crates.io tarball

**File:** `crates/size-classes/Cargo.toml:15-24` — nine lines of comment for a one-line
`workspace = true`:

```toml
[lints]
# size-classes publication audit run 1 (Sol-codex, P3-3): inherits the
# workspace's shared lint policy, matching once-ptr-cell's own
# `[lints] workspace = true` (crates/once-ptr-cell/Cargo.toml). …
workspace = true
```

`Cargo.toml` is in the published tarball and is rendered by crates.io's source browser, so
this is shipped text. Two problems:

1. **It is the last surviving review-ID citation in any shipped `size-classes` file.** I
   grepped `src/lib.rs`, `README.md` and `CHANGELOG.md` for `review|audit|task #|P\d-\d|
   Sol-|rush-|claude|oxx|MS prepublish` — zero hits. The rounds-1–4 cleanup (tasks #1545,
   #1589) covered the rustdoc and missed the manifest.
2. **It cross-references a sibling monorepo crate** (`crates/once-ptr-cell/Cargo.toml`)
   that does not exist in the standalone `size-classes` tarball, and explains a
   *workspace* policy decision that a downstream reader cannot act on — the packaged
   manifest has the lints **inlined** by cargo anyway, so the `workspace = true` line the
   comment explains is not even what ships.

**Suggested fix:** reduce to one line stating the intent without the audit ID and without
the sibling-crate path, e.g. `# Inherit [workspace.lints.rust]; this crate declares no
cfg of its own.`

---

## P4 — nits / optional

### P4-1 · "eight … explicit page-aligned classes — 512 … 16384" is inaccurate for half of them
`src/alloc_core/size_classes.rs:24-25`. The eight are `512, 1024, 2048, 4096, 6144, 8192,
12288, 16384`. Against a 4 KiB `PAGE`, only `4096, 8192, 12288, 16384` are page-aligned;
`512/1024/2048` are sub-page, and `6144` is 1.5 pages (not a multiple of `PAGE`, and not a
power of two). "Page-aligned" is doing work here it cannot do — the classes exist so that
`need = max(size, align)` for a sector- or page-aligned request lands on an
`align`-divisible stride, plus density fill (`6144`, `12288`). Suggest "device-block- and
page-friendly classes" or naming the two roles separately.

### P4-2 · The `SMALL_ALIGN_MAX` drift guard hardcodes `MIN_BLOCK * 2` as its probe's largest class
`src/alloc_core/size_classes.rs:183-190`:

```rust
SizeClassesImpl::<2, { size2class_len(MIN_BLOCK * 2, MIN_BLOCK) }>::build(PROBE)
```

`MIN_BLOCK * 2 == 32` is the 2-class probe's `max_class` **only because `GROWTH == (5, 4)`**
(`round_up(ceil(16·5/4), 16) = 32`). With, say, `GROWTH = (3, 1)` the probe's table is
`[16, 48]`, `L` would have to be 4, and the guard would fail with `build_size2class`'s
`"L must equal size2class_len(max_class, min_block)"` — a confusing panic that names
neither `SMALL_ALIGN_MAX` nor the drift the guard exists to catch. Deriving it
(`const PROBE_T: [usize; 2] = build_table::<2>(PROBE); … size2class_len(PROBE_T[1],
MIN_BLOCK)`) costs two const items and removes the coupling. (The round-4 optimization
that introduced the small probe is otherwise correct and worth keeping — it does avoid the
second 16173/65537/114689-bucket const-eval.)

### P4-3 · `class_for`'s profile prose implies the *consumer's* profile decides; it is `size-classes`' own
`crates/size-classes/src/lib.rs:814-830`. Both `debug_assert!` and the `need - 1`
overflow check are lowered against the profile **`size-classes` itself** was compiled with
(`cfg!(debug_assertions)` is folded in the defining crate's MIR; the overflow check likewise
follows the defining crate's `overflow-checks`). In ordinary cargo usage these track the
consumer's profile, so the doc is right in practice — but a
`[profile.release.package.size-classes] debug-assertions = true` (or the inverse via
`[profile.dev.package."*"]`) desyncs them, and this doc goes to unusual lengths elsewhere
to be exact about profile dependence. One clause ("…on by default whenever this crate is
built with debug assertions, which normally tracks the consumer's profile") would close it.

### P4-4 · `SegmentLayout::try_class_for` names `size_classes::InvalidAlign`, not the re-export added for it
`src/alloc_core/segment_layout.rs:105` and `:124`. `src/lib.rs:417` re-exports
`sefer_alloc::InvalidAlign` specifically so "a caller … can name/match the error type
without adding `size-classes` as their own direct dependency" — but the method's own
signature and doc name only the foreign path, so a docs.rs reader of the method sees no
hint that the re-export exists. Mention `sefer_alloc::InvalidAlign` in the doc line (the
signature's path can stay).

### P4-5 · `# Memory cost` argues about LUT size without naming the sparsity that drives it
`crates/size-classes/src/lib.rs:146-162`. For the crate's own `SEFER` example, buckets
`888..=16172` — **15285 of 16173 (94.5 %)** — resolve to just 14 distinct class indices
(35..=48), because above ~14 KiB the class spacing is ~1.25× while the LUT's resolution
stays a flat 16 B. That one sentence is the concrete reason the section's "`L` scales with
`max_class / min_block`, not with `N`" claim bites, and it is the quantitative case for the
hybrid layout `size2class()`'s stability note already anticipates. Purely additive; no code
change implied (`L` is a public const generic, so a hybrid is a breaking release either way).

### P4-6 · A fifth shipped copy of the removed `need > small_max` guard description
`crates/size-classes/benches/size_classes_bench.rs:181` — "all three old rows fell through
the SAME `need > small_max` branch". This one is *historical* narrative (it describes rows
that were replaced), so it is defensible; but it sits 16 lines above `:197-199`, which
correctly names today's guard (`the `seed_idx >= L - 1` guard`), and a reader scanning the
file sees two different names for the same branch. Cheapest fix: "…fell through the same
early-rejection branch (then spelled `need > small_max`; today `seed_idx >= L - 1`)".

### P4-7 · `class_for`'s trailing `None` is provably unreachable and unremarked
`crates/size-classes/src/lib.rs:925`. `size2class[·] ≤ N − 1` always, and the loop returns
from inside the body at `i == N − 1` at the latest (proof in §5 of the verification section
above), so `while i < N` never fails. The statement is still *needed* — it is the
type-level fallthrough, and the `i < N` bound is what lets the compiler elide the
`self.table[i]` bounds check — but a one-line `// Unreachable: the loop always returns …`
saves the next reader the derivation, and the function's own `Termination:` comment
(`:897-899`) stops just short of stating it.

### P4-8 · `CHANGELOG.md` still says `## 0.1.0 - Unreleased`
`crates/size-classes/CHANGELOG.md:7`. Known release-commit checklist item (raised and
deliberately deferred in each of rounds 1–4). Recorded for completeness only; it must
become a date in the publishing commit.

---

## Summary table

| ID | Severity | File | One-line |
| --- | --- | --- | --- |
| P2-1 | P2 | `src/alloc_core/segment_layout.rs:37-44` (+ `size_classes.rs:85-91`) | "only an alignment greater than `SMALL_MAX` falls through" is false; `class_for(1, 32768) == None` in the default build |
| P3-1 | P3 | `crates/size-classes/tests/builder.rs:863-878, 1016-1024` | Comments describe a `checked_add(1)` and a `next_mult > small_max` clamp that `class_for` no longer has |
| P3-2 | P3 | `crates/size-classes/benches/size_classes_bench.rs:186-195` | `near_small_max_below` and `near_small_max_at` compute the same seed index (16171) — one measurement under two names |
| P3-3 | P3 | `crates/size-classes/tests/builder.rs:1399-1428` | README drift guard compares README against a third hardcoded copy; drift in the mirrored test is undetected |
| P3-4 | P3 | `crates/size-classes/Cargo.toml:15-24` | Last surviving review-ID citation, shipped in the crates.io tarball, cross-referencing a sibling monorepo crate |
| P4-1 | P4 | `src/alloc_core/size_classes.rs:24-25` | Four of the eight "page-aligned classes" are not page-aligned |
| P4-2 | P4 | `src/alloc_core/size_classes.rs:183-190` | Drift guard hardcodes `MIN_BLOCK * 2`, silently coupled to `GROWTH == (5, 4)` |
| P4-3 | P4 | `crates/size-classes/src/lib.rs:814-830` | Profile prose implies the consumer's profile; it is the defining crate's |
| P4-4 | P4 | `src/alloc_core/segment_layout.rs:105,124` | Doc names `size_classes::InvalidAlign`, not the `sefer_alloc::InvalidAlign` re-export added for it |
| P4-5 | P4 | `crates/size-classes/src/lib.rs:146-162` | `# Memory cost` omits the 94.5 %-of-buckets-→-14-answers sparsity that drives its own argument |
| P4-6 | P4 | `crates/size-classes/benches/size_classes_bench.rs:181` | Fifth copy of the removed `need > small_max` guard name, 16 lines from the correct one |
| P4-7 | P4 | `crates/size-classes/src/lib.rs:925` | Trailing `None` is provably unreachable and unremarked |
| P4-8 | P4 | `crates/size-classes/CHANGELOG.md:7` | Still `Unreleased` (known checklist item) |
