# globalalloc-model — independent read-only review, run 6

**Verdict: not quite clean — 0 P0, 0 P1, 0 P2, 1 P3, 28 P4; the single P3 is
that round 5's headline structural fix documents a compile-time coverage
guarantee it does not actually enforce at the trigger it names (adding a
block-creating variant to `Op`), while today's coverage is complete and
every one of the six grid cells is genuinely counterfactual.**

Reviewed at `6ff829f` (`main`), read-only. Scope as briefed: `src/lib.rs`,
`src/raw_allocator.rs`, `src/op.rs`, `src/config.rs`, `src/drive.rs`,
`src/strategy.rs`, `src/arbitrary_stream.rs`, `Cargo.toml`, `README.md`,
`CHANGELOG.md`, all six `tests/*.rs` (`oracle_negative.rs` read fresh, in
full, not from the round-5 summary), `fuzz/fuzz_targets/global_alloc_ops.rs`,
and every `globalalloc-model` section of `.github/workflows/ci.yml` and
`release.yml`. No `cargo`/build/test/lint command was run. The CI drift-check
was re-derived by executing its own shell pipeline verbatim — against the real
`ci.yml` and against sabotaged scratch copies in a temp directory (pure text
processing, no build, no repo file touched).

Trend at P0–P3: **47 → 14 → 10 → 5 → 2 → 1.**

---

## 0. Round-5 fix verification — what I re-derived independently

### (a) `Op` really does have exactly three block-creating variants — and the grid's axis is correct today

`src/op.rs:11-37` declares four variants: `Alloc`, `AllocZeroed`, `Dealloc(usize)`,
`Realloc`. `Dealloc` creates no block, so the block-creating set is exactly the
three `Arm::ALL` (`tests/oracle_negative.rs:444`) lists. The grid's axis is
**correct as of this commit**; what it is not is *forced* — see P3-1.

### (b) All six grid cells — re-derived from the code, not from the commit message

For each cell I re-derived (a) that the fault produces the claimed shape through
the claimed path, (b) that the arm's own null/align check is the FIRST thing
that can fail on that op stream, and (c) that deleting exactly that check in
`drive.rs` fails **that** cell and no other. All six hold.

| cell | how the shape is produced | first possible failure | with the check deleted | isolated? |
|---|---|---|---|---|
| `Alloc × Null` | `Faulty::alloc` → `Fault::NullAlloc => ptr::null_mut()` (`:251`), no arena contact | `drive.rs:251` null branch; `size == original_size == 32`, so the third (un-annotated) message fires — byte-identical to the cell's full-string pin | align assert passes (`0 % 8 == 0`), overlap vacuous (`live` empty), `fill_block(null, 32, ..)` writes through null → **crash** | yes — every other cell's `alloc` is honest |
| `Alloc × Misaligned` | `MisalignedBy(1)` → `at_len(1, 32)` = `base+1`; `Arena::new` pins `base` to 4096 (`:59`), so `addr % 8 == 1` | `drive.rs:278` M1/M4 align assert | overlap vacuous, fill/verify of `[1..33)` in-bounds, teardown no-op → **run completes** → cell fails on "drive COMPLETED" | yes |
| `AllocZeroed × Null` | `Faulty::alloc_zeroed` → `NullAllocZeroed => ptr::null_mut()` (`:288`) | `drive.rs:309` null branch; third message, matches the pin exactly | align passes (`0 % 8 == 0`), overlap vacuous, `verify_zeroed_block(null, 32, 0)` does `null.add(0).read()` → **crash**. Confirms the commit's `STATUS_ACCESS_VIOLATION` observation independently | yes |
| `AllocZeroed × Misaligned` | `MisalignedZeroedBy(1)` → `at_len(1, 32)` = `base+1` | `drive.rs:332` M1/M4 align assert (before `verify_zeroed_block` at `:340`) | the returned range reads as zero (`Arena::new` zero-fills at `:65` and no earlier op moved the cursor), so the zero check, fill and verify all pass → **run completes** → cell fails. Robust either way: had the arena not been pre-zeroed the panic would say `alloc_zeroed:`, which still does not contain `M1/M4:` | yes |
| `Realloc × Null` | `NullRealloc => null` for both realloc ops | `drive.rs:406` `continue`; op 0 lands at `[0..64)`, op 2 at `[64..96)` (disjoint), both survive the run-end sweep and the teardown free-walk → completes | align passes, `assert_no_overlap(.., Some(i), null, 128)` skips the only live block, `verify_prefix_block(null, 64, ..)` reads null → **crash** | yes — no other cell reaches a null realloc |
| `Realloc × Misaligned` | `ReallocAt(9)` copies `keep = 64` bytes `base → base+9` (`ptr::copy`, memmove semantics, so the self-overlap is fine) and returns `base+9`; `9 % 8 == 1` | `drive.rs:410` M1/M4 align assert — before the overlap check (`:424`) and the prefix read (`:428`) | overlap skips index `i` (the only live block); the prefix at `[9..73)` holds the copied `0x01` fill so `verify_prefix_block` passes; refill + verify pass → **run completes** → cell fails | yes — cell 5's realloc returns null and never reaches this assert |

Two supporting facts I checked rather than assumed: the three folded-in tests
(`null_alloc_panics`, `misaligned_alloc_panics`,
`null_realloc_completes_with_old_block_intact`) were deleted with their op
streams and message pins carried into the grid **verbatim** (`git show
6ff829f`), so no coverage was traded away in the fold; and every panicking cell
unwinds through `catch_unwind` with its `Faulty`/`Arena` temporary dropped
inside the statement, so the file stays leak-clean for the miri steps that do
NOT carry `-Zmiri-ignore-leaks`.

### (c) The per-pass CI drift-check — the two round-5 false negatives are genuinely closed, and the check passes today

I executed the step's own pipeline (`ci.yml:932-978`) against the real file:

```
found=4    strict=2
pass 1 listed = config_validate double_free_no_op miri_bounded oracle_negative system_arbitrary system_proptest  → diff clean
pass 2 listed = config_validate double_free_no_op miri_bounded oracle_negative system_arbitrary system_proptest  → diff clean
```

- **Round-5 scenario B (a 7th file added to one pass only): now CAUGHT** — the
  per-pass `diff` at `:965` compares each pass independently.
- **Round-5 scenario C (a run step commented out): now CAUGHT** — the `^[[:space:]]*- run:`
  anchor at `:944`/`:960` excludes `#`-prefixed lines, so a disabled step shows
  up as `found != 4` (`:945`).
- **The self-reference problem is now fail-safe rather than accidental.** Round 5
  noted the old check survived only by a "load-bearing accident of the current
  two-line formatting". Today the script's own lines still cannot match the
  anchored run-line pattern (they begin with `|`, `found=`, or `echo`), and the
  `MIRIFLAG[S]:` bracket trick at `:974` keeps the strict-count grep from
  matching its own line or the `echo` at `:976` (which has no colon after
  `MIRIFLAGS`). More importantly, if a future reformatting DID make a script line
  self-match, `found` becomes 5 and the step fails loudly instead of passing.
- **No cross-job collision.** The `awk` extractor terminates at the next
  two-space key; every line inside a `run: |` block scalar is indented ≥ 10
  columns, so another miri job's similarly-shaped step can never be pulled in.
  A 7th test file added to both passes here *and* to some other job's step is
  invisible to this check by construction.
- **Substring collisions remain impossible.** `--test[= ][A-Za-z0-9_-]+` is
  greedy and the token is delimited by whitespace, so `--test system_proptest`
  can never yield `system_prop`; a `foo` / `foo_bar` pair cannot mask each other.
  The widened class also accepts `--test=name` and hyphenated names (round 5's
  P4-7).
- **Reshaping to ≠ 2 passes fails loudly, with instructions** (`:947`). That is
  the right trade: the positional `for range in 1,2 3,4` grouping is hardcoded,
  but it announces itself rather than silently mis-grouping.

One residual false negative survives and I reproduced it on a scratch copy —
see **P4-1**.

### (d) `panic_message`'s `&*err` vs `&err` — the code is right; the comment's *mechanism* is very likely wrong

`&*err` where `err: Box<dyn Any + Send>` is `&(dyn Any + Send)` pointing at the
**payload**. Unambiguously correct, and both downcast arms are the two shapes a
panic payload actually takes.

The comment's claim (`tests/oracle_negative.rs:587-591`) that `&err` "silently
unsizes into a trait object whose concrete type is the BOX itself" does not
survive re-derivation from the coercion rules. When the target is a reference
type, rustc's coercion for `&Box<dyn Any + Send>` → `&(dyn Any + Send)` goes
through the autoderef path first: it walks `&Box<dyn Any+Send>` →
`Box<dyn Any+Send>` → `dyn Any+Send` (`Box<T: ?Sized>: Deref<Target = T>`) and
unifies at the first step whose `&referent` equals the target — that is the
two-deref step, yielding `&(dyn Any + Send)` **at the payload**. This is the
same rule the reference states plainly ("`&T` to `&U` if `T: Deref<Target = U>`").
The unsizing candidate the comment describes does exist
(`Box<dyn Any + Send>` is `'static` and `Send`, hence itself `Any`), but
`coerce_unsized` is the *fallback* taken only when no autoderef step unifies, so
it is not what would be selected. I could not compile a probe (no `cargo` in
this review), so I state this as a derivation, not an executed experiment —
but the conclusion does not affect correctness either way: the call sites pass
`&*err`, which is right under both theories. Filed as **P4-2** (comment
accuracy), not as a code finding.

### (e) Other things I re-derived and found correct

- **The `Faulty::alloc_zeroed` `if let` → `match` refactor is behaviour-preserving**
  for the pre-existing faults: `OverlapZeroedAt` returns early exactly as before,
  `NotZeroed` still scribbles `0xAA` on the honest path, and
  `fault_touch_first_block` is still called only on that honest path
  (`oracle_negative.rs:284-306`).
- **`Config::validate` accepts a single zero weight, and both front-ends still
  agree on it.** `prop_oneof!`'s two-arm form expands to `TupleUnion::new`
  (proptest 1.11 `sugar.rs:344-350`), which — unlike `Union::new_weighted`
  (`unions.rs:81-90`) — has **no** non-zero-weight assertion; `pick_weighted`
  with `[0, 1]` deterministically selects index 1, matching
  `bound_size`'s `bucket < small_weight` → large arm. No front-end divergence,
  no panic.
- **The clamp ceiling is still exactly `Layout`'s own** (`(isize::MAX/align)*align`),
  `validate_align` still runs before it so the divisor is never 0, and both
  clamp-direction message splits are now pinned in **both** alloc arms
  (`oracle_negative.rs:788-858`), closing the round-5 asymmetry.
- **The new `oversized_realloc_new_size_is_clamped_not_rejected`** (`:677-697`)
  really does exercise the direction round 5 flagged: with the realloc clamp
  removed the fake's own upper-bound precondition assert (`:320-326`) fires
  instead of `"arena exhausted"`.
- **`double_free_no_op.rs`'s count arithmetic is still exact** — 3 executed
  `Dealloc` ops × 2 + a 4th skipped on an empty model + 0 survivors at teardown
  = 6, and the three pointers are distinct because the leaky `dealloc` never
  returns anything to `System`.
- **Release wiring unchanged and still complete** (`release.yml:82`, `:98`,
  `:444-449` — the `--all-features` extra test row that `default = []` requires).

---

## P0 — none

## P1 — none

## P2 — none

---

## P3

### P3-1 — the grid's "compile error until its cell exists" guarantee does not fire at the event it names: adding a block-creating variant to `Op` forces nothing in `oracle_negative.rs`

*Axis: улучшение (a coverage-forcing gate that does not cover its own stated
trigger) + пахнущий код (three doc comments assert a compile-time guarantee that
does not hold as written).*

**Files:** `crates/globalalloc-model/tests/oracle_negative.rs:20-29` (module
doc), `:439-445` (`Arm::ALL`), `:461-469` + `:470-537` (`null_align_cell`);
`crates/globalalloc-model/src/op.rs:11-37`.

The grid's exhaustive `match (arm, shape)` is exhaustive over the **test-local
`Arm` enum**, not over `Op`. `tests/oracle_negative.rs` never matches on `Op` at
all (verified: the only exhaustive `Op` matches in the crate are
`src/drive.rs:231` and `tests/system_arbitrary.rs:51`, the latter gated on
`#![cfg(feature = "arbitrary")]`). So:

1. **Adding a fourth block-creating variant to `Op` is not a compile error
   anywhere in this file.** `src/drive.rs:231` would fail to compile until the
   new arm is handled — which is exactly the moment the maintainer must decide
   about coverage — but `oracle_negative.rs` would still compile, still pass, and
   still silently cover only three of four arms. The forcing function fires only
   if someone *also* thinks to add a variant to `Arm`, i.e. exactly the manual
   discipline the fix was written to replace. All three doc comments state the
   stronger claim: "*a fourth block-creating arm … is a compile error HERE until
   its cell is written — the coverage question is forced by the compiler instead
   of by the next review round*" (`:465-468`; same sentence at `:20-24`).
2. **A weaker second link is also unforced:** `const ALL: [Arm; 3]` (`:444`) is a
   hand-written array with a literal length. Adding an `Arm` variant *does* force
   a new cell (the match is genuinely exhaustive over `Arm × Shape` — that part
   holds), but the cell is then never replayed unless `ALL` is edited too, and
   nothing makes that a compile error. The `Arm::ALL` doc asks for it in prose
   ("Keep in sync…"), which is the prose-discipline the grid claims to have
   retired.

**Why this is P3 and not P4, and what it is not.** There is **no coverage gap
today** — all six cells exist, all six are counterfactual (§0(b)), and adding an
`Op` variant is a deliberate, reviewed, semver-breaking event under this crate's
own compatibility policy (`README.md:83-88`). The finding is that the fix
*documents* a guarantee it does not provide, and does so in the one file whose
job is to prove coverage; a maintainer who reads `:465-468` while adding
`Op::AllocAligned` has been told, in writing, that the compiler will stop them.
That is strictly worse than no claim, and it is the same "the gate does not
enforce the property its own text states" shape round 5 rated P3 for the CI
check (its P3-2).

**Suggested fix (small, mechanical, ~10 lines).** Close the `Op → Arm` link by
adding one exhaustive classifier to the test file and using it:

```rust
/// The grid arm an op belongs to (`None` = not block-creating). Exhaustive
/// over `Op`: a new variant is a compile error HERE until it is classified,
/// which is the event that must force a new grid cell.
fn arm_of(op: &Op) -> Option<Arm> {
    match op {
        Op::Alloc { .. } => Some(Arm::Alloc),
        Op::AllocZeroed { .. } => Some(Arm::AllocZeroed),
        Op::Realloc { .. } => Some(Arm::Realloc),
        Op::Dealloc(_) => None,
    }
}
```

and assert inside the grid loop that each cell's `ops` actually contains its own
arm (`case.ops.iter().filter_map(arm_of).any(|a| matches!((a, arm), ...))`) —
which closes the link *and* adds a cheap cell/stream consistency check for free.
For the `ALL` half, either give `Arm` an exhaustive `fn index(self) -> usize`
and assert `Arm::ALL.map(Arm::index) == [0, 1, 2]` (a fourth variant then forces
`ALL` to grow), or downgrade the three doc claims to what is actually true
("a fourth `Arm` variant is a compile error in `null_align_cell`; adding a
block-creating variant to `Op` obliges you to add that `Arm` variant"). The
honest wording alone would resolve the smell half of this finding; the
classifier resolves the structural half.

---

## P4

### New this round

1. **`.github/workflows/ci.yml:974-978`** *(improvement — a residual false
   negative in the new check, reproduced)*. The strict-provenance guard counts
   `MIRIFLAGS:` lines **globally** and never associates a flag line with the run
   line it belongs to. I patched a scratch copy so that BOTH multi-target steps
   carry `-Zmiri-strict-provenance` and BOTH `double_free_no_op` steps carry only
   `-Zmiri-ignore-leaks`, then ran the step's pipeline: `found=4`, both per-pass
   diffs clean, `strict=2` → **the check passes**, while `oracle_negative`,
   `miri_bounded`, `system_arbitrary`, `system_proptest` and `config_validate`
   never run under plain miri and `double_free_no_op` never runs under strict
   provenance. Single-edit drifts *are* caught (adding strict to one line → 3;
   removing it from one → 1), so this needs a compensating pair — which is why it
   is P4, not P3 — but it is the same class the guard exists for. Fix: extract
   each run line together with its following `env:` block and assert the
   *per-line* flag shape (line 2 plain, line 4 strict), instead of a global count.
2. **`tests/oracle_negative.rs:582-591`** *(smell — a long comment whose central
   claim does not survive derivation)*. See §0(d): `&err` would deref-coerce to
   the payload, not unsize to the `Box`, so the documented "coercion trap" is
   very likely not a trap at all. The code is correct either way. Fix: keep the
   two-payload-shape note (that part is real and load-bearing) and shorten the
   rest to what is checkable — "`&*err` is `&(dyn Any + Send)` at the payload" —
   rather than shipping a compile-behaviour claim the file cannot demonstrate and
   that was reconstructed from two contradictory earlier implementations.
3. **`tests/oracle_negative.rs:486`, `:503`, `:534`** *(improvement — a pin that
   does not identify which arm fired)*. Three cells pin only `"M1/M4:"`, a prefix
   shared by all three arms' align asserts (`drive.rs:281`, `:335`, `:413`).
   Sound today because each of those cells' streams has exactly one candidate op
   (I verified: the `Realloc × Misaligned` cell's op #0 is an honest, aligned
   `alloc`), but a future edit to a cell's stream could let it pass on the wrong
   arm's assert — the very confusion the grid exists to prevent. Each message
   already contains an arm-identifying tail (`alloc(`, `alloc_zeroed(`,
   `realloc(`); pin that too.
4. **`tests/oracle_negative.rs:291`** *(smell — an implicit dependency)*.
   `MisalignedZeroedBy` returns an in-arena pointer **without zeroing it**,
   unlike its honest sibling; the cell's counterfactual outcome ("with the align
   assert deleted the run completes cleanly") therefore rests on `Arena::new`'s
   pre-zeroing at `:65` and on the cell being a single op. Harmless and the
   outcome is robust either way (§0(b)), but the fault's doc says only "Length-checked
   even though `drive` panics at the align check" — one clause about relying on
   the arena's initial zero-fill would keep a future reader from moving that fault
   into a multi-op stream and quietly changing what the cell proves.
5. **`.github/workflows/ci.yml:2177`, `crates/globalalloc-model/Cargo.toml:24-26`**
   *(improvement — the docs.rs configuration is never built)*. The manifest sets
   `rustdoc-args = ["--cfg", "docsrs"]` and `src/lib.rs:95` gates
   `feature(doc_cfg)` on it, but no row in `ci.yml` (for this or any workspace
   crate) ever passes `--cfg docsrs`, so the `#[cfg_attr(docsrs, doc(cfg(...)))]`
   attributes at `lib.rs:120`/`:127` are never compiled anywhere in CI. This is
   the cfg-axis sibling of CLAUDE.md's own "doc-lint gate must run in the EXACT
   configuration that ships" rule (the feature axis is already covered here —
   `--all-features` *is* the docs.rs feature list, and the default-feature row at
   `:2183` exceeds the rule). Low probability of breaking (the nightly job
   already exists for miri), cheap to close:
   `RUSTDOCFLAGS="--cfg docsrs -D warnings" cargo +nightly doc -p globalalloc-model --all-features --no-deps`.
   Noted honestly as a repo-wide pattern, not a regression introduced here.
6. **`.github/workflows/ci.yml:959-964`** *(smell — an opaque failure mode)*. The
   per-pass `listed=$(…)` pipeline has no `|| true` and runs under `pipefail`, so
   a job shape with four valid run lines but no `--test` filters at all (e.g.
   round 5's own suggested alternative of dropping the filters and scoping
   `-Zmiri-ignore-leaks` differently) exits the step with no message — safe, but
   it would read as a mysterious CI failure rather than "this check needs
   reshaping". One `|| true` plus an explicit "no `--test` targets found in pass
   N" branch costs two lines.

### Carried from round 5 — re-verified as still present at `6ff829f`

`6ff829f` closed three of round 5's P4s: its **#3** (the realloc clamp-DOWN test
now exists, `oracle_negative.rs:677-697`), **#5** (the "its"/"their" singular is
fixed at `:11`) and **#7** (the `--test` token pattern now accepts `-` and
`=`). The other 22 remain, with the same reasoning as run 5 §P4 (numbers are
round-5's):

#1 `config.rs:34-37`/`:47-53` "guaranteed M1 null report" overstatement ·
#2 `arbitrary_stream.rs` `u32` size cap (~429 MiB) undocumented in
`Config::small_max`/`large_max` · #4 `raw_allocator.rs:49-55` scopes the
`GlobalAlloc` triple to blanket-impl callers while the crate's own **direct**
implementor asserts it · #6 `lib.rs:92`'s crate-level `#![allow(unsafe_code)]`
is inert (no `[lints]` table, `unsafe_code` is allow-by-default) ·
#8 `config_validate.rs` pins two of `validate()`'s three `max_align` clauses
(`1 << 63` untested) · #9 `ci.yml:2391` still says "its **four** tests/ files"
(there are six) · #10 `ClobberOnLaterAlloc` / `WritesDoNotStick` are one
mechanism differing only in the byte written · #11 `CHANGELOG.md` still never
mentions `Config::validate` · #12 `system_proptest.rs:20-21` shrinks
`CASES`/`MAX_LEN` under miri but not `large_max`, unlike its `system_arbitrary`
sibling · #13 `CHANGELOG.md:23-25` totality claim omits the
never-admissible-align caveat · #14 `config.rs:68-76` `max_align` doc omits the
`arbitrary` front-end's absolute `2^21` cap · #15 `ALIGN_POW_CAP_EXP`'s
sefer-specific 4 MiB rationale in a non-configurable constant ·
#16 `strategy.rs:26` unreachable `debug_assert!` · #17 `arbitrary_stream.rs:56`
dead `.max(1)` · #18 `drive.rs:48-52` `layout_for`'s panic unreachable at all
four call sites · #19 proptest's `no_std` feature forces `num-traits/libm` into
every consumer graph · #20 `drive.rs:451-462` passes a **block** index into
`verify_block`'s `step` parameter (`M3: step #0 …`, now pinned by three
`#[should_panic]` strings) · #21 teardown re-implements `layout_for` inline
(`drive.rs:468-469`) · #22 inert `#[must_use]` on `ranges_overlap` ·
#23 `assert_no_overlap`/`ranges_overlap` compute the same saturating sums twice ·
#24 `double_free_no_op.rs` `dealloc_count` duplicates `frees.len()` ·
#25 `system_arbitrary.rs:44` `(0u16..512).map(|i| (i as u8)…)` truncates — the
"512-byte" buffer is 256 bytes twice · #26 `bucket`/`magnitude` derive from the
same `u32`, clustering the large arm near `small_max + 1` · #27 `RawOp` is
private yet fully `///`-documented · #28 `OpStream`'s doc demotes links the
module header makes · #29 internal review identifiers ("review P3-23") ship in
`CHANGELOG.md:91-93` and `Cargo.toml:6`, both crates.io-rendered.

(28 P4 total: 6 new, 22 carried.)

---

## Summary

| Severity | Count |
|---|---|
| P0 | 0 |
| P1 | 0 |
| P2 | 0 |
| P3 | 1 |
| P4 | 28 (6 new, 22 carried) |

**P0–P3 trend: 47 → 14 → 10 → 5 → 2 → 1.**

For the fourth consecutive round, **nothing is wrong with what the crate
does**: no finding at any severity describes incorrect runtime behaviour, an
unsound public API, or a wrong published claim about the allocator contract. I
looked specifically for one — I re-derived the clamp ceilings against `Layout`'s
own admissibility rule, both front-ends' behaviour at every `Config` boundary I
could construct (including the zero-weight case, checked against proptest
1.11's actual `TupleUnion` code rather than assumed), the overlap oracle's
double-free-at-teardown soundness role, and every `unsafe` site's `// SAFETY:`
argument. They hold.

**Round 5's two structural bets, judged on their own terms:**

- *The exhaustive grid* is the stronger of the two and it delivered its core
  value: the `alloc_zeroed` null branch — the one branch in `drive` whose
  deletion is undefined behaviour rather than a missed report, and the arm that
  had been edited without coverage three rounds running — is now covered by a
  cell whose failure mode is a real crash, and I confirmed all six cells are
  independently counterfactual. What it does not do is fire at the trigger its
  own documentation names (**P3-1**): the compiler is forced by the *test file's*
  enum, not by `Op`. That is a ~10-line gap, and it is the last unenforced link
  in the chain the round-5 fix set out to make mechanical.
- *The per-pass CI check* closed both false negatives round 5 demonstrated — I
  reproduced both catches — and it now fails loudly rather than silently on
  every malformed shape I could construct except one compensating-pair drift
  (**P4-1**), which I did reproduce and which needs two coordinated wrong edits.

**The meta-pattern (four rounds running: a fix silently changing coverage
elsewhere) did NOT recur this round.** I diffed `6ff829f` in full: the three
folded-in tests kept their op streams and message pins verbatim, the
`alloc_zeroed` `if let` → `match` refactor is behaviour-preserving for the
pre-existing faults, and the ci.yml edit touched only the check step and its
comments — the four run lines are byte-identical to before. That is the first
round in this cycle where I can say that.

**Recommendation: the review cycle can conclude.** P3-1 is a
durability-and-honesty fix to a test file, not a defect in the crate; it does
not block the tag, and it is worth landing before it because it is small and
because the claim it corrects is the reason the next maintainer would skip the
check themselves. Nothing in this report requires re-measuring, re-testing, or a
seventh round: if P3-1 is fixed, the appropriate verification is reading the
~10-line diff, not another full pass.
