# globalalloc-model — independent read-only review, run 4

**Verdict: GO-WITH-FIXES.** Not clean at P0–P3. No P0, no P1. **One P2** — the
same defect class as round 3's P2-1 (a CI gate whose documented coverage
diverges from its actual coverage), reintroduced by round 3's *own* new test
file — and **four P3s**, one of which is a coverage gap that has now survived
all three prior rounds untouched: the `size: 0` clamp that round 1 filed as
**P0-1** still has no test anywhere in the crate, and round 3 edited that exact
code path without adding one.

Scope: `crates/globalalloc-model/` at `main` @ `d3d8f5c`, read statically. No
command that builds, tests, lints or formats anything was run; the only tools
used were file reads, `git show`, `git log` and `grep`.

---

## 0. Round-3 fix verification — what I re-derived independently

The prompt named six re-checks. Five hold; one surfaced the P3-1 below. A
seventh question ("anything genuinely new") produced the P2 and P3-2/P3-4.

### (a) `fault_touch_first_block` cannot corrupt a block it shouldn't — HOLDS

`tests/oracle_negative.rs:187-203`. Re-derived rather than trusted:

```rust
match (self.first_block.get(), self.fault) {
    (Some((off, len)), Fault::ClobberOnLaterAlloc) => write 0xCC over [off, off+len),
    (Some((off, len)), Fault::WritesDoNotStick)    => write 0x00 over [off, off+len),
    (None, _) => self.first_block.set(Some((new_off, new_size))),
    _ => {}
}
```

- **"First ever" vs "currently being served" is correctly distinguished.**
  The `(None, _)` arm runs before either corrupting arm can (on the first call
  `first_block` is `None`), so the block being handed out by *this* call is
  never the corruption target. On every later call the target is the *pinned*
  first block: `bump_aligned` advances the cursor monotonically and `drive`
  clamps every size to `>= 1`, so the current block's offset is always
  `>= off + len`. The two ranges are provably disjoint — no fault can scribble
  the block it is simultaneously handing out.
- **No cross-fault contamination.** Both corrupting arms match on a *specific*
  `Fault` variant, so every round-1/round-2 fault (`Honest`, `NotZeroed`,
  `OverlapZeroedAt`, `ReallocAt`, `ReallocNoCopy`, `NullRealloc`) reaching
  `fault_touch_first_block` falls into `(None, _)` once (a harmless `Cell`
  write) and `_ => {}` thereafter. I checked each variant's path:
  `NullAlloc`/`MisalignedBy`/`ShortBlock` never reach the function at all
  (separate `match` arms in `alloc`), and `OverlapZeroedAt` early-returns from
  `alloc_zeroed` before it. Nothing new interacts with the new `Cell` field.
- **Byte ranges re-derived for all four new tests.** cap 4096;
  `clobbered_by_later_alloc`/`alloc_fill_that_does_not_stick`: block 0 `[0,64)`,
  block 1 `[64,96)`, corruption `[0,64)` — disjoint, in-bounds, and invisible to
  the incremental overlap check (which compares address ranges, never contents),
  so only the run-end sweep can see it. `alloc_zeroed_fill_that_does_not_stick`:
  identical offsets through the zeroed arm; op 1's own zero-check reads `[64,96)`
  and passes *because* the corruption lands outside it — the construction is
  correct, not accidental. `null_realloc_completes_with_old_block_intact`:
  fills 1 and 2 on `[0,64)` and `[64,96)`, both survive both null reallocs and
  the run-end sweep. `at_len` bounds hold everywhere.
- One correction to the *claim*, not the mechanism: the run-end sweep is the
  **only** `drive` branch these three `should_panic` tests reach. They pin one
  assert with three inputs (see P4 §new-4), and deleting `drive.rs:274` (the
  Alloc arm's op-time `verify_block`) or `drive.rs:324` (P3-1's new AllocZeroed
  one) still leaves all fourteen tests green. The module doc at `:5-13` is
  honest about this ("One check genuinely has no in-op counterfactual…"), so it
  is not a finding — but round 3's second gap is documented-as-unclosable, not
  closed, and the file's own summary sentence should say so in the plural now
  that P3-1 created a second such check.

### (b) `validate()`'s new size ceiling across BOTH front-ends — HOLDS as a *divergence* check, FAILS as a *class* check

Re-checked `strategy.rs:14-22` and `arbitrary_stream.rs:33-49` against the new
ceiling rather than the `validate()` call site, as asked. The two front-ends
agree **exactly** at every boundary — there is no front-end divergence:

| `Config` | proptest `size_strategy` | arbitrary `bound_size` | agree? |
|---|---|---|---|
| `small_max: 0` | small arm `1..=1` | `magnitude % 1 + 1 = 1` | yes |
| `large_max <= small_max` | `s+1 ..= s+1` | `lo=s+1`, `hi=lo`, `lo + m%1 = s+1` | yes |
| `small_max: isize::MAX` | large arm `2^63 ..= 2^63` | `lo = 2^63`, result `2^63` | yes |

But the third row is the finding: at the ceiling `validate()` now blesses, the
large arm's `lo = small_max + 1` is `2^63`, i.e. **one past** the bound
`validate()` enforces, in both front-ends identically. See **P3-3**.

### (c) P3-6's "only when the clamp changed the value" — one real edge case

`size != original_size` is true for the clamp-**up** direction too (`size: 0`
→ `1`), where the message's OOM note is false. See **P3-1**.

The reverse (false when it should have fired) does not occur: a size at or
below `(isize::MAX / align) * align` and `>= 1` is passed through unchanged, so
the plain message is correct and byte-identical to the pin in
`null_alloc_panics`.

### (d) The `RawAllocator` doc restructuring — links fine, one duplication, one omission

All intra-doc links resolve: `RawAllocator::{alloc,alloc_zeroed,dealloc,realloc}`,
`crate::Config::double_free` (a field link, supported), `crate::drive`,
`GlobalAlloc`, `Layout`, and the `RawAllocator#safety` fragment (the `# Safety`
`h2` anchor survives the two new `##` sub-headings). The
private-module-vs-public-function ambiguity question for `crate::drive` was
settled in run 2 §"the 4-way file split" against in-repo precedent
(`crates/aligned-vmem`'s `mod page_size` + `pub use page_size::page_size` under a
green `-D warnings` doc gate); nothing in this round's diff changes that.

No **contradiction** between the trait-level doc and the blanket-impl doc — but
the three `GlobalAlloc` preconditions are now stated verbatim in *both*
(P4 §new-9), and the newly-created caller-obligations list enumerates
`dealloc`'s rule while omitting `realloc`'s (**P3-2**).

### (e) The miri job's four steps — the plain/strict split HOLDS; the target list does not

Re-verified the whole job (`ci.yml:907-923`), not just the changed line, and
confirmed there is no workflow-level `env: MIRIFLAGS` that could leak into the
now-bare step (`ci.yml:27-40` sets only `CARGO_TERM_COLOR` and the clippy
policy note):

| step | targets | MIRIFLAGS | leak check |
|---|---|---|---|
| 1 | `double_free_no_op` | `-Zmiri-ignore-leaks` | off (correct — `LeakyAllocator` leaks by design) |
| 2 | the other four | *(none)* | **on** ← the restored plain pass |
| 3 | `double_free_no_op` | `-Zmiri-ignore-leaks -Zmiri-strict-provenance` | off |
| 4 | the other four | `-Zmiri-strict-provenance` | **on** |

`-Zmiri-ignore-leaks` is scoped to exactly one target in both passes, and the
"all survivors are freed, model dropped before returning" property `drive`
advertises is still policed by the default leak check on the other targets.
P2-1 is genuinely fixed. What is *not* fixed is which targets appear in that
list at all — see **P2-1** below.

### (f) Bonus: does the `ConfiguredOpStream` newtype really preserve corpus compatibility?

Yes. `libfuzzer-sys`'s typed form calls `Arbitrary::arbitrary_take_rest`;
`ConfiguredOpStream` defines only `arbitrary`, so the trait's default
`arbitrary_take_rest` forwards to it with an `Unstructured` over the full byte
slice — the same decode the pre-round-3 `|data: &[u8]|` form performed by hand,
and the same one the pre-centralization `|stream: OpStream|` form performed.
`size_hint` is the trait default `(0, None)` on both `OpStream` and
`ConfiguredOpStream`, so `fuzz_target!`'s "not enough bytes" early-exit is
unchanged (lower bound 0). The `Err` path is also equivalent: the macro returns
`-1` where the old code did `else { return; }`. Git-tracked seeds in
`fuzz/corpus/global_alloc_ops/` decode identically. The target is compiled
per-push by `ci.yml:3551-3582` (`fuzz-build`), so this is CI-covered, not just
locally checked.

---

## P0 — none

No unsoundness and no undefined behaviour. The three new fault mechanisms write
only inside the arena's single allocation (bounds established by
`bump_aligned`'s own assert before the pointer is ever published); the new
`drive` code paths add no `unsafe`; `config.rs` and the fuzz newtype are
entirely safe code.

## P1 — none

---

## P2

### P2-1 — the miri job's hand-listed target list silently dropped the test file round 3 itself added

*Axis: ошибки (a CI gate that does not do what it documents).*

`.github/workflows/ci.yml:890-892` still claims:

> Two passes over the **whole suite**: plain, then with
> `-Zmiri-strict-provenance`, matching every other miri job in this file.

and `:898-899` says `-Zmiri-ignore-leaks` is scoped to one target "while the
**other four** targets keep the default leak check ON".

The crate now has **six** integration-test targets:

```
tests/config_validate.rs      ← added by round 3's P3-3 fix, in NEITHER list
tests/double_free_no_op.rs
tests/miri_bounded.rs
tests/oracle_negative.rs
tests/system_arbitrary.rs
tests/system_proptest.rs
```

Both `--test`-filtered steps name the same four (`miri_bounded`,
`oracle_negative`, `system_arbitrary`, `system_proptest`), and step 1/3 name
`double_free_no_op`. `config_validate` runs under miri **zero times**, in either
pass, while the job's comment asserts whole-suite coverage — and the job stays
green, so nothing surfaces it. This is round 3's P2-1 exactly, one round later,
introduced by round 3's own fix for a *different* finding.

**Honest impact statement, so this can be re-graded if you disagree:**
`config_validate.rs` executes no `unsafe`, no allocator, and no pointer
arithmetic — every test in it panics inside `Config::validate()` or returns
immediately — so today's coverage loss is **nil**, exactly as round 3's P2-1
lost nil coverage. What is real is (1) the job's documentation is false again,
and (2) the mechanism guarantees recurrence: the job it cites as its model,
`once-ptr-cell-miri` (`ci.yml:879-882`), runs bare `cargo miri test -p
once-ptr-cell` with no `--test` filters and therefore *cannot* drift; this job
hand-lists targets solely to scope `-Zmiri-ignore-leaks`, and a hand-listed set
that must be edited every time a `tests/*.rs` file is added is the same
drift surface CLAUDE.md's `cargo-hack` rule already rejects elsewhere ("each
new row only covers the ONE combination someone thought to write down"). The
next negative-oracle file — which *would* carry real unsafe surface — falls out
of the gate the same silent way. I grade it P2 for consistency with round 3's
identical call, not because a bug can hide behind it today.

**Fix (two parts, both cheap):**

1. Add `--test config_validate` to the steps at `:916` and `:921`, and update
   "the other four targets" at `:898-899` to a count-free phrasing ("every
   target except `double_free_no_op`"), per this repo's no-hardcoded-counts
   convention.
2. Make the drift impossible rather than re-fixable: add one step that fails if
   the file set and the flag set disagree, e.g.

   ```yaml
   - name: miri target list covers every tests/*.rs
     run: |
       set -euo pipefail
       listed=$(grep -o -- '--test [a-z_]*' .github/workflows/ci.yml \
                | grep -A0 . | awk '{print $2}' | sort -u)
       actual=$(ls crates/globalalloc-model/tests/*.rs | xargs -n1 basename | sed 's/\.rs$//' | sort)
       diff <(echo "$listed") <(echo "$actual")
   ```

   (Tighten the `grep` to this job's own line range; the point is that the
   assertion lives in CI, not in a comment.)

---

## P3

### P3-1 — three of the four clamp cases `drive`'s totality promise names have no test at all, and round 3's new null message actively mislabels one of them

*Axis: улучшение (missing coverage on the crate's most soundness-relevant fix)
+ ошибки (message mis-attribution, new this round).*

`src/drive.rs:186-192` promises:

> `drive` is total over every hand-built `Op` value: **a size of `0`**, an
> oversized size, **a `new_size` of `0`**, and **a `new_size` whose round-up
> overflows `isize`** are all clamped into the range `GlobalAlloc`'s own
> contract permits, so the allocator is never invoked outside its documented
> preconditions.

That clamp is round 1's **P0-1** — the single highest-severity finding of this
whole review cycle, and the one thing standing between a safe `pub fn` and
`GlobalAlloc::alloc(Layout(0, 8))`, which is UB. Of the four enumerated cases,
exactly **one** has a test: `oversized_size_is_clamped_not_rejected`
(`tests/oracle_negative.rs:415-429`). I grepped every op literal in all six test
files plus both root-crate consumers: **no test anywhere uses `size: 0` or
`new_size: 0`**. Three rounds of review, and the P0 fix's primary direction is
unexercised.

Round 3 then edited that exact code path (`drive.rs:244-261`, `:286-305`) to add
`original_size` and a clamp-aware null message — and did not add a test for
either direction of the new branch. The clamp-**up** direction now produces a
message that is false:

```
Op::Alloc { size: 0, align: 8 }   // hand-built; allocator returns null
→ M1: op #0 alloc(size=1 [clamped from 0], align=8) returned null — note: the
  harness does not model OOM, so a hand-built size beyond the allocator's real
  capacity reports here
```

A **1-byte** request is not "a size beyond the allocator's real capacity". The
note was written for the clamp-down direction (`usize::MAX` → ~8 EiB) and is
applied to both, so the one case where a null *is* a genuine allocator defect —
the crate's entire product — gets an explanation that points the reader away
from it. This is the same mis-attribution class as round 3's own P3-6, inverted
by its fix.

**Fix:**

1. Gate the OOM note on the clamp *direction*: `if size < original_size` for the
   note; `if size > original_size` gets its own text
   (`"[clamped from 0 — GlobalAlloc forbids a zero-size layout]"`, no OOM note).
2. Add the missing counterfactuals. They are cheap and, unlike the M1 read-back,
   genuinely constructible: give `Faulty::alloc`/`alloc_zeroed`/`realloc` a
   `assert!(layout.size() > 0, "GlobalAlloc precondition violated: zero-size
   layout")` (and the same for `new_size`) — the fake then *documents* the
   precondition it is entitled to rely on, and `Op::Alloc { size: 0, align: 8 }`
   / `Op::Realloc { i: 0, new_size: 0 }` pass only because the clamp ran.
   Deleting the clamp fails them immediately, natively, with a clear message —
   no reliance on miri catching a zero-size `__rust_alloc`.
3. One `Fault::NullAlloc` test at `size: usize::MAX` pins the clamped-down
   message, which is currently unpinned.

### P3-2 — `RawAllocator::realloc` has no documented caller obligation, and its implementor guarantee as written is unsatisfiable

*Axis: ошибки (an unsafe contract that cannot be met as stated) — a gap made
conspicuous by round 3's P3-5 restructuring.*

`src/raw_allocator.rs:24-52`. The new split lists, under **"Guarantees the
implementor must provide"** (`:31-35`):

> `realloc` either returns null (leaving the old block live and valid) or a
> pointer valid for reads and writes of `new_size` bytes, inside one live
> allocation reclaimable by `dealloc` …, consuming the old pointer on a
> non-null return.

and, under **"Guarantees the implementor may rely on from callers"**
(`:39-52`), exactly two bullets: `dealloc`'s matching-pointer rule, and the
blanket-impl `GlobalAlloc` preconditions. **Nothing says `realloc`'s `ptr` must
be a live block previously returned by this allocator with `old_layout`.**

Two consequences, both real:

1. Read literally, the implementor guarantee above is **impossible to honour**:
   it promises a valid `new_size` block for *any* `ptr`/`old_layout` a caller
   supplies, including a dangling or foreign one.
2. `unsafe fn realloc` (`:89-93`) says only "See the trait-level contract" — so
   a third-party caller of the public `unsafe` method has, on the rendered
   docs.rs page, **no stated precondition on `ptr` at all**. This is the same
   shape as round 3's P3-7 (a published `unsafe` surface whose preconditions are
   not reachable from the page), and round 3 fixed the `dealloc` half of it
   (`:80-87` now spells the obligation out inline) while leaving the `realloc`
   half pointing at a list that does not contain it.

**Fix:** add the symmetric bullet to the caller-obligations list and inline it on
the method, mirroring what `dealloc` already got:

```rust
/// - [`realloc`](RawAllocator::realloc) is called only with a pointer
///   previously returned by a matching `alloc`/`alloc_zeroed`/`realloc` on
///   `self` with `old_layout`, and not yet freed or consumed.
```

### P3-3 — `validate()`'s new `isize::MAX` size ceiling does not close the class it was added for, and the test blessing it states a false Layout property

*Axis: ошибки (a false claim in new code) / улучшение (an arbitrary bound
presented as a principled one).*

`src/config.rs:113-136` now rejects `small_max`/`large_max > isize::MAX`,
documented at `:26-56` as closing the "no limit" spelling because "no `Layout`-
based allocator can ever serve such a size". But the admissible ceiling `drive`
itself uses is **align-dependent** — `(isize::MAX / align) * align`
(`drive.rs:245`, `:287`, `:373-376`) — and `isize::MAX` is above it for every
`align > 1`. Two concrete consequences at the value the new bound blesses:

- `tests/config_validate.rs:71-80`'s comment — *"The new bound is inclusive:
  `isize::MAX` itself stays Layout-admissible"* — is **false for the very config
  the test constructs**: it inherits `..Config::default()`, i.e.
  `max_align: 4096`, and `Layout::from_size_align(isize::MAX, 4096)` is an
  `Err` (`size + align - 1` overflows `isize`). It is admissible only at
  `align == 1`.
- `Config { small_max: isize::MAX, large_max: isize::MAX, ..default() }` passes
  `validate()` and then makes **both** front-ends emit sizes of `2^63`
  (`small_max + 1`, one past the bound just enforced — derived in §0.b above,
  identically in `size_strategy` and `bound_size`), which `drive` clamps and a
  real allocator answers with null → *guaranteed* M1 report with the clamp note.
  That is precisely the failure mode P3-4 was added to eliminate, reachable
  through a config the new test pins as valid.

The bound is therefore a *sanity ceiling*, not an admissibility guarantee — which
is fine, but it is documented as the latter in three places (`config.rs:31-36`,
`:44-51`, `:107-112`) and pinned as the latter by a test.

**Fix (cheapest, no behaviour change):** correct the test comment to name the
align dependence, and reword the three field/`# Panics` sentences from "never
admissible by `Layout::from_size_align`" to "far above the align-dependent
ceiling `drive` clamps to (`(isize::MAX / align) * align`), so every reached op
is a guaranteed M1 null report — this bound is a sanity ceiling, not an
admissibility guarantee". Optionally also clamp the derived large-arm floor
(`lo = small_max.saturating_add(1)`) to the same ceiling in both front-ends, so
no generated size can exceed a bound `validate()` accepted.

### P3-4 — the config-aware fuzz entry point's only usage pattern lives outside the published crate, and the obvious alternative reproduces the defect round 3 just fixed

*Axis: улучшение (missing docs on the crate's headline extension point).*

`src/arbitrary_stream.rs:118-133`. `OpStream::arbitrary_with_config` is the
crate's config-aware fuzz route, and its rustdoc's only pointer to a usage
example is:

> the in-tree `global_alloc_ops` fuzz target drives it directly with an
> explicit `Config`

A crates.io / docs.rs reader cannot open that file — it lives in the
`sefer-alloc` repository's `fuzz/` directory, not in the published tarball.
This is the same "dead reference on the published page" shape as round 3's P3-7,
in a sibling doc that the same round did not touch.

It matters more than a normal missing example, because the non-obvious part is
structural: `Arbitrary` takes no parameters, so a custom `Config` *cannot* be
threaded through `fuzz_target!(|stream: OpStream|)`. The obvious workaround a
user reaches for is `fuzz_target!(|data: &[u8]| { let mut u =
Unstructured::new(data); … })` — which is exactly what this repo's own target did
after round 2, and exactly what round 3's **P3-9** filed as a defect (it
silently loses libFuzzer's `Debug`-based crash rendering via
`RUST_LIBFUZZER_DEBUG_PATH` / `cargo fuzz fmt`). The crate's docs currently lead
every downstream user into the pattern this cycle already rejected once.

**Fix:** add a ` ```text ` block (per this repo's no-doctests rule) to
`arbitrary_with_config`'s rustdoc showing the newtype, which is the whole
answer:

```text
#[derive(Debug)]
struct MyStream(OpStream);
impl<'a> arbitrary::Arbitrary<'a> for MyStream {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        OpStream::arbitrary_with_config(u, my_config()).map(Self)
    }
}
fuzz_target!(|s: MyStream| drive(&alloc, my_config(), &s.0.ops));
```

and one sentence saying why (`Arbitrary` cannot carry a parameter; the newtype
keeps the structured crash report). The README's "One model, two front-ends"
section is the natural second home.

*Downgrade to P4 if you read missing usage docs as never above P4 — the finding
is a documentation gap, not a defect in what the crate does. I grade it P3
because the gap has a demonstrated cost: this repo paid it itself, for a full
round, and fixed it in-tree without fixing the doc that caused it.*

---

## P4

### New this round

1. **`.github/workflows/ci.yml:2320`** *(smell)* — the MSRV job's comment says
   "its **four** `tests/` files"; there are six. Hardcoded counts in comments
   drift silently — the same convention (task #776/F10) CLAUDE.md already states
   for the `cargo-hack` command count. Re-phrase count-free.
2. **`tests/oracle_negative.rs:153-159`** *(smell)* — `Fault::ClobberOnLaterAlloc`'s
   doc says "Every **`alloc`** call after the first", but the fault also fires
   from `alloc_zeroed` (both call `fault_touch_first_block`). Its sibling
   `WritesDoNotStick` (`:161-165`) gets this right ("Every `alloc`/`alloc_zeroed`
   call").
3. **`tests/oracle_negative.rs:190-192`** *(smell)* — the `// SAFETY:` note
   justifies the write with "it was handed out length-checked through `at_len`",
   but on the `alloc` path (`:231-233`) `at_len` runs *after*
   `fault_touch_first_block`. The bound is sound — it comes from
   `bump_aligned`'s own `p + n <= cap` assert — but the comment cites the wrong
   check, which is exactly the kind of note a future edit trusts.
4. **`tests/oracle_negative.rs:153-165`, `:431-467`** *(smell)* —
   `ClobberOnLaterAlloc` and `WritesDoNotStick` are the same mechanism differing
   only in the byte written (`0xCC` vs `0x00`); through the `alloc` arm the two
   tests hit the same assert at the same block, offset and failure byte. The
   distinction is defensible (`next_fill` reserves `0` precisely so "lost write"
   is distinguishable from "foreign byte" — `drive.rs:38-39`), but one
   `Fault::ScribbleFirstBlock(u8)` would say that in one place.
5. **`src/arbitrary_stream.rs:45-47`** *(smell)* — "Saturating: an extreme
   `large_max` (**e.g. `usize::MAX`**) must not overflow this arithmetic into a
   division-by-zero panic". `usize::MAX` is now rejected by `validate()`, which
   round 3 added in the same commit. The saturating arithmetic is still needed
   (for `small_max = isize::MAX`, where `hi - lo` is 0), but the cited example is
   unreachable — cite the reachable one.
6. **`src/arbitrary_stream.rs:34-37`** *(smell)* —
   `small_weight.saturating_add(large_weight).max(1)` is dead after
   `validate()` rejects an all-zero weight sum; same shape as run 3's P4-5
   (`max_align.max(1)` at `:53`), which is still open.
7. **`tests/oracle_negative.rs:5-6`** *(smell)* — "Every case is counterfactual —
   deleting the corresponding **assert** in `drive` must fail the matching test".
   Four of the fourteen tests are positive or harness tests; `honest_arena_passes_drive`
   has no corresponding assert at all, and `in_place_realloc_…`,
   `null_realloc_completes_…` and `oversized_size_is_clamped_…` are
   counterfactual against a *branch* (`skip: Some(i)`, the `continue`, the clamp),
   not an assert. Widen "assert" to "assert or branch" and carve out the one
   infrastructure test.
8. **`src/raw_allocator.rs:82-86`** *(improvement)* — `dealloc`'s `# Safety`
   points at `RawAllocator#safety` while naming "caller obligations", which now
   has its own heading and anchor
   (`#guarantees-the-implementor-may-rely-on-from-callers`). Link the sub-heading.
9. **`src/raw_allocator.rs:46-52` vs `:96-105`** *(smell)* — the three
   `GlobalAlloc` preconditions (non-zero size, non-zero `new_size`, no `isize`
   overflow after round-up) are now stated verbatim in both the trait's
   caller-obligations list and the blanket impl's `# Safety`. Composing P3-5 and
   P3-7 produced two copies that must stay in sync by hand; keep the impl's as the
   normative one and have the trait bullet link to it.
10. **`tests/system_proptest.rs:1-4`** *(smell)* — the module doc describes only
    "the proptest front-end, driven against the always-correct `System`
    allocator"; the new `two_default_test_runners_seed_independently`
    (`:39-58`) drives nothing and asserts a build-configuration property. One
    clause.
11. **`fuzz/Cargo.lock:208`, `:248`** *(smell)* — the lock still names
    `racy-ptr-cell`, a crate renamed to `once-ptr-cell` (`crates/` has no
    `racy-ptr-cell`; `Cargo.toml:917` names the new one). Three consecutive
    commits (`e707d76`, `47fc214`, `d3d8f5c`) each record deliberately
    *reverting* the regeneration their own verification triggered. `cargo fuzz
    build` re-resolves without `--locked`, so nothing fails — but the repo's own
    precedent is to sync it (`36b4b3e`, "sync fuzz/Cargo.lock with the
    workspace's tagged-index-stack dependency"). Sync it once, in its own
    commit, and the review-fix cycle stops fighting it.
12. **`CHANGELOG.md:66-71`** *(improvement)* — the `Config` bullet describes the
    knobs but never mentions `Config::validate()`, a public method with a
    documented `# Panics` contract, nor that **both front-ends now panic** on a
    degenerate config before generating anything (`strategy.rs:53-56`,
    `arbitrary_stream.rs:129-132`). For a first release whose changelog says
    "Everything below is new in this version", a new panicking public entry
    point is the bullet most worth having.
13. **`src/drive.rs:314-325` + `tests/system_proptest.rs:15-18`** *(improvement)*
    — P3-1 added a third full-block pass to every `alloc_zeroed` op (zero-check
    read, fill write, read-back). `system_arbitrary.rs:22-27` calls that
    byte-at-a-time work "the dominant cost of the CI miri job" and shrinks
    *sizes* under `cfg!(miri)`; `system_proptest.rs` still shrinks only
    `CASES`/`MAX_LEN` and keeps the 128 KiB default `large_max` under the
    interpreter. Run 3's P4-7 with a bigger multiplier.

### Carried over from round 3, re-verified as still present at `d3d8f5c`

Line numbers unchanged unless noted; see run 3 §P4 for the full reasoning.

14. `CHANGELOG.md:23-25` — the totality claim omits the never-admissible-align
    caveat `drive.rs:190-192` states. (run 3 P4-1)
15. `src/config.rs:66-74` — `Config::max_align`'s rustdoc still does not mention
    that the arbitrary front-end caps aligns at `2^21` regardless. This is the
    one case where a **valid** value is still "silently reinterpreted differently
    by each generator" — the exact thing `validate()`'s own doc (`:88-93`) says it
    exists to prevent. (run 3 P4-2)
16. `src/arbitrary_stream.rs:20-27` — `ALIGN_POW_CAP_EXP` bakes a sefer-specific
    4 MiB-segment rationale into a non-configurable constant. (run 3 P4-3)
17. `src/strategy.rs:26` — `debug_assert!(max_align.is_power_of_two())`
    unreachable after `op_strategy:61`'s `validate()`. (run 3 P4-4)
18. `src/arbitrary_stream.rs:53` — `config.max_align.max(1)` dead. (run 3 P4-5)
19. `src/drive.rs:48-52` — `layout_for`'s `unwrap_or_else(panic)` unreachable at
    all four call sites. (run 3 P4-6)
20. `Cargo.toml:45` — proptest's `no_std` feature forces `num-traits/libm` into
    every consumer's graph. (run 3 P4-8)
21. `src/drive.rs:425-437` — the run-end sweep passes a **block** index into
    `verify_block`'s `step` parameter, so the message reads `M3: step #0 …` while
    `drive`'s `# Panics` (`:216`) promises "Every message names the op index and
    its operands". Now pinned as behaviour by **three** new `#[should_panic]`
    strings, so fixing it costs test churn it did not cost last round. (run 3 P4-9)
22. `src/drive.rs:441-447` — teardown re-implements `layout_for` inline with a
    different message. (run 3 P4-10)
23. `src/drive.rs:33` — `#[must_use]` on `ranges_overlap` is inert. (run 3 P4-11)
24. `src/drive.rs:80-95` — `assert_no_overlap` and `ranges_overlap` compute the
    same two saturating sums twice. (run 3 P4-12)
25. `tests/double_free_no_op.rs:26-29`, `:93`, `:95` — `dealloc_count` duplicates
    `frees.borrow().len()`. (run 3 P4-13)
26. `tests/system_arbitrary.rs:44` — `(0u16..512).map(|i| (i as u8)…)` truncates;
    the "512-byte" buffer is 256 bytes twice. (run 3 P4-14)
27. `src/arbitrary_stream.rs:33-49` — `bucket` and `magnitude` derive from the
    same `u32`, so the large arm clusters near `small_max + 1`. (run 3 P4-15)
28. `src/arbitrary_stream.rs:60-88` — `RawOp` is private yet every variant and
    field carries `///`. (run 3 P4-16)
29. `src/arbitrary_stream.rs:110-115` — `OpStream`'s doc demotes `OpStream::ops`
    and `crate::drive` to plain code spans while the module header at `:2` links
    `[crate::drive]`. (run 3 P4-17)
30. `CHANGELOG.md:91-93`, `Cargo.toml:6` — internal review identifiers
    ("review P3-23") still ship in two crates.io-rendered artifacts. (run 3 P4-18)

---

## Summary

| Severity | Count |
|---|---|
| P0 | 0 |
| P1 | 0 |
| P2 | 1 |
| P3 | 4 |
| P4 | 30 (13 new, 17 carried) |

**Not clean at P0–P3, so this cycle should run one more round** — but the trend
is real and the remaining set is small and mechanical: 47 → 14 → 10 → 5
findings at P0–P3, and for the second round running there is **no defect in
what the crate actually does at runtime**. Everything at P3 is a missing test,
an under-specified doc contract, or a claim that overstates what a bound buys.

Every round-3 fix I could re-derive independently holds: the miri plain/strict
split and its `-Zmiri-ignore-leaks` scoping (all four steps, plus the absence of
a workflow-level `MIRIFLAGS`), the `fault_touch_first_block` bookkeeping
(disjointness proven from `bump_aligned`'s monotonic cursor, no cross-fault
contamination across any round-1/2 variant), the two front-ends' now-identical
treatment of every `Config` boundary, the `RawAllocator` doc's link integrity,
and the fuzz newtype's corpus and `size_hint` compatibility.

The meta-pattern the prompt asked me to hunt **did recur, twice, in the same two
shapes as before**:

- **A gate whose list drifted out of sync with reality** — round 3 fixed the
  miri job's `MIRIFLAGS` and, in the same commit, added a sixth test file that
  the job's hand-listed targets do not include (**P2-1**). Third consecutive
  round in which this job is the finding.
- **A fix that edited a code path without covering it** — round 3 added
  `original_size` and a clamp-aware null message to both alloc arms and pinned
  neither direction, leaving round 1's **P0-1** zero-clamp with no test after
  three full rounds, and giving the clamp-up direction a message that blames the
  allocator for the harness's own rewrite (**P3-1**).

If only two things land before the tag, make them those two. **P3-2** (the
`realloc` caller obligation) is the one I would not ship an `unsafe` public
trait without, and it is four lines of rustdoc.
