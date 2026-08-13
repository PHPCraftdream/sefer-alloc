# `aligned-vmem` — round-8 CLOSING review (verification of the U1–U11 remediation)

**Date:** 2026-08-13

**Scope:** verification of the seven remediation tasks (#897–#903, letters A–G) that closed
`docs/reviews/2026-08-13-aligned-vmem-round8-review.md`'s findings U1–U11, plus the seven
`--no-ff` merge commits that landed them. Every file in the round's diff
(`git diff 8380607..70df07e --stat`: 6 files, +196/−23) and the code each of those changes
makes a claim about. Like rounds 6 and 7, this round was delegated to independent sub-agents in
seven isolated git worktrees (`vmem-r8-a` … `vmem-r8-g`, all branched from `8380607`), then
merged sequentially.

**Reviewed tree:** local `main` @ `70df07e` (the task #903 merge). `git fetch` +
`git log origin/main..HEAD --oneline | wc -l` → **14** — the 7 merge commits plus the 7 task
commits they carry; `git rev-parse origin/main` → `8380607778dca5489e97d9ec8dd483d0f384dfd0`.
**None of round 8 has been pushed**, so there is no new CI signal for this round's own diff.

`git status --porcelain` shows exactly four untracked entries — three pre-existing checkpoints
(`docs/checkpoints/2026-08-13-{0130,1500,1730}.md`) and
`docs/reviews/2026-08-13-aligned-vmem-round8-review.md`. That fourth one is **UC2**.

**Toolchain / host:** `rustc 1.97.0`, stable-x86_64-pc-windows-msvc; Windows 10 Pro, 4 KiB
page. **No Darwin host and no Darwin target** — every Darwin claim below is reasoned from spec
or read from already-published CI logs, never executed here. Unlike the three prior closing
reviews, this one *did* exercise a Unix target: `x86_64-unknown-linux-gnu` is installed on this
host and was used to type-check and lint the arm that round 8 actually changed (see UC5).

**Nature:** read-only. Nothing in the repository was modified other than the creation of this
document. No `git add` / `git commit` / `git push` / branch, worktree or ref mutation. Every
command quoted below was executed on this host; every `file:line` citation was read in the
current tree before being written down.

**Finding prefix:** `UC` (round-8 closing). Prior prefixes deliberately not reused: `V`/`W`/`P`
(rounds 1–2), `F` (round 3), `R`/`CR` (round 4 + closing), `Q`/`QC` (round 5 + closing),
`S`/`SC` (round 6 + closing), `T`/`TC` (round 7 + closing), `U` (round 8).

---

## Verdict up front

**All eleven of U1–U11's fixes landed, and all eleven landed on the intended content.** Not one
edit hit adjacent unrelated text; every approximate line number the sub-agents were handed
resolved correctly, and the round's one new tracked index item (item 50) carries seven `file:line`
citations that I resolved individually — all seven are correct in the post-merge tree. That is
the best mis-citation record any round of this campaign has produced.

**U1 — the round's headline — is correct, and I verified it independently rather than by
reading the task's own argument.** The removed conjunct's invariant claim is true, removing it
generalizes the check correctly with *no* new failure mode on any correct platform, and I proved
the one thing the fix's own commit message did not: that `align` reaching
`try_reserve_aligned_exact` is always a power of two `>= PAGE`, so the newly-unconditional
`is_multiple_of(align)` cannot fire spuriously (`align == 0` and non-power-of-two `align`, the
two shapes that *would* have changed behaviour, are unreachable). Full derivation in
§"U1 under the microscope". **The fix ships without regression-test backing, and I conclude that
was the right call but for a reason the task did not state** — see UC6.

**The campaign's signature pattern held for an eighth time, but weakly and only on process.**
Round 8's own remediation produced six new items, and — for the first time in the campaign —
**none of them is a wrong citation, a contradicted claim, or a half-swept scope**. The three
recurrences are:

1. **The round has no CHANGELOG entry** (**UC1**) — the eighth recurrence of the gap item 1
   exists to track, whose own counter this very round updated to "seven". Round 8's
   decomposition, like round 7's, had no dedicated CHANGELOG task.
2. **The round-8 review doc is not committed** (**UC2**) — TC2 recurring exactly one round
   later, with three committed lines of item 50 citing it by path. This one has a wrinkle worth
   settling rather than re-finding: the repo now contains two *contradictory* conventions about
   whether readonly review docs get committed.
3. **A card written in one worktree is stale on arrival because a different worktree's fix
   landed first** (**UC3**) — item 50's U11 half still asks "if round 8's U1 fix … lands", five
   merges after it landed. The textbook worktree-isolation failure, in its mildest form yet: a
   verb tense, not a contradiction.

The one genuinely technical finding is **UC4**: task F's new smoke test is real and useful, but
its doc comment names a counterfactual — the `page_size()` → `PAGE` validation-base swap — that
the offsets it actually passes cannot detect. That is round 7's T2 shape (a test whose doc
comment claims a regression-guard the assertion cannot deliver) recurring one round later, in
the very test written to close a *coverage* finding.

**Nothing found here is a soundness, correctness, or performance defect.** Round 8's only
runtime change is U1's, and it is verified correct. Publish-readiness (task #658) is not newly
blocked: `cargo package -p aligned-vmem --list` still returns the same 20-file set, and U6's
publish-facing attribution landed on both surfaces with numbers that match their source card
exactly.

---

## What was verified green — every command below was executed on this host

```
$ git fetch && git log origin/main..HEAD --oneline | wc -l
14                                      # 7 merges + 7 task commits; origin/main == 8380607

$ cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" \
      --no-fail-fast
fault_injection 5 / huge_pages 1 / lazy_commit 11 / min_page 2 / mock 0 /
smoke 20 / vmemerror_io_bridge 3 / doc-tests 0        => 42 passed, 0 failed

$ cargo test -p aligned-vmem --all-features --no-fail-fast
0 / 0 / 1 / 11 / 2 / 10 / 20 / 3 / 0                   => 47 passed, 0 failed

$ cargo clippy -p aligned-vmem --all-targets -- -D warnings                           -> clean
$ cargo clippy -p aligned-vmem \
    --features "lazy-commit huge-pages fault-injection bench-internals" \
    --all-targets -- -D warnings                                                      -> clean
$ cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings            -> clean
$ cargo fmt -p aligned-vmem --check                                                   -> clean

$ node scripts/vmem-doc-drift-guard.mjs
[vmem-doc-drift-guard] OK: no unconditional over-reserve/trim statements found  (exit 0)

$ grep -rnE '^(<<<<<<<|=======|>>>>>>>)$' crates/vmem/ docs/CORRECTNESS_OPEN_ITEMS.md \
      docs/perf/OPEN_ITEMS.md CHANGELOG.md
(no output)                             # no leftover conflict markers from the 7 merges

$ cargo package -p aligned-vmem --list --allow-dirty
20 files: .cargo_vcs_info.json, Cargo.{lock,toml,toml.orig}, LICENSE-{APACHE,MIT},
README.md, benches/vmem_bench.rs, examples/v20_849_unix_exact_reserve_hit_rate.rs,
src/{error,fault_injection,lib,mock}.rs, tests/*.rs (7)
                                        # identical to rounds 6/7; still no docs/

# --- NOT part of the standard matrix; added by this review, see UC5 ---
$ cargo check  -p aligned-vmem --target x86_64-unknown-linux-gnu \
    --features "lazy-commit huge-pages fault-injection bench-internals" --all-targets
Finished `dev` profile in 4m 19s                                                      -> clean
$ cargo clippy -p aligned-vmem --target x86_64-unknown-linux-gnu \
    --features "lazy-commit huge-pages fault-injection bench-internals" \
    --all-targets -- -D warnings
Finished `dev` profile in 5m 08s                                                      -> clean
```

**Test counts moved this round, correctly and in exactly one place.** The named-feature row is
42, identical to rounds 6/7. The `--all-features` row is **47**, up from round 7's 46: `mock.rs`
went 9 → 10 with task F's `decommit_silently_skips_contract_violating_offsets`. Task F's *second*
new test (`decommit_contract_violation_never_reaches_madvise`, `smoke.rs:549`) is
`#[cfg(all(unix, feature = "bench-internals", not(feature = "mock"), not(miri)))]`, so it is
compiled out on this Windows host in both rows and out of the `--all-features` row on every host
(`mock` is on there) — it will first execute on the macOS CI row, which is the only Unix +
`bench-internals` + non-`mock` row that exists. `Doc-tests aligned_vmem … 0 passed` in both
rows: the no-doctests convention holds.

**Surface checks on the round's `src/` diff, all negative as claimed:**

```
$ git diff 8380607..70df07e -- crates/vmem/src | grep -E '^\+.*\bunsafe\b'      (no output)
$ git diff 8380607..70df07e -- crates/vmem/src | grep -E '^[+-]\s*pub '         (no output)
$ git diff 8380607..70df07e -- crates/vmem/src | grep -E '^[+-]\s*#\[cfg'       (no output)
$ git diff 8380607..70df07e --name-only
crates/vmem/README.md  crates/vmem/src/lib.rs  crates/vmem/tests/{lazy_commit,mock,smoke}.rs
docs/CORRECTNESS_OPEN_ITEMS.md                 # Cargo.toml untouched: no feature-composition change
```

No new `unsafe` token, no public item's signature changed, no `#[cfg]` on any shipping item
changed, no feature composition changed. `docs/perf/OPEN_ITEMS.md` was not touched at all this
round (U6 quotes item 46 rather than editing it — correct: item 46's card was already accurate;
what was wrong was the two surfaces that dropped its qualifiers).

---

## Round-8 pass (`8380607..70df07e`) — U1–U11 verification

Checked before looking for anything new, because eight consecutive rounds have found the closing
fix to be the next round's bug source.

| # | Status in the current tree | Evidence |
|---|---|---|
| **U1** | **CLOSED — and independently re-derived, not taken on trust** | `lib.rs:2141` is now `if !region_addr.is_multiple_of(align) {`; the `align > page_size() &&` conjunct is gone, replaced by a 24-line comment (`:2117-2140`) that states the hazard, cites item 43's BSD half, records the zero-syscall measurement, and explains why this is a runtime check rather than a `debug_assert!` (CLAUDE.md's R26-4 rule, named explicitly). Item 43's second half landed too: `docs/CORRECTNESS_OPEN_ITEMS.md:1919-1933` appends the misalignment consequence to the `Current-number-or-verdict` bullet, reading coherently with the round-7-era decommit-rounding text immediately above it. Full analysis below. |
| **U2** | **CLOSED, and correct this time** | `smoke.rs:356` and `lazy_commit.rs:340-347` both now name `decommit_recommit_roundtrip`. I read that test's body: `smoke.rs:179-233`, and its zero-fill read at `:219-231` really is `#[cfg(not(any(miri, feature = "mock", target_os = "macos", target_os = "ios", target_os = "tvos", target_os = "watchos")))] assert_eq!(base.add(span / 2).read(), 0, …)`. Both comments also preserve the correction trail (naming the two prior wrong names), which is the right call for a citation that has now been wrong twice. |
| **U3** | **CLOSED** | `docs/CORRECTNESS_OPEN_ITEMS.md:2116`'s S9 bullet now reads "synchronized with `decommit_lazy`'s rustdoc and `madv_free_advice`'s doc comment". Both symbols exist and are unique: `pub unsafe fn decommit_lazy` at `lib.rs:1153`, `fn madv_free_advice` at `lib.rs:2251` — and both still carry the synchronized tvOS/watchOS wording after all six other round-8 edits landed on top (this is the specific cross-task check the brief asked for). Symbol names are also immune to the +33-line shift U1's own hunk introduced below `lib.rs:2114`, which is precisely why TC7's precedent was the right one to follow. |
| **U4** | **CLOSED** | `lib.rs:1624-1625` now reads `// SAFETY: fresh anonymous reserve+commit at a kernel-chosen address; NULL is checked below.` — accurate for a `VirtualAlloc(NULL, commit_len, MEM_RESERVE \| MEM_COMMIT, …)` issued after the preceding call returned `NULL`. The genuine sibling proof on the two-call path is untouched. |
| **U5** | **CLOSED, on both surfaces** | `lib.rs:1115-1119` inserts the Windows clause into `decommit_lazy`'s benign-write paragraph, and `README.md:49`'s table row carries the cheap version. I checked the cross-reference actually resolves: `decommit`'s platform-divergence paragraph is at `lib.rs:1047-1056`, above `decommit_lazy` as claimed, and it does record an incident (item 6, "already crashed an in-repo consumer"), so "see … for the incident this already caused" is not a dangling promise. |
| **U6** | **CLOSED, with the numbers re-checked against their source** | `lib.rs:851-853` and `README.md:40` both gained "measured on WSL2/Linux, x86_64; 30-run aggregate — the hit rate is kernel- and ASLR-dependent and is not expected to transfer to other Unix platforms". Verified against `docs/perf/OPEN_ITEMS.md:1130-1136`: the card says "30-run aggregate on WSL2 (Hyper-V-backed kernel)" and 34.375% / 46.6667% / 56.6667%, which round correctly to the published 34.4 / 46.7 / 56.7. No number was invented in transit. |
| **U7** | **CLOSED on coverage; the new test's own doc comment overclaims** | Two tests added, not one: `mock.rs:255` (`decommit_silently_skips_contract_violating_offsets`, runs in the `--all-features` row) and `smoke.rs:549` (`decommit_contract_violation_never_reaches_madvise`, Unix + `bench-internals` + non-`mock`). Both are non-vacuous against the "delete the guard" and "record-then-reject" mistakes. Neither can detect the `ps` → `PAGE` swap its doc comment names → **UC4**. |
| **U8** | **CLOSED** | `docs/CORRECTNESS_OPEN_ITEMS.md:63` now reads "This has recurred **seven** times across the aligned-vmem campaign alone (rounds 1-7; see the Current-number bullet for the per-round breakdown…)" and defers the count to `:75` rather than duplicating it — a strictly better structure than the review proposed, since only one line now has to move per round. It has to move again immediately → **UC1**. |
| **U9** | **CLOSED, and one step further than filed** | `README.md:172-176` narrows the claim back to "in the public API" and adds "this crate's own `tests/` files, which are not part of the public API, still use `as usize` at a few sites". Task G additionally converted `smoke.rs:148` to `base.addr() % span`, which was the sharpest instance in the finding (the `lib.rs:72` "Runnable form: `tests/smoke.rs`" pointer no longer lands on a different idiom than the doc example). 8 pointer casts remain in `tests/` (`huge_pages.rs:52`, `:118`; `lazy_commit.rs:24`, `:286`, `:287`; `smoke.rs:593`, `:693`, `:694`) — "a few sites" is a fair description of 8. |
| **U10** | **CLOSED as record-only** | Item 50's U10 half. All four `file:line` citations resolve: `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` / `_TWO_CALL_PAIRS` at `lib.rs:239` / `:252` (inside the cited `:206-252`), the two rustdoc claims at `:234-236` and `:245-249`, the five accessors inside `:282-330`, and `grep -rn "windows_reserve_commit" crates/vmem/tests/` still returns nothing. |
| **U11** | **CLOSED as record-only; the card is stale on arrival** | Item 50's U11 half. Citations resolve (`PAGE_SIZE_CACHE` at `lib.rs:168`, the guard at `:390-406` with the acceptance test at `:400`, `query_os_page_size`'s arms at `:409-445`). But the card describes U1's fix as pending → **UC3**. |

All four R34-24 current-state fields are present on the new item 50 (`Status` `:2128`,
`Current-number-or-verdict` × 2 `:2129`/`:2130`, `Next trigger` `:2132`, `Evidence` `:2133`),
plus a supernumerary `Why not fixed now` bullet at `:2131` that is a genuine improvement on the
convention rather than a deviation from it.

---

## U1 under the microscope

The brief asked for this one to be adjudicated skeptically and independently. Four questions,
answered in order.

### 1. Was the removed conjunct's invariant claim true, and does removing it generalize correctly?

**The claim was true, and the generalization is correct.** The old code was:

```rust
// lib.rs @ 8380607
if align > page_size() && !region_addr.is_multiple_of(align) { … }
```

`region_addr` is `mmap`'s return address, which POSIX guarantees is a multiple of the **real**
OS page size. So for any power-of-two `align <= real_page_size`, `region_addr.is_multiple_of(align)`
is provably true and the second conjunct is provably false — the skip was genuinely eliminating
a dead branch, exactly as its comment said. The claim is false only where
`page_size() > real_page_size`, i.e. where `sysconf(_SC_PAGESIZE)` returns a power-of-two value
above the real page size and `page_size()`'s acceptance guard (`lib.rs:400`:
`queried >= PAGE && queried.is_power_of_two()`) lets it through. That is item 43's still-open
BSD half. U1's diagnosis is sound.

Removing the conjunct generalizes correctly **iff** the surviving predicate cannot fire in a
case the conjunct used to suppress legitimately. There are exactly two such shapes —
`align == 0` (where `usize::is_multiple_of(0)` returns `self == 0`, so the check would fire on
every non-null address) and non-power-of-two `align` (where a page-aligned address need not be
an `align` multiple). **Both are unreachable**, and I traced this rather than assuming it:

- `validate_size_align` (`lib.rs:866`) rejects unless `size != 0 && align.is_power_of_two() &&
  align >= PAGE && size.is_multiple_of(PAGE)`.
- It is called on **every** public entry point that can reach the Unix reserve path:
  `try_reserve_aligned` (`:945`), `try_reserve_aligned_lazy` (`:1368`),
  `try_reserve_aligned_huge` (`:1475`).
- Those three are the only callers of `reserve_aligned_raw` / `reserve_aligned_lazy_raw` /
  `reserve_aligned_huge_raw` (`:961`, `:1393`, `:1395`, `:1486`), which are the only callers of
  `unix_reserve` (`:1929`, `:2236`), which is the only caller of `try_reserve_aligned_exact`
  (`:1994`).
- The one internal shortcut, `leak_zeroed_pages` (`:1514-1519`), goes through the *public*
  `reserve_aligned(rounded, PAGE)` and is therefore validated too.

So at `lib.rs:2141`, `align` is always a power of two `>= PAGE`. The check is a no-op on every
correct platform and fires only in the exact case U1 identified. **No new failure mode.**

### 2. Any different failure mode, edge case, or performance regression the task missed?

One behavioural consequence exists that the task's comment does not spell out, and it is the
*correct* trade rather than a defect: on a hypothetical over-reporting platform, every
`reserve_aligned` with `align` in `(real_page_size, page_size()]` now **misses** the fast path
instead of silently succeeding misaligned. `unix_reserve` (`lib.rs:1993-1997`) discards the
error and falls through to the over-reserve path, which computes `align_up_addr` explicitly and
is correct for any `page_size()` value — so the caller still gets a correctly-aligned
reservation, at a cost of 2 extra syscalls (`munmap` + `mmap(size + align)`) and `align` bytes
of VA held for the reservation's lifetime. Trading throughput for a documented guarantee on a
platform whose page-size constant is wrong is unambiguously right, and it is worth noting only
because a future perf round measuring BSD hit rates should expect this, not be surprised by it.

On correct platforms the cost is one `AND`/`TEST` on a path that has just issued an `mmap`
syscall. The comment's "there is no performance cost to always running the check" is a
rounding-to-zero, not a measurable claim — I am not filing it; the substantive claim it rests on
(measurement #849's 480/480 page-size-mode hits) checks out against
`docs/perf/OPEN_ITEMS.md:1131`'s "4 KiB = 480/480 (100%)".

I also checked the huge path: `unix_reserve`'s hugetlb guard (`:1986-1992`) requires `size` and
`align` to both be `LINUX_HUGE_PAGE_SIZE` multiples before `huge` reaches
`try_reserve_aligned_exact`, and `MAP_HUGETLB` returns a huge-page-aligned address, so the
newly-unconditional check is even more trivially satisfied there.

### 3. Regression-test backing — is "no new test" acceptable?

**Yes, but not for the reason the task gave**, and the residual gap is worth recording — see
**UC6**. The short version: a test asserting the *fixed* behaviour is structurally impossible
without a `page_size()` injection seam, which is item 50's U11 half and which CLAUDE.md's
benchmark-hook rule specifically discourages building. A test asserting the *unchanged*
behaviour is possible but would never run in this repo's CI. I am not asking round 8 to add one.

### 4. Does the diff match what was promised?

Yes, byte for byte. `git show e0dbe85` removes exactly the `align > page_size() && ` prefix and
the five-line invariant comment, adds the 24-line replacement comment, and appends 14 lines to
item 43. The commit message's every factual claim — the R26-4 citation, the 480/480 figure, the
"four still-open BSD targets" scope — is accurate against the tree. The one imprecision is in
the commit message only, not the code: it says verification was "cargo test --all-features, 45
tests" where the current tree gives 47; that is a mid-round count from the worktree before tasks
F/G merged, and is harmless.

---

## Category 1 — process recurrences

### UC1 — MEDIUM (process) — round 8 has no CHANGELOG entry: the eighth recurrence of the gap item 1 exists to track, and item 1's own counter — rewritten by this very round's U8 fix to say "seven" — is stale again the moment the round closes

**Where:** `CHANGELOG.md` (the newest `aligned-vmem` section is
`#### aligned-vmem — round-7 follow-up (2026-08-13, tasks #888-894)` at `CHANGELOG.md:401`);
`docs/CORRECTNESS_OPEN_ITEMS.md:63` and `:75`.

`git diff 8380607..70df07e --name-only` lists six files; `CHANGELOG.md` is not among them.
Round 8 landed 14 commits (7 task + 7 merge), including the campaign's first runtime change in
three rounds, and produced no CHANGELOG section. Item 1's `Current number` bullet (`:75`) still
ends at "round 7 (tasks #888-894) is a **7th instance**, caught by TC1 …"; the headline (`:63`)
says "seven times … (rounds 1-7)".

The sting this round is sharper than usual: **U8 was the finding about that exact counter being
stale, and task E fixed it — four merges before the round ended without a CHANGELOG entry that
makes it stale again.** Task E's fix is nonetheless the right shape and deserves credit: it
moved the per-round breakdown out of the headline and into a single bullet, so closing this
needs one edit rather than two.

**Failure scenario.** A round-9 session performs CLAUDE.md's mandatory round-start read, sees a
headline saying seven and a Current-number bullet ending at round 7, and concludes the gap did
not recur in round 8 — that the closing-review catch mechanism finally held. It did not; it is
holding right now only because this document exists, which is precisely the fragility item 1's
own text argues makes the standing rule necessary ("the closing review is itself optional
per-round").

**Fix:** write `#### aligned-vmem — round-8 follow-up (2026-08-13, tasks #897-903)` with a
bullet per task citing the seven real merge SHAs (`491afe9`, `a469643`, `2195ad2`, `ccc017e`,
`f654bda`, `f90a8a4`, `70df07e` — all verified against `git log` here), tagging U1's bullet
`[fix(perf)]` or `[correctness fix]` and the rest `[docs]`/`[test]` per the CHANGELOG's own
convention; then extend `:75` with the 8th instance and `:63`'s "seven"/"rounds 1-7" to
"eight"/"rounds 1-8". Round 8 is **the fourth consecutive round** whose remediation contained no
CHANGELOG task — that is now a stronger argument for the standing CLAUDE.md rule item 1 proposes
than any single instance was.

### UC2 — LOW-MEDIUM — the round-8 review doc is untracked while three committed lines cite it by path; TC2 recurring one round later, on top of a genuine convention conflict this campaign should settle rather than re-discover

**Where:** `docs/reviews/2026-08-13-aligned-vmem-round8-review.md` (untracked — `git ls-files
docs/reviews/` lists rounds 4–7 but not 8; `git status --porcelain` shows `??`); the citations at
`docs/CORRECTNESS_OPEN_ITEMS.md:2127`, `:2130`, `:2133`.

Item 50 — the round's own new tracked item, the durable record a fresh session inherits — cites
the uncommitted doc three times, including in its `Evidence` field, which is the one field whose
entire purpose is to be resolvable later. `git log --oneline -- docs/reviews/2026-08-13-aligned-vmem-round8-review.md`
resolves to nothing.

**This is not a straight repeat of TC2, and that is the part worth recording.** Two conventions
now coexist in this repository and contradict each other for exactly this artifact class:

- Round 6's closing commit `1dbd6b4` committed both of its review docs, round 7's TC2 made
  committing them the explicit resolution, and `8380607` duly committed rounds 6 and 7's four
  docs — the convention this campaign has been following since round 6.
- `CHANGELOG.md:14` (R34-2, task #521) records the *opposite* as "this project's established
  convention that readonly review reports stay uncommitted local artifacts", and describes
  `git rm --cached`-ing two review reports as a self-correction for having committed them.

Both cannot be right, and the cost of leaving it unsettled is that this finding recurs every
round with a 50% chance of being "fixed" in the direction the other convention forbids.

**Failure scenario.** A round-10 reader opens item 50 to decide whether to fund the Windows
counter-coverage work, follows the `Evidence` pointer for the reasoning behind "record-only, not
a defect", and finds nothing — the file exists only in whatever working tree the round-8
reviewer used. Meanwhile a round-9 session that reads `CHANGELOG.md:14` first may `git rm
--cached` rounds 4–7's docs, breaking eleven older citations at once.

**Fix (in this order):** (a) decide the convention explicitly, in one place — the natural home is
a line in `docs/CORRECTNESS_OPEN_ITEMS.md` item 1's neighbourhood or CLAUDE.md itself; (b) if
"commit them" wins, `git add` this round's two review docs alongside the CHANGELOG entry UC1
asks for; (c) if "keep them local" wins, item 50's three citations must be rewritten to cite
task numbers and inline the reasoning rather than pointing at a path that will never resolve.

### UC3 — LOW — item 50's U11 half is stale on arrival: it states in the present tense that `page_size()`'s guard is load-bearing for the alignment guarantee, and asks "if round 8's U1 fix … lands" — five merges after it landed, in the same round

**Where:** `docs/CORRECTNESS_OPEN_ITEMS.md:2130` (the `Current-number-or-verdict, U11 half`
bullet) and `:2132` (the `Next trigger` bullet).

`:2130` ends: "…and round 8's U1 finding (…, tracked separately as task #897) **makes** the
guard's untested acceptance-side load-bearing for the crate's primary alignment guarantee."
`:2132` reads: "U11 — **if** round 8's U1 fix (task #897, deleting the `align > page_size()`
skip) **lands**, this guard's untested acceptance side becomes purely cosmetic, which is itself
an argument for prioritizing that fix".

Task #897 landed at `491afe9`, the round's **first** merge; task #903 (which wrote this card)
landed at `70df07e`, the round's **seventh**. In the tree the card was committed into, the
condition it poses had already resolved and the dependency it asserts had already been removed —
`lib.rs:2141`'s check no longer consults `page_size()` at all. The round-8 review anticipated
this precisely ("If U1's fix is taken, this becomes purely cosmetic, which is one more argument
for taking it"); what the card lost is the tense.

This is the pure worktree-isolation artifact — `vmem-r8-g` branched from `8380607`, where
`page_size()` genuinely was load-bearing — and it is the mildest instance the campaign has
recorded (round 6's SC2 was six sites contradicting each other; round 7's TC3 was two committed
claims in direct opposition; this is a verb). It is filed because R34-24 makes exactly this a
structural defect: an OPEN item's Status card must read as current state, and this one asserts a
live soundness dependency that no longer exists.

**Failure scenario.** A round-9 session doing the mandatory round-start read hits item 50, reads
that a structurally-untestable guard is load-bearing for the crate's headline alignment
guarantee, and either (a) escalates U11 from record-only to actionable on the strength of a risk
that was closed in the same round it was filed, or (b) treats "prioritize U1's fix" as an
outstanding action item and re-opens a settled question.

**Fix:** two sentences. `:2130` → "…made the guard's acceptance side load-bearing for the
crate's primary alignment guarantee **until task #897 (merge `491afe9`, this same round) removed
that dependency by making `try_reserve_aligned_exact`'s check unconditional; the acceptance side
is now cosmetic**." `:2132` → drop the conditional: "U11 — round 8's U1 fix landed
(`491afe9`), so this guard's untested acceptance side is now purely cosmetic; the remaining
trigger is the `sanitize_page_size` extraction described above, which would make the *rejection*
branch testable without a benchmark-hook-shaped seam."

---

## Category 2 — the one code-side finding

### UC4 — LOW — task F's new smoke test names the `page_size()` → `PAGE` validation-base swap as the mistake it exists to catch, but the offsets it passes are rejected under **both** bases, so that swap still goes undetected — the exact counterfactual U7 was filed to close remains open

**Where:** `crates/vmem/tests/smoke.rs:533-543` (the doc comment's claim) versus `:568-569`
(the calls it makes); the guards at `crates/vmem/src/lib.rs:1087` (`decommit`) and `:1155`
(`decommit_lazy`). The mock sibling is `crates/vmem/tests/mock.rs:263` and `:279`.

The doc comment states:

> **Without this**, a future "simplification" that changed the validation base in `lib.rs`'s
> `decommit`/`decommit_lazy` from `page_size()` to the crate's smaller `PAGE` constant (both
> guards currently read `let ps = page_size();`) would forward a
> `PAGE`-aligned-but-not-`page_size()`-aligned offset straight to `madvise(2)` …

The test then passes `decommit(base, 1, PAGE)` and `decommit_lazy(base, PAGE, 0)`. Neither is a
`PAGE`-aligned-but-not-`page_size()`-aligned offset. Both guards read:

```rust
if start >= end || !start.is_multiple_of(ps) || !end.is_multiple_of(ps) { return; }
```

- `decommit(base, 1, PAGE)`: `1 % ps != 0` rejects under `ps = page_size()`, and `1 % PAGE != 0`
  rejects identically under `ps = PAGE`.
- `decommit_lazy(base, PAGE, 0)`: `start >= end` short-circuits before the alignment terms are
  evaluated at all, under either base.

So performing exactly the edit the comment describes — `let ps = page_size();` → `let ps = PAGE;`
at `lib.rs:1086` and `:1154` — leaves both new tests green on every platform and every feature
set, which is verbatim the counterfactual the round-8 review used to justify filing U7 ("The
whole suite stays green on every platform and every feature set, including the macOS CI rows,
because no test decommits at a `PAGE`-but-not-`page_size()` offset").

**In fairness to task F, it implemented what U7 prescribed.** The review's own "Fix (cheap; both
seams already exist)" paragraph specified `decommit(base, 1, PAGE)` for the mock arm and "a
misaligned call" for the `bench-internals` arm. The defect is therefore not disobedience — it is
that the fix as specified closes the *other* counterfactual in U7 (deleting the guard outright,
which both tests genuinely do catch: remove the guard and `decommit(base, 1, PAGE)` records a
`Call::Decommit` / advances `unix_madvise_attempts()` to 1, failing both). What made it a
finding rather than a footnote is that the smoke test's doc comment asserts the coverage it does
not have. `mock.rs:235-254`'s doc comment is careful in exactly the way this one is not: it
describes the `ps`→`PAGE` swap as *motivation* and then claims only that "this locks the
silent-skip contract at the `mock` call-log layer", which is true.

This is round 7's **T2** shape recurring one round later — a test whose doc comment claims a
regression-guard the assertion cannot deliver — this time in the test written to close a
coverage finding, which is where the claim is least likely to be re-checked.

**Failure scenario.** A contributor reads `README.md:100-106`'s asymmetry paragraph, notices
`recommit` validates against `PAGE` while `decommit` validates against `page_size()`, unifies
them for consistency, runs the full suite on a Linux or Windows runner (4 KiB page, `ps == PAGE`,
so nothing changes there anyway) *and* the macOS runner (16 KiB page, where the change is live),
sees green everywhere, and lands it. On that 16 KiB host, a caller decommitting at a 4 KiB-aligned
offset now reaches `madvise(2)`, which rejects the **entire** call with `EINVAL` — the
all-or-nothing failure mode `page_size()`'s own rustdoc (`lib.rs:377-383`) exists to warn about —
and `libc_madvise` discards the return value by design (task #719), so the decommit becomes a
silent no-op. The new test that was added specifically to prevent this stays green, and its doc
comment tells the next reviewer the case is covered.

**Fix (small, and it makes the test genuinely two-sided):** add a third call, gated on
`page_size() > PAGE` so it is a no-op on 4 KiB hosts and live on the macOS CI runner:

```rust
if page_size() > PAGE {
    // PAGE-aligned but NOT page_size()-aligned: rejected under `ps = page_size()`,
    // FORWARDED to madvise(2) under `ps = PAGE`. This is the arm that actually
    // discriminates the two validation bases.
    unsafe { aligned_vmem::decommit(base, PAGE, 2 * PAGE) };
}
```

with the existing `assert_eq!(attempts, 0, …)` after it. If instead the current offsets are kept,
`:533-543` should be reworded to claim only what `mock.rs:235-254` claims — that the guard's
existence is pinned, not its base — and U7's `ps`-vs-`PAGE` counterfactual should be recorded as
still open (item 50 is the natural home, since it already owns the "structurally hard to test on
available CI" category).

---

## Category 3 — verification-process findings

### UC5 — LOW — the round's entire verification matrix ran on `x86_64-pc-windows-msvc`, which compiles neither the function U1 changed nor the test task F added; both are `#[cfg(unix)]`

**Where:** `crates/vmem/src/lib.rs:2094` (`#[cfg(all(unix, not(miri)))] fn
try_reserve_aligned_exact`) and `crates/vmem/tests/smoke.rs:547-549`
(`#[cfg(all(unix, feature = "bench-internals", not(feature = "mock"), not(miri)))] fn
decommit_contract_violation_never_reaches_madvise`).

Every command in the round's stated verification — two `cargo test` rows, three clippy rows,
`cargo fmt`, the drift guard, `cargo check -p sefer-alloc --all-features`, `cargo package` —
targets the host triple. On Windows, `try_reserve_aligned_exact` is `#[cfg]`'d out entirely, as
is `smoke.rs`'s new test. **The round's only runtime change and one of its two new tests were
therefore never compiled by the round's own gate**, let alone linted or run; "42 passing, 47
passing, three clippy rows clean" is true and says nothing about either.

Nothing broke — I closed the gap rather than merely filing it:

```
$ cargo check  -p aligned-vmem --target x86_64-unknown-linux-gnu \
    --features "lazy-commit huge-pages fault-injection bench-internals" --all-targets   -> clean
$ cargo clippy -p aligned-vmem --target x86_64-unknown-linux-gnu \
    --features "lazy-commit huge-pages fault-injection bench-internals" \
    --all-targets -- -D warnings                                                        -> clean
```

Both pass on the post-merge tree, which is the first evidence anywhere that U1's edit and task
F's Unix test compile at all. (`--all-targets` is load-bearing: without it, `tests/` is not
built and the new test is still not checked.)

**Failure scenario.** A typo inside `try_reserve_aligned_exact` — a mismatched brace, a wrong
identifier in the 24-line comment block's surrounding code, a `use` that only the Unix arm needs
— produces a fully green local gate and a red CI on the Linux and macOS rows, discovered only
after the push. That is the precise sequence CLAUDE.md's "before every push" section records as
the reason `npm run check` exists ("a push in this session shipped 17 commits with a red CI …
discovered only by watching the Actions run *after* pushing"), reproduced here at crate scope.

**Fix:** add one line to this crate's standard verification matrix, used by every future round of
this campaign and by any task touching `crates/vmem`:

```
cargo clippy -p aligned-vmem --target x86_64-unknown-linux-gnu \
    --features "lazy-commit huge-pages fault-injection bench-internals" \
    --all-targets -- -D warnings
```

`x86_64-unknown-linux-gnu` is already installed on this host (`rustup target list --installed`
also shows `aarch64-unknown-linux-gnu`, `x86_64-unknown-freebsd`, `x86_64-unknown-netbsd` — the
last two are item 43's own targets, so the same command doubles as a cheap BSD compile check).
Cost measured here: ~5 min cold, seconds warm. This is a strictly better use of five minutes
than a ninth read of `lib.rs`, and it is the only local signal that touches this crate's Unix
arm at all.

### UC6 — LOW — U1's fix has no regression-test backing, and the specific mistake class it could have introduced is invisible to *every* test in this repository, on *every* platform, by construction

**Where:** `crates/vmem/src/lib.rs:1993-1997` (`unix_reserve` discarding the fast path's error);
`crates/vmem/src/lib.rs:2101` / `:2147` (the two counters that are the path's only observable);
`grep -rn "unix_exact_reserve" crates/vmem/tests/` → **no output**.

The brief asked whether this round should push back and require a test for U1. **My answer is
no, and the reasoning is worth recording because it is stronger than "a test was not mandatory".**

A test asserting the *fixed* behaviour (a misaligned `mmap` result is rejected) requires
`page_size()` to over-report, which requires an injection seam into `query_os_page_size()` — the
untestability item 50's U11 half exists to record, whose cheap remedy CLAUDE.md's benchmark-hook
rule specifically discourages. Not writable today.

A test asserting the *unchanged* behaviour (the fast path still hits on a correct platform) is
writable — `reset_bench_internals_counters(); reserve_aligned(4 * MIB, PAGE); assert!(unix_exact_reserve_hits() > 0)`
— and would catch the realistic mistake class: an inverted or over-strict condition. But note
what such a mistake would look like without it. `unix_reserve` does
`if let Ok(…) = try_reserve_aligned_exact(…) { return Ok(…) } ` and otherwise falls through to
the over-reserve path, which returns a **correctly aligned** reservation. So a fast path that
fails 100% of the time is functionally indistinguishable from one that succeeds: every existing
test still passes, on every platform, under every feature set. The only observables are syscall
count and VA footprint, exposed solely through `unix_exact_reserve_hits()`/`_attempts()` — which
nothing in `crates/vmem/tests/` reads (only `examples/v20_849_unix_exact_reserve_hit_rate.rs`,
which CI compiles and never runs).

And such a test would not run in CI even if written: it needs Unix + `bench-internals` +
non-`mock`, and the only CI row with that shape is macOS. Which is why this is a **LOW** and not
a request: the fix is right (§"U1 under the microscope" verifies it by construction, which is
the strongest available substitute), and the missing infrastructure is already filed twice.

**What this actually establishes** is that the Linux `bench-internals`-against-the-real-backend
CI row now has **three independent customers**, not one: item 48's S4 remainder
(`docs/CORRECTNESS_OPEN_ITEMS.md:2118`, the Linux half of the madvise oracle), item 50's U10 half
(the Windows counter analogue, `:2129`), and this — the Unix fast path's only observable. The
round-8 review's own closing recommendation was to stop reviewing and buy evidence instead; that
recommendation is now backed by three named beneficiaries of one CI row.

---

## Checked and explicitly NOT findings

Recorded so round 9 does not re-derive them.

- **U2's residual "identical".** `lazy_commit.rs:340` still reads "Mirrors the **identical**,
  already-established gate in tests/smoke.rs's `decommit_recommit_roundtrip`", but the two gates
  are not identical: `lazy_commit.rs:347` is `#[cfg(not(miri))]` while `smoke.rs:219-226` is a
  six-condition `not(any(miri, feature = "mock", macos, ios, tvos, watchos))` superset. The word
  predates round 8 (task #716 wrote it); U2 was about the *name* it attaches to, and that name is
  now right — smoke.rs's own text says "`not(miri)`-gated", which is true of a superset gate.
  Not filed: fixing a hedge word in a comment whose substantive claim is correct is not worth a
  round-9 task, and both files' gates are individually correct for what they assert.
- **U5's "Linux-only" scope word.** `lib.rs:1115` says "This benign-re-fault story is
  Linux-only". Strictly, the *no-crash* half holds on all Unix (other-Unix `MADV_DONTNEED`
  re-faults a zero page rather than crashing); only the *keeps-old-contents* half is Linux-specific.
  Not filed: the antecedent paragraph (`:1111-1114`) is explicitly Linux-scoped, so the sentence
  reads correctly in place, and the misreading it enables ("be careful on FreeBSD too") is in the
  conservative direction.
- **The new mock test's parallelism.** `crates/vmem/tests/mock.rs` has **no** `SERIAL` mutex —
  every test calls `mock::reset()` and asserts on the recorder while libtest runs the binary's
  tests on parallel threads. I checked this specifically because a process-global recorder would
  have made task F's new test flaky: `crates/vmem/src/mock.rs:201-207` declares `CALLS`,
  `RESERVE_FAILS` and `COMMIT_FAILS` in a `std::thread_local!` block, so there is no shared state
  and no race. Correct as written.
- **The new smoke test's counter race.** `decommit_contract_violation_never_reaches_madvise`
  takes `SERIAL` and calls `reset_bench_internals_counters()` **inside** the critical section
  before asserting `unix_madvise_attempts() == 0`. `libc_madvise` is the sole incrementer and is
  reached only from `decommit_pages_impl`'s two arms (`lib.rs:2181-2182`); all five tests in
  `smoke.rs` that can reach it (`:179`, `:236`, `:384`, `:462`, `:549`) hold `SERIAL`. It also
  satisfies the contract SC9's comment at `smoke.rs:484-490` spells out for exactly this case
  ("a future test that does add such an assertion … unless it also joins SERIAL's contract").
  No new race.
- **Item 50's citations.** All seven resolved individually in the post-merge tree: `lib.rs:168`
  (`PAGE_SIZE_CACHE`), `:206-252` (counters, with the two Windows statics at `:239`/`:252`),
  `:233-236` and `:244-249` (the two rustdoc claims), `:282-330` (five accessors), `:390-406`
  (the guard, acceptance test at `:400`), `:409-445` (`query_os_page_size`'s arms),
  `tests/lazy_commit.rs:71-117` and `tests/smoke.rs:104-113` (the two indirect `reservation_len()`
  regression tests). None was invalidated by the +33-line shift U1's hunk introduced below
  `lib.rs:2114`, because every one of them is above it or in a different file.
- **Item 48's S9 anchor after the fact.** U3 replaced line numbers with symbol names; I
  re-checked that the two named passages still say what the bullet claims after all seven merges
  — `lib.rs:1129-1139` (`decommit_lazy`'s rustdoc, which U5 lengthened by five lines; `REASONED-FROM-SPEC` at `:1132`, `MAY work identically` at `:1135`) and
  `:2239-2245` (`madv_free_advice`'s doc comment, above the `fn` at `:2251`) both still carry the hedged "MAY work identically …
  REASONED-FROM-SPEC" tvOS/watchOS wording that `docs/CORRECTNESS_OPEN_ITEMS.md:2116` must agree
  with. U5's insertion landed above that wording, not inside it.
- **Item 43's appended sentence.** `:1919-1933` reads coherently against the preceding
  wrong-constant text in the same bullet (`:1906-1919`) — it opens "A second, distinct consequence of this same
  unverified-constant gap", which is exactly the right framing, and it is past-tense about the
  fix ("used to skip … Fixed by making the check unconditional"), so unlike item 50 it does not
  suffer UC3.
- **U6's numbers.** Checked against `docs/perf/OPEN_ITEMS.md:1131` rather than against the
  round-8 review's restatement of them. 34.375 → 34.4, 46.6667 → 46.7, 56.6667 → 56.7; "30-run
  aggregate" and "WSL2 (Hyper-V-backed kernel)" both appear verbatim in the card. The added
  "x86_64" is not stated in the card but is inarguable (the raw log was produced on this
  x86_64 host).
- **`docs/perf/OPEN_ITEMS.md` untouched.** Correct, not an omission: item 46's card was already
  accurate; U6 was about two surfaces that dropped its qualifiers, and both were fixed.
- **Semver / API surface.** No public item's signature changed, no `#[cfg]` on any shipping item
  changed, `Cargo.toml` untouched, no new `unsafe` token in `src/`. `cargo package --list`
  returns the identical 20-file set as rounds 6 and 7.
- **The remaining 8 `as usize` casts in `tests/`.** U9's chosen resolution (narrow the README
  claim, convert the one cast the doc example points at) is a legitimate reading of an INFO
  finding. The other eight are unchanged and remain what item 41's future
  `-Zmiri-strict-provenance` step would flag. Not re-filed.

---

## Recommended order

1. **UC1** — the CHANGELOG entry plus two lines in item 1. The round is not closeable without
   it, and it is the eighth recurrence.
2. **UC2** — settle the commit-or-don't convention *once*, then apply it. Doing (b) or (c)
   without (a) guarantees a ninth instance.
3. **UC4** — one `if page_size() > PAGE { … }` block in `smoke.rs`, or a reworded doc comment.
   The only finding here with a code-behaviour consequence behind it.
4. **UC3** — two sentences in item 50.
5. **UC5** — one line added to the crate's verification matrix; costs nothing per round after
   the first warm build.
6. **UC6** — record-only. Fold into whatever task next argues for the Linux `bench-internals`
   CI row; it is that row's third named beneficiary.

---

## On the campaign, eighth round

The round-8 review argued diminishing returns with a number and recommended converting the
campaign from "review the crate again" to "buy the evidence the crate cannot reason its way to".
This closing pass is the strongest evidence yet for that recommendation, from two directions.

**The remediation quality is now very high.** Eleven of eleven fixes landed on the right content
— a first. The one new *technical* finding (UC4) is not a defect in the fix but an overclaim in
its doc comment, and the fix itself does close the counterfactual it was principally aimed at.
Three of the six findings here (UC1, UC2, UC3) are bookkeeping about the campaign's own records,
and a fourth (UC5) is about the campaign's own verification command list. The campaign is now
almost entirely auditing itself.

**And the one place where reading *did* pay this round was in the direction the review predicted
it would stop paying.** U1 required contradicting three prior rounds; verifying U1 required
tracing a call graph and a validation invariant, not reading prose — and the single most useful
thing this pass did was not a read at all, it was running two commands against a target the
round had never compiled (UC5). Both facts point the same way: the remaining value in this crate
is in *executing* things on platforms nobody has executed them on, not in re-reading 2,650 lines
a fifth, sixth, seventh and eighth time.

Concretely, three named items are now blocked on one cheap piece of infrastructure — a Linux CI
row running `bench-internals` against the real (non-`mock`) backend closes item 48's S4
remainder, gives item 50's U10 half a template, and makes U1's fast path observable for the
first time (UC6). Adding the Unix cross-compile line from UC5 to the local matrix is the
zero-cost half of the same idea. If a round 9 happens, those two, plus the `file:line`/test-name
citation resolver the round-8 review sketched (which would have caught UC3 mechanically and
would have caught U2, U3 and U8 last round), are a better round than a ninth read.
