# `aligned-vmem` — round-9 CLOSING review (verification of the V2-1/V2-2/V2-3 remediation)

**Date:** 2026-08-13

**Scope:** verification of the two remediation tasks (#906 task A, #907 task B) that closed
`docs/reviews/2026-08-13-aligned-vmem-round9-review.md`'s findings V2-1, V2-2 and V2-3, plus
the two `--no-ff` merge commits that landed them. The round's whole diff
(`git diff b6bfdac..HEAD --stat`: 3 files, +50/−7) and every piece of code the changed text
makes a claim about: `decommit`/`decommit_lazy`'s guards, `page_size()`'s own guard,
`libc_madvise`'s counter path, `try_reserve_aligned_exact`, and every durable `file:line`
citation the round's line-shifts could have moved. Like rounds 6–8, this round was delegated to
independent sub-agents in isolated git worktrees (`vmem-r9-a`, `vmem-r9-b`, both branched from
`b6bfdac`), then merged sequentially.

**Reviewed tree:** local `main` @ `3900828` (the task #907 merge).
`git rev-parse origin/main` → `b6bfdac08562e7cc8a5369ffc3fd7ca3a7838909`;
`git log origin/main..HEAD --oneline | wc -l` → **4** (the 2 merge commits plus the 2 task
commits they carry). **None of round 9 has been pushed**, so there is no CI signal for this
round's own diff — in particular, neither new discriminating arm has yet executed on the 16 KiB
Apple Silicon runner, which is the only host where either of them is live.

`git status --porcelain` shows exactly one untracked entry:
`docs/reviews/2026-08-13-aligned-vmem-round9-review.md`. That is **V2C7**.

**Toolchain / host:** `rustc 1.97.0` (2d8144b78 2026-07-07), stable-x86_64-pc-windows-msvc;
Windows 10 Pro, 4 KiB page. **No Darwin host and no Darwin target** — every Darwin claim below
is reasoned from spec and from code read in the current tree, never executed here. The
`x86_64-unknown-linux-gnu` cross-compile clippy row (item 51) was run as a first-class matrix
row, as it was in the round-9 review itself.

**Nature:** read-only. Nothing in the repository was modified other than the creation of this
document. No `git add` / `git commit` / `git push` / branch, worktree or ref mutation. Every
command quoted below was executed on this host; every `file:line` citation was read in the
current tree before being written down, and every pre/post comparison was made by reading the
cited line at BOTH `b6bfdac` (via `git show b6bfdac:<path>`) and `HEAD`.

**Finding prefix:** `V2C` (round-9 closing). Prior prefixes deliberately not reused:
`V`/`W`/`P` (rounds 1–2), `F` (round 3), `R`/`CR` (round 4 + closing), `Q`/`QC` (round 5 +
closing), `S`/`SC` (round 6 + closing), `T`/`TC` (round 7 + closing), `U`/`UC` (round 8 +
closing), `V2` (round 9).

---

## Verdict up front

**All three of V2-1/V2-2/V2-3's fixes landed, all three landed on the intended content, and
the round's headline fix genuinely discriminates what it claims to.** I re-derived V2-1's
discrimination from the guard source rather than from the task's argument — both directions,
both entry layers, on both a 16 KiB and a 4 KiB host — and it holds (§"V2-1's discrimination,
derived independently"). V2-2's premise was re-verified mechanically (`try_reserve_aligned_exact`
contains no `page_size()` token at all; the only two runtime call sites in `src/` are
`lib.rs:1088` and `:1156`). The full matrix is green here, re-executed rather than taken on
trust: 42/47 tests, four clippy rows, `fmt --check`, the doc-drift guard, and a clean
conflict-marker sweep. The net diff touches exactly the three files the two task commits touch
— no stray edit rode along on either merge.

**The bonus mock-layer mirror is a real improvement, not padding.** `smoke.rs`'s test is
`not(feature = "mock")`-gated and `mock.rs`'s is `mock`-gated, so before this round the
`--all-features` CI row (`ci.yml:828`, which runs on the macOS runner) had NO discriminating
coverage of either guard's validation base at all. The mirror gives that row one.

**The campaign's signature pattern held for a tenth time — and this round it produced the one
finding class that a diff-scoped review structurally cannot catch.** Five of this closing
review's seven findings are again in one-commit-old text or in this round's own process
residue. But two of them (V2C1, V2C3) are **line-number citations located OUTSIDE this round's
diff that this round's diff invalidated** — task #907's comment hunk at `lib.rs:434-439`
replaced 3 lines with 5, and that `+2` net shifted every
line below it, and two durable citations were pointing there. That matters for the pivot
question, because the round-9 review's proposed replacement (diff-scoped review of commits
touching this crate) would not have found either one: neither citation is in the diff. See
§"Evaluating the round-9 PIVOT recommendation" — my verdict is **agree with the pivot, but
conditional on the citation-resolver script being built, not left as optional item #4.**

**One of the two is also a pre-existing defect that round 9's own full read missed.**
`smoke.rs:74`'s `lib.rs:955-967` / "the literal at `:963`" citation was accurate when task #892
wrote it (verified at `e496071`) but had already drifted 3 lines by `8380607` — it survived
round 8's full read AND round 9's full read, both of which read every test file. Round 9's
review states "zero pre-existing defects"; that claim needs the small qualification V2C1
supplies. It does not argue for a tenth full read (two human full reads already missed it) — it
argues for the script.

**Nothing here is publish-blocking for 0.2.0 (task #658).** V2C1/V2C3 are citation ranges,
V2C2/V2C4/V2C5 are comments and test scope, V2C6/V2C7 are process. No packaged surface changed
this round beyond one comment inside `lib.rs`.

---

## What was verified green — every command below was executed on this host

```
$ git rev-parse HEAD                       3900828c88f41e8bd84b27df32456b65b5ad251e
$ git rev-parse origin/main                b6bfdac08562e7cc8a5369ffc3fd7ca3a7838909
$ git log origin/main..HEAD --oneline | wc -l                                      4
$ git diff b6bfdac..HEAD --stat
 crates/vmem/src/lib.rs     |  8 +++++---
 crates/vmem/tests/mock.rs  | 26 ++++++++++++++++++++++++++
 crates/vmem/tests/smoke.rs | 23 +++++++++++++++++++----
 3 files changed, 50 insertions(+), 7 deletions(-)
    # exactly the union of 247a8b5 (task #906) and c25278a (task #907); no stray file

$ git log --format="%h %p %s" -4
3900828 486a97b c25278a  Merge vmem-r9b (task #907): ...
486a97b b6bfdac 247a8b5  Merge vmem-r9a (task #906): ...
    # both are genuine two-parent merges off b6bfdac, as described

$ cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --no-fail-fast
lib 0 / fault_injection 5 / huge_pages 1 / lazy_commit 11 / min_page 2 / mock 0 /
smoke 20 / vmemerror_io_bridge 3 / doc-tests 0            => 42 passed, 0 failed

$ cargo test -p aligned-vmem --all-features --no-fail-fast
0 / 0 / 1 / 11 / 2 / 10 / 20 / 3 / 0                       => 47 passed, 0 failed

$ cargo clippy -p aligned-vmem --all-targets -- -D warnings                          -> clean
$ cargo clippy -p aligned-vmem \
    --features "lazy-commit huge-pages fault-injection bench-internals" \
    --all-targets -- -D warnings                                                     -> clean
$ cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings           -> clean
$ cargo clippy -p aligned-vmem --target x86_64-unknown-linux-gnu \
    --features "lazy-commit huge-pages fault-injection bench-internals" \
    --all-targets -- -D warnings                                                     -> clean
    # item 51's row
$ cargo fmt -p aligned-vmem --check                                                  -> clean

$ node scripts/vmem-doc-drift-guard.mjs
[vmem-doc-drift-guard] OK: no unconditional over-reserve/trim statements found  (exit 0)

$ grep -rnE '^(<<<<<<<|=======|>>>>>>>)$' crates/vmem/ docs/CORRECTNESS_OPEN_ITEMS.md \
      docs/perf/OPEN_ITEMS.md CHANGELOG.md
(no output — both merges are clean)
```

Test counts are **42 / 47, identical to rounds 8 and 9** — the correct signature for this
round: task #906 extended two EXISTING tests' bodies rather than adding new `#[test]`
functions, and disabled none. Note what that identity also means: **on this 4 KiB host both
new arms are compiled but skipped** (`page_size() > PAGE` is false), so a green local run is
NOT evidence that either arm executes correctly — only the macOS runner will produce that, and
only after this round is pushed.

---

## Round-9 remediation pass — V2-1/V2-2/V2-3 verification

| # | Status in the current tree | Evidence |
|---|---|---|
| V2-1 | **CLOSED for `decommit_lazy`'s guard at BOTH layers, and the fix is correct** | `smoke.rs:594-605` adds `decommit_lazy(base, PAGE, 2 * PAGE)` (the call itself at `:604`) inside the existing `if page_size() > PAGE` block (`:586`), directly under the UC4-era `decommit` call at `:592`, with its own SAFETY comment (`:594-602`). `mock.rs:288-312` adds the volunteered mock-layer mirror with a `Call::DecommitLazy`-absence assertion. Discrimination re-derived from the guard source, not from the task's claim — see the next section. The doc comment at `:582-585` was correctly extended to state that each guard needs its own call. The assert message at `:609-616` (which V2-1 flagged as claiming the guard PAIR) is now accurate as written, because both guards are covered. |
| V2-2 | **CLOSED, and the premise re-verified mechanically** | `lib.rs:434-439` no longer names `try_reserve_aligned_exact`. Premise check: `grep -n "page_size()" crates/vmem/src/lib.rs` returns exactly two runtime call sites — `:1088` (`decommit`) and `:1156` (`decommit_lazy`) — plus `:390` inside `page_size()` itself and doc/comment mentions at `:87`, `:380`, `:394`, `:1034`, `:1065`, `:1461`, `:2369-2370`. `try_reserve_aligned_exact` (`lib.rs:2097-2160`) contains the token only inside U1's own 24-line explanatory comment (`:2119-2142`), never as a call. The comment fix is therefore accurate, not merely plausible. |
| V2-3 | **CLOSED** | `smoke.rs:618-620` now reads "all calls above were rejected", which stays correct at any future arm count — exactly the wording V2-3 recommended. |

Surface checks on the round's diff, all negative as expected: no new `unsafe` token entered the
crate (both new `unsafe {}` blocks wrap calls to already-`unsafe` public functions in `tests/`,
not new seam code); no public item changed; no `#[cfg]` on any shipping item changed;
`Cargo.toml` untouched; `docs/perf/OPEN_ITEMS.md` untouched (correct — nothing in V2-1/2/3
concerned it); `docs/CORRECTNESS_OPEN_ITEMS.md` untouched (this is V2C3 and V2C6).

---

## V2-1's discrimination, derived independently

The brief asked for this to be traced against the real guard rather than trusted. Both guards
are byte-identical (`lib.rs:1088-1091` for `decommit`, `:1156-1159` for `decommit_lazy`):

```text
let ps = page_size();
if start >= end || !start.is_multiple_of(ps) || !end.is_multiple_of(ps) { return; }
```

`PAGE == 1 << 12 == 4096` (`lib.rs:157`). `page_size()` (`:385-407`) returns
`queried` only when `queried >= PAGE && queried.is_power_of_two()`, else `PAGE` — so
`page_size()` is ALWAYS a power of two `>= 4096`, and under the gate `page_size() > PAGE` it is
`>= 8192`.

**Call under test: `decommit_lazy(base, 4096, 8192)` on a 16 KiB host (`ps = 16384`).**

- **Correct guard (`ps = page_size()`):** `4096 >= 8192` → false. `4096.is_multiple_of(16384)`
  → false, so `!… ` → **true** → the `if` fires → `return`. **REJECTED.** No `mock::record`,
  no `decommit_pages_impl`, no `madvise`. `unix_madvise_attempts()` stays 0 and `drain()`
  contains no `Call::DecommitLazy` — both assertions pass. ✓
- **Hypothetically swapped guard (`ps = PAGE`):** `4096 >= 8192` → false;
  `4096.is_multiple_of(4096)` → true → `!` → false; `8192.is_multiple_of(4096)` → true → `!` →
  false. The `if` is `false || false || false` → falls through. **FORWARDED.** ✓

The forwarded case is observable at both layers, which I confirmed by following the call chain
rather than assuming it:

- **Real layer (`smoke.rs`, `not(feature = "mock")`):** `decommit_lazy` →
  `decommit_pages_impl(…, DecommitKind::Lazy)` (`lib.rs:2173-2186`) → the `Lazy` match arm at
  `:2184` → `libc_madvise(addr, len, madv_free_advice())` → `libc_madvise` (`:2502-2532`)
  increments `UNIX_MADVISE_ATTEMPTS` at `:2524` **under `bench-internals`, unconditionally on
  both arms** → `unix_madvise_attempts()` returns 1 → `assert_eq!(attempts, 0)` (`:609-616`)
  fires with a message that names the base swap. ✓ (This is the specific link that makes the
  fix work at all: had the counter been incremented only on the `Eager` arm, the new
  `decommit_lazy` call would have been a no-op oracle. It is not — one `libc_madvise` serves
  both kinds.)
- **Mock layer (`mock.rs`, `feature = "mock"`):** `decommit_lazy`'s `#[cfg(feature = "mock")]`
  branch (`lib.rs:1160-1165`) calls `mock::record(mock::Call::DecommitLazy { … })` → `drain()`
  contains it → `assert!(!calls.iter().any(…))` fires. ✓ Note `query_os_page_size` has no
  `mock` arm (`lib.rs:409`/`:421`/`:443` are unix/windows/miri only), so `page_size()` under
  `mock` is still the REAL OS page size — the gate behaves identically in both test binaries,
  as the arm's design requires.

**The pre-existing lazy call cannot discriminate, confirming V2-1's premise:**
`decommit_lazy(base, PAGE, 0)` (`smoke.rs:569`, `mock.rs:279`) short-circuits on
`4096 >= 0` → true under EITHER base, before any alignment term is evaluated. ✓

**On a 4 KiB host** (this one, and every Linux CI runner) `page_size() == PAGE`, the gate is
false, and both arms are skipped — correct, since no offset can distinguish the two bases when
they are equal. The arms are therefore live ONLY on the macOS runner, on both of its rows
(`ci.yml:823` named features → `smoke.rs`'s arm; `ci.yml:828` `--all-features` → `mock.rs`'s
arm).

**Both new SAFETY comments are accurate.** `PAGE` is trivially a multiple of `PAGE`; and under
the gate, `page_size() >= 2 * PAGE` with both values powers of two, so `PAGE` is genuinely not a
multiple of `page_size()` — the arithmetic claim holds. `base` is the same live `4 MiB`
(`smoke.rs`) / `2 MiB` (`mock.rs`) reservation, and `2 * PAGE = 8192` is well inside both
spans, so the "same live reservation" clause is true and the range is in-bounds even in the
swapped-guard world where the call is actually forwarded to `madvise(2)`. The smoke-side
comment's additional factual claim — that the only other `decommit_lazy` call in the test is
rejected by `start >= end` under either base — is verified above. ✓

---

## Findings

### V2C1 — LOW — this round's `+2` line shift pushed `granted_huge: false` OUT of the `lib.rs:955-967` range that `smoke.rs:74` cites specifically to point at it; the companion `:963` sub-citation was already 3 lines stale before this round and is now 5

**Where:** `crates/vmem/tests/smoke.rs:71-82` (the doc comment on
`ordinary_reservation_never_reports_huge`, `:84`), specifically `:74-75`:

> `reserve_aligned_raw(..).map(...)`, `lib.rs:955-967`, the literal at `:963`

**Traced across four revisions** (`git show <sha>:crates/vmem/src/lib.rs | sed -n '963p;966p'`):

| revision | `lib.rs:963` | where `granted_huge: false` actually is | inside the cited `955-967` range? |
|---|---|---|---|
| `e496071` (task #892, round 7 — the commit that WROTE this citation) | `granted_huge: false,` | `:963` | yes — **citation exact** |
| `8380607` (round-7 closing) | `reservation_len,` | `:964` | yes (sub-citation already off by 1) |
| `b6bfdac` (round-9 base) | `base,` | `:966` | yes (sub-citation off by 3) |
| `3900828` (**HEAD, post-round-9**) | `reserve_aligned_raw(…).map(…)` | **`:968`** | **NO — outside the range** |

So the sub-citation drift is pre-existing (task #907 did not create it), but **task #907's `+2`
insertion at `lib.rs:434-439` is what pushed the cited literal past the range's upper bound.**
The range now spans `lib.rs:955-967`, whose first line is `mock::record(mock::Call::Reserve { … })`
— the `mock` arm of a different code block — and whose last line is `reservation_len,`. A reader
following the citation lands on a range that no longer contains the thing the sentence exists to
show them.

**Failure scenario (concrete).** The comment's entire purpose is to warn that
`ordinary_reservation_never_reports_huge` is near-vacuous because `granted_huge: false` is a
hard-coded literal two frames up — it is the corrective that task #892 landed for finding T2,
and `huge_pages.rs:61-62` is named as the real regression guard. A contributor deciding whether
that test can be strengthened (or whether it may be deleted) opens `lib.rs:955-967`, finds a
`mock::record` call and no `granted_huge` literal anywhere in the range, and concludes either
that the warning is stale (it is not — the literal is four lines below, at `:968`) or that they
are reading the wrong function. Either conclusion re-opens a question T2 already answered. This
is the same staleness class as round 8's U3 and round 7's TC4/T5, one notch milder because the
range still lands in the right function.

**Fix:** replace the range+offset citation with symbol names, exactly as task #901 (U3) did for
item 48's S9 bullet — e.g. "the `granted_huge: false` literal in `reserve_aligned`'s
`reserve_aligned_raw(..).map(..)` closure that builds `RawReservation`" — which is immune to
line drift by construction. `smoke.rs:79`'s sibling `huge_pages.rs:61-62` citation deserves the
same treatment in the same edit (it is unaffected THIS round only because `huge_pages.rs` was
not touched; it is the identical construction and will drift the first time it is).

### V2C2 — LOW — `smoke.rs:541-542`'s claim that the base-swap defect is something "this crate's `mock`-feature test suite has no way to observe at all" was made false by task #906's OWN bonus, in the sibling file that very sentence names, in the same commit

**Where:** `crates/vmem/tests/smoke.rs:528-546` — the doc comment on
`decommit_contract_violation_never_reaches_madvise` (`:549`), which justifies why the
real-syscall test is needed IN ADDITION to `mock.rs`'s sibling; the false clause is at
`:541-542`:

> Without this, a future "simplification" that changed the validation base in `lib.rs`'s
> `decommit`/`decommit_lazy` from `page_size()` to the crate's smaller `PAGE` constant … would
> forward a `PAGE`-aligned-but-not-`page_size()`-aligned offset straight to `madvise(2)` on any
> host where the OS page size exceeds `PAGE` … **which this crate's `mock`-feature test suite
> has no way to observe at all**, and which would go undetected on any CI runner whose OS page
> size happens to equal `PAGE`.

versus `crates/vmem/tests/mock.rs:297-312`, added by this round's task #906, which observes
precisely that defect for `decommit_lazy` at the mock call-log layer on any host where
`page_size() > PAGE`. Task #906's own commit message states this outright — "giving
`decommit_lazy`'s guard a second, mock-layer oracle alongside `smoke.rs`'s
madvise-attempt-count oracle" — i.e. the commit knew it was contradicting the claim and did not
sweep it.

**How much is false, precisely.** Two readings of the "which" exist and I checked both:
- **Narrow reading** ("the mock suite cannot observe the real `madvise(2)` rejection"): still
  TRUE — under `mock` the syscall never happens, so `EINVAL`/all-or-nothing is unobservable.
- **Natural reading** ("the mock suite cannot observe this DEFECT"), which is the one the
  parallel second clause forces (that clause — "would go undetected on any CI runner whose OS
  page size happens to equal `PAGE`" — is unambiguously about the defect, not about `madvise`
  semantics): now FALSE for `decommit_lazy`, still true for `decommit` (see V2C5).

**Failure scenario.** The sentence is a coverage-rationale, and the realistic reader is someone
deciding whether one of the two tests is redundant. Post-#906, a contributor auditing test
duplication reads this comment, believes the mock layer structurally cannot cover this class,
and deletes `mock.rs:297-312` as apparently-dead scope — removing the ONLY discriminating
coverage the `--all-features` CI row has for either guard's validation base, while the comment
that misled them stays in place asserting the deletion was sound.

**Fix:** one clause — "which this crate's `mock`-feature test suite cannot observe as a real
`madvise(2)` rejection (task #906 added a mock-layer arm that observes the FORWARDING for
`decommit_lazy`; the syscall's own failure remains real-layer-only)". Naturally folded into
whatever edit closes V2C5.

### V2C3 — INFO — item 50's `query_os_page_size()` arms citation `lib.rs:409-445` was exact at `b6bfdac` and is now truncated by exactly the 2 lines task #907 added; the round-9 review had re-verified this very citation as correct on the reasoning that no `src/` file had been touched

**Where:** `docs/CORRECTNESS_OPEN_ITEMS.md:2137` (item 50's U11-half card):

> the guard at `crates/vmem/src/lib.rs:390-406` and `query_os_page_size()`'s three `#[cfg]`
> arms (`:409-445`) are structurally untestable

Verified at both revisions. At `b6bfdac`, `lib.rs:445` is `}` — the closing brace of the miri
arm, so `409-445` covered all three arms **exactly**. At HEAD the three arms span `409-447`
(unix `:409-419`, windows `:421-441`, miri `:443-447`) and `:445` is now the miri arm's
`// Miri has no real OS page; …` comment — the cited range stops two lines short, excluding
`PAGE` and the closing brace. The companion citations in the same bullet (`:390-406`, `:168`)
and in the U10-half bullet (`:206-252`, `:233-236`, `:244-249`, `:282-330`,
`tests/smoke.rs:104-113`, `tests/lazy_commit.rs:71-117`) are all ABOVE the insertion point or in
untouched files and remain correct — I resolved each individually.

**Why this is worth recording despite being two lines.** The round-9 review explicitly
re-verified this exact citation and cleared it, with the stated reasoning "`b39882b` touched no
`src/` file, so no shift occurred" (round-9 review, §"Checked and explicitly NOT findings").
That reasoning was sound for round 8's closing commit and is exactly what round 9's own
remediation then invalidated — the tenth iteration of the campaign's round-N-fix →
round-N+1-finding pattern, and the cleanest possible demonstration that this class is a
mechanical problem, not an attention problem.

**Failure scenario.** Low: a reader of item 50 opens `lib.rs:409-445`, sees the miri arm cut
off mid-body, and has to widen the range by hand. No wrong conclusion is reachable. Recorded
because it is the second instance this round of "a commit invalidates a citation that is not in
its diff" (with V2C1), and that pairing is the load-bearing evidence in the pivot assessment
below.

**Fix:** `:409-447`, or — better, and consistent with what task #901 did for U3 — drop the
range and name the symbol (`query_os_page_size()`'s three `#[cfg]` arms).

### V2C4 — INFO — `mock.rs:288`'s new comment says "the two `decommit_lazy` shapes above", but only ONE of the two calls above is a `decommit_lazy` call; the other is a `decommit` call

**Where:** `crates/vmem/tests/mock.rs:288-290` (task #906's bonus block):

> // task #906 (round-9 review, V2-1 bonus): **the two `decommit_lazy` shapes**
> // above are rejected under EITHER validation base (`page_size()` or
> // `PAGE`), so neither discriminates …

versus the two calls actually above it in the same test: `decommit(base, 1, PAGE)` at
`mock.rs:263` (the misaligned shape) and `decommit_lazy(base, PAGE, 0)` at `:279` (the inverted
shape). The two SHAPES are real and correctly described by the earlier comment at `:272-274`
("cover the `start >= end` … shape alongside the misaligned-offset shape above"); attributing
BOTH of them to `decommit_lazy` is the error.

**Failure scenario.** Mild, and the conclusion the sentence draws is correct regardless (the
single `decommit_lazy` call above genuinely does not discriminate). The realistic cost is a
reader who counts the `decommit_lazy` calls, finds one, and re-derives the whole
non-discrimination argument from scratch to work out whether the comment or the code is wrong.
Recorded chiefly because of what it is: **V2-3 was itself a stale-count comment ("both calls"),
and the commit that fixed it introduced a new inaccurate-count comment in the sibling file** —
the pattern reproducing inside a single commit.

**Fix:** "the two contract-violation shapes above (one `decommit`, one `decommit_lazy`) …" or
simply "the `decommit_lazy` call above".

### V2C5 — INFO — the mock-layer mirror is half-scoped in the mirror image of V2-1: it discriminates `decommit_lazy`'s validation base but not `decommit`'s, while the test's own culminating doc sentence names `decommit`

**Where:** `crates/vmem/tests/mock.rs:288-312` adds a `decommit_lazy(base, PAGE, 2 * PAGE)` arm
and no `decommit(base, PAGE, 2 * PAGE)` arm, versus the test's doc comment at `:241` (which
frames the hazard for the pair: "`lib.rs`'s `decommit`/`decommit_lazy`: `if start >= end || …`")
and `:246-252`, whose final clause narrows to the one function that did NOT get an arm:

> A future contributor could "unify" the validation base from `page_size()` to the crate's
> `PAGE` constant … and the whole test suite would stay green with no test noticing -- this
> locks the silent-skip contract at the `mock` call-log layer: **a misaligned,
> contract-violating `decommit` call must record NO `Call::Decommit` at all**

This is structurally identical to V2-1 — "one of the two named guards discriminated" — with the
two functions swapped. It is INFO rather than LOW for one specific reason I verified rather than
assumed: **the union of the two CI rows on the macOS runner does cover both guards.**
`ci.yml:823` runs the named feature set (no `mock`), where `smoke.rs`'s arm now discriminates
BOTH; `ci.yml:828` runs `--all-features` (with `mock`), where `mock.rs`'s arm discriminates
`decommit_lazy` only. So no real coverage hole exists today — only a per-file claim broader than
that file's scope.

**Failure scenario.** If the `--all-features` macOS row is ever the surviving Darwin row (a
plausible CI-cost trim: it has strictly more tests, 47 vs 42, so it looks like the safe one to
keep), `decommit`'s validation base loses its only discriminating oracle silently, while
`mock.rs`'s doc comment continues to name `decommit` as the thing it locks. Note also that the
`:249` clause "the whole test suite would stay green with no test noticing" now describes a
world that ended in this round's own commit — defensible as motivation-framing ("without this
test, …"), which is why it is folded in here rather than filed separately.

**Fix:** either add the four-line `decommit(base, PAGE, 2 * PAGE)` + `Call::Decommit`-absence
mirror inside the same `if page_size() > PAGE` block (symmetric, and makes the mock row
self-sufficient), or narrow `:250-252` to say the base-swap half is locked for `decommit_lazy`
at this layer and for both at the real-syscall layer in `smoke.rs`.

### V2C6 — INFO (process) — round 9 has no CHANGELOG entry: the NINTH recurrence of item 1, and item 1's own headline and `Current number` bullet still say "eight … (rounds 1-8)"

**Where:** `CHANGELOG.md`'s last `aligned-vmem` section is
`#### aligned-vmem — round-8 follow-up (2026-08-13, tasks #897-903)` at `:425`; there is no
round-9 section (`grep -n "#906\|#907\|V2-1\|V2-2\|V2-3" CHANGELOG.md` → no match; the single
`round 9` hit at `:375` is an unrelated reference to the WIDER campaign's round 9). Item 1's
card: headline at `docs/CORRECTNESS_OPEN_ITEMS.md:63` ("recurred **eight** times … rounds 1-8"),
`Status` bullet at `:74`, `Current number` bullet at `:75` (records the 8th instance as round 8,
caught by UC1), `Evidence` bullet at `:77`.

This was the round-9 review's single most confident prediction ("A ninth CHANGELOG recurrence is
otherwise the single most predictable finding of any round-9 closing review") and it is correct.
Round 9 is a **4th consecutive round** (6, 7, 8, 9) where the round's own remediation tasks did
not write the entry and only the closing review caught it — which is the strongest evidence yet
for the standing rule item 1 has been proposing since round 4, since the within-round catch has
now failed four times running.

**Fix:** write the round-9 entry in this round's closing pass (2 tasks, 2 merges: `486a97b`
task #906, `3900828` task #907 — both SHAs verified against `git log`), and update item 1's
headline to "nine … (rounds 1-9)", its `Current number` bullet with the 9th instance, and its
`Evidence` bullet with this document. Per the R34-24 current-state rule, the headline and the
bullet must move together — round 8's U8 was exactly the failure of letting them drift apart.

### V2C7 — INFO (process) — `docs/reviews/2026-08-13-aligned-vmem-round9-review.md` is untracked

**Where:** `git status --porcelain` → `?? docs/reviews/2026-08-13-aligned-vmem-round9-review.md`
(the only untracked entry in the tree; `git ls-files docs/reviews/ | grep round[89]` returns
both round-8 docs and neither round-9 doc).

Item 2 (`docs/CORRECTNESS_OPEN_ITEMS.md:79-84`), settled last round precisely to stop this
question being re-investigated, states this campaign commits its review docs because its own
index re-cites them by path across rounds. Round 9's review is already cited by path in this
document and will be cited again by the CHANGELOG entry V2C6 owes.

**Fix:** `git add` both round-9 review docs (the review and this closing review) in the same
closing commit, as `7c6e4be` / `e60e46a` / `1dbd6b4` / `8380607` / `b39882b` all did.

---

## Checked and explicitly NOT findings

Recorded so a future pass (or the resolver script) does not re-derive them.

- **The net diff contains no stray edit.** `git diff b6bfdac..HEAD --name-only` returns exactly
  `crates/vmem/src/lib.rs`, `crates/vmem/tests/mock.rs`, `crates/vmem/tests/smoke.rs`, matching
  the union of the two task commits' own `--stat`. Both merges are genuine two-parent merges off
  `b6bfdac` with no conflict residue.
- **V2-2's replacement wording is accurate, if less specific than what it replaced.**
  `lib.rs:435-437` now says "since task #897 removed the `align > page_size() &&` conjunct, the
  reserve fast path no longer consults `page_size()` at all". The original named "the Unix-only
  `try_reserve_aligned_exact`"; the replacement drops the platform qualifier while sitting
  inside the WINDOWS `query_os_page_size` arm, two lines above a sentence about the "Windows
  single-call reservation fast path". I checked whether that ambiguity can produce a false
  belief and it cannot: no reserve path on EITHER platform consults `page_size()` (the Windows
  paths use `WIN_ALLOCATION_GRANULARITY`; the Unix path lost its only consultation in U1), so
  both readings of "the reserve fast path" yield a true statement. Precision loss, no defect.
- **The pre-existing "fires only when `query_os_page_size()` is called, which happens on the
  cold path (decommit/decommit_lazy)" framing** omits that `page_size()` is itself `pub` and
  reachable directly by any consumer (and by this crate's own tests, including the two new arms).
  Pre-existing wording, unchanged in substance by this round, and harmless — the sentence is
  about which INTERNAL paths reach the `debug_assert`.
- **`CHANGELOG.md:417`'s `lib.rs:1131` and item 50's/`CORRECTNESS_OPEN_ITEMS.md:1864`'s
  `lib.rs:2239`/`:2250`, `:2130`'s `lib.rs:2379-2380`** all shifted by this round's `+2` too,
  and none is a finding: all three are past-tense HISTORICAL records of where a fix landed at
  the time ("the `mmap` call **formerly** at `lib.rs:2379-2380`"), and `:2239`/`:2250` was
  already far out of date at `b6bfdac` (`:2239` there is `/// Select the lazy-decommit madvise
  advice…`, not the `.map()` closure task #851 fixed). Item 49's card explicitly declines to
  carry fresh line numbers for exactly this reason, which is the right posture. `lib.rs:531`
  citations at `:1955`/`:1974` are `crates/numa/src/lib.rs`, not this crate.
- **`tests/smoke.rs:104-113` and `tests/lazy_commit.rs:71-117`** (item 50's U10 half) are
  unaffected: task #906's insertions in `smoke.rs` begin at `:582`, far below `:113`, and
  `lazy_commit.rs` was not touched.
- **No test was added or removed, and none was silently disabled.** 42/47 exactly matches
  rounds 8 and 9 — the correct signature for two in-place test-body extensions. `mock.rs` 10 and
  `smoke.rs` 20 in `--all-features`; `mock.rs` 0 (file `#![cfg(feature = "mock")]`-gated) and
  `smoke.rs` 20 in the named row.
- **Both new `unsafe {}` blocks are correctly scoped and correctly commented**, and neither
  introduces a new `unsafe` seam: they wrap calls to `aligned_vmem::decommit_lazy`, already an
  `unsafe fn`, from `tests/`. No safe `pub fn` taking a raw pointer was added anywhere (the
  CLAUDE.md benchmark-hook rule's trigger shape); no `src/` code path changed at all this round
  beyond one comment.
- **Counter contamination between the two `SERIAL`-locked smoke tests.** The modified test holds
  `SERIAL` (`smoke.rs:552`) and calls `reset_bench_internals_counters()` (`:562`) after
  acquiring it; `macos_decommit_madvise_syscall_actually_succeeds`'s `attempts == 2` assertion
  (`:473` lock, `:491` reset, `:510`) is under the same lock with its own reset. The new arm's
  calls are rejected anyway (attempts stays 0 on the correct guard), so contamination is
  impossible in both the passing and the failing direction.
- **`mock::reset()` inside the new mock arm** (`mock.rs:298`) wipes the log mid-test, but the
  arm is the LAST block in the function and drains immediately; nothing after it reads the log,
  and `r`'s `Drop` fires afterwards with no assertion depending on it.
- **`page_size` and `PAGE` were already imported in `mock.rs`** (`:7-10`), so the new arm added
  no import and no unused-import risk; clippy is clean on all four rows including the Linux
  cross-compile row that type-checks the Unix arm the real path uses.
- **`docs/perf/OPEN_ITEMS.md` is untouched and correctly so** — nothing in V2-1/V2-2/V2-3 is a
  performance item, and this round changed no syscall count on any path (both new calls are
  rejected before any syscall on every correct platform, and are compiled-in-but-skipped on
  every 4 KiB host).

---

## Categories with nothing to report

- **Memory safety / UB — null.** No `src/` code path changed; the only `src/` hunk is a comment.
  The two new test call sites pass an in-bounds sub-range of a live reservation and are rejected
  before any OS effect on every correct platform; even in the deliberately-broken (swapped-guard)
  world the tests are designed to detect, the forwarded `madvise` covers `[base+4096, base+8192)`
  inside a live 4 MiB / 2 MiB mapping, so the failing assertion is reached without UB.
- **Performance — null, tenth consecutive round.** Zero shipping-code change; zero syscall-count
  change on any path; no counter, storage or `use` left an existing feature gate.
- **Semver / API surface — null.** No public item added, removed, renamed or re-`cfg`-ed;
  `Cargo.toml` untouched since round 7. `cargo package`'s file set cannot have changed (no file
  added or deleted).
- **Error contracts — null.** `error.rs` and the `VmemError` bridge were not touched and no
  claim about them changed.

---

## Recommended order

1. **V2C1** — replace `smoke.rs:74`'s range+offset citation with symbol names (and give
   `:79`'s `huge_pages.rs:61-62` the same treatment while there). The only finding whose citation
   no longer contains its own target.
2. **V2C2 + V2C5** — one edit, same neighbourhood: correct `smoke.rs:541-542`'s "no way to
   observe" clause and either extend `mock.rs`'s mirror to `decommit` or narrow `mock.rs:250-252`
   to its real scope. Doing V2C5 by ADDING the `decommit` arm makes V2C2's fix shorter, since the
   claim then becomes uniformly false rather than half-false.
3. **V2C4** — three words in `mock.rs:288`.
4. **V2C3** — `:409-445` → symbol name in item 50.
5. **V2C6 + V2C7** — the round-9 CHANGELOG entry, item 1's counter (eight → nine), and
   `git add` for both round-9 review docs. Process, not findings, but they are the difference
   between this round closing clean and a hypothetical round-10 opening with the tenth
   CHANGELOG recurrence.

All five are comment/citation/process edits. If the campaign ends here (see below), items 1–4
are still worth landing in the closing commit precisely because they are the last chance to
correct text that no future scheduled read will revisit.

---

## Evaluating the round-9 PIVOT recommendation

The brief asked for an explicit, independent verdict. Mine:

**Agree with the pivot — end the scheduled full-read cycle — but CONDITIONALLY: the
citation-resolver script must move from "optional infrastructure item #4" to a precondition of
the pivot, because this closing review found the one defect class that the proposed replacement
(diff-scoped review) structurally cannot catch.**

**Where the evidence supports the round-9 review's argument.** Round 9's full read of 2,653
lines produced zero pre-existing code defects, and its remediation was 50 net lines across three
files, every one of them a comment or a test call. My verification found both fixes correct on
the merits — V2-1's discrimination holds in both directions at both layers, V2-2's premise is
mechanically true. Five of my seven findings are again in one-commit-old text (V2C2, V2C4,
V2C5) or process residue (V2C6, V2C7). Three consecutive rounds of full re-reading have now
found nothing in code that pre-dates the previous round's own remediation. The find curve the
round-9 review published is real and I did not manufacture a counterexample to it.

**Where I have to qualify the round-9 review's "zero pre-existing defects" claim.** V2C1 IS a
pre-existing defect. It was accurate when written at `e496071`, drifted at `8380607`, and then
survived the round-8 full read AND the round-9 full read — both of which read every file in
`tests/`, and the round-9 review of which explicitly checked a NEIGHBOURING citation
(`huge_pages.rs:61-62`) in the same comment. It is small, and it does not undermine the pivot;
it sharpens WHICH replacement is adequate. Two independent human full reads missed a stale line
citation. A tenth would probably miss it too. A twenty-line resolver that resolves every
`file.rs:NNN` in the repo and checks the target still looks like what the citing text says would
find it in milliseconds, forever.

**The structural argument the round-9 review did not quite make.** The pivot proposes replacing
scheduled full reads with diff-scoped reviews of commits touching `crates/vmem`. That covers
V2C2, V2C4 and V2C5 (all in or adjacent to this round's diff) but **misses V2C1 and V2C3
entirely** — both are citations located OUTSIDE the diff that the diff invalidated by shifting
lines beneath them. That is not an incidental gap: it is the campaign's single most frequent
finding class (F11, T5, TC4, U2, U3, U8, V2-2, now V2C1 and V2C3), and its defining property is
that the defect and the commit that causes it are never in the same file region. Diff-scoped
review is exactly the wrong instrument for it; a whole-tree resolver run in CI is exactly the
right one. Adopting the pivot WITHOUT the resolver would trade a mechanism that catches this
class slowly and unreliably for one that cannot catch it at all.

**Concretely, what I would adopt:**

1. **End the scheduled full-crate reads.** Round 9 and this closing review both support it.
2. **Build the ~50-line citation resolver FIRST**, and run it in CI (it is cheap enough for the
   per-PR path). It would have caught U2/U3/U8 in round 8, V2-2 in round 9, and V2C1/V2C3 here —
   and it is the only proposed mechanism that catches the class at all after the pivot.
3. **Keep diff-scoped zero-trust review** of any commit touching `crates/vmem`, per the
   workspace's existing per-phase convention. It would have caught V2C2/V2C4/V2C5.
4. **Prioritise the Linux `bench-internals` non-`mock` CI row**, which the round-9 review
   correctly identifies as the highest-leverage infrastructure item (four beneficiaries). I add
   a fifth, from this round: both of task #906's new arms are currently live on exactly ONE
   host in the world's CI (the macOS runner), so the entire value of this round's remediation
   rests on a single row continuing to exist. A second `page_size() > PAGE` host would make that
   robust — and if none is available, that fact deserves recording in item 48 rather than being
   rediscovered later.
5. **One final pre-publish pass tied to task #658**, which is a different kind of review
   (packaging surface, docs.rs render, README-vs-crate parity) and is not what this campaign has
   been doing.

**One caveat on timing, which is not an argument against the pivot but is an argument against
declaring it done today.** Round 9 is unpushed, and both new discriminating arms are skipped on
every 4 KiB host — including this one. Neither has ever executed. The round-8 precedent is
directly relevant: UC4's arm was only confirmed genuinely working after its landing SHA's macOS
job was read from the real GitHub API. The same confirmation is owed here before the campaign
closes, otherwise the campaign's final act ships two test arms whose only evidence of working is
a derivation on paper — mine included.
