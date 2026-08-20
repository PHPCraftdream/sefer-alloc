# Correctness / CI-debt open items — [T] Tracked tier (items 5-8)

**Part of the split index.** This file holds the full text of **[T]**
(tracked, not yet actioned) cards **5 through 8**. Start at
`docs/CORRECTNESS_OPEN_ITEMS.md` for the purpose/scope/convention
header and the round-start reading order; come here for these
specific card bodies. See `docs/correctness-open-items/ACTIVE.md` for the
**[A]** tier, `docs/correctness-open-items/RESOLVED.md` for the closure
trail, and the sibling `TRACKED_009_018.md` / `TRACKED_019_043.md` /
`TRACKED_044_093.md` files for the rest of the **[T]** tier's number
ranges (the "see 'Recently resolved' in RESOLVED.md" notes inside this
file's own item-1..4 stub pointers refer to that sibling file, not a
section further down this one).

**Why split by number range, not by topic (task #1221, 2026-08-20):**
this file is one of four that together replace the single
`docs/correctness-open-items/TRACKED.md` (2,322 lines, task #1217),
which had itself grown past CLAUDE.md's R34-24 ~1,000-line threshold.
Every one of the 42+ code/CI/script citations of this index across the
repo cites an item by NUMBER (`` `docs/CORRECTNESS_OPEN_ITEMS.md` item
N ``), never by line or topic — so a number-range filename is a
one-hop lookup with no translation table required, and needed no new
taxonomy invented under time pressure (a thematic split was considered
and rejected for exactly that reason). Ranges were chosen to balance
by LINE COUNT (card sizes vary enormously — one card here is 293
lines), not by card count: this file is 4 cards / ~638 lines; see the
sibling files for the other three ranges (10 cards/~518 lines; 13
cards/~573 lines; 50 cards/~577 lines). (Split 2026-08-20, task #1221.)

---

### [T] Tracked, not yet actioned

_(item 1, the `canary_survives_promotion_and_free_leaves_no_leak` flaky test,
was resolved by an urgent CI-fix task — see "Recently resolved" in RESOLVED.md.)_

_(item 2, the 11 `--features "hardened medium-classes"` clippy dead-code
errors, was resolved by R23-5 (task #374) — see "Recently resolved" in RESOLVED.md.)_

_(item 3, the two flaky coarse-wall-clock tests, was resolved by R23-6
(task #375) — see "Recently resolved" in RESOLVED.md.)_

_(item 4, `canary_survives_promotion_and_free_leaves_no_leak`'s leak-bound
assertion proving no double-release but not no leak, was resolved by R28-2
(task #431) — see "Recently resolved" in RESOLVED.md.)_

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
   `MEM_DECOMMIT` (`crates/aligned-vmem/src/os/windows.rs`'s
   `decommit_pages_impl` — post-split home, task #1082) genuinely UNMAPS the payload pages, unlike Linux
   `MADV_DONTNEED`, which keeps the VA mapping resident and transparently
   re-faults a fresh zero page on next write. The example's Measurement B
   loop assumes the Linux semantics unconditionally (write-after-decommit
   silently re-faults); on Windows a write to a decommitted-but-not-yet-
   recommitted page is a hard access violation without an explicit
   `VirtualAlloc(..., MEM_COMMIT, ...)` recommit call, which the example
   never makes. **Confirmed unrelated to R30-1's `small_cur` fix**: the
   crash reproduces identically with that fix applied or reverted, and
   lives in a code path (`dbg_decomp_decommit_payload` → `os::decommit_pages`
   → `crates/aligned-vmem`) R30-1's diff never touches; isolated by running just
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

   **[FIXED, R31-4/task #467, commit `ca9aba9`, 2026-07-30/31.]**
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

   **[REOPENED then RE-FIXED, R31-15/task #486, 2026-08-01.]** The R31-4
   "FIXED" verdict directly above was PARTIAL, not complete: it closed
   unforgeability and double-release, but NOT owner-binding — a third,
   separate hazard the R31-4 entry's own prose never claimed to address (it
   describes the forgery and double-release guarantees specifically, never
   an owner check). CONFIRMED (independently verified by the task filer
   before filing, not just a review's claim) as a real, safe-reachable P0:
   `AllocCore::dbg_decomp_release(&mut self, handle: ReservedSmallSegment)`
   was a **safe** `pub fn`, and `ReservedSmallSegment` stored only a
   `base: *mut u8` with no owner identity — nothing stopped
   `core_b.dbg_decomp_release(h)` where `h` was reserved on `core_a`, both
   calls type-checking and compiling as ordinary safe code. Verified
   non-vacuously by temporarily reverting the R31-4-era source (`git stash`
   on just the three touched `src/` files) and confirming a throwaway probe
   test performed the cross-core release with **no panic on any build
   profile** — the pre-fix code had zero owner-related guard, release or
   debug. Fixed two ways, layered:
   1. **Structural owner token.** A new `bench-internals`-gated field
      `AllocCore::dbg_reservation_owner_id: u64`, stamped once at
      construction from a process-wide monotonic `AtomicU64` counter
      (`DBG_RESERVATION_OWNER_ID_COUNTER`, `alloc_core.rs`) — deliberately
      NOT the `&self` address (an `AllocCore` can move: it is returned by
      value from `AllocCore::new()` and lives inline in every registry
      `HeapSlot`, so two different logical `AllocCore`s can occupy the same
      address at different times over a process's life). `ReservedSmallSegment`
      gained a matching private `owner_id: u64` field, stamped by
      `dbg_decomp_reserve_and_keep` from the minting core's id.
      `dbg_decomp_release` compares the handle's `owner_id()` against its
      own `dbg_reservation_owner_id` via a release-build `assert_eq!` (NOT
      `debug_assert!` — a check compiled out in `--release` would defeat the
      point) before ever touching `self`'s pool/directory/`SegmentTable`
      state.
   2. **`unsafe fn` again**, with a `# Safety` doc contract, as defence-in-
      depth for what the owner-id check cannot see (the segment must still
      be live/unreleased) — matching the established pattern
      (`HeapCore::dbg_dealloc_own_thread_with_base`). Both `dbg_decomp_release`
      entries (`AllocCore`'s and `HeapCore`'s delegation) moved back into
      `tests/dbg_hook_safety_tripwire.rs`'s `UNSAFE_HOOKS`.
   A genuine correctness bug was found and fixed WHILE building this fix's
   own counterfactual test: asserting BEFORE consuming the handle (`handle`
   still a live local with its leak-detecting `Drop` impl armed) made the
   `assert_eq!` panic unwind straight into `ReservedSmallSegment::drop`'s
   own `debug_assert!(false, "dropped without going through release")` — a
   panic-during-panic, which Rust aborts on unconditionally, observed as a
   raw `STATUS_STACK_BUFFER_OVERRUN` process abort on Windows instead of a
   clean single panic. Fixed by reading `owner_id` and calling `into_base()`
   (disarming `Drop`) BEFORE the `assert_eq!`; the mismatch path still never
   reaches `self.release_or_pool_empty_segment` (the assert fires first),
   so this ordering fix only prevents the separate double-panic-abort
   failure mode, it does not weaken the rejection itself. New counterfactual
   test `tests/r31_15_reserved_small_segment_cross_core_release.rs`: a
   genuine two-`AllocCore` cross-core release (`#[should_panic(expected =
   "handle was reserved by a DIFFERENT AllocCore")]`), a same-core positive
   control (proving the guard doesn't false-positive on the legitimate
   path), and a source-text check that `HeapCore::dbg_decomp_release` stays
   a pure 1-line forward to `AllocCore::dbg_decomp_release` (justifying why
   no separate registry-bound two-heap counterfactual was built). Full
   verification: `cargo test --features "alloc-decommit bench-internals"`
   green for the four directly-touched test files; `cargo test --features
   production` green (`no_stale_doc_references.rs` initially caught the two
   expected doc-drift failures below, both fixed in the same commit);
   `cargo build --features production` / `--all-features` clean; `cargo
   clippy --tests -- -D warnings` / `--features experimental` /
   `--all-features` (the three real CI matrix entries) all clean; `cargo fmt
   --check` clean. Doc-drift fixed as a side effect: test-file count
   230→231 (`docs/ARCHITECTURE.md`), README tier-2 `#[allow(unsafe_code)]`
   site count 68→70 (exactly +2: the new `dbg_decomp_release` item-level
   allow at both the `AllocCore` and `HeapCore` layers). No `production`
   feature composition changed — the new `dbg_reservation_owner_id` field
   and its counter are `bench-internals`-gated, costing nothing in any
   `production`/default build (this crate's `AllocCore` lives inline in
   every `HeapSlot`, `MAX_HEAPS = 4096`, so an always-present field would
   have multiplied its size by 4096 regardless of whether any caller ever
   reaches the decomposition hooks — gating avoids that cost entirely).

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

   **[FIXED, R31-4/task #467, commit `ca9aba9`, 2026-07-31.]**
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
