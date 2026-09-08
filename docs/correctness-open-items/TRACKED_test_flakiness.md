# Correctness / CI-debt open items -- [T] Tracked tier -- flaky / order-dependent / scheduler-sensitive tests

**Part of the split index.** This file holds the full text of every **[T]**
(tracked, not yet actioned) card whose subject matches this file's own
criterion (below). Start at `docs/CORRECTNESS_OPEN_ITEMS.md` for the
purpose/scope/convention header and the round-start reading order, and for
the complete item-number to file lookup table; come here for these specific
card bodies. See `docs/correctness-open-items/ACTIVE.md` for the **[A]**
tier, `docs/correctness-open-items/RESOLVED.md` for the closure trail, and
the sibling `[T]`-tier files (`TRACKED_hook_safety.md`, `TRACKED_verification_coverage.md`, `TRACKED_platform_contracts.md`, `TRACKED_ci_gate_coverage.md`, `TRACKED_correctness_residuals.md`, `TRACKED_publish_readiness.md`, `TRACKED_process_record.md`, `TRACKED_misc.md`) for the rest of
the tier.

**Criterion for this file:** A card belongs here if it documents a test that fails intermittently because of timing, thread ordering, or shared process-wide state -- an actually-observed nondeterministic failure, not a coverage gap (no test exists) or a platform gap (no runner exists).

**Card count:** 7 (items 12, 14, 63, 69, 96, 143, 145). Only **145** is
OPEN; 12, 14, 63, 69, 96 and 143 are CLOSED pointers whose full narratives
live in RESOLVED.md. Verify, never hand-count:

```text
grep -cE "^[0-9]+\. \*\*" docs/correctness-open-items/TRACKED_test_flakiness.md
```

**Why split by theme, not by item-number range (task #1222, 2026-08-20):**
task #1221 (same day) split the former single `TRACKED.md` into four
number-range files, balanced by line count. The owner rejected that split
and asked for a thematic split instead -- grouping cards by what they are
actually ABOUT, derived from reading all 70 cards rather than assumed.
Every citation of this index that points at ONE SPECIFIC ITEM carries
that item's number, in the form `` `docs/CORRECTNESS_OPEN_ITEMS.md`
item N `` -- task #1227 repaired the seven in `aligned-vmem` that did
not, and two outside it were still open as of that task (both are
recorded in the thin index's Structure section). Citations that point
at the FILE as a whole, at a named SECTION, or at a CLASS of items
rather than one item carry no item number and never needed one (task
#1227's finding; until #1236 these headers overclaimed it as a
universal, asserting that no citation ever pointed at anything but
an item number). Only the numbered citations depend on where item
numbers live, and `docs/CORRECTNESS_OPEN_ITEMS.md` (the thin index)
carries the complete, mechanically generated item-N -> file lookup
table covering EVERY `[T]`-tier number (including the `59a`/`59b`
sub-items) that keeps them resolving -- that table, not this file's
name, is what makes the thematic split safe: the lookup is two-hop
(index table, then this file), but mechanical and always correct. No citing-file
count is typed in this header on purpose: the "42+" typed here at
the split was already 43 (census against the split commit) -- #1230
removed it from one of these nine headers, #1236 from the other
eight; compare against this command's output, never a hardcoded
count:

```text
git grep -l "docs/CORRECTNESS_OPEN_ITEMS\.md" -- ':!docs/' | wc -l
```

(Split 2026-08-20, task #1222, superseding task #1221's number-range
split the same day.)

---

**Items 1-4 (relocated here, task #1222): already-resolved flaky-test stub
pointers, verbatim from the original `[T]`-tier intro text.** These four
lines are not numbered CARDS (they do not match the `^N. **` card-header
pattern the census/reconstruction scripts key on) — they are short
already-resolved pointers that lived immediately below the `### [T]
Tracked, not yet actioned` heading in every prior revision of this file's
ancestor. Relocated here, byte-identical, because their subject (three
flaky-test resolutions and a leak-bound-assertion narrative) matches this
file's own criterion most closely of the nine thematic files. Not part of
the 70-card census reproduced by `docs/CORRECTNESS_OPEN_ITEMS.md`'s
"Card census" section.

_(item 1, the `canary_survives_promotion_and_free_leaves_no_leak` flaky test,
was resolved by an urgent CI-fix task — see "Recently resolved" in RESOLVED.md.)_

_(item 2, the 11 `--features "hardened medium-classes"` clippy dead-code
errors, was resolved by R23-5 (task #374) — see "Recently resolved" in RESOLVED.md.)_

_(item 3, the two flaky coarse-wall-clock tests, was resolved by R23-6
(task #375) — see "Recently resolved" in RESOLVED.md.)_

_(item 4, `canary_survives_promotion_and_free_leaves_no_leak`'s leak-bound
assertion proving no double-release but not no leak, was resolved by R28-2
(task #431) — see "Recently resolved" in RESOLVED.md.)_

---

12. **CLOSED** by task #605/K10 (2026-08-06) — the reclaim-count
    undercount (50 expected, 42 observed) was the large-cache HIT arm
    failing to reset `deferred_next`, fixed by R34-14/task #533 commit
    `7ef5a46` for a differently-described symptom. Re-verified closed
    2026-09-08 (task #1934). See "Recently resolved" in RESOLVED.md for
    the full investigation trail.

14. **CLOSED** by task #1933 (2026-09-08) — the flake did not reproduce
    (20 in-file runs + 8 under `--all-features`, all green), and the
    investigation instead found the file's cross-thread premise violated
    20/20: `HeapRegistry::claim()` handed the spawned thread the OWNER's
    heap, making three `is_dropped` assertions vacuous. Fixed by
    `claim_remote_distinct_from` at all five spawn sites. The registry
    behaviour it exposed continues as item 145 below. See "Recently
    resolved" in RESOLVED.md for the full narrative.

_(item 35 (renumbered from a collision, task #623/M2 — see that item's own
history for the prior "15"/"16" mislabel), the F-2 provenance-asymmetry
hypothesis, was resolved-negative by R34-5 (task #524) — see "Recently
resolved" in RESOLVED.md.)_

63. **Flaky test — `shadow_path_activation_oracle_fast_and_slow_both_reachable` scheduler-sensitive percentage thresholds.** See "Recently resolved" §3 for full resolution.

69. **CLOSED** by task #1063 (added the missing `serial_guard()` call to `windows_virtualfree_release_failures_accessor_exists`). See "Recently resolved" in RESOLVED.md for the full closure narrative.

96. **CLOSED** by task #1935 (2026-09-08) — the aggregate-over-5-rounds fix
    (`2b7cb87`) held: 17 of the 25 most recent `CI` runs are descendants of it
    and none shows this test failing, and 12 local runs read a 0.0-4.4% waste
    ratio against a 20% local / 30% CI threshold. See "Recently resolved" in
    RESOLVED.md for the full three-occurrence history and the closing evidence.

143. **CLOSED** by task #1925 (2026-09-08) — `tests/r14_7_max_segments_ceiling.rs`'s
    two ceiling tests failed non-deterministically on a memory-pressured host
    because a null from `alloc` is ambiguous between "the `SegmentTable` is
    full" (the ceiling under test) and "the OS refused the mapping"
    (`ERROR_COMMITMENT_LIMIT`). Closed by adding the failed-reservation counter
    this card's own remaining-work paragraph specified, plus serializing the
    file's two full-ceiling fills. See "Recently resolved" in RESOLVED.md for
    the full investigation and closure narrative.

145. **[T, filed 2026-09-08, task #1933] `HeapRegistry::claim()` can hand a
    spawned thread the heap another live thread is already using — observed
    20/20, mechanism NOT established.** Found while adding a path-activation
    oracle to `tests/regression_xthread_large_free_layout_mismatch.rs` (item
    14 above). The main test thread calls `HeapRegistry::claim()` and never
    recycles; a spawned thread then calls `claim()` and receives the SAME
    `*mut HeapCore` — byte-identical pointer, therefore the same slot.
    **Evidence:** oracle assert firing with `left: 2130866086736, right:
    2130866086736` (`0x1f021840010` on both sides), reproducing in 20 of 20
    runs of that file under `production internals`; and, separately, that
    recycling such a colliding claim measurably drains the owner's deferred
    frees (`DBG_LARGE_XTHREAD_RECLAIMED` +1), which is only possible if the
    two really are one slot.

    **What is established:** the pointers are equal, systematically, and the
    consequence for the tests was real (see item 14 — three assertions were
    vacuous because of it).

    **What is NOT established, and must not be assumed by whoever picks this
    up:** *why*. `claim()` (`src/registry/heap_registry.rs:131`) takes a slot
    only via `pick_slot()` → CAS `STATE_FREE`→`STATE_LIVE`, so a second
    claimer can only obtain a slot that is FREE — meaning the owner's slot
    was on the free list while the owner still held it. Something recycled
    it. One hypothesis worth checking FIRST, because it is cheap to confirm
    or kill: `recycle()` (`:355`) locates the slot by `heap.id()` and CASes
    `LIVE`→`FREE` with **no generation check**, while `claim()` does bump
    `slot.generation`. A recycler holding a pointer whose claim has since
    been superseded would therefore free a slot it no longer owns, and the
    LIVE→FREE CAS would SUCCEED (the existing defensive branch only catches
    the already-FREE case, i.e. plain double-recycle). Whether any live code
    path — as opposed to this test file's unusual manual claim/recycle usage
    — can actually get into that state is exactly the open question. **Do not
    file this as a production bug until that is shown**; equally, do not
    close it as test-only until it is shown it cannot happen via the TLS
    thread-exit recycle path (`src/global/sefer_alloc.rs:185`, "thread exit
    recycles the slot").

    **Next trigger:** any further test that needs two genuinely distinct
    heaps in one process, or any investigation of item 12 (the sibling
    reclaim-count race in `regression_xthread_large_free_no_leak.rs`, which
    shares this claim/recycle idiom and may share this cause).
    **Workaround in place meanwhile:** `claim_remote_distinct_from` in
    `tests/regression_xthread_large_free_layout_mismatch.rs` — claims until
    distinct and never recycles a colliding claim.
