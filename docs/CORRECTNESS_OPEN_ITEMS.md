# Correctness / CI-debt open items — cross-round tracking index

**Purpose.** A single durable, session-surviving checklist of correctness,
flakiness, and CI-coverage-gap items that a commit message, code comment, or
review doc has flagged as *open / follow-up / "left for later"* — the sibling
to `docs/perf/OPEN_ITEMS.md`, which durably tracks the analogous class of item
but ONLY for `docs/perf/*.md` gate reports and perf design docs (see that
file's own `## Scope`). This file exists because R19-1 (task #337, commit
`46ea2db`)'s own commit message flagged TWO follow-ups — a flaky test and a
clippy dead-code combo — that then existed NOWHERE durable: not in
`OPEN_ITEMS.md` (out of its scope by design — it is not a perf gate report),
not in `CHANGELOG.md`, not anywhere else. Two independent reviews
(`docs/reviews/2026-07-26-crush-review-r19-r21.md` §4 P2 and
`docs/reviews/2026-07-26-oh-review-r19-r21.md` §4.1) both independently
rediscovered this gap, and the flaky item was then independently reproduced
TWICE MORE in Round 22 itself (once during task #352's CI verification, once
during task #356's test run) before this file existed to catch it. This file
is the fix: option (b) from both reviews (a sibling index), not a widening of
`OPEN_ITEMS.md`'s own scope — that file's perf-only narrowness is a deliberate,
working design choice for its own domain and stays intact.

**Scope.** This index covers correctness bugs, flaky tests, and CI-coverage
gaps that originate from ANY source — commit message follow-up notes, code
comments (`TODO`/`FIXME`), or review-doc findings — not just
`docs/perf/*.md` reports. It is the correctness/CI-debt counterpart to
`docs/perf/OPEN_ITEMS.md`, which stays scoped to perf gate reports and perf
design docs only; see that file's own `## Scope` for the boundary and its
cross-link back to this file. When in doubt which index an item belongs in:
if it is about wall-clock/Ir/memory numbers or a perf design's
CONDITIONAL-GO trigger, it belongs in `docs/perf/OPEN_ITEMS.md`; if it is
about a test that can fail spuriously, a lint/build combo that is not
clean, or a correctness contract, it belongs here.

**Convention (mandatory — see CLAUDE.md "Phased delivery").**

1. **Round start:** before forming a new round's task queue, read this
   index's tier files end-to-end (`docs/correctness-open-items/ACTIVE.md`
   then the four `docs/correctness-open-items/TRACKED_*.md` files, in
   number order — alongside `docs/perf/OPEN_ITEMS.md`) and decide, for
   each open item, whether this round closes it, defers it (with a
   one-line reason appended), or leaves it. An item must not be silently
   ignored — every round either moves it or explicitly re-defers it.
2. **When you close an item:** move its entry to
   `docs/correctness-open-items/RESOLVED.md`'s "Recently resolved" section
   with the closing round + task number + one-line evidence (commit / doc
   that records the resolution). Do NOT delete the entry — the closure
   trail is itself the artifact that lets a future reviewer confirm an item
   was actually addressed, not just forgotten again.
3. **When a new commit, comment, or review flags a correctness/CI-debt
   follow-up:** add it to `docs/correctness-open-items/ACTIVE.md` or the
   matching-number-range `TRACKED_*.md` file (matching its tier) in the
   same commit (or an immediate follow-up commit), with a citation back to
   its origin (commit SHA / file:line). A flag that lives only inside a
   single commit message body
   or code comment is exactly the failure mode this index exists to
   prevent.

**Tier key.** **[A]** active — a real next step a round should consider
taking. **[T]** tracked-not-actioned — genuinely reproduced/confirmed but
intentionally not yet scheduled for a fix (root-cause investigation or a
scoping decision is the pending step, not implementation).

---

## Structure — this file is a thin index (split 2026-08-20, task #1217)

**This file no longer holds card bodies.** It was split into a folder,
`docs/correctness-open-items/`, because its own single-file size had grown
past CLAUDE.md's R34-24 ~1,000-line threshold a second time (2,423 lines at
the task #1143 deferral that first declined this split — see item 86 in
`docs/correctness-open-items/TRACKED_044_093.md` for that decision and its
reversal). This split reverses that deferral, at the owner's explicit
request, following the R29-6/R34-24 mechanism this repo already uses for
`docs/perf/OPEN_ITEMS.md` — except one level deeper: instead of a single
main-file + single-archive pair, the OPEN portion itself is split by tier,
because tier is the axis this file already uses as its primary organizing
structure (see the "Tier key" above) and needed no new taxonomy invented to
cut along.

**Why the main file survives as an index, rather than being deleted
outright:** 42 code/CI/script files (`src/`, `tests/`, `crates/`,
`scripts/`, `.github/workflows/ci.yml`, and `CLAUDE.md` itself) cite this
exact path, `docs/CORRECTNESS_OPEN_ITEMS.md`, and every one of those
citations is of the form `` `docs/CORRECTNESS_OPEN_ITEMS.md` item N `` —
never a line number. As long as this filename resolves and stays a
one-hop pointer to whichever tier file item N actually lives in, all 42
citations (several of them in *published* `aligned-vmem` rustdoc that
ships to docs.rs) keep resolving without editing a single one of them.
Deleting this file outright would have forced touching all 42 sites,
including the four publish-facing citations task #889 already had to
repair once, for zero reader benefit — the reader is exactly as well
served by "read the index, follow the tier pointer" as by "grep the
monolith."

**The seven files (the `[T]` tier further split into four number-range
files, task #1221, 2026-08-20 — see the note at the end of this section):**

- **`docs/correctness-open-items/ACTIVE.md`** — the **[A]** tier: active
  cards, a real next step a round should consider taking. Small (6 cards
  at split time).
- **`docs/correctness-open-items/TRACKED_005_008.md`,
  `TRACKED_009_018.md`, `TRACKED_019_043.md`, `TRACKED_044_093.md`** — the
  **[T]** tier: tracked, not yet actioned. The bulk of the open cards (70
  at task #1221's re-split, including the `59a`/`59b` sub-items),
  partitioned by ITEM-NUMBER RANGE (each filename's suffix is its
  inclusive item-number range) rather than by topic, because every
  citation of this index across the repo is of the form `` `docs/
  CORRECTNESS_OPEN_ITEMS.md` item N `` — by number, never by topic — so a
  number-range filename is a one-hop lookup with no translation table.
  Ranges are balanced by LINE COUNT, not card count (card sizes vary from
  2 lines to 293): `005_008` (4 cards/~638 lines), `009_018` (10
  cards/~518 lines), `019_043` (13 cards/~573 lines), `044_093` (50
  cards/~577 lines).
- **`docs/correctness-open-items/RESOLVED.md`** — the "Recently resolved
  (closure trail — do not re-list as open)" section: one-line pointers per
  closed item, each resolving further into `ARCHIVE.md`'s full narrative.
- **`docs/correctness-open-items/ARCHIVE.md`** — the full dated historical
  closure narratives, moved (byte-identical relocation, not re-derived)
  from the retired `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` (the R34-24
  split task #1109 created on 2026-08-18). Consulted on demand, not part
  of the mandatory round-start read.

**What to read at round start:** `ACTIVE.md` then all four
`TRACKED_*.md` files, in number order — together they are the full
OPEN-item content this file's own "Round start" convention rule (above)
already requires reading end-to-end; `RESOLVED.md` and `ARCHIVE.md` are
consulted on demand, exactly as the single-file version's own "Recently
resolved" section and the old `CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` were.

**Task #1221 (2026-08-20): the `[T]` tier's own `TRACKED.md` (2,322
lines) had itself grown past the R34-24 ~1,000-line threshold — the same
rule whose enforcement created it at task #1217 earlier the same day —
and was split again, this time by item-number range (tier was already
exhausted as an axis: every card in that file was `[T]`). `TRACKED.md`
no longer exists; it is fully replaced by the four `TRACKED_NNN_NNN.md`
files listed above. No card body was reworded, only relocated; the two
`tests/no_stale_doc_references.rs` tests that parse specific cards
(items 87 and 59a, both now in `TRACKED_044_093.md`) were re-pointed at
their new path in the same commit.

**Citing an item going forward:** the established convention —
`` `docs/CORRECTNESS_OPEN_ITEMS.md` item N `` — is UNCHANGED and remains
correct; this file stays the canonical citation target precisely so nothing
downstream needs to learn a new path. A reader or script that needs the
card body follows the tier pointer above (or, if the item number is
unknown ahead of time, greps `docs/correctness-open-items/*.md`, which is
the only meaningful behavior change versus grepping the old monolith).

**Card census at split time (task #1217, 2026-08-20; re-derived unchanged
after task #1221's further `[T]`-tier re-split, same day).** Re-derived
during review from the committed files themselves, not from the split's
own working copy — see the paragraph below for why that distinction
mattered here. Reproduce with:

```text
grep -cE '^[0-9]+[a-z]*\. \*\*' docs/correctness-open-items/ACTIVE.md
grep -hcE '^[0-9]+[a-z]*\. \*\*' docs/correctness-open-items/TRACKED_*.md
```

**6 `[A]`-tier cards** (1, 2, 11, 13, 42, 62) **+ 70 `[T]`-tier cards**
(5–10, 12, 14, 16–29, 41, 43–55, 58, 59, 59a, 59b, 60, 61, 63–70, 72–74,
76, 78–93, plus 85 which sits out of numeric order between 47 and 48)
**= 76 total open cards**, plus 40 "Recently resolved" pointer lines
resolving into 38 archive entries (two archive item-number collisions,
`3` appearing twice, predate this split and are inherited unchanged — see
`docs/correctness-open-items/ARCHIVE.md`'s own "Structure" section).
Items 1–4's original flaky-test cards are separately already-resolved
stub pointers inside the `[T]` tier's own intro text, superseded by real
cards later reusing numbers 1 and 2 for unrelated `[A]`-tier findings — a
pre-existing, intentional renumbering documented at task #1143, not a
defect introduced by this split.

**One card was lost by this split and restored in review — recorded
because the loss mechanism is reusable, not because it survived.** The
split was performed in a git worktree branched from a commit that did not
yet contain `105cf53` (task #1209), so its source copy of the pre-split
file was 2,555 lines where the shared checkout held 2,566. Those 11 lines
were exactly **item 93**, the card filing the fourth independent audit's
NO-GO verdict — the single newest card in the file. The split's own
card census PASSED while item 93 was missing, because it compared the
output against that same stale source: a census that re-derives both
sides from one snapshot cannot detect a card the snapshot never had.
This is the same class as task #1116, where the FIRST split of this index
truncated 9 pointers mid-heading and lost the verdict on 19 of 32 — that
one was caught by an independent reader, this one by re-running the
census against `git show HEAD:docs/CORRECTNESS_OPEN_ITEMS.md` instead of
against the worktree. **Census both sides from committed history, not
from the working copy the transformation itself read.**
