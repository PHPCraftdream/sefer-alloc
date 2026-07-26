# Design: guard against stale derived-numeric literals in comments

**Task:** R18-6 (#334). **Round:** 18. **Scope:** design / recommendation only —
no `src/` change, no new test file in this round. **Status:** evaluation
delivered; implementation (if any) deferred to a future round per the
recommendation below.

Companion to the R17-6 plan note
(`docs/reviews/2026-07-24-r17-plan.md`, line 31: «оценить лёгкий
автоматизированный чек производных констант в inline-комментариях» — deferred
from Round 17). Format modeled on `docs/PHASE13_4_DEALLOC_DESIGN.md`.

## 0. The defect class (4 recurrences in 3 rounds)

Same shape each time: a comment restates the *resolved numeric value* of a
named compile-time constant instead of citing the constant's name or its
derivation. When the constant is later raised/changed, the comment is not
updated and silently goes stale. A reader takes the stale number at face value
and is misled.

| Round | Task | Site | What rotted |
|---|---|---|---|
| R15-5 | #307 | 5 files | `WORDS_PER_CLASS=16` / `MAX_SEGMENTS=1024` literals left behind after the R14-7 `1024→4096` raise (cascade into every `MAX_SEGMENTS`-derived constant) |
| R16-2 | #312 | `dirty_by_class.rs` | a `55`-class figure mislabeled `58` (R15-5's fix corrected the `WORDS_PER_CLASS` number but got the class-count label wrong) |
| R17-5 | #323 | `heap_core_free.rs` | a `~30 MiB` pad-target figure contradicted by the gate report (refuted, not merely stale) |
| R17-6 | #323 | `segment_table.rs` | `// 2048` (real: `8192`) and `= 16 KiB` (real: `64 KiB`) next to `HASH_CAPACITY` / `HASH_FOOTPRINT` — fixed in `d8f9c9b` |

The existing structural guard `tests/no_stale_doc_references.rs` catches a
*different* drift shape — **known forbidden strings / re-computable counts**
(the removed `Heap` type, the removed abandon/adopt substrate, the live
`unsafe`-inventory counts, the `tests/*.rs` file count). It does NOT address
arbitrary derived numbers in arbitrary `//`/`///` comments, and was never
designed to.

**Root cause (OBSERVED across all four):** the comment restates a *resolved
literal* (`2048`, `16`, `~30 MiB`, `58`) instead of citing the *name*
(`MAX_SEGMENTS`, `2 * MAX_SEGMENTS`, `WORDS_PER_CLASS`). The fix that has
actually prevented recurrence each time was rewording to cite the name /
derivation — e.g. R17-5's follow-up (`fbc48a5`, «the fixed `MAX_SEGMENTS` cap»)
and `segment_table.rs:71`'s `` `HASH_CAPACITY = 2 * MAX_SEGMENTS` `` phrasing.

## 1. Candidate variants — honest evaluation

### Variant 1 — explicit known constant↔comment pair-list (no_stale-style)

A test holding a hand-maintained list of «constant name → current value → file
line where the literal is restated», asserting the literal still matches.

- **Pro:** exact, zero false positives; same proven shape as the existing
  thread-free / abandon-adopt prose guards.
- **Con:** it is **reactive**, not preventive. It can only catch regressions to
  a *pre-registered* stale string — exactly the four above, already fixed. It
  catches nothing about a NEW constant/literal pair someone introduces
  tomorrow, because nobody added that pair to the list. This is the same manual
  add-each-pair discipline that has already failed four times: the failure mode
  is «the author of the new comment did not also remember to extend the
  guard-list», and a pair-list does not change that.
- **Verdict:** weak. A pair-list is a post-hoc checklist for known past drift,
  not a guard against the next instance. Marginal value over the status quo
  (the four are already fixed and named in this doc). **Reject as the primary
  mechanism.**

### Variant 2 — general grep-based lint over `src/**/*.rs`

Scan for `CONST_NAME = <number>` in code, find the same number as a literal in
nearby `//`/`///` comments within N lines, flag mismatches.

Two independent problems, both demonstrated on THIS repository:

1. **Cannot decide «stale» without history.** The lint sees only the CURRENT
   constant value. If a comment legitimately cites the current value, that is
   fine; if it cites a value that *was* current at write-time and the constant
   later moved, the lint flags the mismatch — good. But the lint cannot tell
   «this number is a derived reference to constant X» from «this number is
   unrelated to any constant». It has no constant↔comment binding, which is the
   whole unsolved problem.

2. **High false-positive rate on legitimate historical prose.** This repo
   already contains the exact patterns a grep-lint would mis-flag:
   - `src/alloc_core/bootstrap.rs:228` — «`1024→4096` raise quadrupled this
     loop's trip count» — *intentionally* names both the old and new value to
     describe a past change. Correct. A grep for numbers near `MAX_SEGMENTS`
     flags it.
   - `src/registry/heap_core_xthread.rs:549` — «`HEAP_OVERFLOW_CAP = 2048`
     allows up to 2048 distinct bases» — cites a DIFFERENT constant's value, in
     a comment about `HEAP_OVERFLOW_CAP`, near segment prose. Correct. A naive
     «`2048` near segment» grep conflates it with the long-gone `MAX_SEGMENTS=1024`
     era.
   - `src/alloc_core/segment_header.rs:301,606` — «`104` and `120` round up» /
     «`4096` … UNCHANGED» — descriptive layout arithmetic. Correct.

   Suppressing these needs an allow-list, which collapses back into variant 1's
   manual discipline (and the allow-list itself rots).

- **Verdict:** unsound for this codebase. The false-positive surface is not
  theoretical — it is already present in the tree. **Reject.**

### Variant 3 — convention instead of tooling

A standing prose rule: *never restate the resolved numeric value of a named
constant in a comment; cite the constant's name or its derivation
(`2 * MAX_SEGMENTS`, `MAX_SEGMENTS / 64`). If a resolved number is unavoidable
(e.g. a concrete byte footprint a reader needs to see), restate it as a
derivation and keep the arithmetic visible.*

First, the mechanical-feasibility sub-question (can Rust inject a live value
into a comment at all?):

- **`//` ordinary comments:** no. The compiler strips them before macro
  expansion; there is no `concat!`/`stringify!` reach. Pure text.
- **`///` and `//!` doc comments:** *technically* yes, via
  `#[doc = concat!("…", stringify!(CONST), "…")]` (stable). But (a) a source
  reader sees the macro, not the value — which defeats the entire purpose of
  restating the number inline (the comment exists so a glancing reader *sees*
  `8192`, not `stringify!(HASH_CAPACITY)`); (b) it does not cover `//!` module
  docs readably either; (c) the project **bans doctests** in `src/**/*.rs`
  (`CLAUDE.md`), and while `#[doc = concat!]` is not a doctest, the same
  «keep runnable assertions out of src/ doc comments» spirit applies.
- **Conclusion:** a mechanical self-check of comment text is **not realistically
  available** in Rust. The only enforceable form of «convention» is a prose rule
  upheld by review.

The convention is not vacuous «be careful» — it has a concrete, proven escape
hatch already in the tree: the `dbg_*` test-only accessors
(`src/alloc_core/alloc_core_core_diag.rs:853 dbg_max_segments`,
`:872 dbg_words_per_class`; `src/registry/heap_core_diag.rs:337
dbg_promotion_compiled`). When a number MUST be restated *in executable code*
(a test, an example, a printed harness line), restate it via the accessor, not
as a literal — that is exactly the R15-1 fix for
`examples/r13_9_class_aware_dirty_sidecar_rss.rs`, whose doc and `println!`
hardcoded `16` until `dbg_words_per_class()` replaced it.

- **Pro:** zero false positives; zero new infrastructure; matches what already
  works; the escape hatch (`dbg_*`) is established and cheap.
- **Con:** not machine-enforced for `//` comments — relies on review discipline,
  which is what failed four times. **But:** review discipline with a *named,
  citable rule* («cite the name, per the stale-literal convention») is
  materially stronger than review discipline with no named rule, because a
  reviewer can invoke it as a concrete objection rather than a vague taste
  preference. The four failures all occurred with no such named rule in
  `CLAUDE.md`.
- **Verdict:** **adopt as the primary standing rule.** See §3.

### Variant 4 — test-side mirrored `const` + `assert_eq!` against the real value

Keep the «documented value» as a named `const` in `tests/*.rs`, assert it
equals the real constant (or the `dbg_*` accessor) at test time, and have the
*comment* cite the const name. A future raise that updates the real const
without the mirror breaks the build.

**The precedent already exists and is the strongest single data point in this
doc:** R16-5 (task #315, commit `eedc111`) added
`HeapCore::dbg_promotion_compiled()` precisely so
`tests/r14_4_promotion_move_leg_reduction.rs`'s hand-mirrored `HAS_PROMOTION`
`const bool` could `assert_eq!(HAS_PROMOTION, HeapCore::dbg_promotion_compiled())`
(`:114-118`) — closing a real review finding (P3-2, Round 15) about exactly this
drift shape between a `src/`-private predicate and a test-side mirror.

**Critical honest caveat — variant 4 does NOT check comment text.** It protects
a *test-side mirrored const* from drifting out of sync with a *src const*. The
comment is still unchecked free text. Variant 4 only helps a comment *if* the
comment cites the const NAME (variant 3) rather than the literal — at which
point variant 3 is doing the protective work and variant 4 is a tripwire on the
const the comment names. In other words: **variant 4 is complementary to
variant 3, not a substitute.** Used alone (mirror-const + assert but comment
still cites the literal number), it catches a stale test-side const but the
comment still rots — which is the actual observed failure.

- **Pro:** compile-time tripwire; zero false positives; mature precedent; the
  `dbg_*` machinery for the churning constants already exists.
- **Con:** overhead of one accessor + one assert per protected constant;
  protects only the const-pair, not arbitrary comment text; irrelevant to the
  defect class unless combined with the prose convention.
- **Verdict:** **adopt selectively** for the small set of constants that have
  churned ≥2× (see §3), as a tripwire *backing* the convention — not as a
  general comment-guard.

## 2. Why «just build a lint» is a trap here

The instinct (and the R17-6 plan's framing) is that a no_stale-style test
«should» exist for this. The evidence above says the only variants that are
*mechanically enforceable over arbitrary comment text* are 1 (pair-list,
reactive, same manual discipline that failed) and 2 (grep, unsound on this
repo's existing prose). Variant 4 is enforceable but operates on consts, not
comments. There is no fourth mechanical option in Rust: `//` comments are
inert, and `///` doc-injection via `concat!`/`stringify!` destroys the inline
readability that is the comment's reason to exist.

This is the calibrated honest answer the task asks for: **a general automated
guard for stale derived literals in arbitrary comments is not realistically
buildable in Rust without either an unsound heuristic or a manual pair-list
that reproduces the failure mode it claims to prevent.** The defect is a prose
habit; the cure that has provably worked is a prose habit (cite the name).

## 3. Recommendation

**Convention (variant 3) as the primary standing rule, reinforced by selective
variant-4 tripwires for the highest-churn constants. Explicitly reject variants
1 and 2 as standalone mechanisms.**

This is not «do nothing»: it formalises a rule that currently has no name in
`CLAUDE.md`, backs it with the existing `dbg_*` escape hatch, and targets the
~3 constants that have actually churned repeatedly. It is also not
over-engineering: 4 cases in 3 rounds does not justify new lint/test
infrastructure, especially when the infrastructure that *would* work (1/2) is
either unsound or reproduces the manual-discipline failure.

### 3.1 The convention (to be added to `CLAUDE.md` in a future round)

Proposed wording for the «File and module structure» or a new «Comment
discipline» section:

> **Never restate the resolved numeric value of a named compile-time constant
> in a `//` / `///` / `//!` comment. Cite the constant's name, or show the
> derivation (`2 * MAX_SEGMENTS`, `MAX_SEGMENTS / 64`). If a concrete number is
> genuinely needed for a reader (e.g. a byte footprint), state it as a visible
> derivation and keep the arithmetic in the comment, never as a bare literal
> detached from its source constant. Rationale: four recurrences of stale
> derived literals (R15-5/R16-2/R17-5/R17-6) across three rounds — every one
> was a resolved literal whose source constant had moved. See
> `docs/DESIGN_stale_literal_guard.md`.**

### 3.2 Selective variant-4 tripwires — if «build» next round

Only for constants with ≥2 churn events, i.e. the `MAX_SEGMENTS` family and
`SMALL_CLASS_COUNT`. The accessors already exist for two of these; what is
missing is a companion `assert_eq!` canary in `tests/` (the `HAS_PROMOTION`
pattern) and, where a comment currently restates the resolved value, rewriting
the comment to cite the name (variant 3):

| Constant | Definition site | Current value | Churn history | Existing accessor | Action if built |
|---|---|---|---|---|---|
| `MAX_SEGMENTS` | `src/alloc_core/segment_table.rs:64` | `4096` | raised `1024→4096` (R14-7); cascaded into every derived constant below | `dbg_max_segments()` (`alloc_core_core_diag.rs:853`) — **exists** | add `assert_eq!(MAX_SEGMENTS, dbg_max_segments())`-style canary in a `tests/` file (the accessor already exposes it; a test-side mirror + assert closes the loop) |
| `HASH_CAPACITY` (`= 2 * MAX_SEGMENTS`) | `segment_table.rs:73` | `8192` | the R17-6 stale `// 2048` | via `dbg_max_segments()` ×2, or a new `dbg_hash_capacity()` | rewrite the `// 8192` trailing comment to `` // = 2 * MAX_SEGMENTS `` (variant 3); optionally add accessor |
| `WORDS_PER_CLASS` (`= MAX_SEGMENTS / 64`) | `src/alloc_core/segment_directory.rs:172` | `64` | R15-5 stale `16` across 5 files | `dbg_words_per_class()` (`alloc_core_core_diag.rs:872`, `alloc-segment-directory`-gated) — **exists** | already has an accessor; add a canary `assert_eq!` mirroring it, like `HAS_PROMOTION` |
| `SMALL_CLASS_COUNT` (`= SIZE_CLASS_TABLE.len()`) | `src/alloc_core/size_classes.rs:165` | `49` (default) / `55` (`medium-classes`) / `58` (`medium-classes-wide`) | R16-2 mislabeled `55`↔`58` | none | `dirty_by_class.rs:37-39` **currently restates live derived literals** (`49-class`, `3,136 words = 25,088 bytes = 24.5 KiB`, `55 … 28,160`, `58 … 29,696`) — these are at-risk RIGHT NOW; action is a variant-3 rewrite (cite `SMALL_CLASS_COUNT` × `WORDS_PER_CLASS` symbolically), optionally backed by a `dbg_small_class_count()` accessor + canary |

The `SMALL_CLASS_COUNT` / `dirty_by_class.rs:37-39` case is the one live
at-risk site found while writing this doc — **it is a doc-debt candidate for a
future round regardless of whether variant 4 is adopted** (it would rot the
next time the size-class table or `MAX_SEGMENTS` changes, exactly as R16-2
rotted). Flagging it here as OBSERVED, not fixing it in this design-only round.

### 3.3 What is explicitly NOT recommended

- A new `tests/no_stale_derived_literals.rs` pair-list test (variant 1) —
  reactive, same discipline that failed; the four known cases are already fixed.
- A grep-based lint over `src/**/*.rs` (variant 2) — unsound on this repo's
  existing historical prose (`bootstrap.rs:228`, `heap_core_xthread.rs:549`,
  `segment_header.rs:301/606`); an allow-list collapses into variant 1.
- `#[doc = concat!(stringify!(CONST))]` injection into `///` comments — destroys
  inline readability, doesn't cover `//`, against the spirit of the no-doctests
  rule.
- A blanket «machine every numeric literal in every comment» pass —
  disproportionate to 4 cases in 3 rounds; the convention + selective tripwire
  is the calibrated response.

## 4. What this round delivers

This document only. No `src/` edit, no new test file, no `CLAUDE.md` change —
those are the implementation of §3.1/§3.2 and are explicitly deferred to a
future round per the task's design-only scope. The next round, if it adopts
this recommendation, owns: (a) the `CLAUDE.md` convention wording, (b) the
`dirty_by_class.rs:37-39` variant-3 rewrite (live debt), (c) optionally the two
`assert_eq!` canaries for `WORDS_PER_CLASS` and `MAX_SEGMENTS` mirroring the
`HAS_PROMOTION` precedent.
