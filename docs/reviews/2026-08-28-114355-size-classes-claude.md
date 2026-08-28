# size-classes: pre-publication review, round 2 (Claude, independent blind read)

## Verdict

**GO for publication**, conditional on stamping the `CHANGELOG.md` release date
(P4-3). The library source in `crates/size-classes/src/lib.rs` is arithmetically
sound; I specifically re-derived the recently-rewritten index-space guard from
scratch and it is **correct, exactly equivalent to the check it replaced, and a
genuine improvement** — but the same two commits that landed it left three
shipped comments (in `tests/`, `benches/`, and `README.md`) asserting the
*pre-rewrite* behaviour they were supposed to update, and one benchmark fixture
comment states a block size and a table-walk shape that the table does not have.
All of that ships inside the crates.io tarball.

Checked HEAD: `4eb86c39044ef2c863278d0cbccbf15cbcf209b8`.

Review mode: read-only static analysis, single reviewer, no sub-agents. Per the
request I ran **no** `cargo build` / `test` / `bench` / `clippy` / `doc` /
`package`, and edited nothing. Every numeric claim below was re-derived by hand:
the 40-entry geometric run and the full 49-entry merged `SEFER_TABLE` were
reconstructed entry by entry, the jump loop was hand-simulated for all five
`JUMP_*` fixtures, and the `small_max == (L - 1) * min_block` identity was
proved by substitution rather than spot-checked. I did **not** open any file
under `docs/reviews/` before forming these findings (only the immediately
preceding round's file, and only after writing them, to match its structural
format); prior-round references embedded in current source comments were treated
as ordinary text under review, never as precedent.

## Scope

- `crates/size-classes/src/lib.rs` — all public items, arithmetic, const-eval
  semantics, overflow behaviour, termination, index bounds, all rustdoc;
- `crates/size-classes/tests/builder.rs`, `tests/common/mod.rs`,
  `tests/proptest_builder.rs`;
- `crates/size-classes/benches/size_classes_bench.rs`;
- `crates/size-classes/Cargo.toml`, `README.md`, `CHANGELOG.md`;
- consumer check of `src/alloc_core/size_classes.rs` and
  `src/alloc_core/segment_layout.rs` in the root crate.

---

## Independent verification performed (no findings — recorded so the negative result is citable)

The things I tried hardest to break and could not:

### 1. The `seed_idx >= L - 1` guard — re-derived from scratch, as requested

`lib.rs:889-892`:

```rust
let seed_idx = (need - 1) >> self.min_block_shift;
if seed_idx >= L - 1 { return None; }
```

The in-code comment (`lib.rs:880-888`) claims `small_max == (L - 1) * min_block`
always, and therefore `seed_idx >= L - 1  ⟺  need > small_max`. Both halves check
out:

- **`small_max == (L - 1) * min_block`.** `SizeClasses::build` sets
  `small_max = table[N - 1]` where `table` always comes from `build_table`
  (`lib.rs:632-634` — no other constructor exists, and all fields are private).
  Every `build_table` entry is a multiple of `min_block`: the geometric seed is
  `min_block`; the geometric advance is `rounded = (scaled + mask) & !mask`
  (`lib.rs:357-360`), a multiple by construction; the min-step fallback is
  `cur + min_block` (`lib.rs:372-374`); and every `extras` entry is asserted
  `extras[i] & mask == 0` (`lib.rs:287-290`). So
  `L = size2class_len(small_max, min_block) = small_max / min_block + 1` with an
  exact division ⟹ `(L - 1) * min_block == small_max`. No overflow is possible in
  that product, since `(L - 1) * min_block == small_max <= usize::MAX`.
- **Equivalence.** With `m = min_block = 2^shift`, `seed_idx = ⌊(need-1)/m⌋`.
  For integers, `⌊x/m⌋ >= q  ⟺  x >= q·m`. Substituting `q = L - 1` and
  `x = need - 1`: `seed_idx >= L-1  ⟺  need - 1 >= (L-1)·m == small_max  ⟺
  need > small_max`. Exact in both directions, including the equality boundary
  (which `class_for_slow_path_rejects_next_mult_landing_exactly_on_the_l_minus_1_boundary`
  pins with a hand-built `[16, 32, 48]` table).
- **The `need == 0` wrap case.** `need - 1` wraps to `usize::MAX`, so
  `seed_idx = usize::MAX >> shift`. Since `small_max <= usize::MAX`,
  `L - 1 = small_max >> shift <= usize::MAX >> shift = seed_idx`, so the guard
  always fires and returns `None`. The `extreme64` fixture
  (`min_block = 1 << 62`) is the exact equality boundary (`3 == 3`), which the
  suite pins. (See P3-1 for the one profile where the wrap is a trap instead.)
- **The same argument covers the slow path's `next_idx >= L - 1`**
  (`lib.rs:933-936`), with `next_mult >= 1` guaranteed by the preceding
  `checked_add(1)`.

The rewrite is also a real improvement, not a wash: `L - 1` is a compile-time
constant that directly bounds the index, so LLVM can drop the bounds check on
`self.size2class[seed_idx]`; the old `need > self.small_max` compared against a
runtime field the compiler cannot relate to `L`, leaving the array index's own
check and panic landing pad in place on top of it. **I found nothing to improve
in the guard itself** — the alternative spellings (`seed_idx + 1 >= L`,
`(align - 1) >> shift == 0` for the fast-path test) are either overflow-unsafe or
strictly more instructions.

### 2. Jump-path exactness

The slow path re-seeds through the LUT bucket *top* (`(k+1)*min_block`), not
`next_mult` itself, so it could in principle overshoot an align-divisible class
lying strictly inside a bucket. It cannot: on the slow path `align > min_block`
and both are powers of two ⟹ `min_block | align`; `next_mult` is a multiple of
`align` ⟹ a multiple of `min_block` ⟹ the bucket top *equals* `next_mult`. The
initial `seed` is covered by the same fact applied to the table entries. Hand-
simulated against all five `JUMP_*` fixtures; all five agree with an independent
smallest-fitting-and-divisible scan.

### 3. Termination and index bounds

`table[j] >= next_mult > table[i]` with a strictly increasing table ⟹ `j > i`, so
`i` advances every iteration; `i` is bounded by `N` because
`build_size2class`'s inner scan always breaks (`need <= small_max == table[N-1]`
by the clamp at `lib.rs:522-525`), so no LUT entry is ever `N`. The `while i < N`
condition is *not* dead code even though it is provably always true — it is what
gives LLVM the `i < N` fact needed to elide the `self.table[i]` bounds check, so
it should not be "simplified" away.

### 4. `try_class_for`'s totality

Traced every path: `align` is validated first, so `need >= 1`, so `need - 1`
cannot wrap; both LUT indices are guarded before use; `checked_add` covers the
`usize::MAX` round-up; the loop terminates. `Never panics` holds for every
`(size, align)` pair on a constructed `SizeClasses`. ✔

### 5. Numeric claims in the memory-cost docs

Re-derived independently: SEFER's run reaches `max_class = 258752`,
`L = 16173`, `table` = 49 × 8 = 392 B, whole object ≈ 16.2 KiB
(`lib.rs:155-163`, `README.md:43-54`) ✔. The counterexample scheme
(`min_block = 8`, `growth = (3,2)`, 24 classes) reaches `max_class = 145648`,
`L = 18207` ✔. `size2class_len`'s "reachable only for `min_block == 1` and
`max_class == usize::MAX`" is exact (for `min_block >= 2`,
`max_class / min_block <= usize::MAX / 2`) ✔. `JUMP_A`/`JUMP_B`/`JUMP_MULTI`/
`JUMP_DENSE`/`JUMP_NONE` iteration counts 4/3/2/2/10 ✔. The "~31% vs ~20%"
divisibility densities for align 128 vs 256 are 15/49 and 10/49 ✔.

### 6. Hot-path perf: I do not think there is a win left here

Explicitly checked and rejected as *not* worth doing:

- **Making `min_block` a third const generic** (so `min_block_shift` becomes an
  immediate and the field load disappears). The documented and actual usage is a
  `static` with a `const` initializer (`SizeClasses` doc `lib.rs:557-560`,
  `src/alloc_core/size_classes.rs:206`). rustc emits an immutable static with no
  interior mutability as an LLVM `constant` global, so loads from `SC` are
  already constant-foldable at every call site — the field load the const generic
  would remove is very likely already gone. (I ran no builds, so treat this as an
  argument for *not expecting a win*, not as a measurement.) The cost — a third
  const generic every consumer must derive — is certain; the benefit is not.
- **Rewriting `build_size2class`'s per-bucket loop into a per-class run-fill.**
  It would cut const-eval work maybe 2–3× and costs nothing at runtime, but the
  end-of-run index is `(table[c] >> shift) - 1`, *not* `(table[c] - 1) >> shift`
  (they differ whenever a hand-built entry is not a `min_block` multiple: for
  `table[c] = 33, min_block = 16` they are 1 and 2), and it needs an extra guard
  for `table[c] < min_block`. That is a real off-by-one trap for a const-eval-only
  win, days before a first publish. Not recommended. The cheap version of this
  win is P3-2 instead.
- **A narrower return type** (`Option<u8>`, or a `usize` sentinel) to halve the
  return-register pressure: the API cost is not worth it, and every real call
  site inlines.

The one hot-path lever I do think is worth *measuring* is P3-6.

---

## P1 — blocking

None. I found no correctness defect in `src/lib.rs`.

---

## P2 — should-fix before the release commit

### P2-1. `JUMP_NONE`'s "walks every remaining table entry" is false, in two shipped files, and the path-activation oracle structurally cannot catch it

`crates/size-classes/tests/common/mod.rs:66-69`:

> /// A slow-path case that exhausts the table and returns `None`: seed class 36
> /// (block 17760, not 16384-divisible), **10 jump-loop iterations walking every
> /// remaining table entry** (none 16384-divisible) before the table ends.

and, verbatim in substance, `crates/size-classes/benches/size_classes_bench.rs:143-146`:

> // walks 10 real iterations -- **every remaining table entry from the seed to
> // the table's last class**, none of them divisible by 16384 -- before
> // exhausting the table and returning `None`.

**The count 10 is right; the "every remaining table entry" claim is wrong.**
Hand-simulating the jump loop for `(16385, 16384)` from seed 36 gives the visited
sequence

```
36 (17760) → 39 (34704) → 41 (54240) → 42 (67808) → 43 (84768)
   → 44 (105968) → 45 (132464) → 46 (165584) → 47 (206992) → 48 (258752) → None
```

That is 10 iterations over **10 of the 13** remaining entries (indices 36..=48).
Indices **37 (22208), 38 (27760) and 40 (43392) are skipped** — from block 17760
the next multiple of 16384 is 32768, which is already past 22208 and 27760; from
34704 the next is 49152, past 43392. Skipping classes is precisely what the jump
algorithm is *for*, so the comment describes this fixture as demonstrating a
linear walk when it in fact demonstrates the opposite.

This is the same defect class the previous round's P2-1 fixed for
`multi_jump`: `sefer_bench_jump_rows_genuinely_exercise_the_slow_path`
(`tests/builder.rs:403`) pins the *iteration count* (10) and the *result*
(`None`), and both are correct — so the oracle is green and can never flag the
false part of the sentence.

**Fix:** replace "walking every remaining table entry" with the true shape, e.g.
*"10 jump-loop iterations, visiting 10 of the 13 remaining classes (indices 37,
38 and 40 are skipped by the round-up), before the table ends"* — in **both**
files. Optionally strengthen the oracle to also assert the visited index
sequence, not just its length; `simulate_jump_loop` (`tests/builder.rs:382`)
already walks it and would only need to collect the indices.

### P2-2. The bench names the wrong block size for `JUMP_DENSE`'s seed class — contradicting the shared fixture in the same repo

`crates/size-classes/benches/size_classes_bench.rs:156-158`:

> // a denser slow-path point, still genuinely exercising the jump (**seed
> // class 6, block 192**, is NOT 128-divisible; 2 iterations to `Some(9)`).

`SEFER_TABLE[6]` is **144**, not 192 (192 is index 7). The shared fixture's own
comment gets this right — `tests/common/mod.rs:71-72` says *"seed class 6 (block
144, not 128-divisible)"* — so the crate ships two comments about the same
constant that disagree with each other, and the bench's is the wrong one.

Independently re-derived: the geometric run is `16, 32, 48, 64, 80, 112, 144,
192, 240, …`, and the first extra (256) merges at index 9, so indices 0..8 are
unchanged from the geometric run. `need = max(129, 128) = 129 ⟹ seed_idx =
128 >> 4 = 8 ⟹ bucket top 144 ⟹ class 6 (block 144)`. `144 & 127 == 16 ≠ 0`,
`next_mult = 256`, → class 9 (block 256), divisible → `Some(9)`. Two iterations,
matching the pinned count.

Note this is a **regression of a fix a prior round recorded as complete** (the
in-repo task list has *"claude-review P3-3: bench comment names wrong block size
for JUMP_DENSE's seed class"* marked completed): the corrected text landed in
`tests/common/mod.rs` and the stale copy in the bench was left behind. Worth
fixing at the source of truth this time — the bench comment could simply cite
`common::JUMP_DENSE`'s own doc instead of restating the numbers.

**Fix:** `block 192` → `block 144`.

### P2-3. Three shipped sites still claim `class_for` panics on `(0, 0)` — contradicting `class_for`'s own post-rewrite `# Preconditions`

`lib.rs:828-834` now states, correctly for a default release profile:

> The `align == 0, size == 0` corner does NOT panic: `need - 1` underflows to
> `usize::MAX`, but the resulting index is always `>= L - 1` […] so it takes the
> same early `None` return as any other out-of-range request.

Three other shipped locations still describe the behaviour the index-space guard
removed:

1. `crates/size-classes/tests/builder.rs:1163-1168` —
   > // The exact corner `class_for` cannot handle safely even in release:
   > // `align == 0, size == 0` underflows `need - 1` to `usize::MAX`, **which
   > // then panics on the out-of-bounds `size2class` index (an unconditional
   > // bounds check, not a compiled-away debug_assert …)**.

   This is now flatly false: the out-of-bounds read is unreachable, precisely
   because the guard rejects every index `>= L - 1` *before* indexing. The
   sibling test `need_zero_underflow_index_never_lands_inside_the_seed_range`
   (`tests/builder.rs:1068`) proves the opposite of what this comment asserts,
   35 lines apart in the same file.

2. `crates/size-classes/benches/size_classes_bench.rs:58-59` —
   > // this is the case `class_for` cannot handle at all (**it would panic on
   > // `(0, 0)`**, see `class_for`'s own `# Preconditions`).

   Same problem, and it explicitly points the reader at the `# Preconditions`
   section that now says the opposite.

3. `crates/size-classes/README.md:31-33` — *"risking unspecified behavior (a wrong
   class choice, **or a panic for the `align == 0` corner**)"*. Weaker: in a debug
   build the `debug_assert!` does fire, so this is true-in-debug — but a reader
   comparing it against the crate doc's "does NOT panic" gets a contradiction
   with no stated profile qualifier on either side.

**Fix:** rewrite (1) and (2) to the actual current reason `try_class_for` is
preferable — it rejects `align == 0` **in every profile, before any arithmetic**,
whereas `class_for` silently returns `None` in release and trips a
`debug_assert!` in debug. For (3), add the profile qualifier ("panics in a
debug build") or drop the clause.

---

## P3 — nice-to-have

### P3-1. `class_for`'s "does NOT panic" totality claim has an unstated profile assumption

`lib.rs:828-834` asserts unconditionally that `class_for(0, 0)` does not panic,
in a paragraph whose preceding sentence establishes a release context. But
`debug-assertions` and `overflow-checks` are **independent** Cargo profile knobs:
under `[profile.release] overflow-checks = true` (a common hardening setting, and
one this crate cannot control in a consumer), the `debug_assert!` at
`lib.rs:875-878` is compiled out *and* `need - 1` at `lib.rs:889` traps with
"attempt to subtract with overflow".

So the honest statement is three-valued, not two-valued:

| profile | `class_for(0, 0)` |
|---|---|
| `debug-assertions = on` | panics (`align must be a power of two`) |
| `debug-assertions = off`, `overflow-checks = off` | returns `None` |
| `debug-assertions = off`, `overflow-checks = on` | panics (`need - 1` overflow) |

This matters because the crate elsewhere goes to real lengths to be precise about
exactly this distinction (`lib.rs:176-184`'s discussion of const-eval overflow
checks following the `overflow-checks` profile, and
`tests/builder.rs:815-818`'s note that this is "not a blanket 'always traps'
rule") — so the loose claim stands out.

**Fix:** qualify with "with overflow checks disabled" in `lib.rs:830`, and mirror
it in `CHANGELOG.md:73-74`. `try_class_for`'s own totality claim is unaffected
and stays correct: it rejects `align == 0` before the subtraction.

### P3-2. The root consumer's "only one const-evaluation of `build_size2class`" claim is false, and the second one is not free

`src/alloc_core/size_classes.rs:219-225` states:

> /// This now copies `SC`'s own table (`*SC.size2class()` …) instead of
> /// rebuilding it, **so there is only one const-evaluation of
> /// `build_size2class` in this file now.**

There are two. `src/alloc_core/size_classes.rs:175-177`:

```rust
const _: () = assert!(
    SMALL_ALIGN_MAX == SizeClassesImpl::<TABLE_LEN, S2C_LEN>::build(PARAMS).small_align_max()
);
```

`SizeClasses::build` calls `build_table` **and** `build_size2class` and
materialises the whole `[u8; S2C_LEN]` LUT in the const interpreter, then throws
it away to read a single `u32`-derived field. Const-eval does not dead-code-
eliminate that. So per compile of `alloc-core` the file const-evaluates
`build_table` three times (`SIZE_CLASS_TABLE`, `SC`, this assert) and
`build_size2class` twice — 16 173 buckets each under default features, **65 537
each under `medium-classes`**.

This is compile time only, not runtime, so it is not urgent — but the comment
asserting the opposite is exactly the kind of claim a future reader will trust
without re-checking, and it is the second half of the very fix (task #1518) it
describes.

**Fix (pick one):** (a) correct the sentence to "one const-evaluation of
`build_size2class` for the *lookup table*; the `SMALL_ALIGN_MAX` drift guard
above performs a second one"; or (b) better, move the drift guard out of
const-eval entirely — it is a two-value equality that an integration test can
assert at runtime (`assert_eq!(SegmentLayout::SMALL_ALIGN_MAX, /* SC's */
small_align_max())`) with the same guarantee and no const-eval cost.

### P3-3. The two most recent commits reintroduced internal review IDs into `src/lib.rs`, immediately after the commit that removed the last one

`3449ced` ("trim review-response prose out of published rustdoc") states in its
own body: *"Dropped the one literal review citation left in `src/`"*. The next two
commits put two back:

- `lib.rs:573-574` (`0453cc4`) — `// … storing only the shift (claude publication
  review P4-3) removes two provably-redundant hot-path field loads`
- `lib.rs:880-881` (`060fa09`) — `// Index-space guard, not `need > self.small_max`
  (claude publication review P3-1): …`

These are the *only* two matches for `review|audit|task #` in `src/lib.rs`, so
the file is one edit away from clean. "claude publication review P3-1" is
unresolvable to anyone reading the crates.io tarball; the technical content of
both comments stands without it. (`tests/builder.rs` has 43 such lines and the
bench 9 — those are less visible, but the same argument applies at release time.)

Relatedly, the comment ratio the previous round flagged is **unchanged**:
`src/lib.rs` is now 983 lines = 616 comment / 336 code / 31 blank = **62.7 %
comment**, versus 958 lines before the cleanup commit, which netted +4 lines. The
cleanup removed argumentation but the file grew back. Two specific passages I
would still cut:

- `lib.rs:163-164` — *"Numbers independently re-derived from this formula, not
  read off the crate's own output."* Published rustdoc; a docs.rs reader gains
  nothing from how the author derived them.
- `lib.rs:906-915` — ten lines of comment on a single `block & (align - 1) == 0`,
  of which the last five hedge about what `benches/size_classes_bench.rs`'s
  `jump_vs_walk` rows do and do not isolate. One line suffices: *"`align` is a
  power of two here, so the mask is a division-free `is_multiple_of`."*

### P3-4. `jump_vs_walk`'s two bench rows are confounded — the delta is not "jump vs walk"

`benches/size_classes_bench.rs:103-104`:

> // Both rows use JUMP_A so **the only variable is the algorithm**, not the
> // (size, align) input.

The input is indeed held constant, but the two arms differ in at least four ways
besides jump-vs-step:

| | `_a_jump` (`SizeClasses::class_for`) | `_a_walk` (`common::walk_class_for`) |
|---|---|---|
| divisibility test | `block & (align - 1)` (`lib.rs:916`) | `is_multiple_of(align)` → a real `div` for a runtime divisor (`tests/common/mod.rs:105`) |
| table access | `[usize; N]` field, provable bounds | `&[usize]` slice, runtime bounds checks |
| early guard | index-space guard, no field load | `*table.last().unwrap()` + `need > small_max` per call |
| shift | field load (foldable from a `static`) | `min_block.trailing_zeros()` recomputed per call |

For `JUMP_A` the jump takes 4 iterations and the walk 4 (18→19→20→21 is a
contiguous run, so this fixture is one where the jump skips *nothing*) — so on
this particular input the row measures almost purely the *primitive* differences,
not the algorithmic one. `lib.rs:909-915` already half-acknowledges this
("measures the two algorithms as a whole"), but the bench's own comment does not.

**Fix:** either state the confounders in the bench comment, or switch the walk arm
to `JUMP_DENSE`/`JUMP_NONE` (where the jump genuinely skips classes) and make the
walk arm use the same mask primitive, so the remaining variable really is the
jump-ahead.

### P3-5. The two hot-path commits are tagged `perf(opt-in)` on a false premise; per this repo's own taxonomy they are `perf(runtime)` (or `fix(perf)`)

`060fa09`'s message justifies its prefix with:

> tagged perf(opt-in) […] **this crate is not yet a dependency of sefer-alloc's
> own production feature**

That is not true. In the root `Cargo.toml`:

```
production   = ["alloc-global", …]        (line 419)
alloc-global = ["alloc-core", …]          (line 191)
alloc-core   = ["std", …, "dep:size-classes"]  (line 160)
```

`size-classes` is pulled in by `alloc-core`, which `production` reaches
transitively, and `SizeClasses::class_for` is the small-allocation classification
hot path of a default `--features production` build. CLAUDE.md's R30-12 taxonomy
reserves `perf(opt-in)` for code "a user has to opt in to reach"; a hot-path
algorithm change that stays in `production`'s always-on scope is `perf(runtime)`.
And since the same commit says *"Not claiming a measured speedup"* (the A/B was
correctly discarded as machine-noise), `fix(perf)` — "shipping or opt-in code
changed to restore a documented invariant […] but NO speedup is measured or
claimed" — is arguably the better slot still. `0453cc4` carries the same prefix on
the same premise.

Not a code defect, and the taxonomy is explicitly non-retroactive, so I would not
amend history — but the *premise* should be corrected somewhere durable so the
next size-classes change does not inherit it.

### P3-6. `class_for` is `#[inline]` with the whole jump loop; consider outlining the slow path (unmeasured hypothesis)

`lib.rs:872-940` is one `#[inline]` function whose first ~8 instructions are the
fast path and whose remaining ~15 are a loop that `align <= min_block` callers —
i.e. essentially all of them in the root crate — never execute. Every inlined call
site pays the icache footprint for both.

The classic shape is `#[inline]` fast path + a separate, non-`#[inline]` (or
`#[cold]`) `const fn` for the jump loop, taking `(seed, align)`. It is a
mechanical, behaviour-preserving split, and this crate already has the bench
harness to gate it (`class_for/small_hit` for the fast path,
`class_for/large_align_slow_path*` + `slow_path_none` for the regression risk —
the slow path would gain a real call).

I am **not** claiming a win: it could go either way, and per this repo's own
gate rules it needs a measurement, which I did not run. Listing it because it is
the only remaining hot-path lever I believe is worth the experiment (see
§6 of the verification section for the ones I rejected).

---

## P4 — cosmetic / process

### P4-1. "fewer whenever the seed lands in a run of non-divisible classes" is false for a run of length 1

`lib.rs:808-810`: *"Provably equivalent to a step-by-1 walk, never more
iterations, **fewer whenever the seed lands in a run of non-divisible
classes**."*

If the run has length 1 — `table[i]` not divisible, `table[i+1]` divisible — then
`next_mult <= table[i+1]`, so the jump lands on exactly `i+1` and both algorithms
take 2 iterations. Strictly fewer requires the jump to skip at least one class,
i.e. a run of length ≥ 2. Change "whenever the seed lands in a run of
non-divisible classes" to "whenever the jump skips at least one class".

### P4-2. Two rustdoc passages describe code that no longer exists, and one is muddled

- `lib.rs:424` and `lib.rs:689` describe `class_for`'s guard as its *"`need >
  small_max` early rejection"*. Behaviourally accurate, textually stale after the
  index-space rewrite; a reader grepping for that expression finds nothing.
  Same for `lib.rs:924`'s *"the `> small_max` clamp below"*.
- `lib.rs:687-690`: *"it indexes by `need = max(size, align)`, which is always
  `>= 1` […] — `size` itself is never validated, **`need` just happens to always
  be in range**"*. `need` is emphatically *not* always in range; the whole point
  of the surrounding sentence is that `class_for` **rejects** out-of-range `need`.
  The parenthetical contradicts the clause it is attached to.

### P4-3. `CHANGELOG.md` is still `## 0.1.0 - Unreleased`

`crates/size-classes/CHANGELOG.md:7`. Known release-commit checklist item; noted
only so the GO verdict's one condition is explicit.

### P4-4. `class_for`'s doc carries leftover sefer-specific framing

`lib.rs:812`: *"`size` is expected `>= min_block` (**the caller's contract**)"*.
In a standalone general-purpose crate there is no such contract — that is
`sefer-alloc`'s convention, correctly documented on its own side at
`src/alloc_core/size_classes.rs:269-273` and `segment_layout.rs:78-83`. As
written a new reader cannot tell whose contract is meant, and the very next
clause tells them the real domain is `need >= 1` anyway. Suggest: *"The useful
domain is `size >= 1`; more precisely what must hold is `need = max(size, align)
>= 1`, so `size == 0` alone is fine whenever `align >= 1`. Consumers commonly
clamp `size` to `min_block` before calling; this function does not require it."*

### P4-5. "progressively larger, sparser aligns (256/1024/512/16384)" is not monotone in the order given

`benches/size_classes_bench.rs:154-155`. The listed order (JUMP_A, JUMP_B,
JUMP_MULTI, JUMP_NONE) is 256 → 1024 → 512 → 16384, which is neither
monotonically larger nor monotonically sparser (divisibility densities 20 %,
14 %, 16 %, 2 %). Sorted by align the claim holds. Either sort the list or drop
"progressively".

### P4-6. Stale `#[allow(dead_code)]` in the root consumer

`src/alloc_core/size_classes.rs:160` carries `#[allow(dead_code)] // Phase 10 (M6
decommit policy) consumes this; kept for that.` on `HUGE_THRESHOLD`, but the
constant *is* used five lines below, at line 165, as `Params::huge_threshold`'s
argument in `PARAMS`. The allow is now inert and the comment's rationale is
obsolete.

---

## Summary

| Tier | Count |
|---|---|
| P1 (blocking) | 0 |
| P2 (should-fix) | 3 |
| P3 (nice-to-have) | 6 |
| P4 (cosmetic/process) | 6 |

The library itself is in good shape: I could not find a correctness defect, the
index-space guard rewrite is sound and genuinely better than what it replaced,
and the test suite's oracles are real (independent reference builder, independent
reference classifier, cross-scheme proptests, exact iteration-count pinning). The
residual risk is concentrated in *prose*: comments in `tests/`/`benches/` that
assert mechanisms the code does not have (P2-1, P2-2, P2-3), all of which ship in
the tarball and none of which any existing oracle can fail on.
