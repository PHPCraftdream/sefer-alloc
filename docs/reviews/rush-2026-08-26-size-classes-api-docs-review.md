# `size-classes`: public API contract & documentation-consistency review (Rush)

**Reviewer:** rush (review agent 1 of 3 — `size-classes-review-api-docs`; sibling scopes:
`size-classes-review-numerics` arithmetic/overflow, `size-classes-review-tests` oracle
strength/CI)

**Date:** 2026-08-26

**Mode:** read-only static review. No `cargo` command of any kind was run; no git write
command was run. Verification beyond reading was done with exact `node` BigInt
arithmetic simulations of the documented algorithms (commands and outputs in the
appendix).

## Reviewed state — important drift note

The task specified HEAD `1a908c023fd4618bef187fb23961d3774cde641e`. **The repository
moved during this review**: the actual working-tree state reviewed is
`d00788e774f9cc6fbd73fd1c639698d10092ac04`, four commits ahead of the stated point:

| commit | content |
|---|---|
| `61a5b62` | adds the run-6 Sol-codex report |
| `d1eb74b` | task #1467 — fixes run-6 P2-1+P3-1 (`size2class()` `L * min_block()` bound, "applies both guards") |
| `0cd60c3` | task #1468 — fixes run-6 P4-1 (extreme64 raw-index contract test) |
| `d00788e` | task #1469 — fixes run-6 P4-2 (stale CI "14 tests" comment) |

All findings below are verified against `d00788e` (line numbers cite that state). The
first read of `src/lib.rs` in this session predated `d1eb74b` becoming visible; every
cited line was re-read against the final state. The three run-6 fixes were themselves
reviewed (see "Verified clean") and are sound; nothing below re-flags them.

## Verdict

**NO-GO pending one specific fix (P2-1).** Expected verdict immediately after that
one-line doc correction: **GO**. Everything else found is P4-nit level; all other
surfaces re-derived from the code came back internally consistent.

---

## P2-1 — `build_table`'s `# Panics` example number is stale: `geo_count = 177` does NOT overflow under the current (u128-widened) arithmetic

**Where:** `crates/size-classes/src/lib.rs:179-184` (the count itself on line 183),
inside `build_table`'s `# Panics` paragraph.

**Claim as published:**

> if the geometric progression's advance step overflows `usize` (reachable not just
> with an extreme `min_block`/`growth` combination but also with a large enough
> `geo_count` alone -- e.g. with `min_block = 16`, `growth = (5, 4)` (this crate's own
> tests' example scheme; the crate itself has no defaults), `geo_count = 177` already
> overflows)

**What the code actually does (exact simulation of the shipped algorithm —
`ceil(cur*5/4)` in `u128`, round up to multiple of 16, `<= usize::MAX` check, on a
64-bit target):**

- `geo_count = 177` **builds fine.** Class #177 = 4,893,135,085,226,833,704, well
  under `usize::MAX` = 18,446,744,073,709,551,615. So do 178…182.
- The **first** overflowing `geo_count` is **183** (advance into class #183 computes
  round_up(18,665,829,029,948,565,380, 16) > `usize::MAX`).
- On a 32-bit target the same scheme first overflows at `geo_count = 84`, so the
  unqualified count is wrong in the other direction there too.

**Provenance (this is the crate's signature "fix A, sibling claim survives" defect,
caught this time):** the number was introduced by commit `8a277f6` (task #1448,
17:48), whose own commit message says it was "Verified independently via a node
simulation of the exact advance algorithm (not the review's own rough ~182
estimate)". That verification was correct **for the code as it existed at 17:48** —
40 minutes later, commit `b955768` (task #1452, 18:28, Sol-run2 P3-1) widened the
multiply to `u128`, moving the first-overflow boundary from 177 to 183, and the
example was never recomputed. (Verified: under the pre-widening `usize` multiply,
the product `cur * 5` first exceeds `usize::MAX` exactly when advancing from class
#176 = 3,914,508,068,181,469,360 — i.e. 177 was exactly right then. The dismissed
"~182 estimate" was in fact closer to today's truth.)

**It also directly contradicts a sibling doc.** `CHANGELOG.md:81-85` states the
widening's purpose: "a scheme whose next class fits is not rejected merely because
an intermediate product does not." `geo_count = 177` is *precisely* such a scheme:
the intermediate product 3,914,508,068,181,469,360 × 5 = 19,572,540,340,907,346,800
overflows `usize`, while the next class (4,893,135,085,226,833,704) fits. The
CHANGELOG says the crate accepts exactly this; `lib.rs`'s example asserts it panics.
Both cannot ship together in a first release whose selling point is contract
precision.

**Why it matters to a reader:** this parenthetical is the *only* concrete guidance in
the public docs on how large `geo_count` can be before the overflow panic fires. A
reader sizing a table reads "177 already overflows" and (correctly, on 64-bit)
concludes 178–182 are illegal — they are not — or, on a 32-bit target, assumes
headroom down from 177 that does not exist (the failure is still a loud const-eval
compile error, so this cannot produce silent corruption; the defect is
misinformation, not misbehavior).

**Recommended fix (doc-only, one line):**

```text
`min_block = 16`, `growth = (5, 4)` (this crate's own tests' example
scheme; the crate itself has no defaults), `geo_count = 183` already
overflows on a 64-bit `usize` (84 on a 32-bit one; the boundary scales
with `usize::BITS`)
```

(Optionally add: "— `geo_count` in 178..=182 is exactly the widened-arithmetic case
the CHANGELOG describes: the next class fits even though the intermediate
`cur * num` product does not fit `usize`.") Counterfactual for any future guard: a
test pinning "177 builds, 183 panics" breaks if the doc number and the code ever
drift apart again — that suggestion belongs to the tests reviewer's scope, noted
here only for the overlap.

**Severity rationale:** P2 rather than P0/P1 because it cannot induce wrong runtime
behavior (worst case a user avoids five legal table sizes, or hits a self-explanatory
compile error early); P2 rather than P3/P4 because it is a verifiably false concrete
claim inside a public `# Panics` contract, contradicting the crate's own CHANGELOG,
and this crate's six audit rounds have consistently treated exactly this shape as a
pre-publish fix.

---

## P4-1 — `build_size2class`'s top-bucket sentence still derives the boundary as `L * min_block` without the non-representability caveat the crate just adopted elsewhere

**Where:** `crates/size-classes/src/lib.rs:361-364`:

> EXCEPT the top bucket `L - 1`, whose ideal `need` (`L * min_block`) exceeds
> `table[N - 1]` (the largest class), so no such class exists; that bucket is clamped…

This is *descriptive* of the builder (which computes `need` via `(k + 1).checked_mul`
and folds overflow into the clamp — see the code comment at the `need` computation),
so it is accurate as mathematics and is not a copyable guard. But `d1eb74b` just
established, two hundred lines below (`size2class()`'s doc), the policy that this
bound must never be derived as a byte size because `L * min_block()` is not
guaranteed to fit `usize` even for a valid scheme (`min_block = 1 << 62, L = 4` →
2^64). The two passages now read inconsistently for the same expression. Suggested
parenthetical after "(`L * min_block`)":

```text
(mathematically — not representable in `usize` for extreme schemes; the
builder's `checked_mul` on `(k + 1) * min_block` folds exactly that
overflow into the same clamp)
```

## P4-2 — CHANGELOG: "a compile-time pin that the class count fits a `u8`" is off by the 256-class case the code deliberately accepts

**Where:** `crates/size-classes/CHANGELOG.md:34-35`.

The code (`lib.rs:415-418`) and its rustdoc deliberately accept **exactly 256
classes** (`N <= u8::MAX as usize + 1`) because the entries are indices `0..=255`;
this was itself the subject of run-2's off-by-one fix (`N < 256` → `N <= 256`).
"the class count fits a `u8`" is false for the accepted 256-class table (256 does
not fit `u8`; its *indices* do). The README's phrasing ("a compile-time `u8` pin on
the class count") and lib.rs's ("the largest representable table has 256 classes,
indices `0..=255`") are both correct. Suggested CHANGELOG wording: "with a
compile-time pin that every class *index* fits a `u8` (up to 256 classes)". 

## P4-3 — `Cargo.toml` description: "(the 'falls to a whole segment' bug class, fixed)" is cryptic and oddly tensed for crates.io metadata

**Where:** `crates/size-classes/Cargo.toml:7`.

The parenthetical is accurate in intent (the crate supplies the classifier whose
absence causes that failure mode) but "…, fixed" reads as a changelog fragment, and
"falls to a whole segment" has no meaning to a reader without the README's context.
Suggested: "(fixes the 'large-alignment request silently falls through to a
whole-segment allocation' bug class)". Everything else in the description, the
keywords (`allocator`, `size-class`, `slab`, `alignment`, `no-std` — all valid
crates.io keyword syntax, ≤5), and the categories (`memory-management`,
`data-structures`, `no-std::no-alloc` — all real slugs, accurate for a zero-dep
`no_std` non-allocating crate) check out.

## P4-4 — CHANGELOG signature shorthands drop real parameters

**Where:** `crates/size-classes/CHANGELOG.md:14` (`build_table(params)` — omits the
`N` const generic, acceptable prose) and `CHANGELOG.md:32`
(`build_size2class(table) -> [u8; L]` — **omits the required `min_block` argument**,
the only entry in the list whose shorthand would not compile if copied). Suggested:
`build_size2class(table, min_block) -> [u8; L]`.

---

## Verified clean (commands/outputs in the appendix; not re-flagging closed items)

- **Run-6 fixes re-derived as correct.** `d1eb74b`'s rewritten `size2class()` doc
  (lib.rs:553-580): the `checked_sub(1)` guard advice, the `idx == L-1` vs `idx >= L`
  formulation, the `min_block = 1 << 62, L = 4 → 2^64` overflow example (arithmetic
  checked: `size2class_len(3<<62, 1<<62) = 4`), and the corrected "class_for avoids
  both pitfalls / indexes by `need = max(size, align)`" claim all match the code.
  `0cd60c3`'s new test and `d00788e`'s CI-comment fix are consistent with their
  commit messages.
- **`#[must_use]` hygiene: 15/15 pub fns** (`grep -c '#\[must_use\]'` = 15; the 15
  public functions enumerated: `Params::new`, `size2class_len`, `build_table`,
  `build_size2class`, `build`, `table`, `size2class`, `min_block`, `min_block_shift`,
  `small_align_max`, `small_max`, `count`, `block_size`, `is_huge`, `class_for`).
  No gaps, no inconsistencies.
- **`#[non_exhaustive]` hygiene:** `Params` has it with the `const fn new`
  construction path; `SizeClasses` has all-private fields (struct literals
  impossible, field addition already non-breaking), so omitting it there is correct.
- **README example fidelity:** the `​```text` example (README.md:39-58) matches
  `tests/builder.rs::readme_example_compiles_and_derives_its_generics` (builder.rs,
  last test in file) constant-for-constant and step-for-step; all named methods
  exist with the shown signatures; `build_table::<N>(&PARAMS)` (by-ref) vs
  `SizeClasses::build(PARAMS)` (by-value) are both shown correctly. No magic
  numbers: exact simulation confirms the derived values — `TABLE[N-1] = 258752`,
  `N = 45`, `L = 16173` — so the run-2 P4-1 fix (hand-pinned `258_752` removed)
  still holds. No runnable rustdoc fence exists in `src/` (`grep '```'` → none),
  per the no-doctests policy.
- **The "~16 KiB" Copy-justification claim** (lib.rs:498, CHANGELOG:87) verified:
  the real consumer scheme (`src/alloc_core/size_classes.rs`, 40 geo + 9 extras)
  gives `SMALL_MAX = 258752`, `L = 16173`, `SizeClasses` = 49·8 + 16173 + 5·8 =
  **16,565 B ≈ 16.2 KiB**. Accurate.
- **Alignment/base-address precondition consistency:** the stride-divisibility vs
  carve-base-alignment distinction is stated identically at every site — crate doc
  (lib.rs:30-34), `Params::min_block` (66-70), the `build_table` extras comment
  (217-221), `build` (517-521), `small_align_max` (600-601), `class_for`
  (633-645, 692-700), README (24-28), CHANGELOG (73-77). No residual "blocks aligned
  by construction" phrasing survives anywhere in the crate.
- **The 256/257 boundary** is stated consistently (lib.rs:396-398, 415-418; tests
  pin both sides) modulo the CHANGELOG wording in P4-2 above.
- **Cross-surface agreement** of the huge-threshold-is-policy claim, the
  `O(buckets + classes)` monotone-pointer description, the `growth = (0, den)`
  linear-degradation blessing, the fast/slow-path split at `align <= min_block`, and
  the extras "three machine-checked preconditions" list: identical in lib.rs,
  README, and CHANGELOG wherever each is repeated.
- **`size2class_len`'s overflow parenthetical** ("only reachable with `max_class`
  within `min_block` of `usize::MAX`") is a true necessary condition as stated
  (in fact it is reachable only at `min_block == 1`, which the phrasing does not
  contradict).
- **`class_for(20, 24)` non-pow2 example** (lib.rs:679-680): `32 & 23 == 0` while
  `32 % 24 == 8` — example is arithmetically correct.

## Out of scope / notes for the other reviewers

- The suggested regression test for P2-1 ("177 builds / 183 panics on 64-bit")
  belongs to `size-classes-review-tests` — noted as an overlap, not pursued here.
- `benches/size_classes_bench.rs` JUMP_A/JUMP_B constants match the "keep in sync"
  test comment (verified by grep); deeper oracle strength is the tests reviewer's.
- The untracked `.check-*.log` files in the repo root are not this review's
  artifact.
- No defects were found in files outside the crate under review; the consumer
  `src/alloc_core/size_classes.rs` was read for context only and its "~253 KiB /
  49 classes / ~16 KiB" claims are consistent with the crate's actual output.

## Appendix — verification commands and key outputs

Arithmetic verification used `node` with exact BigInt arithmetic mirroring
`build_table`'s advance step (`scaled = ceil(cur*num/den)`, `rounded = (scaled +
min_block - 1) & !(min_block - 1)`, min-step fallback, `> usize::MAX` check):

```text
geo_count 177 -> ok, last class #177 = 4893135085226836704   (< usize::MAX)
geo_count 182 -> ok, last class #182 = 14932663223958852304  (< usize::MAX)
geo_count 183 -> OVERFLOW advancing into class #183
first overflowing geo_count, 64-bit usize: 183
first overflowing geo_count, 32-bit usize: 84
pre-widening (bare u64 cur*num product): first overflowing geo_count: 177
README example: N = 45, max_class = 258752, L = 16173
SEFER scheme:  N = 49, SMALL_MAX = 258752, SizeClasses = 16565 bytes
```

Git (read-only) used to establish provenance:
`git log -S "geo_count = 177"` → `8a277f6` (17:48);
`git log -S "as u128"` → `b955768` (18:28);
`git log --oneline 1a908c0..HEAD` → the four commits in the drift table above.
