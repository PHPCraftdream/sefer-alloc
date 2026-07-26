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

1. **Flaky test — `canary_survives_promotion_and_free_leaves_no_leak`**
   (`tests/r14_4_promotion_free_correctness.rs`).

   - **First observed:** R19-1 (task #337, commit `46ea2db`), "failed 1 of 3
     runs" on pristine pre-fix code (confirmed unrelated to that commit's
     hardened-Large-dealloc fix).
   - **Independently reproduced twice more in Round 22:**
     - R22-1 (task #352, commit `00fb53c`): the delegated agent's full-suite
       run under `--features "hardened medium-classes"` reproduced it firing
       "at its documented ~1-in-3 rate" (per that commit's message, "Out of
       scope, tracked separately" paragraph).
     - R22-5 / task #356's test run (see TaskList context for this task):
       reproduced again at "roughly the same rate" per this task's own
       prompt.
   - **Exact failing assertion** (the test has two `assert!` sites; the one
     implicated by "failed 1 of 3 runs" is the leak-bound check — see the
     test file's lines 131–135):
     ```text
     assert!(
         released_delta <= reserved_delta,
         "released_delta ({released_delta}) must not exceed reserved_delta \
          ({reserved_delta}) — a double-release would indicate corruption"
     );
     ```
     (There is a second, weaker monotonicity assertion earlier in the same
     test — `segments_reserved_total` must not go backwards — but the
     "released_delta <= reserved_delta" assertion is the one shaped like the
     failure this item's reproductions describe.)
   - **Plausible root-cause category:** the test computes `reserved_delta`
     and `released_delta` as snapshots of PROCESS-WIDE counters
     (`a.stats()` reads `segments_reserved_total`/`segments_released_total`,
     which are not scoped to this single `SeferAlloc` instance's activity —
     see the test's own comment at lines 110–117 acknowledging the delta is
     "since `stats_before`", i.e. it assumes no concurrent activity touches
     the same counters between the two snapshots). `cargo test` runs test
     binaries with multiple test-thread parallelism by default; if any
     OTHER `#[test]` in the same binary (or a background reclaim/decommit
     thread the allocator itself spins up) reserves or releases a segment
     on the shared global counters between this test's `stats_before` and
     `stats_after_free` reads, the delta comparison is not actually
     isolated to this test's own grow+free round-trip, despite the comment
     asserting it is. This would manifest as exactly the observed
     "flaky, ~1-in-3, otherwise reproducible" signature — a genuine race in
     test isolation (shared global allocator state read concurrently by
     other test threads), not a bug in the allocator's own free/promotion
     path. **Next step:** re-run this specific test with
     `cargo test ... -- --test-threads=1` for the affected binary across
     enough iterations to see whether the flake rate drops to zero; if it
     does, the fix is either (a) make the test single-threaded-safe (run it
     in isolation, e.g. via `#[test]` + a process-level mutex already used
     elsewhere in this suite for stats-sensitive tests, if such a pattern
     exists), or (b) switch the assertion to a per-allocation-tracked delta
     that does not depend on global-counter isolation at all.
   - **Status:** open, unowned. Not yet fixed — out of scope for this
     tracking task (task #354/R22-3); tracked here so it cannot go another
     three rounds unnoticed the way the perf-only analog did before
     `docs/perf/OPEN_ITEMS.md` existed.

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

---

## Recently resolved (closure trail — do not re-list as open)

_(none yet — this file was created in R22-3, task #354.)_
