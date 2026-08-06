# `size-classes` — publish-readiness review (read-only)

- **Date:** 2026-08-06
- **Crate:** `size-classes` (`crates/size-classes`), local version `0.1.0`
  (`crates/size-classes/Cargo.toml:3`)
- **Measured on:** `main` @ `2a1ca35`, working tree dirty with untracked review
  docs only (no source changes under `crates/size-classes/`; `git log --oneline
  -- crates/size-classes` shows the crate untouched since `1d39e43`, 2026-07-17)
- **Toolchain:** `rustc 1.97.0 (2d8144b78 2026-07-07)`, host
  `x86_64-pc-windows-msvc`
- **Scope:** investigation only. No file under `crates/`, `src/`, `docs/perf/`
  or `.github/` was modified; no version bumped; no real `cargo publish` run;
  nothing committed. The only artifact produced is this file.

---

## Verdict: **GO-WITH-FIXES** — publish it standalone, but land the two
## precondition asserts (§5.1) first.

Short form of the recommendation, unambiguously:

> **Publish `size-classes` 0.1.0 to crates.io as a standalone crate.** It is
> the *most* defensible standalone candidate of the three unpublished crates
> named in K3 — it is a genuine cross-cutting library (every slab/pool/arena
> allocator needs this exact trio), it is `#![forbid(unsafe_code)]` + `no_std`
> + zero-dependency, it has a leaf position in the dep graph (nothing to
> sequence before it), and every build/test/lint/package/doc gate below is
> green with zero warnings on the first try. **Before publishing**, add the two
> missing `const` asserts that turn today's *silently-wrong-answer* preconditions
> on `Params::extras` into compile errors (§5.1) — both are ~8 lines, both are
> `const fn`-compatible, and both are pre-1.0 API-compatible additions that
> become much more expensive to add once third parties have baked tables.
>
> `publish = false` would be the wrong call here. Unlike more sefer-internal
> plumbing, this crate has no sefer-shaped concepts left in its surface at all
> (see §1) — and it is a **hard blocker for `sefer-alloc 0.3.0` regardless**
> (`Cargo.toml:883` declares `version = "0.1"` on the path dep, so
> `cargo publish` of the root will refuse until `size-classes 0.1.x` exists on
> the registry). Making it `publish = false` therefore also requires editing the
> root manifest to strip the `version` key — strictly more work than publishing
> a crate that is already publish-clean.

Severity ordering of the findings: **S1** (§5.1, one soundness-shaped footgun +
one silent-garbage footgun) → **S2** (§3.4, CI never builds this crate on a real
`no_std` target despite the description promising `no_std`) → **S3** (§7.2,
`Params` has no forward-compat story) → **S4/S5** (doc-accuracy nits).

---

## 1. Should this be a standalone published crate? — **Yes, genuinely.**

This is not "plumbing extracted so the workspace looks modular". Read the
module doc's own framing (`crates/size-classes/src/lib.rs:4-10`):

> "Every slab / pool / arena allocator reinvents the same trio: a table of
> block sizes, an O(1) map from a requested byte size to the smallest class
> that fits it, and a classifier that also honours alignment."

That is an accurate description of a real, recurring, cross-cutting need, and
the crate delivers all three pieces with **the table shape as a parameter**
rather than baked (`Params`, `lib.rs:51-76`). Concrete evidence the abstraction
is real and not sefer-shaped:

- **No sefer concept survives in the public API.** The one coupling that would
  have leaked — "what is huge?" — was deliberately turned into a caller-supplied
  policy parameter (`Params::huge_threshold`, `lib.rs:71-75`; module doc
  `lib.rs:29-34`: "The crate has no notion of an OS segment size"). Sefer passes
  its own `os::SEGMENT` in from the shim side
  (`src/alloc_core/size_classes.rs:145-147`, `:149-155`). The second coupling
  (`MIN_BLOCK >= node::NODE_SIZE`) is likewise pushed out to a caller-side
  `const` assert (`src/alloc_core/size_classes.rs:20-22`).
- **The generality is exercised, not merely claimed.** The crate's own tests
  instantiate three *materially different* schemes: `min_block` 16 / 8 / 64,
  growth 1.25× / 1.5× / 1.125×, extras present / absent / large-page
  (`crates/size-classes/tests/proptest_builder.rs:61-111`). Sefer's own scheme
  is only one of them.
- **The alignment classifier is the actually-valuable part.** `class_for`'s
  divisibility slow path (`lib.rs:363-384`) fixes a specific, named bug class:
  without it "every `align >= 512` request silently falls through to the
  caller's whole-segment path" (`lib.rs:25-27`, `README.md:18-20`). Anyone
  hand-rolling a slab allocator hits this and usually does not notice. That is a
  better reason to publish than the table builder itself.
- **Zero adoption friction.** No dependencies at all
  (`crates/size-classes/Cargo.toml:17-18` — empty `[dependencies]`),
  `#![forbid(unsafe_code)]` + `#![no_std]` (`lib.rs:43-44`), everything `const`
  so it costs nothing at runtime.

Contrast with its two K3 siblings: `racy-ptr-cell` and `tagged-index-stack` are
concurrency primitives whose value proposition to a stranger is "trust my
loom harness"; `size-classes` is pure integer arithmetic a reader can verify by
inspection in 20 minutes. **Of the three, this is the one worth publishing on
its merits, not just to unblock the root crate.**

The crates.io name is currently free — the API returns
`{"errors":[{"detail":"crate \`size-classes\` does not exist"}]}` (queried
2026-08-06), and a search for `size-classes` returns only unrelated crates
(e.g. `zeropool`). Note the name is *generic*; see the open questions.

---

## 2. Metadata completeness — **complete, no gaps.**

`crates/size-classes/Cargo.toml:1-15`:

| Field | Line | Status |
|---|---|---|
| `name` / `version` / `edition` | `:2-4` | present (`size-classes`, `0.1.0`, 2021) |
| `rust-version` | `:5` | `1.88`, matches the workspace floor (`Cargo.toml:5`) |
| `license` | `:6` | `MIT OR Apache-2.0`; **both files present and packaged** (`LICENSE-MIT`, `LICENSE-APACHE` — see §4) |
| `description` | `:7` | present, specific, names the bug class it fixes |
| `readme` | `:8` | `README.md`, present (52 lines) |
| `repository` | `:9` | present |
| `homepage` | `:10` | present, and correctly deep-links to `tree/main/crates/size-classes` rather than the repo root |
| `documentation` | `:11` | `https://docs.rs/size-classes` |
| `keywords` | `:12` | 5 keywords (crates.io max is 5) — all valid |
| `categories` | `:13` | 3, all valid crates.io slugs (`memory-management`, `data-structures`, `no-std::no-alloc`) |

Non-blocking observations:

- **No `authors`** — optional since edition 2018 and consistent with every
  sibling crate; not a defect.
- **No `[package.metadata.docs.rs]`** — correct, because the crate has **no
  `[features]` table at all**. `--all-features` and `--no-default-features` are
  therefore the same build. docs.rs will render the full surface by default.
- **No `[lints] workspace = true`**, unlike `crates/racy-ptr-cell/Cargo.toml:15-18`
  and `crates/tagged-index-stack/Cargo.toml:15-18`. This is *correct* here: the
  shared table only declares the `loom`/`kani` build cfgs, and this crate uses
  neither. It also keeps the published manifest free of a workspace-inherited
  key. Not a defect.
- **No `exclude` / `include`** — not needed; `cargo package --list` is already
  minimal (§4). In particular the L5 "tarball leaks internal review docs and
  local machine paths" problem is a **root-crate** problem, not this crate's.

---

## 3. Build / test / lint health, standalone — **all green, zero warnings.**

All commands run from the repo root on `2a1ca35`.

### 3.1 `cargo test -p size-classes --all-features` — **PASS (9/9)**

```
tests\builder.rs               5 passed  (3.67s)
tests\proptest_builder.rs      4 passed  (0.02s)
Doc-tests size_classes         0 tests
```

Test names: `is_huge_uses_the_policy_threshold_not_an_os_constant`,
`sefer_table_matches_reference_and_is_strictly_increasing`,
`sefer_jump_skips_non_divisible_run_for_align_128`,
`sefer_size2class_matches_scan_for_every_bucket`,
`sefer_class_for_matches_reference_over_full_small_sweep`,
`every_scheme_table_is_strictly_increasing_and_min_block_aligned`,
`scheme_{a,b,c}_jump_eq_walk_and_fidelity`.

Test quality is above average for this repo, and specifically **non-vacuous**:
the tests build an *independent* reference implementation from scratch
(`tests/builder.rs:12-51` `reference_table`, `:55-60` `reference_class_for`;
`tests/proptest_builder.rs:20-46` `walk_class_for`, `:50-55` `scan_class_for`)
and compare three ways — jump vs. step-by-1 walk vs. from-scratch scan
(`tests/proptest_builder.rs:127-133`). That is the right shape: the jump path
is the optimization, the walk is the thing it must be equivalent to, and the
scan is independent of both. Zero doctests, per CLAUDE.md's no-doctest rule
(the README example uses a ```` ```text ```` fence, `README.md:27`).

### 3.2 `cargo clippy -p size-classes --all-features --all-targets -- -D warnings` — **PASS**

`Finished dev profile ... in 22.60s`, no diagnostics emitted for
`size-classes` itself.

### 3.3 `cargo doc -p size-classes --all-features --no-deps` — **PASS, and also passes `-D missing_docs`**

Plain run: `Generated .../doc/size_classes/index.html`, no warnings. Re-run with

```
RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links"
```

also succeeds — so **every public item is documented and every intra-doc link
resolves**. See §6.

### 3.4 `no_std` — claim **holds**, but CI does not check it (**S2**)

- `cargo build -p size-classes --no-default-features` — PASS (trivially: no
  features exist).
- I additionally installed `thumbv7em-none-eabi` and ran
  `cargo build -p size-classes --no-default-features --target thumbv7em-none-eabi`
  — **PASS**. So the `#![no_std]` at `lib.rs:44` is real against a genuine
  bare-metal target, not merely "host build without std".

**S2 — the CI gap.** `.github/workflows/ci.yml:711-725`'s `no_std` job runs
exactly one command: `cargo build --no-default-features --target thumbv7em-none-eabi`
(`:725`) — that is the **root** package with default features off. `size-classes`
is reachable only via the `alloc-core` feature (`Cargo.toml:148`,
`Cargo.toml:883`), which is not in `default`. **Therefore CI has never built
`size-classes` on a `no_std` target**, even though its published `description`
(`crates/size-classes/Cargo.toml:7`) sells `no_std` as a headline property. The
MSRV job (`ci.yml:704-709`, `cargo check --all-features` on 1.88) *does* cover
the crate transitively (all-features turns on `alloc-core`), so the `1.88` floor
is enforced — but only for the library, not its tests/dev-deps.

Recommended (cheap) fix, once the crate is published: add
`cargo build -p size-classes --target thumbv7em-none-eabi` to the existing
`no_std` job. One line, same toolchain, same installed target.

---

## 4. Packaging — **clean; publishes today with no blockers.**

`cargo package -p size-classes --list` (10 files):

```
.cargo_vcs_info.json
Cargo.lock
Cargo.toml
Cargo.toml.orig
LICENSE-APACHE
LICENSE-MIT
README.md
src/lib.rs
tests/builder.rs
tests/proptest_builder.rs
```

`cargo package -p size-classes --allow-dirty` (dry validation, **no publish**):

```
Packaged 10 files, 53.6KiB (17.1KiB compressed)
Verifying size-classes v0.1.0
Compiling size-classes v0.1.0 (.../package/size-classes-0.1.0)
Finished `dev` profile ... in 1.30s
```

Notes:

- **Both license files ship.** Many crates in the wild declare
  `MIT OR Apache-2.0` and ship neither; this one ships both.
- **No leakage.** No `docs/`, no `.claude/`, no absolute machine paths. The
  generated manifest (inspected at
  `<CARGO_TARGET_DIR>/package/size-classes-0.1.0/Cargo.toml`) has an empty
  `[dependencies]` and a single `[dev-dependencies.proptest] version = "1"`
  — i.e. **no `path` key was rewritten, because there are none**. This crate is
  entirely unaffected by L2 (`aligned-vmem ^0.2` unresolvable) and by L5
  (root-tarball content leak).
- **Leaf position.** Nothing must be published before it. In the release map's
  forced ordering
  (`docs/plans/2026-08-05-release-execution-map.md:188-190`), `size-classes` sits
  in the same independent tier as `racy-ptr-cell` / `tagged-index-stack`, and
  unlike `numa-shim` it carries no upstream drift risk (L3).
- **`release.yml` has no target for it.** The workflow knows exactly five
  crates, in both the tag patterns (`.github/workflows/release.yml:51-56`) and
  the `workflow_dispatch` dropdown (`:62-70`): `aligned-vmem`, `sefer-region`,
  `malloc-bench-rs`, `numa-shim`, `sefer-alloc`. Publishing `size-classes`
  requires adding `'size-classes-v*'` to the tag list and `size-classes` to the
  dropdown — two lines. This is the K3 decision made concrete; note the
  workflow's own CHANGELOG guard (L4/K5/K8 work) will then apply to the tag, so
  check whether a per-member CHANGELOG is expected before tagging.

---

## 5. Completeness scan and precondition defects

### 5.0 Marker scan — clean

`grep -rnE "TODO|FIXME|XXX|HACK|unimplemented!|todo!|dead_code|allow\("` over
`crates/size-classes/src/` and `crates/size-classes/tests/` returns **zero
matches**. No placeholders, no `#[allow(...)]` escape hatches, no `dead_code`
suppressions anywhere in the crate. (Contrast the sefer-side shim, which does
carry two `#[allow(dead_code)]` markers —
`src/alloc_core/size_classes.rs:144`, `:216` — but those are the *consumer's*
Phase-10 placeholders, not the crate's.)

386 lines of source, 356 lines of tests. No dead public items: every `pub` item
in `lib.rs` is reached by the shim, the tests, or both.

### 5.1 **S1 — `Params::extras` preconditions are documented in prose, unchecked in code, and produce silently wrong tables**

This is the one finding I would fix before publishing, and it is specifically a
*publishing* concern: in-tree, sefer's `EXTRAS`
(`src/alloc_core/size_classes.rs:94`, `:96-112`, `:114-133`) are hand-verified
and pinned by consumer-side tests, so none of this is reachable. A third-party
consumer has neither.

`build_table` asserts exactly three things (`lib.rs:104-114`): `min_block` is a
power of two, `geo_count > 0`, and `N == geo_count + extras.len()`. The doc
comment at `lib.rs:65-69` states three *further* preconditions and explicitly
assigns them to the caller:

> "a **strictly increasing** list, each entry a multiple of `min_block`, and
> each disjoint from the geometric run (the builder sorted-merges them; the
> disjointness/increasing preconditions are the caller's, matched by a
> consumer-side test)"

None of the three is checked. Both violations below were reproduced against
this exact tree in a scratch crate outside the repo (deleted afterwards).

**(a) Non-`min_block`-multiple extras → a block that violates the crate's own
fast-path premise.** With `min_block = 16`, `extras = &[100, 200]`,
`geo_count = 8`:

```
table = [16, 32, 48, 64, 80, 100, 112, 144, 192, 200]
class_for(96, align=16) -> idx 5, block_size 100
block_size % 16 == 4
```

No `const`-eval panic; a clean compile and a wrong answer. This matters because
`class_for`'s **fast path skips the divisibility check entirely** —
`lib.rs:360-362` returns `Some(seed)` the moment `align <= small_align_max`, on
the strength of the invariant stated at `lib.rs:53-57` ("Every generated class is
a multiple of it, so every block is naturally `min_block`-aligned"). Break the
invariant via `extras` and the fast path hands back a class whose block stride
is 100 bytes, so blocks carved at that stride are **not** 16-aligned — in a
consumer allocator that is misaligned memory handed to a caller who asked for
`align = 16`, i.e. UB downstream, produced by a `#![forbid(unsafe_code)]` crate
that never says `unsafe`.

**(b) Extras overlapping the geometric run → a non-strictly-increasing table
with unreachable classes.** With `min_block = 16`, `extras = &[16, 32]`,
`geo_count = 8`:

```
table = [16, 16, 32, 32, 48, 64, 80, 112, 144, 192]
class_for(16,16) -> Some(0)   class_for(32,16) -> Some(2)   class_for(48,16) -> Some(4)
```

Indices 1 and 3 are unreachable forever. Not memory-unsafe, but it silently
breaks the "strictly increasing" guarantee `build_table`'s own doc promises
(`lib.rs:93-95`) and wastes a per-class slot in every array a consumer sizes by
`SizeClasses::count()` (`lib.rs:307-311`).

**Why this is cheap to fix.** Both are `const fn`-expressible in ~8 lines total
and cost nothing at runtime (const-eval only):

1. In `build_table`, before the merge: loop over `extras` asserting
   `extras[i] & mask == 0` and `extras[i] > extras[i-1]`.
2. After the merge (or at the top of `build_size2class`, which already has an
   assert block at `lib.rs:176-191` and is the natural chokepoint since it is
   the function whose monotone-pointer algorithm *depends* on monotonicity):
   assert `out[i] > out[i-1]` across the whole table. This single check
   subsumes both (a)-style ordering damage and (b) entirely.

Doing this at `0.1.0` rather than `0.1.1` matters: after publication, a table
that compiles today and stops compiling tomorrow is a breaking change for
someone, so the natural window for adding compile-time preconditions is
*before* the first release.

### 5.2 **S4 — two test-module docs overstate what is parameterized (doc accuracy)**

`tests/proptest_builder.rs:2-4` says the crate "lets a proptest vary
`(min_block, growth, geo_count, extras)`", and `tests/builder.rs:2-5` says
"arbitrary property-generated parameterizations". What the files actually do is
declare **three hand-picked `const` schemes** (`proptest_builder.rs:61-111`) and
proptest only `(size, align)` within each (`:123-126`, `:137-140`, `:151-154`).
That is a perfectly sound design — const generics *require* const params, so
proptest cannot generate schemes — but the prose claims sweep coverage the
tests do not have. Given this repo's own honesty-in-reporting discipline
(CLAUDE.md's R30-12 commit-taxonomy rule and the "name your denominator"
reporting rule), a one-line correction to "three hand-chosen schemes, with
`(size, align)` property-generated within each" is worth making, and costs
nothing.

---

## 6. Public API doc coverage — **strong; the best-documented crate surface I
## would expect a stranger to meet.**

Public surface is exactly six items:

| Item | Line | Doc quality |
|---|---|---|
| `Params<'a>` | `lib.rs:46-76` | struct doc + **per-field doc on all five fields**, each naming its own constraints and giving a concrete example (`(5, 4)` = mimalloc 1.25×; typical `extras` uses) |
| `size2class_len` | `lib.rs:78-85` | states the formula and *why* a consumer needs it (pinning the `L` generic) |
| `build_table` | `lib.rs:87-101` | spacing rule spelled out; `# Panics` section enumerating all three const-eval panics; explains *why* it is a hand-rolled merge (`const fn` cannot call `slice::sort`) |
| `build_size2class` | `lib.rs:157-171` | gives the **exact indexing expression the caller must use** (`size2class[(size - 1) >> log2(min_block)]`) and the bucket semantics; `# Panics` section |
| `SizeClasses<N, L>` | `lib.rs:222-229` | explains both generics and points at `build` |
| `SizeClasses::{build, table, size2class, min_block, min_block_shift, small_align_max, small_max, count, block_size, is_huge, class_for}` | `lib.rs:241-385` | every method documented; `block_size` has a `# Panics`; `class_for` has a 20-line doc covering the fit predicate, both paths, and the termination argument |

Verified mechanically: `-D missing_docs` passes (§3.3).

What a stranger who has never seen sefer-alloc gets right:

- The **`L` derivation** is the one genuinely awkward part of the API (a
  consumer must compute two const generics correctly or the build breaks), and
  it is addressed three times — module doc `lib.rs:36-41`, `size2class_len`'s
  own doc `lib.rs:78-81`, and `build_size2class`'s `L must equal ...`
  (`lib.rs:163-164`) — plus a copy-pasteable skeleton in `README.md:27-46`.
- The **`huge_threshold` policy boundary** is stated in its own module-doc
  section (`lib.rs:29-34`), so nobody has to guess whether the crate knows
  about pages or segments.
- The **`class_for` slow path** carries its own correctness argument, including
  why it terminates (`lib.rs:366-369`) and why it is never worse than a
  step-by-1 walk (`lib.rs:346-349`).

Gaps a third-party consumer would still feel, in order:

1. **The `extras` preconditions are prose-only** — §5.1. This is the doc gap
   *and* the code gap; fixing the asserts fixes both.
2. **No worked, compiling example.** The README example
   (`README.md:27-46`) is a ```` ```text ```` fence — correct per this repo's
   no-doctest rule, but it means the example is **never compile-checked** and
   contains a literal `/* table[N-1] — compute or pin */ 258_752`
   (`README.md:33`) that a reader must trust. `README.md:48` says "Runnable
   forms live in `tests/`" and the tests do ship in the tarball, so a
   docs.rs reader can find them — but only by browsing the repo. A tiny
   `examples/basic.rs` would close this at zero doctest cost and is the single
   highest-leverage documentation addition for external adoption.
3. **`build` takes `Params` by value (`lib.rs:251`) while `build_table` takes
   `&Params` (`lib.rs:102`).** Harmless (`Params: Copy`, `lib.rs:50`) but an
   inconsistency a first-time caller notices.

---

## 7. Performance angle — **no pending tuning work; the const-table design is
## stable and the API can be frozen.**

This is the load-bearing question given the crate sits on sefer's hot allocation
path (`src/alloc_core/size_classes.rs:209-211` → `SC.class_for`), so I checked
both indexes end-to-end per CLAUDE.md's round-start convention.

### 7.1 What the open-items indexes actually say

Three hits, all resolved or explicitly closed:

- **`docs/perf/OPEN_ITEMS.md:1305` (item 22 / T10).** The `class_for` `align>16`
  jump-ahead walk over `SIZE2CLASS` (perf#9) is recorded as **KEPT** — i.e.
  *already landed*, already moved from step-by-1 to the bitmask jump, and
  "correctness-pinned by `tests/size_classes_slow_path_equivalence.rs`". The
  NO-GO in that item is the per-class segment hint, which is a `HeapCore`/segment
  concern with nothing to do with this crate. **Not a pending change here.**
- **`docs/perf/OPEN_ITEMS.md:1865-1888` (item 39 / F13, 2026-08-03, task #505),
  sub-verdict (a).** This is the decisive one. It examined exactly this crate's
  `class_for` (`crates/size-classes/src/lib.rs:353-384`) plus
  `SMALL_ALIGN_MAX = 16` (`src/alloc_core/size_classes.rs:74`) on the
  `align > 16` classification hot path and returned **"verdict THIN, not worth a
  round"**, recorded as a deliberate NEGATIVE RESULT so a future round does not
  re-derive it. Its reasoning: the walk was already optimized once (T10's KEPT
  sub-finding above), the remaining walk is "typically 1-2 iterations (one table
  load, one `block & (align-1)` test, one lookup)", and the obvious next step —
  a 1-entry `(size, align) → class` memo — was **considered and rejected**
  because it adds a branch to the hottest path in the allocator, the exact shape
  the X4-B won-front rule (item 18) rejects.
- **`docs/CORRECTNESS_OPEN_ITEMS.md:1404-1440` (item 24).** Purely the
  publication-status item — `README.md:515`'s "each is a real crates.io crate"
  claim is false for `racy-ptr-cell`, `size-classes`, `tagged-index-stack`
  (`:1410-1414`). Its "Next trigger" is the K3/#598 publish-DAG decision this
  review feeds. **Publishing this crate closes one third of that item**; the
  README also currently displays a crates.io badge for it (`README.md:551`) that
  resolves to nothing today.

Nothing else in either index touches `size-classes`, `size_classes`, or
`SizeClass`.

### 7.2 Read: freeze the API, with one caveat

**There is no reason to hold off.** Three independent pieces of evidence:

1. **The last optimization already landed and was pinned** (T10 perf#9), and the
   *next* candidate was explicitly evaluated and rejected on the "adds a branch
   to the hottest path" rule (F13(a), 2026-08-03 — three days before this
   review). This is the strongest possible signal: the tuning question was asked
   recently, answered, and recorded as a negative result specifically so it is
   not reopened.
2. **The source has not changed in ~3 weeks and 4+ rounds.** `git log -- crates/size-classes`
   shows exactly two commits, both on 2026-07-17 (`121d657` extraction,
   `1d39e43` review fixes). Rounds 30-34 landed on top without touching it.
3. **Any future tuning is structurally non-breaking.** All the plausible
   optimizations — a wider `size2class` entry type, a different bucket stride, a
   memo — are *internal* to `build_size2class` / `class_for` and to the private
   fields of `SizeClasses` (`lib.rs:231-239`, all private). The one exception is
   `SizeClasses::size2class()` (`lib.rs:273-277`), which returns `&[u8; L]` and
   therefore **welds the `u8` entry width into the public API**; the `N < 256`
   assert (`lib.rs:182-185`) already caps the class count at 255 for the same
   reason. If a future scheme ever needs >255 classes, that accessor is the
   breaking point. Sefer is nowhere near it (58 classes at the widest, with
   `medium-classes-wide` — `src/alloc_core/size_classes.rs:113-133`), and pre-1.0
   semver makes it cheap regardless, but it is worth knowing which item is the
   frozen one.

**S3 — the one genuine forward-compat caveat, and it is not about performance:**
`Params` has five public fields and no `#[non_exhaustive]`
(`lib.rs:50-76`). Adding a sixth field is therefore a breaking change for every
downstream struct literal. `#[non_exhaustive]` is *not* a usable fix here —
it forbids struct-literal construction outside the crate, which is exactly how
`Params` must be built to stay `const`. So the honest position is: **`Params`
is frozen at five fields for the life of `0.1.x`**, and any new knob costs a
`0.2.0`. That is fine for a `0.x` crate, but it should be a conscious decision
made now rather than a surprise at the first feature request. (A `const`
builder — `Params::new(min_block, growth, geo_count).with_extras(..)` — is the
usual escape hatch if the maintainer wants room; it is not required to ship.)

---

## Summary table

| # | Area | Result |
|---|---|---|
| 1 | Standalone-crate justification | **Yes** — genuine cross-cutting library, no sefer concepts in the API, generality exercised by tests |
| 2 | Metadata | **Complete** — all 11 fields, both license files, valid keywords/categories |
| 3 | `cargo test --all-features` | **PASS** 9/9, non-vacuous (independent reference impls) |
| 3 | `cargo clippy --all-features --all-targets -D warnings` | **PASS**, zero diagnostics |
| 3 | `cargo doc --no-deps` (+ `-D missing_docs`) | **PASS**, zero warnings |
| 3 | `no_std` | **Holds** — verified on `thumbv7em-none-eabi`; **but CI never checks it** (S2) |
| 4 | `cargo package --list` / `--allow-dirty` | **PASS** — 10 files, 17.1 KiB compressed, no leakage, no path deps |
| 5 | TODO/FIXME/dead-code scan | **Clean** — zero markers |
| 5 | Precondition checking | **S1 — two unchecked `extras` preconditions, both reproduced, one produces a misaligned block via the fast path** |
| 6 | Public API doc coverage | **Strong** — every item documented, `# Panics` where needed; missing a compiling example |
| 7 | Perf / API-freeze readiness | **Ready** — no pending tuning; F13(a) explicitly closed the last candidate on 2026-08-03 |

---

## Open questions for the maintainer

1. **Publish or `publish = false`?** My recommendation is unambiguous:
   **publish**, and publish this one *first* among the three K3 crates — it is
   the only one whose standalone value is self-evident to a stranger, it is a
   leaf with no ordering constraint, and it is publish-clean today. Do you
   agree, or do you want all three K3 crates decided as one bundle?
2. **Land the §5.1 asserts before tagging `size-classes-v0.1.0`?** I think yes —
   adding compile-time preconditions after publication breaks someone's build,
   adding them before costs ~8 lines. But this contradicts the "read-only,
   nothing else changes before the freeze" posture of the current pre-release
   pass, so it is your call whether it goes in now or as `0.1.0` never ships and
   the first release is a corrected `0.1.0` cut later.
3. **Name.** `size-classes` is free on crates.io as of today, but it is a very
   generic two-word name that someone else could reasonably want. If you intend
   to publish at all, claiming the name is time-sensitive. Alternatively, is a
   namespaced `sefer-size-classes` preferable for consistency with
   `sefer-region`? (I would keep `size-classes` — the crate genuinely is not
   sefer-specific, and `sefer-region` is namespaced because *it* is.)
4. **`Params` forward-compat (S3).** Accept "five fields, frozen for `0.1.x`",
   or add a `const` builder now to buy room? No wrong answer; just needs to be
   decided rather than discovered.
5. **CI (S2).** Should `cargo build -p size-classes --target thumbv7em-none-eabi`
   be added to the existing `no_std` job (`ci.yml:711-725`) as part of the
   publish, so the crate's headline `no_std` promise is actually gated? Same
   question generalizes to K9's "full test/doc/package matrix for all workspace
   members".
6. **`release.yml` + CHANGELOG guard.** Adding `'size-classes-v*'` and the
   dropdown option is two lines — but the CHANGELOG guard reworked in L4/K5/K8
   will then run against that tag. Does a member crate need its own
   `crates/size-classes/CHANGELOG.md` before it can be tagged, or does the guard
   fall back to the root changelog for members?
