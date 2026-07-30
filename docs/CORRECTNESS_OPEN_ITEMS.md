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

5. **Findings from the R29 post-round independent readonly review
   (`docs/reviews/2026-07-29-r29-readonly-review.md`) not yet independently
   re-verified or actioned beyond this index entry.** The review's two P0/P1
   build breaks (a missing iai-arm stub, an ungated dead-code pair) and the
   R29-16 wall-clock bench design bug were independently confirmed and
   fixed/corrected the same day (see `CHANGELOG.md`'s Round 29 entry and
   `docs/perf/OPEN_ITEMS.md` item 25 for those). The following were NOT
   independently re-verified before filing — flagged here at the review's
   own confidence/severity, for a future round to check and either action or
   dismiss:
   - **[P2 → CONFIRMED P1, 2026-07-30] `AllocCore::dbg_decomp_full_cycle`**
     (`src/alloc_core/alloc_core_small_pool.rs:1014`, R29-3/task #434) is a
     SAFE `pub fn` that calls `reserve_small_segment` then
     `release_or_pool_empty_segment` on the freshly-reserved base.
     **My original text here (below, struck) was FACTUALLY WRONG and is
     corrected in place; the review's claim was right.**
     > ~~"A first-pass trace during this session's own review suggested
     > `small_cur` is likely never touched by this hook at all (the
     > assignment it would need to collide with lives in a codepath
     > `dbg_decomp_full_cycle` doesn't call)."~~

     **Corrected trace (2026-07-30, verified line-by-line after a SECOND
     independent review — `docs/reviews/2026-07-30-r29-followup-readonly-review.md`
     §1.2 — reached the same conclusion and explicitly flagged my note as
     "based on a mistaken call trace"):** `self.small_cur = base;` is the
     **last statement of `reserve_small_segment` itself**
     (`alloc_core_small.rs:2210`, inside the fn spanning 1848–2212) — i.e. it
     lives in exactly the function the hook calls, not in a different
     codepath. My error was assuming the assignment belonged to
     `alloc_small_with_virgin` (its caller) rather than to the callee. The
     full confirmed sequence:
     1. `dbg_decomp_full_cycle` → `reserve_small_segment()` → sets
        `self.small_cur = base` (`alloc_core_small.rs:2210`).
     2. → `release_or_pool_empty_segment(base)`
        (`alloc_core_small_pool.rs:333`). Pool full ⇒
        `release_empty_segment_now(...)` + `self.table.recycle(base)`
        (`:380-381`) — the OS reservation is RELEASED and the slot recycled.
     3. Neither function clears or restores `small_cur`. It now points at
        unmapped memory.
     4. The next ordinary small alloc starts at
        `alloc_small_with_virgin`'s step 1,
        `self.pop_free(self.small_cur, ...)` (`alloc_core_small.rs:278`) — a
        read through the released segment's header.

     **Not hypothetical:** `examples/r29_3_decomposition_gate.rs:82-84`
     DELIBERATELY pre-fills the pool (`for _ in 0..(pool_cap + 2)`, its own
     comment: "not the pool-push path") specifically to drive the release
     branch, then loops the hook N=200 times. The only reason there is no
     in-tree crash is that the harness never performs an ordinary small
     allocation afterward — that is caller luck, not a sound API.
     `dbg_decomp_reserve_and_keep` + `dbg_decomp_release` has the identical
     state hazard (marking only the raw-pointer half `unsafe` expresses the
     pointer contract but not the cursor invariant).

     **Why R29-9's tripwire missed it:** the scanner only selects safe
     `pub fn dbg_*` whose signature text contains `*mut`/`*const`.
     `dbg_decomp_full_cycle(&mut self) -> bool` takes no pointer and returns
     none, so it is structurally out of scope — the zero-argument
     state-invalidating hole listed in the `[P3]` tripwire item below, now
     with a confirmed live instance.

     **Needs (Round 30, correctness-before-measurement):** either a
     measurement-only reservation primitive that does the OS/table/metadata
     work without touching the live `small_cur`, or save-and-restore of the
     prior cursor with an assertion that the restored base is still
     registered (preferred over merely making the hook `unsafe`, which would
     leave the allocator unusable-by-contract). Plus a counterfactual test:
     fill the pool → call the hook → perform and free a normal small alloc on
     the same heap; it must fail before the fix and pass after.

     **[FIXED, R30-1/task #450, 2026-07-30.]** Took fix option 1 (the
     "best" option in the task spec) — a measurement-only reservation
     primitive that never touches `small_cur` at all, rather than option 2
     (save/restore) or option 3 (`unsafe fn` + do-not-alloc-after contract).
     Option 1 was chosen because it was structurally cheap here:
     `reserve_small_segment`'s ENTIRE cursor-publishing side effect was
     already isolated to its literal last statement
     (`self.small_cur = base;`, immediately before `Some(base)`), with
     nothing earlier in the function reading `small_cur` and nothing after
     it depending on the write — so the function split cleanly into a new
     `pub(super) fn reserve_small_segment_impl(&mut self) -> Option<*mut u8>`
     (`alloc_core_small.rs`, everything BEFORE that last line) and a
     one-line `reserve_small_segment` wrapper (`let base =
     self.reserve_small_segment_impl()?; self.small_cur = base; Some(base)`)
     kept for the three production callers (`alloc_small`,
     `alloc_small_with_virgin`, `refill_class_bump_impl`), which still need
     the publish. `dbg_decomp_full_cycle` and `dbg_decomp_reserve_and_keep`
     (`alloc_core_small_pool.rs`) now call `reserve_small_segment_impl`
     instead, so `small_cur` is never touched by either hook and cannot be
     left dangling by them, at any pool fill level, however many times they
     run. `dbg_decomp_release` additionally got a defence-in-depth
     `debug_assert!(base != self.small_cur, ...)` (not reachable today, but
     cheap). Added `tests/r30_1_decomp_full_cycle_cursor_safety.rs` — two
     tests, one per hook pair named in the task, each: fills the pool to
     capacity, drives the release branch repeatedly via the hook, then
     performs an ordinary alloc + write + readback + free on the SAME heap.
     **Verified non-vacuous**: temporarily reverted the two hooks' call
     sites back to `reserve_small_segment()` (the pre-fix code) and reran —
     `full_cycle_hook_leaves_small_cur_valid_for_ordinary_alloc` crashed the
     whole test process with `STATUS_ACCESS_VIOLATION` (Windows hard fault,
     exit code `0xc0000005`), a genuine use-after-free through the dangling
     cursor — then re-applied the fix and confirmed both tests pass. Full
     `cargo test --features "bench-internals alloc-global alloc-xthread
     alloc-decommit fastbin alloc-segment-directory primordial-lazy-commit
     class-aware-dirty"` is green (228 test binaries, 0 failures) including
     this new test; `cargo clippy` clean on both `--features production`
     and `--features "production bench-internals"`; `cargo fmt --check`
     clean. The R29-3 gate (`docs/perf/R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md`)
     was re-run post-fix on its original WSL2/Linux measurement platform —
     verdict unchanged (trigger 2 still does not fire); see that doc's new
     §8 append-only correction section, which also documents a SEPARATE,
     pre-existing, unrelated finding surfaced during that re-verification
     (a native-Windows crash in the example's decommit/refault arm, caused
     by Windows `MEM_DECOMMIT` semantics differing from Linux
     `MADV_DONTNEED` — confirmed unrelated to this fix and filed as item 6
     below rather than fixed here).
   - **[P3 → partly CONFIRMED, 2026-07-30] `has_bench_internals_cfg()` accepts
     any cfg attribute whose TEXT merely contains `"bench-internals"`** — so
     `not(feature = "bench-internals")` and a permissive
     `any(feature = "bench-internals", ...)` would both be accepted as if
     they gated the hook (second review, §1.3). Read and confirmed by
     inspection. No live false-accept exists today (no `dbg_*` hook currently
     uses either shape), so this is a latent scanner weakness, not a live
     hole — but it is a substring test standing in for a cfg-predicate
     parse. Fix alongside the scope widening below.

     **[FIXED, R30-2/task #451, 2026-07-30.]** Replaced the substring test
     with a small hand-written recursive-descent parser
     (`CfgParser`/`CfgExpr`/`parse_cfg_inner`/`requires_bench_internals` in
     `tests/dbg_hook_safety_tripwire.rs`) for the actual cfg-predicate
     grammar subset this project uses: `feature = "x"`, `all(...)`,
     `any(...)`, `not(...)`, nested and comma-separated. `syn` is NOT a
     dev-dependency of this crate (checked `Cargo.toml`'s
     `[dev-dependencies]` before hand-rolling this — no `syn` entry), so a
     small hand-rolled parser was the right call over adding a dependency
     for a test-only predicate check. `requires_bench_internals` implements
     exactly the rule the review specified: `all(...)` counts if ANY child
     requires the feature (conjunction — one required child forces it on);
     `any(...)` NEVER counts, even in the degenerate case where every branch
     happens to require it individually (a deliberate refusal to reward the
     more permissive shape, since rewarding it would reopen exactly the
     `any(feature = "bench-internals", X)` hole); `not(...)` never counts.
     Two new dedicated tests: `cfg_parser_rejects_negated_and_optional_or_bench_internals`
     (unit-tests the parser directly against both target shapes plus the
     genuine-gate shapes already used in this crate, including nested
     `all(all(...))`) and `no_dbg_hook_cfg_uses_negated_or_optional_or_bench_internals_shape`
     (re-confirms, using the NEW structural parser rather than a substring
     match, that no current `dbg_*` hook in the crate actually uses either
     adversarial shape — the same "no live false-accept today" fact the
     review found by manual inspection, now asserted mechanically going
     forward). Both pass; see the R30-2 commit for verification details.
   - **[P3] `tests/dbg_hook_safety_tripwire.rs`'s allowlist may have scope
     holes**, per the review: possible misclassification of `any`/`not`
     `#[cfg]` predicates, hooks keyed by an integer parameter rather than a
     pointer, and zero-argument hooks that still return a raw pointer — none
     independently re-verified this session. If real, these are gaps in the
     R29-9 tripwire's own coverage (task #440), not yet a confirmed live
     soundness hole.

     **[FIXED, R30-2/task #451, 2026-07-30.]** Confirmed the scope gap was
     real and live, not merely theoretical: R30-1 (task #450, commit
     `25433c3`) found and fixed a CONFIRMED soundness bug
     (`AllocCore::dbg_decomp_full_cycle`, a zero-argument, no-raw-pointer,
     `&mut self -> bool` hook) that was structurally invisible to the R29-9
     scanner, exactly the "zero-argument hook" gap this item flagged as a
     possibility. Redesigned the tripwire's policy to be shape-independent
     per the review's own recommendation: every crate-public `dbg_*` hook
     (any signature shape — raw pointer, zero-arg `&mut self`,
     `usize`/index-keyed, or an integer-encoded-address return) is now
     enumerated by `scan_file` (which no longer branches on `*mut`/`*const`
     substrings at all) and must land in exactly one of three buckets:
     `PURE_OBSERVERS` (read-only, no justification needed beyond "read-only"),
     `SAFE_MUTATORS` (safe hooks that mutate allocator/ring state, each with
     a one-line invariant justification — bounds check, delegation to the
     identical production code path, or a correctness-inert policy/heuristic
     knob), or `UNSAFE_HOOKS` (already-`unsafe fn`, enumerated for exhaustive
     accounting only, not a new safety argument). Rebuilt the allowlist from
     scratch by enumerating every `pub fn dbg_*`/`pub unsafe fn dbg_*` in
     `src/` and `crates/` (~140 hooks) and reading each function BODY (not
     just its signature) to classify it — not guessed from names. Two hooks
     surfaced during that re-classification as worth flagging explicitly
     rather than silently accepted: `remote_free_ring.rs::dbg_set_cursors`
     and `heap_overflow.rs::dbg_reserve_unpublished_for_test` both mutate a
     REAL production ring's cursors under a documented "quiescent ring"
     precondition that is enforced only by a `debug_assert!` (compiled out
     in `--release`), not a release-surviving guard — allowlisted with an
     explicit `[DEBUG_ASSERT ONLY]` tag in their justification (misuse can
     only corrupt the ring's own bookkeeping — lost/miscounted entries —
     never dereference a caller pointer or write outside the ring's own
     cursor words, so accepted as SAFE_MUTATORS rather than escalated to
     KNOWN_UNFIXED, but flagged so a future reviewer does not have to
     re-derive that distinction). **Non-vacuity proof**: added
     `widened_scanner_catches_r30_1_shape_zero_arg_mutator`, which feeds
     `scan_file` a synthetic in-memory fixture mimicking the exact pre-fix
     `dbg_decomp_full_cycle` shape (`pub fn ..._SCRATCH_FIXTURE(&mut self)
     -> bool`, no cfg, no raw pointer anywhere in the signature — the
     fixture never touches real `src/`) and asserts the widened scanner
     finds it, classifies it safe/ungated, and that it is not allowlisted
     (i.e. it would surface as a tripwire failure) — then separately asserts
     the fixture's source text genuinely contains neither `*mut` nor
     `*const`, proving the OLD R29-9 scanner would have silently skipped it.
     Verification: `cargo test --features "bench-internals alloc-global
     alloc-xthread alloc-decommit fastbin alloc-segment-directory
     primordial-lazy-commit class-aware-dirty" --test
     dbg_hook_safety_tripwire` green (4 tests); full `cargo test --features
     "production bench-internals"` green (0 failures); `cargo clippy
     --features "production bench-internals" --tests -- -D warnings` clean;
     `cargo fmt --check` clean. R29-9's original commit message claim that
     it "closes the bug class for good" is corrected by this entry and by
     the widened test file's own header doc comment — the bug class
     recurred through a shape the original scanner didn't model, and R30-2
     makes the check shape-independent instead of re-asserting closure.
   - **[P3] R29-1's replacement leak-bound invariant** (`segments_released_total
     <= segments_reserved_total`, task #432) may be "near-unfalsifiable" per
     the review, meaning most of the file's actual leak coverage now rests
     on the `alloc-decommit + alloc-xthread`-gated per-base diagnostic block
     rather than the global counter itself. Not independently re-verified;
     if true this narrows (but per R29-1's own scope note does not
     eliminate) what the global invariant alone catches.

     **[FIXED, R30-11/task #460, commit `a32acf9`.]** Confirmed the review
     right: the cumulative invariant proves no impossible double/over-release
     occurred, but a MISSING release only makes it MORE comfortably true, so
     it has zero leak-detection power on its own — exactly the concern this
     item flagged. R29-1's LOGIC was untouched (still correct, not
     re-litigated); the defect was that
     `tests/r14_4_promotion_free_correctness.rs`'s single combined test
     function, `canary_survives_promotion_and_free_leaves_no_leak`, kept a
     name promising "no leak" in EVERY feature combination it compiled
     under — including the CI-tested `hardened medium-classes` row
     (`hardened = ["fastbin"]` = `alloc-global + alloc-xthread`, WITHOUT
     `alloc-decommit`), where only the cumulative check (renamed, see below)
     exists and the real per-base leak proof does not compile at all. Split
     into two `#[test]`s, each named for exactly what it proves:
       - `canary_survives_promotion_and_free_no_double_release` — always
         compiled under the file's top-level gate; canary survival + the
         cumulative `segments_released_total <= segments_reserved_total`
         invariant (renamed from the ambiguous `no_double_release` framing
         to an explicit "over-release", not "leak", claim in both the local
         variable name and the assertion failure message) + no corruption.
         Never claims "no leak".
       - `canary_survives_promotion_and_free_leaves_no_leak_per_base` — NEW
         name, gated `alloc-decommit + alloc-xthread` (unchanged gate,
         unchanged assertion logic from the pre-existing per-base block);
         the genuine per-base leak proof, compiled only where its
         diagnostic surface (`dbg_live_count_for`, `alloc-decommit`-gated)
         exists.
     **Confirmed no stronger diagnostic is available for `hardened
     medium-classes`** (deliverable 3(b) — investigated, not assumed):
     `dbg_contains_base` alone (`alloc-global + alloc-xthread`, available
     under `hardened`) cannot substitute for `live_count`, because without
     `alloc-decommit` the small-segment release/pool machinery itself
     (`dec_live_and_maybe_decommit` / `dec_live_batch_and_maybe_decommit`,
     `src/alloc_core/alloc_core_small_pool.rs`) is entirely
     `#[cfg(feature = "alloc-decommit")]` — small/medium segments are never
     released or live-count-tracked at all under that combo, so
     `dbg_contains_base` would read `true` forever regardless of whether a
     leak occurred. This is an honest, documented gap (module doc + the
     `no_double_release` test's own doc comment in
     `tests/r14_4_promotion_free_correctness.rs`), not a silently accepted
     one.
     **Non-vacuity re-verified (both of R28-2's original counterfactual
     paths, against the restructured file):**
       - **Large-promoted path** (`production medium-classes`): disabled
         `self.table.unregister(base)` in the cache-admitted leg of
         `AllocCore::dealloc`'s Large branch
         (`src/alloc_core/alloc_core.rs`, `#[cfg(any())]`) — reproduces
         R28-2's own documented alternate outcome at this exact site: a
         deterministic `STATUS_ACCESS_VIOLATION` crash (both `--release`
         and debug profiles), because the segment becomes genuinely
         double-owned (still in `large_cache` AND left registered) and
         `dbg_trim_current_thread`'s `evict_all` double-frees it. A crash is
         still a detected, non-vacuous `cargo test` failure (nonzero exit,
         reported as failed) — reverted cleanly (`git diff` empty on
         `src/`), test passes again.
       - **Medium-ladder path** (`production medium-classes
         exact-span-large`): disabled the `dec_live_batch_and_maybe_decommit`
         block in `flush_run` (`src/alloc_core/alloc_core_small_magazine.rs`,
         `#[cfg(any())]`) — clean assertion failure,
         `live_count went from Some(2) to Some(2)`, exactly the "no change
         at all" signature the assertion's own doc comment predicts.
         Reverted cleanly (`git diff` empty on `src/`), test passes again.
     Verified personally: all three relevant combos compile and pass
     (`hardened medium-classes` — 2 tests, no per-base test present;
     `production medium-classes` and `production medium-classes
     exact-span-large` — 3 tests each, per-base test present and passing);
     `cargo clippy --features "hardened medium-classes" --all-targets -- -D
     warnings` and `cargo clippy --features "production bench-internals"
     --all-targets -- -D warnings` both clean; `cargo fmt --check` clean.
     No `src/` behavior change (the two counterfactual breaks used for
     non-vacuity verification were both reverted before this commit — `git
     diff` on `src/` is empty). No version bumps.

6. **[T, filed 2026-07-30 during R30-1/task #450's verification]
   `examples/r29_3_decomposition_gate.rs` crashes with
   `STATUS_ACCESS_VIOLATION` when run NATIVELY on Windows** (as opposed to
   under WSL2/Linux, which is where this example's own gate report,
   `docs/perf/R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md`, has always
   measured — see that doc's "Platform measured" line). The crash is in
   Measurement B: the `write_volatile` re-fault loop immediately after
   `HeapCore::dbg_decomp_decommit_payload`. Root cause: Windows
   `MEM_DECOMMIT` (`crates/vmem/src/lib.rs`'s `cfg(windows)`
   `decommit_pages_impl`) genuinely UNMAPS the payload pages, unlike Linux
   `MADV_DONTNEED`, which keeps the VA mapping resident and transparently
   re-faults a fresh zero page on next write. The example's Measurement B
   loop assumes the Linux semantics unconditionally (write-after-decommit
   silently re-faults); on Windows a write to a decommitted-but-not-yet-
   recommitted page is a hard access violation without an explicit
   `VirtualAlloc(..., MEM_COMMIT, ...)` recommit call, which the example
   never makes. **Confirmed unrelated to R30-1's `small_cur` fix**: the
   crash reproduces identically with that fix applied or reverted, and
   lives in a code path (`dbg_decomp_decommit_payload` → `os::decommit_pages`
   → `crates/vmem`) R30-1's diff never touches; isolated by running just
   the R30-1-relevant hooks' pre-fill/A/C/A' loops (which never call
   `dbg_decomp_decommit_payload`) natively on Windows for hundreds of
   iterations with no crash. **Needs (future round):** either gate
   Measurement B's re-fault loop on `cfg(not(windows))` with an honest
   "irreducible floor not measured on this platform" note, or add the
   missing `VirtualAlloc(MEM_COMMIT)` recommit call before the
   `write_volatile` loop so the measurement is platform-correct everywhere
   (this would also make Measurement B's timing include the ACTUAL Windows
   recommit cost — currently assumed `0 ns` "implicit" per the doc's own
   §2 table, which is a Linux-only claim; Windows `MEM_COMMIT` is a real
   syscall, not implicit).

7. **[T, filed 2026-07-30, R30-10/task #459]
   `dbg_decomp_reserve_and_keep`/`dbg_decomp_release`
   (`src/alloc_core/alloc_core_small_pool.rs:1070-1115`) mint-then-redeem a
   bare `*mut u8` segment base with only a `debug_assert!` (compiled out in
   `--release`) guarding against releasing the live `small_cur` cursor —
   the same hazard class R30-1 (task #450) fixed for `dbg_decomp_full_cycle`,
   still standing on a weaker, non-release-surviving backstop for this
   specific pair. R30-10's design evaluation
   (`docs/design/R30_10_MEASUREMENT_HOOK_ISOLATION_DESIGN.md` §5) found this
   is the ONE current hook pair in the crate that both mints a NEW raw
   pointer via a `dbg_*` call and requires the caller to hold and later hand
   it back — the shape a typed, non-forgeable, move-consumed handle
   (`ReservedSmallSegment`, sketched in that document's §5.2-5.3) would fix
   structurally: a forged handle becomes uncomputable (private field +
   `pub(crate)` constructor) and a double-release becomes a compile error
   (E0382, moved value) instead of an unchecked runtime hazard. NOT
   implemented this round — the retrofit is a small (~5-file) but
   NOT-zero-risk diff (touches `AllocCore`'s definition, `HeapCore`'s
   forwarding delegate in `heap_core_diag.rs:854-857`, and both real
   callers, `examples/r29_3_decomposition_gate.rs` and R30-1's OWN
   counterfactual regression test `tests/r30_1_decomp_full_cycle_cursor_safety.rs`)
   that deserves its own review as the first typed-handle pattern in this
   codebase, not a same-task rubber stamp alongside the design doc that
   proposes it. **Trigger to action:** either (a) a 6th confirmed instance
   of the R25-1/R29-7/R29-8/R29-17/R30-1 "safe `dbg_*` hook touches live
   allocator state unsoundly" bug class, or (b) any future task adding a
   SECOND mint-then-redeem raw-pointer `dbg_*` pair to the inventory
   (enumerated in `tests/dbg_hook_safety_tripwire.rs`), at which point one
   handle type amortizes across both pairs. Full crate-wide hook
   relocation into one module — the OTHER piece of the architecture this
   task evaluated — was declined outright, not deferred: measured at
   102-139 distinct `tests/`/`examples/`/`benches/` files touched (4-5x the
   ~26-file footprint R24-6/task #384 already declined for a SINGLE hook,
   `dbg_push_to_ring`), AND independently shown to not address the actual
   defect mechanism in any of the five real incidents (each was fixed by
   changing hook BODY/signature, never by relocation — see the design
   doc's §3 table). Not re-opened by this trigger; would need a materially
   different argument (e.g. a demonstrated scatter-caused maintenance cost,
   not just a recurrence of the already-explained bug class) to revisit.

   **[FIXED, R31-4/task #467, commit `<R31_4_SHA_PLACEHOLDER>`, 2026-07-30/31.]**
   Implemented `ReservedSmallSegment` exactly per §5.2-5.3's sketch, in a new
   one-export file (`src/alloc_core/reserved_small_segment.rs`, per this
   project's file-structure rule): a private `base: *mut u8` field, a
   `pub(super)` constructor (`new_from_reservation`) reachable only from
   `AllocCore`'s own reservation path inside `alloc_core_small_pool.rs` — no
   `pub` constructor exists anywhere in the crate, so a handle cannot be
   forged from an arbitrary address — and a `pub(super) fn into_base(self)
   -> *mut u8` that consumes the handle by value (`core::mem::forget`ting
   `self` first to disarm the `Drop` leak-detector, since this consumption
   IS the release, not a leak). `dbg_decomp_reserve_and_keep` now returns
   `Option<ReservedSmallSegment>`; `dbg_decomp_release` now takes
   `ReservedSmallSegment` by value and is NO LONGER `unsafe fn` — the
   precondition that used to live in an `unsafe fn`'s `# Safety` contract is
   now upheld by the type itself. Calling `dbg_decomp_release(handle)` twice
   on the same binding is `rustc` error E0382 ("use of moved value") at
   COMPILE time — verified as the actual mechanism (not merely asserted) by
   confirming `ReservedSmallSegment` derives no `Copy`/`Clone` and carries a
   `Drop` impl, which makes `Copy` a hard compiler-rejected combination, not
   a project convention that could silently lapse. The existing
   `debug_assert!(base != self.small_cur, ...)` R30-1 added stays as
   secondary defence-in-depth. A `#[doc(hidden)] pub fn base(&self) -> *mut
   u8` read-only accessor (the established test-only-export pattern) lets
   `examples/r29_3_decomposition_gate.rs` read the payload address between
   reserve and release for its `write_volatile` measurement, without
   weakening the unforgeability guarantee (reading a value out is not
   constructing a new handle). Updated exactly the ~5 files the design doc
   estimated: `src/alloc_core/alloc_core_small_pool.rs` (the two hook
   definitions), `src/registry/heap_core_diag.rs` (the `HeapCore` forwarding
   delegates), `examples/r29_3_decomposition_gate.rs`,
   `tests/r30_1_decomp_full_cycle_cursor_safety.rs` (R30-1's own
   counterfactual regression test — re-verified still passes, both its
   tests, after the retrofit), and `tests/dbg_hook_safety_tripwire.rs`
   (removed both `dbg_decomp_release` entries from `UNSAFE_HOOKS` — the hook
   is safe now and stays `bench-internals`-gated, so it needs no
   `SAFE_MUTATORS` entry either). New counterfactual test
   `tests/r31_4_reserved_small_segment_handle.rs`: checked `Cargo.toml`'s
   `[dev-dependencies]` first — no `trybuild` or equivalent compile-fail
   harness exists in this crate — so per this task's own instruction, no new
   test-tooling dependency was added; instead the file thoroughly exercises
   the legitimate single-use and repeated-use (16-cycle) paths and documents,
   in both a code comment and `ReservedSmallSegment`'s own module doc,
   exactly why a second release call cannot compile. Full verification:
   `cargo test --features "production bench-internals alloc-stats"` green
   (230 test binaries, 0 failed); `cargo test --features "bench-internals
   alloc-global alloc-xthread alloc-decommit fastbin alloc-segment-directory
   primordial-lazy-commit class-aware-dirty" --test dbg_hook_safety_tripwire`
   green (7 tests); `cargo clippy --features "production bench-internals
   alloc-stats" --all-targets -- -D warnings` clean; `cargo clippy --features
   production -- -D warnings` clean, confirmed via a throwaway compile probe
   that `HeapCore::dbg_large_cache_hits` and (transitively)
   `dbg_decomp_reserve_and_keep`/`dbg_decomp_release` are genuinely absent
   from a plain `production` build; `cargo fmt --check` clean. Fixed two
   resulting doc-drift failures as a side effect (test-file count 227→228,
   README tier-2 `#[allow(unsafe_code)]` site count 68→66 — see item 8's
   entry below for why the count dropped by 2, not the expected-from-this-
   item-alone 1). No `production` feature composition changed.

8. **[T, filed 2026-07-30, UNVERIFIED-BY-ME findings from the Round 30 full
   independent review (`docs/reviews/2026-07-30-r30-full-review.md` §5
   P2-1/P2-2)]** The following two P2 findings were NOT independently
   re-verified before filing — flagged here at the review's own
   confidence/severity, for a future round to check and either action or
   dismiss, per this file's own convention (item 5 above is the precedent
   for this exact "filed, not fixed" pattern):
   - **P2-1 — `has_bench_internals_cfg` (`tests/dbg_hook_safety_tripwire.rs:657`)
     accepts `#[cfg_attr(...)]` as if it were a genuine `#[cfg(...)]` gate,
     latent instance of the same substring-match class R30-2 (task #451)
     fixed for two other shapes.** The review's claim: the parser's 5-byte
     prefix match `#[cfg` also matches `#[cfg_attr(`, and the parser then
     reads `cfg_attr`'s first argument (its *predicate*, not a gate
     condition on the attribute's own presence) as if it were a `cfg`
     predicate — the review states it proved this by extracting lines
     471-702 verbatim into a standalone `rustc` binary outside this repo
     and observing `cfg_attr(feature = "bench-internals", allow(dead_code))`
     parse as `true` (i.e. treated as a genuine gate). The review also
     states no live instance exists today (no `cfg_attr` in `src/` or
     `crates/` mentions `bench-internals`, per its own grep) — i.e. this
     is a latent parser gap, not a currently-exploitable hole, the same
     status R30-2 itself gave the two `cfg` shapes it did fix. Suggested
     fix per the review: match the literal `#[cfg(` (including the open
     paren) instead of the shorter `#[cfg` prefix.
   - **P2-2 — `HeapCore::dbg_large_cache_hits` (new, R30-6/task #455) is
     gated `alloc-decommit` alone, not `all(alloc-decommit,
     bench-internals)` like its four sibling measurement delegations in
     the same file.** The review's claim:
     `src/registry/heap_core_diag.rs:352-357` gates the hook on
     `alloc-decommit` alone (justified in its own doc comment as "matching
     `AllocCore::dbg_large_cache_hits`'s own gate exactly"), which is
     inside `production` (`Cargo.toml:399`) and so widens a `production`
     build's safe public surface; the same file's other four measurement
     delegations (`dbg_pool_cap`, `dbg_segment_state_reconciliation`,
     `dbg_large_cache_used`, `dbg_large_cache_slot_sizes`) are each gated
     `all(alloc-decommit, bench-internals)` and each cite "no production
     caller -> R25-10 sub-rule 2" — the CLAUDE.md benchmark-hook rule that
     any hook with no production caller MUST default to `bench-internals`
     unless it is the one sanctioned `dbg_push_to_ring` exception. The
     review notes this is NOT a soundness issue (the hook is
     `&self -> u64`, read-only, no pointer parameter, no mutation) and
     that `tests/dbg_hook_safety_tripwire.rs`'s `PURE_OBSERVERS` list
     already includes it (R30-6 added it there), so the R30-2 tripwire
     itself is satisfied — the finding is specifically that "the
     delegated method's pre-existing gate" is the reasoning CLAUDE.md's
     rule 2 rejects for NEW hooks, applied here to a genuinely new hook.
     Suggested fix per the review: add `feature = "bench-internals"` to
     its `cfg` and adjust the tripwire's gate-list accordingly (the
     review states the R30-6 probe that calls it already requires
     `bench-internals`, so nothing else should break).
   - **Next trigger:** independent re-verification of both claims (re-run
     the review's standalone `rustc` cfg-parser extraction for P2-1;
     re-read `heap_core_diag.rs:302-373` and the tripwire's gate-list for
     P2-2), then either apply the review's suggested one-line fixes or
     record a reasoned dismissal, in a future round.
   - **Evidence:** `docs/reviews/2026-07-30-r30-full-review.md` §5 P2-1,
     P2-2 (the review's own text is the only source cited here — this
     entry is a filing, not an independent confirmation).

   **[FIXED, R31-4/task #467, commit `<R31_4_SHA_PLACEHOLDER>`, 2026-07-31.]**
   Both claims independently re-verified before fixing, per the "Next
   trigger" instruction above.

   - **P2-1 confirmed and fixed.** Re-derived the review's claim directly
     (not by re-running its external `rustc` extraction, but by tracing
     `has_bench_internals_cfg`'s own logic): the 5-byte match `&bytes[i..i +
     5] == b"#[cfg"` matches the prefix of `#[cfg_attr(`, and
     `parse_cfg_inner` parses only the FIRST term of the parenthesised text
     that follows — for `#[cfg_attr(feature = "bench-internals",
     allow(dead_code))]` that is `feature = "bench-internals"`, which
     `requires_bench_internals` correctly reports `true` for, wrongly
     treating a non-gating `cfg_attr` predicate as a genuine gate. Fixed by
     requiring the literal 6-byte `#[cfg(` (open paren included), which
     structurally cannot match `#[cfg_attr(` (7th byte is `_`, not the
     paren the match now requires at position 6). Two new tests added:
     `has_bench_internals_cfg_rejects_cfg_attr_shape` (direct unit proof
     against the exact adversarial string, plus a regression guard that a
     genuine `#[cfg(feature = "bench-internals")]` is still accepted) and
     `scan_file_treats_cfg_attr_bench_internals_hook_as_ungated` (end-to-end
     proof via the real `scan_file` classifier against a synthetic
     `cfg_attr`-decorated hook fixture — confirms it surfaces as UNGATED,
     the correct conservative behavior, not silently accepted as gated). A
     third test, `no_dbg_hook_cfg_uses_cfg_attr_bench_internals_shape`,
     re-confirms (mechanically, going forward) the review's own finding that
     no CURRENT hook in `src/`/`crates/` uses this shape — this was a
     latent scanner gap, not a live false-accept, matching the review's own
     assessed severity.
   - **P2-2 confirmed and fixed.** Re-read `heap_core_diag.rs`'s
     `dbg_large_cache_hits` and confirmed the review's claim exactly: gated
     `#[cfg(feature = "alloc-decommit")]` alone (inside `production`), while
     its four siblings in the same file (`dbg_pool_cap`,
     `dbg_segment_state_reconciliation`, `dbg_large_cache_used`,
     `dbg_large_cache_slot_sizes`) are each gated `all(alloc-decommit,
     bench-internals)`. Tightened to match. Verified BOTH current callers
     (`examples/r30_6_large_cache_headroom_ab_gate.rs`,
     `examples/r31_1_large_cache_headroom_crossing_regime_gate.rs`) already
     list `bench-internals` in their `Cargo.toml` `required-features` — the
     review's own prediction ("nothing else should break") held, confirmed
     rather than assumed. Removed `"src/registry/heap_core_diag.rs::dbg_large_cache_hits"`
     from `tests/dbg_hook_safety_tripwire.rs`'s `PURE_OBSERVERS` list (a
     gated hook is tracked in neither allowlist — `scan_file` only feeds
     ungated hooks into the allowlist-diff check). Confirmed the hook is
     genuinely unreachable under plain `production` via a throwaway compile
     probe (`E0599: no method named 'dbg_large_cache_hits' found` when
     building against `--features production` alone, deleted after
     confirming) rather than assuming the `#[cfg]` change alone was
     sufficient proof. This also fixed the two doc-drift test failures item
     7's entry above flags: removing `dbg_decomp_release`'s TWO `unsafe fn`
     `#[allow(unsafe_code)]` item-scoped sites (one in `AllocCore`, one in
     `HeapCore`'s delegation — item 7's retrofit, not this item's gating
     change) dropped README's tier-2 count from 68 to 66; the new
     `tests/r31_4_reserved_small_segment_handle.rs` file brought
     `docs/ARCHITECTURE.md`'s tracked test-file count from 227 to 228. Both
     docs updated to match; `tests/no_stale_doc_references.rs` green.
   - **Verification (both P2-1 and P2-2 together):** `cargo test --features
     "production bench-internals alloc-stats"` green (230 test binaries, 0
     failed, including the 3 new P2-1 tests); `cargo test --features
     "bench-internals alloc-global alloc-xthread alloc-decommit fastbin
     alloc-segment-directory primordial-lazy-commit class-aware-dirty"
     --test dbg_hook_safety_tripwire` green (7 tests — confirms the
     tripwire's allowlist is accurate after the P2-2 gating change); `cargo
     clippy --features "production bench-internals alloc-stats"
     --all-targets -- -D warnings` clean; `cargo clippy --features
     production -- -D warnings` clean; `cargo fmt --check` clean.

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

   - **R29-1 correction (2026-07-29, task #432) — REOPENED then RE-RESOLVED
     with a real root cause.** The R28-2 entry above recorded "1 anomalous
     failure out of ~155 total runs, attributed to this session's heavy
     concurrent multi-agent build contention on the shared `target`
     directory" and marked the item RESOLVED on that attribution. An
     independent readonly review
     (`docs/reviews/2026-07-29-r28-readonly-review.md` §"P0 — the R28-2
     anomalous failure is not explained") flagged that build-lock contention
     explains DELAY, not a COMPLETED assertion failure, and that the original
     root cause was therefore unproven. R29-1 investigated and confirmed the
     review was right to flag it: the anomaly is REAL (reproduced), but its
     root cause is a **test-logic bug (classification (a) from the task's
     taxonomy), NOT an allocator correctness concern, NOT infrastructure**.
     - **Reproduction:** the test binary was built from a PRIVATE isolated
       target dir (`target-r29-1-isolated/`, since cleaned up) so shared
       build-lock contention was structurally eliminated from the loop —
       the same isolation technique R26-1 used. A 2000-run sweep of the
       `production medium-classes` combo (the Large-promotion path,
       `HAS_PROMOTION == true`) using a HYBRID binary that kept the ORIGINAL
       windowed assertion (`released_delta <= reserved_delta`) but added
       failure-path-only diagnostics reproduced **6 failures out of 2000
       (0.30%)**, all with an IDENTICAL trajectory — evidence captured in
       `docs/_raw_r29_1_repro_captured.log` (150 lines, 6 full
       stdout/stderr dumps). The `production medium-classes exact-span-large`
       combo (`HAS_PROMOTION == false`, the medium-ladder path) showed **0
       failures in 600 runs** — the bug is specific to the Large-promotion
       path.
     - **Classification (a) — proven, not inferred.** Every one of the 6
       captured failures shows: (1) the R28-2 per-base leak proof at line 284
       PASSED (it ran before the failing line-319 guard and did not fire) —
       `grown`'s own segment was correctly freed
       (`still_registered=false`, `live_count_before_free=None`,
       `live_count_after_trim_recheck=None`); (2) the GLOBAL cumulative
       invariant held at failure time (`reserved_total=4 >
       released_total=2`) — no double-release. The failing trajectory was
       always: `reserved before=3 after_promote=4 after_free=4 | released
       before=0 after_promote=1 after_free=2` — i.e. the promotion grow
       released `p`'s now-empty OLD segment (reserved during heap/TLS init
       or by the sibling test via the persistent TLS heap binding, BEFORE
       this test's `stats_before` snapshot) INSIDE the window, while only 1
       segment (grown's Large) was reserved INSIDE the window. The windowed
       `released_delta <= reserved_delta` guard's premise ("every in-window
       release has a matching in-window reserve") is INVALID for
       process-wide cumulative counters read over an arbitrary snapshot
       window — a segment reserved before the window can be released inside
       it. This is the SAME mechanism the earlier `TEST_LOCK` fix (item 1
       above) partially addressed (the concurrency race between the two
       test FUNCTIONS) but did NOT fully close: the `TEST_LOCK` serializes
       against the sibling test function's concurrent activity, but NOT
       against segments left in the persistent TLS heap by PRIOR test
       invocations on the same thread, whose later release crosses the
       window. NO `src/` allocator code is implicated — the R28-2 per-base
       proof (the real leak detector) correctly passed in all 6 failures.
     - **Fix applied:** replaced the unsound WINDOWED assertion
       (`released_delta <= reserved_delta` since `stats_before`) with the
       sound GLOBAL cumulative invariant
       (`segments_released_total <= segments_reserved_total`, no windowing)
       — which is window-independent and exactly captures the guard's stated
       intent ("a double-release would indicate corruption": only a genuine
       double-release of the same OS reservation can push global released
       past global reserved). Leak detection was never this counter's job
       (the R28-2 entry's own "Scope" note at lines 121-132 already said so)
       — it is the per-base proof's job, which is reliable and
       segment-specific. The windowed deltas are retained as diagnostic
       CONTEXT only (printed on the failure path, never asserted). Failure-
       path-only diagnostics (zero pass-path cost, so they do not perturb
       the timing of the race they diagnose) were added on every assertion
       path in the test: the trajectory across all three snapshots, plus the
       cfg-gated per-base `still_registered`/`live_count` state, so a future
       CI failure is self-diagnosing from logs alone.
     - **Fix verified:** a 2000-run sweep of the same `production
       medium-classes` combo with the global-invariant fix showed **0
       failures out of 2000** (evidence: `docs/_raw_r29_1_confirm_captured.log`,
       5 lines), against the pre-fix 6/2000. All three CI-relevant feature
       combos compile clean, including the cfg-narrowing-sensitive `hardened
       medium-classes` combo (= `fastbin` + `medium-classes` = `alloc-global
       + alloc-xthread` WITHOUT `alloc-decommit`, where the per-base
       diagnostic block's `#[cfg(all(feature = "alloc-decommit", feature =
       "alloc-xthread"))]` gate correctly compiles out — the same
       R28-2-documented cfg-narrowing gap, re-verified not reintroduced).
     - **Corrected status:** the R28-2 "1 anomalous failure attributed to
       build contention" hypothesis is **REFUTED** — the anomaly is a real
       ~0.3% false-positive rate of an unsound windowed assertion form, now
       fixed. The item is **RESOLVED** on the corrected root cause
       (classification (a), test-logic window-asymmetry bug, fixed and
       verified at 0/2000). This is NOT a still-live allocator correctness
       concern and does NOT block anything.
