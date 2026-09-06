# globalalloc-model — independent read-only review, run 3

**Verdict: GO-WITH-FIXES.** Not clean at P0–P3. No P0, no P1. **One P2** (a CI
gate that no longer does what it documents — a regression introduced by round
2's own P3-8 fix, invisible because the job still goes green) and **nine P3s**
(one oracle asymmetry, four coverage gaps, two doc-of-contract defects, one
message-attribution defect, one fuzz-debuggability regression). Everything at
P3 is safe to land after the 0.1.0 tag; the P2 is one line of `ci.yml`.

Scope: `crates/globalalloc-model/` at `main` @ `47fc214`, read statically. No
command that builds, tests, lints or formats anything was run; the only tools
used were file reads, `git show`, `grep`, and reads of the vendored
`proptest-1.11.0` source in the local registry (to settle the feature-resolution
question in §0.a from primary source rather than from the commit message).

---

## 0. Round-2 fix verification — what I re-derived independently

The prompt named five specific re-checks. Four hold. One does not.

### (a) The two `proptest` manifest entries — HOLDS

`Cargo.toml:45` declares `proptest = { optional = true, default-features =
false, features = ["alloc", "no_std"] }`; `Cargo.toml:60` adds
`[dev-dependencies] proptest = { version = "1" }` (defaults on). Re-derived
from primary sources rather than from the commit message:

- The workspace root sets `resolver = "2"` explicitly (`Cargo.toml:87`), and
  the crate's own `edition = "2021"` gives resolver 2 when built standalone
  from the published tarball. So dev-dependency feature unification is
  suppressed for every build that does not compile a dev target.
- proptest 1.11's `no_std` feature is **`no_std = ["num-traits/libm"]` and
  nothing else** — `grep -rn 'feature = "no_std"'` over
  `proptest-1.11.0/src/` returns **0 matches**. It is a dependency-routing
  flag, not a "turn std off" switch (that is `default-features = false`).
  Therefore `std` + `no_std` both enabled is not a conflicting combination:
  every `#[cfg(feature = "std")]` site — including
  `TestRng::default_rng` (`proptest-1.11.0/src/test_runner/rng.rs:383-424`,
  whose `#[cfg(not(feature = "std"))]` branch at `:424` is the hardcoded-seed
  path round 2 diagnosed) — takes the std branch under unification.
- Per CI row: `cargo build … --target thumbv7em-none-eabi`
  (`ci.yml:2088`, `:2095`) and `cargo check -p globalalloc-model
  --all-features` (`ci.yml:2326`, `:2351`) build no dev target → no_std
  feature set → the bare-metal claim still holds. `cargo test`
  (`ci.yml:2101`, `:2102`), `cargo test --no-run` (`:2327`) and
  `cargo miri test` (`:912-921`) do build dev targets → std → real seeding.
  `cargo package`/`publish --dry-run` verify-builds the packaged crate with
  `default = []`, so the optional `[dependencies]` entry is not even
  activated and the dev-dep only has to *resolve* (it does; it carries a
  version, as publishing requires).

No path reintroduces `std` into a bare-metal build. **One caveat, not a
finding:** the round-2 commit's own verification list enumerates
test/clippy/fmt/doc, both bare-metal builds, MSRV 1.85, `cargo package`, and
the fuzz target's compile — but **not a miri run**. The P2-1 fix materially
changes what miri interprets (from a hardcoded-seed RNG to
`XorShiftRng::from_os_rng()` plus proptest's `std`/`fork`/`timeout`
machinery). Static reading finds nothing miri lacks a shim for
(`getrandom`, `std::env::var`, `Instant::now` are all shimmed;
`rusty-fork`/`tempfile` are compiled but never executed at
`fork: false`/`timeout: 0`), so I expect it is green — but it was not
verified by that commit and I cannot verify it here.

### (b) `Config::validate()` at every call site — HOLDS

Checked all five reachable configs against both preconditions
(`max_align` a non-zero power of two ≤ `isize::MAX`; weight sum > 0):

| Config | `max_align` | weights | verdict |
|---|---|---|---|
| `Config::default()` (`config.rs:109-117`) | 4096 | 9+1 | passes |
| fuzz target (`fuzz/fuzz_targets/global_alloc_ops.rs:90-97`) | 2 MiB | 9+1 | passes |
| `tests/alloc_core_differential.rs:58-65` (ROOT crate) | 4096 | 1+1 | passes |
| `tests/heap_differential.rs:58-65` (ROOT crate) | 4096 | 9+1 | passes |
| `system_arbitrary.rs:31-35` miri variant | 4096 | 9+1 | passes |

No consumer regressed. `drive()` deliberately does **not** call `validate()`
(it reads only `double_free`), which is consistent — a `Config` with a
degenerate `max_align` is harmless to `drive`.

### (c) The alloc arms' `validate_align` + `clamp` — HOLDS

Re-derived rather than trusted: for any power-of-two `align = 2^k`,
`(isize::MAX as usize / align) * align = 2^63 − 2^k`, and
`Layout::max_size_for_align(align) = isize::MAX − (align − 1) = 2^63 − 2^k`.
**Identical.** So the clamp ceiling is exactly `Layout`'s admissible maximum,
in both alloc arms and the realloc arm. `validate_align` (`drive.rs:59-66`)
rejects `align == 0`, non-powers-of-two, and `2^63` (the only power of two
whose round-up overflows `isize`) **before** the division, so the
`/ align` cannot divide by zero and the subsequent `layout_for` is genuinely
infallible. Ordering is correct.

The one behavioural consequence the fix did introduce is real but not a bug
in the arithmetic — see **P3-6**.

### (d) `oracle_negative.rs` fault arithmetic after `bump_aligned`/`at_len` — HOLDS

I re-derived every byte range from scratch for all ten tests rather than
reading the round-2 commit's numbers. Sample of the load-bearing ones:

- `undersized_block_overlap_panics`: op 0 → `p = 0`, cursor `0→63`, block
  model extent `[0,64)`; op 1 → `p = (63−1)/8*8 = 56`, cursor `63→126`,
  extent `[56,120)` — overlaps `[0,64)` by **8 bytes**, matching the enum
  doc's own corrected claim (`oracle_negative.rs:118-122`). `at_len(56,64)`
  → `120 ≤ 4096` ✓.
- `overlap_on_realloc_panics`: blocks land at `[0,64)` and `[64,128)`;
  `ReallocAt(80)` returns `[80,144)` — inside block 1, disjoint from the old
  block, so M3 is the only assert that can fire. ✓
- `in_place_realloc_inside_own_old_block_passes`: `ReallocAt(8)` →
  `[8,72)`, `skip: Some(0)` excludes the only live entry, prefix copy is a
  `ptr::copy` (memmove) over an overlapping range. Passes, as pinned. ✓
- `oversized_size_is_clamped_not_rejected`: `usize::MAX` clamps to
  `9223372036854775800`, `bump_aligned` asserts `0 + that ≤ 4096` → panics
  `"arena exhausted"`. ✓ (But see **P3-6** — this message is arena-specific;
  a real allocator produces a different, mislabelled message.)
- Every `at_len` call across all fault branches stays inside `cap`. No
  offset shifted such that a test now exercises something else.

### (e) The miri job's 4-step split — **DOES NOT HOLD.** See **P2-1**.

### (f) Bonus: is the fuzz target's "historical reach" claim true?

Yes, exactly. The pre-centralization target (`git show
33d9a2b:fuzz/fuzz_targets/global_alloc_ops.rs`) used
`bound_size(raw) = (raw % 2 MiB) + 1` and
`bound_align(raw) = 1usize << (raw % 22)`. The new explicit `Config`
reproduces the align range bit-for-bit:
`min(trailing_zeros(2 MiB) = 21, ALIGN_POW_CAP_EXP = 21) + 1 = 22`, so
`bound_align` computes `1 << (raw % 22)` — identical. Size reach restored to
2 MiB; the distribution is now 9:1 small-heavy rather than historically
uniform, which is a deliberate and documented change, not drift.

Corpus compatibility also survives the `|stream: OpStream|` →
`|data: &[u8]|` switch: `libfuzzer-sys` uses
`Arbitrary::arbitrary_take_rest`, and `OpStream` defines only `arbitrary`,
so the derived `arbitrary_take_rest` forwards to it — the git-tracked
`fuzz/corpus/global_alloc_ops/` seeds decode identically. (There *is* a
separate cost to that switch — **P3-9**.)

---

## P0 — none

No unsoundness, no undefined behaviour, no wrong result. `drive`'s
`RawAllocator` calls all stay inside `GlobalAlloc`'s preconditions; the
teardown free-walk frees each survivor exactly once; the `skip: Some(i)`
realloc exclusion cannot admit a duplicate pointer into `live`.

## P1 — none

---

## P2

### P2-1 — the miri job's "plain, then strict-provenance" split runs the strict pass twice and the plain pass never

*Axis: ошибки (a CI gate that does not do what it documents).*

`.github/workflows/ci.yml:915-917` and `:921-923`:

```yaml
      - run: cargo miri test -p globalalloc-model --all-features --test miri_bounded --test oracle_negative --test system_arbitrary --test system_proptest
        env:
          MIRIFLAGS: "-Zmiri-strict-provenance"          # line 917  ← should be the PLAIN pass
      - run: cargo miri test -p globalalloc-model --all-features --test double_free_no_op
        env:
          MIRIFLAGS: "-Zmiri-ignore-leaks -Zmiri-strict-provenance"
      - run: cargo miri test -p globalalloc-model --all-features --test miri_bounded --test oracle_negative --test system_arbitrary --test system_proptest
        env:
          MIRIFLAGS: "-Zmiri-strict-provenance"          # line 923  ← byte-identical to line 917
```

Steps 2 and 4 are **byte-identical** — same argv, same `MIRIFLAGS`. So of the
five test targets, only `double_free_no_op` actually gets both provenance
modes (steps 1 and 3 differ correctly). The other four —
`miri_bounded`, `oracle_negative`, `system_arbitrary`, `system_proptest` —
run **twice under strict provenance and zero times under plain miri**.

This directly contradicts the job's own comment at `:891-892`: *"Two passes
over the whole suite: plain, then with `-Zmiri-strict-provenance`, matching
every other miri job in this file."* It is a regression introduced by round
2's P3-8 fix (which split one command into four to scope
`-Zmiri-ignore-leaks`), and it is exactly the pattern round 2 warned about:
the job still goes green, so nothing surfaced it.

**Honest impact statement, so this can be re-graded if you disagree:**
`-Zmiri-strict-provenance` only *adds* errors (it makes int-to-ptr casts an
error); it never accepts something plain miri rejects. So the *coverage* loss
is nil. What is real is (1) the job's documentation is false, and (2)
`system_proptest` + `system_arbitrary` under the interpreter are the dominant
cost of this job — the round-2 commit explicitly added a `cfg!(miri)` size
reduction to `system_arbitrary.rs` *because* of that cost — and the job now
pays that cost twice for zero added signal. I grade it P2 because a gate
whose stated coverage and actual coverage diverge is the defect class this
project treats as same-round-fixable, not because a bug can hide behind it.

**Fix:** drop the `env:` block from the step at `:915-917` (or set
`MIRIFLAGS: ""`), so it is the plain pass the comment describes.

---

## P3

### P3-1 — `alloc_zeroed` blocks never get the M1 write-read-back that `alloc` blocks get

*Axis: ошибки (an oracle asymmetry) / улучшение.*

`src/drive.rs:261-267` (Alloc arm):

```rust
let fill = next_fill(&mut cycle);
unsafe {
    fill_block(ptr, size, fill);
    verify_block(ptr, size, fill, "M1", op_idx, "alloc");   // ← read-back
}
```

`src/drive.rs:295-300` (AllocZeroed arm):

```rust
unsafe { verify_zeroed_block(ptr, size, op_idx) };
let fill = next_fill(&mut cycle);
unsafe { fill_block(ptr, size, fill) };                     // ← no read-back
```

The zero-check reads all `size` bytes and the fill writes all `size` bytes,
so readability and writability are both exercised — but the M1 property
*"write a distinctive fill byte, read it back"* is not. A block whose writes
do not persist is caught at op time for `alloc` and only at the run-end
sweep for `alloc_zeroed` — i.e. **never**, if that block is deallocated
before the run ends.

Both public descriptions of M1 state the read-back unconditionally:
`src/lib.rs:6-10` ("every returned pointer is non-null, aligned …, and
writable for the requested size (write a distinctive fill byte, read it
back…)") and `README.md:9-13`. Neither carves out `alloc_zeroed`.

**Fix:** either add `verify_block(ptr, size, fill, "M1", op_idx,
"alloc_zeroed")` after the fill (symmetric, costs one more byte loop per
zeroed block — relevant under miri), or state the asymmetry explicitly in
`drive`'s rustdoc and in the M1 bullets of `lib.rs`/`README.md`.

### P3-2 — `oracle_negative.rs`'s stated completeness claim is false: three `drive` branches have no counterfactual test

*Axis: улучшение (missing coverage) + ошибка (a false claim in the file's own doc).*

`tests/oracle_negative.rs:5-6`: *"Every case is counterfactual — deleting the
corresponding assert in `drive` must fail the matching test (verified during
development)."* Read as written this asserts the negative suite covers the
oracles. It does not cover three of them. Deleting any of the following from
`drive.rs` leaves **all ten** tests in this file green:

1. **`drive.rs:400-412` — the run-end M3 sweep.** All three overlap tests
   (`overlap_on_alloc_zeroed_panics`, `overlap_on_realloc_panics`,
   `undersized_block_overlap_panics`) fire on the *incremental*
   `assert_no_overlap`, at op index 1, long before the run-end loop. Nothing
   reaches the sweep with a clobbered survivor.
2. **`drive.rs:266` — the M1 fill read-back** (`verify_block` in the alloc
   arm). No `Fault` variant produces a block whose writes do not stick, so
   this assert has no counterfactual at all. (Cf. P3-1: its `alloc_zeroed`
   counterpart does not even exist.)
3. **`drive.rs:355-358` — the null-`realloc` skip** (`if new_ptr.is_null() {
   continue; }`). This branch is *soundness-critical*, not merely an oracle:
   delete the `continue` and the following code does
   `null.addr() % align == 0` (passes), then `assert_no_overlap` on
   `[0, new_size)` (passes — no live block is near address 0), then
   `verify_prefix_block(null, keep, …)`, which dereferences null. No test
   exercises a null return from `realloc`.

**Fix:** three small additions — a `Fault::ClobberOnLastAlloc` (or similar)
that scribbles over an earlier live block on a later `alloc`, reaching the
run-end sweep; a `Fault::WritesDoNotStick` (e.g. an `alloc` that returns a
pointer into a region the fake re-zeroes) for the read-back; and a
`Fault::NullRealloc` asserting the run *completes* and the old block still
verifies at run end. Alternatively, soften the claim at `:5-6` to name which
oracles are covered.

### P3-3 — `Config::validate()` is new public API with documented panics and zero tests

*Axis: улучшение (missing coverage).*

`src/config.rs:87-100` was added by round 2 (P3-2). It is `pub`, it is
rendered on docs.rs, it has a `# Panics` section naming two distinct failure
modes, and **nothing anywhere tests it** — not a passing case, not either
panic, not either message. This is the one place in the crate where a
documented `# Panics` contract has no pinned behaviour, in a repo whose own
negative suite exists precisely to pin panic-message prefixes as behaviour
(`tests/oracle_negative.rs:3-5`).

It also means the round-2 claim that both front-ends now "reject a degenerate
config identically" rests on reading two call sites (`strategy.rs:61`,
`arbitrary_stream.rs:137`), not on a test.

**Fix:** a small `tests/config_validate.rs` with four cases —
`Config::default()` passes; `max_align: 0` panics with `"non-zero power of
two"`; `max_align: 3000` panics; `small_weight: 0, large_weight: 0` panics
with `"must not both be zero"` — plus one `#[should_panic]` through
`op_strategy` and one through `OpStream::arbitrary_with_config`, proving both
front-ends really do reject before generating.

### P3-4 — `validate()` bounds `max_align` but leaves `small_max`/`large_max` unbounded, so the "no limit" spelling of a size is a guaranteed oracle failure

*Axis: улучшение (an incomplete precondition set).*

`src/config.rs:46-54` documents, correctly and at length, that
`max_align: usize::MAX` — *"the natural spelling of 'no limit'"* — is a
precondition violation, and `validate()` rejects it. But `small_max`
(`:28-30`) and `large_max` (`:33-36`) are documented as *"NOT a precondition
violation"* with no upper bound at all, and `validate()` (`:87-100`) checks
neither.

The consequence is asymmetric with the `max_align` reasoning it sits next to.
`Config { large_max: usize::MAX, .. }` — the same "no limit" spelling — makes
`size_strategy` (`strategy.rs:16-17`) generate sizes up to `usize::MAX`,
which `drive` clamps to ~8 EiB and hands to the allocator, which returns
null, which `drive` reports as **`M1: op #N alloc(...) returned null`** — a
false oracle failure on every such case. Even a merely large value
(`large_max: 1 << 40`) is a portability landmine: plausibly served under
Linux overcommit, always null on Windows.

That the author already considered `usize::MAX` here is visible in
`arbitrary_stream.rs:45-47`, whose comment reads *"Saturating: an extreme
`large_max` (e.g. `usize::MAX`) must not overflow this arithmetic into a
division-by-zero panic"* — the arithmetic was hardened against the value, but
the value was never rejected.

**Fix:** either add a `large_max <= isize::MAX as usize` (or a documented
sanity ceiling) check to `validate()`, matching the `max_align` treatment, or
add a sentence to both field docs stating that oversized values produce
guaranteed M1 null reports and that bounding them is the caller's job.

### P3-5 — the trait's `# Safety` section mixes an implementor guarantee with a caller obligation under a heading that says otherwise

*Axis: пахнущий код / улучшение (doc-of-contract, freshly rewritten in round 2).*

`src/raw_allocator.rs:14-38`. The section opens (`:16-17`) with *"This trait
is `unsafe` to implement. The implementor must guarantee exactly this and
nothing more"* — then bullet 2 (`:27-33`) is:

> `dealloc` **is called only with** a pointer previously returned by a
> matching `alloc`/`alloc_zeroed`/`realloc` on `self` with the same `layout`.

That is a *caller* obligation (what `drive` promises), not something an
implementor guarantees. Bullets 1 and 3 are implementor guarantees. Round
2's P2-3 fix correctly separated *oracles* from *safety*, but left
*implementor guarantees* and *caller obligations* interleaved in the same
list under a heading that explicitly claims the former.

**Fix:** split the `# Safety` section into two labelled sub-lists
("Guarantees the implementor must provide" / "Guarantees the implementor may
rely on from callers"), or move the `dealloc` bullet down to
`RawAllocator::dealloc`'s own method-level `# Safety` (`:67-69`), which
today is only a pointer back to the trait.

### P3-6 — an oversized hand-built `Alloc`/`AllocZeroed` now reports as an M1 oracle violation instead of a harness rejection

*Axis: ошибки (message mis-attribution introduced by round 2's P2-4 fix).*

`src/drive.rs:243-253`. Before round 2, `Op::Alloc { size: usize::MAX,
align: 8 }` panicked from `layout_for` with `"op #0:
Layout::from_size_align(size=…, align=…) rejected: …"` — unmistakably a
*harness* rejection. After the fix it is clamped to `9223372036854775800`
(~8 EiB), passed to the allocator, which returns null, and `drive` reports:

```
M1: op #0 alloc(size=9223372036854775800, align=8) returned null
```

i.e. **an oracle verdict against the allocator for what is plainly an OOM**.
The realloc arm's own comment (`:338-347`) reasons this through explicitly
and concludes it is fine *there* — *"An allocator is expected to answer that
with null (tolerated here as documented realloc failure)"*. The alloc arms
adopted the identical clamp **without** the identical tolerance: for `alloc`,
null is defined as failure (`drive.rs:207-209`). So the same clamp is benign
on one arm and a guaranteed false verdict on the other.

Round 2's own new test does not expose this, and that is the interesting
part: `oversized_size_is_clamped_not_rejected`
(`tests/oracle_negative.rs:359-373`) pins `"arena exhausted"` — a message
produced by the *fake arena's* `bump_aligned` assert, not by any real
allocator. Against `System` the same op yields the mislabelled M1 message
above. The test therefore verifies the clamp happened, but not what the clamp
*produces* for the crate's actual users.

Scope-limiting note, so this is not overstated: neither generator can produce
such a size (both cap at `large_max`), so this reaches only hand-built
streams; and the sub-ceiling variant of the same problem (`size: 5 TiB` →
null → M1) pre-dates round 2 and is documented at `drive.rs:207-209`. That is
why this is P3 and not P2.

**Fix (cheapest):** in the two alloc arms, keep the pre-clamp value and name
it in the null message when clamping occurred — e.g. `"M1: op #N
alloc(size=…, align=…, clamped from requested …) returned null — note the
harness does not model OOM"`. Alternatively add one sentence to `drive`'s
`# Panics` section stating that a hand-built size the allocator cannot serve
is reported as M1 by design.

### P3-7 — the published trait doc points at its own preconditions via a comment docs.rs never renders

*Axis: улучшение (doc-of-contract).*

`src/raw_allocator.rs:10-12` tells the reader:

> An implementor backed by a `GlobalAlloc` carries `GlobalAlloc`'s stricter
> preconditions (**see the blanket impl below**); `drive` upholds them by
> construction.

Those preconditions are spelled out — accurately and usefully — at `:79-85`:
a non-zero `Layout` size, a non-zero `realloc` `new_size`, and no `isize`
overflow after the alignment round-up. But `:79-85` is a `//` line comment,
not a doc comment, so on the rendered docs.rs page the sentence *"see the
blanket impl below"* leads to nothing: the impl block shows four forwarding
signatures and no text.

This matters because the trait is `pub` and callable. A third-party caller
doing `unsafe { RawAllocator::alloc(&System, Layout::from_size_align(0,
1).unwrap()) }` has, on the published page, no stated precondition forbidding
it — and it is UB via the blanket impl.

**Fix:** promote the three preconditions from the `//` comment at `:79-85`
into rendered rustdoc — either into the trait-level `# Safety` section as a
"callers of the blanket impl must additionally ensure …" clause, or into the
per-method `# Safety` sections at `:56-57`, `:62-63`, `:74-75`.

### P3-8 — `system_proptest.rs`'s hermetic-run comment is right, but nothing pins the seeding the whole P2-1 fix exists to restore

*Axis: улучшение (missing coverage of a property that has already regressed once).*

`tests/system_proptest.rs:18-27` is the exact test round 2 found running on a
constant seed for an entire round. The fix is a manifest entry
(`Cargo.toml:60`) — nothing in the test tree observes it. If a future change
re-narrows the dev-dependency (or a consumer copies the `[dependencies]`
stanza as a template), the test silently reverts to a single fixed seed and
every gate stays green, exactly as before.

The round-2 commit did verify this empirically ("three separate process runs
… produce three different random Ops") but with *a throwaway probe*, which
is precisely the shape this repo's CLAUDE.md raw-log/evidence rules exist to
discourage: the verification is not reproducible from the commit.

**Fix:** one cheap test that makes the property self-checking, e.g. build
`op_strategy(Config::default(), 8..9)` twice through two independently seeded
`TestRunner`s and assert the two generated `Vec<Op>`s are not identical
(`Op` now derives `PartialEq`, so this is a one-liner). Flaky-by-construction
at 1-in-astronomical odds given the size/align space; if that is not
acceptable, assert instead that `proptest`'s `std` feature is on via a
`#[cfg]`-visible signal.

### P3-9 — the fuzz target lost libFuzzer's structured crash rendering when it switched to `|data: &[u8]|`

*Axis: улучшение (a debuggability regression from round 2's P2-2 fix).*

`fuzz/fuzz_targets/global_alloc_ops.rs:85`. The target changed from
`fuzz_target!(|stream: OpStream| …)` to `fuzz_target!(|data: &[u8]| …)` in
order to pass an explicit `Config`. That is the right *outcome* but it gives
up a real facility: for the `Arbitrary` form, `libfuzzer-sys` renders the
**decoded** input via `Debug` into `RUST_LIBFUZZER_DEBUG_PATH` on a crash,
and `cargo fuzz fmt` prints it. `OpStream` derives `Debug` (and, since round
2, `Clone`) precisely so that works. With `&[u8]`, a crash report is now raw
hex bytes and a reproducer must be re-decoded by hand to see which
`Op` sequence broke the allocator.

Reproduction itself still works (`cargo fuzz run global_alloc_ops
artifact.bin`, as the module doc at `:36` says), and the corpus is unaffected
(see §0.f) — this is purely the human-facing triage step.

**Fix:** keep the explicit `Config` *and* the structured form with a local
newtype in the fuzz crate:

```rust
#[derive(Debug)]
struct ConfiguredOpStream(OpStream);
impl<'a> arbitrary::Arbitrary<'a> for ConfiguredOpStream {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        OpStream::arbitrary_with_config(u, fuzz_config()).map(Self)
    }
}
fuzz_target!(|stream: ConfiguredOpStream| { … });
```

`OpStream::arbitrary_with_config` keeps its "first real consumer" either way.

---

## P4

### New this round

1. **`CHANGELOG.md:23-25`** *(smell)* — the published changelog now states
   *"Total over every hand-built `Op` value: zero/oversized sizes are clamped
   into `GlobalAlloc`'s own contract, so the allocator is never invoked
   outside it"* and omits the caveat `drive`'s own rustdoc is careful to
   state (`drive.rs:190-192`): a never-admissible **align** is still a
   harness rejection panic. The crates.io reader gets the stronger half of
   the claim only.
2. **`src/config.rs:46-54` vs `src/arbitrary_stream.rs:20-27`** *(improvement)*
   — the two front-ends diverge on aligns above `2^21`: `align_strategy`
   (`strategy.rs:39-42`) honours `max_align` without a cap, while
   `bound_align` caps at `ALIGN_POW_CAP_EXP = 21` regardless. `CHANGELOG.md:45`
   documents this for the arbitrary side; `Config::max_align`'s own rustdoc —
   the field a consumer actually reads when choosing a value — does not, even
   though it otherwise enumerates precisely which front-end reads what
   (`config.rs:5-13`). One clause closes it.
3. **`src/arbitrary_stream.rs:20-27`** *(smell)* — `ALIGN_POW_CAP_EXP`'s
   justification (*"staying below a typical 4 MiB segment so large-align
   routing is exercised without hitting a rejected corridor"*) is a
   sefer-specific rationale — the historical target's own comment
   (`git show 33d9a2b:fuzz/fuzz_targets/global_alloc_ops.rs:93-99`) names
   `SEGMENT = 1 << 22` explicitly — baked as a non-configurable constant into
   a general-purpose published crate. A consumer whose allocator supports
   16 MiB aligns cannot fuzz them at all.
4. **`src/strategy.rs:26`** *(smell)* — `debug_assert!(config.max_align.is_power_of_two())`
   is now unreachable-by-construction: `op_strategy:61` calls
   `config.validate()`, which asserts the same thing unconditionally, before
   `align_strategy` is ever built, and `align_strategy` is private with that
   single caller. The long comment at `:34-38` explaining what happens when
   it is violated is now describing an impossible state.
5. **`src/arbitrary_stream.rs:53`** *(smell)* — same shape:
   `config.max_align.max(1)` is dead after `arbitrary_with_config:137` calls
   `validate()`, which already rejects `max_align == 0`.
6. **`src/drive.rs:48-52`** *(smell)* — `layout_for`'s `unwrap_or_else(panic)`
   is now unreachable at **all four** call sites: `:246` and `:280` are
   preceded by `validate_align` + clamp (both arms' own comments say
   "Infallible after the two steps above"), and `:312`/`:332` take
   already-validated `(size, align)` pairs out of `live`. Its doc ("naming
   the op index if the pair is rejected") describes behaviour that can no
   longer occur.
7. **`tests/system_proptest.rs:15-16` vs `tests/system_arbitrary.rs:30-38`**
   *(smell)* — round 2's P3-9 fix gave `system_arbitrary.rs` a `cfg!(miri)`
   **size** reduction (`small_max: 256, large_max: 4096`) while
   `system_proptest.rs` still reduces only `CASES`/`MAX_LEN` and keeps
   `Config::default()`'s 128 KiB `large_max` under the interpreter. The two
   siblings now bound miri cost along different axes; `system_arbitrary.rs`'s
   comment at `:23-25` describes this accurately, so it is a consistency nit,
   not a wrong claim.
8. **`Cargo.toml:45`** *(improvement)* — enabling proptest's `no_std` feature
   unconditionally forces `num-traits/libm` into every consumer's graph,
   including std ones (unification is additive; `no_std` is
   `["num-traits/libm"]` and nothing else in proptest 1.11). Harmless, but it
   is one extra crate compiled for every user of the `proptest` feature who
   is not on bare metal. A `proptest-std` alias feature would let them opt
   out.

### Carried over from round 2, still open

These were rated P4 in run 2 and were not part of the "P4 bonus items"
the round-2 commit picked up. Re-verified as still present, with line
numbers updated to `47fc214`:

9. **`src/drive.rs:400-412`** — the run-end sweep passes `block_idx` into
   `verify_block`'s `step` parameter (`:111`), so the message reads
   `M3: step #2 …` for what is a live-**block** index, not an op index. (run 2 P4-1)
10. **`src/drive.rs:416-422`** — teardown re-implements `layout_for` inline
    with a different message. (run 2 P4-2)
11. **`src/drive.rs:33`** — `#[must_use]` on `ranges_overlap` is inert; its
    only call site is an `assert!` condition. (run 2 P4-3)
12. **`src/drive.rs:80-95`** — `assert_no_overlap` computes `end`/`other_end`
    for the message while `ranges_overlap` recomputes the same two saturating
    sums. (run 2 P4-4)
13. **`tests/double_free_no_op.rs:26-29`, `:93`, `:95`** — `dealloc_count:
    Cell<usize>` duplicates `frees.borrow().len()`; both are asserted against
    the same `6`. (run 2 P4-6)
14. **`tests/system_arbitrary.rs:44`** — `(0u16..512).map(|i| (i as u8)…)`
    truncates, so the "512-byte" buffer is the same 256 bytes twice. (run 2 P4-8)
15. **`src/arbitrary_stream.rs:33-49`** — `bucket = raw % total` and
    `magnitude = raw / total` come from the same `u32`, so for the small
    integers libFuzzer favours the large arm clusters near `small_max + 1`
    and `large_max` is practically unreachable. This now matters more than it
    did in run 2: the fuzz target's whole P2-2 fix is about restoring
    `large_max` to 2 MiB. (run 2 P4-9)
16. **`src/arbitrary_stream.rs:60-88`** — `RawOp` is private yet every variant
    and field carries a `///`; `#![deny(missing_docs)]` exempts private
    items. (run 2 P4-10)
17. **`src/arbitrary_stream.rs:110-111`** — `OpStream`'s doc demotes
    `OpStream::ops` and `crate::drive` to plain code spans while the module
    header at `:2` still links `[crate::drive]`. (run 2 P4-11)
18. **`CHANGELOG.md:91-93` and `Cargo.toml:6`** — internal review identifiers
    ("review P3-23") still ship in two crates.io-rendered artifacts. The
    `src/**` occurrences were correctly moved to `//` comments
    (`drive.rs:233`, `:333`, `raw_allocator.rs:84`), so this is now partial,
    not open. (run 2 P4-14)

---

## Summary

| Severity | Count |
|---|---|
| P0 | 0 |
| P1 | 0 |
| P2 | 1 |
| P3 | 9 |
| P4 | 18 (8 new, 10 carried) |

**Not clean at P0–P3, so this cycle should run one more round.** But the gap
is narrow and the shape has changed: rounds 1 and 2 each found *silent
coverage regressions in shipped behaviour*; round 3 finds one CI-config
regression (P2-1, a one-line fix) and eight P3s that are overwhelmingly
**missing tests and doc-of-contract precision, not defects in what the crate
does**. Every round-2 fix I could re-derive independently — the clamp bounds,
the fault arithmetic, the `Config::validate()` call sites, the
feature-resolution story across all five build modes, the restored 2 MiB
fuzz reach — holds up exactly as claimed.

The pattern the prompt asked me to hunt did recur, once and in the same
place it recurred before: a fix (round 2's P3-8 miri leak-check scoping)
silently changed what CI actually runs, and its own round's verification
did not catch it because the job still passes. That is **P2-1**, and it is
the only thing in this report I would block a tag on.

Three P3s are worth doing in the same pass because they are all one small
test file each and they close the crate's only untested branches: **P3-2**
(three `drive` branches with no counterfactual, one of them soundness-
critical), **P3-3** (`Config::validate()`), and **P3-8** (a seeding property
that has already regressed once and is still unobserved by any gate).
