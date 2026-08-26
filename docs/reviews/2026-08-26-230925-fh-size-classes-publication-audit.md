# size-classes — publication audit (fh, 2026-08-26 23:09)

Fresh, fully independent static-analysis run over `crates/size-classes` and its
root-crate consumers, per the audit brief: prior reports (Sol-codex runs 1–8,
rush waves 1–2) were deliberately NOT used as a source of truth — every claim
below was re-derived from the code at `HEAD` (`fa4ba69`). Read-only run: no
cargo invocations; all numeric claims re-verified by independent arithmetic
(exact big-integer replay of the builder's formulas, outside Rust).

## Verdict: **GO** — ready for first publication to crates.io

No blockers, no correctness bugs, no doc/behavior mismatches, and — the
explicit focus of item 5 — **no regressions found in the 22-commit
post-Sol-run5 wave** (`1a908c0..HEAD`). Everything found is P4 polish,
publishable as-is or trivially foldable into the release commit. Rationale in
brief:

- **Algorithms verified from scratch** (§3): the sorted merge, the
  monotone-pointer LUT builder, and the divisibility-jump slow path are each
  correct by an argument I re-derived independently (including the jump ≡
  step-by-1 equivalence proof and the termination/progress argument), not by
  trusting the doc comments.
- **Every checked-arithmetic site is genuinely needed and genuinely correct**
  (§3.4): the u128-widened geometric advance, the min-step fallback
  `checked_add`, `size2class_len`'s `checked_add(1)`, the bucket-`need`
  `checked_mul`-folds-into-clamp (with a correct "overflow ⟹ exceeds
  `small_max` ⟹ clamp is the right answer" argument), and the slow path's
  `next_mult` `checked_add(1)`-folds-into-`None`.
- **Every numeric claim in the docs re-verified by independent computation**
  (§3.5): geo_count overflow boundary 183 (64-bit) / 84 (32-bit) — both exact;
  SEFER fixture `SMALL_MAX = 258752` (~253 KiB), 49 unique classes, `L =
  16173`; bench-comment seeds 1200 (for `(1025, 256)` → lands on 2048) and
  2368 (for `(2049, 1024)` → lands on 4096); the `~16 KiB, almost all of it
  the LUT` Debug-doc claim (16,173 B LUT + 392 B table).
- **The consumer's caller-owned precondition is actually satisfied** (§5):
  the M4 address-alignment argument in `src/alloc_core/size_classes.rs`
  traces correctly through `carve_block`'s `align_up(bump, block_size)`
  (absolute multiple of `block_size` in segment-relative coordinates,
  `src/alloc_core/alloc_core_small.rs:1452`) and `os::SEGMENT = 1 << 22`
  (4 MiB-aligned base, `src/alloc_core/os.rs:65`), including the doc's
  correct observation that base alignment alone would NOT suffice.
- **CI gates are comprehensive** (§6): `cargo publish --dry-run` on every PR
  (`size-classes-gates`), bare-metal `thumbv7em-none-eabi` no_std build,
  `cargo test` in debug AND release, `clippy --all-targets -D warnings`,
  `RUSTDOCFLAGS="-D warnings" cargo doc`, and pinned-1.88 MSRV
  check/test-compile/bench-compile rows.

---

## 1. Findings (all P4 — none blocking)

### P4-1. Public growth-formula rustdoc is ambiguous about division semantics

`Params::growth` (lib.rs:84), `build_table`'s summary (lib.rs:171-172),
README:9, and CHANGELOG:16 all state the step as
`round_up(prev * num / den, min_block)`. The code computes **ceiling**
division first (`div_ceil`, lib.rs:314). Read as exact rational arithmetic
the published formula is correct (`round_up(q, m) == round_up(ceil(q), m)`
for any rational `q`); read with Rust integer-division (floor) semantics it
is not — e.g. `min_block = 16, prev = 16, growth = (65, 32)`: exact `q =
32.5` → code produces 48, a floor reading predicts 32. The internal comment
(lib.rs:294, `round_up(ceil(cur * num / den), min_block)`) and the
hand-derived golden test (`geometric_run_matches_hand_derived_golden_values`,
tests/builder.rs:483-505, "round up to an integer, i.e. ceiling division")
are both explicit — only the four public-facing copies are ambiguous.
**Suggested fix:** write `round_up(ceil(prev * num / den), min_block)` (or
"`prev * num / den` in exact rational arithmetic") in the public rustdoc /
README / CHANGELOG. One-line doc edit at four sites; entirely optional
pre-publish.

### P4-2. No 32-bit test execution anywhere; the "84 on a 32-bit" claim is doc-only

The `thumbv7em-none-eabi` CI row is a lib-only `cargo build` (compiles no
test target), and every extreme-value test in the crate is
`#[cfg(target_pointer_width = "64")]`-gated, so on a 32-bit target the
overflow tests simply vanish. The lib.rs:196 claim "84 on a 32-bit one" is
correct (verified by independent computation in this audit: first
overflowing `geo_count` = 84 for 32-bit `usize`, 183 for 64-bit), but no
test can ever pin it. **Optional improvement:** an
`i686-unknown-linux-gnu` test row (`gcc-multilib` on ubuntu) plus a small
`#[cfg(target_pointer_width = "32")]` boundary test — this would also give
the crate's ordinary tests one real 32-bit execution. Low priority: the
32-bit-relevant arithmetic is width-generic (`usize::BITS`-parametric) and
the 64-bit tests exercise the same code paths.

### P4-3. README code fence is ` ```text ` — free syntax highlighting is available

The no-doctest policy applies to doc comments in `src/**/*.rs` (compiled by
`cargo test --doc`). `README.md` is NOT doctested — lib.rs does not
`#[doc = include_str!]` it (verified) — so its example fence could be
` ```rust ` at zero test cost, restoring syntax highlighting on crates.io
and GitHub. lib.rs's own ` ```text ` fence (line 52) must stay, per policy.
Cosmetic.

### P4-4. Release-commit checklist items (process, not defects)

- `CHANGELOG.md` "0.1.0 - Unreleased" — date it in the publish commit.
- Crate-name availability for `size-classes` on crates.io cannot be checked
  offline; confirm at publish time (the CI dry-run does not check name
  ownership).
- Per the CI comment (ci.yml:721-724), add the `cargo semver-checks` row
  after the first publish lands.

### P4-5. Fixture-drift and knob-drift observations (both currently exact)

- `tests/common/mod.rs:19-21` calls the SEFER fixture "the default in-tree
  scheme" — verified TRUE today (byte-identical `EXTRAS`/`GEO_COUNT`/
  `MIN_BLOCK`/growth vs `src/alloc_core/size_classes.rs`), but it is a
  snapshot; nothing can cross-pin it (the dependency direction forbids it).
  If the root crate re-tunes its `EXTRAS`, this comment silently rots.
- Root shim `SMALL_ALIGN_MAX` (src/alloc_core/size_classes.rs:90) is defined
  independently as `MIN_BLOCK` rather than read from the crate scheme.
  Structurally equal today (the crate hardcodes `small_align_max =
  params.min_block` in `build`, lib.rs:572), and the crate-side test
  `sefer_table_matches_reference_and_is_strictly_increasing` pins
  `small_align_max() == MIN_BLOCK`; but if the crate ever grows the
  anticipated `small_align_max` knob (README:33-34 names it as the plausible
  future field), the shim constant could drift. A root-side unit test
  comparing `SegmentLayout::SMALL_ALIGN_MAX` against a freshly built
  scheme's accessor would close it (note: a `const _:` assert cannot read
  the `static SC`, E0013 — it would need `SizeClassesImpl::build(PARAMS)
  .small_align_max()` inline, or a plain `#[test]`).

---

## 2. Review of the session's 22 commits (`1a908c0..HEAD`) — the regression hunt

Full list re-read as diffs (not just subjects). Composition: 15 docs-only,
4 test-only (`d1eb74b`, `0cd60c3`, `bf64ce7`, `ff5a2ea`), 1 test/bench
refactor (`b85249a`), 1 packaging-metadata edit (`eaa3310`), and exactly
**one code change** — `c6fa927` (hand-written `Debug`). Per-commit
cross-checks against the code and against each other:

- `d1eb74b`/`2c9625d`/`0cd60c3` (the `L * min_block` non-representability
  caveat + its test): consistent — `min_block = 1 << 62, L = 4` genuinely
  makes `L * min_block = 2^64`; the doc's replacement guidance (compare to
  `small_max()`, or reason about the index) matches what
  `raw_index_via_shift_avoids_the_overflowing_l_times_min_block_bound`
  actually exercises.
- `9800297` (177 → 183 boundary): **independently recomputed — 183 is
  exact** (first `geo_count` whose advance overflows 64-bit `usize` for
  `min_block = 16, growth = (5,4)`), and the parenthetical "84 on a 32-bit
  one" is also exact. The adjacent "`geo_count` up to `182` is the
  widened-arithmetic case" is consistent: advances up to class 182 succeed
  precisely because only the true next class must fit `usize`.
- `a71b12b` (CHANGELOG signature `build_size2class(table, min_block)`):
  matches the code; the u8-pin rewording ("every class INDEX fits a `u8`,
  up to 256 classes, indices `0..=255`") matches the `N <= u8::MAX as usize
  + 1` assert and both boundary tests (256 accepted, 257 rejected).
- `cdebcfd` → `aafa09a` (the `class_for(0,0)` paragraph added, then removed
  as factually wrong): the removal is CORRECT — `class_for`'s
  `debug_assert!(align.is_power_of_two())` is lexically first (lib.rs:752),
  so a debug build panics on the align precondition, never reaching the
  `need - 1` underflow; the removed text claimed the opposite. Verified no
  dangling `(0, 0)` references remain anywhere in the crate (grep-clean).
  This pair was itself a run-7-caught regression of a run-6-wave fix — the
  final state is clean.
- `d22679e` (`pub` → `pub(crate)` in tests/common): correct and inert —
  each consumer compiles the file as a private child module of its own
  binary crate root.
- `565da79`/`65e9d3a` (root shim M4 + `SegmentLayout::class_for` domain
  docs): both verified against the actual code, §5 below. The M4 text's
  strengthened two-fact argument (SEGMENT-aligned base AND
  absolute-multiple carving) is exactly what `carve_block` implements; the
  "would silently misalign every `align = 8192/16384` request" hypothetical
  is right (meta_end is only PAGE=4 KiB-aligned).
- `727fd39`/`4b5bf37` (Cyrillic fragments, wave-1 cleanup): grep for
  `[Ѐ-ӿ]` across all shipped crate files is clean.
- `8fe10dc` (growth doc broadened to `num <= den` generally): the broadened
  claim is mathematically right, and it silently relies on `cur` being a
  multiple of `min_block` — which holds by induction (start `min_block`;
  both advance branches produce `min_block`-multiples), so
  `round_up(scaled, min_block) <= cur` whenever `scaled <= cur`. Verified;
  no counterexample exists.
- `c6fa927` (hand-written `Debug`, `feat!`): the one behavior change.
  Pre-first-release output-format change — no semver obligation exists yet;
  correctly pinned by `debug_impl_prints_a_summary_not_the_raw_tables`
  (positive fields AND negative `table:`/`size2class:` assertions);
  `finish_non_exhaustive` is MSRV-safe (1.53 < 1.88). The two follow-up
  doc corrections (`33d647a`: LUT-vs-table size split; `fa4ba69`: shim
  comment "implements", not "derives") both verified accurate — the
  ~16 KiB figure decomposes as 16,173 B LUT + 392 B table for the SEFER
  scheme.
- `ae7c66b`/`313504c` (precondition consolidation, L-derivation worked
  example): every cross-reference updated consistently — `Params::min_block`,
  `SizeClasses::build`, the fast-path paragraph, README, CHANGELOG all now
  point at `# Preconditions` instead of restating the stride-vs-address
  caveat; the crate-doc example block is byte-consistent with the README
  example and with `readme_example_compiles_and_derives_its_generics`.

**Conclusion: no fix in this wave broke a neighbor, contradicted another
doc/comment/test, or weakened a guarantee.** This is the first wave in the
series (by the brief's own account) where the regression hunt comes back
empty.

## 3. Core-code review (lib.rs, 799 lines) — independent re-derivation

### 3.1 `build_table`

- Precondition checks: pow2 `min_block`, `geo_count > 0`, `den > 0` (with a
  correct comment on why `num == 0` is NOT rejected), `N == geo_count +
  extras.len()` via `checked_add` (diagnostic-grade, correctly reasoned),
  per-entry extras checks (multiple-of, `>= min_block` — which catches `0` —
  strictly increasing), and the merged-table strict-increase pass. Each has
  a named message and a `#[should_panic]` test pinning that exact message.
- Merge: classic two-pointer; `oi` increments once per iteration and total
  iterations are exactly `N`; `cur` is only read while `gi < geo_count`
  (checked first in `take_geo`); a geo/extra TIE takes the extra first and
  produces an adjacent duplicate that the merged-table check rejects loudly.
  The advance is only computed while `gi < geo_count`, so a scheme whose
  last class sits near `usize::MAX` is not spuriously rejected by a 41st
  unused advance. All verified.
- Advance arithmetic: `u128` widening is the right call and the doc's
  rationale example (`min_block = 2^62, growth = (3,3)` at `cur = 2^63`:
  product overflows `usize`, quotient fits) is exact. The
  128-bit-`usize`-hypothetical caveat (lib.rs:308-312) is honest.

### 3.2 `build_size2class`

- The monotone-pointer loop is `O(L + N)`; `class_idx` never reaches `N`
  because the clamp guarantees `table[N-1] >= need` — so `as u8` cannot
  truncate given the `N <= 256` pin. The clamp's overflow-fold
  (`checked_mul` → `_ => small_max`) carries a correct proof: an
  unrepresentable product certainly exceeds `small_max`, so the clamp IS
  the mathematically right answer. For `k <= L-2` the unclamped value
  `(k+1)*min_block <= small_max` always (for `min_block`-multiple
  `small_max`), so mid-buckets are never accidentally clamped.
- The hand-built-table caveats (unreachable non-multiple entries; a
  non-multiple `small_max` making bucket `L-1` reachable-and-correct) are
  both accurate — re-derived on the doc's own `[16, 24, 32]` example and on
  a `small_max`-not-multiple example.
- The `L == size2class_len(...)` delegation (rather than a re-derived
  formula) is the right shape and its overflow path is pinned by
  `build_size2class_l_check_overflow_panics_instead_of_accepting_a_wrong_l`.

### 3.3 `class_for`

- Fast path: `seed = size2class[(need-1) >> shift]` = smallest class `>=
  round_up(need, min_block)` = smallest class `>= need` (classes are
  `min_block`-multiples) — minimal and correct; divisibility is vacuous for
  `align <= min_block`.
- Bucket-range safety: for `need <= small_max` (a `min_block`-multiple),
  `(need-1) >> shift <= L-2` — the clamped sentinel bucket `L-1` is
  structurally unreachable through `class_for`, exactly as documented.
- Slow path: `next_mult = (block | (align-1)) + 1` is the smallest multiple
  of pow2 `align` strictly above `block`; classes in `(block, next_mult)`
  cannot be `align`-multiples, so the jump skips only non-solutions;
  `next_mult` is a `min_block`-multiple (pow2 tower: `min_block | align |
  next_mult`), so the re-seed lookup returns the smallest class `>=
  next_mult`, whose index is strictly `> i` — progress guaranteed, loop
  bounded by `N`. The `checked_add(1)`-folds-into-`None` on
  `block | (align-1) == usize::MAX` is correct and pinned by
  `class_for_next_multiple_overflow_returns_none` (whose pre-fix
  infinite-loop narrative I verified is what the wrapped arithmetic would
  actually have done). The `while i < N` guard is belt-and-braces (LUT
  entries are always `< N`) — harmless.
- The debug-only pow2-`align` guard and its three documented release
  failure modes (fast path skips divisibility; wrong round-up; mask ≠
  divisibility, `class_for(20, 24) → 32`, `32 % 24 == 8`) are all accurate.
- The `# Preconditions` base-alignment contract is stated exactly right:
  stride divisibility preserves base alignment, cannot create it; the
  crate's inability to even `debug_assert` it is honestly flagged.

### 3.4 Checked-arithmetic inventory (complete)

Six sites, each with a reachability test: u128 advance multiply+add, the
`rounded <= usize::MAX` narrow, min-step `checked_add`
(`min_step_fallback_overflow_panics_...`), `size2class_len`'s
`checked_add(1)`, the bucket-`need` `checked_mul` (both the
`Params`-assembled clamp test and the hand-built release-answer-flipping
test), and `next_mult`'s `checked_add(1)`. No unchecked arithmetic on a
value that can overflow remains anywhere in the crate — the only bare ops
are index/shift arithmetic already bounded by earlier checks.

### 3.5 Independently recomputed numbers (exact big-integer replay)

| Claim | Site | Result |
|---|---|---|
| geo_count = 183 first overflow (64-bit) | lib.rs:194-196 | exact (182 OK, 183 panics) |
| geo_count = 84 first overflow (32-bit) | lib.rs:195 | exact |
| SEFER `SMALL_MAX = 258752` ≈ 253 KiB, 49 unique classes | tests, shim doc | exact (252.6875 KiB) |
| `L = 16173` for SEFER | tests | exact |
| `(1025, 256)` seed = 1200-class, lands 2048 | bench:56-60, tests:245-277 | exact (3 jump hops) |
| `(2049, 1024)` seed = 2368-class, lands 4096 | bench:63-67 | exact (2 hops) |
| `(128, 128)` seed = 144-class, one hop to 256 | tests:232-242 | exact |
| Debug doc "~16 KiB, almost all LUT; table a few hundred bytes" | lib.rs:520-522 | exact (16,173 + 392 B) |
| all nine SEFER extras interleave (no geo duplicates) | tests:1013-1017 | exact |
| extreme64 fixtures' tables/verdicts (`[2^62, 2^63, 3·2^62]`, `[0,1,2,3]` vs `[0,1,2,2]`, `usize::MAX`-OR case) | tests/builder.rs extreme64 module | all exact |

## 4. Hot path & const-eval — nothing worth changing

- `class_for` fast path compiles to: max, compare-vs-`small_max`,
  compare-vs-`small_align_max`, shift, one u8 load, widen. That is the floor
  for a safe-code implementation; the LUT being `u8` keeps the working set
  at ~16 KB (SEFER) — one line of scalars + one LUT byte per call.
- The slow path's mask-instead-of-modulo idiom is already the measured
  winner (per the in-code note: ~24-45% on the slow-path benches) and the
  bench rows genuinely exercise the jump body, enforced by a
  path-activation oracle test — the R30-8 convention applied inside the
  crate, which is better bench hygiene than most published crates have.
- `block_size`'s bounds check is the only theoretically shavable branch;
  removing it needs `unsafe` (forbidden by design) and the branch is
  perfectly predicted in practice. Keep.
- Options considered and rejected: dropping the `small_align_max` field
  (future knob, zero measurable cost — and `repr(Rust)` field order is
  compiler-chosen anyway); returning `(idx, block_size)` pairs from
  `class_for` (API churn for one predictable load); padding the LUT to
  absorb the `need > small_max` branch (loses the `None` signal, grows the
  table).
- Const-eval: everything is already compile-time; builder cost is
  `O(N + L)` per scheme (~16k steps for SEFER) — negligible. No build-time
  work is left on the table.

## 5. Consumer contract check (root crate)

- **M4 address-alignment proof holds.** `class_for` guarantees
  `align | block_size`; `carve_block` places blocks at
  `segment_base + align_up(bump, block_size)` — an ABSOLUTE multiple of
  `block_size` (alloc_core_small.rs:1451-1453, segment-relative, bounded by
  `SEGMENT`); `segment_base` is 4 MiB-aligned (os.rs:65,103); every align
  the scheme can serve is a pow2 `<= 16384` (default; largest pow2 dividing
  any class — extras top out at `2^14`; geo classes carry only small pow2
  factors, e.g. `258752 = 2^6 · 4043`) or `<= 2^20` (`medium-classes`),
  all dividing `SEGMENT`. Hence `block_addr % align == 0`. The shim doc's
  claim that base alignment alone would NOT suffice is also right —
  `small_meta_end` is only PAGE-aligned.
- `MIN_BLOCK >= NODE_SIZE` is genuinely `const`-asserted
  (segment_header.rs:1287).
- The `SegmentLayout::class_for` re-export doc's domain statement
  (`size >= 1` given `align >= 1`; `size >= MIN_BLOCK` is the entry points'
  clamping convention) is consistent with the crate doc's
  `need = max(size, align) >= 1` formulation — the two documents now agree
  after `65e9d3a`.
- The root hot paths go through the `SizeClasses` shim (const fn
  forwarders), not the raw `SIZE2CLASS` static; the deliberate duplicate
  table (`SIZE2CLASS` + `SC`'s embedded copy, both const-derived from one
  `PARAMS`) is documented with its cost and cannot drift.

## 6. Packaging & CI

- `Cargo.toml`: metadata complete and accurate (description no longer reads
  as a changelog fragment post-`eaa3310`; 5 keywords, valid categories
  incl. `no-std::no-alloc` — backed by the bare-metal CI build);
  `[lints] workspace = true` resolves against the root's
  `[workspace.lints.rust]` and is normalized away by `cargo publish`;
  MSRV 1.88 covers everything used (`div_ceil` 1.73,
  `finish_non_exhaustive` 1.53, `is_multiple_of` in tests 1.87);
  dev-deps (`proptest = "1"`, `bench-scale-tool = "0.1"`) are bare-version
  registry deps — the per-PR `cargo publish --dry-run` row is the standing
  empirical proof they resolve for the packaged manifest.
- CI rows for this crate: publish dry-run, thumbv7em no_std lib build,
  `cargo test --all-features` in debug AND release (release matters: the
  debug-only `class_for` guard test is correctly `#[cfg(debug_assertions)]`
  -gated), `clippy --all-targets -D warnings`, rustdoc `-D warnings`
  (docs.rs-exact: no features, no docs.rs metadata → no fifth-meta-pattern
  gap here), and 1.88-pinned check/test-compile/bench-compile. The only gap
  is P4-2 (no 32-bit test execution).
- Test suite quality is exemplary: independent reference implementations
  (with the circular-oracle risk explicitly closed by a hand-derived golden
  vector), an exhaustive ~340k-case sweep, a triple-oracle proptest
  (jump ≡ walk ≡ scan) across three distinct schemes, message-pinned
  `#[should_panic]` coverage for all ten documented panic conditions, both
  sides of the 256-class boundary, all four extreme-2^62 overflow fixtures,
  a Debug-shape pin, the README mirror test, and a path-activation oracle
  for the bench rows.

## 7. Scope honestly stated

Static analysis only, per the brief: no build/test/clippy/rustdoc was run in
this audit. Every claim above rests on reading the code at `fa4ba69`,
independent big-integer recomputation of the arithmetic, and the CI
configuration as evidence that the dynamic checks run green elsewhere. The
one thing this audit cannot rule out by construction is a
toolchain-specific compile issue outside the code's semantics — covered by
the existing CI matrix.
