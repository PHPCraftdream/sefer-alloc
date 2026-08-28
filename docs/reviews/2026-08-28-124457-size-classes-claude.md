# size-classes: pre-publication review, round 3 (Claude, independent blind read)

## Verdict

**GO for publication**, conditional on stamping the `CHANGELOG.md` release date
(P4-5). The library arithmetic in `crates/size-classes/src/lib.rs` is sound — I
re-derived the full 49-entry `SEFER_TABLE`, hand-simulated the jump loop for all
five `JUMP_*` fixtures, and re-proved the `seed_idx >= L - 1` index-space guard
and the jump-vs-walk equivalence from scratch; every one checks out. The one
finding that matters is again a **cross-file comment regression introduced by a
prior review round**: commit `32cc1d8`, whose entire stated purpose was "fix
false jump-iteration-count premise", removed a false iteration-count claim from
`benches/size_classes_bench.rs` and simultaneously wrote a *new* false
iteration-count claim into the `tests/common/mod.rs` file it was creating in the
same commit — where round 2 then edited the line directly beneath it and left it
standing. Everything else is small: one provably-redundant `checked_add`/`- 1`
round-trip in the slow-path loop, one directional cross-reference pointing the
wrong way, and four stale-comment/doc items.

Checked HEAD: `2349cc6a11b5a962d729eca3640fe264f59b5546` (working tree clean for
`crates/size-classes/` and `src/alloc_core/`).

Review mode: read-only static analysis, single reviewer, no sub-agents. Per the
request I ran **no** `cargo build` / `test` / `bench` / `clippy` / `doc` /
`package`, and edited no file under review. `git log` / `git show` were used
read-only, and only *after* each finding was already derived from the current
source, to date the regressions. I did not open any file under `docs/reviews/`
before forming these findings (only the immediately preceding round's file, and
only afterwards, to match its structural format).

## Scope

- `crates/size-classes/src/lib.rs` — all public items, arithmetic, overflow
  behaviour, const-eval semantics, loop termination, index bounds, all rustdoc;
- `crates/size-classes/tests/builder.rs`, `tests/common/mod.rs`,
  `tests/proptest_builder.rs`;
- `crates/size-classes/benches/size_classes_bench.rs`;
- `crates/size-classes/Cargo.toml`, `README.md`, `CHANGELOG.md`;
- consumer check of `src/alloc_core/size_classes.rs` and
  `src/alloc_core/segment_layout.rs` in the root crate.

---

## Independent verification performed (no findings — recorded so the negative results are citable)

### 1. `SEFER_TABLE` reconstructed entry by entry

From `tests/common/mod.rs:31-43`'s actual `Params` (`min_block = 16`,
`growth = (5, 4)`, `geo_count = 40`, nine extras). In `min_block` units the
geometric recurrence is exactly `m -> ceil(5m/4)` starting at `m = 1`, giving
40 terms ending at `m = 16172` → `258752`. Merged with the extras the table is:

```
 0..8:   16 32 48 64 80 112 144 192 240
 9:      256*        10..12: 304 384 480       13: 512*
14..16:  608 768 960                           17: 1024*
18..20:  1200 1504 1888                        21: 2048*
22..24:  2368 2960 3712                        25: 4096*
26..27:  4640 5808                             28: 6144*
29:      7264                                  30: 8192*
31..32:  9088 11360                            33: 12288*
34:      14208                                 35: 16384*
36..48:  17760 22208 27760 34704 43392 54240 67808 84768 105968 132464
         165584 206992 258752
```
(`*` = an `extras` entry.) 49 entries, `max_class = 258752`,
`L = 258752/16 + 1 = 16173` — matching `lib.rs:157`, `README.md:50` and the
`# Memory cost` worked example. The second worked example (`min_block = 8`,
`growth = (3, 2)`, 24 classes) also reproduces exactly: `max_class = 145648`,
`L = 18207`, and object size `18207 + 24*8 = 18399 > 16173 + 49*8 = 16565`, so
the "fewer classes, larger object" claim is genuinely true, not rhetorical.

### 2. All five `JUMP_*` fixtures hand-simulated against that table

| fixture | `(size, align)` | seed idx / block | iterations | result | doc claim |
|---|---|---|---|---|---|
| `JUMP_A` | (1025, 256) | 18 / 1200 | 4 (18→19→20→21) | `Some(21)` = 2048 | ✅ |
| `JUMP_B` | (2049, 1024) | 22 / 2368 | 3 (22→24→25) | `Some(25)` = 4096 | ✅ |
| `JUMP_MULTI` | (513, 512) | 14 / 608 | 2 (14→17) | `Some(17)` = 1024 | numbers ✅, comparison ❌ (P2-1) |
| `JUMP_DENSE` | (129, 128) | 6 / 144 | 2 (6→9) | `Some(9)` = 256 | ✅ |
| `JUMP_NONE` | (16385, 16384) | 36 / 17760 | 10 (36→39→41→42→43→44→45→46→47→48) | `None` | ✅ |

`JUMP_NONE`'s "visiting 10 of the 13 remaining classes (indices 37, 38 and 40
are skipped by the round-up)" is exactly right — 36..=48 is 13 classes, 10
visited, and 37/38/40 are precisely the three the round-up skips. The
`class_for/dense_align_slow_path` density figures are right too: 128 divides
15/49 = 30.6% ("~31%") of the table, 256 divides 10/49 = 20.4% ("~20%").

### 3. The `seed_idx >= L - 1` guard and the jump-vs-walk equivalence

- `small_max == (L - 1) * min_block` holds because `build_table` forces every
  entry to be a `min_block` multiple, so
  `(L-1)*min_block == (small_max/min_block)*min_block == small_max`.
- Hence `seed_idx >= L - 1 ⟺ (need-1)/min_block >= small_max/min_block ⟺
  need > small_max` exactly. Correct, and the compile-time `L - 1` really does
  let the `self.size2class[seed_idx]` bounds check be elided.
- Jump correctness: for a power-of-two `align > min_block`, `next_mult` is a
  multiple of `align` and therefore of `min_block`, so
  `round_up(next_mult, min_block) == next_mult` and the re-seed lands on the
  *smallest* class `>= next_mult`. Every class skipped lies strictly in
  `(block, next_mult)` and so cannot be an `align` multiple. Never skips a
  valid answer; index strictly increases each iteration, so it terminates.
- `while i < N` can never actually fail (`build_size2class` guarantees every
  LUT entry is `< N`), but it is not dead weight: it is what proves
  `self.table[i]` in-bounds to the compiler. Correct as written.

### 4. `try_class_for`'s "never panics, for any `(size, align)` pair"

Holds for any `SizeClasses` reachable through `build`: `align` is validated
first, so `need >= 1`; `L >= 2` always (the first table entry is `min_block`,
so `size2class_len >= 2`), so `L - 1` cannot underflow; `min_block_shift <
usize::BITS`; the slow path's only arithmetic is `checked_add`-guarded and both
array indices are guarded. No hole found.

### 5. Overflow/boundary paths re-derived

`size2class_len`'s `checked_add`, `build_table`'s `u128` widening + min-step
`checked_add`, `build_size2class`'s `(k+1).checked_mul` folding into the
`small_max` clamp, and `build_size2class`'s `N <= 256` u8-index bound (256
classes → indices `0..=255`, and `class_idx` provably never reaches `N`) are
all correct, and each has a matching test with a distinct expected message.
The `min_block = 16, growth = (5,4)` overflow boundary cited in `build_table`'s
`# Panics` (183 on 64-bit, 84 on 32-bit) is consistent with the `m -> ceil(5m/4)`
recurrence I derived above.

### 6. Root-crate consumption

`src/alloc_core/size_classes.rs` and `src/alloc_core/segment_layout.rs` consume
the crate correctly: `PARAMS` is a single source, `SIZE2CLASS` copies `SC`'s own
LUT rather than re-deriving it, `SMALL_ALIGN_MAX`'s `const _` drift guard is
honest about being tautological today, `SegmentLayout` exposes both `class_for`
and `try_class_for`, and the M4 base-alignment precondition the crate documents
but cannot check is discharged explicitly (segment base `SEGMENT`-aligned **and**
`carve_block` aligning to `block_size`). No consumer-side defect found.

### 7. Speedup ideas considered and rejected

- **Make `min_block_shift` a const generic** (`SizeClasses<N, L, SHIFT>`) so
  `(need-1) >> SHIFT` and `1 << SHIFT` fold to constants. Rejected: every real
  instantiation is a `static` with a const initializer in the consuming crate,
  so LLVM already constant-folds those field loads; the cost is a third public
  const generic frozen into 0.1.0 for a win that is probably zero. Worth *not*
  doing, but worth having decided before the freeze.
- **Outline the slow path behind `#[inline(never)]`** so the `#[inline]` fast
  path stays tiny. Unmeasured hypothesis; no static argument settles it, and
  the request was read-only, so I am not raising it as a finding.
- **Niche-optimize the return type.** `Option<usize>` costs a second register,
  but with `N` allowed up to 256 there is no free niche in a class index, and
  changing the return type is a breaking API change. Rejected.

---

## P1 — blocking

None. Nothing found in this round prevents publication.

---

## P2 — should fix before publishing

### P2-1. `JUMP_MULTI`'s doc makes exactly the false iteration-count comparison the previous round removed one file over — a self-contradicting sentence, written by the commit that fixed the original

**File:** `crates/size-classes/tests/common/mod.rs:58-59`

```rust
/// A denser multi-iteration slow-path case than `JUMP_A`/`JUMP_B`: seed class
/// 14 (block 608, not 512-divisible), 2 jump-loop iterations to `Some(17)`
/// (block 1024). ...
pub(crate) const JUMP_MULTI: (usize, usize) = (513, 512);
```

**Problem.** "A denser **multi-iteration** slow-path case **than
`JUMP_A`/`JUMP_B`**" is false on both available readings, and the *same
sentence* supplies the refutation:

- **Iteration depth.** `JUMP_MULTI` takes **2** jump-loop iterations (stated
  eight words later in its own doc). `JUMP_A` takes **4**, `JUMP_B` takes **3**
  — both pinned as exact expected values in
  `tests/builder.rs:424-429`. `JUMP_MULTI` is the *shallowest* of the three,
  not a deeper "multi-iteration" case.
- **Density.** If "denser" means the align divides a denser subset of the
  table: `align = 512` divides 8 of 49 entries (16.3%); `JUMP_A`'s `align = 256`
  divides 10 of 49 (20.4%). Denser than `JUMP_B` (1024 → 7/49, 14.3%), sparser
  than `JUMP_A`. So the claim is false against `JUMP_A` on this axis too.

**Why this is a P2 and not a nitpick.** This is a *reintroduction*, in a new
location, of a claim a prior round explicitly killed. `git show 32cc1d8` —
commit subject "fix false jump-iteration-count premise", body: "JUMP_A=4
iterations, JUMP_B=3, JUMP_MULTI=2, so JUMP_MULTI was actually the SHALLOWEST of
the three" — is the commit that *created* this line while deleting the
equivalent claim from the bench. `benches/size_classes_bench.rs:132-141` today
carries the correct, explicitly-contradicting text ("this row's `JUMP_MULTI`
takes only 2, making it the SHALLOWEST of the three, not the deepest"), so the
crate now ships two files stating opposite things about the same constant.
Round 2 (`5341c6b`) then edited `JUMP_NONE`'s doc — the *next three lines* in
the same block — and left this one. `tests/` is inside the published tarball.

**Fix.** Replace the comparative clause with what the fixture actually is —
the bench's own justification is already correct and can be reused verbatim:

```rust
/// A slow-path case seeded from a lower, denser region of the table than
/// `JUMP_A`/`JUMP_B` (not a deeper one -- at 2 iterations it is the
/// shallowest of the three): seed class 14 (block 608, not 512-divisible),
/// 2 jump-loop iterations to `Some(17)` (block 1024). ...
```

---

## P3 — nice to have

### P3-1. The slow path's `checked_add(1)` is immediately undone by `- 1`; the whole round-trip is provably redundant

**File:** `crates/size-classes/src/lib.rs:924-931`

```rust
let next_mult = match (block | (align - 1)).checked_add(1) {
    Some(v) => v,
    None => return None,
};
let next_idx = (next_mult - 1) >> self.min_block_shift;
if next_idx >= L - 1 {
    return None;
}
```

**Problem.** `next_mult` has exactly one use: `next_mult - 1`. And
`next_mult - 1 == block | (align - 1)` by construction. So the code computes
`x + 1` and then immediately recovers `x`, with a branch in between whose only
job is the `x == usize::MAX` case. That case is already covered by the guard
two lines down: if `block | (align - 1) == usize::MAX`, then
`next_idx = usize::MAX >> shift`, and since `small_max <= usize::MAX` and `>>`
is monotone, `usize::MAX >> shift >= small_max >> shift == L - 1` — so the
existing `next_idx >= L - 1` check returns the same `None`. The rewrite below
is therefore **bit-identical on every input**, not merely equivalent on the
reachable ones:

```rust
// `block | (align - 1)` is one below the smallest multiple of `align`
// strictly greater than `block` -- exactly the value the bucket index
// wants, so there is no reason to add 1 and subtract it again. If no next
// multiple exists (`block | (align - 1) == usize::MAX`), the shifted index
// is `usize::MAX >> shift >= small_max >> shift == L - 1`, so the guard
// below returns `None` on its own.
let next_idx = (block | (align - 1)) >> self.min_block_shift;
if next_idx >= L - 1 {
    return None;
}
i = self.size2class[next_idx] as usize;
```

**Impact.** Removes one add, one subtract and one conditional branch per
slow-path iteration. Honest caveat: when `min_block_shift` and `L` are
constant-folded (the normal case — the scheme is a `static` with a const
initializer in the consuming crate), LLVM can already prove the overflow arm
dead, so the measured win there is likely nil; the guaranteed win is in
codegen-opaque cases (a `SizeClasses` behind a reference across a
non-LTO crate boundary) and, more durably, in removing a construct that reads
as a bug ("why add 1 and subtract it again?") from the crate's only non-trivial
loop. It also deletes the sole site that needs the comment flagged in P3-2.

### P3-2. Inline comment still describes the `> small_max` clamp that no longer exists

**File:** `crates/size-classes/src/lib.rs:920-922`

```rust
// `checked_add` because `block | (align - 1)` can already be
// `usize::MAX`, meaning no next multiple exists -- which is
// exactly the `None` the `> small_max` clamp below yields for
// every other out-of-range case. Unchecked, it wrapped to `0`.
```

**Problem.** There is no `> small_max` clamp below (or anywhere) since
`060fa09` replaced it with the index-space guard; the code below is
`if next_idx >= L - 1`. Round 2 fixed the *rustdoc* passages that described the
removed guard but not this *inline* comment — the same one-of-N-copies pattern
as P2-1. Cosmetic (the two conditions are provably equivalent, and line 928
right below says "Same index-space guard as the seed above"), but it is a
third shipped copy of a phrase the source no longer uses.

**Fix.** Either delete the whole comment along with the `checked_add` under
P3-1, or change "`> small_max` clamp below" → "index-space guard below".

### P3-3. `class_for`'s domain sentence names an expression the function never computes, and thereby states the opposite of what it means

**File:** `crates/size-classes/src/lib.rs:810-812`

```
/// The useful domain is `size >= 1`; more precisely, what must hold is
/// `need = max(size, align) >= 1`, so `size == 0` alone is fine whenever
/// `align >= 1` -- `(size - 1) >> shift` never underflows in that case.
```

**Problem.** The function computes `(need - 1) >> shift` (line 894), never
`(size - 1)`. As written, the trailing clause asserts that `size - 1` does not
underflow *in the very case where `size == 0`* — which it plainly would. The
sentence is self-refuting; the intended (and correct) claim is that `need - 1`
never underflows because `need >= align >= 1`. The `size2class()` accessor doc
at lines 686-690 states the identical fact correctly ("it indexes by
`need = max(size, align)` … so the `size - 1` underflow above never applies to
it"), so the two published passages disagree.

**Fix.** `-- `(need - 1) >> shift` never underflows in that case.`

### P3-4. "see `build`'s own invariant assert **below**" points backwards

**File:** `crates/size-classes/src/lib.rs:887-888`

```rust
// `build_table` entry is a `min_block` multiple -- see `build`'s own
// invariant assert below), so `seed_idx >= L - 1 <=> need >
```

**Problem.** `SizeClasses::build`'s `debug_assert!(small_max.is_multiple_of(..))`
is at `lib.rs:637-640` — 250 lines **above** `class_for` (line 879), and it has
never been below it (`build` is the first method in the `impl` block).
Introduced by `060fa09` and carried through `5341c6b` unchanged. This is the
same directional-cross-reference defect class that task #1512 already fixed once
in this file. Note that the *other* such reference in the file — the field
comment at `lib.rs:570` ("see `build` below") — is correct, because the struct
definition genuinely precedes `build`; only this one is inverted.

**Fix.** "below" → "above".

### P3-5. `small_max` is now a redundant cached field — the same cleanup as task #1549, left half-done

**Files:** `crates/size-classes/src/lib.rs:574` (field), `:632`/`:645`
(initialisation), `:747-749` (accessor), `:609` (`Debug`)

**Problem.** After `060fa09` moved `class_for` to the index-space guard,
`self.small_max` has **no hot-path reader left**. Its only two consumers are
`small_max()` and the `Debug` impl, and both can read `self.table[N - 1]`
directly — `N >= 1` is guaranteed (`build_table` asserts `geo_count > 0`), and
`build` already computes `small_max` as exactly `table[N - 1]` at line 632. The
struct comment at `lib.rs:569-572` documents precisely this reasoning for
`min_block`/`small_align_max` ("storing only the shift removes two
provably-redundant hot-path field loads"), which is why `small_max` standing is
conspicuous: it is the same redundancy, one field over, that the cleanup
stopped short of.

**Fix.**

```rust
pub const fn small_max(&self) -> usize {
    self.table[N - 1]
}
```

and drop the field plus its `build` initialiser. Costs 8 bytes per instance and
one fewer field to keep in sync; `debug_impl_prints_a_summary_not_the_raw_tables`
(`tests/builder.rs:274-299`) keeps passing unchanged because it asserts on the
printed *value*, not the field's storage. Not urgent — it is private state, so
it is not an API freeze concern — but it is the last piece of a cleanup that is
otherwise complete.

---

## P4 — cosmetic / process

### P4-1. The full-sweep test's header comment describes a sweep shape two later edits removed

**File:** `crates/size-classes/tests/builder.rs:145-146`

```rust
// Every alignment the slow path can carry (powers of two up to SMALL_MAX),
// and every size 1..=SMALL_MAX+1, against the independent reference.
```

Both halves are now false, and the code contradicting them is in the same
function: task #1480 pushed one align *past* `SEFER_MAX` on purpose (line 158,
with its own explaining comment), and task #1519 replaced the step-by-1 sweep
above 8192 with a step-by-`SEFER_MIN_BLOCK` walk plus explicit boundary points
(lines 160-189, likewise with its own comment). A reader hits the summary first
and the two refutations 12 and 15 lines later.

**Fix.** Rewrite the two-line header to match: "Every power-of-two alignment up
to `SEFER_MAX`, plus the first one above it; sizes step-by-1 up to
`SMALL_STEP_CEIL` and step-by-`MIN_BLOCK` above it, with every table entry and
every align value explicitly included as a boundary point."

### P4-2. The bench's sharing comment still names only two of the five shared fixtures

**File:** `crates/size-classes/benches/size_classes_bench.rs:16`

"…and the **JUMP_A/JUMP_B** slow-path pairs are mechanically shared with
tests/builder.rs via this module". `JUMP_DENSE`, `JUMP_MULTI` and `JUMP_NONE`
moved into `common` too (commit `32cc1d8`) and are imported six lines below at
`:23`. `tests/common/mod.rs`'s own doc already covers all five.

**Fix.** "the `JUMP_*` slow-path pairs".

### P4-3. Third copy of the removed `need > small_max` guard description

**File:** `crates/size-classes/benches/size_classes_bench.rs:197`

"One past small_max -- the early-rejection path (`need > small_max`)". Same
issue as P3-2 — the source-level condition is now `seed_idx >= L - 1`. (The
*historical* mention at `:181`, describing what the three replaced rows used to
do, is fine and should stay.) Harmless because the two are provably equivalent,
but if P3-2 is fixed this copy should be fixed with it, or the same
fixed-in-one-place-only pattern repeats a fourth time.

**Fix.** "the early-rejection path (`need` past `small_max`, i.e. the
`seed_idx >= L - 1` guard)".

### P4-4. The `# Memory cost` byte figures are silently 64-bit-only

**Files:** `crates/size-classes/src/lib.rs:157-159`, `README.md:50`

"`table` itself is only `N * size_of::<usize>()` = **392 bytes** … ~**16.18
KiB** total". `49 * 8 = 392` assumes a 64-bit `usize`; on a 32-bit target the
same scheme's table is 196 bytes and the object is ~15.98 KiB. The crate
explicitly supports 32-bit (`tests/builder.rs:761-778` carries
`#[cfg(target_pointer_width = "32")]` fixtures, and `usize::BITS - 1` is used
instead of literal shifts precisely for portability), so the unqualified figure
is the odd one out. The `L = 16173` figure *is* width-independent and correct.

**Fix.** "= 392 bytes on a 64-bit target" (and, in the README, "~16.18 KiB total
on a 64-bit target").

### P4-5. `CHANGELOG.md` still says `Unreleased` while `Cargo.toml` says `0.1.0`

**Files:** `crates/size-classes/CHANGELOG.md:7`, `Cargo.toml:3`

Known release-checklist item (recorded twice in prior rounds); re-listed only
because it is the single remaining thing between this verdict and an actual
publish. Stamp `## 0.1.0 - <date>` in the release commit.

---

## Things deliberately not raised

- **Comment density in published rustdoc.** `src/lib.rs` is 981 lines for
  roughly 200 lines of executable code, and several passages (the rustc
  issue-74823 defence at `:171-182`, the flat-LUT stability note at `:693-705`)
  read as review-response prose. Prior rounds already adjudicated this
  deliberately; I do not think re-litigating it is a finding.
- **`build_size2class` accepting hand-built tables whose entries are not
  `min_block` multiples.** Documented at `:434-444` with a concrete worked
  example, and `tests/builder.rs:1010-1064` *depends* on the permissiveness to
  reach a release-only clamp bug. Correct as a documented tradeoff.
- **Double const-evaluation of `build_table`** (once for the consumer's `TABLE`,
  once inside `build`). Documented at `:50-54` as the price of the two never
  drifting; the alternative reintroduces exactly the drift the design removes.
- **The redundant `debug_assert!` re-check when `try_class_for` delegates to
  `class_for`.** Debug-only, and keeping `class_for` self-guarding is the right
  call.
