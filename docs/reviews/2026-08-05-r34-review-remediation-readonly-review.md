# R34-review remediation wave (F1–F8, tasks #547–552) — independent READONLY review

**Date:** 2026-08-05
**Reviewer:** independent `@oh` session (readonly; no working-tree change other
than this file, which stays untracked per this project's established convention).
**Scope reviewed:** `c5db553..HEAD` = **7 commits** — `4d52cfb`, `73817ee`,
`7faa377`, `e496d8b`, `5710a6e`, `d46c349` (amended from `8e615a1`), `4623dc3`.
The six substantive commits are the remediation of the eight findings in
`docs/reviews/2026-08-05-round34-readonly-review.md`; `4623dc3` is
CHANGELOG + two session checkpoints.
**Method:** `git show`/`git diff`/`git log`/`git cat-file` on every commit;
full diff reads on all six; independent re-derivation of every load-bearing
claim (the 43-commit span and its category partition re-computed as a set from
`git log`; the `~1.8–2.1×` ratios re-checked arithmetically against
`R34_23_REALLOC_AND_VEC_GATE.md`'s own tables; the gzip artifact decompressed
and byte-compared against the original blob in `ba716a0`; the five tripwire
`file:line` citations re-grepped; `miri-plain`'s `MIRIFLAGS` re-read).
Four executions were run (all read-only w.r.t. the tree): the new
check-matrix row, `tests/ci_clippy_matrix_consistency.rs`,
`tests/no_stale_doc_references.rs` + `tests/no_panic_doc_accuracy.rs`, and
both repo verifier scripts (`verify-commit-prefixes.mjs`,
`verify-gate-report.mjs`).

---

## 0. Verdict

**Seven of the eight findings are genuinely and correctly closed.** The
engineering is careful and the zero-trust discipline is visible in the diffs:
F1's fix is structurally non-vacuous (not merely "a row was added"), F2's
correction re-derives the verdict rather than swapping a string, F7's gzip
roundtrip is genuinely byte-identical, and F4/F8 are handled per this
project's own append-only-correction convention rather than by silent
rewriting. `d46c349` is a tree-identical, parent-identical message-only amend
of `8e615a1` — confirmed, not assumed. No TODO/FIXME/placeholder, no
out-of-scope file, no new safe `pub fn` accepting a raw pointer (the wave's
only `src/` change is a module doc comment).

**Seven findings**, none a correctness or soundness defect in shipping code.
One is worth acting on before the next push (`[P2]`); the rest are
documentation/process accuracy. Two of them are the *same defect class the
wave existed to fix, recurring inside the wave itself* — a later task
invalidating an earlier artifact's cited number, with no sweep.

| # | Sev | Finding | Anchor |
|---|-----|---------|--------|
| G1 | **P2** | `73817ee`'s `fix(perf):` prefix is a hard FAILURE of the repo's own R30-12 lint — `npm run check` exits 1 | `scripts/verify-commit-prefixes.mjs` |
| G2 | P3 | F5 only PARTIALLY closed — `CHANGELOG.md:38` still bills the manifest as "all 38 of this round's commits" | `CHANGELOG.md:38` |
| G3 | P3 | The extended manifest still omits one of CLAUDE.md's four mandated manifest elements (raw-log count + total size) | `docs/perf/round-manifests/R34_MANIFEST.md` |
| G4 | P4 | F2 sweep incomplete — `OPEN_ITEMS_ARCHIVE.md` § `L12`, the file the fixed card points to, still asserts the refuted ~40× | `docs/perf/OPEN_ITEMS_ARCHIVE.md:1181` |
| G5 | P4 | `d46c349`'s body says sites 2–5 were "left untouched"; the diff modifies all five entries | `d46c349` commit message |
| G6 | P4 | The new `OPEN_ITEMS.md` entry reuses item number `40`, already taken by a `[D]`-tier item | `docs/perf/OPEN_ITEMS.md:1947` |
| G7 | P4 | `scripts/check-matrix.mjs` (tracked source) cites a review doc that is deliberately never committed | `scripts/check-matrix.mjs:162` |

---

## 1. `4d52cfb` (task #547, F1) — non-vacuous `internals` boundary row — **CONFIRMED, correct**

Every claim in the commit body checks out, and the fix is non-vacuous *by
construction*, not merely by assertion:

**Is the row picked up by CI?** Yes. `.github/workflows/ci.yml`'s
`check-matrix` job's single step is
`node scripts/run-check-matrix.mjs --kind check --kind test` (`ci.yml:203`).
`run-check-matrix.mjs:60-62` filters `PER_PR_ROWS` by `kind`; the new row's
`kind: 'test'` is inside that filter. No ci.yml job-structure change was
needed — exactly as the pre-existing header comment at `ci.yml:174-178`
promised ("a future round adds a non-clippy `PER_PR_ROWS` entry, it lands
here automatically").

**Is it picked up locally?** Yes. `check-all.mjs:72` builds `otherRows` as
`PER_PR_ROWS.filter((r) => r.kind !== 'clippy')` and splices them at
`check-all.mjs:143`. `test` ≠ `clippy`, so it runs.

**Is `tests/ci_clippy_matrix_consistency.rs` untouched?** Yes, verified two
ways. Structurally: `parse_manifest_clippy_rows` (`:100`) collects only rows
whose extracted `kind == "clippy"`, so a `test`-kind row is skipped
regardless of its position. I also confirmed the file's brace-scanning
parser survives the new row's nested `target: { flag, name }` object — it
resolves `obj_end` at the *nested* `}`, reads `kind: 'test'` from that slice,
skips it, and resumes past it; the pre-existing
`check-perf-gate-iai-default` row has the identical shape, so this is not a
new hazard. Empirically: I ran the test — 1 passed.

**Is the row's configuration genuinely the one the guard needs?** Yes.
`rowToCargoArgs` emits
`cargo test --test r34_3_internals_boundary_api --features "alloc-core alloc-global alloc-decommit"`.
That satisfies the test file's `#![cfg(all(alloc-core, alloc-global,
alloc-decommit))]` (line 60-64) while leaving `internals` OFF, so
`src/lib.rs:343` declares `mod alloc_core` as `pub(crate)` and
`src/lib.rs:365/377` do the same for `global`/`registry`. Under that
configuration the crate-root `pub use alloc_core::{AllocCore, SegmentLayout}`
(`src/lib.rs:408`) and `pub use global::{AllocStats, SeferAlloc}`
(`:411`) are the *only* paths by which the test's `use` can resolve — so
moving any of them behind `internals` is a compile error here. The guard can
now fail for the reason it was written.

**Ran it:** `cargo test --features "alloc-core alloc-global alloc-decommit"
--test r34_3_internals_boundary_api` → `2 passed; 0 failed`.

The commit's own counterfactual (temporarily gating `AllocCore` behind
`internals`, observing `E0432`, reverting) I did **not** reproduce — it
requires mutating `src/lib.rs`, which a readonly review must not do. The
structural argument above independently establishes the same property, so the
claim is corroborated rather than merely trusted.

*Residual, not a finding (the original review already named it):* only the
POSITIVE half of the boundary is now guarded. The NEGATIVE half
(`sefer_alloc::alloc_core::*` must NOT resolve without `internals`) remains
unguarded and is honestly disclosed as such in the test file's own module
doc.

---

## 2. `73817ee` (task #548, F2) — `[L]`12 verdict re-derivation — **numbers CORRECT, reasoning SOUND, prefix WRONG (G1)**

**Are the numbers real?** Yes, and they are the report's own, not
reconstructed. `docs/perf/R34_23_REALLOC_AND_VEC_GATE.md:30` publishes
"sefer ~238 µs, mi ~431 µs (criterion); sefer ~210 µs, mi ~444 µs (direct
gate)". Arithmetic re-check: 431/238 = **1.81**; 444/210 = **2.11** — the
card's "~1.8× (criterion) to ~2.1× (direct gate)" is exact in both directions.
The report's §5 (`:263`) states the same pair independently. The
"physically impossible" characterisation is the report's own words at `:145`
and `:252`.

**Is the verdict re-derivation genuine, or a claim wrapped around a string
swap?** Genuine, and I verified its factual premise. The card now argues that
the item's decisive datum "was never the OPT-G ratio, it is the sub-16 KiB
ladder's Stage-1 hit rate, which the item's evidence explicitly says is
'currently-unmeasured'". `docs/perf/OPEN_ITEMS_ARCHIVE.md:1177-1178` — the
item's own archived history — reads: "would plausibly show a 20–50% Stage-1
hit rate — a real, **currently-unmeasured** data point." So the premise is the
item's own text, not a convenient invention, and the conclusion follows: the
corrected ratio removes the *stated reason* for the low-value verdict without
supplying a reason to raise it, which is precisely "downgrade from
confidently low-value to genuinely unmeasured, still low-priority."
`tests/no_stale_doc_references.rs`'s two `OPEN_ITEMS.md`-structural checks
still pass (12/12, run by me).

**But the commit prefix is wrong — see G1 below.** The body argues
"`fix(perf)`, not `perf(runtime)`/`perf(opt-in)`: doc-only". That inverts
CLAUDE.md's own definition of the slot, which opens with "**shipping or
opt-in code changed** to restore a documented invariant"; a doc-only edit is
not in that slot at all.

### G1 [P2] — `73817ee` fails the repo's own commit-prefix lint; `npm run check` is red

`scripts/verify-commit-prefixes.mjs` is **step 14 of `npm run check`**
(`check-all.mjs:180-183`) and its own CI complement is the
`commit-prefix-lint` job (`ci.yml:220-255`). Run against this wave:

```
[verify-commit-prefixes] 1 FAILURE(s) (direction 1 — R30-12 taxonomy violation):
  - 73817ee "fix(perf): re-derive OPEN_ITEMS.md [L]12 verdict off the corrected
    realloc ratio" — prefix claims a shipping/opt-in code fix in perf-sensitive
    code, but every changed path is under docs/examples/benches/tests/scripts/
    (1 path(s): docs/perf/OPEN_ITEMS.md); use bench: or docs(config): instead
    if no shipping/opt-in code actually changed.
[verify-commit-prefixes] FAILED
```

`node scripts/verify-commit-prefixes.mjs c5db553..HEAD` → **exit 1**.

Two aggravating details:

1. **This wave's own CHANGELOG bullet tags the same task `[docs, P2]`**
   (`CHANGELOG.md:47`) — the CHANGELOG and the commit subject disagree about
   what kind of change this was, which is the exact reader-facing confusion
   R30-12 exists to prevent.
2. **The same check was applied by hand, one commit later, and passed** —
   `docs/checkpoints/2026-08-05-0920.md:13` records that task #552's
   sub-agent used a non-taxonomy `docs(fix)` prefix and that it was amended
   to `docs:`. So the taxonomy *was* on the reviewer's mind; the automated
   lint that would have caught the other one simply was never run over the
   wave.

**Additionally — a finding the prior review missed:** the default-range run
(`@{u}..HEAD`, 108 unpushed commits) reports **3** failures, not 1. The other
two are Round 34's own `43115cf` and `5c1142f` (both `fix(perf):` on a
summary-CSV-only change). `docs/reviews/2026-08-05-round34-readonly-review.md`
§7 asserted "Commit-prefix taxonomy (R30-12): correctly applied throughout" —
the repo's own lint disagrees, and has since those two commits landed. So
`npm run check` was already red on this step before this wave and is now red
on three commits. Since the branch is 108 commits ahead of `origin/main` and
CLAUDE.md mandates `npm run check` before every push, this will block (or
should block) the next push regardless of what else lands.

Cheapest fix: `git rebase`-free message amends are no longer safe here
(`73817ee` has five descendants), so either amend nothing and record an
explicit exemption, or add the three SHAs to the script's own
non-retroactive clipping list with a documented reason. Deciding between
those is a judgement call for the maintainer, not something this review
prescribes.

---

## 3. `7faa377` (task #549, F3) — correctness item 15 closed resolved-negative — **CONFIRMED**

**Is the trigger condition genuinely met?** Yes, verified by reading, not by
trusting the commit body. `.github/workflows/ci.yml:860` sets
`MIRIFLAGS: "-Zmiri-disable-isolation -Zmiri-preemption-rate=0.5"` for the
`miri-plain` job. A repo-wide grep for `tree-borrows` across `ci.yml` and
`scripts/miri.mjs` returns **zero** hits, so miri's default Stacked Borrows
model is active — exactly the model item 15's decision rule names. The job's
`run:` step (`ci.yml:900-903`) does list
`--test regression_xthread_small_ring_miri` alongside the two pre-existing
large-block plain-miri tests. The decision rule ("only if that test flags
under Stacked Borrows…") is therefore answered in the negative, and closing
resolved-negative is the correct disposition.

I did not re-run miri locally (~60 s under nightly + a full miri build); the
commit body records a fresh local re-run at 1 passed / ~67 s, which
reproduces #524's own published ~49 s result. Given the CI wiring is
confirmed by reading and the test is in a per-PR-adjacent job, the residual
risk of taking that one number on trust is low.

**Numbering collision in "Recently resolved"?** No. That section's items run
`1, 2, 3, 4, 5, 6, 12, 13, 14, 15` before this commit; the new entry is `16`,
which is free within the section. (The `6 → 12` gap is pre-existing and not
this task's doing.) The open-items list's own item 15 is correctly replaced
with a one-line pointer per CLAUDE.md's R34-24 structural rule, and the
subsequent `16.` still renders as 16 under CommonMark's ordered-list `start`
attribute despite the intervening paragraph.

*Nit, not a finding:* the closure's verdict sentence extends the negative
result to "`atomic_u32_at`/`atomic_u64_at`/`atomic_u8_at`", while the item's
own decision rule named only `atomic_u32_at`, and the new test exercises the
small-block ring (`atomic_u32_at`). `atomic_u64_at`'s
`SegmentHeader::owner_state` is in fact driven by the two large-block
plain-miri tests in the same job, so the extension is defensible — but it is
an extension the item's literal rule did not authorise, stated without that
caveat.

---

## 4. `e496d8b` (task #550, F5+F6) — manifest span + CHANGELOG attribution — **F6 CONFIRMED, F5 PARTIAL (G2)**

**Is the span really 43?** Verified by me:
`git log --oneline 40241b0..c5db553 | wc -l` → **43**;
`git log --oneline 40241b0..8cb89ea | wc -l` → **42**. Both counts in the
manifest's preamble and §4 are correct.

**Do §1's aggregate counts sum to 43 without double-counting or omission?**
Yes — verified as a **set**, not just as a sum. The nine category rows list
6 + 0 + 8 + 15 + 3 + 6 + 2 + 1 + 2 = **43** SHAs, and I matched every one of
those 43 SHAs against the 43 commits `git log 40241b0..c5db553` actually
produces: every commit appears in exactly one category, and no category
lists a SHA outside the span. §1's table rows are numbered 1…43 contiguously
with no duplicate.

**Is the `a9edc87` attribution correct?** Yes. `a9edc87` is
"fix(perf): promote RemoteFreeRing cached_head from Relaxed to
Acquire/Release", which `CHANGELOG.md:20` attributes to R34-6/task #525; the
manifest's §1 row 13 and §2 row now agree with it, and §2 additionally folds
in `7aeee2d` (the rustfmt-drift sibling) with an explicit note. Correct.

**F6:** `CHANGELOG.md:17` now reads "Commits `27879af`+`b47cc6a`+`0762772`
(the module/test cfg-gate + CI sync — the actual substance of the task), plus
untagged follow-up `f9ae91f`". Verified: `27879af` is
"feat(api): gate alloc_core/global/registry behind new `internals` feature",
i.e. the commit that actually creates the boundary; `f9ae91f` is a
**1-line** `docs/ARCHITECTURE.md` edit. The correction is exactly right, and
labelling `f9ae91f` rather than deleting it preserves the trail. **F6
closed.**

### G2 [P3] — F5 is only partially closed: the CHANGELOG still bills the manifest at 38

The original review's F5 named **two** CHANGELOG sites: the header pointer
(`CHANGELOG.md:10`) and the claim that the manifest "classifies all **38** of
this round's commits". Only the first was fixed. `CHANGELOG.md:38` (the
R34-24 bullet) still reads:

> `docs/perf/round-manifests/R34_MANIFEST.md` is the first real instance,
> classifying **all 38 of this round's commits**.

After `e496d8b` the manifest classifies **43**. So the CHANGELOG now
contradicts the artifact it describes — a state strictly worse than before
the fix, where at least the two agreed on 38. This is the *same defect class
the entire wave was created to remediate* (a later task invalidating an
earlier artifact's cited number with no sweep), recurring inside the
remediation itself, and in a file the same commit had open.

The fix is one number. Note that the surrounding sentence is a Round-34
historical bullet, so a bare edit is arguably a silent rewrite; the wave's own
F8 precedent (append a dated correction rather than overwrite) or a short
parenthetical ("38 at the time; extended to the full 43-commit span by task
#550") would be more consistent with this project's conventions.

### G3 [P3] — the extended manifest still omits a mandated element

CLAUDE.md's round-manifest rule requires four things, and adds: "The manifest
**also records the count and total size of raw-log files committed that
round**, making aggregate `docs/perf/` growth visible per-round". The
manifest's §3 names three individual raw logs with individual sizes
(`145 KiB`, `91 KiB`) but nowhere states a count or an aggregate size for the
round. A `grep -i 'raw.\?log'` over the file returns one hit, and it is a §2
narrative cell, not a census.

This is inherited from R34-24's original version, not introduced by `e496d8b`
— but `e496d8b` is the pass that re-opened the file specifically to bring it
into compliance with the round's real shape, and the manifest is explicitly
positioned as "the reference example future rounds should match", so the
omission now propagates as a template. Two lines would close it
(`find docs/perf -name '_raw_*.log' -newer …` style census, or the simpler
"N raw logs, M KiB total, committed this round").

---

## 5. `5710a6e` (task #551, F7) — tier-2 gzip remediation — **CONFIRMED, cleanly executed**

Verified end-to-end, with the roundtrip checked against the *original blob*
rather than against a local re-run:

- `git ls-files docs/perf/r34_23_runs/` returns exactly two entries: the new
  `.gz` and the untouched 69 KiB `_vec_raw.json`. The uncompressed
  `2026-08-04T22-03-44-381Z_direct_raw.json` is gone from the index **and**
  from the working tree (`ls` confirms two files only) — no
  both-forms-committed duplication.
- `git show ba716a0:…direct_raw.json | wc -c` → **263,907**.
  `gunzip -c …json.gz | wc -c` → **263,907**, and `cmp` of the two streams
  reports no difference: **byte-identical roundtrip, independently
  confirmed.**
- The decompressed stream parses as JSON (`timestamp`, `task`, `identity`,
  `samples`, `cells`), and its `identity` block carries the
  `git_write_tree` immutable-source SHA the gate report cites — i.e. the
  compression did not cost the report its provenance chain.
- `.gz` is 8,674 bytes, comfortably under the 200 KiB tier-1 ceiling; the
  choice of gzip over truncation is justified in-commit against CLAUDE.md's
  own tier-2 point 2(b) and the justification is accurate (the file is
  uniform per-sample records the summary CSV derives from in full).
- `.gitignore` gains `/docs/perf/r34_23_runs/` with an explicit note that it
  does not untrack already-committed files — correct git semantics, correctly
  stated.
- `R34_23_REALLOC_AND_VEC_GATE.md`'s artifact table now cites the `.gz` path
  with a `gunzip -k`/`zcat` note, and gains a dated task-#551 note.
  `node scripts/verify-gate-report.mjs` → PASS (104 reports scanned).
- The `OPEN_ITEMS.md` entry is a proper current-state card and — unusually
  good practice — separates the CLOSED specific violation from the still-open
  general gap, with an explicit numeric reopening trigger ("a THIRD
  tier-2/tier-3-sized raw artifact outside the `_raw_*.log` convention").

*Worth stating plainly, though the commit does not claim otherwise:* the
258 KiB blob remains permanently in git history (`ba716a0`), so this does not
reduce clone size — it stops the file being a *standing* tier-2 violation at
HEAD. Neither the commit nor the OPEN_ITEMS entry overstates this. Not a
finding.

### G6 [P4] — the new entry reuses item number `40`

`docs/perf/OPEN_ITEMS.md:1020` already carries a `[D]`-tier item **40**
("R30_7 CSV-naming mismatch"); the new F7 entry at `:1947` is also **40**, in
the `[L]` tier. Duplicate numbers across tiers are pre-existing here (13 and
38 are each used twice already), and the archive's `<tier-letter><number>`
anchor scheme (`D40` vs `L40`) technically disambiguates — but prose
references of the form "OPEN_ITEMS.md item 40", which both the commit message
and `CHANGELOG.md:49` use, now resolve ambiguously. The next globally-free
number is 43.

---

## 6. `d46c349` (task #552, F4+F8) — tripwire citation + ALLOC_BENCH correction — **CONFIRMED**

`d46c349` and `8e615a1` share the identical tree
(`84b8052…`) **and** the identical parent (`5710a6e`), so the amend was
message-only — confirmed by `git rev-parse`, not assumed.

**F4.** The commit's factual premise is exact: `grep -n` puts the
`"known-base realloc called for a segment not owned by this core"` message at
`src/alloc_core/alloc_core.rs:2205`, not the doc's cited `:2158`. And sites
2–5's cited line numbers were **still byte-exact at the time of the edit** —
I re-verified all four independently:
`large_cache_slot_take: empty base slot` → **147**;
`… empty extension slot` → **160**;
`unreachable!("large_cache_slot_take: idx out of base range …")` → **166**;
`unreachable!("large_cache_slot_set: …")` → **321**. The chosen remedy
(drop line numbers from all five, keep file + function name, add a note
explaining why) is the drift-proof option and is consistent with
`tests/no_panic_doc_accuracy.rs`, which pins by message string and
occurrence count. That test still passes (2 passed, run by me), as does
`no_stale_doc_references.rs` (12/12).

**F8.** `docs/ALLOC_BENCH.md`'s original table is **fully intact**: the
`9.67 µs / 39.6× faster` and `~1,500× faster` rows are still at `:247-248`
verbatim, with the correction added *below* them as a dated blockquote — a
genuine append-only correction, not a silent rewrite, exactly as the commit
claims. The note states both directions of drift (the geometric row
overstated at ~40× vs a real ~1.8–2.1×; the neighbour-pressure row
*understated* at ~1,500× vs a re-measured ~3,350×), which is more honest than
correcting only the embarrassing direction. Both figures match
`R34_23_REALLOC_AND_VEC_GATE.md:30-31` and its §2.2 table row
(`neighbour_pressure`: 400 ns vs 1,343,900 ns → 3,359×, cited as ~3,350×).

### G5 [P4] — the commit body contradicts its own diff on sites 2–5

The F4 paragraph reads:

> Sites 2-5 (alloc_core_large_cache.rs:147/160/166/321) were independently
> re-verified byte-exact and **left untouched**.

The diff **does** modify sites 2–5: line numbers are dropped from all five
entries, and entry 3 additionally gains an explicit `in large_cache_slot_take`
that the line-numbered original left implicit. The intended meaning ("the
*source* at those sites was untouched, and their cited line numbers were still
correct") is recoverable from the next paragraph ("dropped line numbers from
all five entries"), but the sentence as written asserts the opposite of what
the diff shows, and it is the sentence a reviewer scanning the message for
scope would key on. Low impact — no wrong code, no wrong doc — but it is
precisely the kind of commit-message/diff mismatch this project's zero-trust
convention exists to surface.

### G4 [P4] — F2's sweep stopped one file short of the file its own card points to

`docs/perf/OPEN_ITEMS_ARCHIVE.md:1178-1184` (§ `L12`) still reads:

> `realloc_grow_geometric` (64 B→4 MiB) is already reported as **~40× faster
> than `mimalloc`** (9.7 µs vs 383 µs; `README.md:244-245`/`:639`)

Three problems compound: (a) the ~40× / 9.7 µs figure is the one R34-23
called physically impossible; (b) the `README.md:244-245`/`:639` pointers now
land on completely unrelated content (`:243-246` is small-pool latency prose;
`:638-641` is the `unsafe`-inventory table) — README's live realloc rows are
at `:918` / `:923` / `:1203` and are correctly corrected; and (c) the
current-state card `73817ee` *did* fix ends with
"Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L12`", so a reader who
follows the card's own pointer lands on the uncorrected text.

The archive is deliberately a historical record under an append-don't-rewrite
convention, so overwriting it would be wrong — but this same wave established
the correct treatment for exactly this situation, one commit later: `d46c349`
added a dated correction blockquote beside an equally-archival stale figure in
`docs/ALLOC_BENCH.md` rather than leaving it unflagged. Applying F8's remedy
to § `L12` would make the two consistent. P4 because the live decision surface
(the card) is correct and now points at R34-23.

---

## 7. `4623dc3` (task #554) — CHANGELOG + checkpoints — **accurate**

The new "Post-closing independent review remediation" subsection is
append-only (it does not rewrite any pre-existing Round 34 bullet), lands
after R34-26's entry, and each of its six bullets cites the correct SHA. Spot-
checked every claim in the F1 and F5+F6 bullets against the diffs — all
accurate, including the "Runtime improvements: 0" line (the wave's only `src/`
change is a doc comment). The two committed checkpoints are honest, including
about process failures (the peak-hours provider refusal, the premature edit to
`crush-fallback.md`, and the `docs(fix)` → `docs:` amend). The deliberate
exclusion of the four `docs/reviews/*` reports and `.claude/` matches this
project's established convention and is stated in the commit body.

### G7 [P4] — a tracked source file now cites a file that is never committed

`scripts/check-matrix.mjs:162` embeds
`'R34 review finding F1 (P2, docs/reviews/2026-08-05-round34-readonly-review.md §5): '`
as the row's runtime-printed `note`. That path does not exist in any clone —
review reports are deliberately kept untracked (`git ls-files docs/reviews/`
lists 74 files; none of the four current ones). So a CI log line will point a
reader at a path that cannot be opened.

There is direct precedent from the round being remediated —
`tests/regression_xthread_small_ring_miri.rs:3` cites the equally-untracked
`docs/reviews/2026-08-04-release-stabilization-audit.md` — so this is a
convention-level tension (uncommitted review artifacts vs. committed source
citing them), not a regression introduced here. Flagged only so the tension is
on the record; the surrounding sentence is self-explanatory enough that the
note still carries its meaning without the file.

---

## 8. Wave-level checks

**Are any of F1–F8 unclosed or only partially closed?**

| Finding | Status |
|---|---|
| F1 (P2, vacuous boundary test) | **CLOSED** — verified structurally + executed |
| F2 (P2, stale ~40× in live card) | **CLOSED** for the live card; residual stale copy in the archive it points to (G4) |
| F3 (P3, stale BLOCKED tag) | **CLOSED** |
| F4 (P3, stale line citation) | **CLOSED** |
| F5 (P3, manifest span + CHANGELOG framing) | **PARTIAL** — manifest fixed; `CHANGELOG.md:38`'s "all 38" survives (G2) |
| F6 (P3, R34-3 commit list) | **CLOSED** |
| F7 (P3, tier-2 violator) | **CLOSED** |
| F8 (P4, ALLOC_BENCH figure) | **CLOSED** |

**New TODO / FIXME / placeholder / half-wired code?** None.
`grep -rE "TODO|FIXME|XXX|unimplemented!|todo!\(\)"` over every file the wave
touched (`src/global/sefer_alloc.rs`, `scripts/check-matrix.mjs`,
`scripts/check-all.mjs`, `.gitignore`) returns zero hits.

**Out-of-scope edits?** None. The wave's 15 changed paths are all either
named in their commit's own subject/body or a necessary consequence of it.
`5710a6e`'s `.gitignore` edit is in-scope (it is half of F7's remedy) and is
the sole reason for the prefix lint's one WARNING, which is correct and
benign.

**New safe `pub fn` taking a raw pointer and touching allocator metadata?**
None — CLAUDE.md's benchmark-hook rule is not engaged at all. The wave's only
`src/` diff is 19 lines of module doc comment in
`src/global/sefer_alloc.rs`; no function, signature, `cfg`, or feature gate
changed anywhere in `src/`.

**Commit-prefix taxonomy (R30-12).** `test:` (`4d52cfb`) is the
outside-taxonomy `test` slot — correct for a CI/test-infrastructure-only
change. `docs:` ×3 (`7faa377`, `e496d8b`, `d46c349`) and `docs(config):`
(`5710a6e`) are correct. `fix(perf):` (`73817ee`) is **wrong** — see G1. The
`docs:` on `4623dc3` is correct.

**Did the wave break anything?** No. Everything I ran is green:
`ci_clippy_matrix_consistency` (1 passed), `no_panic_doc_accuracy`
(2 passed), `no_stale_doc_references` (12 passed, including the two
`OPEN_ITEMS.md`-structural checks and `honest_reject_sections_are_indexed`),
the new check-matrix row (2 passed), and `verify-gate-report.mjs` (PASS, 104
reports). The only red gate is `verify-commit-prefixes.mjs`, and that is G1.

---

## 9. What I checked and found clean (no finding)

Recorded so a later reader knows these were covered, not skipped:

- `8e615a1` → `d46c349`: identical tree SHA and identical parent — a
  genuine message-only amend, not a silent content change.
- The 43-commit round span, re-derived twice (42 to `8cb89ea`, 43 to
  `c5db553`) and matched as a set against the manifest's nine category rows —
  every SHA present exactly once, none extraneous.
- `27879af` really is the commit that creates the `internals` feature;
  `f9ae91f` really is a 1-line doc-count edit. F6's re-ordering is factually
  right.
- Every `~1.8×` / `~2.1×` / `~3,350×` figure traced to
  `R34_23_REALLOC_AND_VEC_GATE.md`'s own tables and re-computed from the raw
  µs/ns values.
- The gzip artifact's byte-identical roundtrip, checked against the original
  git blob rather than a local regeneration.
- `miri-plain`'s `MIRIFLAGS` and test list, read directly from `ci.yml`;
  zero `tree-borrows` occurrences repo-wide in `ci.yml`/`miri.mjs`.
- The five tripwire sites' current line numbers, all re-grepped.
- `docs/ALLOC_BENCH.md`'s original rows still present verbatim below the
  correction (append-only confirmed, not asserted).
- `tests/ci_clippy_matrix_consistency.rs`'s brace-scanning parser against the
  new nested-`target` row shape (both structurally and by execution).
- `docs/CORRECTNESS_OPEN_ITEMS.md`'s "Recently resolved" numbering — no
  collision at 16; markdown list continuation across the replaced item 15
  renders correctly.
- Repo-wide grep for surviving `~40×` / `9.67 µs` / `9.7 µs` claims — one
  live site remains (G4); every other hit is correction context, a review
  doc, a checkpoint, a commit-subject quotation in the manifest, or an
  unrelated `1.40×`.
- `git status` — the only untracked paths are `.claude/` and the four review
  reports, all correctly excluded by `4623dc3`.

---

## 10. Recommended follow-ups (suggested priority)

1. **G1** — decide and record a disposition for the three `fix(perf)` prefix
   lint failures (`73817ee` from this wave; `43115cf`/`5c1142f` from
   Round 34) before the next push, since `npm run check` currently exits 1
   and the branch is 108 commits ahead of `origin/main`.
2. **G2** — close F5's second half: `CHANGELOG.md:38`'s "all 38 of this
   round's commits" vs the manifest's 43 (append a parenthetical rather than
   silently rewriting the historical bullet).
3. **G3** — add the raw-log count + total size to `R34_MANIFEST.md`, the one
   element of CLAUDE.md's four-part manifest rule it still omits — it is the
   template future rounds copy.
4. **G4** — apply F8's own remedy (a dated correction note) to
   `OPEN_ITEMS_ARCHIVE.md` § `L12`, the file the corrected `[L]`12 card
   points readers to.
5. **G5/G6/G7** — cosmetic: note the sites-2–5 wording mismatch for future
   commit messages; renumber the new `OPEN_ITEMS.md` entry `40` → `43`; decide
   whether tracked source may cite deliberately-uncommitted review reports.

---

*This report is a local, untracked artifact by convention — it is not intended
to be committed.*
