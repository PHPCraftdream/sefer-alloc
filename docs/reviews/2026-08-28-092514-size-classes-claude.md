# size-classes: pre-publication review (Claude, independent blind read)

## Verdict

**GO for publication**, conditional on stamping the `CHANGELOG.md` release date
(P4-1) — the library source in `crates/size-classes/src/lib.rs` is arithmetically
sound and I found no correctness defect in it, but three comments in the
`benches/` + `tests/` layer make factually false claims about the very
mechanism they exist to pin (P2-1, P2-2, P3-3), and those ship inside the
crates.io tarball, so they should be fixed in the release commit.

Checked HEAD: `9a108e7a9cc6785e89ae67b1fe240f8950244671` (`2026-08-27T22:41:41+02:00`).

Review mode: read-only static analysis, single reviewer, no sub-agents. Per the
request I ran **no** `cargo build` / `test` / `bench` / `clippy` / `doc` /
`package`, and edited nothing. Every numeric claim below was re-derived by hand
from the source (the full 49-entry `SEFER_TABLE` was reconstructed
independently, entry by entry, and the jump loop was hand-simulated for each
benchmark fixture). I did **not** open any prior review under `docs/reviews/`
before forming these findings; prior-round references embedded in current
source comments were treated as ordinary text under review, never as
precedent.

## Scope

- `crates/size-classes/src/lib.rs` (all public items, arithmetic, const-eval
  semantics, overflow behavior, termination, index bounds, all rustdoc);
- `crates/size-classes/tests/builder.rs`, `tests/common/mod.rs`,
  `tests/proptest_builder.rs`;
- `crates/size-classes/benches/size_classes_bench.rs`;
- `crates/size-classes/Cargo.toml`, `README.md`, `CHANGELOG.md`;
- forwarding-only check of `src/alloc_core/size_classes.rs` and
  `src/alloc_core/segment_layout.rs` in the root crate.

Not in scope: the root crate's own concerns, CI workflow contents beyond the
one release gate cited in P4-1.

## Independent verification performed (no findings, recorded so the negative
## result is citable)

These are the things I tried hardest to break and could not:

- **Jump-path correctness.** The slow path's re-seed
  (`lib.rs:867`) looks up the bucket *top* `(k+1)*min_block`, not `next_mult`
  itself, so it could in principle overshoot an align-divisible class lying in
  `[next_mult, (k+1)*min_block)`. It cannot: `align > min_block` and both are
  powers of two ⟹ `min_block | align`; `next_mult` is a multiple of `align` ⟹
  a multiple of `min_block` ⟹ `k = next_mult/min_block - 1` ⟹ the bucket top
  *equals* `next_mult` exactly. The same argument covers the initial `seed`
  (`lib.rs:826`), because every `build_table` entry is a multiple of
  `min_block`, so no class can lie strictly inside a bucket.
- **Termination.** `table[j] >= bucket_top >= next_mult > table[i]` with a
  strictly increasing table ⟹ `j > i`; the clamp case is excluded by the
  preceding `next_mult > small_max` return. `i` therefore advances every
  iteration and is bounded by `N`.
- **`class_idx` cannot reach `N` in `build_size2class`** (`lib.rs:521-527`) —
  `need <= small_max = table[N-1]` by the clamp, so the inner scan always
  breaks; the `N <= 256` bound is therefore genuinely sufficient for the `u8`
  pin, as `lib.rs:463-467` claims.
- **u128 widening is sufficient** for every `usize <= 64` target:
  `(2^64-1)^2 + (2^64-1) < 2^128`, so neither `panic!` at `lib.rs:349` nor
  `lib.rs:354` is reachable today — matching the in-code comment at
  `lib.rs:341-346`.
- **Documented numbers re-derived and confirmed:** `L = 16173` /
  `max_class = 258752` / 392 B table / ~16.18 KiB object for the SEFER
  fixture; `L = 18207` / `max_class = 145648` for the `min_block = 8`,
  24-class, `(3,2)` counterexample (`lib.rs:163-167`, `README.md:49-51`); the
  `geo_count = 183` (64-bit) / `84` (32-bit) overflow boundaries
  (`lib.rs:226-231`) are consistent with an independent growth estimate;
  `128` divides 15/49 and `256` divides 10/49 of `SEFER_TABLE`
  (`benches/size_classes_bench.rs:44-46`).
- The two root-crate forwarders (`src/alloc_core/size_classes.rs:275-311`,
  `src/alloc_core/segment_layout.rs:92-119`) are pure delegation with no
  argument transformation, and `src/alloc_core/size_classes.rs:175-177`'s
  `const _` drift guard genuinely pins `SMALL_ALIGN_MAX == small_align_max()`.
- `bench-scale-tool 0.1.0` is a real crates.io package (`Cargo.lock:64-67`),
  so the dev-dependency does not block `cargo publish`.

## Findings by priority

### P1 — none

No `unsafe`, no FFI, no concurrency, no allocation, no `Drop`. For in-contract
inputs I could not construct an out-of-bounds access, a non-terminating loop, a
silent wrap, or a wrong class.

---

### P2-1 — the `multi_jump` benchmark row rests on a false premise, and its own oracle cannot detect that

**Where:** `crates/size-classes/benches/size_classes_bench.rs:172-178`.

**Current text:**

> ```
> // size-classes publication audit run 7 (oxx, P3-2): JUMP_A/JUMP_B above
> // each take exactly ONE jump-loop iteration before resolving -- this row
> // exercises a seed that needs a SECOND round-up-and-reseek before landing
> // on an align-divisible class.
> ```

**The problem:** hand-simulating `class_for` against the reconstructed
`SEFER_TABLE` gives the opposite ordering.

- `JUMP_A = (1025, 256)`: seed = class 18 (`1200`). `1200 % 256 = 176` → jump
  to `1280` → class 19 (`1504`); `1504 % 256 = 224` → jump to `1536` → class
  20 (`1888`); `1888 % 256 = 96` → jump to `2048` → class 21 (`2048`) →
  `Some(21)`. **4 loop iterations, 3 round-up-and-reseeks.**
- `JUMP_B = (2049, 1024)`: seed = class 22 (`2368`) → `3072` → class 24
  (`3712`) → `4096` → class 25 (`4096`) → `Some(25)`. **3 loop iterations, 2
  reseeks.**
- `JUMP_MULTI = (513, 512)`: seed = class 14 (`608`) → `1024` → class 17
  (`1024`) → `Some(17)`. **2 loop iterations, 1 reseek.**

So `JUMP_A`/`JUMP_B` do *not* take "exactly ONE" iteration, and `multi_jump`
is strictly **shallower** than both rows it claims to extend — it adds no
depth coverage at all. `class_for/multi_jump` and
`class_for/large_align_slow_path` measure the same shape, with the new row
doing less work.

Why the oracle misses it: `tests/builder.rs:423-427` asserts only
`iters >= 2`, which `JUMP_MULTI` satisfies. The oracle was written to detect a
*collapse* to 0/1 iterations, not to detect that the premise about the
neighbouring rows was wrong in the first place. `JUMP_A`/`JUMP_B` themselves
are never iteration-counted anywhere
(`sefer_bench_jump_rows_genuinely_exercise_the_slow_path`,
`tests/builder.rs:345-360`, checks only that the seed is non-divisible).

**Concrete fix:** either (a) delete the false premise and re-justify the row on
its own terms (a different table *region*, which it genuinely is), or (b) pick
a fixture that actually is deeper than 4 iterations, and (c) extend the oracle
to pin the *exact* iteration count for all five fixtures (`JUMP_A` = 4,
`JUMP_B` = 3, `JUMP_MULTI` = 2, `JUMP_DENSE` = 2, `JUMP_NONE` = 10) rather than
`>= 2`. An exact-count oracle also makes the surrounding prose self-checking,
which is what this repo's own R30-8 path-activation-oracle convention is for.

---

### P2-2 — a test comment claims to be a drift guard for something it structurally cannot guard

**Where:** `crates/size-classes/tests/builder.rs:363-373`.

**Current text:**

> ```
> // ... so the constants are duplicated here deliberately -- this test
> // is the guard against the two copies silently drifting apart, exactly as
> // `sefer_bench_jump_rows_genuinely_exercise_the_slow_path` above guards
> // JUMP_A/JUMP_B.
> const JUMP_MULTI: (usize, usize) = (513, 512);
> const JUMP_NONE: (usize, usize) = (16385, 16384);
> const JUMP_DENSE: (usize, usize) = (129, 128);
> ```

**The problem:** the claim is false twice over.

1. Nothing in `sefer_bench_new_jump_rows_genuinely_exercise_the_slow_path`
   (`tests/builder.rs:376-476`) ever reads the bench's copies of these
   constants — the test only asserts properties of its own local copies.
   Editing `benches/size_classes_bench.rs:35` to `(1025, 256)` would leave the
   whole suite green while the bench row silently changed meaning. That is
   *exactly* the failure mode `tests/common/mod.rs` was created to eliminate
   (see its own module doc, `tests/common/mod.rs:1-7`), reintroduced with a
   comment asserting it has been prevented.
2. The cited precedent is not analogous: `JUMP_A`/`JUMP_B` have **one**
   definition (`tests/common/mod.rs:45-46`), shared into the bench by
   `#[path]`. There are no "two copies" there for a test to guard; the
   guarantee is structural, not test-based.

**Concrete fix:** move `JUMP_MULTI`/`JUMP_NONE`/`JUMP_DENSE` into
`tests/common/mod.rs` alongside `JUMP_A`/`JUMP_B`, delete the local copies, and
delete the paragraph. The scoping rationale it gives ("the bench-coverage task
this test belongs to is scoped to editing only the bench file") is a
process artifact that has no meaning to a reader of the published tarball and
should not survive into 0.1.0 in any case.

---

### P3-1 — the hot path carries an unelidable bounds check and a panic landing pad

**Where:** `crates/size-classes/src/lib.rs:822-826` (and the same shape at
`:864-867`).

```rust
let need = if size > align { size } else { align };
if need > self.small_max {
    return None;
}
let seed = self.size2class[(need - 1) >> self.min_block_shift] as usize;
```

**The problem:** `L` is a const generic, so `[u8; L]` indexing compares against
a compile-time constant — but `small_max` and `min_block_shift` are *runtime
fields*, so LLVM cannot prove `(need - 1) >> min_block_shift < L` from the
preceding guard. The result is a second compare + branch plus a
`panic_bounds_check` block in the crate's single hottest function, on top of
the `need > small_max` compare that already established the same fact. Three
field loads (`small_max`, `min_block_shift`, `small_align_max`) are also on the
fast path.

**Why the compiler cannot help:** `#![forbid(unsafe_code)]` rules out
`get_unchecked`, and `<[T]>::get` is not `const fn`, so it is unavailable
inside a `const fn`. The fix has to come from restructuring the guard.

**Concrete fix — move the guard into index space:**

```rust
let idx = (need - 1) >> self.min_block_shift;
if idx >= L - 1 {
    return None;
}
let seed = self.size2class[idx] as usize;
```

This is *exactly* equivalent for any `SizeClasses` (not merely conservative):
`SizeClasses::build` (`lib.rs:614-616`) always derives its table from
`build_table`, every entry of which is a multiple of `min_block`, so
`small_max = (L - 1) * min_block` and therefore
`idx >= L - 1  ⟺  need > small_max`. Verified on the SEFER fixture:
`need = 258752` → `idx = 16171 < 16172` (accepted); `need = 258753` →
`idx = 16172` (rejected). Because `L - 1` is a compile-time constant, LLVM can
then prove `idx < L - 1 < L` and drop both the bounds check and the panic
path; `small_max` also disappears from the fast path's loads. The same rewrite
applies to the slow path's `next_mult > self.small_max` guard at `:864`.

**Two things to handle if this is taken:**

- `SizeClasses::build` should gain a `const` assert that
  `small_max % min_block == 0` (trivially true today, but it is the property
  the rewrite leans on, and it is currently only implied by `build_table`).
- It changes one documented out-of-contract behavior: `class_for(0, 0)` in a
  release build currently panics via the bounds check (`lib.rs:775-779`); after
  the rewrite it returns `None` for any realistically-sized `L`. That is
  arguably better (it strengthens `try_class_for`'s totality story), but
  `lib.rs:773-779` and `CHANGELOG.md:74-77` both promise the panic and would
  need updating.

I have not measured this — the request was static-only. It should be gated
through the crate's own `class_for/small_hit` bench row before being claimed as
a win.

---

### P3-2 — no `#[inline]` anywhere in the crate

**Where:** `crates/size-classes/src/lib.rs` — `grep -c inline src/lib.rs` = 0.

Every query method is a candidate: `class_for`, `try_class_for`,
`block_size`, `is_huge`, and the seven one-load accessors
(`table`, `size2class`, `min_block`, `min_block_shift`, `small_align_max`,
`small_max`, `count`, `huge_threshold`).

**Honest caveat:** these are methods on `SizeClasses<const N, const L>`, so
their MIR *is* exported and monomorphized in the consumer's crate; cross-crate
inlining is therefore already possible without LTO, unlike a non-generic
function. So this is a hint, not a fix for a hard barrier. It still matters:
whether LLVM inlines is a cost-model decision, `-Zshare-generics` can reuse an
upstream instantiation in debug builds, and for a crate whose entire pitch is
"an O(1) hot-path lookup", a one-instruction accessor emitting a real `call`
is a bad look. `#[inline]` on the query methods costs nothing and removes the
question.

---

### P3-3 — a benchmark comment names the wrong block size for its own seed class

**Where:** `crates/size-classes/benches/size_classes_bench.rs:47-49`.

> ```
> // Seed class 6 (block 192, NOT 128-divisible) ->
> // round up to 256 -> class 9 (block 256, 128-divisible): 2 iterations,
> // `Some(9)`.
> ```

`SEFER_TABLE[6]` is **144**, not 192 (`192` is `SEFER_TABLE[7]`). Derivation:
`need = max(129, 128) = 129`; `(129 - 1) >> 4 = 8`; bucket 8's top is
`9 * 16 = 144`; the smallest class `>= 144` is `144` at index 6. The
conclusion (`2 iterations`, `Some(9)`) is correct, and 192 also happens not to
be 128-divisible, so nothing downstream is wrong — but the number is simply
false, and `tests/builder.rs:317-318` states the correct 144 for the adjacent
`(128, 128)` case, so the two files disagree.

**Fix:** `block 144`.

---

### P3-4 — a duplication is justified by a reason the same file disproves

**Where:** `crates/size-classes/benches/size_classes_bench.rs:52-60` vs
`:20-21`.

> ```
> /// Independent
> /// linear-walk twin of `tests/proptest_builder.rs`'s `walk_class_for` (bench
> /// files cannot import from `tests/`, so this is a copy, not a re-export)
> ```

Thirty lines above, the same file does exactly that:

```rust
#[path = "../tests/common/mod.rs"]
mod common;
```

So the stated reason is false, and the duplication is avoidable: the
slice-taking form already exists at `tests/proptest_builder.rs:23-49` and
could live in `tests/common/mod.rs`, `#[path]`-included by all three consumers.
As it stands the same reference walk exists twice
(`proptest_builder.rs:23`, `size_classes_bench.rs:61`) and the reference *jump*
simulation exists twice more (see P3-5) — four hand-maintained copies of the
two algorithms the crate's correctness argument rests on.

**Fix:** hoist `walk_class_for` (the slice-parameterized version) into
`tests/common/mod.rs` and delete both copies; drop the false parenthetical.

---

### P3-5 — a 16-line block is duplicated byte-for-byte inside one test

**Where:** `crates/size-classes/tests/builder.rs:407-422` and `:450-465`.

The jump-loop simulation body is identical in both places:

```rust
let mut i = seed;
let mut iters = 0usize;
let mut result = None;
while i < SEFER_TABLE.len() {
    iters += 1;
    let block = SEFER_TABLE[i];
    if block.is_multiple_of(align) { result = Some(i); break; }
    let next_mult = (block | (align - 1)) + 1;
    if next_mult > small_max { break; }
    i = SEFER_SC.size2class()[(next_mult - 1) >> shift] as usize;
}
```

The first occurrence is already inside a table-driven loop whose rows carry
`want: Option<usize>` (`:388-391`). Adding `(JUMP_NONE.0, JUMP_NONE.1, None)`
as a third row collapses the second copy entirely — the only thing the second
half adds is slightly different assertion messages. Two copies of a reference
oracle is exactly the shape that lets a future edit fix one and not the other.

---

### P3-6 — the public rustdoc contradicts itself about whether the crate has a default scheme

**Where:** `crates/size-classes/src/lib.rs:160-163` vs `:227-228`, mirrored in
`README.md:48-50`.

- `lib.rs:160-162`: "the crate's own `SEFER`-fixture **default** (`min_block =
  16`, 49 classes, `max_class = 258752`) gives `L = 16173`"
- `lib.rs:227-228`: "(this crate's own tests' example scheme; **the crate
  itself has no defaults**)"

Both are in published rustdoc, roughly 60 lines apart. The second is correct:
`SEFER_*` lives in `tests/common/mod.rs`, is `pub(crate)` to the *test* crate,
and is invisible from the rendered docs — so `lib.rs:161` additionally cites an
identifier a docs.rs reader cannot resolve, as a "default" the crate does not
have.

**Fix:** in `lib.rs:160-163` and `README.md:48-50`, replace "the crate's own
`SEFER`-fixture default" with something self-contained, e.g. "a realistic
scheme (`min_block = 16`, `growth = (5, 4)`, `geo_count = 40`, nine extras up
to 16 KiB) …". The numbers themselves are correct; only the framing is.

---

### P3-7 — `Params` is passed by reference to two entry points and by value to the third

**Where:** `lib.rs:240` (`build_table<N>(params: &Params)`), `lib.rs:454`
(`build_size2class`, takes `&[usize; N]`), `lib.rs:614`
(`SizeClasses::build(params: Params)`).

The README example has to write both forms three lines apart:

```rust
const TABLE: [usize; N] = build_table::<N>(&PARAMS);   // README.md:71
static SC: SizeClasses<N, L> = SizeClasses::build(PARAMS);  // README.md:76
```

`Params` is `Copy` (`lib.rs:85`) and holds five words, so there is no reason
for the split. Pick one — by value everywhere is simplest and removes a `&`
from the more common call — and do it before 0.1.0 freezes both signatures.

Related, same area: constructing one scheme costs the consumer four coupled
`const` items (`PARAMS`, `N`, `TABLE`, `L`) plus a `static`, and the crate doc
spends `lib.rs:52-68` explaining why. A `size_classes! { ... }` declarative
macro emitting the whole block from one `Params` expression would remove the
entire class of "I pinned `L` wrong" errors that four of the crate's own
`#[should_panic]` tests exist to catch. Not a 0.1.0 blocker — additive later —
but worth deciding now, because it would also settle the by-value/by-reference
question above.

---

### P3-8 — the FIRST-release CHANGELOG documents fixes to code that was never released, by internal ticket number

**Where:** `crates/size-classes/CHANGELOG.md:28-31`, `:42`, `:47`, `:74`.

The file opens (`:9-10`) with "First release. Everything below is new in this
version; nothing has shipped before it." It then says, of `build_table`:

> "(task #731 tightened several of these from bare division panics to named
> asserts; the publication audit's P2-1/P2-2 findings closed the two remaining
> gaps — a release-profile overflow in the length arithmetic, and the
> merged-table monotonicity check itself)"

and similarly "(task #731)" at `:42`, "(task #728)" at `:47`, "(task #729)" at
`:74`. For a reader arriving from crates.io these are unresolvable identifiers
attached to a change history that, by the file's own first line, does not
exist. They also leak a private tracker's numbering into a public artifact.

**Fix:** state the resulting *behavior* and drop the provenance — e.g. "every
precondition is a named assert, not a bare division/index panic". The internal
history is preserved in git and in `docs/reviews/`; it does not need to be in
the published release notes.

---

### P3-9 — the rustdoc is 62% comment, and several passages are review-response prose in the published API docs

**Where:** `crates/size-classes/src/lib.rs` — 566 of 919 lines are `///`/`//!`/`//`.

Individual passages I would cut or relocate:

- **`:36-43`** — the *crate-level* doc's eight-line essay defining what "zero
  cost" does and does not mean for `try_class_for`, ending in a pointer to
  `benches/size_classes_bench.rs`'s row names. This is the first screen a
  docs.rs visitor sees. The operative sentence is the last one ("Use
  `try_class_for` unless `align` is already known-valid by construction"); the
  preceding disambiguation belongs on `try_class_for` itself, which already
  says it (`:872-895`).
- **`:176-187`** — twelve lines defending the claim that const-eval overflow
  checks follow the `overflow-checks` profile, complete with a GitHub issue
  link and the phrase "not a misreading of the Rust Reference's more general
  … wording". This is a rebuttal to a reviewer preserved verbatim in a
  `# Panics` section. The *fact* is worth one sentence plus the link; the
  argument belongs in a `//` comment or a design note.
- **`:216-238`** — `build_table`'s `# Panics` is a **single sentence of ~17
  lines** with three levels of nested parentheses, mixing eight distinct panic
  conditions with a worked 64-bit-vs-32-bit overflow example. It is close to
  unparseable. A `# Panics` section should be a bulleted list of conditions;
  the overflow worked example should be its own paragraph below the list.
- **`:576-584`** — `InvalidAlign`'s doc devotes eight of twelve lines to
  justifying why it is not `#[non_exhaustive]`, closing with "Settled before
  0.1.0, same decide-now-not-in-six-months discipline as `SizeClasses`'s own
  `Copy` removal." That is meeting minutes in a public type's docs.
- **`:848-849`** — the only literal review citation left in `src/`:
  "…; MS round-3 prepublish review P2-4." Should be dropped; the surrounding
  observation stands on its own.
- **`:552-561`** — the `Copy`/`Debug` rationale is legitimate and useful, but
  says the same thing (the object is ~16 KiB, so a cheap-looking copy would be
  a trap) three separate times.

None of this is wrong. It is bulk: the crate's genuinely valuable safety
contract (`class_for`'s `# Preconditions`, `:764-815`) is buried among
justifications of decisions the reader did not ask about. A pass that keeps
every *fact* and moves every *argument* into `//` comments would cut the file
by roughly a third with no information loss.

---

### P3-10 — the LUT's shape is frozen into the public type, and it is the largest shape available

**Where:** `lib.rs:563-571` (`size2class: [u8; L]`), `:670-672`
(`size2class(&self) -> &[u8; L]`), `:189` (`size2class_len`).

`L = max_class / min_block + 1` gives one `u8` per `min_block`-sized bucket
over the *whole* size range — 16,173 bytes for the SEFER shape, i.e. ~97.6% of
the object (`table` is 392 B). The crate documents this cost carefully
(`:153-168`, `README.md:41-53`) but not the alternative: mimalloc, the
explicitly-cited model, does *not* use a flat byte-per-bucket LUT over the full
range — it uses a small direct table for the low sizes and a
`ilog2`-plus-fine-bits computation above, on the order of a hundred bytes
total.

For a `#[global_allocator]`'s hot path, 16 KiB of randomly-indexed `.rodata`
is real L1/L2 footprint. The counter-argument is good — a flat LUT is the only
shape that stays O(1) for *arbitrary* `extras`, which is the crate's whole
selling point — so I am not saying the current choice is wrong.

I am saying it should be a *recorded* decision now rather than an implicit one,
because `L` is a public const generic and `size2class() -> &[u8; L]` is public
API: a two-level or hybrid layout (exact LUT below some threshold, computed
above) cannot be introduced after 0.1.0 without a breaking release. The cheapest
hedge, if the flat LUT is kept: say explicitly in the `SizeClasses` docs that
`size2class`'s representation is not a stability guarantee.

---

### P4-1 — `CHANGELOG.md` is still marked `Unreleased`, which the repo's own release workflow rejects

`crates/size-classes/CHANGELOG.md:7` reads `## 0.1.0 - Unreleased`;
`.github/workflows/release.yml:301-306` fails any non-dry-run publish whose
matching section matches `unreleased`. Mechanical, and expected at this point
in the cycle — noted only so the release commit does not forget it.

---

### P4-2 — `size2class_len`'s overflow parenthetical is far looser than the real condition

`lib.rs:173-175`: "if `max_class / min_block + 1` overflows `usize` (only
reachable with `max_class` within `min_block` of `usize::MAX`)".

The check overflows only when `max_class / min_block == usize::MAX`, which
requires `max_class >= usize::MAX * min_block` — impossible for any
`min_block >= 2`. So the genuine condition is exactly
`min_block == 1 && max_class == usize::MAX`, which is precisely what the
regression test uses (`tests/builder.rs:836`:
`size2class_len(usize::MAX, 1)`). The current wording implies a whole family of
reachable cases (`min_block = 16`, `max_class = usize::MAX - 5`, …) that in
fact overflow nothing. Technically it states a necessary condition, so it is
not false — just misleadingly weak in a paragraph whose entire purpose is
precision.

**Fix:** "(reachable only for `min_block == 1` and `max_class == usize::MAX`;
for any `min_block >= 2` the quotient cannot reach `usize::MAX`)".

---

### P4-3 — three fields encode one number

`lib.rs:566-568`: `min_block: usize`, `min_block_shift: u32`,
`small_align_max: usize`. `build` (`:621-623`) sets `min_block` and
`small_align_max` from the same `params.min_block`, and `min_block_shift` is
its `trailing_zeros()` — so `min_block == small_align_max == 1 << min_block_shift`
unconditionally, and `class_for` loads `small_align_max` on the fast path while
`min_block` exists only to back its accessor.

Keeping `small_align_max` as a distinct *name* is defensible (the root crate's
`const _` guard at `src/alloc_core/size_classes.rs:175-177` explicitly
anticipates it becoming a real `Params` knob, and `README.md:55-57` says so).
Keeping it as a distinct *field* today is not: 16 bytes and one hot-path load
for a value that is provably equal. Both accessors can be kept while storing
only `min_block_shift`.

---

### P4-4 — item ordering in `lib.rs`

`InvalidAlign` is declared at `:586`, i.e. between the `SizeClasses` struct
(`:563`) and its `Debug` impl (`:588`); its `Display` and `Error` impls are at
`:909` and `:919`, 320 lines later, after the entire `impl SizeClasses` block.
Nothing is wrong, but the error type and its three impls should be adjacent
(and, if the file is ever split per this repo's one-file-one-export
convention, they are the obvious first module to lift out).

---

### P4-5 — the property tests build a second copy of each LUT

`tests/proptest_builder.rs:73`, `:84`, `:96`:

```rust
static A_S2C: [u8; A_L] = build_size2class::<A_N, A_L>(&A_T, A_MB);
```

`A_SC` (line 72) already contains a byte-identical array, reachable as
`A_SC.size2class()`. This is a second const-evaluation of the same derivation
and ~22 KiB of duplicate `.rodata` across the three schemes — the same
redundancy the root consumer deliberately removed
(`src/alloc_core/size_classes.rs:218-225`). Passing `A_SC.size2class()` into
`walk_class_for` removes all three statics.

---

### P4-6 — `build_size2class` documents a footgun it could cheaply reject

`lib.rs:430-441` explicitly permits a hand-built table whose entries are not
multiples of `min_block`, and gives the example `min_block = 16`,
`table = [16, 24, 32]`, where class `24` is permanently unreachable through the
generated lookup. Every *other* precondition in this crate is machine-checked
— that asymmetry is the smell, in a function carrying seven asserts.

I am filing this at P4 rather than higher because closing it has a real cost:
`tests/builder.rs:1049-1051` deliberately uses
`TABLE = [1 << 62, 2 << 62, (3 << 62) + 2, (3 << 62) + 5]` with
`MIN_BLOCK = 1 << 62` to reach the release-silent top-bucket-clamp case, and a
divisibility check would reject it. So the permissiveness is load-bearing for
the crate's own hardest regression test, and the current doc paragraph reads
like a deliberate decision. Recording it as a known, accepted footgun rather
than proposing a change.

---

### P4-7 — 919 lines and six public items in one `lib.rs`

`Params`, `size2class_len`, `build_table`, `build_size2class`, `SizeClasses`,
`InvalidAlign`. The root `CLAUDE.md`'s "single-file seam crates in `crates/`"
exception explicitly sanctions this shape, so it is **not** a rule violation —
but the exception's own rationale ("the whole crate is one module") is strained
at this size, and the house style everywhere else in the repo is
`lib.rs`-as-reexports plus one file per export. A `params.rs` / `build.rs` /
`size_classes.rs` / `error.rs` split would cost nothing at the API boundary.
Lowest priority of anything here; noting it because it is the one place the
crate diverges from the repo's primary convention.

---

### P4-8 — the CHANGELOG never states the MSRV

`Cargo.toml:5` pins `rust-version = "1.88"`. For a first release, a one-line
"MSRV: 1.88" bullet in `CHANGELOG.md` is the conventional place for it, and
makes future MSRV bumps diffable in the same file.

## Summary

| Tier | Count |
| --- | --- |
| P1 (blocking) | 0 |
| P2 (should-fix) | 2 |
| P3 (nice-to-have) | 10 |
| P4 (cosmetic/process) | 8 |

The library itself is in good shape: the jump algorithm's equivalence to a
linear walk, its termination, the `u8` class-index pin, the `u128` widening,
and every documented numeric example all survive independent re-derivation. The
weak layer is the newest benchmark/oracle scaffolding, where three comments
state things about the table that the table does not say, and the doc layer,
where roughly a third of the rustdoc is argument rather than fact.
