# Correctness / CI-debt open items — [T] Tracked tier (items 9-18)

**Part of the split index.** This file holds the full text of **[T]**
(tracked, not yet actioned) cards **9 through 18**. Start at
`docs/CORRECTNESS_OPEN_ITEMS.md` for the purpose/scope/convention
header and the round-start reading order; come here for these
specific card bodies. See `docs/correctness-open-items/ACTIVE.md` for the
**[A]** tier, `docs/correctness-open-items/RESOLVED.md` for the closure
trail, and the sibling `TRACKED_005_008.md` / `TRACKED_019_043.md` /
`TRACKED_044_093.md` files for the rest of the **[T]** tier's number
ranges.

**Why split by number range, not by topic (task #1221, 2026-08-20):**
this file is one of four that together replace the single
`docs/correctness-open-items/TRACKED.md` (2,322 lines, task #1217),
which had itself grown past CLAUDE.md's R34-24 ~1,000-line threshold.
Every one of the 42+ code/CI/script citations of this index across the
repo cites an item by NUMBER (`` `docs/CORRECTNESS_OPEN_ITEMS.md` item
N ``), never by line or topic — so a number-range filename is a
one-hop lookup with no translation table required. Ranges were chosen
to balance by LINE COUNT, not by card count: this file is 10 cards /
~518 lines; see the sibling files for the other three ranges (4
cards/~638 lines; 13 cards/~573 lines; 50 cards/~577 lines). (Split
2026-08-20, task #1221.)

---

9. **[T, filed 2026-07-31, UNVERIFIED-BY-ME findings from the Round 31 full
   independent review (`docs/reviews/2026-07-31-r31-full-review.md` §7
   P2-4, P2-5, P2-11, P2-12)]** The following four P2 findings were NOT
   independently re-verified before filing — flagged here at the review's
   own confidence/severity, for a future round to check and either action
   or dismiss, per this file's own convention (item 8 above is the direct
   precedent for this exact "filed, not fixed" pattern, one round earlier).
   Note: the review's P2-6 (`ReservedSmallSegment` should be `#[must_use]`)
   is NOT filed here — it was fixed directly in the same task that filed
   this item (one-line, zero-risk, per the task brief's own instruction to
   check first) — see the Round 31 review-response CHANGELOG entry.
   - **P2-4 — `ReservedSmallSegment`'s `pub(super)` scoping doc claim is
     wrong in three places.** The review's claim:
     `src/alloc_core/reserved_small_segment.rs:23-27` and `:80-85` say
     `new_from_reservation` is "callable only from within
     `alloc_core_small_pool.rs`'s own module tree," and `:108-112` says
     `into_base` is "not exposed outside this module tree" — both
     overstate. Actual scope is `pub(in crate::alloc_core)` (since
     `reserved_small_segment` is declared `pub mod` as a direct child of
     `alloc_core` in `src/alloc_core/mod.rs:99`), reachable from every
     sibling module under `alloc_core` (`alloc_core_large.rs`,
     `alloc_core_small.rs`, `alloc_core_small_magazine.rs`, …), not just
     `alloc_core_small_pool.rs` — Rust has no sibling-module-only
     visibility, so the stated scoping is not even expressible. The review
     states this is NOT a live exploit (whole-repo grep found exactly one
     caller of each) and the load-bearing property (external
     unforgeability across the crate boundary) is unaffected — a
     documentation-only defect. Suggested fix per the review (doc-only):
     "reachable from anywhere inside `alloc_core`; in practice called from
     exactly one site (`alloc_core_small_pool.rs:1095`). Rust has no
     sibling-module-only visibility, so this is the tightest expressible
     bound."
   - **P2-5 — the double-release counterfactual test has a cheap runtime
     check its own file's two-options analysis missed.** The review's
     claim: `tests/r31_4_reserved_small_segment_handle.rs` weighs exactly
     two options (`trybuild` vs. prose) for proving a compile-error
     property, but a third exists at zero cost:
     `assert!(core::mem::needs_drop::<ReservedSmallSegment>())` —
     `needs_drop` is callable at runtime, and a type with a `Drop` impl can
     never be `Copy` (a hard rustc rule), so combined with the file's
     existing by-value-signature exercise this is the complete
     compile-error argument, and unlike the prose it would actually FAIL if
     a future refactor removed `Drop` and added `Copy`.
   - **P2-11 — `AllocCore::dbg_large_cache_hits` remains a safe `pub fn` in
     a plain `production` build, unlike its `HeapCore`-level sibling R31-4
     tightened.** The review's claim, verified by its own out-of-tree
     compile probe: `AllocCore::dbg_large_cache_hits` compiles against
     `features = ["production"]` alone (R31-4/item 8 P2-2 above tightened
     only the `HeapCore` delegation, not this one). It is allowlisted in
     `tests/dbg_hook_safety_tripwire.rs`'s `PURE_OBSERVERS`
     (`:213`) and is a zero-argument `&self` counter read with no pointer
     and no mutation, so the review calls it a *sanctioned* exception under
     the tripwire — but notes CLAUDE.md's benchmark-hook rule 2 ("no
     production caller ⇒ MUST default to `bench-internals`") applies to it
     by the identical reasoning R31-4 used against its own sibling, and the
     R31-4 commit does not say why the pair was split. Suggested fix per
     the review: one sentence of justification, or a matching tightening
     to `all(alloc-decommit, bench-internals)`.
   - **P2-12 — the R31-4 retrofit narrowed tripwire coverage of the exact
     hook shape it hardened.** The review's claim: `scan_file`
     (`tests/dbg_hook_safety_tripwire.rs:814`) matches only `pub fn dbg_` /
     `pub unsafe fn dbg_`; the raw-pointer RETURN that used to live on
     `dbg_decomp_reserve_and_keep` (and was therefore scanned) now lives on
     `ReservedSmallSegment::base(&self) -> *mut u8`, a differently-named
     method the scanner's name-prefix match cannot see. The review calls
     this harmless today (`bench-internals`-gated; returns a pointer the
     caller already legitimately holds) but a coverage gap for the scanner
     going forward. Suggested fix per the review: rename to `dbg_base()`,
     or widen the scanner to also enumerate `#[doc(hidden)] pub fn`
     returning `*mut`/`*const` on measurement-only types.
   - **Next trigger:** independent re-verification of each sub-finding
     (re-read the `mod.rs` declarations for P2-4's visibility claim;
     confirm `needs_drop::<ReservedSmallSegment>()` for P2-5; re-run the
     review's out-of-tree compile probe for P2-11; re-read `scan_file`'s
     match logic for P2-12), then either apply the review's suggested fixes
     or record a reasoned dismissal, in a future round. None of these
     threaten correctness per the review's own text.
   - **Evidence:** `docs/reviews/2026-07-31-r31-full-review.md` §7 P2-4,
     P2-5, P2-11, P2-12 (the review's own text is the only source cited
     here — this entry is a filing, not an independent confirmation).

   **[FIXED, R31-14b/task #484, 2026-07-31.]**
   All four claims independently re-verified before fixing, per the "Next
   trigger" instruction above.

   - **P2-4 confirmed and fixed (doc-only).** Re-read `src/alloc_core/mod.rs`
     directly: `reserved_small_segment` is declared `pub mod` as a direct
     child of `alloc_core` (line 99), a SIBLING of `alloc_core_small_pool`
     (declared `mod alloc_core_small_pool` at line 22), not nested inside
     it — confirming `pub(super)` on `new_from_reservation`/`into_base`
     resolves to `pub(in crate::alloc_core)`, reachable from every module
     under `alloc_core`. Confirmed the single real caller via
     `grep -n "new_from_reservation\|into_base"
     src/alloc_core/alloc_core_small_pool.rs` → lines 1095 and 1117 exactly.
     Fixed all three overstated doc-comment locations
     (`reserved_small_segment.rs:23-27`, `:80-85`, `:108-112`) to state
     "reachable from anywhere inside `alloc_core`... Rust has no
     sibling-module-only visibility, so this is the tightest expressible
     bound," with the exact caller line numbers cited, matching the
     review's own suggested wording.
   - **P2-5 confirmed and fixed.** Re-read
     `tests/r31_4_reserved_small_segment_handle.rs` and confirmed it weighed
     exactly two options (trybuild vs. prose), no `needs_drop` check.
     Verified the runtime counterfactual independently: compiled a
     throwaway `struct NoDrop { x: *mut u8 }` (no `Drop` impl) and confirmed
     `core::mem::needs_drop::<NoDrop>()` returns `false` — proving the new
     assertion is non-vacuous (it WOULD fail if `ReservedSmallSegment` lost
     its `Drop` impl), not merely a decoration. Added
     `reserved_small_segment_needs_drop_so_it_cannot_be_copy` (a new `#[test]`
     asserting `core::mem::needs_drop::<ReservedSmallSegment>()`) plus a
     documented "option 3" in the file's module doc explaining the argument
     and citing this review finding.
   - **P2-11 confirmed; decision: keep as a sanctioned exception, add
     justification (not tighten).** Re-verified `AllocCore::dbg_large_cache_hits`
     (`src/alloc_core/alloc_core_large_cache.rs:544`) is gated
     `#[cfg(feature = "alloc-decommit")]` alone — reachable in plain
     `production`. Unlike its `HeapCore` sibling (R31-4/item 8 P2-2 above,
     which had ZERO callers outside `bench-internals`-gated examples before
     tightening), this method has genuine `#[test]` regression callers that
     run in a plain `production` test build without `bench-internals`:
     `tests/alloc_zeroed_fresh_large_skip.rs` and
     `tests/regression_large_cache_span_usable_stable.rs` both gate only on
     `#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]` and
     assert on this method's return value — confirmed by running
     `cargo test --features production --test alloc_zeroed_fresh_large_skip
     --test regression_large_cache_span_usable_stable`, both green.
     Tightening to `bench-internals` would break these two real test files.
     CLAUDE.md's benchmark-hook rule 2 ("no production caller ⇒
     `bench-internals`") does not apply here precisely because a production
     caller (the test binary) DOES exist, which is the deciding difference
     from the `HeapCore` sibling's case. Added a doc-comment paragraph to
     `dbg_large_cache_hits` explaining this asymmetry explicitly, so a
     future reader does not have to re-derive it.
   - **P2-12 confirmed and fixed.** Re-read `tests/dbg_hook_safety_tripwire.rs`'s
     `scan_file` (`:814`, `trimmed.starts_with("pub fn dbg_")`) and confirmed
     it structurally cannot match `pub fn base`. Renamed
     `ReservedSmallSegment::base` → `dbg_base` and updated all call sites
     (`tests/r31_4_reserved_small_segment_handle.rs` ×3,
     `examples/r29_3_decomposition_gate.rs` ×3, confirmed via a repo-wide
     `grep -rn "handle\.base()\|h2\.base()"` returning zero hits post-fix).
     The rename alone surfaced a SECOND, related gap the review did not
     flag: the tripwire scans the attribute block immediately preceding
     each `pub fn dbg_*` line, not the enclosing `impl` block's own `#[cfg]`
     — `dbg_base` was gated only at the `impl ReservedSmallSegment` level,
     so after the rename `cargo test --features "production bench-internals
     alloc-stats" --test dbg_hook_safety_tripwire` genuinely FAILED
     ("NEW unaccounted-for SAFE, non-bench-internals-gated hooks:
     ...::dbg_base") until a redundant per-method
     `#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]`
     was added directly on `dbg_base` — confirming both that the tripwire
     genuinely works end-to-end and that repeating the gate per-item (the
     established pattern elsewhere in this crate, e.g.
     `heap_core_diag.rs`'s methods) is required, not optional decoration.
   - **Verification (all four together):** `cargo build --features
     "production bench-internals alloc-stats" --all-targets` clean;
     `cargo test --features "production bench-internals alloc-stats"` green
     (231 test-binary result lines, 0 failed); `cargo test --features
     production --test alloc_zeroed_fresh_large_skip --test
     regression_large_cache_span_usable_stable --test
     regression_large_cache_multi_size_cycle` green; `cargo clippy
     --features "production bench-internals alloc-stats" --all-targets -- -D
     warnings` clean; `cargo clippy --features production -- -D warnings`
     clean; `cargo clippy --features experimental --all-targets -- -D
     warnings` clean; `cargo clippy --all-features --all-targets -- -D
     warnings` clean; `cargo fmt --check` clean.

---

10. **[T, filed 2026-07-31, UNVERIFIED-BY-ME findings from the Round 32 full
    independent review (`docs/reviews/2026-07-31-r32-full-review.md` §11
    P2-1, P2-6, P2-7, P2-8, P2-11)]** Five P2 findings — NOT independently
    re-verified before filing, per this file's own convention (item 9 above
    is the direct precedent, one round earlier). The round's three P1s
    (P1-1/P1-2/P1-3, all against R31-10) WERE independently re-verified and
    fixed directly in the same session — see the review itself and
    `CHANGELOG.md`'s Round 31 entry for what changed; not filed here.
    - **P2-1 — README's per-file `unsafe` inventory row for
      `src/registry/heap_core_diag.rs` drifted; the tripwire cannot see it.**
      The review's claim: `README.md:594` states 6 hooks for that file; the
      real count is **7** — R31-6 (task #469) added
      `dbg_decomp_recommit_payload` there and correctly bumped the
      AGGREGATE totals (66→68) and the `alloc_core_small_pool.rs` row (2→3),
      but left this file's own row (and its 6-hook prose enumeration)
      untouched. `tests/no_stale_doc_references.rs` asserts only the three
      aggregate tokens, never per-file rows, so this class of drift is
      invisible to CI by construction.
    - **P2-6 — `CHANGELOG.md` covers 1 of Round 32's 11 tasks, and now
      contains a claim `docs/CORRECTNESS_OPEN_ITEMS.md` item 9's own
      resolution has since made stale.** The review's claim: only `38fbe8f`
      (R31-10) touched `CHANGELOG.md`; absent entirely are R31-8's new
      CLAUDE.md rule, three new process tools
      (`verify-gate-report.mjs`/`verify-commit-prefixes.mjs`/
      `tests/ci_clippy_matrix_consistency.rs`), R31-6's correctness fix, and
      all ten fixed review-P2 repairs (R31-14a/b). The existing Round-31
      CHANGELOG bullet still says "the other 11 P2s were filed, not fixed"
      — no longer true for ten of them.
    - **P2-7 — `tests/r31_10_trim_current_thread_api.rs`'s AC1 test asserts
      equality on a process-wide counter across a window its sibling tests
      in the same file can perturb.** The review's claim:
      `SeferAlloc::stats()` is documented process-wide;
      `ac1_trim_empties_pool_and_evicts_large_cache` asserts
      `released_after_cache == released_before` across an alloc+dealloc
      window while libtest runs the file's tests concurrently by default —
      `ac3`'s two threads and `ac4`'s spawned thread(s) can each increment
      `segments_released_total` via their own trims/`AbandonGuard::drop`.
      Low-probability real flake vector, not yet observed. Suggested fix per
      the review: assert a delta computed by the same thread around its own
      trim, or serialise the file's tests.
    - **P2-8 — `ba52822`'s commit subject `fix(examples):` under-declares
      its diff, and the R31-5c lint structurally cannot catch this shape.**
      The review's claim: that commit adds two new `pub unsafe fn` hooks to
      `src/` and edits README's `unsafe` inventory under a subject naming
      only `examples`. `verify-commit-prefixes.mjs`'s direction-2 WARN
      applies only to `bench(...)`/`docs(...)` prefixes; a `fix(...)`
      subject lands in the `'other'` bucket, explicitly out of the lint's
      scope (consistent with R30-12's letter, which governs `perf` commits
      specifically) — but it is the same reader-misleading shape the rule
      exists to prevent for `perf`.
    - **P2-11 — a Round 32 task committed before its own `npm run check`
      finished, and created/removed two scratch commits directly on
      `main`.** The review's claim: `eb6935b` (R31-5c) honestly states in
      its own message that the full test+iai tail of `npm run check` was
      "still completing... at commit time" — a literal deviation from
      CLAUDE.md's "Between phases: run tests and commit" (the tree is
      green now, independently re-confirmed by the review; no harm
      resulted). The same task also created and removed two scratch commits
      (`8eae855`/`3dc528d`) via `git reset --soft` directly on `main`,
      visible only in `git reflog` — nothing was lost and history stayed
      linear, but a shared-workspace round should prefer a scratch branch
      or worktree for that kind of manoeuvre going forward.

    **[P2-6 RESOLVED — 2026-08-02, task #489 ledger housekeeping.]**
    Independently re-verified against `CHANGELOG.md`'s actual Round 31
    section (not just trusted commit `e124a48`'s own message): all of the
    content P2-6 named as missing is now present as CHANGELOG bullets —
    R31-8 (task #472, the same-workload-regime CLAUDE.md rule), R31-5a
    (task #480, `scripts/verify-gate-report.mjs`), R31-5c (task #482,
    `scripts/verify-commit-prefixes.mjs`), R31-5b (task #481, the four
    WARN-level checks + `scripts/capture-measurement-identity.mjs`), R31-11
    (task #475, `tests/ci_clippy_matrix_consistency.rs`), R31-6 (task #469,
    the Windows decommit-crash correctness fix), and both R31-14a/R31-14b
    (tasks #483/#484, the 10 fixed review-P2 repairs) all have their own
    bullets (`CHANGELOG.md` lines 32, 34-40 in the `[Unreleased]` → Round 31
    section as of this check). The stale "the other 11 P2s were filed, not
    fixed" wording P2-6 flagged has its own in-place `**UPDATE (Round 32,
    tasks #483/#484): 10 of these 11 were independently re-verified and
    FIXED**` correction already present in the same bullet (line 31). P2-6
    is RESOLVED; the other four findings in this item (P2-1, P2-7, P2-8,
    P2-11) remain open and unverified — this note closes only P2-6, per
    this file's append-only convention (do not silently drop the other
    four from the bundle).

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

16. **[T, filed 2026-08-04, R34-2/task #521] Cross-thread routing's documented
    residual (caller-contract-violation surface) needs to reach the release
    notes (`docs/reviews/2026-08-04-release-stabilization-audit.md` F-3 [low]).**
    `dealloc_foreign_routing` (`src/registry/heap_core_xthread.rs:858-1007`)
    reads and writes foreign segment memory under a "magic != 0" guard only;
    the code documents honestly (`:864-885`) that a live-foreign vs
    already-released segment is O(1)-indistinguishable, so a double free of a
    released segment is "fundamentally UB … not fixed by this change" — the
    standard caller-contract residual every allocator has. The action item is
    NOT a code fix (none is needed — for a single legitimate cross-thread free,
    `live_count ≥ 1` until the owner's drain reclaims, so the segment cannot be
    released underneath the freer); the action is to **state this residual in
    the release notes** so a downstream reader knows the documented limitation.
    Filed because no Round-34 task owns release-notes writing.

    **Status: RESOLVED (2026-08-05, task #597/K2, commit `f43600d`).** The
    exact action this item requested — a release-notes statement of the
    residual — landed in `CHANGELOG.md`'s new "Known limitations (as of
    this release)" subsection. Left in place rather than moved to "Recently
    resolved" / renumbered (that structural cleanup, spanning several
    pre-existing item-numbering gaps in this file, is task M2/#623's
    broader scope, not duplicated here item-by-item).

17. **[T, filed 2026-08-04, R34-2/task #521] Five tier-1 `unsafe` seams have
    no miri, no loom, and no kani harness — covered by ordinary integration
    tests only (`docs/reviews/2026-08-04-release-stabilization-audit.md` G3
    [medium]).** The five: `global::sefer_alloc` (the `unsafe impl GlobalAlloc`
    itself), `global::fallback` (the `static mut MaybeUninit<HeapCore>` +
    spinlock), `registry::heap_slot` (the single load-bearing `unsafe impl
    Sync` in the crate), `alloc_core::sidecar` (the shared lazily-materialised
    sidecar deref boundary, on the `production` path via
    `alloc-segment-directory` + `class-aware-dirty`), and
    `alloc_core::large_cache_extended`. Additionally, `alloc_core::dirty_by_class`
    has `loom_class_aware_dirty` but per ci.yml's own note that model uses
    hand-rolled `loom::sync` atomics, not the real `PerClassDirty`/`RacyPtrCell`
    types — so the real sidecar deref is unmodelled there too. For a
    stabilization release, adding at least a miri or loom harness to each
    (especially `sidecar`, which is on the `production` path) closes the
    largest remaining verification-coverage gaps.

    **Status: PARTIALLY RESOLVED (2026-08-06, task #606/K11) — 2 of 5 claims
    corrected, 2 real CI-wiring gaps closed, 2 seams remain genuinely
    uncovered (accepted risk, see below).**

    - **`registry::heap_slot`'s claim was already stale when filed.**
      `tests/regression_xthread_thread_free_alias_miri.rs` (its own header
      comment: "`Sync` `HeapSlot`, mirroring W3) is required") already
      exercises the exact `unsafe impl Sync for HeapSlot` this item names,
      under real cross-thread miri, and was already wired into
      `ci.yml`'s `miri-plain` job (line ~973) before this item was even
      filed. No action needed — the original claim was simply wrong.
    - **`alloc_core::sidecar` / `alloc_core::large_cache_extended` — real
      gap, partially closed.** `tests/segment_directory_a5_miri.rs` (R7-A5's
      own miri target) already existed, already passed, and genuinely
      exercises the shared `alloc_core::sidecar::OwnedSidecar` primitive
      (via `os::reserve_directory_sidecar`/`deref_directory_sidecar`, which
      call `sidecar::reserve_zeroed_with`/`sidecar::deref` directly — same
      primitive `large_cache_extended.rs` calls) — but was never wired
      into any CI job, so it never actually ran. Wired into `miri-core`
      as a new step (commit `4dd0624`). Residual gap, explicitly NOT
      closed: this test is BELOW-threshold only (`table.count() < 32`,
      the sidecar never actually materialises) — the test's own header
      comment explains why the full materialised path (reserve, rebuild,
      lookup, set/clear bits, 32+ segments) is impractically slow under
      miri and is instead covered only by NATIVE tests
      (`segment_directory_a1.rs`/`_a2.rs`/`_a3.rs`/`_a5.rs`/`_a5_proptest.rs`).
      The materialised-path `sidecar::reserve`/`deref` calls themselves —
      the actual UB-sensitive boundary — remain unproven under miri.
      Writing a miri-tractable materialised-threshold test (a lower
      test-only threshold, or a direct unit-level `OwnedSidecar` miri test
      that bypasses the 32-segment precondition entirely) is real
      follow-up work, not attempted here.
    - **A second, unrelated CI-wiring gap found and closed in the same
      pass**: `tests/remote_fanin.rs`'s `remote_fanin_miri_minimal_retry_ub_check`
      (a purpose-built minimal miri UB-detection harness for
      `push_with_overflow_retry`'s retry path, per its own doc comment
      "Harness 3: minimal miri UB-detection target") also existed, already
      passed, and was also never wired into any CI job. Wired into
      `miri-core` as its own step (commit `4dd0624`) — kept separate from
      the pre-existing `reclaim_offset_unit` step rather than combined
      with a positional test-name filter, after that combination was tried
      first and found to silently zero `reclaim_offset_unit`'s own test
      out of its run ("0 passed ... 1 filtered out") — the exact
      false-PASS shape `miri-core`'s own header comment already documents
      from a prior incident (a bare positional filter matching nothing
      while still reporting green). Caught before landing, not shipped.
    - **`global::sefer_alloc` (the `unsafe impl GlobalAlloc` boundary
      itself) and `global::fallback` (the `static mut MaybeUninit<HeapCore>`
      plus spinlock) — genuinely zero miri/loom/kani coverage, confirmed by
      direct grep across `src/` and `tests/`, ACCEPTED AS RESIDUAL RISK for
      this release rather than closed.** Both are exercised extensively by
      ORDINARY (non-miri/loom) integration tests (`tests/global_alloc.rs`,
      `tests/global_alloc_mt.rs`, `tests/global_alloc_installed.rs`, and
      indirectly by the whole test suite, since `SeferAlloc` is the
      `#[global_allocator]` under `--features production`) — functional or
      logic bugs in these paths would be caught. What miri/loom
      specifically add beyond that — Stacked/Tree Borrows aliasing
      violations, data races invisible without a memory model, the exact
      class of bug `heap_slot`'s own dedicated test above was written to
      catch for a DIFFERENT boundary — remain unproven here. Rationale for
      accepting this rather than blocking release: (a) `global::sefer_alloc`'s
      own trait impl is a thin TLS-lookup-and-dispatch wrapper (the heavy
      unsafe logic it delegates to — `HeapCore::alloc`/`dealloc` — already
      has substantial miri coverage via `reclaim_offset_unit`,
      `decommit_miri_cycle`, and now
      `remote_fanin_miri_minimal_retry_ub_check` above); (b)
      `global::fallback`'s pre-TLS/post-teardown windows are, by the
      module's own doc comment, rare and effectively single-threaded in
      practice, narrowing the real-world UB surface relative to the hot
      per-thread path. Writing dedicated miri/loom harnesses for both
      remains real, valuable follow-up work, not attempted here — this
      status update is the explicit "record the accepted residual risk"
      resolution K11's own filing offered as an alternative to full
      harness-writing.

18. **[T, filed 2026-08-04, R34-2/task #521] kani proves only the smallest
    seam and a deprecated tier — two highest-value CBMC-reachable properties
    are unproven (`docs/reviews/2026-08-04-release-stabilization-audit.md` G4
    [low]).** `src/kani_proofs.rs` covers `alloc_core::node` primitives and
    `concurrent::hand` (the research tier). The two unproven high-value
    properties are: (a) the ring's wrap arithmetic — that
    `t.wrapping_sub(h) < RING_CAP` is an invariant of the push/drain pair
    across the `u32::MAX → 0` boundary; and (b) `pack_entry`/`unpack_entry`
    (both hardened and non-hardened packings) round-trip and never produce
    `RING_SLOT_EMPTY` over the full real input ranges. Both are pure
    arithmetic with no pointers — ideal kani targets — and both are currently
    protected only by unit tests plus `const _: () = assert!` on the *bounds*,
    not on the *round trip*.

    **Status: RESOLVED (2026-08-06, task #611/K16, commit `772b36d`).** Both
    (a) and (b) now have real, verified Kani proofs in `src/kani_proofs.rs`:
    `ring_wrap_proofs` (2 harnesses, generalising
    `tests/regression_ring_cursor_wrap.rs`'s hand-picked wrap-boundary values
    into an exhaustive proof over every `u32` head and every occupancy
    `0..=RING_CAP`) and `ring_entry_pack_proofs` (4 harnesses: round-trip +
    `RING_SLOT_EMPTY`-never-collides, for both the non-hardened and
    `hardened`-only packings). All 6 verified via a real `cargo kani` run
    (kani-verifier 0.67.0 under WSL2 — Kani does not support Windows at all,
    confirmed: `kani-verifier` fails to even compile under
    `x86_64-pc-windows-msvc`) and one counterfactually confirmed non-vacuous
    (a deliberately injected off-by-one bug was caught as `FAILURE`, then
    reverted and reverified `SUCCESS`).

    **Also discovered and fixed in the same task**: Kani had NEVER been
    wired into any CI job before this — the 13 pre-existing proof harnesses
    in `src/kani_proofs.rs` (`node_proofs`, `hand_proofs`, `pack_proofs`)
    were never continuously re-verified either, only run by hand at
    authoring time. Added a new `kani` CI job running all 19 harnesses
    (13 pre-existing + 6 new) per-PR — measured at ~30s total, comparable to
    this workflow's existing miri jobs.
