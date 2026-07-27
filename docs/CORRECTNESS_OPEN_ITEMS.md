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

1. **Round start:** before forming a new round's task queue, read this file
   end-to-end (alongside `docs/perf/OPEN_ITEMS.md`) and decide, for each open
   item, whether this round closes it, defers it (with a one-line reason
   appended), or leaves it. An item must not be silently ignored — every
   round either moves it or explicitly re-defers it.
2. **When you close an item:** move its entry to §"Recently resolved" with
   the closing round + task number + one-line evidence (commit / doc that
   records the resolution). Do NOT delete the entry — the closure trail is
   itself the artifact that lets a future reviewer confirm an item was
   actually addressed, not just forgotten again.
3. **When a new commit, comment, or review flags a correctness/CI-debt
   follow-up:** add it here in the same commit (or an immediate follow-up
   commit), with a citation back to its origin (commit SHA / file:line). A
   flag that lives only inside a single commit message body or code comment
   is exactly the failure mode this index exists to prevent.

**Tier key.** **[A]** active — a real next step a round should consider
taking. **[T]** tracked-not-actioned — genuinely reproduced/confirmed but
intentionally not yet scheduled for a fix (root-cause investigation or a
scoping decision is the pending step, not implementation).

---

## Open items

### [T] Tracked, not yet actioned

_(item 1, the `canary_survives_promotion_and_free_leaves_no_leak` flaky test,
was resolved by an urgent CI-fix task — see "Recently resolved" below.)_

2. **Clippy dead-code — `--features "hardened medium-classes"` is not
   clippy-clean.**

   - **Exact combination:** `cargo clippy --all-targets --features "hardened
     medium-classes" -- -D warnings`.
   - **First observed:** R19-1 (task #337, commit `46ea2db`), "11 dead-code
     errors confirmed present identically on pristine code — that combo is
     simply outside today's CI feature matrix, also a follow-up."
   - **CI status as of this round:** R22-1 (task #352, commit `00fb53c`)
     added a `cargo test --features "hardened medium-classes"` row to
     `ci.yml`'s hardened-tier job — so this feature combo IS now exercised
     by `cargo test` in CI — but deliberately did NOT add a `clippy -D
     warnings` row for it (explicitly out of scope for that task; see its
     commit message: "Out of scope, tracked separately: the 11 dead-code
     clippy warnings under 'hardened medium-classes' (pre-existing, task
     #354)"). R22-5 (task #356, commit `de5e0dc`) independently re-ran the
     same clippy invocation as part of its own zero-trust verification and
     confirmed "exactly 11 pre-existing dead-code errors (the same baseline
     R19-1's commit message already documented), none from any file this
     change touches" — i.e. the count has been stable at 11 across at least
     two independent re-runs three rounds apart.
   - **The actual 11 dead-code lint errors** (from a fresh run performed for
     this task, 2026-07-26, on `main` @ `91510ce`):
     1. `src/alloc_core/alloc_core.rs:54` — unused import `SMALL_CLASS_COUNT`
        (`use super::size_classes::{AllocKind, SizeClasses, SMALL_CLASS_COUNT};`)
     2. `src/alloc_core/alloc_core_large.rs:448` — variable does not need to
        be mutable: `let mut seg = Segment::reserve(usable);`
     3. `src/alloc_core/alloc_core_small.rs:893` — unused variable
        `small_cur`: `let small_cur = self.small_cur;`
     4. `src/alloc_core/alloc_core_small.rs:1941` — variable does not need to
        be mutable: `let mut seg = Segment::reserve(SEGMENT);`
     5. `src/alloc_core/alloc_core_small_reclaim.rs:506` — unused variable
        `small_cur`: `let small_cur = self.small_cur;`
     6. `src/alloc_core/alloc_core.rs:2115` — method `small_cur` is never
        used: `pub(crate) fn small_cur(&self) -> *mut u8`
     7. `src/alloc_core/sidecar.rs:230` — function `reserve_zeroed_with` is
        never used: `pub(crate) unsafe fn reserve_zeroed_with<T>(fixup:
        impl FnOnce(*mut T)) -> Option<*mut T>`
     8. `src/alloc_core/sidecar.rs:275` — function `deref` is never used:
        `pub(crate) unsafe fn deref<T>(p: *const T) -> &'static T`
     9. `src/alloc_core/sidecar.rs:303` — function `deref_mut` is never
        used: `pub(crate) unsafe fn deref_mut<T>(p: *mut T) -> &'static mut T`
     10. `src/registry/heap_core_xthread.rs:586` — constant
         `EMPTIED_BASES_CAP` is never used: `const EMPTIED_BASES_CAP: usize
         = 64;`
     11. `src/registry/heap_registry.rs:523` — struct `ConflictRollback` is
         never constructed: `struct ConflictRollback { ... }`

     (Compiler summary: "could not compile `sefer-alloc` (lib test) due to
     11 previous errors" / "could not compile `sefer-alloc` (lib) due to 11
     previous errors" — both counts agree at 11.)
   - **Plausible root-cause category:** these read as ordinary
     feature-combination dead-code fallout, not a real bug — items 1–5 look
     like leftover `small_cur`/mutability artifacts from a refactor that
     changed under a DIFFERENT feature combination than `hardened
     medium-classes`, leaving stale code paths only this specific
     intersection still compiles; items 7–9 (`sidecar.rs`'s
     `reserve_zeroed_with`/`deref`/`deref_mut`) and 10–11
     (`EMPTIED_BASES_CAP`, `ConflictRollback`) look like helpers written for
     a code path that is itself feature-gated differently than `hardened
     medium-classes` gates it, so under this exact combo the caller is
     compiled out but the helper is not. **Next step:** for each item,
     check whether the caller/consumer is gated behind a DIFFERENT feature
     predicate than the item itself (a `#[cfg(...)]` mismatch) — if so, the
     fix is aligning the two predicates (add `medium-classes`/`hardened` to
     the item's own gate, or widen the caller's), not deleting the item
     outright, since some of these may be genuinely used under other
     feature combinations already in CI.
   - **Status:** open, unowned. The `cargo test` gap for this combo is now
     closed (R22-1); the `clippy -D warnings` gap is not. Fixing is out of
     scope for this tracking task (task #354/R22-3).

3. **Two flaky coarse-wall-clock tests surfaced by `npm run check`'s
   `--all-features` step, discovered post-Round-22 while investigating a
   real (now-fixed) test failure.**

   - `tests/regression_segment_table_tombstone_rebuild.rs::backshift_no_latency_spike_at_threshold_boundary`
     — failed twice across two independent `npm run check` runs (2026-07-26,
     post-R22-18) with "slowest dealloc (N ns) is 42.2x the median" (an
     `O(HASH_CAPACITY)` per-delete regression signal); passed cleanly 3/3
     times when re-run in isolation immediately after each failure. The
     test's own panic message already self-documents the risk: "(Coarse
     wall-clock; confirm with `npm run iai`.)"
   - `tests/dealloc_sublinear.rs::own_thread_free_is_subquadratic` — failed
     once across the same investigation window; passed cleanly when re-run
     in isolation. Asserts wall-clock free-time scaling is sub-quadratic —
     a timing assertion with no feature gate, sensitive to host CPU
     contention under `npm run check`'s parallel-test-binary load.
   - **Plausible root-cause category:** both are coarse wall-clock latency
     assertions (not `alloc-stats`-counter-based like item 1 above) that
     compare a measured operation's time against a computed multiple of the
     median — inherently sensitive to scheduler/CPU contention when many
     test binaries run in parallel (as `npm run check`'s `--all-features`
     step does), not a correctness regression in the code under test. Both
     tests already carry their own "this is coarse, verify with iai if in
     doubt" disclaimer in their assertion messages, suggesting the authors
     were aware of this risk when writing them.
   - **Next step:** either (a) serialize these two tests against the rest of
     the suite (a `TEST_LOCK`-style process-wide mutex, matching the pattern
     already used elsewhere in this suite for stats-sensitive tests), or (b)
     widen their tolerance multiplier, or (c) accept them as known-flaky
     wall-clock canaries and exclude them from `npm run check`'s pass/fail
     gate specifically (deterministic `Ir`-based judges remain the real
     regression gate; these two are best-effort human-readable signals).
     Not decided here — tracked so it does not go unnoticed the way the
     analogous perf-only gap did before `OPEN_ITEMS.md` existed.
   - **Status:** open, unowned. Not fixed — out of scope for the task that
     found it (a fix for `tests/r21_2_opt_h_stage1_precondition_probe.rs`'s
     own unrelated `--all-features` geometry bug).

4. **`canary_survives_promotion_and_free_leaves_no_leak`'s leak-bound
   assertion proves no double-release, not no leak.**

   - **First observed:** independent read-only review of `bc4aacf`
     (`docs/reviews/2026-07-27-post-r22-followups-readonly-review.md`),
     surfaced while verifying `bc4aacf`'s test-isolation-race fix (see
     "Recently resolved" below for that fix itself).
   - **The gap:** `tests/r14_4_promotion_free_correctness.rs`'s assertion
     `released_delta <= reserved_delta` (line ~157) only proves the
     released count never exceeds the reserved count — a
     double-release/corruption guard. It does NOT prove no leak: if a grow
     reserves a segment and never releases it, `reserved_delta=1,
     released_delta=0` satisfies `0 <= 1` trivially, so a genuinely leaked
     (never-released) segment would pass this assertion silently.
   - **Status:** pre-existing (predates `bc4aacf`, which correctly left it
     untouched — that commit's scope was the test-isolation race only, not
     the assertion's own semantics). Not yet scheduled for a fix.
   - **Possible future strengthening (not decided/scheduled here):** a
     per-heap/per-allocation observable (e.g. asserting the specific freed
     promoted base is no longer registered / reachable, or is in an
     expected bounded decommit/cache state) rather than a process-global
     reserved/released delta, which by construction cannot distinguish
     "still held by something else" from "genuinely never released."

---

## Recently resolved (closure trail — do not re-list as open)

1. **Flaky test — `canary_survives_promotion_and_free_leaves_no_leak`**
   (`tests/r14_4_promotion_free_correctness.rs`) — **RESOLVED** by an urgent
   CI-fix task (2026-07-26), responding to `origin/main` CI run `30217256247`
   / job `89833506941` failing on the `test (--features "hardened
   medium-classes")` step with `error: 1 target failed: --test
   r14_4_promotion_free_correctness`.

   - **Root cause, confirmed:** `SEGMENTS_RESERVED_TOTAL`/
     `SEGMENTS_RELEASED_TOTAL` (`src/alloc_core/os.rs:52,57`) are
     process-wide `static AtomicU64`s. Both `#[test]` functions in this file
     (`canary_survives_promotion_and_free_leaves_no_leak` and
     `repeated_promote_and_free_does_not_leak_unboundedly`) read `a.stats()`
     — which loads these same global atomics — take a before/after
     snapshot, and assert a leak-free delta. `cargo test` runs test
     functions concurrently on multiple OS threads within one process by
     default; the two tests in this file (or any other test in the same
     binary) could reserve/release a segment on the shared counters between
     one test's own snapshots, polluting its delta with unrelated activity
     — exactly the historically observed "failed 1 of 3 runs" signature.
   - **Fix:** added a file-scoped `static TEST_LOCK: Mutex<()>` + `serial()`
     helper (the SAME established pattern already used in
     `tests/directory_authoritative_miss.rs`, `tests/alloc_zeroed_fresh_large_skip.rs`,
     `tests/r13_3_magazine_virgin_hit_skips_zero.rs`,
     `tests/r21_2_opt_h_stage1_precondition_probe.rs` for tests that read
     process-wide stats/diagnostic counters), and bound `let _guard =
     serial();` at the top of BOTH test functions in the file (both read
     the same global counters, so both needed serialization, not just the
     one named in the CI failure). No assertion logic was changed — the
     `released_delta <= reserved_delta` leak-bound check is untouched.
   - **Verification:** 4 full runs of the exact CI command (`cargo test
     --features "hardened medium-classes" --no-fail-fast`, matching R22-1's
     CI row exactly — 223 test binaries each run) — all clean, 0 failures.
     Additionally ~190 direct repeated invocations of the specific compiled
     test binary (`--test-threads=4/8/16`, mimicking CI-like concurrency)
     plus several `cargo test --test r14_4_promotion_free_correctness`
     invocations — 0 failures out of roughly 200+ total runs, against the
     historical ~1-in-3 failure rate. `cargo fmt --check` clean on the
     changed file.
   - **Files changed (test/implementation only):**
     `tests/r14_4_promotion_free_correctness.rs`; this index entry itself
     is the second file touched in the same commit (`bc4aacf`).
   - **Scope of what this fix actually proves:** this fix resolved the
     test-ISOLATION RACE only — it did not touch, and did not strengthen,
     the test's own leak-bound assertion (`released_delta <=
     reserved_delta`). That assertion is a DOUBLE-RELEASE guard (released
     count never exceeding reserved count), not a proof of no leak: if a
     grow reserved a segment and never released it, `reserved_delta=1,
     released_delta=0` satisfies `0 <= 1` trivially, so a genuine
     never-released segment would not be caught by this test. This
     semantic gap pre-dates `bc4aacf` and was correctly left untouched by
     it — fixing the test-isolation race was that commit's actual, correct
     scope. See open item 4 above ("Open items" §`[T]`) for a tracked
     follow-up on strengthening leak detection itself.
