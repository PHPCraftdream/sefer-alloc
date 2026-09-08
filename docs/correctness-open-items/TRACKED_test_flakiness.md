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

**Card count:** 7 (items 12, 14, 63, 69, 96, 143, 145). Only **96** and
**145** are OPEN; 12, 14, 63, 69 and 143 are CLOSED pointers whose full
narratives live in RESOLVED.md. Verify, never hand-count:

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

96. **[T, filed 2026-08-23, task #1247, mitigated 2026-08-30] `wasted_dirty_drains_stays_low_under_class_aware_routing`
    (`tests/class_aware_dirty_routing.rs`, around lines 680-709) failed once in CI on the F1-F4
    push (commit `7037a4c`), not reproduced on immediate rerun.** Failure:
    `assertion failed: ratio < 0.20` with the observed ratio at 26.7%
    (`ratio = wasted / drained` from `run_round(8)`) — the test's own threshold comment
    (lines ~695-701) already documents this as an accepted risk: "20% is a generous
    ceiling... tolerating real-world scheduler jitter and the rare cross-class same-segment
    carve." Confirmed transient, not a regression introduced by the F1-F4 push: the push's
    diff (tasks #1240/#1242/#1244/#1246, range `a1554aa..7037a4c`) never touches
    `tests/class_aware_dirty_routing.rs` or any file `class_aware_dirty_routing`'s own
    module doc names as production dependencies; `gh run rerun <run-id> --failed` on the
    same landing SHA came back 100% green across all 40 non-skipped jobs, including the one
    that had failed. Filed per this file's own convention (see item 12's identical shape —
    one CI-observed failure, confirmed non-reproducible, recorded as a data point) so a
    repeat occurrence has this one on record rather than being independently re-diagnosed
    from scratch. Not investigated further here — the test's own threshold already accepts
    this failure class; tightening it or replacing the ratio-threshold approach entirely is
    out of scope for a filing task.

    **Second occurrence (2026-08-30, `test (feature isolation)` job, commit `296628a`):**
    same test, same shape — `ratio < 0.20` tripped at 26.7% again, not reproduced on an
    immediate `gh run rerun --failed` of the same commit (100% green, including Kani).
    Unrelated to the landing commit's actual diff (`crates/tagged-index-stack/**` +
    two new CI steps in `.github/workflows/ci.yml`, neither touching this test or its
    production dependencies) — confirmed by reading the test's own code before accepting
    the rerun-green signal at face value, not just trusting the precedent. **Mitigated**
    (not fully eliminated — a true 30%+ CI-jitter spike remains theoretically possible)
    by owner request: the assertion now uses a **dual threshold** — `0.30` when the
    standard `CI` environment variable is set (GitHub Actions, and effectively every other
    CI provider, sets `CI=true`), `0.20` otherwise, so a local `cargo test` run keeps the
    original tighter bound while CI gets more headroom against exactly the shared-runner
    jitter both occurrences exhibited. See the commit that added this for the exact diff.

    **Third occurrence (2026-09-03, `test (feature isolation)` job, commit `0310fdb`):**
    same test, now tripping the WIDENED 30% CI ceiling itself — observed ratio 33.3%
    (`drained=15, wasted=5`). Unrelated to the landing commit's diff (a checkpoint-doc-only
    commit on top of `crates/tagged-index-stack/**` + doc-comment-only changes in
    `src/registry/heap_registry.rs`/`bootstrap.rs`/`src/lib.rs`, none touching
    `class_aware_dirty` machinery — confirmed by reading `git log`/`git diff` over the
    relevant range before accepting this as transient). Rather than widening the threshold a
    third time (masking the same single-sample noise floor, not addressing it), fixed the
    real mechanism: a single `run_round(8)` has a small denominator (~15 drains), so one
    scheduler-jitter-induced extra wasted drain moves the ratio by ~6.7 points — exactly the
    granularity both prior occurrences and this one show. The test now runs 5 independent
    rounds and asserts the threshold against the AGGREGATE ratio (same accepted 0.20/0.30
    thresholds, unchanged, now applied to a ~5x larger and materially less noisy
    denominator) — this can only reduce the false-positive flake rate, never raise it, since
    a real regression to ~95% waste would read ~95% in every round and therefore in the
    aggregate too. Verified locally: 4 consecutive runs of the fixed test all read 0.0%
    (drained_total in the 48-92 range across runs). Left as **[T]**, not closed — a
    structural fix, but CI has not yet re-observed this test post-fix across enough runs to
    call the flake class eliminated.

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
