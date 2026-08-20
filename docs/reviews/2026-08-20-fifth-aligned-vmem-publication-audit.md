# Fifth independent pre-publication audit — `aligned-vmem` 0.2.0 @ `1b72e73`

Read-only, static + executed. Every finding below was reproduced personally by the auditor on
this checkout (`D:\dev\rust\sefer-alloc`, `main`, `1b72e73`, Windows host). No file was edited,
no git write command was run.

**Provenance.** Conducted 2026-08-20 by an independent agent (`@oh`, Opus, effort=high) under
the brief committed as this task's own record; the brief is reproduced in task #1216's
description and its working copy was `.audit5-brief.md` (scratch, not committed). The audit had
no sub-agents. Unlike the third and fourth audits, this one EXECUTED code: `cargo test`,
`clippy`, `cargo doc` under two feature sets, `cargo package --list`, and all six repository
guard scripts. Where a finding rests on execution the report says so; where it rests on reading
alone it says that too.

**Filename deliberately ASCII-only.** The third and fourth audit reports are named
`…-Сол-кодекс.md`, and those non-ASCII names are exactly what breaks
`scripts/verify-commit-prefixes.mjs` (task #1218): git quotes such paths and the guard's
`startsWith('docs/')` test then fails, while a second bug in the same script leaves it blind to
the file's contents. Until #1218 is fixed a Cyrillic filename reproduces both bugs; after it is
fixed there is still no reason to give a file a name that requires a working guard not to
misfire.

The brief's premise held. **This wave reproduced the class it fixed, in the same wave, 19
minutes apart, on the crates.io front page.**

---

## Findings

### F1 — HIGH — `Advised`'s corrected contract never reached the two publishable artifacts, and one of them was *written by this wave* using the exact sentence the wave then deleted from `src/` as false

**Files:**
`crates/aligned-vmem/README.md:56`, `crates/aligned-vmem/CHANGELOG.md:130`,
`crates/aligned-vmem/src/decommit_outcome.rs:15-17`

**Claimed vs. true.** The fourth audit's M2 (a declared release blocker) said
`DecommitOutcome::Advised` must not promise an OS acceptance under `--cfg aligned_vmem_mock`/miri,
where no syscall runs. Commit `1522d25` (task #1212) states: *"All three sites (the variant,
`lib.rs`, `dispatch_try_decommit`) now say the same thing: **the SELECTED BACKEND accepted the
request**."* Card 93 records `M2 → #1212` as **DONE** inside the blocking set `H1 + M1 + M2 + M3`.

Three sites still carried the deleted wording:

1. `README.md:56` — the API-table row a `cargo add` reader meets first:
   `` `Advised` (the backend call was made and the OS/kernel accepted it — does **not** by
   itself mean the physical pages were reclaimed) ``. No mock/miri qualification.
2. `CHANGELOG.md:130` — inside the **BREAKING** entry that defines the shipping contract: same
   sentence.
3. `src/decommit_outcome.rs:15-17` — the *type-level* rustdoc, rendered directly above the
   corrected variant on docs.rs: *"three variants that distinguish 'nothing was asked of the OS'
   from 'the OS was asked and refused' from 'the OS was asked and accepted'."* Under mock the OS
   is never asked, yet `Advised` is returned unconditionally (`src/api/decommit.rs:331-343`).

Site 3 is inside a line range the fourth audit **explicitly cited**: M2 names
`src/decommit_outcome.rs:10-24,51-66`. The fix changed only `51-66`. Confirmed `10-24` is
byte-identical between `dc2ecdd` and `1b72e73` (`git show 1522d25 -- crates/aligned-vmem/src/decommit_outcome.rs`
— the only hunk is `@@ -48,21 +48,58 @@`).

**Evidence run.**

```
$ git show 05557a6 -- crates/aligned-vmem/README.md | grep -n "OS/kernel accepted"
116:+| `try_decommit(...)` ... `Advised` (the backend call was made and the OS/kernel accepted it ...)
```

`05557a6` (task #1211, 09:05) **added** that sentence to `README.md:56`. `1522d25` (task #1212,
09:24, same wave) **removed** the identical sentence from `src/decommit_outcome.rs` as
incorrect. Two tasks in one wave, each treating the other's file as out of scope; the earlier
one authored the defect the later one was chartered to eliminate.

```
$ grep -rn "OS/kernel accepted it\|asked and accepted" crates/aligned-vmem/README.md crates/aligned-vmem/CHANGELOG.md crates/aligned-vmem/src/
README.md:56 · CHANGELOG.md:130 · src/decommit_outcome.rs:17
```

(`src/decommit_outcome.rs:56` also matches but is the correct, in-context "Native backend"
bullet.)

**New or reproduction:** reproduction of the *M1 class* (publishable docs describing behaviour
the code does not have) committed by the *M1 fix itself*, plus an incomplete fix of *M2* on a
line range the audit named.

---

### F2 — MEDIUM — three shipping rustdoc sites still say "no test reads memory content back"; the commit that certified them clean checked the wrong sub-claim

**Files:**
`crates/aligned-vmem/src/api/decommit.rs:140-145`,
`crates/aligned-vmem/src/api/reserve_aligned_huge.rs:126-128`,
`crates/aligned-vmem/src/api/reserve_aligned_huge.rs:136-138`

**Claimed vs. true.** `decommit.rs:141` — *"It does NOT prove … that a subsequent access
re-faults zeroed memory — **no test on this path reads memory content back**."*
`reserve_aligned_huge.rs:126` — *"**no test reads memory content back**."*
`reserve_aligned_huge.rs:136-138` — *"(3) post-decommit memory CONTENT (physical backing /
zero-fill on next access), **which no test on any platform reads back**."*

All three are false since task #1174. `crates/aligned-vmem/tests/decommit_capability.rs:1034`
(`ci_hugetlb_real_pool_decommit_actually_zeroes_memory_on_reaccess`) writes `0xAB` across a
2 MiB `MAP_HUGETLB` grant, calls `reservation.decommit(0, size)`, then reads every byte and
panics on the first non-zero (`:1090-1101`). It is hard-run and double-sentinelled in CI
(`.github/workflows/ci.yml:563` `grep -F "test ci_hugetlb_real_pool_decommit_actually_zeroes_memory_on_reaccess ... ok"`,
plus the isolated `--exact` marker run at `:584-588`).

The corrected statement already exists in the same crate — `README.md:250-266` and
`src/reservation.rs:778-796` both now say zero-fill-on-readback **is** proven. So the crate
contradicts itself across three publishable surfaces.

**Why it survived:** `1522d25`'s body asserts *"`api/decommit.rs:104-138` and
`api/reserve_aligned_huge.rs:109-138` already said the crate does not prove physical backing and
never mentioned #1174 as open. … Recorded so a later reader does not 'fix' correct text."* Both
sub-claims are true and both are the wrong test. The stale sentence is about **content
readback**, and it sits at `decommit.rs:141` — three lines *below* the cited range, exactly the
off-by-a-paragraph miss the same commit caught for `reservation.rs` (*"The real second site was
`reservation.rs:774-777`, one paragraph off the cited range"*) and did not repeat the check for.
`reserve_aligned_huge.rs` was last touched by `cecdeec` (task #1164), i.e. before #1174 landed;
the sentence rotted and the wave chartered to de-rot it certified it clean.

**Evidence run:** `sed -n '138,146p' src/api/decommit.rs`; `sed -n '118,140p' src/api/reserve_aligned_huge.rs`;
`sed -n '1029,1113p' tests/decommit_capability.rs`; `sed -n '550,600p' .github/workflows/ci.yml`;
`git log --oneline -3 -- crates/aligned-vmem/src/api/reserve_aligned_huge.rs`.

**New or reproduction:** reproduction of the M1 "publish-facing doc describes a superseded proof
strength" class, *inside the M1 fix*, in the two files the fix explicitly declared out of scope.

---

### F3 — MEDIUM — the split's own justification is false for 7 publish-facing citations, which the split turned from resolvable into unresolvable

**File:** `docs/CORRECTNESS_OPEN_ITEMS.md:82-90`

**Claimed vs. true.** The thin index justifies keeping the top-level filename with: *"42
code/CI/script files … cite this exact path, and **every one of those citations is of the form
`` `docs/CORRECTNESS_OPEN_ITEMS.md` item N `` — by NUMBER, never a line number, never a
filename**. As long as this filename resolves and this section stays a complete, accurate
item-N → file lookup, all 42 citations … keep resolving without editing a single one of them."*
`1b72e73`'s body repeats it.

Seven publish-facing citations carry **no item number at all**:

| File | Text |
|---|---|
| `crates/aligned-vmem/README.md:291-292` | "…see \<URL\> **for the open item**." (Darwin gap; the item is 48) |
| `crates/aligned-vmem/README.md:346` | "see `docs/CORRECTNESS_OPEN_ITEMS.md` **for the decision record**." (MIPS) |
| `crates/aligned-vmem/src/api/commit_range.rs:26-27` | "\<URL\> **for the incident this class of bug produces on Windows**." |
| `crates/aligned-vmem/src/api/decommit.rs:178-179` | "\<URL\> **for the open item**." |
| `crates/aligned-vmem/src/api/recommit.rs:30-31` | "\<URL\> **for the incident…**." |
| `crates/aligned-vmem/src/os/unix.rs:740` | "See `docs/CORRECTNESS_OPEN_ITEMS.md`." |
| `crates/aligned-vmem/src/os/unix.rs:745` | inside the MIPS `compile_error!` message a user actually sees |

Five of the seven are `///` rustdoc that ships to docs.rs. Before the split, following one of
those links landed on a 2,566-line file **containing** the card, findable by Ctrl-F. After the
split it lands on a 381-line table of contents that holds **zero card bodies** and offers only
an item-number lookup table — which is useless without an item number. The lookup table's
completeness does not rescue these seven: they are precisely the citations the claim asserts do
not exist.

This is a second visit to the same class: `CHANGELOG.md:406` records task #889 already having
had to repair *"7 publish-facing `docs/CORRECTNESS_OPEN_ITEMS.md` citations"* that *"resolved to
nothing for a crates.io/docs.rs reader."*

**Evidence run:**

```
$ git grep -n -A2 "CORRECTNESS_OPEN_ITEMS\.md" -- 'crates/aligned-vmem/src' 'crates/aligned-vmem/README.md'
```

then per-hit inspection of the following two lines for an `item N` token.

**New or reproduction:** reproduction of the task-#889 "publish-facing citation that does not
resolve" class, re-opened by the split whose own text asserts it cannot happen.

---

### F4 — MEDIUM — the mandatory round-start convention block still describes the *retired* split axis, contradicting the same file 100 lines later

**File:** `docs/CORRECTNESS_OPEN_ITEMS.md:38-39` and `:51`

**Claimed vs. true.** The **Convention (mandatory — see CLAUDE.md "Phased delivery")** block —
the text CLAUDE.md's round-start rule points a fresh session at — says:

- `:38-39` — *"read this index's tier files end-to-end (`ACTIVE.md` then **the four**
  `docs/correctness-open-items/TRACKED_*.md` files, **in number order**…)"*
- `:51` — *"add it to `ACTIVE.md` or the **matching-number-range** `TRACKED_*.md` file…"*

Both describe `1525ccd`'s number-range split, which `1b72e73` replaced. There are now **nine**
thematic files and no number ordering. The same file contradicts itself at `:246-250`: *"**What
to read at round start:** `ACTIVE.md` then **all nine** `TRACKED_*.md` files (**any order** —
unlike the retired number-range split, there is no natural reading sequence across themes)."*

A round that follows the mandatory block literally reads four files that do not exist, in an
order that does not exist, and files new cards by a number range that no longer selects
anything.

**Evidence run:** `sed -n '36,53p'` and `sed -n '244,252p' docs/CORRECTNESS_OPEN_ITEMS.md`;
`ls docs/correctness-open-items/` → nine `TRACKED_*.md`.

**New or reproduction:** reproduction of the "one number living in two places" class that
`4f4d9f4`'s own body names (task #1161) — the wave's third split left the operating instructions
describing its second.

---

### F5 — LOW — nine files assert a "one-hop lookup" that CLAUDE.md and the commit body both say is two-hop

**Files:** all nine `docs/correctness-open-items/TRACKED_*.md`, each at its own line 27

**Claimed vs. true.** Each file's header ends: *"…that table, not this file's name, is what
keeps the by-number citation convention **a one-hop lookup** under a thematic split."*

`CLAUDE.md:149-151` says the opposite: *"a citation by number **stays a two-hop (not one-hop)**
but still mechanical and always-correct lookup."* `1b72e73`'s body agrees: *"A range filename
made that a one-hop lookup; **a subject filename does not**."*
`docs/CORRECTNESS_OPEN_ITEMS.md:105-110` agrees too.

The wave's headline achievement was "pay the navigation cost the new axis creates instead of
noting it" — and the boilerplate it stamped into all nine files denies the cost exists.

**Evidence run:** `grep -c "one-hop lookup under a thematic split" docs/correctness-open-items/TRACKED_*.md`
→ `1` in each of nine files; `sed -n '100,155p' CLAUDE.md`.

**New or reproduction:** new, but the same self-contradiction shape as F4.

---

### F6 — LOW — `recommit`/`try_recommit`'s `# Safety` omits exactly the bound that `#1213/L2` established is a UB precondition

**File:** `crates/aligned-vmem/src/api/recommit.rs:22-31` (and `:43-45`, which forwards)

**Claimed vs. true.** `1522d25` (task #1213/L2) added to `decommit`'s `# Safety`: *"**`end <=
reservation.len()`** … a MANDATORY precondition of the pointer arithmetic this function performs
internally (`base.add(start)`) … Passing `end > reservation.len()` is undefined behavior."*

`recommit`'s Windows backend performs the identical arithmetic — `src/os/windows.rs:396`,
`let addr = unsafe { base.add(start) };` — but its `# Safety` (`recommit.rs:24-31`) states only
"a live reservation whose `[base+start, base+end)` range was previously decommitted", plus
alignment and `start <= end`. No bound, no UB framing. `try_recommit`'s `# Safety` is "Same as
`recommit`", so it inherits the gap.

This is the wave's own rejected framing, verbatim: it argued that leaving the bound to an
adjacent sentence treats it *"as if it were a behavioural preference"* and that *"for an `unsafe
fn`, a bounds requirement that determines whether pointer arithmetic is even defined belongs
inside `# Safety` itself, restated in full."* The sibling `commit_range`
(`src/api/commit_range.rs:22-27`) already spells out `end <= len`, so `recommit` is the one
outlier of the three.

Note this is a documentation-completeness gap, **not a live bug**: both safe methods
bounds-check (`src/reservation.rs:1000-1002`, `:1017-1019`).

**Evidence run:** `sed -n '22,46p' src/api/recommit.rs`; `grep -n "fn recommit_pages_impl" -A 30 src/os/windows.rs`;
`sed -n '990,1025p' src/reservation.rs`.

**New or reproduction:** reproduction of L2's own class, one file over, unfixed.

---

### F7 — LOW — `tests/decommit_outcome.rs`'s first sentence now contradicts its own line 272

**File:** `crates/aligned-vmem/tests/decommit_outcome.rs:1-6` vs `:271-273`

**Claimed vs. true.** Line 1: *"**three counterfactual tests, one per [`DecommitOutcome`]
variant**, each asserting the SPECIFIC variant returned … honoured for all three below."*
Line 271-273: *"the `Refused` variant has **NO deterministic test coverage anywhere in this
crate** as of task #1210."*

The file contains three `#[test]` fns — `advised_…`, `skipped_…_empty_range`,
`skipped_…_huge_page_skip` — i.e. **two** for `Skipped`, one for `Advised`, **zero** for
`Refused`. "One per variant" is flatly false. The header predates the wave
(`git show dc2ecdd:…` — identical), but `1f930e2` rewrote the bullet at `:20-38` twelve lines
below it and left the headline count untouched, in the commit whose stated purpose was recording
the coverage loss honestly.

**Evidence run:** full read of the file; `grep -n "^fn \|^#\[test\]" tests/decommit_outcome.rs`;
`git show dc2ecdd:crates/aligned-vmem/tests/decommit_outcome.rs | sed -n '1,6p'`.

**New or reproduction:** reproduction of the self-falsifying-count class.

---

### F8 — LOW — a line citation invalidated by this wave's own `src/` commit, in a file the wave's next commit rewrote

**File:** `crates/aligned-vmem/tests/decommit_outcome.rs:52` and `:102`

**Claimed vs. true.** Both cite the free `try_decommit`'s empty-range short-circuit as
`api/decommit.rs:387-389`.

At `dc2ecdd` that was exact (`git show dc2ecdd:crates/aligned-vmem/src/api/decommit.rs | sed -n '387,389p'`
→ `if start == end {` / `return Ok(DecommitOutcome::Skipped);` / `}`). `1522d25` (09:24) inserted
~37 lines above it; the branch is now at `424-426`. `1f930e2` (09:28) rewrote this same test file
four minutes later without re-checking.

**Evidence run:** `git show dc2ecdd:crates/aligned-vmem/src/api/decommit.rs | sed -n '380,395p'`;
`grep -n "if start == end" crates/aligned-vmem/src/api/decommit.rs` → `424`.

**New or reproduction:** reproduction of the "cited line numbers drift" class the wave recorded
twice against the audit itself (`dd1061e`: *"the audit's cited `unix.rs` line numbers … had
drifted"*; `05557a6`: *"`CHANGELOG.md:292-355` — content matched, line numbers had drifted
~13"*).

---

### F9 — LOW — `verify-commit-prefixes.mjs` mis-classifies any path containing non-ASCII characters as "outside `docs/`", and is blind to its diff

**File:** `scripts/verify-commit-prefixes.mjs:458-461` (`changedPaths`) and `:423` (the
`diff --git a/\S+ b/(\S+)` regex in `hasNonCommentChange`)

**Claimed vs. true.** Running the guard over this wave produces a warning naming a path that is
plainly under `docs/`:

```
$ node scripts/verify-commit-prefixes.mjs "dc2ecdd..1b72e73"
[verify-commit-prefixes] 1 WARNING(s) (direction 2 — comment-only src/ delta):
  - 105cf53 "docs: task #1209 …" — prefix reads as measurement/docs-only, but 1 changed
    path(s) fall outside docs/examples/benches/tests/scripts/ …
    "docs/reviews/2026-08-20-073908-aligned-vmem-publication-audit-/320/241/…md"
```

Root cause, reproduced directly:

```
$ git show --name-only --format= 105cf53 | cat -A
docs/CORRECTNESS_OPEN_ITEMS.md$
"docs/reviews/…-\320\241\320\276\320\273-\320\272\320\276\320\264\320\265\320\272\321\201.md"$
$ git -c core.quotepath=false show --name-only --format= 105cf53
docs/reviews/2026-08-20-073908-aligned-vmem-publication-audit-Сол-кодекс.md
```

`core.quotepath` defaults to true, so git wraps the non-ASCII path in `"` and octal-escapes it.
`changedPaths` does not unquote, so `p.startsWith('docs/')` is false. Independently,
`hasNonCommentChange`'s `^diff --git a/\S+ b/(\S+)` also fails to match git's quoted
`diff --git "a/…" "b/…"` header, so every added line of such a file is silently attributed to
the previously-seen file and never inspected.

Impact today is a non-blocking false positive (the second bug masks the first:
`hasNonComment === false` downgrades ERROR → WARNING at `:670-679`). But the guard is fail-loud
in one direction and blind in the other, and this repo has committed Cyrillic-named review files
twice already.

**New or reproduction:** new. Falls squarely in the brief's "each guard has, at least once,
claimed a property it did not have" pattern.

---

### F10 — LOW — item 86's card points at `TRACKED.md`, deleted by `1525ccd`, in the present tense

**File:** `docs/correctness-open-items/TRACKED_process_record.md:342`

**Claimed vs. true.** *"…split into a folder, `docs/correctness-open-items/` (`ACTIVE.md` for
`[A]`, **`TRACKED.md` for `[T]` — this card now lives there** — `RESOLVED.md` …). … Split into
`docs/correctness-open-items/{ACTIVE,TRACKED,RESOLVED,ARCHIVE}.md`."*

`TRACKED.md` was deleted by `1525ccd` and does not exist at `1b72e73`. The card lives in
`TRACKED_process_record.md`. `1525ccd`'s body asserts the sweep was complete: *"Everything else
matching the old name is historical narrative in past tense and correctly unchanged."* "this
card now lives there" is not past tense and is not correct. `1b72e73`'s later sweep was scoped to
the *range* filenames only, so it did not revisit this.

CLAUDE.md handles the same narrative correctly (`:144`: *"neither `TRACKED.md` nor the four
`TRACKED_NNN_NNN.md` files exist any longer"*), which is why the guard-free doc drifted alone.

**Evidence run:** `sed -n '336,346p' docs/correctness-open-items/TRACKED_process_record.md`;
`ls docs/correctness-open-items/`.

**New or reproduction:** reproduction of the task-#1116 dangling-index-pointer class.

---

### F11 — INFO — the "42 citations" figure matches neither derivation

**Files:** `docs/CORRECTNESS_OPEN_ITEMS.md:82`,
`docs/correctness-open-items/TRACKED_process_record.md:342`, and both `1525ccd` / `1b72e73`
commit bodies.

```
$ git grep -l "docs/CORRECTNESS_OPEN_ITEMS\.md" -- ':!docs/' | wc -l
43
```

Strictly by the index's own enumerated categories (`src/`, `tests/`, `crates/`, `scripts/`,
`ci.yml`, `CLAUDE.md`) the figure is **41**; adding root `Cargo.toml` (`:2140`, cites "item 11")
and root `CHANGELOG.md` gives **43**. Neither is 42. The figure was also already wrong when
written — `tests/decommit_outcome.rs` cited the index at `dc2ecdd` and lost that citation when
`1f930e2` deleted the test.

Recorded as INFO rather than LOW because the number is used rhetorically ("all 42 keep
resolving") rather than as a gate, but F3 shows the rhetoric is load-bearing.

---

### F12 — INFO (tracked, not new) — M3's blocker wording is still in the published rustdoc

**File:** `crates/aligned-vmem/src/reservation.rs:1288`

*"**Open question, not yet answered by the crate owner:** whether `aligned-vmem` should ever
support adopting a HugeTLB mapping at a granularity other than 2 MiB (e.g. 1 GiB)…"*

This ships to docs.rs under the `package.metadata.docs.rs` feature set. The fourth audit's M3
explicitly required removing it before publication; the wave correctly did **not** claim to
close it (card 93: *"M3 → #1190 … STILL OPEN"*). Confirmed present, confirmed honestly tracked,
confirmed a blocker.

---

## Explicit NULL results

- **Task #1207's NULL is correct.** Extracted and read `.github/workflows/release.yml:255-311`.
  The guard resolves the CHANGELOG path from `cargo metadata` by package name (`:261`), fails
  closed on a missing file (`:268-272`), requires exactly one anchored version section
  (`:283-299`), and only then rejects `unreleased` case-insensitively (`:302-308`).
  `ANCHORED_PATTERN="^## \[?0\.2\.0\]?(\]|$| )"` matches `## 0.2.0 - Unreleased` at
  `crates/aligned-vmem/CHANGELOG.md:7`. The task's original premise ("no guard looks at that
  word") was indeed false. Correctly recorded as NULL.
- **The three splits lost no card.** Not accepted from the commit bodies — re-derived. Card
  bodies parsed out of `git show 105cf53:docs/CORRECTNESS_OPEN_ITEMS.md` (the last pre-split
  state, *including* item 93) and out of `ACTIVE.md` + all nine `TRACKED_*.md` at `1b72e73`:
  **77 vs 76**, the single delta being the literal `3.` of the Convention prose list, which
  correctly stays in the thin index. `LOST: []` for real cards, `ADDED: []`. The 25 changed
  cards are all either the wave's own edits or mechanical `"Recently resolved" below` →
  `in RESOLVED.md` re-pointing.
- **The item-N → file lookup table is exact.** Independently re-derived: 70 table rows parsed
  out of the index, real card locations parsed out of the nine files, compared.
  `table entries: 70, real cards: 70, missing: [], extra: [], WRONG FILE: [], duplicates: []`.
  Per-file counts match the index's stated 4/5/13/18/4/4/9/11/2 exactly.
- **The `Refused` deletion is honestly recorded.** `grep` over the whole tracked tree finds no
  dangling reference to `refused_variant_is_produced_by_a_genuine_os_refusal` — no CI sentinel,
  no workflow grep, no doc. Card 92 (`TRACKED_ci_gate_coverage.md:269`) states the loss without
  softening. The marker fn `refused_variant_has_no_deterministic_coverage_in_this_file` exists
  and is greppable. `grep -rn "Refused" crates/aligned-vmem/tests/` confirms the "zero
  deterministic coverage anywhere in this crate" claim is true, not rhetorical.
- **The "126 passing" claim is exact.** `cargo test -p aligned-vmem --all-features` on this
  host: 126 passed, 0 failed, summed across binaries — matching `1f930e2`'s claim of 126
  (was 127).
- **No stale-binary trap.** The `no_stale_doc_references` binary run has
  `D:\dev\rust\sefer-alloc` baked as `CARGO_MANIFEST_DIR` and contains zero `worktrees` strings
  — the task-#1073 hazard that made an earlier counterfactual falsely pass did not affect this
  run. It reads `correctness-open-items/{ARCHIVE,RESOLVED,TRACKED_ci_gate_coverage,TRACKED_platform_contracts}.md`,
  i.e. the parser really was re-pointed at the new paths, not merely described as such.

---

## Verified clean — already covered, do not re-audit

- **Lookup-table completeness/correctness**, per-file card counts, and card-body preservation
  across all three splits (mechanically re-derived, above).
- **No dangling split-file pointer anywhere in the tracked tree** — every
  `docs/correctness-open-items/*.md` reference in `git grep` resolves to an existing file.
  `docs/perf/OPEN_ITEMS.md:2645` was correctly re-pointed to `TRACKED_platform_contracts.md`.
- **`tests/no_stale_doc_references.rs`** — 16/16 pass under `--features "production internals"`;
  all path builders and every panic/diagnostic string re-pointed at the thematic files.
- **All six guards run and pass**, in both CI and the local gate: `vmem-doc-drift-guard.mjs`
  (exit 0, 40 `.rs` + Cargo.toml + README), `vmem-linux-android-pairing-guard.mjs` (exit 0, 8
  allowlist entries, none new), `verify-ci-sentinels.mjs` (OK — 47),
  `verify-aligned-vmem-bench-internals-exhaustive.mjs` (17/17),
  `verify-vmem-page-constant-call-sites.mjs` (29 fixture self-tests + 567 files, 0 findings),
  `verify-commit-prefixes.mjs` (PASS, modulo F9). Wiring confirmed at
  `.github/workflows/ci.yml:852,933,941,953,972,1037,1043` and
  `scripts/check-all.mjs:787,800,821,834,867,1049`.
- **`cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings`** — clean.
  **`cargo fmt -p aligned-vmem --check`** — clean.
- **`RUSTDOCFLAGS="-D warnings" cargo doc -p aligned-vmem --no-deps`** — clean under
  `--all-features` **and** under the exact `package.metadata.docs.rs` set
  `lazy-commit,huge-pages,fault-injection`. CI runs both rows, deriving the second from
  `cargo metadata` rather than hand-copying it (`ci.yml:308`, `:330-333`) — CLAUDE.md's docs.rs
  feature-set rule is satisfied correctly. The default-feature row emits three pre-existing
  `crate::api::reserve_aligned_huge` dead links (`src/reservation.rs:281`,
  `src/api/decommit.rs:90`, `:162`); these are the known, documented, accepted state and do not
  affect docs.rs, which builds with `huge-pages` on.
- **`cargo package -p aligned-vmem --list`** — succeeds; tarball carries `LICENSE-MIT`,
  `LICENSE-APACHE`, `README.md`, `CHANGELOG.md`, `src/**`, `tests/**`, `benches/`, `examples/`.
  `bench-scale-tool 0.1.0` is a genuine registry dev-dependency with a checksum in `Cargo.lock`,
  not a path dep. `cargo publish --dry-run -p aligned-vmem` runs in CI at `ci.yml:334`.
- **README API table vs. real signatures** — every row cross-checked against `grep`ed
  `pub fn`/`pub unsafe fn` declarations. `try_decommit -> Result<DecommitOutcome, VmemError>`
  (`src/api/decommit.rs:401-405`) now matches `README.md:56`; M1's signature half is genuinely
  fixed. `into_parts`/`into_reservation_parts`/`into_full_parts`/`release`/`release_parts`/
  `is_huge`/`page_size`/`try_page_size`/`PAGE`/`MIN_PAGE`/`leak_zeroed_pages` and the whole
  `LazyReservation` block all match.
- **`README.md` runnable example** matches `tests/readme_example.rs` line for line.
- **`README.md` "six platform and failure-mode divergences"** — six bullets present.
  **CI-verified target list** matches reality: `test-macos` (`ci.yml:1537-1594`) really runs
  `cargo test -p aligned-vmem` on `macos-latest` in both real-backend and mock-cfg rows; i686
  gnu+musl compile checks at `ci.yml:301,303`.
- **M1's CHANGELOG half** — every surviving `Ok(())` in `crates/aligned-vmem/CHANGELOG.md`
  (lines 64, 67, 86, 128, 152, 323, 517) is either explicitly annotated "Superseded"/"as
  shipped" or is correct historical narrative. The layered-annotation choice is consistent with
  the file's pre-existing convention.
- **`#[must_use]` migration (L3)** — `ReservationParts` and `ReservationFullParts` carry
  type-level `#[must_use]` with leak-specific messages; the four redundant function-level
  attributes were removed; `into_parts` (returns a bare tuple), `as_tuple`, and
  `into_reservation` correctly keep theirs. Clippy `double_must_use` clean.
- **L1's atomic-load fix** — `decommit_range_is_well_formed(start, end, ps)` performs no load;
  both callers snapshot once (`src/api/decommit.rs:186`, `:417`). The fail-closed
  `is_multiple_of(usize::MAX)` property is preserved and the caller contract ("pass the UNMASKED
  value") is documented.
- **Safe-method bounds checks** — `Reservation::{recommit, try_recommit, commit_range,
  try_commit_range, try_decommit}` all reject `end > self.len()` before touching a free
  function.
- **`Cargo.toml` metadata** — name/version/edition/rust-version/license/description/readme/
  repository/homepage/documentation/keywords(5)/categories(2) all present and valid;
  `[lints.rust] unexpected_cfgs` declares both build cfgs; no `[dependencies]`, so the "zero
  dependencies" claim holds for consumers.
- **Empty-by-cfg test binaries** are only the expected ones (`decommit_poison_no_panic`, `mock`,
  `mock_reentrancy`, `page_size_override`, `page_size_query_failure`), each covered by its own
  dedicated `--cfg` CI row with `tee` + `grep -F` postconditions. No new green-and-dead binary
  was introduced by the wave.

---

## Verdict

# NO-GO

**Publishing `aligned-vmem` 0.2.0 to crates.io as it stands at `1b72e73` is not safe.**

Two independent grounds, either sufficient:

1. **M3 remains open by the project's own record** (F12). `src/reservation.rs:1288` ships an
   unresolved design question on an `unsafe` boundary to docs.rs. Card 93 says so; the fourth
   audit's release-gate step 4 is unmet.
2. **M2 is recorded as closed but is not closed on the publishable surface** (F1). The
   crates.io front page (`README.md:56`), the CHANGELOG's BREAKING contract entry
   (`CHANGELOG.md:130`), and the `DecommitOutcome` type doc itself
   (`src/decommit_outcome.rs:15-17`) all still tell a reader that `Advised` means the OS
   accepted the call. Under `--cfg aligned_vmem_mock` and under miri no OS call is made. The
   README sentence was authored by this wave and contradicted by this wave 19 minutes later. An
   index that marks a blocker DONE while the blocker's text sits on the front page is worse than
   an index that marks it open.

Minimum to clear, in priority order: **F1** (three sites, one edit each) → **F2** (three sites)
→ **F12/#1190** (owner decision) → **F3** (add item numbers to the seven publish-facing
citations, or restore card bodies at the cited path) → **F4** → F5–F11.

The good news, stated because it would otherwise be lost: the NO-GO **narrowed again**. H1 is
genuinely gone — the out-of-bounds `base.add(64 MiB)` arithmetic is deleted, not re-gated, and
no offset larger than the reservation length reaches any decommit entry point from `tests/`.
M1's signature half is correct. L1/L2/L3 landed with real code changes. The index survived
three consecutive restructurings on the same day with zero card loss and an exact 70/70 lookup
table — a materially better result than the two prior splits of the same file. What did not
survive is the wave's own certification of the files it declined to change.
