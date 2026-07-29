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

_(item 2, the 11 `--features "hardened medium-classes"` clippy dead-code
errors, was resolved by R23-5 (task #374) — see "Recently resolved" below.)_

_(item 3, the two flaky coarse-wall-clock tests, was resolved by R23-6
(task #375) — see "Recently resolved" below.)_

_(item 4, `canary_survives_promotion_and_free_leaves_no_leak`'s leak-bound
assertion proving no double-release but not no leak, was resolved by R28-2
(task #431) — see "Recently resolved" below.)_

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

2. **Clippy dead-code — `--features "hardened medium-classes"` was not
   clippy-clean (11 errors)** — **RESOLVED** by R23-5 (task #374). All 11
   were genuine `#[cfg(...)]` predicate mismatches (an item gated one way,
   its only consumer gated a DIFFERENT way, so under the specific
   intersection `hardened medium-classes` the consumer compiled out but the
   item did not) — confirmed exhaustively per item via `grep` across
   `src/`, `tests/`, `benches/`, `crates/` before touching anything; NONE
   were genuine orphans, so nothing was deleted.

   - **Items 1, 2, 4 — independent single-item mismatches:**
     - `src/alloc_core/alloc_core.rs:54` (unused import `SMALL_CLASS_COUNT`):
       both of the import's only two usages
       (`alloc_core.rs:711`/`directory_miss_streak` field,
       `alloc_core.rs:978`/its initializer) are
       `#[cfg(feature = "alloc-segment-directory")]`-gated, but the `use`
       itself was not. Fix: split the import so `SMALL_CLASS_COUNT` gets its
       own `#[cfg(feature = "alloc-segment-directory")]` line, matching its
       usages; `AllocKind`/`SizeClasses` (used unconditionally elsewhere)
       stay ungated.
     - `src/alloc_core/alloc_core_large.rs:448` and
       `src/alloc_core/alloc_core_small.rs:1941` (`let mut seg = ...` "does
       not need to be mutable"): both `seg` bindings are reassigned ONLY
       inside a `#[cfg(feature = "alloc-decommit")]` pool-drain-and-retry
       block a few lines below; with `alloc-decommit` off (as under
       `hardened medium-classes`) the binding is genuinely never mutated.
       Fix: `#[allow(unused_mut)]` on each binding, following the identical
       established pattern already at
       `src/registry/heap_core_ownership.rs:167` for the same
       feature-conditional-mutation shape.
   - **Items 3, 5, 6 — one unified root cause (`small_cur`), as suspected in
     the task brief:** `AllocCore::small_cur()` (`alloc_core.rs`, was gated
     `#[cfg(feature = "alloc-xthread")]`) has exactly one caller in the
     entire crate — `heap_core_xthread.rs::drain_heap_overflow`, which reads
     it ONLY inside its own `#[cfg(feature = "alloc-decommit")]` block
     (feeding `dec_live_and_maybe_decommit`, which itself requires that
     feature). `alloc-xthread` without `alloc-decommit` (exactly `hardened
     medium-classes`: `hardened = ["fastbin"]` →
     `["alloc-global","alloc-xthread"]`, neither of which pulls in
     `alloc-decommit`) left the method callable-but-uncalled. The two local
     `let small_cur = self.small_cur;` bindings
     (`alloc_core_small.rs:893`, `alloc_core_small_reclaim.rs:506`) are the
     SAME pattern one level down: each is read only inside its own sibling
     `#[cfg(feature = "alloc-decommit")]` block a few lines later. Fix:
     tightened `small_cur()`'s gate to
     `#[cfg(all(feature = "alloc-xthread", feature = "alloc-decommit"))]`
     (its true minimal predicate, matching its one caller), and gated both
     local bindings `#[cfg(feature = "alloc-decommit")]` directly (matching
     their one reader each). Verified two OTHER `let small_cur = ...`
     bindings at `alloc_core_small.rs:1132` and `:2545` were NOT in the
     11-error list and left untouched — clippy did not flag them (their
     enclosing functions/blocks have their own gating that made them a
     non-issue under this combo), confirming the fix was scoped to exactly
     the 3 flagged sites, not a mechanical crate-wide rename.
   - **Items 7-9 — one unified root cause (`sidecar.rs`), as suspected in
     the task brief:** `reserve_zeroed_with` has exactly one caller,
     `os.rs::reserve_directory_sidecar`, gated
     `#[cfg(feature = "alloc-segment-directory")]`. `deref`/`deref_mut` each
     have TWO independent consumer groups — `alloc_core_small.rs`'s
     `directory`/`directory_mut`/`maybe_materialize_directory` +
     `alloc_core_core_diag.rs`'s `dbg_rebuild_directory` (all inside
     `#[cfg(feature = "alloc-segment-directory")]`), and
     `large_cache_extended.rs`'s `deref_large_cache_extension[_mut]`
     forwarders (the whole module gated
     `#[cfg(feature = "large-cache-extended")]`) — either feature alone
     keeps them used. Under `hardened medium-classes`, `alloc-segment-directory`
     is off AND `large-cache-extended = ["alloc-decommit"]` is transitively
     off too (via `alloc-decommit`), so all three functions had zero live
     callers. Fix: followed the EXISTING convention already used one
     function above in the same file (`reserve`'s
     `#[cfg_attr(not(feature = "large-cache-extended"), allow(dead_code))]`,
     predating this task) rather than a hard `#[cfg]` on the function
     itself (keeps these generic `pub(crate) fn`s type-checking under
     `cargo-hack`-style per-feature builds) —
     `#[cfg_attr(not(feature = "alloc-segment-directory"), allow(dead_code))]`
     on `reserve_zeroed_with`, and
     `#[cfg_attr(not(any(feature = "alloc-segment-directory", feature = "large-cache-extended")), allow(dead_code))]`
     on `deref`/`deref_mut` (the `any(...)` reflecting their two independent
     consumer groups, neither of which alone is necessary).
   - **Items 10-11 — two independent single-item mismatches, as suspected:**
     - `src/registry/heap_core_xthread.rs:586`
       (`const EMPTIED_BASES_CAP: usize = 64;`, itself ungated): every
       actual usage (the `emptied_bases`/`emptied_count` declarations and
       both `if emptied_count < EMPTIED_BASES_CAP` comparisons) is already
       `#[cfg(feature = "alloc-decommit")]`-gated; only the constant
       declaration itself lacked the gate. Fix: added
       `#[cfg(feature = "alloc-decommit")]` to the `const` line, matching
       its usages.
     - `src/registry/heap_registry.rs:523` (`struct ConflictRollback`, and
       its `impl Drop`): constructed exactly once, inside
       `claim_with_config`'s config-mismatch branch — and
       `claim_with_config` itself is `#[cfg(feature = "alloc-decommit")]`-gated
       ("Only present under `alloc-decommit`", per its own doc comment).
       Fix: added `#[cfg(feature = "alloc-decommit")]` to both the struct
       and its `impl Drop`.
   - **One additional latent issue found and fixed in the same task (not
     among the original 11, but the same predicate-mismatch class, and
     newly exposed by fixing the 11 above — the lib now compiles under this
     combo, so `--all-targets` reaches this test target for the first
     time):** `tests/regression_batch_flush.rs`'s `DECOMMIT_COUNTER_SERIAL`/
     `SerialGuard` (a `TEST_LOCK`-style serialization guard) and its
     `use std::sync::atomic::{AtomicBool, Ordering}` import were declared
     unconditionally, but every actual use is inside
     `#[cfg(feature = "alloc-decommit")]`-gated test functions. Fixed the
     same way: gated the static/struct/impls/import on
     `#[cfg(feature = "alloc-decommit")]`.
   - **No deletions.** Every one of the 11 (plus the 1 latent test-file
     issue) was confirmed genuinely used under some other feature
     combination already in this project's CI matrix before any fix was
     applied — verified by `grep`ing every call site across the whole repo
     (not just under `hardened medium-classes`).
   - **Verification:**
     `cargo clippy --all-targets --features "hardened medium-classes" -- -D warnings`
     — 0 errors, 0 warnings (down from the stable 11). No new warning
     surfaced as a side effect of any individual fix (re-ran the full
     command after each fix). `cargo test` green across all of: `""`
     (default), `production`, `--all-features`, `hardened medium-classes`,
     `production alloc-stats`, `pinning` (the full
     `scripts/check-all.mjs` test-step feature matrix) — 0 failures in
     every combination. `cargo fmt --all -- --check` clean.
   - **CI:** added a 4th step to the `clippy` job in `.github/workflows/ci.yml`
     (`clippy (--features "hardened medium-classes")`, alongside the
     existing `clippy ()` / `clippy (--features experimental)` /
     `clippy (--all-features)` steps in that same job) so this combination's
     `-D warnings` gate now runs per-PR, not just `cargo test` (closed
     R22-1's deliberately-left-open gap).
   - **Files changed:** `src/alloc_core/alloc_core.rs`,
     `src/alloc_core/alloc_core_large.rs`, `src/alloc_core/alloc_core_small.rs`,
     `src/alloc_core/alloc_core_small_reclaim.rs`, `src/alloc_core/sidecar.rs`,
     `src/registry/heap_core_xthread.rs`, `src/registry/heap_registry.rs`,
     `tests/regression_batch_flush.rs`, `.github/workflows/ci.yml`, and this
     index.

3. **Two flaky coarse-wall-clock tests surfaced by `npm run check`'s
   `--all-features` step** — **RESOLVED** by R23-6 (task #375). One
   independent read-only review first corrected the originally-proposed fix
   (a `TEST_LOCK`-style mutex): a mutex only serializes test FUNCTIONS
   within ONE test binary/process, but the actual flakiness source is CPU
   contention from MULTIPLE test binaries (separate OS processes) running
   concurrently under `npm run check`'s `--all-features` step, plus the CI
   runner's own background load — a mutex inside one binary cannot
   serialize against a different process. That correction was confirmed
   independently before this task began and is reflected in the fix below
   (no `TEST_LOCK` was added to either file).

   - **`tests/regression_segment_table_tombstone_rebuild.rs::backshift_no_latency_spike_at_threshold_boundary`
     — got a deterministic replacement.** The test's (b) claim ("no single
     `unregister`/`recycle` does `O(HASH_CAPACITY)` work") maps exactly onto
     `SegmentTable::hash_remove`'s backward-shift scan-step count (the
     `j = (j+1) & mask` walk across both its find-the-slot and
     shift-the-cluster phases). Added `HASH_REMOVE_MAX_SCAN_STEPS`
     (`src/alloc_core/segment_table.rs`) — a process-wide high-water-mark
     `AtomicU64`, `alloc-stats`-gated increment (same convention as
     `OPT_H_ATTEMPTS`/`HARDENED_LARGE_NOOP_COUNT`), reset hook
     `reset_hash_remove_max_scan_steps`, and `AllocCore` accessors
     `dbg_hash_remove_max_scan_steps`/`dbg_reset_hash_remove_max_scan_steps`
     (`src/alloc_core/alloc_core_core_diag.rs`) — deliberately a MAX not a
     sum, matching the original test's own "no single call is an outlier"
     framing rather than conflating many small deletes with one large one.
     New test `backshift_max_scan_steps_bounded_at_threshold_boundary`
     (`#[cfg(feature = "alloc-stats")]`, same file) drives the identical
     `W = 600`-distinct-bases wave-then-drain shape as the original and
     asserts the high-water mark stays `<= 4 * W` (`HASH_CAPACITY = 8192`
     would be ~13.6x that bound) — a deterministic per-run assertion, zero
     timing, zero retries. (R24-1/task #379 wording-precision note: the
     MEASUREMENT is exact per-run, but the `4 * W = 2400` threshold is a
     regression bound calibrated to this wave's `W = 600`, not a proven
     O(cluster) worst-case for arbitrary configurations — it reliably catches
     a full O(`HASH_CAPACITY`) regression but could miss a smaller
     pathological cluster under 2400 steps.) The original wall-clock test is
     KEPT (not deleted, for manual/`--ignored` diagnostic value) but marked
     `#[ignore = "..."]`
     with a message pointing at the deterministic replacement and
     `npm run iai`.
   - **`tests/dealloc_sublinear.rs::own_thread_free_is_subquadratic` —
     no clean deterministic replacement exists; demoted to non-blocking.**
     Investigated seriously (per this task's explicit instruction not to
     default to demotion): the guard this test protects
     (`AllocCore::dealloc_small`'s M2 double-free check,
     `src/alloc_core/alloc_core_small.rs`) is, by design, an UNCONDITIONAL
     O(1) `AllocBitmap::is_free` bit test with NO loop — Phase 13.4a already
     replaced the O(free-list-length) `free_list_contains` walk this test
     guards against with exactly that O(1) bitmap test. A call-count counter
     ("how many times was the guard tested") would read identically (= N
     after N frees) under BOTH the correct O(1) implementation and the
     regressed O(N²) walk it guards against — the walk's CALL COUNT never
     changed across that regression, only its internal LENGTH did, and there
     is no length-dependent loop left in production code to instrument. The
     only way to get a counter would be adding one to code that would first
     need to reintroduce the very walk being guarded against — not a
     diagnostic-only addition. Per this task's constraint ("if a new counter
     requires touching a genuinely hot path... stop and explain the
     tradeoff"), this test is `#[ignore]`d instead, with a message pointing
     at manual `--ignored` runs and `npm run iai` /
     `benches/perf_gate_iai.rs`'s `small_churn_16b`-family arms as the
     deterministic Ir-based judges for this same free-path cost.
   - **Mechanism confirmed:** `scripts/check-all.mjs` runs `cargo test
     --features <combo>` for each of its feature-matrix entries and fails
     the whole gate on ANY test failure (including any `#[ignore]`d-off
     test simply not running) — `#[ignore]` is exactly the mechanism
     `cargo test` (and therefore this repo's CI/`check-all.mjs`) already
     uses to exclude a test from the blocking pass/fail gate while keeping
     it runnable via `cargo test -- --ignored`, so no `check-all.mjs`/CI
     workflow change was needed.
   - **Non-vacuity — mutation counterfactual (the new deterministic test):**
     temporarily forced `hash_remove`'s phase-1 find loop to burn
     `HASH_CAPACITY - 1` extra counter increments before matching (simulating
     the pre-N3 O(HASH_CAPACITY) tombstone-scan regression class directly,
     without touching pointer/unsafe logic) —
     `backshift_max_scan_steps_bounded_at_threshold_boundary` FAILED
     immediately (`max_steps = 8191` against the `2400` bound, with a
     message correctly naming the O(HASH_CAPACITY) regression class);
     reverted, and the test passed again. Confirms the new test is
     non-vacuous — it fails without the property it's checking for holding.
     No counterfactual was performed for `own_thread_free_is_subquadratic`
     (it was demoted, not replaced) — its own pre-existing counterfactual
     documentation (module doc comment, "author-verified" restoring the old
     `free_list_contains` walk trips the assertion) is unchanged and still
     applies to manual/`--ignored` runs.
   - **Verification:** `cargo test --features production` (223 binaries),
     `cargo test --features "production alloc-stats"` (223 binaries, exit
     0 — this is the combo that actually compiles and runs the new
     deterministic test), and `cargo test --all-features` all green, 0
     failures. Both previously-flaky tests confirmed `... ignored, <reason>`
     under every combo they're compiled under; the new deterministic test
     confirmed `... ok` under `production alloc-stats` and `--all-features`,
     and confirmed ABSENT (not vacuously passing) under plain `production`
     (no `alloc-stats`). `cargo clippy --all-targets -- -D warnings` clean
     across all three CI feature-matrix entries (`""`, `experimental`,
     `--all-features`). `cargo fmt --all -- --check` clean on all touched
     files.
   - **Files changed:** `src/alloc_core/segment_table.rs`,
     `src/alloc_core/alloc_core_core_diag.rs`,
     `tests/regression_segment_table_tombstone_rebuild.rs`,
     `tests/dealloc_sublinear.rs`, and this index.

4. **`dealloc_batch_small` doc comment claimed the LAST `TCACHE_CAP` freed
   blocks stay magazine-warm; the implementation keeps the FIRST.** —
   **RESOLVED** by R24-7 (task #385), a doc-only policy decision (no `src/`
   behavior change, no numbers measured).

   - **First observed:** independent read-only review of Round 23
     (`docs/reviews/2026-07-27-r23-readonly-review.md` §5.3).
   - **The gap:** `src/registry/heap_core_dealloc_batch.rs`'s
     `dealloc_batch_small` "Trade-off" doc comment (from the original R11-4
     commit `ff9ad7a`) claimed the LAST `TCACHE_CAP` blocks stay
     magazine-warm. The implementation iterates `for &p in blocks` in slice
     order and fills the magazine until `count == TCACHE_CAP`, then routes
     every further ACCEPTED block to `flush_class` — so with an empty
     magazine the FIRST `TCACHE_CAP` accepted blocks stay warm, the opposite
     of the claim.
   - **Decision (option (a) of the R24-7 brief): correct the documentation
     to describe the actual first-warm behavior; do NOT switch to a
     rolling-buffer last-warm algorithm.** `git blame` shows the "last warm"
     text was in the original R11-4 commit, unedited since, with no recorded
     rationale — an aspirational doc error matching scalar temporal-locality
     intuition, never verified against the always-first-warm implementation;
     there is no design reason "last" was specifically chosen that "first"
     would defeat. A last-warm rolling buffer would add, per overflow block,
     a `clear_magazine` RMW on a hot L1 bitmap line plus rotation/index
     arithmetic plus an extra stage write — strictly MORE per-block work
     than the current overflow arm (which only writes to `stage`), i.e. the
     SAME cost category two adjacent Round-24 tasks measured as NO-GO
     regressions in this exact code region: R24-3 (task #381,
     +37 Ir/overflow-event) and R24-4 (task #382, +14 Ir/block). The benefit
     (locality for "free a large batch then immediately realloc same
     class") is contested by the doc's own use-case argument AND has no
     in-tree consumer (R23-7: the batch API ships experimental with no
     production caller), so even a zero-cost switch would realize no
     production benefit today. Under that prior, prototyping the rolling
     buffer would very likely reproduce the R24-3/R24-4 regression class.
   - **The corrected doc comment's secondary claim is unaffected and still
     holds:** a small batch (`N <= TCACHE_CAP`) is byte-for-byte as warm as
     the scalar loop under EITHER first- or last-warm policy (all `N`
     accepted blocks fit the magazine).
   - **No in-context Ir measurement was run,** because option (b) was not
     pursued: the brief's own recommendation frames (a) as the default and
     (b) as the higher-bar prove-it-first path, and the structural prior
     (two NO-GOs in the same cost category + no consumer + the doc's own
     argument against the benefit) made the measurement very likely to only
     confirm a regression. The mandatory-if-pursued measurement gate
     therefore did not apply.
   - **Files changed:** `src/registry/heap_core_dealloc_batch.rs` (doc
     comment only) and this index. Zero `src/` behavior change; `git diff
     HEAD -- src/` shows only the doc-comment edit. No version bumps.

5. **`canary_survives_promotion_and_free_leaves_no_leak`'s leak-bound
   assertion proved no double-release, not no leak.** — **RESOLVED** by
   R28-2 (task #431), a test-only strengthening (no `src/` behavior change).

   - **The gap (recap):** the pre-existing `released_delta <=
     reserved_delta` assertion in
     `tests/r14_4_promotion_free_correctness.rs` is satisfied trivially by
     `reserved_delta=1, released_delta=0`, so a grow that reserves a segment
     and never releases it would pass silently — the assertion is a
     double-release/corruption guard, not a leak proof.
   - **Observable used — no new hook needed.** Investigated existing
     `SegmentTable`/diagnostic surface before adding anything: `HeapCore`
     already exposes `dbg_contains_base` (`&mut self`, gated
     `alloc-global + alloc-xthread`, `src/registry/heap_core_diag.rs:482`)
     and `dbg_live_count_for` (`&self`, gated `alloc-decommit`,
     `heap_core_diag.rs:317`), both safe `pub fn`s already appropriately
     gated per the benchmark-hook rule (not new — pre-existing, ungated
     wider than needed). Both gates are satisfied by plain `production`
     (`alloc-xthread` and `alloc-decommit` are both in the `production`
     feature list), so no new hook and no `bench-internals` dependency was
     required. To reach a `*mut HeapCore` for the CURRENT thread's own
     `SeferAlloc` from a `tests/` integration test (`SeferAlloc` itself
     exposes no direct `HeapCore` accessor), reused the SAME established
     save/poison/restore pattern `tests/dealloc_only_no_bind_torn.rs`
     already uses: `sefer_alloc::global::tls_heap::dbg_mark_local_torn_for_test()`
     (snapshot + poison `LOCAL`) immediately followed by
     `dbg_restore_local_for_test(saved)` (undo the poison), yielding the
     saved pointer — binding is per-THREAD (TLS), not per-`SeferAlloc`-
     instance, so this is exactly the same `HeapCore` the test's own
     `a.alloc`/`a.dealloc` calls already routed through.
   - **The new assertion:** resolves `grown`'s segment base
     (`dbg_segment_base_of_ptr`) and calls the production teardown-trim
     primitive `SeferAlloc::dbg_trim_current_thread()` (pre-existing,
     `src/global/sefer_alloc.rs:423` — flushes every tcache class, drains
     the empty-small-segment hysteresis pool, evicts the large_cache) BOTH
     immediately before taking a `live_count` baseline AND immediately after
     freeing `grown`, so both snapshots are read in the same converged,
     magazine/pool/cache-free regime (the double trim matters: a freed
     block routinely sits in the per-thread magazine rather than being
     reconciled into `live_count` immediately — see the gap found in
     development, below). After freeing and trimming, asserts `grown_base`
     is in exactly one of two sanctioned states: (a) fully unregistered
     (`dbg_contains_base == false` — the Large-segment-free path always
     calls `table.unregister` before returning, cache-admitted or not), or
     (b) still registered but with `live_count` decreased by EXACTLY one
     relative to the pre-free baseline (covers the `!HAS_PROMOTION`
     medium-ladder case, where segments are shared across size classes via
     a single per-thread `small_cur` bump cursor and routinely host other
     live blocks). Any other outcome (segment still registered with an
     unchanged or increased live_count) fails with a message naming the
     leak.
   - **Design iteration during development (kept in the report per the
     task's non-vacuity requirement, not just the final counterfactual):**
     two earlier designs were tried and rejected by ACTUAL feature-combo
     test runs, not just review — (1) a bare `dbg_contains_base(grown_base)
     == false` assumption failed under `production medium-classes
     exact-span-large` (`HAS_PROMOTION == false`) because medium-class
     segments are shared across size classes (`AllocCore::carve_block`'s
     single per-thread `small_cur`), so the segment legitimately stays
     registered with other live blocks; (2) an absolute
     `live_count_after_free == Some(0)` assumption also failed under the
     same combo (`live_count` went `Some(2)` before and after — unrelated
     co-tenant blocks were still magazine-buffered, not yet reconciled),
     which led to discovering the magazine-residency gap (`dealloc` does
     not call `dec_live` for a block that lands in the tcache — see
     `HeapCore::dbg_is_free_for`'s doc comment) and the final double-trim,
     before/after-delta design above.
   - **Non-vacuity — mutation counterfactual, run TWICE (once for the
     Large-promoted path, once for the medium-ladder path, since they are
     structurally different code paths):**
     - **Large path** (`production medium-classes`, `HAS_PROMOTION ==
       true`): commented out the `self.table.unregister(base)` call in the
       cache-admitted leg of `AllocCore::dealloc`'s Large branch
       (`src/alloc_core/alloc_core.rs:1451`), simulating a grow that
       deposits a segment into `large_cache` but never removes it from the
       table. New assertion FAILED immediately: `LEAK: grown_base (...) is
       still registered in the segment table ... live_count went from None
       to None`. Reverted; `git diff` confirmed byte-identical to the
       original; test passed again.
     - **Medium-ladder path** (`production medium-classes exact-span-large`,
       `HAS_PROMOTION == false`): commented out the
       `dec_live_batch_and_maybe_decommit`-driven block inside `flush_run`
       (`src/alloc_core/alloc_core_small_magazine.rs:682-693`, guarded with
       `#[cfg(any())]` for a clean single-site disable), simulating a leak
       where a block returned to the magazine-flush path never reconciles
       its live_count. New assertion FAILED with `live_count went from
       Some(2) to Some(2)` — exactly the "no change at all" signature the
       assertion's own doc comment predicts for this failure mode. Reverted;
       `git diff` confirmed byte-identical to the original; test passed
       again. (An earlier, less isolated counterfactual attempt at the same
       Large-branch call site under this combo produced a
       `STATUS_ACCESS_VIOLATION` crash instead of a clean assertion failure
       — because skipping `unregister` while `large_cache_slot_set` still
       ran created a genuinely double-owned segment that
       `dbg_trim_current_thread`'s `evict_all` then double-freed; this was
       diagnostic noise from an overly-blunt counterfactual site, not a
       defect in the new assertion, so the `flush_run` site above was used
       instead for a clean, isolated result.)
   - **CI-compatibility gap found and fixed during zero-trust review (before
     commit):** the strengthened block's own two accessors need
     `alloc-decommit` (`dbg_live_count_for`) and `alloc-xthread`
     (`dbg_contains_base`), a strictly NARROWER feature set than this file's
     own top-level `#![cfg(all(feature = "alloc-global", feature =
     "medium-classes"))]` gate — and `.github/workflows/ci.yml` runs a
     dedicated `test (--features "hardened medium-classes")` step
     (`hardened = ["fastbin"]` = `alloc-global + alloc-xthread`, WITHOUT
     `alloc-decommit`) that exercises this exact file. The as-delegated diff
     compiled clean only under the two combos it was directly tested against
     (`production medium-classes[, exact-span-large]`, both of which include
     `alloc-decommit` via `production`) and failed to compile under `hardened
     medium-classes` with two `E0599: no method named dbg_live_count_for`
     errors — confirmed via `cargo test --no-run --features "hardened
     medium-classes" --test r14_4_promotion_free_correctness` BEFORE the fix.
     Fixed by narrowing the new block's own gate to `#[cfg(all(feature =
     "alloc-decommit", feature = "alloc-xthread"))]` (a `let (heap,
     grown_base, live_count_before_free) = { ... };` tuple-block before the
     unconditional `a.dealloc` call, and a second `#[cfg(...)]` block after
     it for the assertion itself — the actual `a.dealloc(grown, ..)` call the
     pre-existing `released_delta <= reserved_delta` assertion needs stays
     UNCONDITIONAL either way) rather than widening the file's own top-level
     gate, which would have silently dropped this test from the `hardened
     medium-classes` CI row's coverage entirely. Re-verified after the fix:
     `cargo test --no-run --features "hardened medium-classes" --test
     r14_4_promotion_free_correctness` compiles clean and the test still
     passes (exercising only the original double-release guard, as before
     this task); both counterfactuals above were RE-RUN against the
     restructured code (not just the pre-restructure version) and still fail
     correctly.
   - **Verification:** `cargo test --release --features "production
     medium-classes" --test r14_4_promotion_free_correctness` and `cargo
     test --release --features "production medium-classes exact-span-large"
     --test r14_4_promotion_free_correctness` both green (2 passed, 0
     failed) after the final design landed. Repeat-run flake check: 35
     `cargo test` invocations plus 120 direct repeated binary invocations
     (60 per feature-combo binary, `--test-threads=4`) — 1 anomalous failure
     out of ~155 total runs, attributed to this session's heavy concurrent
     multi-agent build contention on the shared `target` directory
     (repeatedly observed "Blocking waiting for file lock on build
     directory" messages throughout), not reproduced in any of the
     subsequent 120 direct-binary runs. Full-suite regression check:
     `cargo test --release --features production` (226 `test result: ok`
     blocks, 0 `FAILED`) and `cargo test --release --features "production
     medium-classes"` (226 `test result: ok` blocks, 0 `FAILED`) both clean.
     `cargo fmt --check` clean on the changed file.
   - **Files changed:** `tests/r14_4_promotion_free_correctness.rs` and this
     index. No `src/` changes (the two counterfactual breaks used for
     non-vacuity verification were both reverted before this commit — `git
     diff` on `src/` is empty). No version bumps.
