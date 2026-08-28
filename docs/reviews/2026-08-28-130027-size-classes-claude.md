# `size-classes` — independent publication review, round 4 (Claude)

**Date:** 2026-08-28 13:00:27
**Reviewer:** Claude (Opus), blind static analysis — no prior review docs read before findings were formed
**Base:** `origin/main` @ `fddee8e01ce67fb4ab8942cd7e725d348888e2da` (CI-green: `CI` + `Kani verification`)
**Mode:** read-only. No `cargo build`/`test`/`bench`/`clippy`/`doc`/`package` was run; no file under review was edited.

---

## Verdict

**GO for publication**, conditional on the two P2 documentation fixes below (both are
one-paragraph/one-line rustdoc edits; neither touches code).

No correctness defect was found in the shipped code path. I re-derived, by hand and from
scratch, the full 49-entry `SEFER` table, all five `JUMP_*` fixtures' seed classes and exact
jump-loop iteration counts, both `# Memory cost` worked examples, the two overflow boundaries
(`geo_count` 182/183 on 64-bit, 83/84 on 32-bit), the align-divisibility densities, and the
three structural proofs the implementation relies on (`seed_idx >= L - 1 ⟺ need > small_max`,
slow-path jump ≡ step-by-1 walk, `class_idx` never reaching `N`). **Every one of them matched
the code and the prose.** The arithmetic core of this crate is, at this point, unusually well
pinned; three prior rounds have visibly done their job.

What is left is documentation hygiene, and one of the two P2s is a *rendered* defect on the
crate's docs.rs landing page, which is why the verdict is conditional rather than unconditional.

**Findings: P1 = 0, P2 = 2, P3 = 4, P4 = 9.**

---

## Scope

Reviewed in full:

- `crates/size-classes/src/lib.rs` (977 lines) — every public item, all arithmetic, overflow
  behaviour, const-eval semantics, loop termination, index bounds, and all rustdoc.
- `crates/size-classes/tests/builder.rs` (1412 lines), `tests/common/mod.rs`,
  `tests/proptest_builder.rs`.
- `crates/size-classes/benches/size_classes_bench.rs`.
- `crates/size-classes/Cargo.toml`, `README.md`, `CHANGELOG.md`.
- Consumer: `src/alloc_core/size_classes.rs`, `src/alloc_core/segment_layout.rs`.

Supporting read-only inspection: root `Cargo.toml` (`[workspace]`, `[workspace.lints]`),
absence of any `clippy.toml` in the tree, `git log` on the reviewed paths (consulted only
*after* the findings below were derived, to date the `large_const_arrays` claim).

---

## Independent verification performed (negative results worth recording)

These are things I checked that turned out **correct**. Recorded so a fifth round does not
spend budget re-deriving them.

### V1 — The full `SEFER` table, re-derived by hand

Starting at 16, `next = round_up(ceil(cur * 5 / 4), 16)`, 40 geometric classes:

```
16 32 48 64 80 112 144 192 240 304 384 480 608 768 960 1200 1504 1888 2368 2960
3712 4640 5808 7264 9088 11360 14208 17760 22208 27760 34704 43392 54240 67808
84768 105968 132464 165584 206992 258752
```

Merged with `[256, 512, 1024, 2048, 4096, 6144, 8192, 12288, 16384]` → 49 entries,
`max_class = 258752`, `L = 258752/16 + 1 = 16173`. **Matches** `size2class_len`'s
`# Memory cost` figures (`lib.rs:152-163`), the README's `L = 16173` / `~16.18 KiB`
(`49 * 8 + 16173 + 12 = 16577 B = 16.19 KiB`), and `common/mod.rs`'s `SEFER_*`.

### V2 — The second `# Memory cost` example, re-derived by hand

`min_block = 8`, `growth = (3, 2)`, 24 classes, no extras:

```
8 16 24 40 64 96 144 216 328 496 744 1120 1680 2520 3784 5680 8520 12784
19176 28768 43152 64728 97096 145648
```

`max_class = 145648`, `L = 145648/8 + 1 = 18207`. **Matches** the doc's claim that a
24-class scheme out-sizes the 49-class one. The "fewer classes can mean a larger LUT"
point is genuinely true, not a plausible-sounding invention.

### V3 — All five `JUMP_*` fixtures, re-simulated from scratch

Independently computed seed index, seed block, every round-up hop and the terminal answer,
without reading `simulate_jump_loop`'s expectations first:

| fixture | (size, align) | seed idx / block | iters | result |
|---|---|---|---|---|
| `JUMP_A` | (1025, 256) | 18 / 1200 | 4 | `Some(21)` (2048) |
| `JUMP_B` | (2049, 1024) | 22 / 2368 | 3 | `Some(25)` (4096) |
| `JUMP_MULTI` | (513, 512) | 14 / 608 | 2 | `Some(17)` (1024) |
| `JUMP_DENSE` | (129, 128) | 6 / 144 | 2 | `Some(9)` (256) |
| `JUMP_NONE` | (16385, 16384) | 36 / 17760 | 10 | `None` |

All five agree with `sefer_bench_jump_rows_genuinely_exercise_the_slow_path`'s pinned
counts and with every prose comment in `tests/common/mod.rs` and the bench. `JUMP_NONE`'s
visit set is exactly `{36, 39, 41, 42, 43, 44, 45, 46, 47, 48}` — 10 of the 13 remaining
classes, skipping 37, 38, 40 — matching both files' comments verbatim. The bench's
`jump_vs_walk` caveat ("for JUMP_A the jump takes 4 iterations and a naive walk ALSO takes
4") is also correct: 18→19→20→21 is contiguous.

### V4 — Align-divisibility densities

Entries of `SEFER_TABLE` divisible by 128: `{256, 384, 512, 768, 1024, 2048, 3712, 4096,
6144, 8192, 9088, 12288, 14208, 16384, 43392}` = 15/49 = **30.6% ≈ 31%**. Divisible by 256:
10/49 = **20.4% ≈ 20%**. `JUMP_DENSE`'s "~31% vs ~20%" claim is accurate.

### V5 — The `geo_count` overflow boundaries

For large `cur` the step ratio converges to exactly 1.25 (`258752/34704 = 7.455`,
`7.455^(1/9) = 1.25003`). From `c39 = 258752`, `usize::MAX = 1.8447e19` is reached at
`c_182 ≈ 1.87e19` (overflow) with `c_181 ≈ 1.49e19` (fits) — so `geo_count = 182` is the
last that succeeds on 64-bit, matching `build_table`'s `# Panics` doc and the two
`#[cfg(target_pointer_width = "64")]` tests. Same derivation gives `c_83 ≈ 4.4e9 >
4.295e9` on 32-bit, so 83 succeeds / 84 overflows, matching the 32-bit pair.

### V6 — The index-space guard identity

`build_table` guarantees every entry is a multiple of `min_block`, so
`small_max = (L - 1) * min_block` exactly. Therefore
`seed_idx = (need - 1) >> shift >= L - 1  ⟺  need - 1 >= (L - 1) * min_block  ⟺  need > small_max`.
The guard at `lib.rs:894` is exactly equivalent to the `need > small_max` comparison it
replaced, and the comment's proof sketch is sound. The same identity makes the slow path's
`next_idx >= L - 1` guard equivalent to `next_mult > small_max` — including the
`block | (align - 1) == usize::MAX` case, where `usize::MAX >> shift >= small_max >> shift
== L - 1` forces `None` without any overflow check. Verified against
`class_for_slow_path_rejects_next_mult_landing_exactly_on_the_l_minus_1_boundary`'s
`[16, 32, 48]` fixture by hand: `class_for(48, 32)` → seed 2, `48 | 31 = 63`, `63 >> 4 = 3
== L - 1` → `None`. The test is non-vacuous, and its claim that a fault-injected `>= L`
infinite-loops against that fixture is correct (`s2c[3] = 2`, re-seeding onto itself).

### V7 — Slow-path equivalence, no-skip, and termination

- **No valid class skipped:** from a non-divisible `block = b`, any later multiple of
  `align` is `>= next_mult`, and `next_mult` is by construction the *smallest* multiple
  strictly greater than `b`; the lookup returns the smallest class `>= next_mult`.
- **Never more iterations than the walk:** `table` is strictly increasing and
  `next_mult > b`, so the jump target index is `>= i + 1`.
- **Termination:** the jump target index is strictly `> i` every iteration, and `i` is
  bounded by `N`.
- **Fit is preserved:** `i` only increases from a seed with `block >= need`.

### V8 — `class_idx` never reaches `N`, so `N <= 256` is exact, not off by one

In `build_size2class`, `need` is clamped to `small_max = table[N - 1]`, so the inner scan
always breaks at or before `class_idx = N - 1`. Hence `out[k] as u8` tops out at 255 for
`N = 256`. The assert `N <= u8::MAX as usize + 1` and its comment are correct, and
`exactly_256_classes_build_and_index_up_to_255` / `exactly_257_classes_are_rejected` pin
both sides.

### V9 — `try_class_for`'s totality claim ("never panics, for any `(size, align)`")

Holds. A power-of-two `align` is `>= 1`, so `need >= 1` and `need - 1` cannot underflow in
any profile; `seed_idx < L - 1` is checked before indexing `size2class`; `i < N` is the loop
condition, so `table[i]` is in bounds; `size2class[next_idx]` is guarded by
`next_idx >= L - 1`; the `debug_assert!` in the delegated `class_for` cannot fire because
`align` was validated first. Also `L >= 2` through the public `build` path (since
`table[0] == min_block <= small_max`), so `L - 1` never underflows.

### V10 — Operator precedence in the two mask tests

`extras[i] & mask == 0` (`lib.rs:287`) and `block & (align - 1) == 0` (`lib.rs:913`) parse
as `(x & y) == 0` in Rust (`&` binds tighter than `==`, unlike C). Correct as written.

### V11 — No hot-path speedup found (this is a negative result, not an omission)

I looked specifically for a provable win and did not find one:

- `if seed_idx >= L - 1 { return None }` lets LLVM prove `seed_idx < L`, so
  `self.size2class[seed_idx]` needs no bounds check — as the comment claims.
- The slow loop's `while i < N` is *not* redundant overhead: it doubles as `self.table[i]`'s
  bounds check. `i` provably stays in `0..=N-1`, so the trailing `None` is unreachable, but
  removing the condition would only move the check, not eliminate it.
- `align - 1` and `self.min_block_shift` are loop-invariant and will be hoisted.
- `min_block_shift` being a field rather than a const generic costs nothing at the sefer
  consumer: `static SC` has no interior mutability, so rustc emits it as an LLVM `constant`
  and the shift constant-folds.
- Replacing `1usize << self.min_block_shift` in the fast-path comparison with a stored
  `min_block` field would trade one `shl` for one extra field load — not a win, and
  `0453cc4` already removed those fields deliberately.

The one real efficiency item I did find is compile-time, in the consumer — see **P3-1**.

### V12 — Manifest / packaging sanity

`license = "MIT OR Apache-2.0"` with both `LICENSE-*` files present; `keywords` = 5 (limit
5), `categories` = 3 (limit 5), all valid slugs; `description` = 155 chars; `readme`,
`repository`, `homepage`, `documentation` all set; `rust-version = "1.88"` matches
`CHANGELOG.md`'s MSRV line. `[lints] workspace = true` is inlined by `cargo package`
(Cargo ≥ 1.74), so the published manifest is self-contained. `benches/` reaches
`tests/common/mod.rs` via `#[path]`; both directories are packaged by default, so the
published tarball's `cargo bench` still builds. No stray `.rush/`, `target/`, or editor
directories in the crate root.

### V13 — `#[deny(missing_docs)]` vs `InvalidAlign`'s positional field

`pub struct InvalidAlign(pub usize)` has an undocumented public field, but rustc's
`missing_docs` skips positional fields (`is_positional()` guard), so this is not a latent
lint failure.

### V14 — CHANGELOG ↔ API surface completeness

Every public item is described: `Params` + `Params::new`, `size2class_len`, `build_table`,
`build_size2class`, `SizeClasses` + all ten accessors + `build` + `class_for` +
`try_class_for`, `InvalidAlign` + its `Display`/`Error` impls, `is_huge`. Nothing shipped is
undocumented in the changelog, and nothing in the changelog is absent from the code.

### V15 — README ↔ mirrored-test drift guard

`readme_example_lines_appear_verbatim_in_readme_md` checks 8 declaration lines, all of which
I confirmed present verbatim in `README.md`. The example itself is valid, `L` is derived
rather than hard-pinned, and the `static`-not-`const` note matches the crate doc.

---

## P1 — Blocking correctness defects

**None.**

---

## P2 — Should be fixed before publication

### P2-1 — Published crate rustdoc cites `clippy::large_const_arrays` for a case where that lint provably cannot fire (3 sites + 1 CHANGELOG claim)

`crates/size-classes/src/lib.rs:555-559`:

```rust
/// Intended use is a `static` referenced in place (a `const` this size
/// re-materializes at every use site, duplicating the embedded tables -- see
/// `clippy::large_const_arrays`); no method needs ownership.
```

Two independent reasons the citation is inapplicable:

1. **Wrong type kind.** `large_const_arrays` matches only `ty::Array` on a `const` item's
   own type. `SizeClasses<N, L>` is a *struct* that happens to contain arrays; the lint does
   not look through struct fields. `const SC: SizeClasses<49, 16173>` is never linted.
2. **Under the threshold anyway.** The lint's `array-size-threshold` defaults to
   **512,000 bytes**, and this workspace ships **no `clippy.toml`** (verified: `find . -name
   clippy.toml` is empty). The largest array anywhere in this scheme family is
   `[u8; 114689]` (`medium-classes-wide`), ~112 KiB — 4.5× under the default threshold. Even
   for a genuine `const [u8; L]` the lint would stay silent in every feature combination.

The *advice* ("use a `static`, not a `const`") is correct and well-motivated by
`const`-rematerialization on its own. Only the lint citation is wrong — and it is a claim
about an external tool's behaviour, in rustdoc that ships to docs.rs, that a reader may act
on ("clippy will catch it for me" — it will not).

Two consumer sites repeat the same claim, one of them with an additional numeric assertion:

- `src/alloc_core/size_classes.rs:199-203` — "`SizeClassesImpl` embeds its own copy of the
  size2class table, so at the `medium-classes` size it trips the identical
  `clippy::large_const_arrays` `.rodata`-duplication lint". `SC` is a struct → reason (1);
  and 65,537 B ≪ 512,000 B → reason (2).
- `src/alloc_core/size_classes.rs:213-215` — "avoiding the `.rodata` duplication
  `clippy::large_const_arrays` flags at the `medium-classes` ~64 KiB size". `SIZE2CLASS`
  *is* a real `[u8; 65537]`, so reason (1) does not apply here — but reason (2) still does:
  65,537 < 512,000.

The root `CHANGELOG.md:3688` carries the historical version of the same claim
("`large_const_arrays` lint once the table grew ~16 → ~64 KiB"), which by the same threshold
arithmetic could not have been what actually fired.

**Suggested fix:** drop the lint name from all three shipped comments and keep the real
reason ("a `const` this size is re-materialized at every use site; a `static` is one
fixed-address copy"). The CHANGELOG is history and per this repo's own non-retroactive
convention should be left alone.

**Impact:** documentation-only; no behavioural consequence. Raised to P2 because it is in the
published API docs of a crate about to have its first release frozen, it is repeated three
times, and it is mechanically checkable.

### P2-2 — A hyphen-split word in the crate's landing-page rustdoc renders as `align- divisible` on docs.rs

`crates/size-classes/src/lib.rs:32-33`:

```
//!   extracted from: `align >= 512`). The classifier picks an align-
//!   *divisible* stride; see [`SizeClasses::class_for`]'s `# Preconditions`
```

A soft line break inside a CommonMark paragraph renders as a **space**. The published HTML
therefore reads:

> The classifier picks an align- *divisible* stride

…on the crate-level doc page — the first screen a docs.rs visitor sees, in the paragraph that
explains the crate's headline feature. This is not a source-formatting nit; it is a visible
defect in the rendered output.

**Suggested fix:** move `align-` and `*divisible*` onto the same line (e.g. reflow the
sentence, or write `` `align`-divisible `` on one line).

The same pattern exists once in the root crate's public docs — see **P4-2**.

---

## P3 — Real issues, non-blocking

### P3-1 — The consumer's `SMALL_ALIGN_MAX` drift guard const-evaluates the entire LUT a second time, to read one field

`src/alloc_core/size_classes.rs:174-176`:

```rust
const _: () = assert!(
    SMALL_ALIGN_MAX == SizeClassesImpl::<TABLE_LEN, S2C_LEN>::build(PARAMS).small_align_max()
);
```

`build` runs `build_table` **and** `build_size2class` in full. `build_size2class` is an
`O(buckets + classes)` const-eval loop over `S2C_LEN` buckets: **16,173** in a default build,
**65,537** under `medium-classes`, **114,689** under `medium-classes-wide`. The guard needs
exactly one `u32` out of that. The `static SC` on line 205 performs the same work again, so
the crate pays for two complete LUT const-evaluations per compilation.

The comment at lines 224-228 already *acknowledges* the cost ("compile-time cost only, not
eliminated by this change") but does not note that it is trivially avoidable. The invariant
being guarded is a property of the **crate** (`build` sets `small_align_max = min_block`),
not of sefer's specific table, so a two-class scheme proves exactly the same thing:

```rust
const _: () = {
    // A 2-class probe scheme: proves the crate still derives `small_align_max`
    // from `min_block`, at ~3 const-eval buckets instead of ~65,537.
    const PROBE: Params = Params::new(MIN_BLOCK, GROWTH, 2, &[], HUGE_THRESHOLD);
    assert!(SMALL_ALIGN_MAX == SizeClassesImpl::<2, { size2class_len(MIN_BLOCK * 2, MIN_BLOCK) }>::build(PROBE).small_align_max());
};
```

(The probe still uses sefer's own `MIN_BLOCK`, so the guard keeps its stated meaning:
"`SMALL_ALIGN_MAX` is `MIN_BLOCK` because the crate says so.")

Magnitude is milliseconds, not seconds — but it is provably eliminable work in every build of
the root crate, and the current comment reads as if the cost were unavoidable.

### P3-2 — Published rustdoc is 63% comment, and the review-response prose the round-1 cleanup targeted has grown rather than shrunk

Line census of `crates/size-classes/src/lib.rs` (977 lines):

| kind | lines | share |
|---|---|---|
| `///` + `//!` doc | 490 | 50.2% |
| `//` inline comment | 126 | 12.9% |
| code | 330 | 33.8% |
| blank | 31 | 3.2% |

616 of 977 lines (**63%**) are prose. A prior round already flagged this at 62% and landed a
cleanup commit (`3449ced`, "trim review-response prose out of published rustdoc"); the ratio
is now *higher*. The residue is specifically the argumentative register — text written to
answer a reviewer, not to inform a user. Four concrete surviving instances, all in
**published** rustdoc:

1. `lib.rs:176-183` (`size2class_len`, `# Panics`) — a six-line paragraph defending a claim
   against an unnamed objection: *"This is a real, tracked rustc characteristic, not a
   misreading of the Rust Reference's more general 'overflow is a compile-time error in const
   contexts' wording: see <https://github.com/rust-lang/rust/issues/74823> …, independently
   reproduced against this crate's own MSRV and current stable toolchains."* The *fact* is
   worth one sentence and the issue link is worth keeping; the adversarial framing
   ("not a misreading of…", "independently reproduced") is the tell — it is answering a
   reviewer, in a doc a user reads.
2. `lib.rs:583-587` (`InvalidAlign`) — *"the future-proofing `#[non_exhaustive]` + accessor
   shape would only cost every caller's `Err(InvalidAlign(n))` pattern match for a
   flexibility this type has no concrete use for."* A design-decision justification; the
   user-facing fact is "it is a plain tuple struct, so you can match `Err(InvalidAlign(n))`".
3. `lib.rs:698-704` (`size2class()`) — *"this note does not promise otherwise. It exists so a
   caller doesn't read 'flat LUT' as an unstated permanent guarantee"*. The doc explaining
   why the doc exists.
4. `lib.rs:941-946` (`try_class_for`) — *"A separate function, not a wrapper `class_for` calls
   into -- so `class_for`'s own codegen is unaffected by whether a caller uses this one"*.
   An implementation-strategy note in a doc whose reader cannot observe the difference.

None of this is *wrong*. It is the specific failure mode the task brief calls "review-response
prose baked into shipped comments/docs", and a first release is the last cheap moment to trim
it — after 0.1.0 the docs are what people quote.

### P3-3 — `try_class_for`'s doc points at "the measured difference" that does not exist

`lib.rs:944-946`:

> …at the cost of `try_class_for` itself doing strictly more work (the added power-of-two
> check; see `benches/size_classes_bench.rs`'s `try_class_for/*` rows for the measured
> difference).

The bench rows exist (`try_class_for/small_hit`, `try_class_for/invalid_align_reject`,
alongside `class_for/small_hit` with an identical input — good design, correctly paired). But
a bench *harness* is not a *measured difference*: no number is published in the README, the
CHANGELOG, the rustdoc, or any committed artifact. And a docs.rs reader cannot open
`benches/` at all — the sentence sends them somewhere they cannot go, for something that is
not there when they arrive.

**Suggested fix:** either state the measured number inline (with the machine/profile it was
measured on), or reword to "…see the `try_class_for/*` rows in this crate's bench if you want
to measure it on your own target".

### P3-4 — `build_table`'s `# Panics` overstates which `geo_count` range exercises the u128 widening

`lib.rs:240-242`:

> `geo_count` up to `182` is exactly the widened-arithmetic case this crate's `CHANGELOG.md`
> describes: the next class fits even though the intermediate `cur * num` product does not fit
> `usize`.

"up to 182" reads as the whole range `1..=182`. It is not. The intermediate `cur * 5`
first exceeds `usize::MAX` only once `cur > usize::MAX / 5 ≈ 3.69e18`, which — by the same
1.25× asymptotic derivation used in **V5** — happens around `c_175`/`c_176`. So for roughly
`geo_count <= 176` the `u128` path is entirely inert (the product fits `usize` unwidened),
and only the last handful of steps are genuinely the widened case. Concretely: without the
widening the boundary would move from 182 to roughly 177 — a real but narrow band, not the
whole range.

**Suggested fix:** "at the top of that range (roughly the last half-dozen steps) the next
class fits even though the intermediate `cur * num` product does not". The
`representable_next_class_survives_an_unrepresentable_intermediate_product` test already pins
the mechanism correctly on a purpose-built scheme; only this prose overstates its reach.

---

## P4 — Minor / cosmetic / recorded

### P4-1 — The drift guard's stated compile-error rationale is contradicted by working code in the same crate

`src/alloc_core/size_classes.rs:169-171`:

> …not read off the built scheme (a `const` item cannot reference the `SC` `static`, E0013 --
> so this re-runs `build` fresh instead).

Since Rust **1.83** (`const_refs_to_static`), a `const` *can* reference a `static`. What it
still cannot do is *read through* that reference. The counterexample is 70 lines away in the
same crate: `src/alloc_core/segment_layout.rs:75` is
`pub const SIZE2CLASS: &'static [u8] = &super::size_classes::SIZE2CLASS;` — a `const`
referencing a `static`, compiling green today.

The guard's *conclusion* is still right (reading `SC.small_align_max()` requires a read, which
is rejected), but the stated reason is the pre-1.83 formulation and is now falsified by the
crate's own code. Reword to "…a `const` cannot *read through* a reference to the `SC`
`static`". (Interacts with **P3-1**: if the guard shrinks to a probe scheme, this whole
comment can shrink with it.)

### P4-2 — Second hyphen-split rendering defect, in the root crate's public rustdoc

`src/alloc_core/segment_layout.rs:137-139`:

```
/// the **TIGHT** metadata boundary (4 KiB aligned); the decommit/recommit-
/// safe boundary is
/// [`primordial_decommit_start`](Self::primordial_decommit_start).
```

Renders as "the decommit/recommit- safe boundary". Same class as **P2-2**, but on
`SegmentLayout::PRIMORDIAL_META_END` rather than a landing page, and in the root crate rather
than the one being published — hence P4. (Note the sibling at line 127-128 is fine; only this
one wraps mid-hyphenation.)

### P4-3 — The M4 invariant's parenthetical argument does not extend to `medium-classes-wide`

`src/alloc_core/size_classes.rs:62-64`:

> Every `align` this scheme ever serves divides both `block_size` (the crate's own stride
> guarantee) and `SEGMENT` (16 KiB / 1 MiB largest served class both divide 4 MiB)

The same module doc describes three feature configurations three bullets earlier, and the
third one — `medium-classes-wide` — has `SMALL_MAX = 1.75 MiB`, which does **not** divide
4 MiB. The invariant itself still holds, because what actually matters is the largest
*power-of-two* `align` the scheme can serve (1 MiB under `medium-classes-wide`, since
`align > SMALL_MAX` is rejected and 2 MiB > 1.75 MiB), and every power of two ≤ 4 MiB divides
4 MiB trivially. But the parenthetical as written enumerates only two of the three
configurations and states the argument in terms of the largest *class*, which is the wrong
quantity and is false for the third. Restate as: "every `align` served is a power of two
`<= SMALL_MAX < SEGMENT`, hence divides `SEGMENT`".

### P4-4 — `SegmentLayout::class_for`'s panic note omits the `overflow-checks`-in-release case

`src/alloc_core/segment_layout.rs:87-88` says a non-power-of-two `align` trips "the underlying
`debug_assert!` … debug builds only". The crate's own `class_for` doc spends a full paragraph
on the case this omits: with `debug_assertions` **off** but `overflow-checks` **on** (a
separate Cargo knob a consumer can enable in release), `class_for(0, 0)` panics on
`need - 1`. `SegmentLayout` is a *public* introspection surface of the published root crate,
so an external caller can reach exactly that input. Largely mitigated by
`SegmentLayout::try_class_for` sitting immediately below with an accurate "never panics"
claim, which is why this is P4 and not higher.

### P4-5 — Ragged doc wrapping throughout `lib.rs` — the visible residue of many single-line edits

Orphaned short doc lines with no structural reason: `lib.rs:17` (`  with`),
`lib.rs:159-160` (`…on a 64-bit / target) — but a smaller \`min_block\``, with "on a 64-bit
target" appearing twice in one parenthetical), `lib.rs:7-8` (`This crate` / `packages that
trio`), `lib.rs:962` (`(checked) versus`). Rustdoc reflows these, so the *rendered* output is
unaffected (unlike **P2-2**) — but in source they are a legible signature of repeated
targeted edits without a reflow pass, and they make future diffs noisier than necessary.

### P4-6 — `tests/common/mod.rs` couples a generic helper to heavy fixtures

`tests/proptest_builder.rs` imports exactly one item — `walk_class_for`, which is generic over
`table`/`min_block` and needs nothing else — but `mod common;` compiles the whole module, so
that test binary const-evaluates and links `SEFER_TABLE` (392 B), `SEFER_SC` (~16.2 KiB) and
`SEFER_L`'s LUT it never touches. `#![allow(dead_code)]` silences the symptom. Splitting the
generic helper out (`tests/common/walk.rs`) from the SEFER fixtures would let each consumer
pay only for what it uses. Low value — the cost is one const-eval and 16 KiB of `.rodata` in
one test binary — but the module's own doc comment already explains the awkwardness at length,
which is usually the signal that the shape is wrong rather than that it needs more prose.

### P4-7 — A documented behavioural claim in `build_size2class`'s rustdoc has no test

`lib.rs:436-445` gives a concrete, checkable example: `min_block = 16`, `table = [16, 24, 32]`
→ bucket `(16, 32]` resolves to `32`, leaving `24` "monotonicity-valid but permanently
unreachable". I verified it by hand (`L = 3`, `s2c = [0, 2, 2]`, index 1 never selected). It
is correct — and it is essentially the only documented behaviour in this crate that nothing
pins, in a suite that otherwise pins ten separate `# Panics` messages by exact substring. A
six-line test would close it. (Note also that the doc says `24` is merely "unreachable";
it is additionally *suboptimal* — for `size` in `17..=24` the LUT answers `32` where `24`
fits — which the phrasing understates.)

### P4-8 — README omits MSRV; the README drift guard omits the `use` line

`README.md` states no MSRV, though `CHANGELOG.md` has an MSRV section and `Cargo.toml` sets
`rust-version = "1.88"` (crates.io renders the latter, so this is cosmetic). Separately,
`readme_example_lines_appear_verbatim_in_readme_md` pins 8 declaration lines but not the
example's `use size_classes::{build_table, size2class_len, Params, SizeClasses};` line — the
one line most likely to rot if an item is ever renamed.

### P4-9 — `CHANGELOG.md` is still `## 0.1.0 - Unreleased`

Fourth consecutive round flagging this. It is a release-commit checklist item, not a code
defect, and is correctly left alone until the publish commit — recorded only so the checklist
does not lose it.

---

## Summary table

| ID | Priority | File | Subject |
|---|---|---|---|
| P2-1 | P2 | `crates/size-classes/src/lib.rs:558` (+2 consumer sites) | `clippy::large_const_arrays` citation cannot apply (wrong type kind; 4.5× under the 512,000 B default threshold) |
| P2-2 | P2 | `crates/size-classes/src/lib.rs:32-33` | `align-` / `*divisible*` line split renders as `align- divisible` on the docs.rs landing page |
| P3-1 | P3 | `src/alloc_core/size_classes.rs:174-176` | Drift guard const-evaluates the full 16 K/64 K/112 K-bucket LUT a second time to read one `u32` |
| P3-2 | P3 | `crates/size-classes/src/lib.rs` (4 passages) | Published rustdoc is 63% prose; review-response register survived (and grew past) the round-1 cleanup |
| P3-3 | P3 | `crates/size-classes/src/lib.rs:944-946` | "see … for the measured difference" — no measurement is published anywhere |
| P3-4 | P3 | `crates/size-classes/src/lib.rs:240-242` | "`geo_count` up to 182 is exactly the widened-arithmetic case" overstates the range (real band ≈ 177..182) |
| P4-1 | P4 | `src/alloc_core/size_classes.rs:169-171` | "a `const` cannot reference a `static`, E0013" — falsified by `segment_layout.rs:75` since Rust 1.83 |
| P4-2 | P4 | `src/alloc_core/segment_layout.rs:137-138` | Second hyphen-split rendering defect (`decommit/recommit- safe`) |
| P4-3 | P4 | `src/alloc_core/size_classes.rs:62-64` | M4 parenthetical omits `medium-classes-wide`, whose largest class does not divide `SEGMENT` |
| P4-4 | P4 | `src/alloc_core/segment_layout.rs:87-88` | "debug builds only" omits the `overflow-checks`-on release panic |
| P4-5 | P4 | `crates/size-classes/src/lib.rs` | Ragged doc wrapping / orphan lines from repeated targeted edits |
| P4-6 | P4 | `crates/size-classes/tests/common/mod.rs` | Generic helper coupled to heavy SEFER fixtures; every consumer pays for both |
| P4-7 | P4 | `crates/size-classes/src/lib.rs:436-445` | Documented `[16, 24, 32]` unreachable-entry behaviour has no test |
| P4-8 | P4 | `crates/size-classes/README.md`, `tests/builder.rs:1390` | No MSRV in README; drift guard omits the `use` line |
| P4-9 | P4 | `crates/size-classes/CHANGELOG.md:7` | Still `Unreleased` (release-checklist item, no action) |
