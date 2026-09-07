# globalalloc-model — independent read-only review, run 5

**Verdict: not clean — 0 P0, 0 P1, 0 P2, 2 P3, 25 P4; both P3s are missing
verification (one uncovered soundness-critical branch, one residual hole in
round 4's own drift-check), and for the third round running there is no defect
in what the crate does at runtime.**

Reviewed at `9c81fb6` (`main`), read-only. Scope as briefed: `src/lib.rs`,
`src/raw_allocator.rs`, `src/op.rs`, `src/config.rs`, `src/drive.rs`,
`src/strategy.rs`, `src/arbitrary_stream.rs`, `Cargo.toml`, `README.md`,
`CHANGELOG.md`, all six `tests/*.rs`, `fuzz/fuzz_targets/global_alloc_ops.rs`,
and every `globalalloc-model` section of `.github/workflows/ci.yml` and
`release.yml`. No `cargo`/build/test/lint command was run; the CI drift-check
was re-derived by executing its own shell text-processing pipeline against the
real `ci.yml` and the real `tests/` listing (pure text processing, no build).

Trend at P0–P3: **47 → 14 → 10 → 5 → 2.**

---

## 0. Round-4 fix verification — what I re-derived independently

### (a) The new CI drift-check step — HOLDS for its motivating class, with two constructible false negatives

I ran the step's own pipeline (`.github/workflows/ci.yml:925-943`) verbatim
against the current tree:

```
LISTED  = config_validate double_free_no_op miri_bounded oracle_negative system_arbitrary system_proptest
ACTUAL  = config_validate double_free_no_op miri_bounded oracle_negative system_arbitrary system_proptest
diff    = clean
```

- **The `awk` job-block extraction is correct.** `/^  globalalloc-model-miri:/`
  … `/^  [^ ]/` yields exactly the 74 lines of that job and stops at `clippy:`.
  No line inside the job starts at two-space indent (the `run: |` key sits at
  column 8, so its block scalar can never be indented to 2), so the extractor
  cannot terminate early.
- **The self-reference is harmless.** The step's own line
  `| grep 'run: cargo miri test -p globalalloc-model' \` survives the first
  `grep` (it contains that literal) but carries no `--test`, so it contributes
  nothing. The sibling line `| grep -o -- '--test [a-zA-Z0-9_]*' \` does contain
  `--test`, but it does *not* contain the first grep's literal, so it is
  filtered out one stage earlier. Both are load-bearing accidents of the current
  two-line formatting, not invariants.
- **Substring collisions are NOT a false-negative mode.** `[a-zA-Z0-9_]*` is
  greedy and the names are space-delimited, so `--test system_proptest` yields
  `system_proptest`, never `system_prop`; a `foo` / `foo_bar` pair cannot mask
  each other.
- **Scenario A — a 7th test file added, neither miri step updated (the exact
  three-round recurrence): CAUGHT.** I injected `new_thing` into the `actual`
  side and the diff fails with the missing entry, as intended. The structural
  fix does close the class it was built for.
- **Scenario B — a 7th test file added to ONE of the two passes: NOT caught.**
  See **P3-2**.
- **Scenario C — a run step commented out: NOT caught.** See **P3-2**.

Also verified: the drift risk is miri-only. The `globalalloc-model` rows in the
gates job (`ci.yml:2136-2137`) run `cargo test -p globalalloc-model
--all-features` with no `--test` filter, so a new test file is picked up there
automatically; only the `-Zmiri-ignore-leaks`-scoping filters need the list.

### (b) The five new zero-clamp tests and the `Faulty` preconditions — HOLD, and the third edge case the brief posits is provably unreachable

- **`(isize::MAX / align) * align` is exactly `Layout`'s own ceiling, with zero
  margin.** For a power-of-two `a`, `isize::MAX` has every low bit set, so
  `isize::MAX mod a == a - 1` and `(isize::MAX / a) * a == isize::MAX - (a - 1)`
  — byte-identical to `Layout`'s internal `max_size_for_align`. So
  `layout_for` after the clamp really is infallible (`drive.rs:247`, `:305`),
  and `Faulty::realloc`'s new upper-bound assert (`oracle_negative.rs:287-293`)
  is *exactly* `GlobalAlloc::realloc`'s precondition — neither loose nor strict.
- **The posited "ceiling reachable at 0" case cannot happen.** `validate_align`
  runs first and only admits an `align` for which
  `Layout::from_size_align(1, align)` succeeds — i.e. a power of two with
  `1 <= align <= isize::MAX`. Then `ceiling >= align >= 1`, so
  `size.clamp(1, ceiling)` can never be called with `min > max` and can never
  panic. There is no third direction: `clamp` raises only when the input is
  `< 1`, so `size > original_size` implies `original_size == 0` **always**, and
  the clamp-UP message's "GlobalAlloc forbids a zero-size layout" text is
  unconditionally accurate.
- **The two message pins do distinguish the directions in every case.** The
  clamp-DOWN pin's expected substring (`[clamped from 18446744073709551615],
  align=8) returned null — note: the harness does not model`) cannot match the
  UP message (which has no `— note:` tail), and the UP pin's expected substring
  (which contains the `— GlobalAlloc forbids a zero-size layout` bracket) cannot
  match the DOWN message. I byte-checked the em dash and the single space either
  side of it in `drive.rs:259` against `oracle_negative.rs:592` (`cat -A`:
  `M-bM-^@M-^T` = U+2014 in both, one space before the line-continuation
  backslash, so the assembled string is `0 — GlobalAlloc`) — the pin matches the
  literal exactly.
- **Counterfactual strength, re-derived:** reverting the message split makes
  `clamped_up_null_alloc_gets_no_oom_note` fail (the old shared message lacks the
  bracket); reverting the clamp itself makes all three
  `zero_size_*_is_clamped_up_not_rejected` tests fail on the fake's own
  precondition assert rather than on UB. `clamped_down_null_alloc_names_oom_note`
  pins only the DOWN message's content (it would survive a revert of the split),
  which is the correct division of labour between the two pins.
- **No gap in the new asserts.** `Faulty::alloc` / `alloc_zeroed` need only
  `size > 0` — the `isize` bound is already carried by the `Layout` type itself,
  so there is nothing else to assert; `realloc` takes a bare `new_size` and
  therefore needs both asserts, and has both.

### (c) `RawAllocator::realloc`'s new caller-obligation bullet vs. what `Faulty` actually asserts — consistent, but the trait doc under-states it for direct implementors

No contradiction: the fake's asserted preconditions are exactly the
`GlobalAlloc` triple, and `drive` upholds that triple for **every** implementor
(its own doc, `drive.rs:186-192`, states the totality promise unconditionally).
But the trait's caller-obligations list (`raw_allocator.rs:49-55`) attaches that
triple only to "a caller going through the blanket `GlobalAlloc` impl", and
`Faulty` implements `RawAllocator` **directly**. A direct implementor reading
only the trait contract therefore cannot learn the range `drive` will call it
within — the guarantee its asserts rely on lives one page away. Documentation
scoping only; filed **P4-4**.

### (d) `Config::validate()`'s rewording — code untouched, and the rewording is true except for one residual quantifier

- `git show 04f479a -- src/config.rs` touches **only** doc comment lines; the
  `pub fn validate` body (`config.rs:118-141`) is byte-identical to before. The
  "no behavior change" claim holds.
- The substantive claims are now **true**: "sanity ceiling, NOT an admissibility
  guarantee" ✔; "the largest size any `Layout` can admit is align-dependent —
  `(isize::MAX / align) * align`" ✔ (proved above); "exactly the ceiling `drive`
  clamps sizes to" ✔; "strictly below `isize::MAX` for any `align > 1`" ✔
  (`isize::MAX - a + 1 < isize::MAX` iff `a > 1`).
- `bounds_at_the_ceiling_pass_validation`'s reworded comment
  (`config_validate.rs:72-87`) is also now correct: `isize::MAX` is
  `Layout`-admissible only at `align == 1`, and under the inherited
  `max_align: 4096` the front-ends do emit `small_max + 1 = 2^63` which `drive`
  clamps.
- One residual overstatement survives in the two *field* docs (not in the
  `# Panics` block, which words it correctly as "**can** generate"): **P4-1**.

### (e) Other round-4 edits, re-derived

- **`bound_size`'s dropped `.max(1)`** (`arbitrary_stream.rs:33-37`) is safe:
  `arbitrary_with_config` calls `validate()` first, which `assert!`s the weight
  sum `> 0` — an `assert!`, not a `debug_assert!`, so it holds in `--release`
  too. `bound_size` is private and reachable only through that path, so the
  modulus can never be 0.
- **The `saturating_sub`/`saturating_add` in the large arm** is still required
  and its comment now cites the reachable case (`large_max <= small_max` at the
  ceiling), not the rejected `usize::MAX` one.
- **`align_strategy`'s `a != 0` guard** comment (`strategy.rs:33-38`) is exactly
  right: `validate()` caps `max_align` at `isize::MAX`, so the largest admissible
  power of two is `2^62` and the `1 << 63` shift is unreachable through the
  front-end.
- **Both front-ends still agree on every `Config` boundary I could construct**
  (`small_max == 0` → an arm of exactly `1`; `large_max <= small_max` → an arm of
  exactly `small_max + 1`; a single zero weight → that arm is never picked), with
  one newly-noticed exception at very large arms: **P4-2**.
- **Workspace plumbing that the crate's own claims rest on:** `resolver = "2"` is
  set at `Cargo.toml:87`, so the std-featured dev-dependency `proptest` really is
  kept out of the bare-metal `--features proptest` row — the manifest comment at
  `crates/globalalloc-model/Cargo.toml:48-59` is accurate, not aspirational.
- **`fuzz/Cargo.lock`** (round 4's P4-11) is synced by `9c81fb6`; no
  `racy-ptr-cell` entry remains.
- **Release wiring** is complete: the `globalalloc-model-v*` tag trigger, the
  `workflow_dispatch` choice, the leaf-crate ordering note, and the
  `--all-features` extra test row for `default = []`
  (`release.yml:82`, `:98`, `:58-65`, `:444-449`).

---

## P0 — none

## P1 — none

## P2 — none

---

## P3

### P3-1 — the `AllocZeroed` arm's null branch has no counterfactual anywhere, and deleting it dereferences null

*Axis: улучшение (missing coverage) — of a branch whose removal is UB, not just
an unreported oracle.*

`src/drive.rs:309-331` is the `alloc_zeroed` null check (three panic messages,
two of them written by round 4's own P3-1 fix). **No test in the crate ever
makes `alloc_zeroed` return null**, so this entire block is untested:

- `Fault::NullAlloc` (`tests/oracle_negative.rs:122-123`) is matched **only** in
  `Faulty::alloc` (`:226`). `Faulty::alloc_zeroed` (`:251-274`) has no null arm
  at all — its only fault branches are `OverlapZeroedAt` and `NotZeroed`, both of
  which return an in-arena pointer.
- `LeakyAllocator::alloc_zeroed` (`tests/double_free_no_op.rs:40-43`) forwards to
  `System` with a 64-byte request; the proptest/arbitrary suites also drive
  `System`. Nothing returns null.

Delete `drive.rs:309-331` and **every test in the crate still passes**, while
`drive` becomes unsound: `null.addr() % align == 0` passes the align assert
(`:332-337`), `assert_no_overlap` against `[0, size)` passes (no live block sits
near address 0), and `verify_zeroed_block(null, size, …)` (`:340`) then does
`ptr.add(b).read()` — a null dereference. This is the identical shape round 3
rated **P3** (its P3-2 item 3, the null-`realloc` skip) and fixed with
`Fault::NullRealloc`; the `alloc_zeroed` twin was never enumerated in any of the
four prior rounds.

Two smaller instances of the same per-arm asymmetry ride along:

- **`drive.rs:332-337` (the `AllocZeroed` M1/M4 align assert) has no
  counterfactual either.** `Fault::MisalignedBy` is matched only in
  `Faulty::alloc` (`:227`), and `OverlapZeroedAt(16)` with `align: 8` happens to
  be aligned. Deleting this assert loses an advertised M4 oracle silently.
- **`drive.rs:410-418` (the `Realloc` M1/M4 align assert) likewise.** Both
  `ReallocAt` offsets in use (80 and 8) are 8-aligned and the other realloc
  faults go through `bump_aligned`, so no test ever returns a misaligned
  `realloc` pointer.

Net: of the six null/align oracle asserts across the three block-creating arms,
three have no counterfactual, and one of the three is soundness-critical — in the
crate whose stated product is *detection* and whose negative suite exists
precisely to prove each oracle fires.

Note this is the same meta-pattern the brief asks about, in a milder form:
round 4 wrote **four** new panic-message strings (two arms × two clamp
directions) and pinned **two**, both in the `Alloc` arm; the `alloc_zeroed`
twins at `:313-316` and `:321-326` are unreachable by any current test.

**Suggested fix (small, mechanical):** add `Fault::NullAllocZeroed` and
`Fault::MisalignedZeroedBy(usize)` arms to `Faulty::alloc_zeroed`, plus a
`Fault::ReallocMisalignedBy(usize)` (or reuse `ReallocAt` with an odd offset —
`ReallocAt(9)` on an `align: 8` block is already enough for the realloc case),
and four tests mirroring `null_alloc_panics` / `misaligned_alloc_panics`:
`null_alloc_zeroed_panics`, `misaligned_alloc_zeroed_panics`,
`misaligned_realloc_panics`, and — since the fault is then available — one pin
for the `alloc_zeroed` clamp-UP message so round 4's copy is behaviourally
pinned in both arms rather than only one.

### P3-2 — round 4's drift-check enforces the UNION of the job's `--test` names, not membership in BOTH miri passes (and it matches commented-out steps)

*Axis: улучшение (a structural gate that does not cover the class its own error
message states) + пахнущий код (text matching where the job means "executed
steps").*

`.github/workflows/ci.yml:925-943` collects every `--test <name>` from every line
of the job matching `run: cargo miri test -p globalalloc-model`, `sort -u`s them,
and diffs that **union** against `tests/*.rs`. The job runs **four** steps, in two
pairs (plain, then `-Zmiri-strict-provenance`), and the check's own failure
message (`:941`) instructs: *"Add every new test file to **BOTH** target-filtered
miri steps above"* — which is exactly the property it does not verify.

Re-derived by running the pipeline over a modified copy of the job text (no files
written, no build):

| scenario | check result | reality |
|---|---|---|
| current tree | pass | correct |
| 7th test file, neither step updated (the 3-round recurrence) | **fail** ✔ | correct |
| 7th test file added to the **plain** pass only | **pass** ✘ | never runs under strict provenance |
| both multi-target steps commented out (`# - run: …`) | **pass** ✘ | four targets stop running entirely |

Both misses are the same defect class the check exists to prevent — a test file
silently not executing under one of the two miri configurations while the gate
stays green — and the "added to one of two nearly identical adjacent lines" slip
is precisely the shape that has recurred here three rounds running. The
commented-out case is a `grep` artifact: the pipeline reads YAML as text, so a
disabled step still donates its target names.

**Suggested fix:** compare per-step instead of per-union, and only over steps
that are actually run lines. Minimal shape, same tooling:

```bash
mapfile -t steps < <(printf '%s\n' "$job" | grep -E '^\s*- run: cargo miri test -p globalalloc-model')
# expect exactly 4 run lines; for each PAIR (leak-scoped + rest) the union of that
# pair's --test names must equal `actual`, and each pass must appear twice.
```
i.e. assert (1) the run lines are anchored at `^\s*- run:` so a commented step is
excluded and its disappearance is itself a failure, (2) the job has exactly the
expected number of run lines, and (3) the union **per pass** (plain / strict)
equals `tests/*.rs`, not the union across all four. Alternatively remove the
whole class: drop the `--test` filters and scope `-Zmiri-ignore-leaks` by
`#[cfg_attr(miri, ignore)]`-style selection or by moving `LeakyAllocator`'s test
into a target the leak checker tolerates — then there is no hand-listed set to
police at all.

---

## P4

### New this round

1. **`src/config.rs:34-37`, `:47-53`** *(smell — a residual overstatement inside
   the sentence round 4 wrote to replace an overstatement)*. "a bound at or near
   this ceiling still turns **every reached op** into a **guaranteed** M1 null
   report" is not true as quantified: the small arm still draws servable sizes
   (`1..=small_max` in proptest), and in the `arbitrary` front-end the small arm
   is capped at `u32::MAX / weight_sum` (≈ 429 MiB by default) regardless of how
   large `small_max` is, so a null is not guaranteed for it at all. The
   `# Panics` block's own phrasing at `:113-117` — "even a config this bound
   accepts **can** generate sizes `drive` clamps" — is correct; reuse it in the
   field docs.
2. **`src/arbitrary_stream.rs:33-52`, `:66-90` vs `src/config.rs:26-58`**
   *(improvement — an undocumented front-end divergence for a **valid** config)*.
   `RawOp`'s `size` / `new_size` are `u32`, so `magnitude = raw / weight_sum`
   caps every generated size at ≈ `u32::MAX / weight_sum` (≈ 429 MiB with the
   default 9:1). A validated `small_max`/`large_max` above that is silently
   unreachable through the `arbitrary` front-end while the proptest front-end
   honours it — the same "a valid value is reinterpreted differently by each
   generator" class as the still-open `max_align`/`2^21` item (carried #14
   below). Harmless for the default and for the in-tree fuzz config (2 MiB), but
   it belongs in `Config::small_max`/`large_max`'s rustdoc next to the
   `max_align` note.
3. **`tests/oracle_negative.rs:287-293` + `src/drive.rs:399-402`**
   *(improvement — the one clamp direction still without a test)*. Round 4
   covered alloc-up, alloc-down and realloc-up; the **realloc clamp-DOWN**
   direction has no test, so `Faulty::realloc`'s upper-bound assert can never
   fire in the suite as it stands. One test — `[Alloc{64,8},
   Realloc{i:0,new_size:usize::MAX}]` against `Fault::Honest`, expecting the
   arena's own `"arena exhausted"` — mirrors
   `oversized_size_is_clamped_not_rejected` exactly.
4. **`src/raw_allocator.rs:49-55` vs `tests/oracle_negative.rs:220-224`,
   `:282-293`** *(improvement — see §0(c))*. The trait scopes the `GlobalAlloc`
   triple to "a caller going through the blanket `GlobalAlloc` impl", but the
   crate's own **direct** implementor asserts that triple, and the guarantee it
   relies on is stated only in `drive`'s doc. Add one clause to the trait's
   caller-obligations list ("`drive` calls **every** implementor, direct or
   blanket, within that range") so a direct implementor knows it never sees
   `size == 0` / `new_size == 0`. Cosmetic sibling: the fake's assert messages
   say *"GlobalAlloc precondition violated"* in a type that is not a
   `GlobalAlloc`.
5. **`tests/oracle_negative.rs:10-11`** *(smell)*. "**Two** checks genuinely have
   no in-op counterfactual and are pinned at **its** next observable read" — a
   singular left over from the round-4 pluralisation edit. One word.
6. **`crates/globalalloc-model/src/lib.rs:86-92`** *(improvement)*. The
   crate-level `#![allow(unsafe_code)]` is **inert**: `unsafe_code` is
   allow-by-default, this crate has no `[lints]` table, and
   `[workspace.lints.rust]` (root `Cargo.toml:103-113`) carries only
   `unexpected_cfgs` — so nothing stops `unsafe` from appearing in `config.rs`,
   `op.rs`, `strategy.rs` or `arbitrary_stream.rs`, which today hold none. The
   in-repo pattern that actually enforces the seam is `#![deny(unsafe_code)]` at
   the crate root plus narrower lifts (`src/lib.rs:280`,
   `crates/tagged-index-stack/src/lib.rs:307` + its per-item
   `#[allow(unsafe_code)]`s). Two module-level lifts in `drive.rs` and
   `raw_allocator.rs` would make the crate's own headline claim ("the single
   reason this crate holds `unsafe`") machine-checked. Noted honestly:
   `crates/aligned-vmem/src/lib.rs:129` is an in-repo precedent for the current
   shape, so this is a strengthening, not a violation.
7. **`.github/workflows/ci.yml:935`** *(smell — fragility, fails loud)*.
   `grep -o -- '--test [a-zA-Z0-9_]*'` requires exactly one space and an
   underscore-only name, so a legal `tests/my-test.rs` target or a `--test=name`
   spelling reports drift that does not exist. Not a coverage hole (it fails the
   build rather than passing silently), but it makes the gate reject valid
   states. `[A-Za-z0-9_-]` and `[= ]` cost nothing.
8. **`tests/config_validate.rs:16-33` vs `src/config.rs:103-106`**
   *(improvement)*. `validate()`'s documented `max_align` rejection has **three**
   clauses (zero / not a power of two / greater than `isize::MAX`); the suite
   pins two. `Config { max_align: 1 << 63 }` is a power of two above the ceiling
   and hits only the third — one more `#[should_panic]`.

### Carried from round 4, re-verified as still present at `9c81fb6`

Round 4's fix commit silently closed seven of its own P4s (its #2, #3, #5, #6,
#7, #8, #9 — the `ClobberOnLaterAlloc` doc, the `SAFETY` note's citation, the
`usize::MAX` example, the dead `.max(1)` on the weight sum, the counterfactual
wording, `dealloc`'s sub-heading link, and the duplicated `GlobalAlloc`
preconditions), and `9c81fb6` closed its #11 (`fuzz/Cargo.lock`). These remain,
with the same reasoning as run 4 §P4 / run 3 §P4:

9. `.github/workflows/ci.yml:2356` — "its **four** `tests/` files"; there are six.
   (run 4 P4-1)
10. `tests/oracle_negative.rs:158-176`, `:463-516` — `ClobberOnLaterAlloc` and
    `WritesDoNotStick` are one mechanism differing only in the byte written.
    (run 4 P4-4)
11. `CHANGELOG.md:66-71` — no mention of `Config::validate()`, a public method
    with a documented `# Panics` contract that both front-ends now call.
    (run 4 P4-12)
12. `tests/system_proptest.rs:20-21` — shrinks `CASES`/`MAX_LEN` under miri but
    keeps the 128 KiB default `large_max`, while its `system_arbitrary.rs:29-38`
    sibling shrinks the sizes. (run 4 P4-13)
13. `CHANGELOG.md:23-25` — the totality claim omits the never-admissible-align
    caveat `drive.rs:190-192` states. (run 3 P4-1)
14. `src/config.rs:68-76` — `max_align`'s rustdoc still does not mention the
    `arbitrary` front-end's absolute `2^21` cap. (run 3 P4-2)
15. `src/arbitrary_stream.rs:20-27` — `ALIGN_POW_CAP_EXP` bakes a sefer-specific
    4 MiB-segment rationale into a non-configurable constant. (run 3 P4-3)
16. `src/strategy.rs:26` — `debug_assert!(max_align.is_power_of_two())`
    unreachable after `op_strategy:61`'s `validate()`. (run 3 P4-4)
17. `src/arbitrary_stream.rs:56` — `config.max_align.max(1)` dead for the same
    reason. (run 3 P4-5)
18. `src/drive.rs:48-52` — `layout_for`'s `unwrap_or_else(panic)` is unreachable
    at all four call sites (both alloc arms clamp first; both
    `Dealloc`/`Realloc` sites rebuild a layout that already constructed once).
    (run 3 P4-6)
19. `Cargo.toml:45` — proptest's `no_std` feature forces `num-traits/libm` into
    every consumer's graph. (run 3 P4-8)
20. `src/drive.rs:451-462` — the run-end sweep passes a **block** index into
    `verify_block`'s `step` parameter, so the message reads `M3: step #0 …` while
    `drive`'s `# Panics` (`:216`) promises the op index. Now pinned by three
    `#[should_panic]` strings. (run 3 P4-9)
21. `src/drive.rs:468-469` — teardown re-implements `layout_for` inline with a
    different message. (run 3 P4-10)
22. `src/drive.rs:33` — `#[must_use]` on `ranges_overlap` is inert. (run 3 P4-11)
23. `src/drive.rs:84-95` — `assert_no_overlap` and `ranges_overlap` compute the
    same two saturating sums twice. (run 3 P4-12)
24. `tests/double_free_no_op.rs:26-29`, `:93`, `:95` — `dealloc_count`
    duplicates `frees.borrow().len()`. (run 3 P4-13)
25. `tests/system_arbitrary.rs:44` — `(0u16..512).map(|i| (i as u8)…)`
    truncates; the "512-byte" buffer is 256 bytes twice. (run 3 P4-14)
26. `src/arbitrary_stream.rs:33-52` — `bucket` and `magnitude` derive from the
    same `u32`, so the large arm clusters near `small_max + 1`. (run 3 P4-15)
27. `src/arbitrary_stream.rs:63-91` — `RawOp` is private yet every variant and
    field carries `///`. (run 3 P4-16)
28. `src/arbitrary_stream.rs:113-119` — `OpStream`'s doc demotes `OpStream::ops`
    and `crate::drive` to plain code spans while the module header links
    `[crate::drive]`. (run 3 P4-17)
29. `CHANGELOG.md:91-93`, `Cargo.toml:6` — internal review identifiers ("review
    P3-23") still ship in two crates.io-rendered artifacts. (run 3 P4-18)

(25 P4 total: 8 new, 17 carried.)

---

## Summary

| Severity | Count |
|---|---|
| P0 | 0 |
| P1 | 0 |
| P2 | 0 |
| P3 | 2 |
| P4 | 25 (8 new, 17 carried) |

**P0–P3 trend: 47 → 14 → 10 → 5 → 2.** The cycle is converging as expected, and
for the third consecutive round nothing is wrong with what the crate *does*: no
finding at any severity describes incorrect runtime behaviour, an unsound public
API, or a wrong published claim about the allocator contract. Both P3s are
missing **verification**, not missing correctness.

Round 4's structural bet paid off partially and honestly: the drift-check does
catch the exact three-round recurrence it was built for (I reproduced the catch),
and it fails loudly rather than silently on malformed input. What it does not do
is enforce the property its own error message states — membership in **both**
miri passes — which leaves the narrower half of the same class open (**P3-2**).

The meta-pattern the brief asks about did recur once more, in its mildest form
yet: round 4 added four new panic-message strings across the two alloc arms and
pinned only the two in the `Alloc` arm, leaving the `alloc_zeroed` arm's entire
null branch — including a branch whose deletion dereferences null — without any
counterfactual, four rounds after the crate's own negative suite was created to
guarantee exactly that (**P3-1**). This is the third distinct time the
`alloc_zeroed` arm has been the one edited without being covered (run 3's P3-1
added its fill read-back; run 4's P3-1 added its clamp split), which suggests the
durable fix is a table-driven negative suite over `{Alloc, AllocZeroed, Realloc}
× {null, misaligned}` rather than one more hand-written pair.

If only one thing lands before the tag, make it **P3-1** — it is ~30 lines of
test plus two `Fault` variants, and it closes the last branch in `drive` whose
removal is undefined behaviour rather than a missed report.
