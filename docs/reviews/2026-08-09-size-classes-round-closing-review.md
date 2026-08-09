# `size-classes` round-closing review (read-only, end-to-end)

**Date:** 2026-08-09
**Reviewed range:** `7ffeba5^..9018c07` — the 5 fix commits `7ffeba5` (#701), `a80ba49` (#728),
`5741243` (#729), `d07102a` (#730), `9d2d2fa` (#731), plus the two post-work commits `d1a4031`
(CHANGELOG, task #753) and `9018c07` (checkpoint, task #754).
**Scope check:** `git diff --stat 7ffeba5~1..9018c07` touches
`crates/size-classes/{src/lib.rs,README.md,tests/builder.rs,tests/proptest_builder.rs}`,
`src/alloc_core/size_classes.rs` (one line, #728's `Params::new` conversion), `CHANGELOG.md`, and
one `docs/checkpoints/` file. **No out-of-scope edits.**
**Baseline audit:** `docs/reviews/2026-08-07-size-classes-rust-intel-audit.md`
(0 critical, 0 high, 4 medium, 8 info).
**Mode:** read-only. No tracked file in this repository was modified. `git status --short` was
identical before and after every probe (one pre-existing untracked checkpoint, nothing else). All
counterfactuals ran in **workspace-detached scratch copies** of `crates/size-classes` under
`%TEMP%`, plus a standalone `%TEMP%` cargo project depending on the crate by path; both were
deleted afterwards. Verbatim tool output is inlined below.

---

**Bottom line.** The five fix commits are, on their own terms, *correct*. I re-derived #730's
golden values by hand (they are right), re-ran #701's counterfactual in both profiles (the
commit's subtle debug-vs-release argument is mechanically sound, and the regression test is a
real, non-vacuous counterfactual), reproduced E0639 against `#[non_exhaustive]` from outside the
crate, confirmed `Params::new`'s argument order matches the field declaration order exactly, and
confirmed both of #731's new release-active `assert!`s are hard const-eval compile errors that no
existing call site trips. Every one of the audit's 12 findings is genuinely addressed.

The problems are all at the *boundary* — what the round did not look at.

> **F1 (HIGH): #729's `debug_assert!` breaks an existing, CI-covered root test, and `main` is red
> right now.** `tests/medium_classes_correctness.rs:185` passes `align = 320 * 1024` (`327680`, a
> non-power-of-two) to `SegmentLayout::class_for`. The new guard fires. Three whole-suite CI rows
> run this file. The commit's own justifying sentence — "Every real caller in this repo derives
> `align` from `core::alloc::Layout`, which already guarantees power-of-two by construction" — is
> **false**, and one `grep` over `tests/` would have shown it. #729 verified the root crate with
> `cargo check` and `cargo clippy` but never with `cargo test`, which is precisely the gap.

> **F2 (HIGH): `size-classes` has no `cargo test` step anywhere in CI.** Its only appearance in
> `.github/workflows/ci.yml` is a `cargo build ... --target thumbv7em-none-eabi` (line 783), which
> compiles no test target. All 14 tests — including the three regression tests this round *just
> added* — have never executed in CI, and never will as things stand. This is the **identical gap
> class** this same six-crate sweep already found and closed for `tagged-index-stack` (task #772,
> F4/F5) and `racy-ptr-cell` (task #773, F1). Nobody checked whether the sixth crate had it too.

And **F3 (HIGH)** is why F2 cannot be closed by copy-pasting the sibling fix: `cargo test -p
size-classes --release` **fails today**.

Given task #660 (first crates.io publish) is gated on this review, **F5** also matters
disproportionately: the README's only code example — the crates.io / docs.rs front page — still
uses the struct-literal syntax that #728 made a hard compile error.

---

## 0. Current-state green check (re-run personally, not trusted from commit messages)

| Command | Result |
| --- | --- |
| `cargo test -p size-classes --all-features` | **14 passed**, 0 failed (`builder` 10 · `proptest_builder` 4 · lib 0 · doc 0) — matches #731's claimed 14/14 |
| `cargo test -p size-classes --release` | **FAILS — 9 passed, 1 failed** (`class_for_non_pow2_align_violates_debug_assert`: "test did not panic as expected") — **F3** |
| `cargo clippy -p size-classes --all-features --all-targets -- -D warnings` | clean |
| `cargo fmt -p size-classes -- --check` | clean (exit 0) |
| `grep -rn "TODO\|FIXME\|XXX\|unimplemented\|todo!" crates/size-classes/` | **0 hits** |
| `grep -rn "unsafe" crates/size-classes/{src,tests}/` | 3 hits, **all doc prose** (`#![forbid(unsafe_code)]` at `src/lib.rs:43`; "memory unsafety" at `:537`). No `unsafe` token, no raw pointer, no `pub fn` taking `*mut`/`*const` anywhere in the crate. The benchmark-hook rule in CLAUDE.md has nothing to bite on here. |
| **0 doctests** in every configuration | CLAUDE.md's no-doctest rule holds (the README example is a ` ```text ` fence — see F5 for the cost of that) |
| `cargo test --features "hardened medium-classes internals" --test medium_classes_correctness` | **FAILS — 12 passed, 1 failed** — **F1** |
| `cargo test --all-features --test medium_classes_correctness item1_mib` | **FAILS** — **F1** |

The `--all-features` count of 14 is right. The round's per-task "14/14 clean" claims are all
truthful *for the debug profile of this one crate in isolation* — which is exactly the blind spot
F1/F2/F3 live in.

---

## 1. What I verified independently and found CORRECT

Recorded so the next round does not re-litigate settled ground.

### 1.1 #701's overflow fix and its counterfactual — CONFIRMED, both halves

`crates/size-classes/src/lib.rs:274-281` is the fix:

```rust
let mut next = cur
    .checked_mul(num)
    .expect("geometric progression overflows usize -- reduce geo_count/growth")
    .div_ceil(den);
next = next
    .checked_add(mask)
    .expect("geometric progression overflows usize -- reduce geo_count/growth")
    & !mask; // round up to a multiple of min_block
```

I reverted both to `wrapping_mul`/`wrapping_add` in a detached scratch copy and re-ran
`tests/builder.rs::geometric_advance_overflow_panics_instead_of_silently_wrapping` in **both**
profiles.

**Debug** (verbatim):

```
thread '...' panicked at src\lib.rs:283:28:
attempt to add with overflow
note: panic did not contain expected string
      panic message: "attempt to add with overflow"
 expected substring: "geometric progression overflows usize"
```

**Release** (verbatim):

```
note: test did not panic as expected at tests\builder.rs:285:4
```

Both of the commit message's claims hold, and the second one is non-obvious enough to be worth
confirming rather than accepting:

- **(a) The debug-vs-release argument is mechanically sound.** In debug the pre-fix code *does*
  panic, but at `src/lib.rs:283` — the untouched bare `+` in the min-step fallback — with the
  message `attempt to add with overflow`, which is a *different bug surfacing*, not the silent
  wraparound §B26 describes. Only `--release` exhibits the true failure mode. I additionally
  printed the table the pre-fix release build produces: `[9223372036854775808, 0]` — a class of
  size **zero**, returned with no diagnostic whatsoever.
- **(b) The regression test is a real counterfactual, not vacuous.** It fails under the revert in
  *both* profiles (in debug for the wrong-message reason, in release for the right one) and passes
  only against the checked arithmetic.

The `# Panics` rustdoc at `:168-174` is accurate and complete for the new asserts.

### 1.2 #730's golden values — RE-DERIVED BY HAND, correct

`tests/builder.rs:259` publishes `GOLDEN = [16, 32, 48, 64, 80, 112, 144, 192]` for
`min_block = 16`, `growth = (5, 4)`. Recomputed from scratch, `round_up(ceil(prev * 5 / 4), 16)`,
without reading the crate's or the test's arithmetic comment first:

| n | `prev` | `ceil(prev*5/4)` | `+15` | `& !15` | `> prev`? | class |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | — | — | — | — | — | **16** |
| 1 | 16 | 20 | 35 | 32 | yes | **32** |
| 2 | 32 | 40 | 55 | 48 | yes | **48** |
| 3 | 48 | 60 | 75 | 64 | yes | **64** |
| 4 | 64 | 80 | 95 | 80 | yes | **80** |
| 5 | 80 | 100 | 115 | 112 | yes | **112** |
| 6 | 112 | 140 | 155 | 144 | yes | **144** |
| 7 | 144 | 180 | 195 | 192 | yes | **192** |

Matches exactly. The min-step fallback never fires for this scheme, so the golden run is a genuine
test of the *geometric* formula, which is what the §D1a circular-oracle finding asked for. The
arithmetic transcribed into the test's own comment (`:250-258`) is also correct line for line.

I separately re-derived the pre-fix table asserted in `extras_overlapping_geometric_run_panics`'s
comment (`:189-190`): merging geo `[16,32,48,64,80,112,144,192]` with `extras = [16, 32]` under the
`cur < extras[ei]` tie-break gives `[16, 16, 32, 32, 48, 64, 80, 112, 144, 192]` with indices 1
and 3 unreachable. The comment is right.

### 1.3 #728's `#[non_exhaustive]` — REAL ENFORCEMENT, and the argument order is right

From a standalone external crate depending on `size-classes` by path:

```
error[E0639]: cannot create non-exhaustive struct using struct expression
```

Not vacuous. Two further checks the task asked for:

- **Zero remaining struct-literal constructions.** `grep -rn "Params {" crates/size-classes src/
  --include=*.rs` returns only `Self { ... }` inside `Params::new` itself (`src/lib.rs:121-127`)
  and doc prose. All 11 construction sites in the crate's tests plus the one in
  `src/alloc_core/size_classes.rs:149` use `Params::new`.
- **Argument order matches field declaration order.** Fields (`src/lib.rs:71-103`): `min_block`,
  `growth`, `geo_count`, `extras`, `huge_threshold`. Constructor (`:114-120`): identical order.
  No silent `usize`/`usize` swap. (This was worth checking precisely because `min_block`,
  `geo_count` and `huge_threshold` are three same-typed `usize`s the compiler could not
  distinguish.)

### 1.4 #729's `debug_assert` fires in debug and not in release — CONFIRMED

`src/lib.rs:547-550`. Fires in debug (`class_for: align must be a power of two (the Layout
contract)`, observed in §0's failing root test). Does not fire in release (proved by F3's
"test did not panic as expected"). The `debug_assert`-vs-`assert` choice follows the audit's own
§B26 guidance verbatim and is correctly justified in the doc comment. **The guard itself is right;
F1 is about an unverified claim made next to it, not about the guard's severity classification.**

### 1.5 #731's two release-active asserts — no call site trips them

`grep -rn "build_table\|size2class_len" --include=*.rs .` — every call site across the workspace
(11 in the crate's tests, plus `src/alloc_core/size_classes.rs:155-156` and `:167`) uses
`min_block = 16` / `min_block = 8` / `min_block = 64` (all powers of two) and a growth denominator
of 4, 2, or 8 (all `> 0`). Nothing trips either assert; the whole workspace still compiles.

Confirmed the const-eval behavior the task asked about — a panicking `assert!` inside a `const` is
a **hard compile error**, not silently ignored:

```
error[E0080]: evaluation panicked: size2class_len: min_block must be a power of two
```

Note one real semver consequence, correctly out of scope for a pre-publish crate but worth
recording: `size2class_len(max, min_block)` previously *returned a value* for a non-pow2
`min_block` and now panics. Because this lands **before** first publish, it costs nothing. It
would have been a breaking change afterwards.

### 1.6 CHANGELOG `d1a4031`'s numa-shim section — accurate

The task flagged this as a likely drift site (a retroactive summary of `f97bf1d`/`fd2a3bb` written
after the fact). I cross-checked every load-bearing specific against the two commits' diffs and
messages. All check out:

- `Topology { len: [usize; 64], buf: [[u8; 1024]; 64] }` — matches the diff exactly.
- `NODE_CPUMAP_BUF_LEN = 1024` "down from #720's original 4096" — matches.
- "64 KiB instead of 256 KiB" — `64 × 1024 = 65536` ✓ (the `[usize; 64]` adds 512 B, so "64 KiB"
  is a rounded but honest figure).
- "~3640 CPUs per single node" — a 1024-byte cpumap of 9-char `xxxxxxxx,` groups is
  `⌊1024/9⌋ × 32 = 3616..3641` CPUs ✓.
- F9's "added clippy steps to the existing `numa-shim-mock`/`numa-shim-windows` CI jobs" —
  `git show fd2a3bb -- .github/workflows/ci.yml` shows exactly those two steps.
- F10's "reconciled ... to `#697`+`#724`", F4/F5's "items 44-47" + "item 42 → CLOSED", F8's
  "already corrected in #777's own commit", F13's `MockCall::CurrentNode` decision — all match.

One drift found, filed as **F7** below (a publish-gating detail dropped in the summary).

---

## 2. Findings

### F1 — `class_for`'s new `debug_assert!` breaks an existing CI-covered root test; `main` is red — **HIGH**

**Where:** guard at `crates/size-classes/src/lib.rs:547-550` (commit `5741243`, task #729);
false justifying claim at `crates/size-classes/src/lib.rs:540-544`; the breaking caller at
`tests/medium_classes_correctness.rs:185`, fed by `MEDIUM_SIZES` at `tests/medium_classes_correctness.rs:24-31`.

**What.** `MEDIUM_SIZES` is `[256 KiB, 320 KiB, 384 KiB, 512 KiB, 768 KiB, 1 MiB]`. Three of those
six are **not powers of two**: `320 * 1024 = 327680` (`5 × 2^16`), `384 * 1024 = 393216`
(`3 × 2^17`), `768 * 1024 = 786432` (`3 × 2^18`). The test
`item1_mib_alignment_resolves_to_small_not_large` loops over that list *as the `align` argument*:

```rust
for &align in MEDIUM_SIZES {
    let got = SegmentLayout::class_for(1, align);
```

`SegmentLayout::class_for` forwards to `SizeClasses::class_for` via
`src/alloc_core/size_classes.rs:203-205`. The new `debug_assert!` fires on the second iteration.

**Reproduced** (verbatim, this host, unmodified `9018c07`):

```
$ cargo test --features "hardened medium-classes internals" --test medium_classes_correctness
running 13 tests
test item1_mib_alignment_resolves_to_small_not_large ... FAILED

thread 'item1_mib_alignment_resolves_to_small_not_large' panicked at crates\size-classes\src\lib.rs:547:9:
class_for: align must be a power of two (the Layout contract)

test result: FAILED. 12 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out.
```

**Blast radius.** The file's gate is `#![cfg(all(feature = "alloc-core", feature = "medium-classes",
feature = "internals"))]`. Three whole-suite CI rows satisfy it:

- `.github/workflows/ci.yml:387` — `cargo test --features "hardened medium-classes internals"`
- `.github/workflows/ci.yml:426` — `cargo test --all-features` (re-verified failing above)
- `.github/workflows/ci.yml:599` — `cargo test --features "production medium-classes exact-span-large internals"`

(`ci.yml:592` also enables `medium-classes` but is `--test r14_4_promotion_move_leg_reduction`
target-selected, so it is unaffected.)

**Why the round missed it — this is the interesting part.** #729's commit message states, and
`src/lib.rs:540-544` publishes as rustdoc:

> "Every real caller in this repo derives `align` from `core::alloc::Layout`, which already
> guarantees power-of-two by construction, so practical exposure is low."

That sentence is **false**, and it is the sentence the whole `debug_assert`-vs-`assert` risk
assessment rests on. It was inherited uncritically from the audit's own §F2 ("Practical exposure is
low because any `Layout`-derived align is pow2") and never checked against the tree. `grep -rn
"class_for(" tests/` takes two seconds and lists the offender. Separately, #729's verification
block cites `cargo check` and `cargo clippy --features "production internals"` on the root crate —
**neither compiles or runs `tests/`**, and `production` does not enable `medium-classes`, so even a
root `cargo test` under that exact feature set would have stayed green. Two independent misses had
to line up.

I swept the rest of the workspace for the same shape: every other `class_for` call site in `src/`,
`tests/`, `benches/`, `examples/` and `fuzz/` passes either a `Layout::align()` (pow2 by
construction), or the literal `1`, or `SegmentLayout::SMALL_ALIGN_MAX` (= 16).
`tests/medium_classes_wide_correctness.rs` uses `SMALL_ALIGN_MAX` throughout and passes (12/12,
re-run). `tests/size_classes_proptest.rs` and `tests/size_classes_slow_path_equivalence.rs`
generate pow2 aligns only. **`medium_classes_correctness.rs:185` is the only site.**

**Worth noting about the semantics:** pre-#729 this call returned the *correct* answer — for
`align = 327680` the slow path lands on the exact 320 KiB medium class, and the test's own
`block % align == 0` assertion passed. So the guard is not catching a latent wrong answer here; it
is rejecting a call that worked. That does not make the guard wrong (the general non-pow2 hazard
§B26 describes is real), but it does mean the fix is a straight choice between two options, not a
bug hunt.

**Recommended fix — pick one, in the same commit:**

1. **Change the test** (preferred). The test's stated intent is "align equal to a real table entry
   is trivially divisible by itself" — it is using the medium *sizes* as aligns opportunistically.
   Restrict the loop to the pow2 members of `MEDIUM_SIZES` (`256 KiB`, `512 KiB`, `1 MiB`), or add
   a `.filter(|a| a.is_power_of_two())` with a comment naming the `class_for` precondition. Keep
   the non-pow2 sizes covered by the *size*-axis loop at `:196`, which is already correct.
2. **Or widen the contract.** If serving non-pow2 aligns is intended behavior for the medium tier,
   `class_for`'s precondition and the `debug_assert!` are wrong and should be replaced by an
   explicit divisibility check on the fast path.

Either way, **delete or correct the "every real caller in this repo derives `align` from
`Layout`" sentence at `src/lib.rs:540-544`** — it is published rustdoc on an about-to-be-published
crate and it is not true.

---

### F2 — `size-classes` has no `cargo test` step anywhere in CI — **HIGH**

**Where:** `.github/workflows/ci.yml`, the `test-workspace` job (lines 683-839).

**What.** Exhaustive grep of `.github/workflows/` for this crate returns:

```
ci.yml:773       # comment
ci.yml:783       - run: cargo build -p size-classes --no-default-features --target thumbv7em-none-eabi
release.yml:42,47,72,87   # publish plumbing
```

`cargo build` compiles **no test target**. `package.json` / `scripts/*.mjs` contain zero references
to the crate, so `npm run check` does not cover it either. And `cargo test` at the repo root tests
only the root package — the `test-workspace` job's own header comment (`ci.yml:684-685`) says so
explicitly.

**Consequence.** All 14 of this crate's tests have never run in CI. That includes every regression
test this round added — `geometric_advance_overflow_panics_instead_of_silently_wrapping` (#701),
`class_for_non_pow2_align_violates_debug_assert` (#729), and
`geometric_run_matches_hand_derived_golden_values` (#730). If any of the three fixes were silently
reverted tomorrow, nothing in CI would notice.

**This is a repeat.** The `test-workspace` job carries long inline comments documenting the
*identical* gap being closed for two sibling crates in this same six-crate sweep:

- `ci.yml:709-724` — task #639/P5, `tagged-index-stack`: "12 of the crate's 16 tests ... had never
  run in CI."
- `ci.yml:785-805` — task #773 F1, `racy-ptr-cell`: "`cell_unit.rs`'s 7 tests, including 4 added by
  the racy-ptr-cell rust-intel remediation round ... had never run in CI."

The `size-classes` line (`:783`) sits *between* those two blocks. Nobody asked whether the crate on
that line had the same problem. Given task #660 (first publish) is gated on this review, shipping a
crate to crates.io whose test suite has never executed in CI is not an acceptable posture.

**Recommended fix.** Add to `test-workspace`, mirroring the `racy-ptr-cell` pair immediately below
line 805 (**but see F3 first — the release row will not pass as-is**):

```yaml
- run: cargo test -p size-classes --all-features --no-fail-fast
- run: cargo test -p size-classes --release --no-fail-fast
```

`--all-features` rather than default: the crate declares no features, so the two are equivalent
today, and `--all-features` stays correct if one is ever added.

---

### F3 — `cargo test -p size-classes --release` fails; the new `should_panic` test is not profile-gated — **HIGH**

**Where:** `crates/size-classes/tests/builder.rs:315-330` (commit `5741243`, task #729).

**What.** `class_for_non_pow2_align_violates_debug_assert` is `#[should_panic(expected = "align
must be a power of two")]` against a **`debug_assert!`**. Under `cargo test --release` the `bench`
profile turns `debug-assertions` off, the guard compiles away, and the test fails:

```
$ cargo test -p size-classes --release
test class_for_non_pow2_align_violates_debug_assert - should panic ... FAILED
---- class_for_non_pow2_align_violates_debug_assert stdout ----
note: test did not panic as expected at crates\size-classes\tests\builder.rs:317:4
test result: FAILED. 9 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out.
```

**Why this matters beyond the red bar.** It is entangled with F2. The obvious way to close F2 is
to copy the `racy-ptr-cell` pattern — a debug row *and* a release row — and both sibling crates got
their release rows for a specific, documented reason (`ci.yml:725-735`, `:798-803`): a `debug_assert!
→ assert!` promotion is invisible to a debug-only run, so a silent revert would go unnoticed. So
F2's correct fix lands F3 as an immediate CI failure. They have to be fixed together.

There is also a smaller honesty point. #701's commit message explicitly documents running
`--release` as part of its counterfactual — so this round *did* exercise the release profile, at a
point (`7ffeba5`) before #729 existed. Three commits later `5741243` introduced a test that cannot
pass there, and the round's per-task verification blocks (all `cargo test -p size-classes
--all-features`, debug only) never revisited it.

**Recommended fix.** Gate the test on the profile that can satisfy it:

```rust
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "align must be a power of two")]
fn class_for_non_pow2_align_violates_debug_assert() { ... }
```

with a comment stating that the guard is deliberately debug-only (per `class_for`'s own doc) and
that the release profile therefore has nothing to assert. Do **not** promote the guard to a hard
`assert!` just to make the test profile-independent — that would be a behavior change on a hot
classification path, and it would make F1 a release-mode panic in a shipped allocator rather than a
debug-mode test failure.

---

### F4 — #701 left the min-step fallback's bare `+` unchecked, and it produces a zero-sized class silently in release — **MEDIUM**

**Where:** `crates/size-classes/src/lib.rs:283` — `next = cur + min_block; // enforce the minimum step`.

**What.** #701 promoted the geometric advance's `*` and `+` to `checked_mul`/`checked_add`
(`:274-281`) with this stated rationale (`:254-259`, and again in the commit message):

> "this is a library (`build_table`/`Params` are `pub`, and some call sites reach this at runtime,
> not just in `const` table construction), so per §B26 it cannot assume the consumer built with
> `overflow-checks = true`."

The very next statement, two lines below the last `checked_add`, is a bare `+` on the same
accumulating value — and #701's own commit message **names it**:

> "a separate bare `+` in the untouched `next = cur + min_block` min-step fallback still trips
> debug's overflow-checks after the wrapped multiply lands `next` back at 0"

It was observed, described, and left in place, while its immediate neighbours were fixed for
exactly the reason that also applies to it. My own debug counterfactual in §1.1 panics at
`src/lib.rs:283:28`, confirming reachability at the round's own chosen test parameters.

**Reproduced standalone** (external crate, path dependency, `--release`, current `9018c07`, no
modification to the crate at all):

```rust
let p = Params::new(1usize << 62, (1, 1), 5, &[], 1 << 20);
let t = build_table::<5>(&p);
```
```
table = [4611686018427387904, 9223372036854775808, 13835058055282163712, 0, 4611686018427387904]
strictly increasing = false
```

That is a **zero-sized size class** and a duplicate, returned from a `pub fn` with no panic, no
diagnostic, and nothing in the `# Panics` section (`:168-174`) covering it. It is a *worse* outcome
than the bug #701 fixed: #701's masked table was at least strictly increasing (wrong geometry, valid
shape), whereas this one is not even monotone.

**And it fires on a parameterisation the crate's own docs bless as valid.** `src/lib.rs:186-191`
(added by #731) says:

> "`growth.0 == 0` is NOT rejected: it silently degrades to a linear min_block-step table via the
> existing `next <= cur` min-step fallback rather than panicking, which is an intentional (if
> unusual) valid scheme, not a contract violation"

With `growth.0 == 0` the fallback is the *only* advance path, so every step goes through the
unchecked `+`. Reproduced, same corrupt output:

```rust
let p = Params::new(1usize << 62, (0, 1), 5, &[], 1 << 20);
```
```
num=0 table = [4611686018427387904, 9223372036854775808, 13835058055282163712, 0, 4611686018427387904]
strictly increasing = false
```

**Mitigating.** Reaching this needs a `min_block` in the 2^62 range, which no sane consumer picks,
and `SizeClasses::build` catches it downstream — `build_size2class`'s monotonicity assert
(`:333-343`) panics on the resulting table. Only a consumer calling the `pub` `build_table` *on its
own* sees the corrupt array. That is why this is MEDIUM, not HIGH. But `build_table` is documented
and exported as a standalone entry point (crate doc `:14-17`, README `:8-15`), and the crate is
about to be published.

**Recommended fix.** One line, same message, same style as its neighbours:

```rust
if next <= cur {
    next = cur
        .checked_add(min_block)
        .expect("geometric progression overflows usize -- reduce geo_count/growth");
}
```

Per CLAUDE.md's benchmark-hook precedent (#701's own rule 3: re-evaluate the artifact in the same
task), the right time to close a defect a commit message *names* is that commit.

---

### F5 — README's only code example uses the struct-literal syntax #728 made a compile error — **MEDIUM**

**Where:** `crates/size-classes/README.md:40-46`.

```
const SC: SizeClasses<N, L> = SizeClasses::build(Params {
    min_block: MIN_BLOCK,
    growth: (5, 4),
    geo_count: 40,
    extras: EXTRAS,
    huge_threshold: 4 * 1024 * 1024,
});
```

`Cargo.toml:8` sets `readme = "README.md"`, so this is the **crates.io landing page and the docs.rs
front page**. As of `a80ba49`, every external consumer who copies it gets:

```
error[E0639]: cannot create non-exhaustive struct using struct expression
```

reproduced verbatim in §1.3. #728's message claims "Updated every construction site in the
workspace"; the README was not in the sweep. #731 then *edited this exact file* (10 lines, the
`extras` precondition wording at `:8-15`) three lines above the broken block and did not notice.

**Nothing mechanical can catch this.** The fence is ` ```text `, correctly so — CLAUDE.md bans
runnable rustdoc examples in `src/**/*.rs`, and `cargo test --doc` does not compile README fences
for a crate without `doc = include_str!`. So this is exactly the class of drift that only a reader
finds, on the one page every new user reads first.

Two other things in the same example that I checked and found **correct**, so they do not need
touching: `MAX_CLASS = 258_752` really is `build_table::<45>` 's last entry for
`min_block = 16, (5,4), geo_count = 40, extras = [256,512,1024,2048,4096]` (verified by running it),
and `size2class_len(258_752, 16) = 16173` is right.

**Recommended fix:**

```
const SC: SizeClasses<N, L> = SizeClasses::build(Params::new(
    MIN_BLOCK,
    (5, 4),
    40,
    EXTRAS,
    4 * 1024 * 1024,
));
```

Add a line to the README noting `Params` is `#[non_exhaustive]` and `Params::new` is the
construction path — that is the single most semver-relevant fact about the crate's only config
type, and the README currently does not mention it at all. Consider also promoting this example
into `tests/` as a real compiled test (per CLAUDE.md's "the runnable version of the example belongs
in `tests/`"), which would make future drift mechanically impossible.

---

### F6 — the root shim still carries the unqualified "no panics" claim #731 fixed in the crate — **LOW**

**Where:** `src/alloc_core/size_classes.rs:187-188` and `:208`.

**What.** #731's §F2 fix qualified the crate-side claim
(`crates/size-classes/src/lib.rs:388-395`) to "no panics on the lookup path FOR IN-CONTRACT
INPUTS". The byte-identical sentence in this repository's own shim over that crate was not touched:

```rust
/// All methods are `const` pure arithmetic — no allocations, no panics on the
/// lookup path.
pub(crate) struct SizeClasses;
```

It is now *more* wrong than when #731 was written: `class_for` gained a panic in #729 (the very
one F1 trips), and this shim forwards straight to it (`:203-205`).

Adjacent, pre-existing, same paragraph: `:208` documents `block_size` as "Panics (debug) if out of
range". It panics in **every** profile — `self.table[idx]` is a bounds-checked array index, which
is not `debug_assertions`-gated. Not introduced by this round, but it is the same sentence a fix
for the above would be rewriting.

**Recommended fix.** Mirror #731's qualification into the shim, and correct "(debug)" to
"(all profiles)" on `block_size`. Both are `pub(crate)` docs, so this is zero-risk.

---

### F7 — the CHANGELOG's numa-shim section drops the two open items that actually gate publishing — **LOW**

**Where:** `CHANGELOG.md`, closing paragraph of the "`numa-shim` — round-closing-review
follow-ups" section added by `d1a4031`.

**What.** It reads:

> "`numa-shim`'s own crates.io publish decision (task #657) remains deferred to a maintainer call,
> **gated behind this two-task follow-up now being genuinely complete**"

`fd2a3bb`'s own commit message says something materially different:

> "numa-shim's own crates.io publish decision (task #657) remains gated behind **items 46 (semver
> coupling) and 47 (never-executed-on-Linux status)** in `docs/CORRECTNESS_OPEN_ITEMS.md`, both
> newly filed by this task."

A reader of `CHANGELOG.md` alone concludes the gate is now cleared and only a maintainer's
signature is missing. In fact `fd2a3bb` *created two new blocking items in the same commit* — one
of which (item 47) records that the round's Linux-only code has never been empirically executed.
That is precisely the kind of caveat a publish decision needs in front of it. The rest of the
section is accurate (§1.6); this one sentence is where the retroactive summary drifted.

**Recommended fix.** Append the two item numbers to that sentence. Append-only, per this project's
non-retroactive correction convention.

---

### F8 — construction-site count is off by one in both the #728 commit message and the CHANGELOG — **INFO**

**Where:** `a80ba49`'s message ("9 sites across
`crates/size-classes/tests/{builder,proptest_builder}.rs` plus the one real external consumer") and
`CHANGELOG.md`'s #728 bullet ("Updated all 10 construction sites across the workspace (9 in
`crates/size-classes/tests/{builder,proptest_builder}.rs`, plus the one real external consumer)").

`git show a80ba49 -- crates/size-classes/tests/ | grep "^+.*Params::new"` returns **8** lines
(`SEFER_PARAMS`, two `extras_*` tests, the #701 overflow test, `P_SMALL`, and `A_P`/`B_P`/`C_P`).
Plus `src/alloc_core/size_classes.rs` = **9 total**, not 10. (The current totals — 8 in
`builder.rs`, 3 in `proptest_builder.rs` — include three sites added *later*, by #729 and #730.)

Cosmetic; recorded only because the CHANGELOG is the durable artifact.

---

### F9 — the CHANGELOG's size-classes header mis-describes #701 and #731 as non-runtime guards — **INFO**

**Where:** `CHANGELOG.md`, `size-classes` section header:

> "**Runtime improvements: 0** — every fix below is a `const`-eval-time or debug-only guard
> promotion, an API-surface decision, or documentation"

#701's `checked_mul`/`checked_add` and #731's `assert!(params.growth.1 > 0)` /
`assert!(min_block.is_power_of_two())` are **release-active runtime guards**, not const-eval-time
or debug-only. The same CHANGELOG's own #701 bullet says so three paragraphs later: "the fix's real
effect is **entirely on the previously-unguarded runtime call sites**."

The trailing clause of the header ("no shipping algorithm's OBSERVABLE runtime behavior changed on
any in-contract input") is correct and is the claim that matters, so "Runtime improvements: 0" and
the `fix(perf)` prefixes are right under R30-12. Only the enumerated middle clause is inaccurate.

**Recommended fix.** "...is a `const`-eval-time, release-active-precondition, or debug-only guard
addition, an API-surface decision, or documentation."

---

## 3. Things the task asked about that turned out to be non-findings

Recorded so they are not re-checked next round.

- **`Params::new` argument order** — matches field declaration order exactly (§1.3). No swap.
- **`#[non_exhaustive]` completeness** — `Params` is the crate's only multi-field public data type
  with public fields. `SizeClasses<N, L>` has all-private fields and is constructed only through
  `build`, so `#[non_exhaustive]` would add nothing. The crate defines no enums, no traits, and no
  blanket impls. No further `#[non_exhaustive]` gap.
- **Panic contracts** — `build_table` (`:168-174`), `build_size2class` (`:305-310`),
  `size2class_len` (`:136-138`) and `block_size` (`:481-484`) all carry accurate `# Panics`
  sections covering every `assert!` they contain. The one gap is F4's unchecked `+`, which is a
  missing *guard*, not a missing doc.
- **Safe `pub fn` touching allocator metadata through a raw pointer** — none. The crate has no raw
  pointers at all and `#![forbid(unsafe_code)]` at `src/lib.rs:43` makes one a compile error.
  CLAUDE.md's R25-1 benchmark-hook rule has no surface here.
- **Out-of-scope edits** — none across the 7-commit range.
- **TODO/FIXME/placeholder/half-wired features** — zero hits.
- **MSRV** — `rust-version = "1.88"` covers `usize::div_ceil` in const (1.83) and
  `unsigned::is_multiple_of` (1.87). Both are used; both are within the declared minimum.
- **Unaddressed audit findings** — all 12 findings in
  `docs/reviews/2026-08-07-size-classes-rust-intel-audit.md` map onto one of the five tasks. None
  was silently dropped.

---

## 4. Recommended disposition

**Do not publish (task #660) until F1, F2, F3 and F5 are closed.** F4 and F6-F9 are appropriate to
batch into the same follow-up commit.

A natural two-task split matching this sweep's established pattern:

- **Task A (HIGH):** F1 + F2 + F3 together — they are one interlocked change. Fix the non-pow2
  align in `tests/medium_classes_correctness.rs`, correct the false rustdoc sentence at
  `src/lib.rs:540-544`, `#[cfg(debug_assertions)]`-gate the `should_panic` test, then add both the
  debug and release `cargo test -p size-classes` rows to `test-workspace` and confirm the whole
  thing is green — including the three previously-failing root CI rows. Counterfactual obligation:
  confirm each new CI row would have caught its corresponding defect.
- **Task B (MEDIUM/LOW):** F4 (`checked_add` on the min-step fallback, with a regression test) +
  F5 (README example) + F6 (shim docs) + F7/F8/F9 (append-only CHANGELOG corrections).

---

## 5. Method note

Every counterfactual in this report was executed, not reasoned about. The `wrapping_mul`/
`wrapping_add` revert (§1.1) ran in a copy of `crates/size-classes` under `%TEMP%` with a
`[workspace]` stanza appended so it detached from this workspace; F4's and §1.3's probes ran in a
separate `%TEMP%` cargo project depending on the crate by path, against the **unmodified** working
tree. Both scratch directories were deleted. `git status --short` in
`D:\dev\rust\sefer-alloc` shows the same single untracked checkpoint file before and after this
review; **no tracked file was modified by it**, and no `git` command that mutates the working tree,
index or refs was run.
