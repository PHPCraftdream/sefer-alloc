# globalalloc-model — independent read-only review, run 8

**Verdict: NOT clean — the P0–P3 finding count is 1, not ZERO. 0 P0, 0 P1,
0 P2, 1 P3, 11 P4 (new this round).** All FIVE of round 7's fixes are
**confirmed closed** — each independently re-derived, and two of them
re-*executed* against scratch copies rather than reasoned about. The single
P3 is a two-line documentation drift that the P0 fix itself introduced: the
commit added the crate's first `unsafe` item that is **not** about raw
pointers (`DoubleFreeOk::new`) under a seam whose inventory row — in the
root `README.md` §"Where unsafe lives — the complete list", the table
`CLAUDE.md` designates as the reference any formal audit compares against —
still states the crate's "single documented reason" is the `RawAllocator`
trait's pointer validity.

Reviewed at `67beae5` (`main`), read-only. Scope as briefed: narrow —
confirm round 7's five fixes, not a fresh 6-axis audit. Read in full:
`docs/reviews/2026-09-07-041409-globalalloc-model-review-ox-run-7.md`, the
complete 14-file diff of `67beae5`, and (for context) `src/lib.rs`,
`src/double_free_ok.rs`, `src/config.rs`, `src/drive.rs`,
`src/raw_allocator.rs`, `src/arbitrary_stream.rs`,
`crates/globalalloc-model/Cargo.toml`, `README.md`, `CHANGELOG.md`,
`tests/double_free_no_op.rs`, `tests/system_arbitrary.rs`, the head +
fault-taxonomy + new-tests regions of `tests/oracle_negative.rs`, the whole
`globalalloc-model-miri` job and the `globalalloc-model` rows of
`.github/workflows/ci.yml`, and `release.yml`'s crate-specific test gate.
No `cargo`/build/test/lint command was run (same discipline as runs 1–7;
also avoids `target/` lock contention with concurrent agents). Two claims
below were verified by **executing** pipelines against scratch copies in a
temp directory outside the repo (no repo file touched), and one by
enumerating the generator's arithmetic.

Trend at P0–P3: **47 → 14 → 10 → 5 → 2 → 1 → 5 → 1.**

---

## 0. Round-7 fix verification — what I re-derived independently

### (1) P0-1 — `Config::double_free: Option<DoubleFreeOk>` — **CONFIRMED CLOSED**

The token is genuinely unforgeable in safe code. I looked specifically for a
safe path and found none:

- `pub struct DoubleFreeOk(());` (`src/double_free_ok.rs:14`) — the single
  tuple field has **inherited (private) visibility**, so the tuple-struct
  *constructor function* is visible only inside `mod double_free_ok`. Even
  `src/config.rs`, which imports the type, cannot write `DoubleFreeOk(())`;
  from `tests/` (a separate crate) it is E0423.
- The derive list is exactly `Clone, Copy, Debug, PartialEq, Eq`
  (`:13`) — **no `Default`**, no `From`, no `serde`, no `arbitrary::Arbitrary`,
  no `proptest::Arbitrary`. `Config` itself has no `Arbitrary` impl either
  (`src/arbitrary_stream.rs` derives `Arbitrary` only for the private
  `RawOp`; `OpStream::arbitrary` delegates to `arbitrary_with_config(u,
  Config::default())`, i.e. `double_free: None`), so no fuzz front-end can
  synthesise one.
- `grep -rn "DoubleFreeOk\|double_free"` over `crates/globalalloc-model/`,
  `fuzz/` and the root `tests/`: **no safe function anywhere returns
  `DoubleFreeOk` or `Option<DoubleFreeOk>`.** The only producer is
  `pub const unsafe fn new()` (`:27`).
- The one remaining safe route — copying an existing token out of a
  `Config` (`Config: Copy`, `DoubleFreeOk: Copy`) — requires one to exist
  first, so it does not weaken the property.
- `Config::default()` yields `None` (`src/config.rs:166`); `drive` gates on
  `config.double_free.is_some()` (`src/drive.rs:377`).

Round 7's exact reproducer (`Config { double_free: true, ..Config::default() }`)
no longer type-checks, and `Some(DoubleFreeOk::new())` outside an `unsafe`
block is E0133 — the commit message's three independent compile-time
rejections (E0308 / E0133 / E0423) are the three I would predict from the
shape above.

All **four** in-tree call sites are converted, each with a `// SAFETY:` note
naming the allocator's documented no-op contract:
`crates/globalalloc-model/tests/double_free_no_op.rs:87-90`,
`fuzz/fuzz_targets/global_alloc_ops.rs:102-104`,
`tests/alloc_core_differential.rs:65-67`, and `tests/heap_differential.rs:64`
(`None`). Round 7's own enumeration named three; the fix found the fourth by
grepping the tree, which is the right method.

Both requested doc corrections landed and are accurate as written:
`src/raw_allocator.rs:48-57` moves the double-free clause out of the
unconditional caller-obligation and states the blanket impl "NEVER permits
the relaxation — no value of `Option<DoubleFreeOk>` can be produced **in
safe code**" (the qualifier is present and load-bearing, so the sentence is
true, not an overclaim); `:120-138` extends the blanket impl's `# Safety`
note to `dealloc`'s absolute matching-pointer precondition.

*Residual, P4 only (see P4 §8): the token is not allocator-bound.*

### (2) P2-1 — the initialization obligation — **CONFIRMED CLOSED**

I re-derived the read-before-write inventory from scratch rather than
trusting the "exactly two places" claim. Every read in `drive` is one of:

| site | reads | preceded by a write? |
|---|---|---|
| `verify_block` @ `:299`, `:359`, `:448` | `size`/`new_size` bytes | yes — `fill_block` on the immediately preceding line |
| run-end M3 sweep @ `:466` | `l.size` bytes | yes — every `live` entry was `fill_block`ed across its full recorded extent when created/realloc'd |
| **`verify_zeroed_block` @ `:349`** | `size` bytes | **no** |
| **`verify_prefix_block` @ `:440`** | `min(l.size, new_size)` bytes | **no** |

So "exactly two places" (`src/lib.rs:50-51`) is exact, and the two new
clauses cover precisely those two: `alloc_zeroed`'s full `layout.size()`
(`src/raw_allocator.rs:30-34`, restated per-method at `:92-95`) and
`realloc`'s first `min(old_layout.size(), new_size)` (`:39-42`, restated at
`:114-116`) — the latter matching `keep = l.size.min(new_size)` at
`drive.rs:437` exactly, since `l.size` *is* `old_layout.size()` by
construction at `:395`.

The strengthening does not create a new hole of its own: the blanket
`unsafe impl<A: GlobalAlloc>` must now uphold the added obligation, and it
does — `GlobalAlloc::alloc_zeroed` is documented to return zeroed (hence
initialized) memory, and `GlobalAlloc::realloc` is documented to preserve
the first `min(layout.size(), new_size)` bytes (hence initialized, given
`drive` filled the old block). No in-tree direct implementor is affected
either: `LeakyAllocator` and both `CoreUnderTest`s forward
`alloc_zeroed`/`realloc` to `System`/`AllocCore`, and
`tests/oracle_negative.rs`'s `Arena::new` pre-zeroes its whole capacity.

The trait's opening promise at `:19-22` ("detecting one must not itself be
undefined behavior") is now consistent with the list beneath it: violating
the Zeroing/Prefix **oracles** (wrong *values*) is soundly detectable;
handing back uninitialized bytes is a breach of the **obligation**, not an
oracle failure. That was precisely round 7's complaint.

*Residuals, P4 only (see P4 §4 and §5).*

### (3) P3-1 — `validate_align` coverage — **CONFIRMED CLOSED**

Four tests at `tests/oracle_negative.rs:912-966`. I checked each against
`validate_align`'s actual format string (`src/drive.rs:59-66`) and
re-derived each counterfactual arithmetically rather than trusting the
comments:

| test (`:line`) | op | `validate_align` verdict | message pinned | counterfactual without the guard |
|---|---|---|---|---|
| `zero_align_is_rejected_not_divided_by` (`:926`) | `Alloc{32, 0}` | `Layout::from_size_align(1, 0)` → Err (align 0) | `"op #0: align 0 is not a usable Layout alignment"` ✓ substring of the real panic | `(isize::MAX as usize / 0)` @ `:254` → "attempt to divide by zero" ✓ |
| `non_power_of_two_align_is_rejected_not_layout_matched` (`:937`) | `Alloc{32, 3}` | Err (not a power of two) | `"op #0: align 3 is not a usable Layout alignment"` ✓ | `layout_for` @ `:256` → `"Layout::from_size_align(size=32, align=3) rejected"` ✓ |
| `overflowing_align_is_rejected_not_clamped_into_a_panic` (`:947`) | `Alloc{32, 1<<63}` | `1 > isize::MAX − (2⁶³−1) = 0` → Err | `"op #0: align 9223372036854775808 …"` ✓ (`1<<63` decimal is exactly that) | `(isize::MAX/2⁶³)·2⁶³ = 0`, so `32.clamp(1, 0)` → `Ord::clamp`'s `min <= max` assertion ✓ |
| `zero_align_alloc_zeroed_is_rejected_not_divided_by` (`:961`) | `AllocZeroed{32, 0}` | as row 1 | as row 1 ✓ | `(isize::MAX as usize / 0)` @ `:312` — the **separate** `AllocZeroed`-arm call site ✓ |

Coverage is *complete*, not merely improved: `validate_align` has exactly
two call sites in the crate (`:252` and `:310`), and the `Realloc` arm
correctly needs none (it reuses `l.align` from an already-validated live
block, `:394-395`). Deleting either call site fails at least one of these
four tests with a message mismatch — so the per-arm asymmetry runs 5, 6 and
7 each found once is closed for this guard.

*Residuals, P4 only (see P4 §3 and §6).*

### (4) P3-2 — the miri drift-check's `--all-features` pin — **CONFIRMED CLOSED (reproduced)**

`.github/workflows/ci.yml:1015-1028`. I transcribed the whole check step
into a standalone script and ran it verbatim against the real `ci.yml` and
against three sabotaged copies in a temp directory:

```
### real ci.yml ###                                  CHECK PASSED (found=4, steps=4, missing=0)   exit=0
### one run line stripped of --all-features ###      1 miri run line(s) lack --all-features       exit=1
### all four stripped ###                            4 miri run line(s) lack --all-features       exit=1
```

That is exactly the single-edit false negative round 7 demonstrated, now
caught. Two edge cases I probed and found **not** to be defects:

- **The `|| true` does not create a vacuity hole.** If the `grep -E` prefilter
  ever matched zero lines, `grep -cv` would print `0` and this check would
  pass silently — but the neighbouring `found -ne 4` assert at `:945-952`
  uses the **byte-identical** grep pattern and fails first (exit 1). The
  `|| true` is there only because `grep -c` exits 1 on a zero count under
  `set -o pipefail`, which is correct usage.
- **`grep -cv -- '--all-features'`** parses correctly: `--` ends option
  parsing, and the pattern contains no BRE metacharacters.

*Residual, P4 only (see P4 §7): a contrived trailing YAML comment defeats it
— reproduced, but the same limitation the pre-existing `--test` and
`MIRIFLAGS` extractors already have.*

### (5) P3-3 — `system_arbitrary`'s half-vacuous sweep — **CONFIRMED CLOSED, and the "new bias?" question answered NO**

`tests/system_arbitrary.rs:46-62`. The fix is `bytes[0] |= 1` plus a
per-seed `assert!(!stream.ops.is_empty(), …)`. Two things I checked rather
than assumed:

- **No new collision bias.** `bytes[0] = (31·seed) mod 256`; `gcd(31, 256) = 1`,
  so the map is a bijection mod 256. Forcing the low bit merges `v` and `v+1`
  only if two seeds land on an even/odd adjacent pair — which needs
  `s₂ − s₁ ≡ 31⁻¹ ≡ 223 (mod 256)`, impossible for `|s₂ − s₁| ≤ 31`. I
  enumerated all 32: **32 distinct odd first bytes, zero duplicates.** The
  remaining 511 bytes are untouched, so the streams stay as distinct as
  before.
- **No coverage regression on the four `seen[]` variant asserts.** For the
  16 *odd* seeds, `bytes[0]` was already odd, so `|= 1` is a no-op and their
  decoded streams are byte-for-byte what they were pre-fix. Those 16 alone
  already satisfied all four variant asserts (round 7's own finding), so the
  asserts cannot have been weakened; the 16 even seeds are pure addition.
- **The assert, not the generator, is what holds the property.** `bytes[0] |= 1`
  only forces the *first* continue-bit — later iterations read their bits
  from later bytes — so a future change to `MAX_OPS`, to `RawOp`'s field
  layout, or to the buffer could still shorten a stream. The per-seed
  `!is_empty()` assert is what would catch that, and it is present. Good
  shape.

The module-doc addendum at `:27-29` ("roughly double its pre-fix size") is
arithmetically right: 16 of 32 seeds contributed ~zero work before.

---

## P0 — none

## P1 — none

## P2 — none

---

## P3

### P3-1 — the P0 fix added the crate's first non-pointer `unsafe` item, and neither the root README's "complete list" unsafe inventory nor `src/lib.rs`'s own header — both of which state a *single* reason that does not cover it — was updated in the same commit

*Axis: то, что нужно улучшить (a documented-invariant drift, in the exact
document `CLAUDE.md` designates as the audit reference) + пахнущий код (a
source header that now describes only part of its own file set).*

**Files:** `README.md:599` (repo root — the §"Where unsafe lives — the
complete list" table row for this crate);
`crates/globalalloc-model/src/lib.rs:104-110` (the header comment above
`#![allow(unsafe_code)]`); the new item is
`crates/globalalloc-model/src/double_free_ok.rs:27`.

`CLAUDE.md`'s "Active rules" section makes the README table normative:

> The seams are inventoried in README §"Where unsafe lives — the complete
> list" and mirrored in the `src/lib.rs` header. … Any formal audit compares
> against this command's output, and an `unsafe` token not covered by a
> tier-1 module or a tier-2 item-level allow is a hard compile error …

This crate is a **tier-1** seam — `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]'
crates/globalalloc-model/` returns exactly one line,
`src/lib.rs:110` — and the rule requires each tier-1 seam to carry **a single
documented reason to hold `unsafe`**. Its README row states that reason as:

> `#![allow(unsafe_code)]` — single documented reason: the `unsafe trait
> RawAllocator` (its impls must return valid pointers for the requested
> layout); every impl + call carries `// SAFETY:`

and `src/lib.rs:104-109` states it even more narrowly:

> This crate's one job includes calling the allocator-under-test's raw-pointer
> API and dereferencing the pointers it hands back — inherently `unsafe`. That
> is the single reason this crate holds `unsafe` … Every such site carries a
> `// SAFETY:` note.

`67beae5` added `pub const unsafe fn DoubleFreeOk::new()`. Enumerating every
`unsafe` item in `src/` shows it is the **only one** that is not about the
raw-allocator pointer surface:

```
src/drive.rs:101,111,143,165      unsafe fn fill_block / verify_block /
                                  verify_zeroed_block / verify_prefix_block   -> pointers
src/raw_allocator.rs:82,87,96,    unsafe trait RawAllocator + its 4 methods,
  105,117,139,140,145,150,155     unsafe impl<A: GlobalAlloc> + its 4 methods -> pointers
src/double_free_ok.rs:27          pub const unsafe fn new()                   -> NOT pointers
```

`DoubleFreeOk::new` dereferences nothing, returns no pointer, and has no
`layout`. It is an *unforgeable-consent token*: an entirely different reason
to hold `unsafe`, and — as an `unsafe fn` **declaration** — a genuine
`unsafe_code`-lint trigger in its own right (rustc's `unsafe_code` lint fires
on `unsafe fn` declarations, not only on `unsafe {}` blocks), so it really is
covered by that tier-1 allow rather than being incidental. It also carries a
`# Safety` rustdoc section rather than a `// SAFETY:` note, so the header's
closing sentence ("Every such site carries a `// SAFETY:` note") does not
describe it either.

This is not cosmetic in this repo's own terms: the README table is the repo's
GitHub landing page and the thing `CLAUDE.md` says a formal audit compares
against, and it now understates the crate's unsafe surface by omitting the
crate's *only* publicly reachable `unsafe fn` constructor — which is exactly
the item a downstream auditor most wants pointed at, because it is the one an
external consumer must discharge a contract for. It is minor (nothing
behavioural, nothing published in rustdoc), hence P3 and not P2.

**Suggested fix (two edits, no code):**

1. `README.md:599` — replace "single documented reason" with two named ones,
   e.g. "*two documented reasons:* (a) the `unsafe trait RawAllocator` (its
   impls must return valid pointers for the requested layout), and (b)
   `DoubleFreeOk::new` — the unforgeable consent token for the M2
   double-free oracle, whose `# Safety` contract the allocator author
   discharges; every impl + call carries `// SAFETY:`, every `unsafe fn`
   declaration a `# Safety` doc."
2. `crates/globalalloc-model/src/lib.rs:104-109` — add one sentence to the
   same effect, and soften "Every such site carries a `// SAFETY:` note" to
   cover the `# Safety`-doc form as well.

(Doing this now, before the 0.1.0 tag, keeps the inventory true at the moment
the crate becomes externally auditable — which is the whole point of the
table.)

---

## P4 — new this round

Round 7's 30 P4s were explicitly out of this round's narrow scope and were
**not** re-audited; I did not verify whether any of them are now closed
(spot-checked incidentally: round-7 new-#4's ` ```text ` README fence,
round-5 #25's 512→256 buffer duplication, and round-5 #6's inert crate-level
`#![allow(unsafe_code)]` are all unchanged). The eleven below are new
consequences of `67beae5`.

1. **`crates/globalalloc-model/tests/double_free_no_op.rs:7`** *(bug —
   stale API reference the fix missed in a file it edited)*. The module doc
   was updated at `:2` (`Config::double_free = Some(DoubleFreeOk::new())`)
   and at `:12` (`if config.double_free.is_some()`) but **not** at `:7`,
   which still reads "the same category the crate's own doc names as a valid
   `double_free: true` consumer". `double_free: true` no longer type-checks
   anywhere. One word.
2. **`crates/globalalloc-model/src/double_free_ok.rs:18`** *(smell — a dead
   lint allow)*. `#[allow(clippy::new_without_default)]` is inert: clippy's
   `new_without_default` returns early for an `unsafe fn` new
   (`if sig.header.is_unsafe() { return; }` — "can't be implemented for
   unsafe new"), so the lint could never have fired here. The accompanying
   comment ("No `Default` impl on purpose: a safe default constructor would
   reintroduce the exact hole this type closes") is genuinely valuable and
   should stay; the attribute above it is a claim about a lint that does not
   apply. This is the same shape as round-5 P4 #6's inert
   `#![allow(unsafe_code)]`, one file over.
3. **`crates/globalalloc-model/tests/oracle_negative.rs:952-955`**
   *(improvement — a new 32-bit compile break, unreachable today)*.
   `align: 1 << 63` is `1usize << 63`, which on a 32-bit `usize` is a
   deny-by-default `arithmetic_overflow` **hard error**, so the whole
   `oracle_negative` test binary stops compiling there. The file already
   carried 64-bit-only *message* pins (`18446744073709551615` at `:850` and
   `:900`), so 32-bit `cargo test` was already red — but this upgrades "two
   tests fail" to "the target does not build", and this crate does advertise
   32-bit support (CI builds it for `thumbv7em-none-eabi`,
   `ci.yml:2208`/`:2215`). No CI row compiles this crate's *tests* on a
   32-bit target, so nothing is red today. A `#[cfg(target_pointer_width =
   "64")]` gate, or `1usize << (usize::BITS - 1)` with a computed expected
   string, closes it.
4. **`crates/globalalloc-model/src/raw_allocator.rs:120-138`**
   *(improvement — one of the commit's two new obligations is left
   undischarged in the blanket impl's own note)*. The same commit added the
   initialization obligation to the implementor-guarantee list (`:30-34`,
   `:39-42`) **and** extended the blanket impl's `# Safety` note to cover
   `dealloc`'s matching-pointer precondition — but the note never states why
   `unsafe impl<A: GlobalAlloc>` satisfies the *initialization* clause. It
   trivially does (`GlobalAlloc::alloc_zeroed` is documented to zero;
   `GlobalAlloc::realloc` to preserve the prefix), which is exactly why one
   sentence saying so is cheap and makes the note enumerate all three of its
   obligations instead of two.
5. **`crates/globalalloc-model/src/lib.rs:57-59`** *(improvement —
   an overstated sentence in the new `# Limitations`)*. "An allocator whose
   `alloc_zeroed` hands back genuinely uninitialized memory … violates that
   obligation: **natively, `drive` still reports the oracle failure
   correctly**". For the single most likely real break — `alloc_zeroed`
   forwarding to `alloc` on fresh OS pages — the bytes read as zero natively
   and **no** oracle failure is reported at all. The sentence is round 7's
   own suggested wording, so this is not a regression introduced by
   misreading the review; it is simply a place where "reports the failure"
   should be "reports the failure *if the uninitialized bytes happen to read
   non-zero*".
6. **`crates/globalalloc-model/tests/oracle_negative.rs:919-921`**
   *(improvement — an evidence citation that does not resolve)*. The new
   block's header says the four counterfactuals were "all four verified
   during development — see the drive.rs probe notes in the commit history of
   this fix". `67beae5`'s commit message documents exactly **one**
   (`zero_align_is_rejected_not_divided_by`, via commenting out the Alloc
   arm's `validate_align`); there are no probe notes for the other three
   anywhere in the history. The claim is almost certainly true (I re-derived
   all four arithmetically above), but the pointer is to something that does
   not exist. Either drop the citation or name the three derivations inline,
   which the per-test comments already half do.
7. **`.github/workflows/ci.yml:1021-1023`** *(smell — a text-grep
   false-negative, reproduced)*. A trailing YAML comment on a run line makes
   the check pass while the actual command lacks the flag:
   ```
   - run: cargo miri test -p globalalloc-model --test miri_bounded … # --all-features intentionally dropped
   ```
   → `CHECK PASSED (found=4, steps=4, missing=0)`. Contrived, and the
   pre-existing `--test`-name extraction (`:965`) and MIRIFLAGS extractor
   (`:984-996`) share the property, so this is a note about the check
   family's design rather than a defect this commit introduced. Stripping
   `#.*$` from each line before matching would close all three at once.
8. **`crates/globalalloc-model/src/double_free_ok.rs:20-26`**
   *(improvement — the token proves consent, not consent **for this
   allocator**)*. `DoubleFreeOk` is not parameterised by the allocator, so a
   token correctly minted for `AllocCore` can — in 100% safe code, since it
   is `Copy` and lives in a `Copy` `Config` — be handed to
   `drive(&System, cfg, ops)`. This is **not** a soundness hole: the
   constructor's contract explicitly binds the *future* use ("The allocator
   **subsequently passed to `drive`** must document …"), which is the same
   delayed-obligation shape `Pin::new_unchecked` uses and is accepted
   practice. A `DoubleFreeOk<A>` carrying `PhantomData<fn() -> A>`, with
   `drive` requiring `Option<DoubleFreeOk<A>>`, would move the obligation to
   the compiler — at the cost of making `Config` generic, which would ripple
   through both front-ends and `Config::default()`. Worth one sentence in the
   token's doc naming the tradeoff as deliberate; probably not worth the
   generic.
9. **`crates/globalalloc-model/CHANGELOG.md:74-86`** *(smell — a
   "Changed (breaking)" section inside a first-release changelog)*. The file's
   own preamble says "First release. Everything below is new in this version;
   **nothing has shipped before it**", and then a `### Changed (breaking
   relative to earlier drafts of this unreleased crate)` heading appears
   below it. On crates.io a reader of 0.1.0 sees a breaking-change notice for
   something that never existed publicly. Folding the paragraph's substance
   into the `### Added` `Config` bullet (which already forward-references
   "see Changed below", `:73`) would remove both the contradiction and the
   forward reference.
10. **`crates/globalalloc-model/CHANGELOG.md` `### Added`** *(improvement)*.
    `DoubleFreeOk` is a brand-new **public export** (`src/lib.rs:142`) and
    the crate's only publicly reachable `unsafe fn`; it appears in the
    changelog only parenthetically inside the `Config` bullet and inside the
    `### Changed` paragraph, never as an `### Added` item of its own. Every
    other public item (`drive`, `RawAllocator`, `Op`, `Config`, `op_strategy`,
    `OpStream`) has one.
11. **`crates/globalalloc-model/README.md:15`, `:86`; `src/double_free_ok.rs:20`**
    *(improvement — cosmetics)*. Two README lines now run well past the
    file's ~78-column wrap because `Some(unsafe { DoubleFreeOk::new() })` was
    substituted in place of `true` without rewrapping. Separately,
    `DoubleFreeOk::new`'s doc comment starts directly at `/// # Safety` with
    no summary line, so rustdoc renders an **empty description** for `new` in
    the type's method list on docs.rs — the crate's own landing page.

---

## Summary

| Severity | Count |
|---|---|
| P0 | 0 |
| P1 | 0 |
| P2 | 0 |
| P3 | 1 |
| P4 | 11 (new this round; round 7's 30 not re-audited) |

**P0–P3 count is 1 — NOT zero. The stop condition for this review-fix cycle
is not met, by one two-line documentation edit.**

**P0–P3 trend: 47 → 14 → 10 → 5 → 2 → 1 → 5 → 1.**

**Round 7's five fixes: 5 of 5 confirmed closed.** P0-1 (unforgeable
`DoubleFreeOk`), P2-1 (initialization obligation + `# Limitations`), P3-1
(four `validate_align` `#[should_panic]` pins covering both call sites),
P3-2 (per-run-line `--all-features`, reproduced green-on-real /
red-on-sabotaged), P3-3 (`bytes[0] |= 1` + per-seed non-vacuity assert,
with no new bias — verified by enumeration). Three of the five I verified
*executably* or arithmetically rather than by reading: the CI pipeline
against four scratch copies, the generator's 32 first bytes, and each of the
four align counterfactuals' arithmetic. I found no defect in any of the five
fixes themselves.

The one P3 is the same *class* of finding this crate's review history keeps
producing and is a fair summary of where the remaining risk lives: not in
the code, which is now genuinely careful, but in the **documents that claim
to describe the code completely**. Round 7's P0 was an API-shape defect with
zero behavioural symptom; its fix is correct, and the one thing it left
behind is that the fix itself changed a fact the repo's `unsafe` inventory
asserts — the crate acquired an `unsafe` item that is not about pointers, in
a table whose row says the only reason is pointers. Nothing about that is
subtle once looked at; it is exactly the kind of thing only a reader who
goes *outward* from the diff to the documents that describe the diff's
subject will notice, which is why it survived a diff-level zero-trust
review.

**Recommendation: land the two-line README/`lib.rs` inventory correction
(P3-1) and tag 0.1.0.** Nothing here blocks publication on
correctness/soundness grounds — the P0 that blocked the tag in round 7 is
closed, and I could not find a replacement for it. A run 9 is worth doing
only as a one-shot confirmation that P3-1's two lines landed correctly; a
ninth full pass over this crate would not be a good use of the budget, since
rounds 7 and 8 together have now examined both the runtime behaviour (runs
1–6) and the API/contract shape (runs 7–8) that runs 3–6 were narrowing away
from. Of the P4s, #1 (the stale `double_free: true` doc line), #2 (the dead
clippy allow) and #6 (the unresolvable evidence citation) are each a
one-liner and are the natural companions to the P3 edit in the same commit.
