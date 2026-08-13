# `aligned-vmem` — round-7 CLOSING review (verification of the T1–T10 remediation)

**Date:** 2026-08-13

**Scope:** verification of the seven remediation tasks (#888–#894, letters A–G) that closed
`docs/reviews/2026-08-13-aligned-vmem-round7-review.md`'s findings T1–T10, plus the seven
`--no-ff` merge commits that landed them. Every file in the round's diff
(`git diff 1dbd6b4..HEAD --stat`: 7 files, +176/−84) and the code each of those changes makes
a claim about. Like round 6, this round was delegated to independent sub-agents in seven
isolated git worktrees (`vmem-r7-a` … `vmem-r7-g`, all branched from `1dbd6b4`), then merged
sequentially.

**Reviewed tree:** local `main` @ `1e532a7` (the task #894 merge). `git fetch` +
`git log origin/main..HEAD --oneline | wc -l` → **14** — the 7 merge commits plus the 7 task
commits they carry; `origin/main` is still `1dbd6b4`. **None of round 7 has been pushed**, so
there is no new CI signal for this round's own diff; the macOS evidence T1 is *about* belongs
to the already-pushed `1dbd6b4`, not to anything reviewed here.

`git status --porcelain` shows exactly three untracked entries — two pre-existing checkpoints
(`docs/checkpoints/2026-08-13-0130.md`, `docs/checkpoints/2026-08-13-1730.md`) and
`docs/reviews/2026-08-13-aligned-vmem-round7-review.md`. That third one is **TC2**.

**Toolchain / host:** `rustc 1.97.0`, stable-x86_64-pc-windows-msvc; Windows 10 Pro, 4 KiB
page. **No Darwin host and no Darwin target** — every Darwin claim below is reasoned or read
from the already-published `1dbd6b4` CI logs, never executed here.

**Nature:** read-only. Nothing in the repository was modified other than the creation of this
document. No `git add` / `git commit` / `git push` / branch, worktree or ref mutation. Every
command quoted below was executed on this host; every `file:line` citation was read in the
current tree before being written down.

**Finding prefix:** `TC` (round-7 closing). Prior prefixes deliberately not reused: `V`/`W`/`P`
(rounds 1–2), `F` (round 3), `R`/`CR` (round 4 + closing), `Q`/`QC` (round 5 + closing),
`S`/`SC` (round 6 + closing), `T` (round 7).

---

## Verdict up front

**All ten of T1–T10's fixes landed, and all ten landed on the right content** — every
approximate line number the sub-agents were handed resolved to the intended code; not one
edit hit adjacent unrelated text. That is a better mis-citation record than round 6's parallel
delegation produced. The full verification matrix is green here, re-executed rather than taken
on trust (§"What was verified green"), and `cargo package -p aligned-vmem --list` still returns
the same sane 20-file set, so nothing in T5's URL rewrite regressed publish-readiness.

**And the campaign's signature pattern held for a seventh time.** Round 7's own remediation
produced nine new items, three of which are the same three failure modes the brief predicted:

1. **The round has no CHANGELOG entry** (**TC1**) — the seventh recurrence of the process gap
   `docs/CORRECTNESS_OPEN_ITEMS.md` item 1 exists to track, whose own counter still says six.
   Round 7's decomposition, unlike round 6's, had no dedicated CHANGELOG task; task A appended
   a *follow-up paragraph inside round 6's section* (CHANGELOG.md:399) and nothing created a
   round-7 section at all.
2. **The round-7 review doc is not committed** (**TC2**) — and four durable records now cite it
   by path, including the only place that records the nine unfixed FFI sites item 49 owns.
   Round 6's closing commit `1dbd6b4` explicitly committed both of its review docs; round 7
   broke that precedent silently.
3. **Task D's fix contradicts a bullet task A owned in the same file-pair** (**TC3**) — the
   textbook worktree-isolation failure: two agents edited non-overlapping prose about the same
   fact (`MADV_FREE_REUSABLE` on tvOS/watchOS) and produced a repository that now asserts both
   "XNU-wide, all four Darwin targets could use it" (`lib.rs:1123-1126`, new this round) and
   "no `MADV_FREE_REUSABLE` there" (`docs/CORRECTNESS_OPEN_ITEMS.md:2102`, untouched). No git
   conflict, because the edits are 1,000 lines and one file apart.

The half-swept-scope check the brief called out specifically came back **mostly clean**: task D
(T4) did catch all five macOS→macOS/iOS advice sites the review named, and re-grepping every
`macOS`/`iOS` token in `lib.rs` + `README.md` finds no sixth site that should have been swept.
The half-sweep this round is in **T5** instead — task B fixed eight of the nine citations T5's
own "Where" clause enumerated, leaving `Cargo.toml:103` (**TC4**).

The rest is smaller: one mis-citation pointing at a test that does not exist (**TC5**), one
surface of T1's own staleness left un-updated (**TC6**), one open-items card missing a required
field (**TC7**), and two INFO notes (**TC8**, **TC9**).

**Nothing found here is a soundness, correctness, or performance defect.** Round 7 changed no
runtime behavior at all — every `src/` hunk is a comment, a doc comment, a `#[cfg]` predicate on
a test, a `// SAFETY:` line, or a `ptr as usize` → `ptr.addr()` swap. Publish-readiness (task
#658) is not newly blocked; TC4 is the only publish-facing item and it is one line.

---

## What was verified green — every command below was executed on this host

```
$ git fetch && git log origin/main..HEAD --oneline | wc -l
14                                      # 7 merges + 7 task commits; origin/main == 1dbd6b4

$ cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" \
      --no-fail-fast
fault_injection 5 / huge_pages 1 / lazy_commit 11 / min_page 2 / mock 0 /
smoke 20 / vmemerror_io_bridge 3 / doc-tests 0        => 42 passed, 0 failed

$ cargo test -p aligned-vmem --all-features --no-fail-fast
0 / 0 / 1 / 11 / 2 / 9 / 20 / 3 / 0                    => 46 passed, 0 failed

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
                                        # identical to round 7's own listing; still no docs/
```

Counts match round 7's review exactly (42 / 46), so no test was added, removed, or silently
disabled by this round — consistent with the fact that its only test-file change is a `#[cfg]`
predicate on a macOS-only test and two doc comments.

One command the round-7 review could not meaningfully run (it had 0 unpushed commits) is
meaningful now:

```
$ node scripts/verify-commit-prefixes.mjs
[verify-commit-prefixes] range: @{u}..HEAD  (14 commit(s) total)
[verify-commit-prefixes] 2 WARNING(s) (direction 2 — hidden runtime change?):
  - 5cc77d4 "docs(vmem): document MAP_ANON/MAP_HUGETLB arch dependency ..." — ... crates/vmem/src/lib.rs
  - e3e3b27 "docs(vmem): replace 7 publish-facing ... citations with resolvable URLs" — ...
            crates/vmem/Cargo.toml, crates/vmem/README.md, crates/vmem/src/lib.rs
[verify-commit-prefixes] PASS (with warnings above)
```

**Both warnings are false positives and I verified them personally, not by reading the lint's
own hedge.** `5cc77d4`'s entire `src/lib.rs` hunk is one `//` block comment plus three `///`
lines above two `const` declarations whose values and `#[cfg]`s are byte-identical before and
after; `e3e3b27`'s `src/lib.rs` hunks are five `///` citation rewrites, no code. The `docs(...)`
prefix is correct under CLAUDE.md's R30-12 taxonomy in both cases. Recorded so round 8 does not
re-derive it, and so the warnings are not mistaken for a lint failure on push.

---

## Round-7 remediation (`1dbd6b4..1e532a7`) — T1–T10 verification

Checked first, before looking for anything new — seven consecutive rounds have found the fix to
be the next round's bug source.

| # | Task | Status in the current tree | Evidence |
|---|---|---|---|
| T1 | A (#888) | **CLOSED, with the sub-note honored** | Item 43's Status/Current-number/Evidence/Next-trigger all rewritten to macOS-half-CLOSED, BSD-half-OPEN (`docs/CORRECTNESS_OPEN_ITEMS.md:1893-1913`); the macOS half moved to "Recently resolved" with the run+job citation (`:3333-3356`); item 48's Root-cause bullet (`:2101`) records **exactly** the two-run wording the review's sub-note insisted on — "H2 is ruled out by run `31692217669`; combined with run `31676133649`'s stale byte, H1 … is the only remaining explanation — NOT 'H1 confirmed by CI'". This was the one place the review said it would push back on a naive closure, and the naive closure was not taken. A third surface still reads as pending → **TC6**. |
| T2 | E (#892) | **CLOSED, and the cited line numbers are right** | `tests/smoke.rs:71-82` now states what the test actually pins and names `huge_pages.rs:61-62` as the real W2 guard. Verified both citations against the tree: `lib.rs:963` is literally `granted_huge: false,` inside the `:955-967` `finish_reservation(...)` block, and `huge_pages.rs:61-62` is `#[cfg(not(target_os = "linux"))]` + `assert!(!r.is_huge(), …)`. |
| T3 | C (#890) | **CLOSED** | `tests/smoke.rs:356` is now `#[cfg(all(target_os = "macos", target_arch = "aarch64", not(miri)))]`, with a 8-line rationale at `:348-355` naming `query_os_page_size()`'s `#[cfg(miri)]` arm. One clause of that rationale names a test that does not exist → **TC5**. |
| T4 | D (#891) | **CLOSED at all five named sites; a NEW cross-file contradiction introduced** | `lib.rs:1103` (summary), `:1113-1126` (the paragraph + reworded back-reference), `:2205-2208` (`madv_free_advice`), `:2312` (`MADV_FREE_REUSABLE`), `README.md:49`. I re-grepped every `macOS`/`macos`/`iOS`/`target_os = "ios"` token in `lib.rs` + `README.md` (37 hits) and found **no sixth advice-selection site left behind** — the sweep is genuinely complete on its own axis. The review's "Adjacent" suggestion (add the crate-cfg-omission clause) was also taken, and that is what creates **TC3**. |
| T5 | B (#889) | **8 of the 9 sites its own "Where" clause named** | `README.md:143`, `:163`; `lib.rs:1052`, `:1069`, `:1078`, `:1178`, `:1236`; `Cargo.toml:114` — all now `<https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>`. `Cargo.toml:103` was named and not swept → **TC4**. |
| T6 | F (#893) | **CLOSED (documenting option)** | `lib.rs:2235-2251` (the `MAP_ANON` block comment) + `:2270-2274` (`MAP_HUGETLB`'s doc). Both MIPS values transcribed correctly from the review (`MAP_ANONYMOUS = 0x0800`, `MAP_HUGETLB = 0x80000`), the `MAP_RENAME` detail preserved, the `EBADF` → fail-closed chain stated, and the note is explicitly marked `REASONED-FROM-SPEC, not executed` — matching the `_SC_PAGESIZE` / `LINUX_HUGE_PAGE_SIZE` precedent it cites. The arch enumeration is under-inclusive → **TC9**. |
| T7 | G (#894) | **The one concrete bug fixed; the other nine filed, not silently dropped** | `lib.rs:2414-2427` — `let _ = huge;` and the `// SAFETY:` line are now separated by a blank line and the `mmap` call sits in an explicit `unsafe { … }`, so rustfmt can no longer glue the proof onto the discard. The remaining nine sites became item 49 (`docs/CORRECTNESS_OPEN_ITEMS.md:2107-2110`). That is the honest disposition for an INFO record-only finding. The card is missing a required field → **TC7**. |
| T8 | G (#894) | **CLOSED** | `lib.rs:1609` — `// SAFETY: same range within the same live reservation.` immediately above the retry `VirtualAlloc`, byte-identical to the two-call path's sibling at `:1681-1686`. |
| T9 | G (#894) | **CLOSED, more completely than asked** | All six mock recording sites converted to `.addr()` (`lib.rs:775`, `:986`, `:1089`, `:1145`, `:1204`, `:1294`), plus both showcase snippets (`lib.rs:61`, `README.md:26`) changed from `base as usize % span` to `base.addr() % span`. `grep -rn "as usize" crates/vmem/src/` now returns **zero** pointer casts — the three remaining hits are integer widening (`v as usize`, `dw_page_size as usize`) and two comments. The README caveat added alongside it describes the state the same commit removed → **TC8**. |
| T10 | G (#894) | **CLOSED** | `docs/perf/OPEN_ITEMS.md:1158-1171` — the plumbing gap, the required error-channel widening, AND the non-obvious munmap-ordering point the review said was "the non-obvious part" are all recorded; `Full history:` bumped to name task #894. Correctly cites no line numbers, so it cannot go stale. |

No conflict markers anywhere. No new `unsafe` token: `git diff 1dbd6b4..HEAD -- crates/vmem/src/lib.rs | grep '^+.*unsafe'` yields only the T7 `unsafe { mmap(...) }` block, which *narrows* rather than widens the unsafe surface (an explicit block inside an already-`unsafe fn`). No public API changed; no `#[cfg]` on any shipping item changed; no feature composition changed.

---

## Category 1 — process records the round owes and did not write

### TC1 — MEDIUM — round 7 has no `CHANGELOG.md` entry, and `docs/CORRECTNESS_OPEN_ITEMS.md` item 1's recurrence counter still reads 6

**Where:** `CHANGELOG.md:377` (the last `aligned-vmem` section heading,
`#### aligned-vmem — round-6 follow-up (2026-08-13, tasks #880-886)`), `:399` (the paragraph
task #888 appended *inside* it), `:401` (the next heading, `#### size-classes …`) — there is no
heading between `:377` and `:401` other than round 6's own; `docs/CORRECTNESS_OPEN_ITEMS.md:75`
(item 1's `Current number` bullet).

Round 7 landed 14 commits across 7 tasks touching 7 files. The only CHANGELOG text it produced
is the T1 follow-up paragraph at `:399`, which is *about* round 6's push and lives *inside*
round 6's section. There is no `#### aligned-vmem — round-7 follow-up (…, tasks #888-894)`
section. Grepping every `^#### ` in `CHANGELOG.md` confirms it: the aligned-vmem sequence runs
`… round-4 follow-up (:330) → round-5 follow-up (:345) → macOS decommit CI-discovery fix (:367)
→ round-6 follow-up (:377) → [size-classes]`.

Item 1's card is the durable tracker for exactly this and it is now itself stale. Its
`Current number` bullet (`:75`) enumerates six instances and ends "round 6 (tasks #880-886) is
a **6th instance**, caught by SC3 in the round-6 closing review rather than by the round's own
remediation — so the 'within-round catch' streak was 2 rounds long, not a settled pattern."
Round 7 is a **7th instance**, and it is a *stronger* data point than round 6 was: round 6 at
least had a dedicated task G that wrote CHANGELOG text (SC3 caught that it wrote the wrong
text); round 7's decomposition had no CHANGELOG owner at all, so the gap reproduced by simple
omission rather than by error. That is precisely the "the gap reappears the moment the round's
own remediation doesn't happen to include a CHANGELOG-writing task" mechanism item 1's own
prose predicts, observed a second time.

**Failure scenario (concrete).** A round-8 session performs CLAUDE.md's mandatory round-start
read of both indexes. It reads item 1's card, sees "6th instance … round 6", and concludes the
last recorded occurrence is round 6 — i.e. that round 7 either did not happen or did not have
the gap. Separately, anyone reconstructing what shipped in `aligned-vmem` from `CHANGELOG.md`
alone sees round 6's section end with "**None of round 6's work has been pushed as of this
entry**" plus a follow-up saying the push went green, and no record whatsoever that seven
further remediation tasks then landed on top — including the URL rewrite that changed the
crates.io landing page and the `not(miri)` gate that changes which tests run on a Darwin
contributor's machine. Both are publish-relevant (task #658) and neither is in the changelog a
0.2.0 consumer would read.

**Fix:** write `#### aligned-vmem — round-7 follow-up (2026-08-13, tasks #888-894)` citing the
seven real merge SHAs from `git log`, and bump item 1's `Current number` + `Evidence` bullets to
record the 7th recurrence (with this document as the catching artifact, the same way SC3/QC9/
CR10 are recorded).

### TC2 — MEDIUM — `docs/reviews/2026-08-13-aligned-vmem-round7-review.md` is untracked; four durable records already cite it by path, and one of them delegates its entire evidence to it

**Where:** the file itself (`git status --porcelain` → `?? docs/reviews/2026-08-13-aligned-vmem-round7-review.md`;
`git log --all --oneline -- docs/reviews/2026-08-13-aligned-vmem-round7-review.md` → **empty**,
i.e. it exists in no commit on any ref). Citations: `CHANGELOG.md:399`,
`docs/CORRECTNESS_OPEN_ITEMS.md:2107`, `:2110`, `:3349`.

`git ls-files docs/reviews/` ends at `2026-08-13-aligned-vmem-round6-review.md` +
`…-round6-closing-review.md` — both committed by `1dbd6b4`, whose own subject line says so
("… commit both round-6 review docs"). Round 7's remediation cites its review doc four times and
never committed it.

The worst of the four is item 49's Evidence bullet (`:2110`):

> **Evidence:** `docs/reviews/2026-08-13-aligned-vmem-round7-review.md` finding T7 (full list of
> ten sites with line numbers as of that review).

Item 49's own body (`:2107`) names the *functions* (`winapi_virtual_release`, `libc_munmap`,
`mmap`, `munmap`, two `madvise`, `std::alloc::dealloc`) but explicitly delegates the line
numbers — the thing that makes the item actionable without re-running the lint — to the
uncommitted file.

**Failure scenario (concrete, and cheap to trigger).** The working tree is cleaned, the session
ends on a different machine, or a `git clean` runs — outcomes an untracked file is one command
away from at all times. `CHANGELOG.md:399` then cites a nonexistent document as the source of
the T1 closure; the "Recently resolved" #43 entry (`:3349`) cites a nonexistent document as the
authority for closing item 43's macOS half; and item 49 loses its only record of which nine
sites remain, so a future edition-2024 migration re-derives the list from scratch. Note the
asymmetry with round 6, which committed *both* of its review docs in the same commit as its
remediation precisely so its citations would resolve.

**Fix:** `git add` the round-7 review doc (and this closing review) in the round-closing commit,
matching `1dbd6b4`'s precedent. Independently worth doing: inline the nine line numbers into item
49's body so the item survives its evidence file regardless.

---

## Category 2 — the worktree-isolation inconsistency

### TC3 — LOW-MEDIUM — task D's new tvOS/watchOS rationale in `lib.rs` asserts the exact opposite of item 48's S9 bullet, which task A edited in the same round and did not touch; the claim is also unhedged where every sibling claim in the file is marked `REASONED-FROM-SPEC`

**Where:** `crates/vmem/src/lib.rs:1123-1126` (added by `b37845d`, task #891) versus
`docs/CORRECTNESS_OPEN_ITEMS.md:2102` (item 48's "Darwin lazy-path alternative fix (round-6
review S9…)" bullet, written round 6, untouched by round 7).

New text, `lib.rs:1123-1126`:

> This tvOS/watchOS fallback is a limitation of this crate's current `madv_free_advice` cfg
> coverage, **not a platform limitation: `MADV_FREE_REUSABLE` is XNU-wide, so all four Darwin
> targets could in principle use it** (see `madv_free_advice`'s doc).

Standing text, `docs/CORRECTNESS_OPEN_ITEMS.md:2102`:

> … route macOS/iOS's eager `decommit` to `MADV_FREE_REUSABLE` and issue `MADV_FREE_REUSE` from
> `recommit` — this would close the "return physical backing to the OS" half … but NOT the
> "reads as zero" half …, and **would not help tvOS/watchOS at all (no `MADV_FREE_REUSABLE`
> there)**; only re-mapping closes both halves on all four targets.

These cannot both be true. The review's T4 "Adjacent" note asked for one clause naming the crate
cfg omission; task D wrote that clause *and* added the "XNU-wide" premise underneath it, which
is what collides. No git conflict arose because the two edits are ~980 lines and one file apart —
exactly the shape the brief predicted for parallel worktrees editing overlapping *prose* rather
than overlapping *text*.

Which is right matters, and neither statement carries a hedge. The crate is scrupulous about
this elsewhere: `_SC_PAGESIZE`'s table (`lib.rs:2318-2339`), `LINUX_HUGE_PAGE_SIZE`
(`:2287-2297`), and the brand-new T6 note (`:2250-2251`) all say `REASONED-FROM-SPEC, NOT
empirically verified` in so many words. The new `MADV_FREE_REUSABLE`-is-XNU-wide claim is a
header-read assertion about three platforms this repo has never compiled for, stated flatly.

**Failure scenario (concrete).** Item 48's `Next trigger` instructs a future round to implement
a real Darwin fix and records two candidates: the `mmap(MAP_FIXED)` re-map, and S9's cheaper
`MADV_FREE_REUSABLE` + `MADV_FREE_REUSE` route. The deciding factor between them, as item 48
itself frames it, is coverage: "only re-mapping closes both halves **on all four targets**."
That round reads `lib.rs` and learns the S9 route covers all four; it reads item 48 and learns
it covers two. It either picks the wrong candidate on the wrong coverage premise, or spends the
task re-deriving a fact from XNU headers it has no way to execute against — which is the same
"spend a task on an answer that already exists / decline the work because the premise reads as
unestablished" cost T1 was filed for, one level over. Note also that a widened `madv_free_advice`
cfg would silently break `macos_decommit_madvise_syscall_actually_succeeds`'s `attempts == 2`
assertion (`smoke.rs:489-493`) on tvOS/watchOS, which SC10's own comment at `:479-486` warns
about for the *other* candidate change but not this one.

**Fix:** decide which claim stands, make both sites say it, and mark it `REASONED-FROM-SPEC`
(no Darwin target is buildable here, so it cannot be anything else). If the XNU-wide reading is
kept, item 48's S9 bullet's parenthetical and its "only re-mapping closes both halves on all
four targets" conclusion both need rewording, because the second follows from the first.

---

## Category 3 — half-swept scope, mis-citation, and stale claims

### TC4 — LOW (publish-relevant, task #658) — T5's sweep fixed 8 of the 9 sites its own "Where" clause enumerated; `Cargo.toml:103` still cites a workspace-root path that is not in the published tarball

**Where:** `crates/vmem/Cargo.toml:103` — `# question from
\`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md\` F11 with`. T5's "Where" clause reads
"… Plus `crates/vmem/Cargo.toml:103`, `:113`, which ship via `Cargo.toml.orig`." Task B fixed
`:113` (now `:114`) and not `:103`.

Verified the exact identification against the base revision, not by trusting the line number:
`git show 1dbd6b4:crates/vmem/Cargo.toml | sed -n '95,120p'` puts the `SPEEDUP_OPPORTUNITY_SURVEY`
line at 103 and the `CORRECTNESS_OPEN_ITEMS` line at 113. `Cargo.toml.orig` is in
`cargo package --list`, so the comment ships verbatim to anyone who vendors or inspects the
crate. `docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` does exist in the repository, so the
same one-line URL rewrite applies as the other eight.

Task B's commit subject ("replace **7** publish-facing `docs/CORRECTNESS_OPEN_ITEMS.md`
citations") explains the miss: it took T5's headline count (7 = 2 README + 5 rustdoc) as the
scope and treated the two `Cargo.toml` sites as a bonus, of which it happened to do the one that
matched the headline's filename. The finding's own enumeration was 9.

**Also unswept, and not in T5's list at all:** `crates/vmem/src/lib.rs:262` — a `///` doc comment
on `UNIX_MADVISE_ATTEMPTS` reading "the empirical oracle for `docs/CORRECTNESS_OPEN_ITEMS.md`
item 48". It is `#[doc(hidden)]` so it does not render on docs.rs, but the source ships and it is
the same dead reference in the same file the sweep edited five other times. Three further bare
`docs/…` citations ship in test/comment surfaces (`lib.rs:179`, `:2162`, `smoke.rs:208`, `:335`,
`:435`, `examples/v20_849_…:1`); these are lower-value (non-rustdoc, and `tests/`/`examples/`
readers are already looking at a repository) but are listed here so a future sweep has the
complete set rather than another partial one.

**Failure scenario.** Exactly T5's, unchanged, for the one surviving `Cargo.toml` site: a reader
evaluating the crate from crates.io or a vendored copy follows the citation to justify the
`bench-internals` feature's existence and finds no such file. Post-0.2.0 this needs a version
bump to correct; pre-publish it is one line.

### TC5 — LOW — the `not(miri)` rationale task C added cites "`decommit_lazy_roundtrip`'s sibling in `lazy_commit.rs`" as one of the pattern's precedents; no such test exists

**Where:** `crates/vmem/tests/smoke.rs:355`.

The sentence reads: "Matches the `not(miri)` exclusion this crate's other real-OS-property
assertions already use (e.g. the zero-fill assertion above, the madvise oracle below,
`decommit_lazy_roundtrip`'s sibling in `lazy_commit.rs`)." The first two resolve correctly
(`smoke.rs:219`'s `#[cfg(not(any(miri, …)))]` and `smoke.rs:439-444`'s `not(miri)`). The third
does not:

- `decommit_lazy_roundtrip` is at `crates/vmem/tests/smoke.rs:378` — the *same file*, not
  `lazy_commit.rs`. `grep -rn decommit_lazy_roundtrip crates/vmem/` returns four hits, all in
  `smoke.rs`.
- `lazy_commit.rs` contains 11 tests (`lazy_reserve_basic_write_initial_region`,
  `lazy_reserve_then_commit_range_grows_accessible`, `commit_range_*`, …). None is named for or
  derived from `decommit_lazy_roundtrip`; `lazy_commit.rs` never calls `decommit_lazy` at all.
- The `not(miri)` gate T3 actually cited as the fourth sibling (`lazy_commit.rs:343`) lives
  inside `sequential_commit_range_grows_incrementally` (`fn` at `:303`), and its own comment
  (`:336-342`) says it mirrors *`smoke.rs`'s
  `recommit_is_fallible_and_reports_success_on_the_happy_path`* — a different test again.

The review handed task C the bare citation "`tests/lazy_commit.rs:343` — the zero-page read"
without a test name; the agent supplied a name by inference and got it wrong. This is the
mis-citation class the brief flagged, in its mildest form: the `#[cfg]` change itself is
correct, only the prose pointer is not.

**Failure scenario.** A future contributor (or a round-8 reviewer doing exactly what this pass
did) follows the pointer to learn the established pattern for `not(miri)` gating, searches
`lazy_commit.rs` for `decommit_lazy_roundtrip`, finds nothing, and either concludes the pattern
was invented for this one test or spends the time this pass spent proving otherwise. In a crate
whose review campaign has spent seven rounds on citation accuracy specifically, an unresolvable
in-tree cross-reference is cheap to introduce and cheap to fix.

**Fix:** name the real precedent — `sequential_commit_range_grows_incrementally`'s zero-page
read in `lazy_commit.rs`, or (better, since it is the one that actually documents the same
real-OS-vs-miri distinction) `recommit_is_fallible_and_reports_success_on_the_happy_path` in
this same file.

### TC6 — LOW — T1 updated both index cards and the CHANGELOG but not the third surface making the same "pending" claim: the madvise oracle's own rustdoc still describes the confirming CI run as future work

**Where:** `crates/vmem/tests/smoke.rs:397-419`, specifically `:410` — "… on the next real macOS
CI run -- ruling OUT H2 if it passes (the syscall did return 0), which then leaves H1 …".

Task A's remit was item 43's and item 48's cards, and it executed that well (see the T1 row
above — including the two-run caveat the review specifically warned against dropping). But the
run has now happened, and a repo-wide grep for the pending-phrasing family
(`next (real )?macOS( CI)? run|has NOT yet run|awaiting real CI|not yet run on real`, excluding
`docs/reviews/`) returns exactly one live source-code hit after the round: `smoke.rs:410`. The
other two hits are `CHANGELOG.md:383` and `:397`, both inside round 6's historical section and
both now correctly superseded by the follow-up paragraph task A appended at `:399` — an
append-only historical record, which is the right convention there.

`smoke.rs:397-419` is not a historical record; it is the live doc comment on the artifact that
produced the answer, and it is the surface a reader of the *crate* (rather than of the indexes)
reaches first. Its remaining forward-looking sentences are: "proves the `madvise` SYSCALL ITSELF
succeeded … **on the next real macOS CI run**", "ruling OUT H2 **if it passes**", and "WITHOUT
this crate having macOS hardware to run the confirmation on directly." The last is still true.
The first two are not.

**Failure scenario.** The same one T1 named, one surface over: a round-8 session (or a
contributor) reading `smoke.rs` to understand what the oracle established concludes the
H1-vs-H2 question is still awaiting its first run, and re-derives or re-schedules an answer that
`31692217669` already produced — or, worse, treats the two index cards' now-confident wording as
unsupported, since the artifact those cards cite describes itself as unrun.

**Fix:** one sentence in the rustdoc recording the run (`31692217669`, job `94421845398`,
`attempts == successes == 2`) and re-tensing "on the next real macOS CI run" to "on CI run
`31692217669`", keeping the H1 two-run caveat item 48 now carries so the two do not diverge again.

### TC7 — LOW — item 49's current-state card omits `Current-number-or-verdict`, one of the four fields CLAUDE.md's R34-24 rule requires; the two neighbouring items edited in this same round both carry all four

**Where:** `docs/CORRECTNESS_OPEN_ITEMS.md:2107-2110`.

Item 49 (added by task G) has `Status`, `Next trigger`, `Evidence`. Items 43 (`:1893-1913`) and
48 (`:2099-2106`) — both rewritten by task A in this same round, in this same file — each carry
`Status` / `Current-number-or-verdict` / `Next trigger` / `Evidence`. CLAUDE.md's R34-24 rule
names all four explicitly ("every open item carries a current-state card (Status /
Current-number-or-verdict / Next trigger / Evidence) as its FIRST visible block").

The missing number is not a formality here — it is the item's whole content. Item 49's headline
is "**ten** FFI call sites", its body says one was fixed, so the current number is nine, and
that nine appears nowhere as a field a round-start scan can read. It has to be inferred from
prose.

**Failure scenario.** A round-8 round-start read scans the cards for current numbers to decide
what this round closes. Item 49 presents no number; the reader either files it as
unquantified/deferred without opening it, or reads the full paragraph and derives "nine" by
subtraction — which is exactly the re-derivation cost the current-state-card convention exists to
eliminate.

**Adjacent, recorded not filed:** the new "Recently resolved" entry is numbered **43**
(`:3333`), while the main index simultaneously keeps an open item **43** (`:1888`) — the same
number naming two live entries, one open (BSD half) and one resolved (macOS half). Here it is
arguably deliberate and self-documenting (the entry's own title is "Item 43 (macOS half only)"),
and the file already has pre-existing collisions of this shape (two `46.` entries at `:1988` and
`:2059`; resolved `#3` and open `42.` describe the same `mock` feature-unification decision), so
this is not filed as a round-7 defect. It is noted because a *partially* resolved item split
across both sections is a new pattern in this file and the next such split should decide the
convention deliberately rather than by precedent-accident.

---

## Category 4 — INFO

### TC8 — INFO — the README provenance caveat task G added describes the state task G's own commit eliminated

**Where:** `crates/vmem/README.md:171-175`, added by `c37c248` (task #894).

The paragraph now reads: "The returned pointers preserve provenance (no exposed-address
`as usize` round-trips in the public API — **the mock backend's diagnostic-only call recorder
does expose addresses as `usize`** for comparison/logging, but none of those values is ever cast
back into a pointer)."

The same commit converted all six of those recorder sites from `ptr as usize` to `ptr.addr()`
(`lib.rs:775`, `:986`, `:1089`, `:1145`, `:1204`, `:1294`). Under the strict-provenance model
`.addr()` is specifically the **non-exposing** accessor; `as usize` / `expose_provenance()` is
the exposing one. `grep -rn "as usize" crates/vmem/src/` now returns three integer-widening
casts and two comments, and zero pointer casts. So the crate no longer exposes any address
anywhere, and the caveat added in the same breath says it does.

T9's stated rationale for wanting the caveat was that the guarantee "is literally true only
because these are exposures rather than round-trips" — a distinction that dissolved the moment
the same task removed the exposures. What remains true is the weaker, uncontroversial fact that
`mock::Call`'s fields are `usize`-typed addresses; that is worth saying, but "does expose
addresses as `usize`" is the wrong way to say it, because it borrows the exact term of art the
sentence is disclaiming.

**Failure scenario.** A downstream auditor (or a future round pursuing item 41's
`cargo miri test` CI step with `-Zmiri-strict-provenance`) reads the README, expects to find
exposed-address casts in the mock path, greps, finds none, and has to reconcile the
documentation with the code — or, filing in the other direction, reports the crate as having
exposures it does not have. Zero runtime consequence; pure documentation drift, introduced by
the commit that fixed the thing being documented.

**Fix:** reword to what is actually true — e.g. "(the `mock` recording backend stores addresses
as `usize` for diagnostic comparison, obtained via `.addr()`, never via an exposing cast, and
never converted back into a pointer)" — or drop the qualifier, since after this round the
original unqualified sentence is simply correct.

### TC9 — INFO — T6's new architecture enumeration labels an incomplete list "the tier-1/tier-2 targets"

**Where:** `crates/vmem/src/lib.rs:2239-2241`.

> `0x20` / `0x40000` are `asm-generic/mman-common.h`'s values, correct on every mainstream Linux
> architecture this crate targets (x86, x86_64, aarch64, arm, riscv, powerpc — **the tier-1/tier-2
> targets**).

The values *are* correct on every architecture named — this is not a factual error about any
listed entry. But the list is not "the tier-1/tier-2 targets": `s390x-unknown-linux-gnu` and
`loongarch64-unknown-linux-gnu` are both tier-2 Rust Linux targets and both absent. (Both use
`asm-generic/mman-common.h`, so the constants are correct there too — the enumeration is
under-inclusive, not wrong, which is why this is INFO and not a correctness finding.) `sparc64`
and `hexagon` are further asm-generic Linux targets outside the list.

**Failure scenario.** Someone auditing this constant's portability for a new target reads the
parenthetical as an exhaustive tier-1/tier-2 roster, does not find their target in it, and
concludes the constant is unverified for their platform — repeating work the note was written to
prevent. The reverse failure is milder but real: the label invites a future editor to "complete"
the list by trusting it was complete when written.

**Fix:** either drop the "— the tier-1/tier-2 targets" gloss (the preceding clause "every
mainstream Linux architecture this crate targets" already carries the meaning without claiming a
roster), or state the rule rather than the roster: "every Linux architecture that uses
`asm-generic/mman-common.h`, which is all of them except MIPS, Alpha, PA-RISC and Xtensa."

---

## Checked and explicitly NOT findings

Recorded so round 8 does not re-derive them.

- **T4's sweep completeness — the thing the brief asked about specifically.** I re-grepped every
  `macOS` / `macos` / `iOS` / `target_os = "ios"` token in `crates/vmem/src/lib.rs` and
  `crates/vmem/README.md` (37 hits) and classified each. All five advice-selection sites T4 named
  are fixed; there is no sixth. The remaining `macOS`-only mentions are correct as written:
  `lib.rs:86`, `:153`, `:376`, `:560` (Apple-Silicon page size — a macOS-specific hardware fact,
  iOS is not a CI/host platform here), `:1402` (`MADV_HUGEPAGE` is a genuine Linux-only hint, so
  "no-op on macOS and other non-Linux Unix" is right), `:2279` (`HUGE_SUPPORTED`, correctly says
  "macOS, iOS, BSD, etc."), and `:192`/`:267`/`:2160`/`README.md:159` (historical references to
  the macOS CI run and item 48's macOS-observed failure, which really were macOS).
- **T2's line numbers.** `lib.rs:963` is `granted_huge: false,`; `:955-967` is the
  `finish_reservation(…)` block; `huge_pages.rs:61-62` is the `#[cfg(not(target_os = "linux"))]`
  + `assert!(!r.is_huge(), …)` pair. All three read in the current tree. Round 7 added no lines
  above `:955` (its first line-count-changing hunk is at `:1048`), so the citations were correct
  when written and still are.
- **The seven merges.** No conflict markers; `git diff 1dbd6b4..HEAD --stat` totals (7 files,
  +176/−84) reconcile with the seven task commits' individual diffs; every merge is `--no-ff`
  with a task-numbered subject.
- **Publish-readiness (task #658).** `cargo package -p aligned-vmem --list` returns the same
  20 files as before the round; no file added, removed, or renamed; `Cargo.toml`'s
  `[package.metadata.docs.rs]`, `version`, `edition`, `rust-version` and feature table are
  untouched by the round's diff. The URL rewrite uses `<…>` autolinks in rustdoc/README, which
  render as links on docs.rs and crates.io. TC4 is the only outstanding publish-facing item.
- **The `.addr()` conversion's runtime equivalence.** `<*mut T>::addr()` and `as usize` produce
  the same integer; only the provenance-exposure semantics differ. `mock::Call`'s fields are
  unchanged `usize`, and the 9 `mock`-feature tests pass under `--all-features`. Stable since
  1.84 vs. the crate's `rust-version = "1.88"` — no MSRV impact.
- **The two `verify-commit-prefixes.mjs` warnings.** Personally verified as false positives
  (both `src/lib.rs` hunks are comments only) — see "What was verified green".
- **T7's remaining nine sites.** Left unfixed *by design*, filed as item 49 rather than dropped.
  That is the correct disposition for an INFO record-only finding; a mechanical
  `unsafe_op_in_unsafe_fn` pass across ten FFI sites in one round would have been out of scope
  and unreviewable against its own zero-trust bar. Only the card's missing field (TC7) is filed.
- **`decommit_lazy_roundtrip`'s vacuousness (item 48's S4 remainder).** Still open, still
  correctly recorded at `docs/CORRECTNESS_OPEN_ITEMS.md:2105`, deliberately untouched by round 7
  — not re-filed here.
- **Performance — null, eighth consecutive pass.** Round 7's diff contains no executable change
  whatsoever outside a `#[cfg]` predicate on one macOS-only test and an `unsafe {}` block that
  changes no generated code. Nothing to measure.
- **Safety — null.** No new `unsafe` token; the one `unsafe {}` block added narrows an existing
  implicit-unsafe-fn body. No new safe `pub fn` takes a raw pointer (CLAUDE.md's benchmark-hook
  rule); no `dbg_*`/`bench-internals` surface changed at all.

---

## Recommended order

1. **TC2** — commit the round-7 review doc (and this one). It is one `git add`, it is a
   precondition for four already-written citations to mean anything, and every hour the file
   stays untracked is an hour it can be lost.
2. **TC1** — write the round-7 CHANGELOG section and bump item 1's counter to 7. Do it in the
   same closing commit as TC2, which is what the standing rule item 1 proposes would have
   required anyway.
3. **TC3** — decide the tvOS/watchOS `MADV_FREE_REUSABLE` question once and make `lib.rs:1123-1126`
   and `docs/CORRECTNESS_OPEN_ITEMS.md:2102` agree, with a `REASONED-FROM-SPEC` marker. This is
   the only finding here that can send a future round down the wrong implementation path.
4. **TC4** — one URL, before 0.2.0 publishes (task #658). Consider sweeping `lib.rs:262` in the
   same pass so the rustdoc surface is finally complete.
5. **TC6** — one sentence in `smoke.rs`'s oracle rustdoc, re-tensing the run from future to past.
6. **TC5**, **TC7** — one clause and one card field.
7. **TC8**, **TC9** — record-only; fold into whatever task next touches those lines.

---

## On the campaign's signature pattern — round 7's honest score

Round 7's remediation is the **most accurate** of the campaign so far on the axis that has hurt
most: not one of the ten fixes landed on wrong content, and the two hardest judgment calls — T1's
"do not write 'H1 confirmed by CI'" sub-note and T7's "fix the one real bug, file the other nine"
disposition — were both executed exactly as the review asked, which is not the norm for delegated
remediation here.

What it reproduced instead is the *process* half of the pattern, twice (TC1, TC2), and the
worktree-isolation half once (TC3). Those three share a root cause worth naming: **round 7's
decomposition assigned every finding an owner and assigned the round's own bookkeeping to nobody.**
Round 6 had a dedicated task G for the CHANGELOG and still got SC3; round 7 had none and got a
missing section, an uncommitted review doc, and a card missing a field. The seven task agents each
did their job; the gaps are all in the seams between them, which is precisely where a
worktree-parallel decomposition has no owner by construction.

TC3 is the one I would not want lost. It is the only finding here that changes what a future
round would *do* rather than what it would *read*: item 48's `Next trigger` picks between two
Darwin fixes on a coverage criterion, and the repository now answers that criterion two different
ways depending on which file you open. The wording is a one-line fix; deciding which claim is true
needs a header read nobody in this campaign has been able to execute, which is exactly why it
should be marked `REASONED-FROM-SPEC` rather than settled by whichever file the next reader happens
to open first.
