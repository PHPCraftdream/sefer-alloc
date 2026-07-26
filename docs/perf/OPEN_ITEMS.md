# Open items — cross-round tracking index

**Purpose.** A single durable, session-surviving checklist of every item that a
`docs/perf/*.md` gate report or design doc has flagged as *open / deferred /
follow-up / "revisit when X lands"*, so it can be checked at the START of each
new round before the task queue is formed. This file exists because R14-4's
explicitly-marked-open item ("re-run `scripts/r10_2_medium_gate.mjs` once R14-5
lands") hung unnoticed through three entire rounds (15, 16, 17) and was only
caught by an external review accidentally re-reading the right file (closed as
R18-2). The in-session `TaskList` does NOT fill this role — it does not survive
a session boundary, so a fresh session inherits no memory of prior rounds'
flagged-open items. This file does.

**Convention (mandatory — see CLAUDE.md "Phased delivery").**

1. **Round start:** before forming a new round's task queue, read this file
   end-to-end and decide, for each open item, whether this round closes it,
   defers it (with a one-line reason appended), or leaves it. An item must not
   be silently ignored — every round either moves it or explicitly re-defers it.
2. **When you close an item:** move its entry to §"Recently resolved" with the
   closing round + task number + one-line evidence (commit / doc that records
   the resolution). Do NOT delete the entry — the closure trail is itself the
   artifact that lets a future reviewer confirm an item was actually addressed,
   not just forgotten again.
3. **When a new gate report flags an open item:** add it here in the same commit
   that lands the report (or the report's own follow-up commit), with a
   `file:line`/section pointer back to the report's own "Open items" / §6 /
   "Follow-up" section. A flag that lives only inside a single report's prose is
   exactly the failure mode this index exists to prevent.

**Scope.** This index covers `docs/perf/*.md` only (gate reports + perf design
docs). It is NOT a general issue tracker — code `TODO`/`FIXME` comments, roadmap
wishes, and `docs/reviews/*` plan items are out of scope unless a perf gate
report explicitly flags them as a follow-up. For the analogous durable index
covering correctness bugs, flaky tests, and CI-coverage gaps (the class of
item this file's own scope deliberately excludes), see the sibling document
`docs/CORRECTNESS_OPEN_ITEMS.md` (added R22-3, task #354, after two
independent reviews found R19-1's flaky-test and clippy-dead-code follow-ups
tracked nowhere durable).

**Tier key.** **[A]** active / high-value — a real next step a round should
consider taking. **[D]** deferred design — a complete CONDITIONAL-GO design
exists; implement only if its trigger/victim materializes. **[L]** low-priority
— an "honest reject with a revisit trigger"; not recommended now but documented
for completeness.

---

## Open items

### [A] Active / high-value

1. **R10-2 §5 #1 — in-place medium-class grow within a segment.** The genuine
   blocker for clearing the `medium-classes` realloc kill-gate (still RED after
   R18-2's re-run: ~1,180× / ~380× slower than baseline's in-place Large
   realloc, and unmoved by R20-2's NULL result on destination-side reserved
   capacity). Reaffirmed as the one lever no existing-feature coordination
   addresses by `R18_9...md` §9, `R14_4...md` §7, and `R20_2...md` §6.4. A
   design existed (R20-3, task #348): `R20_3_INPLACE_MEDIUM_GROW_DESIGN.md`
   proposed "OPT-H" — a tail-of-segment bump-cursor in-place grow, sound and
   zero-new-metadata, CONDITIONAL-GO pending a Stage-1 hit-rate measurement
   on a new single-hot-buffer harness. **Stage 1 measured, R21-2 (task #351,
   2026-07-26): trigger NOT met.** Both harnesses show a **0% hit rate**:
   R10-2's existing N=16 harness (0/320 attempts, matching §5.2's
   prediction) AND — more decisively — R21-1's new single-hot-buffer harness
   built specifically to realize OPT-H's predicted victim pattern (0/20
   attempts). Root cause (traced in `R21_2_OPT_H_STAGE1_HIT_RATE.md` §4):
   the single-hot-buffer harness promotes to Large on its very first grow
   crossing every round (by construction — `REALLOC_BASE` already sits at
   `MEDIUM_REALLOC_PROMOTION_THRESHOLD`), so OPT-H's code path is reached
   only once per round, always at the same alignment-unfriendly carve
   position (`256 KiB % 320 KiB ≠ 0`). **Verdict: NO-GO for implementing
   OPT-H's real grow action on current evidence** — not a rejection of the
   mechanism's soundness, but neither available harness demonstrates the
   predicted victim workload materializing. A genuinely un-promoted,
   walks-the-Small-ladder-without-crossing-into-Large harness is the one
   remaining unexplored variant if this lever is revisited; not yet built,
   not scoped. Evidence: `R10_2_MEDIUM_CLASSES_NATIVE_GATE.md` §5 item 1
   (lines 343–347); `R18_9_ADAPTIVE_LARGE_POLICY_DESIGN.md` §8.1/§9 (lines
   613–623, 680–683); `R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §5/§6/§9;
   `R21_2_OPT_H_STAGE1_HIT_RATE.md` (full Stage-1 measurement + verdict).
2. **R18-7 §3b — add a `mimalloc` comparison arm to `perf-gate.yml` /
   `perf_gate_iai.rs`.** "The single biggest open question the plan left on the
   table": the cold-16 B gap has been a 10-round wall-clock argument because
   nobody has the deterministic cross-allocator `Ir` number that would settle
   whether the residual is honest page-map work or ceremony. **Feasibility
   check done — R20-4 (task #349), 2026-07-26: FEASIBLE**, and cheaper than
   the original framing assumed: mimalloc's C core is statically linked into
   the same binary Callgrind already instruments (no dynamic-link/JIT
   attribution gap), this repo's own established pattern
   (`benches/global_alloc.rs`) already calls `mimalloc::MiMalloc` directly via
   `GlobalAlloc` without installing it as `#[global_allocator]` — so a mimalloc
   arm can live in the SAME `perf_gate_iai.rs` file, no new bench binary
   required — and the CI C-toolchain question is already retired by
   `ci.yml`'s currently-green `clippy --all-features` job, which already
   compiles mimalloc's `cc`-built static lib on the identical `ubuntu-latest`
   image `perf-gate.yml` uses. Still NOT implemented — this item stays open
   for the actual arm (new `#[library_benchmark]` fns + an arm-aware
   bootstrap-constant fix in `scripts/iai.mjs`, see the report §8). Evidence:
   `R18_7_MIMALLOC_GAP_STATUS.md` §3b (lines 154–170) + §6 (lines 270–281);
   `R20_4_MIMALLOC_IR_ARM_FEASIBILITY.md` (full report, §0/§8 for the verdict
   and implementation sketch).

### [D] Deferred designs — implement only if trigger/victim materializes

3. **R17-10 — batched deferred reclaim (sub-design A + B).** Design-only;
   proposes a future-round implementation + dual-axis wall-clock gate. Sub-design
   A (batch the per-block decommit check) is independent and small; sub-design B
   (deferred cross-segment finalization within one `drain_dirty_segments` sweep)
   is CONDITIONAL on a §5.1 stage-1 finding that a non-negligible fraction of
   sweeps empty >1 segment — check BEFORE writing B's code. Evidence:
   `R17_10_BATCHED_DEFERRED_RECLAIM_DESIGN.md` §6 + §7 (lines 555–668).
4. **R11-7 page-run layer (R12-13 deferred).** NO-GO now; the complete design
   remains a reusable CONDITIONAL-GO starting point IF a real workload
   materializes that allocates thousands of simultaneously-live 1.25–2.0 MiB (or
   larger uniform-size) objects and is measured `MAX_SEGMENTS`-bound or
   OS-reservation-syscall-bound (not RSS-bound — that is solved wherever
   `exact-span-large` is enabled). No demonstrated victim exists today.
   Evidence: `R12_13_PAGE_RUN_LAYER_DEFERRED.md` §4 (lines 188–237).
5. **R14-7 expandable / chained `SegmentTable`.** Design-only; implement ONLY
   when (1) a real workload needs >`MAX_SEGMENTS`−1 (4095) simultaneously-live
   Large objects, OR (2) a future `MAX_SEGMENTS` raise stops being "cheap" by
   §1's criteria, OR (3) page-run is pursued (then re-evaluate this doc's
   tagged-`SegmentId` widening alongside it — both touch the same header field).
   Evidence: `R14_7_EXPANDABLE_SEGMENT_TABLE_DESIGN.md` §5 (lines 374–391).
6. **R10-4 run-origin oracle (class-align carve).** DESIGN-ONLY, CONDITIONAL GO.
   Sound and real density gain (wide classes 2/1/1 → 3/2/2), but only worth it
   if `medium-classes-wide` is pursued — which is itself NO-GO'd for
   `production` (large realloc regression). Re-evaluate only if wide classes are
   re-opened. Evidence: `R10_4_RUN_ORIGIN_ORACLE_DESIGN.md` §0/§7/§8.

### [L] Low-priority — "honest reject" with a documented revisit trigger

7. **R14-5 §4 — dedicated timing gate for O(40) vs O(8) Large-cache scan on a
   narrow working-set-after-burst shape.** Deferred "if a future review wants a
   number attached to the 'cheap' claim" specifically for N=1/2/4 (R13-8 already
   measured the 24-distinct-size turnover shape). Evidence:
   `R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` (lines 240–248).
8. **R14-6 §1.1 — compounding reserved-capacity growth factor (beyond 4×).**
    Deferred "if 4×'s real numbers ever stop being enough"; would need new
    per-segment chain-identity state or a threaded hint through the shared
    `alloc_large_slow` path. Evidence:
    `R14_6_ADAPTIVE_RESERVED_CAPACITY_GATE.md` (lines 89–95).
9. **R15-1 §7 — nonempty-summary-word optimisation for `drain_dirty_segments`.**
    Explicitly NOT recommended now (ceiling below this task's own noise floor).
    Revisit ONLY if `MAX_SEGMENTS` is raised again by a large factor (toward the
    R14-7 expandable table) OR a much-higher producer-class fan-in than N=8
    becomes a real target. Evidence: `R15_1_MAX_SEGMENTS_DRAIN_SCAN_COST.md` §7
    (lines 519–555).
10. **R9-9 — warm-batch-on-`SeferAlloc`-heap arm.** A fourth bench arm reusing
    the warm heap (no page faults) would give the fairest batch-vs-tcache
    comparison; "explicitly left for a future task if the 16 B / n=1024 signal
    warrants it." Evidence: `R9_9_BATCH_BENCH_FOLLOWUP.md` (lines 334–343).
11. **R11-3 — joint threshold×pad-target sweep.** The R11-3 probe fixed the
    pad-target at 2 MiB; "a joint threshold×pad-target sweep is future work."
    Only relevant if `medium-classes-wide` promotion is re-opened. Evidence:
    `R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md` (lines 483–485).

---

## Recently resolved (closure trail — do not re-list as open)

- **R18-9 §9 — execute the §3 coordinated Large-policy matrix, cell C4.**
  Resolved by **R20-2 (task #347)**, 2026-07-26: **NULL verdict.** Measured
  `production,medium-classes,exact-span-large,large-reserved-capacity` (C4)
  against the R10-2 realloc-heavy harness (W1), plus a direct, load-matched
  paired A/B/B/A comparison against C1 (`production,medium-classes` alone) —
  the decisive test showed no statistically resolvable difference
  (t=1.209 ≪ crit 2.101, sign test dead-even 10/20). Reserved-capacity
  headroom does NOT reduce the structural medium→Large promotion `memcpy`
  cost; this confirms R18-2 §10.7's mechanism-level prediction (the copy
  happens at promotion time, before the fresh Large segment's
  `reserved_capacity` is established, so headroom can only help a later grow,
  never the copy that created the promotion). A genuine but orthogonal
  finding: `exact-span-large` still roughly halves resident commit for this
  workload (~50.5 MiB → ~23.9 MiB) with an identical cache-hit-rate proxy —
  a memory win, not a realloc-speed win, and it does not move R10-2's kill
  gate. The one remaining lever for R10-2's gate is unchanged: item 1 above
  (in-place medium-class grow), still not designed. Recorded in
  `R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE.md` (full report) + companion
  `R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE_summary.csv`.
- **R18-7 §6 (docs fix) — correct the stale "pending the Linux Ir gate"
  wording.** Resolved by **R20-1 (task #346)**, 2026-07-26: added a short
  "(Resolved: ...)" note right after each stale "pending"-framed sentence in
  `CHANGELOG.md:4457`/`:4476` and `docs/ALLOC_BENCH.md:625`/`:688` (the header
  at `:688` covers the two bullet mentions at `:704`/`:708` in the same
  section) — original prose kept intact as the honest point-in-time record,
  per both files' historical/point-in-time nature. This entry's original
  citation (`CHANGELOG.md:4311`) was itself stale/wrong (that line is an
  unrelated M2-guard sentence); the line numbers above are re-verified against
  the file as it stands after this same fix.
- **R14-4 §6 item 2 — "re-run `scripts/r10_2_medium_gate.mjs` once R14-5 lands."**
  Resolved by **R18-2 (task #331)**, 2026-07-26: re-ran on `main` @ `912740f` for
  three feature compositions; `large-cache-extended` helps the realloc phase
  (~3.5×) but does NOT clear the 20% kill-gate (still ~380× slower). Root cause
  confirmed as structural promotion-copy cost, not the leak R17-4 fixed. Recorded
  in `R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` §6 item 2 + §7.1 + §10; companion
  summary `R18_2_MEDIUM_REALLOC_GATE_RERUN_summary.csv`.
- **R14-4 §6 item 1 — pad-target probe commit-cost discrepancy.** Resolved by
  **R17-4 (task #321, commit `1b761f4`)**: a fastbin magazine dealloc-dispatch
  bug keyed on `class_for(layout.size())` instead of segment `kind`; fixed in
  `src/registry/heap_core_free.rs`, pinned by
  `tests/r17_4_inplace_grown_large_dealloc_routes_by_kind.rs`.
- **PERF_PLAN_beat_mimalloc_small_medium — "can we beat mimalloc at cold 16 B?"**
  Resolved by **R18-7 (task #335)**: the plan's named eurekas are exhausted
  (every tautology already removed); the residual 16 B gap is either honest
  per-block page-map/fault work or unverifiable without the cross-allocator `Ir`
  number (→ open item #2 above). Recorded in
  `R18_7_MIMALLOC_GAP_STATUS.md` §1/§5.
- **R10-6 — NUMA node-aware bit selection for the segment directory.**
  Resolved by **R11-6 (task #234)**: added the node-indexed
  `class_nonempty_by_node` bitmap and wired the per-bucket scan so the
  directory-driven lookup is active under `numa-aware` too (local-first, then
  shared unknown bucket, then foreign real-node buckets ascending) — not
  "disabled, linear scan only" as R10-6 originally found. See
  `src/alloc_core/alloc_core_small.rs:554-571`'s own "R11-6 UPDATE" comment.
