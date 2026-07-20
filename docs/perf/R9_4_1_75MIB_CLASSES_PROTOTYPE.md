# R9-4 — `medium-classes-wide` prototype (1.25 / 1.5 / 1.75 MiB): density verdict

**Task:** #226 (R9-4) — the code-change follow-up to R8-9's "close the 1 MiB–
2 MiB gap" recommendation. Prototypes three new fixed size classes at
1.25 / 1.5 / 1.75 MiB behind a NEW opt-in feature `medium-classes-wide`, on
top of the existing six-class `medium-classes` substrate, and measures the
real density win (vs the review's rough 3x/2x/2x guess) before writing it into
any report.
**This IS a code-change task** (unlike the pure-measurement R9-2 / R9-3 tasks
before it): `src/alloc_core/size_classes.rs` (1 new cfg arm, 3 new constants)
and `Cargo.toml` (1 new feature entry) are touched. The change is minimal and
strictly additive — see §1.
**Date:** 2026-07-20
**Base revision:** `main` @ `c8f5f32` (R9-3 just landed; the substrate under
test is identical to R9-3's, plus this task's three-line `EXTRAS` append).
**Platform:** Windows 10 Pro x86-64 (the test-density measurement host). The
density numbers are deterministic geometry (`floor(SEGMENT / block_size) - 1`)
empirically confirmed by carve, not timings — so they are host-independent.

---

## 1. What was built — the new feature, in one paragraph

A new opt-in feature `medium-classes-wide` is added to `Cargo.toml`:

```toml
medium-classes-wide = ["medium-classes"]
```

It **implies `medium-classes`** (which transitively implies `alloc-core`) and
is NOT part of `production` or any default bundle — exactly like
`medium-classes` itself. The feature adds **exactly one new cfg arm** in
`src/alloc_core/size_classes.rs` that appends three exact classes
(1.25 / 1.5 / 1.75 MiB = `1280 * 1024` / `1536 * 1024` / `1792 * 1024`) on
top of the existing six-class `medium-classes` `EXTRAS` list. The existing
two arms (`#[cfg(not(feature = "medium-classes"))]` and the now
`#[cfg(all(feature = "medium-classes", not(feature = "medium-classes-wide")))]`
medium arm) are byte-identical to their pre-R9-4 values — see §5 for the test
that pins this. Under `medium-classes-wide` the small-class table grows
`SMALL_CLASS_COUNT` 55 → 58 and `SMALL_MAX` 1 MiB → 1.75 MiB.

**Choice of feature name.** `medium-classes-wide` was picked over alternatives
(`medium-classes-xl`, `wide-classes`, `medium-classes-1mib-2mib`) because it
reads cleanly next to the existing `medium-classes` (it IS the medium-classes
feature, widened), matches the repo's `kebab-case` feature-naming convention,
and does not promise a specific size range in the name (so future retuning of
the wide list does not invalidate the feature name).

---

## 2. The density claim, and what was actually measured

The external review (transcribed in the task brief) estimated the three new
classes would deliver ~3x / ~2x / ~2x objects-per-segment density vs the
existing Large path's 1x, using the naive `payload / block_size` arithmetic
where `payload = SEGMENT - SMALL_META_END`. The task explicitly asked to
**verify these ratios against the REAL per-segment metadata overhead** before
writing them into any report, and to **report the real numbers even if they
differ from the guess**.

### 2.1 The real segment geometry (from `SegmentLayout` constants)

```text
SEGMENT           = 4,194,304 B (4096 KiB = 4 MiB)
SMALL_META_END    =    73,728 B (72 KiB)      — per-segment metadata overhead
fresh-segment     = 4,120,576 B (4024 KiB)    — SEGMENT - SMALL_META_END
  payload (naive)
```

`SMALL_META_END = 72 KiB` is small relative to 4 MiB (1.8%), so the naive
`payload / block_size` density is within ~1 block of `SEGMENT / block_size`.
The review's guess used this arithmetic; the REAL density is one lower — see
§2.2 for the mechanism.

### 2.2 The carve-path alignment tax — why REAL density is one lower

`src/alloc_core/alloc_core_small.rs::carve_block` does
`let aligned_bump = align_up(bump, block_size);` for every carved block. This
is load-bearing: the free path derives the block start from a pointer via
`align_down(ptr, block_size)` (the class comes from the caller's `Layout`,
not from `PageMap` — `PageMap` is per-page "first class wins" and is NOT a
reliable class oracle by design; see `tests/medium_classes_correctness.rs`
item 3). So every carved block MUST sit at a `block_size`-aligned offset.

For a block size `> SMALL_META_END` (true for all three wide classes — they
are all `>= 1.25 MiB > 72 KiB`), the FIRST carved block in a fresh segment
goes at offset `= block_size`, NOT at `small_meta_end`, so the in-segment
layout is:

```text
  [ metadata ][unused: small_meta_end .. block_size][block 1][block 2]...
```

The k-th block sits at offset `k * block_size` and ends at
`(k+1) * block_size`; the carve check
`if aligned_bump + block_size > SEGMENT { return None; }` succeeds iff
`(k+1) * block_size <= SEGMENT`. So the maximum `k` (the REAL objects-per-
segment count) is:

```text
  empirical_density = floor(SEGMENT / block_size) - 1
```

— exactly **one fewer** than the naive `floor(SEGMENT / block_size)` the
review's `payload / block_size` approximates. This is the SAME mechanism that
makes the existing `medium-classes` 1 MiB class fit ~3 blocks per segment
(not the naive ~4) — see `benches/medium_size_sweep.rs` lines 13-18's "~15
fit per segment" comment for the 256 KiB class
(`floor(4 MiB / 256 KiB) - 1 = 16 - 1 = 15`).

### 2.3 The three new classes — measured density vs the review's guess

Empirically confirmed by `tests/medium_classes_wide_correctness.rs`
(`empirical_carve_matches_predicted_density_for_every_wide_class` carves
blocks of each wide class and asserts the max per-segment residency equals
`empirical_density_for(block_size)`; `report_real_density_per_wide_class`
prints the comparison table below; `--test-threads=1` output captured
2026-07-20):

```text
  class        review guess   naive         REAL          density-win?
  1.25 MiB     3              3             2             YES (2x vs Large's 1x)
  1.50 MiB     2              2             1             NO (same 1x as Large; warm-freelist win still applies)
  1.75 MiB     2              2             1             NO (same 1x as Large; warm-freelist win still applies)
```

- **1.25 MiB: REAL = 2** (review guessed 3). Delivers a real **2x density
  win** over the Large path's 1x. Confirmed behaviorally by
  `one_point_25_mib_class_proves_a_real_density_win`: carving 6 same-size
  blocks produces max per-segment residency >= 2 (the Large path would put
  every block in its own segment).
- **1.5 MiB: REAL = 1** (review guessed 2). **NO density win.** Only one
  block fits per fresh segment — the SAME 1x density as the existing Large
  path — because `floor(4 MiB / 1.5 MiB) = 2` and the §2.2 alignment tax
  takes that to 1. Confirmed behaviorally by
  `one_point_5_and_1_point_75_mib_classes_carry_no_density_win_documented`:
  carving 4 same-size blocks produces max per-segment residency == 1.
- **1.75 MiB: REAL = 1** (review guessed 2). **NO density win**, same
  mechanism as 1.5 MiB (`floor(4 MiB / 1.75 MiB) = 2`, tax → 1).
- **2 MiB (out of scope):** would also be `floor(4 MiB / 2 MiB) - 1 = 1` —
  the same 1x as the Large path, confirming the task's "explicitly out of
  scope" call on it. Closing 2 MiB would need a larger medium-arena / page-run
  layer (an 8–16 MiB segment for that sub-range), a separate, larger design
  this prototype does NOT attempt.

### 2.4 The warm-freelist consolation prize (still real, just not density)

Even for the two classes with NO density win (1.5 / 1.75 MiB), the small path
STILL delivers the warm-freelist advantage R8-9 §4.3 measured: a same-size
reuse converts the Large path's ~90 µs free (a whole-segment `VirtualFree` +
re-`VirtualAlloc` on the next alloc) into a ~60 ns freelist push/pop. So
these two classes are not useless — they convert a per-op syscall into a
freelist op for steady same-size churn. But that is the freelist win, NOT the
density win the prototype's headline was sized on; report-readers should not
conflate them.

---

## 3. The cliff moved, partially — and a new mini-cliff appeared at 1.3 MiB

Under `medium-classes-wide`, the SMALL_MAX boundary that R8-9 §5 K7 flagged
as "the relocated cliff" moves UP from 1 MiB to 1.75 MiB. But the density
win does NOT cover that whole range uniformly — it covers only 1 MiB + 1 B
through 1.25 MiB exactly (and every off-class size that rounds up to the 1.25
MiB class, e.g. 1.1 MiB → 1.25 MiB class). The moment a request rounds up to
the 1.5 MiB class (i.e. request size > 1.25 MiB), the density win evaporates
— that allocation still pays one dedicated segment's worth of reservation
footprint, the same as the Large path. So the prototype **partially** closes
the 1–2 MiB cliff (the 1 MiB–1.25 MiB sub-range gains 2x density), and a
**new mini-cliff** sits at ~1.3 MiB (rounding threshold into the 1.5 MiB
class): 1.25 MiB exact = 2x density; 1.3 MiB rounds up to 1.5 MiB = 1x
density. This mini-cliff is the same shape as every other rounding cliff in
the geometric / extras table, just at a higher absolute size.

Confirmed behaviorally by `size_just_above_one_mib_routes_into_small_path_not_large`
(1 MiB + 1 → 1.25 MiB class → Small path) and
`size_just_above_one_point_75_mib_falls_through_to_large`
(1.75 MiB + 1 → Large path, plus 2 MiB → Large).

---

## 4. Kill-gate / verdict

| # | Criterion (the task's questions) | Target / expectation | Measured | Verdict |
|---|---|---|---|---|
| K1 | Do the 3 new classes appear in the table at the right positions, MIN_BLOCK-aligned, strictly increasing? | exact | All three at `table[55..58]`, strictly increasing, 16-byte aligned; table len 58; `SMALL_MAX` = 1.75 MiB (§1, `wide_classes_present_in_table_at_the_right_sorted_position`) | **PASS** |
| K2 | Does each matching-size request classify correctly (no overflow into the wrong class)? | exact round-trip | `class_for(sz, MIN_BLOCK)` returns the index whose block_size == sz, for all three wide sizes (`each_matching_size_classifies_to_a_small_class_at_the_right_index`) | **PASS** |
| K3 | Does a request just above 1 MiB now route into the small path? | yes (was Large under plain medium-classes) | 1 MiB + 1 → 1.25 MiB class → Small (`size_just_above_one_mib_routes_into_small_path_not_large`) | **PASS** |
| K4 | Does a request just above 1.75 MiB still fall through to Large? | yes (cliff MOVED, not disappeared) | SMALL_MAX + 1 → None (Large); 2 MiB → None (Large) (`size_just_above_one_point_75_mib_falls_through_to_large`) | **PASS** |
| K5 | Does the 1.25 MiB class deliver a real density win (multiple blocks per segment)? | yes | REAL density = 2; 6 same-size allocs produce max residency >= 2 (`one_point_25_mib_class_proves_a_real_density_win`) | **PASS** |
| K6 | Do the 1.5 / 1.75 MiB classes deliver the review's claimed density win? | review guessed 2x | **NO** — REAL density = 1 for both (same as Large path). Documented + behaviorally pinned (`one_point_5_and_1_point_75_mib_classes_carry_no_density_win_documented`) | **FAIL (finding)** |
| K7 | Is the existing six-class `medium-classes` substrate undisturbed? | byte-identical topology | The first 55 entries of the -wide table are byte-identical to plain-`medium-classes`'s `EXTRAS` (the 6 medium classes at `table[49..55]`); the wide feature only APPENDS, never mutates (`wide_does_not_disturb_six_class_medium_table_topology`) | **PASS** |
| K8 | Do the encoding-headroom ceilings still hold (PageClass sentinel, hardened-ring class field)? | yes | `SMALL_CLASS_COUNT = 58 < 0xFE` (PageClass) and `<= 62` (hardened ring 6-bit field, 4 headroom values left); pinned at compile time by existing const-asserts, re-asserted at the public dbg surface (`small_class_count_is_58_and_below_every_encoding_ceiling`) | **PASS** |

### Verdict

**PROTOTYPE LANDED AS PLANNED, with an honest density downgrade.**

The feature ships as specified (`Cargo.toml` entry, three new constants, one
new cfg arm, 12 passing tests). The headline finding the task asked for —
**verify the review's 3x/2x/2x density guess against the REAL segment
geometry** — is that the guess was optimistic by exactly one block per
segment for every wide class, because the carve path's `block_size`-alignment
requirement (load-bearing for the free path's `align_down`-based block-start
recovery) wastes one block worth of capacity at the segment start. So the
REAL density is **2x / 1x / 1x**, not 3x / 2x / 2x:

- **1.25 MiB: real win (2x density).** The only one of the three that
  delivers the prototype's headline density claim.
- **1.5 MiB: no density win (1x = same as Large).** Still gains the warm
  freelist for same-size reuse, but not the density win.
- **1.75 MiB: no density win (1x = same as Large).** Same as 1.5 MiB.

The prototype therefore **partially** closes the 1–2 MiB cliff R8-9 §5 K7
identified: the 1 MiB–1.25 MiB sub-range gains 2x density (and a new
rounding mini-cliff sits at ~1.3 MiB where requests start rounding into the
no-density-win 1.5 MiB class). 2 MiB itself remains out of scope (would also
be 1x; needs a larger medium-arena / page-run layer).

### Conditions / caveats on the verdict

1. **The hardened-ring class-field headroom shrank from 7 to 4.**
   `SMALL_CLASS_COUNT <= 62` is the const-asserted ceiling
   (`src/alloc_core/remote_free_ring.rs`); adding 3 classes takes the count
   55 → 58, leaving 4 headroom values. A future further bump past 62 needs a
   wider class field; see the `entry_never_collides_with_ring_slot_empty`
   regression test.
2. **No wall-clock benchmarking was run for this prototype.** The density
   numbers are deterministic geometry (empirically confirmed by carve, not
   timings), so they are host-independent and need no statistical treatment.
   The warm-freelist latency claim in §2.4 is cited from R8-9 §4.3's
   measurement of the existing six-class medium substrate (same small-path
   mechanism), not re-measured here; re-running `benches/medium_size_sweep`
   with `--features "medium-classes-wide"` is the natural follow-up if a
   wall-clock number for the 1.25 MiB class is wanted.
3. **This report supplies evidence; it does not flip any bundle.**
   `medium-classes-wide` is purely opt-in, NOT part of `production`. Its
   promotion (if any) is a separate explicit decision depending on workload
   data showing real allocation volume in the 1.0–1.25 MiB sub-range; the
   1.5 / 1.75 MiB classes' lack of density win argues against promoting them
   into a default bundle without the page-run layer that would actually pack
   them.

---

## 5. Tests — what was added, what was run

New test file `tests/medium_classes_wide_correctness.rs`, whole-file gated
on `#![cfg(all(feature = "alloc-core", feature = "medium-classes-wide"))]`
(no-op without the feature — matches the sibling
`tests/medium_classes_correctness.rs` convention). 12 tests covering the
task's §4 (a)–(e) checklist:

```text
cargo test --features "medium-classes-wide" --test medium_classes_wide_correctness
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Twelve tests, mapped to the task's §4 items:

- **(a)** each new class classifies correctly:
  `wide_classes_present_in_table_at_the_right_sorted_position`,
  `small_max_updates_to_one_point_75_mib`,
  `small_class_count_is_58_and_below_every_encoding_ceiling`,
  `each_matching_size_classifies_to_a_small_class_at_the_right_index`,
  `size_just_above_one_mib_routes_into_small_path_not_large`,
  `size2class_o1_lookup_agrees_with_brute_force_at_wide_boundaries`.
- **(b)** the density measurement:
  `report_real_density_per_wide_class` (prints the §2.3 table),
  `empirical_carve_matches_predicted_density_for_every_wide_class` (asserts
  the empirical density formula).
- **(c)** the new Large boundary:
  `size_just_above_one_point_75_mib_falls_through_to_large` (incl. 2 MiB).
- **(d)** density win is REAL where it exists, and HONESTLY documented where
  it does NOT:
  `one_point_25_mib_class_proves_a_real_density_win`,
  `one_point_5_and_1_point_75_mib_classes_carry_no_density_win_documented`.
- **(e)** feature-OFF behavior unchanged:
  `wide_does_not_disturb_six_class_medium_table_topology` (pins the first 55
  entries byte-identical to plain-`medium-classes`). The plain-`medium-
  classes` build does not compile this file at all (`#![cfg(...)]`); its
  class-count regression guard already lives in
  `tests/medium_classes_correctness.rs::item4_small_class_count_stays_below_page_class_sentinel_range`
  (asserts 55) and `tests/segment_directory_a5.rs::medium_classes_directory_rebuild`
  (asserts 55), so R9-3's promotion-gate measurements are not disturbed.

**Existing test regression check** (plain `medium-classes`, without `-wide`):

```text
cargo test --features "medium-classes" --test medium_classes_correctness
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

All 14 pre-existing medium-classes tests still pass — the new feature's cfg
arm is strictly additive and does not touch the six-class substrate.

**fmt + clippy.** `cargo fmt --check` clean after `cargo fmt`. `cargo clippy
--features "medium-classes-wide" --all-targets -- -D warnings` reports 11
errors, all PRE-EXISTING in unrelated files (`src/alloc_core/alloc_core_small.rs`
unused-mut, `src/alloc_core/magazine_bitmap.rs` dead-code) — the identical 11
errors appear with plain `--features "medium-classes"` (the unmodified
baseline), so this prototype introduces ZERO new clippy warnings. None of
the 11 errors reference any file this task touched.

---

## 6. Recommendations

1. **Do NOT promote `medium-classes-wide` into `production` as-is.** Only the
   1.25 MiB class delivers the headline density win; the 1.5 / 1.75 MiB
   classes carry no density benefit (only the warm-freelist win, which is
   cheaper to get by other means). If promotion is considered, restrict the
   wide list to just 1.25 MiB, OR pair it with the page-run layer (next
   recommendation).
2. **The real fix for 1.5–2 MiB is the page-run layer R8-9 §5 always pointed
   to.** A larger medium arena (8–16 MiB segment for this sub-range) would
   make `floor(arena / block_size)` large enough that the alignment tax
   becomes negligible: e.g. a 16 MiB arena would fit
   `floor(16 MiB / 1.5 MiB) - 1 = 9` of the 1.5 MiB class and
   `floor(16 MiB / 2 MiB) - 1 = 7` of the 2 MiB class — real density wins.
   That is the separate, larger design R8-9 §5 K7 identified and this
   prototype deliberately does NOT attempt.
3. **Cheapest follow-up if only the 1.25 MiB win matters: drop 1.5 / 1.75
   MiB from the wide list.** A `medium-classes-wide` that ships only the
   1.25 MiB class would be an unambiguous 2x density win in its covered
   sub-range, with no "looks like a win but isn't" classes muddying the
   story. The 1.5 / 1.75 MiB classes are kept in THIS prototype because the
   task asked for all three; the measurement shows two of them do not earn
   their place on the density axis.
4. **Re-run `benches/medium_size_sweep.rs` with
   `--features "medium-classes-wide"`** if a wall-clock number for the 1.25
   MiB class's warm-freelist speedup is wanted (the deterministic density
   numbers in this report do not require it).

---

## 7. Caveats

- **Single host, single run per test config.** The density numbers are
  deterministic geometry (`floor(SEGMENT / block_size) - 1`), empirically
  confirmed by carve; they are not timings, so no multi-sample statistical
  treatment is needed or meaningful. The `--test-threads=1` run cited in §2.3
  is reproducible to the exact residency counts.
- **The `dbg_segment_id_of` diagnostic is instance-scoped** (not the
  process-wide `dbg_segments_reserved_total`), matching the convention
  `tests/medium_classes_correctness.rs::item2_*` established to avoid
  parallel-test flakiness — see that file's comment on why the process-wide
  counter is the wrong surface under `cargo test`'s default parallel
  execution.
- **The warm-freelist latency numbers in §2.4 are cited from R8-9 §4.3**,
  not re-measured in this task. R8-9 measured the existing six-class medium
  substrate's same-size reuse (the same small-path mechanism the wide
  classes use); re-measuring under `medium-classes-wide` is recommendation
  #4, not a gap in this report's density verdict.
- **`src/` and `Cargo.toml` WERE modified** (this is a code-change task, per
  the task constraints). The diff is: 1 new feature entry in `Cargo.toml`
  (`medium-classes-wide = ["medium-classes"]` with its documenting comment),
  1 new cfg arm + 3 new constants + a doc-comment update in
  `src/alloc_core/size_classes.rs`, and the new test file. No existing
  behavior is modified — see K7.
- **2 MiB is out of scope by design.** It would also be
  `floor(4 MiB / 2 MiB) - 1 = 1` — the same 1x as the Large path — so a fixed
  class for it carries no density win. Closing 2 MiB needs the larger
  medium-arena / page-run layer R8-9 §5 K7 identified, which is a separate,
  larger design this prototype does NOT attempt (see recommendation #2).
