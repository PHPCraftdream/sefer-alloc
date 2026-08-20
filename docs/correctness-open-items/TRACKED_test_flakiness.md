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

**Card count:** 4.

**Why split by theme, not by item-number range (task #1222, 2026-08-20):**
task #1221 (same day) split the former single `TRACKED.md` into four
number-range files, balanced by line count. The owner rejected that split
and asked for a thematic split instead -- grouping cards by what they are
actually ABOUT, derived from reading all 70 cards rather than assumed.
Every one of the 42+ code/CI/script citations of this index across the
repo cites an item by NUMBER, never by topic or file, so
`docs/CORRECTNESS_OPEN_ITEMS.md` (the thin index) now carries a complete
item-N to file lookup table covering all 70 numbers (including the
`59a`/`59b` sub-items) -- that table, not this file's name, is what keeps
the by-number citation convention working under a thematic split: the
lookup is two-hop (index table, then this file), but mechanical and
always correct.
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

12. **[T, filed 2026-08-02, task #498] `xthread_large_double_free_no_double_reclaim`
    (`tests/regression_xthread_large_free_no_leak.rs`) failed once during a
    full `cargo test --features production` run, not reproduced on 7
    subsequent runs.** One full-suite run (during task #498's own
    verification pass) reported: `assertion `left == right` failed:
    expected exactly 50 reclaims (one per distinct double-freed segment),
    got 42` — a plausible cross-thread reclaim-counting race under system
    load (this test spawns real OS threads and races a remote double-free
    against the owner's deferred-free drain; see the test file's own module
    doc for the exact shape). NOT reproduced on: 5 consecutive isolated
    `--test regression_xthread_large_free_no_leak -- --test-threads=1` runs,
    1 full-suite re-run of the exact same tree that produced the original
    failure, and 1 full-suite run of the PRE-task-#498 base commit
    (`2dfeaa3`) in an isolated worktree (also clean) — i.e. this is not
    caused by task #498's diff (the base commit, entirely unmodified, was
    tested clean in the same session) and is not reliably reproducible
    on-demand, consistent with a genuine low-probability timing flake in
    the test's own concurrency shape rather than a real bug. Not
    investigated further here (out of task #498's scope; the task's own
    diff does not touch `heap_core.rs`'s deferred-free stack or
    `reclaim_large_segment`'s deposit/release logic — only the header
    WRITE inside the already-registered-or-not-yet-registered window, which
    this specific test's counter never observes). Filed per this file's own
    convention so a future round can watch for a repeat and, if one occurs,
    has this occurrence on record as the first data point.

    **Status: RESOLVED (2026-08-06, task #605/K10).** The above paragraph's
    own "the counter never observes this window" reasoning was wrong — not
    about THIS test's immediate window, but about state carried forward
    from an EARLIER test in the same process via the large-cache. Root
    cause identified with full confidence, not merely hypothesized: task
    #498's own commit `eb2463a` ("large-cache HIT arm writes 4 SegmentHeader
    fields instead of the whole 144-byte struct") replaced a full-struct
    header rewrite on large-cache reuse with 4 targeted field writes
    (magic/large_size/large_align/bump), silently dropping the implicit
    reset of `owner_state`/`owner_thread_free`/`deferred_next` the old
    full-struct write used to perform. A segment that had gone through the
    cross-thread deferred-free path (as several do in this file's OTHER
    tests, `xthread_large_free_reclaims_segments_no_leak` in particular,
    which runs earlier in the same serialized test binary) retains a
    non-`ABANDONED_TAIL` `deferred_next` link value; when the large-cache
    later hands that same segment back out as a "fresh" allocation (a cache
    hit) for THIS test's first loop, and the remote thread subsequently
    frees it, `push_large_deferred_free`'s double-push claim CAS (which
    requires the link word to read `ABANDONED_TAIL`) fails on the FIRST
    free attempt — not the second, deliberate double-free — silently
    dropping that segment from the deferred-free stack entirely. Each
    dropped segment is one fewer reclaim than expected: exactly the
    "got 42, not 50" undercount symptom, for however many of the 50
    allocations happened to land on a stale cache hit in that run.

    This defect was independently found and fixed two days later by an
    unrelated task — R34-14/task #533, commit `7ef5a465cc23e20c518f9163520640aebc7a7ee0`
    ("reset owner/deferred fields on large-cache hit") — whose own commit
    body describes the identical mechanism verbatim ("a segment that went
    through the deferred-large-free path retains a non-`ABANDONED_TAIL`
    link value ... push_large_deferred_free's CAS from `ABANDONED_TAIL`
    FAILS") and ships a dedicated counterfactual regression test,
    `tests/r34_14_deferred_next_reset_on_cache_hit.rs`, that reproduces
    the silent-drop with the reset removed and passes with it restored.
    Nobody connected that fix to closing THIS item at the time — R34-14 was
    framed entirely around its own symptom (a permanent leak), not this
    flake.

    Verified, not merely inferred: (1) `git merge-base --is-ancestor
    7ef5a46 HEAD` confirms the fix is an ancestor of current `HEAD`; (2)
    `cargo test --release --test regression_xthread_large_free_no_leak
    --features "production internals" -- --test-threads=1
    xthread_large_double_free_no_double_reclaim` run 5 consecutive times,
    all green; (3) `cargo test --release --test
    r34_14_deferred_next_reset_on_cache_hit --features "production
    internals"` — the dedicated counterfactual — passes on current `HEAD`.
    No further action needed; this item required no NEW fix, only
    identifying that an already-landed one (for a differently-described
    symptom) already closed it.

14. **[T, filed 2026-08-02, task #499] Flaky (pre-existing, NOT caused by
    task #499's changes) —
    `tests/regression_xthread_large_free_layout_mismatch.rs`'s
    `xthread_large_free_tiny_size_huge_align_is_reclaimed` fails when run as
    part of its own 5-test file (`cargo test --test
    regression_xthread_large_free_layout_mismatch`, default parallel test
    threads) but passes reliably when run in isolation
    (`... xthread_large_free_tiny_size_huge_align_is_reclaimed`, single
    test). Failure shape: `a legitimate tiny-size/huge-align cross-thread
    free was NOT reclaimed (delta 0)` — `DBG_LARGE_XTHREAD_RECLAIMED` did
    not advance the expected amount, at `tests/regression_xthread_large_free_layout_mismatch.rs:334`.
    **Confirmed pre-existing and unrelated to task #499's `maybe_decay_large_cache`
    stride-throttle change:** reproduced identically (same failure, same
    line) on a clean `git worktree add` at commit `48fed64355f03181c6a89f42cab636b800994c7f`
    (the commit immediately BEFORE task #499's changes) with its own
    isolated `CARGO_TARGET_DIR`, ruling out both task #499's own diff and
    cross-contamination from other agents' concurrent builds in this shared
    workspace as the cause. The test uses `SerialGuard::acquire()` (a
    `TEST_LOCK`-style serialization primitive, per this file's own item-13
    citation of the same pattern) but the failure's within-file-only
    reproduction (5/5 runs failed when run with its siblings; 3/3 runs
    passed in isolation, `cargo test ... regression_xthread_large_free_layout_mismatch`
    invoked 3 times back-to-back) points at test-order or shared
    process-wide-counter (`DBG_LARGE_XTHREAD_RECLAIMED` is itself a
    process-wide static, per the test's own imports) interaction with a
    sibling test in the same binary, not a genuine reclaim-logic regression.
    **Not root-caused further** (which sibling test's ordering/timing
    causes the interaction, and whether `SerialGuard` has a gap) — filed
    here so a future round investigating cross-thread reclaim correctness
    or CI flakiness in this file starts from "already reproduced as
    pre-existing, isolated-run-clean" instead of re-diagnosing from
    scratch.

_(item 35 (renumbered from a collision, task #623/M2 — see that item's own
history for the prior "15"/"16" mislabel), the F-2 provenance-asymmetry
hypothesis, was resolved-negative by R34-5 (task #524) — see "Recently
resolved" in RESOLVED.md.)_

63. **Flaky test — `shadow_path_activation_oracle_fast_and_slow_both_reachable` scheduler-sensitive percentage thresholds.** See "Recently resolved" §3 for full resolution.

69. **CLOSED** by task #1063 (added the missing `serial_guard()` call to `windows_virtualfree_release_failures_accessor_exists`). See "Recently resolved" in RESOLVED.md for the full closure narrative.
