# `size-classes` numerics review (arithmetic / overflow / const-parity)

Reviewer: `size-classes-review-numerics` (read-only static review; no cargo, no edits, no git writes).
Target: `crates/size-classes` at working-tree HEAD `d1eb74b80e7d8baa98da6e075be42f466497b152`
(the task brief cited `1a908c0`; HEAD is one commit past it — `d1eb74b`, a docs-only commit
touching the `size2class()` sentinel-bound text, whose claims I also verified below).
Scope: every `+ - * / << >> as` in `src/lib.rs`; tests/benches fixtures re-derived; consumer
(`src/alloc_core/size_classes.rs`) skimmed for context only.

## Verdict

**GO** — no arithmetic defect found in the shipped code. One P3 (stale numeric example in
`build_table`'s `# Panics` doc — a false count, doc-only fix recommended before publish) and one
P4 (out-of-contract profile-dependent corner in `class_for`). Everything else the prior five/six
audit rounds closed is still correctly in place; I re-derived each proof independently rather
than trusting the comments.

## Findings

### F1 — P3: `build_table`'s panic doc says `geo_count = 177` overflows; under the shipped code it does not (first panicking count is 183)

`crates/size-classes/src/lib.rs:179-184` (the `177` literal is on line 183):

> … if the geometric progression's advance step overflows `usize` (… e.g. with `min_block = 16`,
> `growth = (5, 4)` …, `geo_count = 177` already overflows) …

Failure scenario (as a false claim, not a crash): a consumer sizing `geo_count` from this
documented example concludes that 177 is the validity ceiling for the crate's own example
scheme, when in fact `geo_count` up to and including **182** builds a fully valid table and
**183** is the first count that panics. The doc *understates* the accepted domain — the exact
domain that Sol-run2 P3-1 (commit `b955768`, task #1452) deliberately widened.

Provenance (verified via read-only git): the number was written at `8a277f6` (task #1448), when
the advance was `cur.checked_mul(num)` on **usize** — under that code, `177` was exactly right:
class 175 = 3914508068181469360, and `cur * 5` = 19572540340907346800 > `usize::MAX` first at
the advance into class 176, i.e. exactly when `geo_count = 177`. The u128 widening in `b955768`
moved the boundary (intermediates that overflow usize are now fine if the rounded next class
fits); the doc example was never updated. No test pins the number (verified:
`rg -n "177|182|183" tests/ benches/ src/` matches only `src/lib.rs:183`).

Exact verification (Node BigInt simulation of the shipped recurrence — `checked_mul`/`div_ceil`/
`checked_add`/mask-round/min-step-fallback in `build_table` order; not verified by cargo, this
being a read-only review):

```
$ node -e '…exact recurrence, min_block=16, growth=(5,4)…'
class 175 = 3914508068181469360
class 176 = 4893135085226836704     <- geo_count=177 builds classes 0..=176: NO panic
…
class 181 = 14932663223958852304    <- geo_count=182: still fine
class 182 = 18665829029948565392    -> > usize::MAX (18446744073709551615): panic
minimal panicking geo_count = 183   (usize-product boundary, i.e. the stale 177: 176→177)
```

(The first 8 values of the same recurrence reproduce the hand-derived GOLDEN
`[16, 32, 48, 64, 80, 112, 144, 192]` of `tests/builder.rs`, so the simulation is anchored to
the crate's actual output, not just to my reading of the code.)

Width note: the example is also width-dependent — on a 32-bit target the same params first
panic at `geo_count = 84`, so "177 already overflows" is true there only by accident of
overflowing even earlier. Whatever number replaces it should either say "on 64-bit" or be phrased
independent of width.

Recommended fix (doc-only): in `build_table`'s `# Panics`, replace `geo_count = 177` with
`geo_count = 183` (and ideally "on a 64-bit `usize`"), or drop the specific number and keep the
axis. *Overlap note:* this lives in a doc comment, so `size-classes-review-api-docs` may also
flag it; it is a measurably false numeric claim, reported here for that reason — fix once.

### F2 — P4: `class_for(0, 0)` — the crate's only unchecked, wrap-capable arithmetic — is profile-dependent, and on near-`usize::MAX` schemes it silently returns `Some` in release

`crates/size-classes/src/lib.rs:711`: `let seed = self.size2class[(need - 1) >> self.min_block_shift] …`
with `need = if size > align { size } else { align }` (line 707).

Failure scenario: `size == 0 && align == 0`. `need == 0`, so `need - 1` underflows. This requires
violating **two** documented preconditions at once (`size >= 1` is the documented well-defined
domain, lib.rs:656-657; `align` must be a power of two, and `is_power_of_two(0)` is false).
Behavior:

- debug (or checked-profile const eval): overflow panic on the subtraction — loud;
- release: wraps to `usize::MAX`; index = `usize::MAX >> shift`. For ordinary schemes this is
  astronomically ≥ L (SEFER: L = 16173) → out-of-bounds array index panic — still loud, but with
  a baffling message;
- release **and** a scheme whose `small_max` is within `2 * min_block` of `usize::MAX`
  (in-bounds ⟺ `small_max >= 2^64 − 2·min_block`): the index lands **inside** the LUT and the
  fast path returns `Some(sentinel-class)` — a silent wrong answer. This is reachable on the
  crate's **own test fixture**: `extreme64_overflow` (`tests/builder.rs:517-533`) has
  `min_block = 1<<62`, `small_max = 3<<62 = 2^64 − 2·min_block` (exactly the boundary), L = 4,
  index = `(2^64−1) >> 62` = 3 < 4 → `class_for(0, 0)` returns `Some(2)` in a release build,
  `panic!`s in debug.

This is consistent with the documented stance that a non-pow2 `align` "can only produce a wrong
CLASS CHOICE, never memory unsafety" (lib.rs:666-680) — hence P4, not higher.

Recommended fix (optional, one comparison, no observable change for any in-contract input):
add `need == 0` to the existing early rejection, e.g.
`if need == 0 || need > self.small_max { return None; }` — makes `class_for` total. Equally
defensible: leave as-is; the raw-LUT doc (post-`d1eb74b`, lib.rs:555-556) already tells direct
LUT users to `checked_sub`, and `class_for`'s own doc pins `need >= 1` via the pow2-align
contract.

## Verified clean (re-derived independently; no findings)

Every arithmetic operation in `src/lib.rs`, with its safety argument:

- `size2class_len` (lib.rs:144-153): `/` is guarded (pow2 ⇒ `min_block >= 1`); the `+ 1` is
  `checked_add` with a named panic. Overflow ⇔ `min_block == 1 && max_class == usize::MAX`
  (for `min_block >= 2`, `max_class/min_block <= usize::MAX/2`), so the doc's "only reachable
  with `max_class` within `min_block` of `usize::MAX`" (lib.rs:136-137) is a true necessary
  condition. The doc's wrap-to-0 claim (lib.rs:139-142) is arithmetically correct.
- `build_table` (lib.rs:191-355):
  - `mask = min_block - 1` (lib.rs:215): `min_block >= 1` asserted ⇒ no underflow; `min_block == 1`
    (mask 0, shift 0) degenerates correctly at every use.
  - `geo_count.checked_add(extras.len())` (lib.rs:209): checked; the diagnostic-vs-soundness
    comment (lib.rs:205-208) accurately describes the unchecked counterfactual.
  - Merge loop (lib.rs:267-329): exactly `geo_count + extras.len() == N` iterations (asserted),
    so `oi ∈ [0, N)` at every `out[oi]`; `gi/ei/oi` increments are bounded by their loop
    conditions in every profile.
  - Geometric advance (lib.rs:298-322): **the u128 widening bound is complete** — every
    intermediate is either u128-`checked_*` or asserted before the narrowing cast:
    `checked_mul` → `div_ceil` (divisor `den >= 1` asserted at lib.rs:202, so no div-by-zero;
    `div_ceil` cannot overflow by construction) → `checked_add(mask)` → `assert!(rounded <=
    usize::MAX)` → `as usize` → min-step fallback `checked_add`. No path trusts a fit. I also
    re-derived (and agree with) lib.rs:291-297: on every ≤64-bit target both u128 `checked_*`
    branches are provably dead — `(2^64−1)^2 = 2^128 − 2^65 + 1 < 2^128`, and
    `scaled + mask ≤ 2^128 − 2^64 < 2^128` — and on a hypothetical 128-bit `usize` the failure
    mode is a *loud false rejection*, never a silent wrong table. (Recorded here so the next
    audit need not re-derive it; no action.)
  - Induction: every `cur` is a multiple of `min_block` (start `min_block`; both advance arms
    preserve it), so the merged-table strict-increase check (lib.rs:338-352) is exactly the
    duplicate-catcher the docs claim, and geometric steps are always ≥ `min_block`.
  - The final geometric class is never advanced (advance guarded by `gi < geo_count` after the
    increment), so overflow *past* the last class does not spuriously panic — matches the
    `geo_count = 1` fixtures.
- `build_size2class` (lib.rs:401-478): `N > 0` and pow2 asserted; `u8::MAX as usize + 1 = 256`
  cannot overflow; `N <= 256` admits exactly indices `0..=255` and the inner scan provably
  breaks at `class_idx <= N-1` (since `need <= small_max = table[N-1]`), so `class_idx as u8`
  is lossless — the comment at lib.rs:410-414 is correct. `k + 1` (lib.rs:464) cannot overflow:
  `k < L <= usize::MAX`. `(k + 1).checked_mul(min_block)`'s clamp fold is correct: when the
  product overflows, its true value exceeds `usize::MAX >= small_max`, and the guard
  `v < small_max` clamping `v == small_max` to `small_max` is a no-op — so the entry equals the
  unwrapped-multiply answer in every case (comment at lib.rs:460-463 accurate). `need(k)` is
  non-decreasing, so the monotone pointer is sound. The `L` check delegates to `size2class_len`,
  inheriting its overflow panic (documented, lib.rs:393-395).
- `SizeClasses::build` (lib.rs:525-538): `table[N-1]` is safe — `build_table` runs first and its
  asserts force `N = geo_count + extras.len() >= 1`; `min_block.trailing_zeros()` of a nonzero
  pow2 is `<= 63 < 64`, so every `>> min_block_shift` in the crate is shift-overflow-free.
- `class_for` (lib.rs:702-749), in-contract path: `need = max(size, align) >= 1` (pow2 `align >= 1`
  or `size >= 1`); the `need > small_max` early-out bounds the index: for `small_max =
  m·min_block` (always, via `build()`), max index `= (small_max−1) >> shift = m−1 = L−2` — the
  sentinel bucket `L−1` is never read by `class_for`, exactly as documented (lib.rs:361-372).
  Fast path: `align <= min_block`, both pow2 ⇒ `align | min_block | block` — the divisibility
  shortcut is valid. Slow path: `align > min_block >= 1` ⇒ `align >= 2`, so `align - 1` (twice,
  lib.rs:729/739) cannot underflow *even for out-of-contract align*; `block & (align−1) == 0` ≡
  divisibility for pow2; `(block | (align−1)).checked_add(1)` correctly returns `None` when no
  next multiple exists (`block | mask == usize::MAX`); `next_mult >= block + 1 >= 2` so
  `next_mult − 1` cannot underflow; and because `align` is a multiple of `min_block`, the
  re-seed bucket's need equals `next_mult` exactly (not merely ≥), so the jump lands on
  precisely the smallest class `>= next_mult`; `table[new] > table[old]` ⇒ strict advance ⇒
  termination, and skipped classes all have `block < next_mult` = the next multiple of `align`,
  so none is divisible — the jump ≡ step-by-1-walk claim (lib.rs:719-721, 652-654) is correct.
- const-eval vs runtime parity: every wrap-capable op is `checked_*`/`assert!`ed
  (profile-independent), and every remaining bare op is in-range by a bound that holds in both
  profiles and both eval contexts (derived above). The crate's comments apply the correct mental
  model — const-fn *bodies* follow the Cargo `overflow-checks` profile, bare literal `const`
  items always trap — at lib.rs:139-142 and in the tests' comments (builder.rs:461-466,
  564-577). The only profile divergences: the deliberate `debug_assert!` (align pow2) and F2
  above.
- Edge inputs: `size == 1` (documented well-defined, and correct: bucket 0 → class 0);
  `align == 1` (fast path, `need = size`); `size == 0` with `align >= 1` (actually well-behaved,
  `need = align`), with `min_block == 1` and `min_block == 2^63` (extremes re-derived: `2^63`
  schemes panic loudly at the first advance or fallback-add — covered by
  `geometric_advance_overflow_panics…`/`min_step_fallback_overflow_panics…`); `N = 256` admitted
  / `257` rejected (`exactly_256_classes_build_and_index_up_to_255` /
  `exactly_257_classes_are_rejected`); `geo_count = 1` never advances.

Test/bench numeric fixtures re-derived independently (exact BigInt recurrence + LUT replay):
GOLDEN `[16,32,48,64,80,112,144,192]` ✓; sefer scheme N=49, SMALL_MAX=258752, L=16173, all nine
extras present and interleaved ✓; `JUMP_A(1025,256)` seed block 1200 (1200 mod 256 = 176 ≠ 0)
and `JUMP_B(2049,1024)` seed block 2368 (2368 mod 1024 = 320 ≠ 0) — the bench comments' claimed
seeds are correct ✓; align-128 fixture: seed 144 (144 mod 128 = 16), one hop to the 256 extra ✓;
`extreme64` table `[2^62, 2^63, 3·2^62]`, L=4, `class_for(3<<62, 1<<63) = None` via the
`checked_add` None branch ✓; hand-built-table expectation `[0,1,2,3]` ✓; linear-256 scheme
`1..=256`, L=257, top two LUT entries 255 ✓; the literals in test comments
(`2^63·3 = 27670116110564327424`, `3·2^62 = 13835058055282163712`) ✓; the release-equivalence
argument in `build_size2class_bucket_need_overflow_clamps_to_last_class`
("L·min_block = small_max + min_block = 2^64 exactly, wraps to 0") is *provable*, not just
plausible: `small_max` is a multiple of `min_block` (a pow2) below `2^64`, and the top bucket
overflows iff `(m+1)·min_block ≥ 2^64`, which then forces equality ✓.

Consumer skim (`src/alloc_core/size_classes.rs`): all arithmetic is delegation plus `const`
derivations (`TABLE_LEN = 40 + 9 = 49`, `S2C_LEN = size2class_len(258752, 16) = 16173`); the
`medium-classes`/`-wide` extras (262144 … 1835008) are all multiples of 16, strictly increasing,
and strictly above the 40-class geometric top (258752) — no numeric hazard, no finding.

## Not re-flagged (prior rounds, verified still in place)

Run-1 P2-1 (`size2class_len`'s `+ 1`, now checked) · run-1 P2-2 (merged-monotonicity chokepoint)
· run-2 P2-1 (u8 bound off-by-one, now `N <= 256`) · run-2 P3-1 / `b955768` (false overflow
rejection → u128 widening) · rust-intel §B26 / task #701 (advance multiply) · #755 F4 (min-step
fallback add) · #1417 P2-1 items 3/4 (`need` clamp, `next_mult` add) · run-3 P4-3 (widened
proof machine-checked) · run-4/5/6 doc items (including `d1eb74b`'s index-based sentinel
boundary and "need is always >= 1 given align's pow2 contract" — both re-derived as accurate).

## Overlap notes

- F1 is a doc-claim fix; `size-classes-review-api-docs` may independently report it. It is
  reported here because it is a measurably false number in a published panic contract.
- `tests/builder.rs`'s `reference_table` mirrors `build_table`'s formula byte-for-byte
  (acknowledged circularity, mitigated by the hand-derived GOLDEN test) — oracle-strength
  territory for `size-classes-review-tests`; the *numbers* in that reference and in GOLDEN are
  correct, so nothing numeric to report.
