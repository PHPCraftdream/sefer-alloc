# Wave-2 regression hunt: did wave 1's own fixes introduce new inconsistencies?

**Agent:** `size-classes-review2-regression-hunt` (read-only static review)
**Date:** 2026-08-26
**Scope:** adversarial re-read of the 12 wave-1 fix commits, hunting the crate's
documented recurring failure mode — a fix for one finding introducing a new,
different inconsistency.

## Verdict: **GO**

No open wave-1 regression above P3. Wave 1 did contain one genuine P1-class
regression (`cdebcfd`, the `class_for(0, 0)` rustdoc paragraph) — but it was
already caught by Sol-run7 and deleted by commit `aafa09a`, which landed
**while this review was in progress**. Everything still open is P3/P4.

---

## Range reviewed

Base confirmed by `git log`: `61a5b62` ("docs(size-classes): add publication
audit run 6 (Sol-codex)") is the pre-wave-1 state. The 12 wave-1 commits
(`d1eb74b`..`cdebcfd`) were read as a full diff
(`git diff 61a5b62..HEAD -- crates/size-classes .github/workflows/ci.yml`),
plus each commit body.

**Mid-review repo movement (disclosed):** HEAD advanced past `cdebcfd` while I
was reviewing — `e69d977` (Sol-run7 report, docs only) and `aafa09a` (task
#1481, deletes a 15-line rustdoc paragraph added by `cdebcfd`). All "current
line" citations below are re-read at HEAD = `aafa09a`.

---

## Finding 1 — `cdebcfd`'s `class_for(0, 0)` paragraph was false on BOTH axes
(P1 as introduced; **already fixed at HEAD** by `aafa09a`)

- **Introduced by:** `cdebcfd` (task #1475, rush-numerics F2), inserted at what
  was then `crates/size-classes/src/lib.rs:686-699`.
- **Current state:** deleted in `aafa09a`; no current line remains.
- **The regression:** the paragraph claimed `class_for(0, 0)` is "unchecked
  even in debug (the underflow itself panics there, loudly)". False:
  `class_for`'s lexically first statement is
  `debug_assert!(align.is_power_of_two(), ...)` (current lib.rs:723), and
  `align == 0` fails `is_power_of_two()`, so a debug build panics on the
  **align precondition**, never reaching `need - 1`. This directly contradicted
  the crate's own pre-existing test
  `class_for_non_pow2_align_violates_debug_assert` (tests/builder.rs:830),
  whose comment already says the debug_assert fires "before either the
  fast-path or slow-path arithmetic even runs".
- **Independent addendum (beyond what Sol-run7/#1481 filed):** the same
  paragraph's **release** claim was also imprecise. It said the wrapped index
  "lands IN-BOUNDS and silently returns `Some(sentinel-class)`" for "a scheme
  whose `small_max` sits within `2 * min_block` of `usize::MAX`". Re-derived:
  the wrapped index is `usize::MAX >> shift`; it is in-bounds iff
  `small_max >= (L - 1) * min_block`. For `build_table`-produced tables
  (`small_max` a multiple of `min_block`) that means `small_max` in the **top
  one** `min_block`-band of `usize::MAX` — but for a hand-built table
  (`build_size2class` accepts them) a `small_max` within `2 * min_block` of
  `usize::MAX` can still give `idx >= L`, i.e. an OOB **panic**, not the silent
  `Some`. So the stated sufficient condition was not sufficient. Moot at HEAD
  (paragraph deleted); recorded here because it confirms the fix-then-re-break
  pattern was real in this wave, twice over in one paragraph.
- **Recommended fix:** none needed — deletion (`aafa09a`) is the right shape;
  this finding is closed.

## Finding 2 — `bf64ce7` miscounts its own test batch: "eight" is nine
(P3)

- **Introduced by:** `bf64ce7` (task #1477), comment block at
  `crates/size-classes/tests/builder.rs:352-357`; same count in the commit
  title ("pin the eight documented # Panics conditions plus block_size's OOB
  panic").
- **Current line:** tests/builder.rs:353 — "eight documented `# Panics`
  conditions (plus `block_size`'s out-of-range panic) had zero test coverage".
- **The contradiction:** the commit adds **ten** tests. Enumerating the
  per-condition coverage that was genuinely zero before wave 1 (verified
  against `61a5b62:tests/builder.rs`'s ten pre-existing `#[should_panic]`s):
  1. `size2class_len` non-pow2 `min_block`
  2. `build_table` non-pow2 `min_block`
  3. `build_size2class` non-pow2 `min_block`
  4. `build_table` `geo_count == 0`
  5. `build_table` growth denominator `== 0`
  6. `build_table` `N` mismatch
  7. `build_table` `extras` non-increasing among themselves
  8. `build_size2class` empty table
  9. `build_size2class` plain (non-overflow) `L` mismatch

  — nine documented `# Panics` conditions, plus `block_size`'s OOB panic =
  ten tests. The commit's own body enumerates all nine ("size2class_len/
  build_table/build_size2class's pow2 checks, geo_count==0, growth
  denominator==0, N-mismatch, the per-entry extras strictly-increasing check,
  ... empty-table and plain L-mismatch asserts, and ... block_size's
  out-of-range panic") — the headline count "eight" contradicts the body's own
  list and the file's test count. Exactly the "a count that does not match"
  class this repo's audit rules call out.
- **Recommended fix:** reword the comment (and nothing else) to "nine" / "all
  ten", or drop the numeral: "every documented `# Panics` condition that had
  zero coverage". Purely a comment edit; no test changes.

## Finding 3 — `a71b12b` fixed the `u8`-pin claim in CHANGELOG but not its
README sibling (P4)

- **Introduced by:** `a71b12b` (task #1472+#1474) corrected
  `CHANGELOG.md:35` from "a compile-time pin that the class count fits a `u8`"
  to "every class INDEX fits a `u8` (up to 256 classes, indices `0..=255`)" —
  correct (matches `lib.rs`'s `N <= u8::MAX as usize + 1` and the
  `exactly_256_classes_build_and_index_up_to_255` test).
- **Sibling left saying the old thing:** `crates/size-classes/README.md:16` —
  "with a compile-time `u8` pin on the class count". This is the classic
  one-file-fixed-sibling-untouched propagation miss. (The crate-root rustdoc
  at `src/lib.rs:22` says only "a compile-time `u8` pin" with no claim about
  what is pinned — vague, but not false.)
- **Recommended fix:** align README.md:16 with the corrected CHANGELOG
  wording, e.g. "a compile-time `u8` pin on the class indices (up to 256
  classes)". Optionally tighten lib.rs:22 the same way.

## Finding 4 — `b85249a` left two dangling prose references to constants it
moved out (P4)

- **Introduced by:** `b85249a` (task #1479, shared-fixture refactor).
- **Current lines:**
  - `crates/size-classes/tests/builder.rs:3` — module doc still says "against
    sefer's own concrete parameterization (`SEFER_PARAMS` **below**)". There
    is no `SEFER_PARAMS` below anymore; it lives in `tests/common/mod.rs`
    (and is not even imported into builder.rs). Line 9 of the same doc also
    references `SEFER_PARAMS` (that one without a "below", so merely stale
    naming, not a dangling pointer).
  - `crates/size-classes/benches/size_classes_bench.rs:45` — pre-existing
    comment "Since 256 and 1024 are themselves table entries (`SEFER_EXTRAS`)"
    now names a constant that no longer exists in this file (moved to
    `common`, not imported into the bench).
- **Recommended fix:** builder.rs:3 → "(`SEFER_PARAMS` in `common/mod.rs`)";
  bench:45 → "(`SEFER_EXTRAS` in `common/mod.rs`)" or just "table entries".
  Cosmetic; no behavioral impact.

## Finding 5 — `a692c47`'s new CI comment mis-attributes the cause to
`harness = false` (P4)

- **Introduced by:** `a692c47` (task #1476), `.github/workflows/ci.yml:2067`
  — "`cargo test --no-run` does NOT compile a `harness = false` bench target
  (verified empirically ...)".
- **Current line:** ci.yml:2067-2074 (comment + the new
  `cargo bench -p size-classes --no-run` row).
- **Assessment:** the row itself is correct and valuable — per the cargo book,
  `cargo test`'s default target set is lib/bins/examples/unit-tests/
  integration-tests/doc-tests and **excludes bench targets entirely**, so the
  bench file genuinely was never compiled at 1.88, and the new row fixes it.
  The nit: the exclusion's operative cause is the target *kind* (bench
  targets are not in `cargo test`'s default selection), not `harness = false`.
  If anyone later flips the bench to `harness = true`, the comment implies
  `cargo test --no-run` would then cover it — it still would not. Trailing
  grammar nit on the line the same commit rewrote (ci.yml:2061): "the
  `proptest` dev-dependency **were** never compiled" (singular subject).
- **Recommended fix:** reword to "does NOT compile bench targets (a
  `harness = false` bench is never run as a test; bench targets are outside
  `cargo test`'s default target set — the same fact task #1395 established
  ...)" and fix `were` → `was`.

---

## Verified clean under adversarial re-derivation

Everything below was re-derived by hand (arithmetic via exact-width BigInt
replay of `build_table`'s formula); no build/test commands were run, per the
read-only constraint.

1. **`9800297` — geo_count overflow boundary 183/84/182** (lib.rs:183-188).
   Replayed `v_{k+1} = round_up_16(ceil(v_k * 5 / 4))` in exact arithmetic:
   the advance into `v_182 = 2^64`-scale first exceeds `usize::MAX` on 64-bit,
   and that advance only runs when `geo_count >= 183` → **183 panics, ≤ 182
   accepted** — the doc's numbers are exact. 32-bit: advance into `v_83`
   first exceeds → **84 panics, 83 OK** — matches "84 on a 32-bit one". The
   trailing claim "`geo_count` up to `182` is exactly the widened-arithmetic
   case ... the next class fits even though the intermediate `cur * num`
   product does not fit `usize`" is also exact: `v_181 * 5 ≈ 7.47e19 >
   usize::MAX ≈ 1.84e19` (product overflows usize, fits u128) while `v_181`
   itself fits — and this explains why the pre-fix "177" was ever written: 177
   is the boundary under the *old* intermediate-product rule that Sol-run2's
   P3-1 widening replaced. No stale "177" remains anywhere outside historical
   review docs (grep-verified).
2. **`2c9625d` — the `L * min_block` non-representability caveat**
   (lib.rs:366-370). `min_block = 1<<62, L = 4` gives `L * min_block = 2^64`,
   not representable ✓; "the builder computes it as
   `(k + 1).checked_mul(min_block)` and folds that same overflow into the
   clamp below, never evaluating the unrepresentable product" matches
   lib.rs:472-475 exactly ✓. Also checked the surrounding unchanged claim
   ("ideal need exceeds `table[N-1]`") is universally true:
   `(floor(s/mb)+1)*mb > s` for any `small_max`.
3. **`d1eb74b` — `size2class()` raw-LUT doc rewrite** (lib.rs:564-587).
   `L * min_block() == 2^64` for the cited scheme ✓; `extreme64_overflow`
   fixture exists ✓ (64-bit-gated; the doc doesn't claim otherwise); "idx ==
   L-1 in-bounds sentinel / idx >= L OOB panic" re-derived: for
   `build_table` schemes in-range sizes (`≤ small_max`) provably index at most
   `L-2`, so the sentinel zone is indeed "beyond `small_max()`" as scoped ✓;
   the `class_for` paragraph's claims (`need >= 1` from `align >= 1`;
   `need > small_max` rejected before indexing, lib.rs:727-730) match the
   code ✓. No contradiction with `build_size2class`'s hand-built-table caveat
   (lib.rs:376-380): the new text is explicitly scoped "beyond `small_max()`",
   not "only beyond".
4. **`bf64ce7` — all ten `#[should_panic]` tests hand-derived** (the task
   asked for ≥3; I derived all ten). For each: confirmed the cited assert is
   the one that actually fires **in assert-order**, and that no other panic
   message contains the expected substring. Highlights:
   - `build_table_rejects_non_increasing_extras_among_themselves`:
     `geo 4 + extras 2 == N=6` passes the N-check; `[64,32]` passes
     multiple-of-16 and `>= 16`; the per-entry `extras[1] > extras[0]` check
     (lib.rs:251-254) fires with "Params::extras: must be strictly
     increasing" — a substring unique to that site (the merged-table message
     at lib.rs:345-353 is entirely different text). Counterfactual holds:
     delete the per-entry assert and this test FAILS (the merged-table panic
     it would instead hit does not contain the expected substring). This is
     precisely the task-#730 collision class, and it does not recur here.
   - `build_size2class_rejects_wrong_l`: assert order is non-empty (N=3 ✓) →
     pow2 (16 ✓) → ≤256 ✓ → monotonic ([16,32,48] ✓ passes) → `L == 4 ≠ 5`
     fires (lib.rs:452-455). No overflow involved, so distinct from the
     pre-existing overflow-variant test ✓.
   - `build_size2class_rejects_non_pow2_min_block`: pow2 is assert #2
     (lib.rs:414-417), fires before the L-check the input would also trip —
     pins the intended site ✓.
   - `block_size_rejects_out_of_range_index`: `DOMAIN_SC.block_size(3)` on
     `table=[16,32,64]` (N=3, pinned by an earlier test) → slice-index panic
     "index out of bounds: the len is 3 but the index is 3" — bounds panics
     are release-active, so the test is valid in both CI profiles ✓.
   - The remaining six (`size2class_len` pow2, `build_table` pow2 / geo==0 /
     den==0 / N-mismatch, `build_size2class` empty) each fire their intended
     first-tripped assert with exact-string matches; none has an earlier
     assert that the input also violates.
5. **`ff5a2ea` — T3 accessor pins + T5 large-align-alone**. T3:
   `min_block() == 16`, `small_align_max() == 16` both match `build`'s field
   assignments (lib.rs:540-543) and the documented `small_align_max ==
   min_block`. The claim "zero call sites anywhere in the suite" is true and
   in fact understated — `git grep` at `61a5b62` shows **zero call sites
   repo-wide**, production included. T5: computed the SEFER table from
   scratch: `SEFER_MAX = 258752`, so the loop's exit value `a = 262144` is
   indeed the first power of two strictly greater, and `align = 262144 >
   small_max = 258752` drives `need > small_max` via `align` alone for every
   `size` in the sweep, with the reference classifier agreeing (`None`) ✓.
6. **`b85249a` — shared-fixture refactor**, the task's specific question 4:
   every constant resolves from `common` — builder.rs imports
   `{JUMP_A, JUMP_B, SEFER_EXTRAS, SEFER_GEO, SEFER_MAX, SEFER_MIN_BLOCK,
   SEFER_N, SEFER_SC, SEFER_TABLE}`, all used; the bench imports
   `{HUGE_THRESHOLD, JUMP_A, JUMP_B, SEFER_MAX, SEFER_SC}`, all used; no
   unused imports on either side (and `cargo clippy --all-targets -D warnings`
   at ci.yml:1865 would catch one anyway). `common/mod.rs`'s own four imports
   are all used. Values are byte-identical to both former definitions
   (checked in the pre-wave-1 blobs). No live "keep in sync" comment survives
   (only three historical mentions describing its removal).
   `tests/proptest_builder.rs` shares nothing it should: its three schemes are
   deliberately DIFFERENT parameterizations (its module doc says so; scheme A
   is sefer-*like*, geo 32 with 5 extras, not the 40/9 SEFER scheme), and
   wiring it to `SEFER_*` would defeat its stated purpose — independence is
   intentional and still valid. Only defects: the two dangling prose refs in
   Finding 4.
7. **`0cd60c3` — extreme64 raw-index test**: `(3<<62 + 1 - 1) >> 62 == 3 ==
   L-1` and `(usize::MAX - 1) >> 62 == 3 == L-1` ✓; `size2class()[3] == 2 ==
   N-1` matches the clamp pinned by the neighboring test ✓; for this scheme
   no `size` can reach `idx >= L` (would need `size >= 2^64 + 1`), which is
   exactly the doc's warning case, so the test and doc agree ✓.
8. **`eaa3310` — Cargo.toml description**: new text is self-contained,
   accurate, consistent with README/lib.rs ("falls through to the caller's
   whole-segment path"), 283 chars ( comfortably within crates.io limits),
   no TOML quoting hazard ✓.
9. **`a71b12b` — CHANGELOG**: corrected signature
   `build_size2class(table, min_block) -> [u8; L]` matches lib.rs ✓; index-vs-
   count wording matches the assert and the 256-class boundary tests ✓ (the
   README sibling is Finding 3).
10. **`d00788e` — dropping the hardcoded "14"**: removes a stale count the
    right way; surrounding claims untouched and still accurate ✓.
11. **`a692c47`'s factual premise** (independent of Finding 5's phrasing
    nit): `cargo test`'s documented default target set excludes bench
    targets, so the row closes a real 1.88-coverage gap; the "only thing in
    any job that type-checks the bench on the pinned 1.88 toolchain" claim is
    accurate (the stable-toolchain `clippy --all-targets` row covers the bench
    but not at 1.88) ✓.

## Out of scope / not assessed

The production consumer (`alloc-core`'s shim) was not examined — that is
`size-classes-review2-consumer-integration`'s scope. No overlap findings to
report from my side.

## Commands run

Read-only only: `git log/show/diff/grep`, `rg`, `sed`, `grep -n`, one
`fetch` of the cargo book (cargo-test(1) target-selection semantics), and
exact-arithmetic replays of `build_table`/`size2class_len` math via `node`
BigInt. No cargo build/test/check/clippy/doc, no git writes, no file edits
other than this report.
