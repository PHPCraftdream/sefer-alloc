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

1. **`contains_base`'s share of a real free's `Ir` — measured MATERIAL
   (18.6%).** R22-17 (task #368), 2026-07-26: `HeapCore::dealloc_routing`'s
   own-thread ownership probe (`SegmentTable::contains_base`, a two-tier
   4-entry-cache-then-hash-probe check) accounts for 18.6% of a real free's
   instruction count on a single-hot-segment churn workload (Tier-1 cache-hit
   case — a conservative/lower-bound estimate; a workload spanning more than
   `OWN_CACHE_SIZE` (4) concurrently-hot segments would show a LARGER share).
   Clears the "double-digit percentage" bar for a future design task. A
   header-first alternative (mirroring mimalloc's pointer-mask + one
   header-field read) is sketched but explicitly NOT implemented: it inverts
   `contains_base`'s current liveness-before-dereference ordering guarantee,
   and no way was found in this task's scope to make a bare header read safe
   against a foreign/use-after-decommit pointer without some other
   liveness proof that itself costs something — an open question for
   whoever designs this next, not an assumed-solved prerequisite. Evidence:
   `R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE.md` (full report, §4 for the
   design sketch + the soundness caveat).
   **2026-07-27 update (superseded by the 2026-07-27 DONE update directly
   below — kept for the historical trail, not the current number):** an
   independent read-only review
   (`docs/reviews/2026-07-26-r22-readonly-review.md` P1) found the "18.6%"
   figure is NOT an isolated `contains_base` measurement — the probe arm
   (`benches/perf_gate_iai.rs:232-239`) calls `dbg_segment_base_of_ptr` +
   `dbg_contains_base` as two separate non-inlined function calls plus
   `black_box`, so the reported share is really
   `(segment_base_of_ptr + contains_base + call-boundary overhead)`, an
   upper envelope, not a precise isolated cost; the "conservative lower
   bound" framing above is unproven either direction. Queued for
   correction: task #370 (R23-1) will add a `segment_base_of_ptr`-only
   arm to isolate the true `contains_base`-only share before this item's
   number is cited further.
   **2026-07-27 update — DONE (task #370, R23-1):** added
   `dealloc_segment_base_of_ptr_probe_only_16b`, an isolated arm calling ONLY
   `dbg_segment_base_of_ptr` (never `dbg_contains_base`), following the same
   shared-prefix-subtraction pattern as the existing three arms. Measured
   `npm run iai` (two independent runs, byte-identical Ir): raw Ir 7,581 →
   loop-only 578 (7,581 − 7,003 prefix) → 9.03 Ir/call. Decomposition:
   `contains_base_only_ir = composite_probe_loop_ir (1,101) −
   base_only_loop_ir (578) = 523`, i.e. **`contains_base`'s isolated share of
   a real free's `Ir` is 523 / 5,920 = 8.8%** (not 18.6% — the original
   figure was the sum of this 8.8% plus `segment_base_of_ptr`'s own 9.8%
   (578/5,920) plus zero separately-isolable residual, since 578 + 523 =
   1,101 exactly). **Cite 8.8% going forward, not 18.6%.** 8.8% still clears
   a MATERIAL (non-negligible) bar, so §4's header-first design-sketch
   discussion in the report remains valid — it was never contingent on the
   exact percentage. The "conservative lower bound" claim from the original
   report is retracted as unproven in either direction (per the review);
   whether Tier-2-hash-probe-heavy workloads would show MORE than 8.8% is an
   open question, not a proven floor. Full arithmetic, raw logs, and updated
   summary CSV: `R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE.md` §7 (the
   correction section) + `R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE_summary.csv`
   (R23-1 rows) + `docs/perf/_raw_r23_1_contains_base_isolation_full.log` /
   `_raw_r23_1_contains_base_isolation_rerun1.log`. Original 18.6% figure and
   its history preserved verbatim in the report per this file's own
   "do not delete, only correct the interpretation" convention.

### [D] Deferred designs — implement only if trigger/victim materializes

2. **R17-10 — batched deferred reclaim (sub-design A + B).** Design-only;
   proposes a future-round implementation + dual-axis wall-clock gate. Sub-design
   A (batch the per-block decommit check) is independent and small; sub-design B
   (deferred cross-segment finalization within one `drain_dirty_segments` sweep)
   is CONDITIONAL on a §5.1 stage-1 finding that a non-negligible fraction of
   sweeps empty >1 segment — check BEFORE writing B's code. Evidence:
   `R17_10_BATCHED_DEFERRED_RECLAIM_DESIGN.md` §6 + §7 (lines 555–668).
3. **R11-7 page-run layer (R12-13 deferred).** NO-GO now; the complete design
   remains a reusable CONDITIONAL-GO starting point IF a real workload
   materializes that allocates thousands of simultaneously-live 1.25–2.0 MiB (or
   larger uniform-size) objects and is measured `MAX_SEGMENTS`-bound or
   OS-reservation-syscall-bound (not RSS-bound — that is solved wherever
   `exact-span-large` is enabled). No demonstrated victim exists today.
   Evidence: `R12_13_PAGE_RUN_LAYER_DEFERRED.md` §4 (lines 188–237).
4. **R14-7 expandable / chained `SegmentTable`.** Design-only; implement ONLY
   when (1) a real workload needs >`MAX_SEGMENTS`−1 (4095) simultaneously-live
   Large objects, OR (2) a future `MAX_SEGMENTS` raise stops being "cheap" by
   §1's criteria, OR (3) page-run is pursued (then re-evaluate this doc's
   tagged-`SegmentId` widening alongside it — both touch the same header field).
   Evidence: `R14_7_EXPANDABLE_SEGMENT_TABLE_DESIGN.md` §5 (lines 374–391).
5. **R10-4 run-origin oracle (class-align carve).** DESIGN-ONLY, CONDITIONAL GO.
   Sound and real density gain (wide classes 2/1/1 → 3/2/2), but only worth it
   if `medium-classes-wide` is pursued — which is itself NO-GO'd for
   `production` (large realloc regression). Re-evaluate only if wide classes are
   re-opened. Evidence: `R10_4_RUN_ORIGIN_ORACLE_DESIGN.md` §0/§7/§8.
6. **R22-16 — remap-instead-of-copy for the medium→Large promotion memcpy
   (MediumExtent sub-path).** DESIGN-ONLY. **Status pending correction (task
   #373/R23-4):** the design doc as committed verdicts NO-GO for
   remap-in-place under the current shared-segment model, reasoning that a
   promotion-time "is anyone else sharing my pages" check is unsolved — an
   independent review (`docs/reviews/2026-07-26-r22-readonly-review.md` P1)
   found, and I personally re-verified against `carve_block`'s bump-
   monotonicity (`src/alloc_core/alloc_core_small.rs`) and the empty-segment-
   only reset (`decommit_empty_segment_impl`,
   `src/alloc_core/alloc_core_small_pool.rs`), that this specific blocker is
   based on a flawed premise: a live carved block's byte range is provably
   exclusive for its entire live lifetime, so no promotion-time check is
   actually needed for LINUX sub-region remap specifically. The
   base-address-stability blocker for WHOLE-segment remap is real and
   unaffected. Task #373 will correct the document's verdict (expected:
   NO-GO for whole-segment remap stands; Linux sub-region remap becomes
   CONDITIONAL-GO pending a correctness prototype) — do not cite this
   item's current NO-GO framing until that correction lands. Separately, the
   MediumExtent one-object-per-segment redesign (a different, larger-scope
   mechanism reusing the `SegmentKind::Large` pattern) remains its own
   CONDITIONAL-GO, gated on a still-unrun Stage-1 workload-shape measurement
   (what fraction of medium allocations actually cross the promotion
   threshold). Evidence: `R22_16_PROMOTION_REMAP_DESIGN.md` (full report,
   §4/§6 for the two candidate directions and verdict, pending correction).

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
12. **R22-6 — sub-16 KiB geometric-ladder OPT-H probe (optional, ~1 hour).**
    Not a variant of the now-closed medium-ladder item (see "Recently resolved"
    below) — a DIFFERENT ladder with much friendlier LCM ratios. The
    geometric run below the medium classes steps by ~1.25× per class (e.g.
    16 KiB → 32 KiB doubles, giving `lcm(16 KiB, 32 KiB)/32 KiB` = ratio 2, i.e.
    roughly 50% of tail-adjacent carve positions clear OPT-H's alignment
    precondition, vs. the medium ladder's 1-in-3-to-1-in-30 ratios). A
    Vec-push-shaped 16 B→16 KiB hot-buffer harness would plausibly show a
    20–50% Stage-1 hit rate — a real, currently-unmeasured data point. BUT:
    this is also the size range where the move-leg OPT-H would avoid is
    already cheapest — `realloc_grow_geometric` (64 B→4 MiB) is already
    reported as **~40× faster than `mimalloc`** (9.7 µs vs 383 µs;
    `README.md:244-245`/`:639`) via the existing OPT-G in-place Large-grow
    mechanism — so the marginal payoff of also fast-pathing the sub-16 KiB
    tail is small even at a favorable hit rate. Recorded as optional,
    low-priority, roughly-one-hour-if-ever-revisited — explicitly NOT the
    "one remaining unexplored variant of an active high-value lever" (that
    framing is retired along with item 1's closure below; this is a
    low-value, low-cost curiosity probe, not a next step a round should plan
    around). Evidence: this file's own item-1 closure entry below (the LCM
    argument that motivates distinguishing the two ladders) +
    `docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §5.3 (the single-hot-buffer
    victim pattern, which applies equally to the geometric ladder).

---

## Recently resolved (closure trail — do not re-list as open)

- **R18-7 §3b — add a `mimalloc` comparison arm to `perf-gate.yml` /
  `perf_gate_iai.rs`.** Implemented by **R22-15 (task #366)**, 2026-07-26
  (commit `ff48029`): 7 new mimalloc `#[library_benchmark]` fns added to
  `benches/perf_gate_iai.rs`, each mirroring an existing SeferAlloc bench
  byte-for-byte; `scripts/iai.mjs` taught an arm-aware bootstrap constant so
  the two allocators' different one-time init costs are never conflated.
  Measured, deterministic result (byte-identical `Ir` across 3 independent
  runs): SeferAlloc retires **1.3x-2.4x more instructions per op than
  mimalloc** on every matched workload (1.326x on hot churn, up to 2.430x on
  cold-carve/recycle) — a real, honestly-reported, unfavorable gap, settling
  the 10-round wall-clock argument this item names. (This entry itself was
  left un-closed when R22-15 landed — its own commit did not touch
  `OPEN_ITEMS.md` — and was only moved here by R22-17/task #368 while adding
  a sibling item; a stale-open item sitting one round past its actual
  resolution, caught in passing rather than by a dedicated check.) Evidence:
  `R22_15_MIMALLOC_IR_ARM_GATE.md` (full report).
  **2026-07-27 update:** an independent read-only review
  (`docs/reviews/2026-07-26-r22-readonly-review.md` P1) found this ratio's
  bootstrap-subtraction is asymmetric across the two allocators
  (`large_alloc_free_cycle` = 3308 Ir is ~41% of Sefer's raw churn Ir;
  `mimalloc_bootstrap_proxy` = 13050 Ir is ~78% of mimalloc's raw churn
  Ir), making the 1.326x-2.430x figures two small remainders after two
  differently-sized subtractions — a real statistical fragility
  `scripts/iai.mjs`'s own comment already flags as an approximation
  designed for within-allocator regression tracking, not cross-allocator
  ratios. The DIRECTION of the gap (SeferAlloc costs more `Ir` than
  mimalloc) is not in doubt, but the exact multiplier is provisional.
  Queued for correction: task #371 (R23-2) will add a warm `N`-vs-`2N`
  matched-workload gate that cancels the bootstrap constant
  algebraically instead of subtracting an external proxy, and publish
  both figures side by side.
  **2026-07-27 update — DONE (task #371, R23-2):** added six `_2n`/`_4n`
  sibling bench arms (`small_churn_16b_2n`, `mimalloc_small_churn_16b_2n`,
  `cold_alloc_free_256x16b_2n`/`_4n`, and their mimalloc mirrors) and
  computed `c = (Ir(2N) - Ir(N)) / N` for both allocators, cancelling the
  bootstrap constant algebraically with no proxy bench needed. Measured
  `npm run iai` (5 independent runs across two build stages,
  byte-identical `Ir` throughout). **Correcting the "direction is not in
  doubt" claim directly above: it WAS in doubt, and on the hot-churn
  workload the direction flips.** Hot-churn ratio: 1.326 -> **0.896**
  (SeferAlloc's genuine marginal cost, 69.0 Ir/op, is LOWER than
  mimalloc's, 77.0 Ir/op — SeferAlloc is marginally CHEAPER on this
  workload once the asymmetric proxy is removed, not 1.3x costlier).
  Cold-carve ratio: 2.430 -> **~2.0-2.08** (a 3-point N/2N/4N linearity
  check found both allocators' marginal cost drifts down slightly,
  3.7%-7.4%, as batch size grows — not the segment-crossing failure mode
  the correctness caveat worried about, but a genuine small non-linearity
  reported honestly; the ratio stays near 2.0 regardless of which adjacent
  point-pair is used — direction unchanged here, magnitude reduced ~18%
  from 2.430). Isolation-mechanism investigation confirmed each
  `#[library_benchmark]` fn runs in its own fresh process under Callgrind
  (already documented three times in `benches/perf_gate_iai.rs`'s own
  comments), so no separate "warm-up" pre-loop was needed — the existing
  single-timed-loop pattern already bakes in the full bootstrap cost per
  bench by construction. The four remaining R22-15 pairs (`churn_256b`,
  both `recycle_alloc_free_256x*b` pairs) were NOT re-measured with N/2N
  arms in this task (scoped to the two required pairs plus a linearity
  extension) and remain unverified under this corrected method. Evidence:
  `R23_2_WARM_N_2N_MIMALLOC_GATE.md` (full report) +
  `R23_2_WARM_N_2N_MIMALLOC_GATE_summary.csv` +
  `docs/perf/_raw_r23_2_warm_n_2n_gate.log` /
  `_raw_r23_2_warm_n_2n_gate_rerun1.log`. Original 1.326x-2.430x figures and
  their history preserved verbatim in `R22_15_MIMALLOC_IR_ARM_GATE.md` per
  this file's own "do not delete, only correct the interpretation"
  convention (see that report's §9).
- **Product fate of `medium-classes` — should it ship, in any form?**
  Resolved by **R22-18 (task #369)**, 2026-07-26: **decision recorded, not
  merely deferred again.** After 4 independent NULL/NO-GO attempts across 3
  rounds to clear the realloc axis (R18-2's ~1,180×/~380× re-run, R20-2's
  NULL on reserved-capacity headroom, R21-2's 0%/0% OPT-H hit rate, R22-6's
  closed-form LCM proof that the medium ladder cannot support OPT-H
  structurally, and R22-16's design-level NO-GO on OS-level remap), this
  record recommends **(b) — a named opt-in workload profile**, not (a) ship
  in `production` (rejected — the realloc regression is real, large, and
  unanimous across every measurement) and not (c) reject-and-remove
  (rejected — the alloc/free win, ~31×/~211×, is real, thrice-reproduced,
  and never contradicted; removal would delete a genuine win to solve a
  problem a documentation commitment solves just as durably). Full evidence
  re-verification, the three-option tradeoff analysis, a stub for the
  workload-profile doc, and an explicit "what would count as new evidence to
  reopen this" falsifiability clause are in
  `docs/perf/R22_18_MEDIUM_CLASSES_FATE_DECISION.md`. This closes the
  recurring re-measurement cost the R22 plan synthesis
  (`docs/reviews/2026-07-26-r22-plan.md` §2.3 item 4) flagged — a future
  round should cite this record rather than re-measuring, absent one of the
  three narrowly-defined reopen triggers §5 of that document names.
- **R10-2 §5 #1 — in-place medium-class grow within a segment (OPT-H).**
  Closed by **R22-6 (task #357)**, 2026-07-26, with a closed-form arithmetic
  proof, not a further measurement. R21-2 (task #351) had already found a 0%
  Stage-1 hit rate on both available harnesses and left the entry open with
  the framing that "a genuinely un-promoted, walks-the-Small-ladder harness
  is the one remaining unexplored variant" — that framing is retracted here:
  OPT-H's own two preconditions make even a friendlier, purpose-built harness
  structurally incapable of a useful hit rate on the medium ladder, so no
  third harness would change the conclusion.

  **The arithmetic.** OPT-H's preconditions
  (`docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §2.1) require, for a grow
  from `old_class` to `new_class` at offset `off`: precondition 3
  (tail-adjacency — `off` is a carve position, hence a multiple of
  `block_size(old_class)`) AND precondition 4 (new-class alignment — `off`
  must ALSO be a multiple of `block_size(new_class)`). Together these force
  `off` to be a multiple of `lcm(block_size(old_class), block_size(new_class))`.
  The six medium classes are 256 / 320 / 384 / 512 / 768 / 1024 KiB
  (`src/alloc_core/size_classes.rs:106-111`, the `EXTRAS` array under
  `medium-classes`). Working in units of 64 KiB (256K=4u, 320K=5u, 384K=6u,
  512K=8u, 768K=12u, 1024K=16u; segment = 4 MiB = 64u) so all ratios stay
  integers, the per-transition `lcm` and the count of legal carve offsets
  within one 4 MiB segment satisfying precondition 5
  (`off + block_size(new_class) <= SEGMENT`) are:

  | transition | lcm (64 KiB units → MiB) | legal offsets in one 4 MiB segment |
  |---|---|---|
  | 256K→320K | lcm(4,5)=20u = 1.25 MiB | 2 (at 1.25, 2.5 MiB) |
  | 320K→384K | lcm(5,6)=30u = 1.875 MiB | 1 (at 1.875 MiB) |
  | 384K→512K | lcm(6,8)=24u = 1.5 MiB | 2 (at 1.5, 3.0 MiB) |
  | 512K→768K | lcm(8,12)=24u = 1.5 MiB | 2 (at 1.5, 3.0 MiB) |
  | 768K→1M | lcm(12,16)=48u = 3.0 MiB | 1 (at 3.0 MiB) |

  (`off = 0` is excluded from every row: that offset lies inside the
  segment's own metadata region — header/page map/bin table — never a legal
  carve position for a payload block, so it is not a candidate tail-adjacent
  offset regardless of the lcm arithmetic; the 4 MiB upper bound in
  precondition 5 is exact, but the lower bound of "legal offsets" starts at
  one `lcm` step in, not at the segment's raw start.)

  i.e. only a small, fixed handful of carve-order positions per segment ever
  clear precondition 4 for a single hop, no matter how the harness is built —
  this bounds the BEST CASE (a single hot buffer, tail-adjacent on every
  grow, per §5.3 of the design doc), not just the adversarial N=16 case R21-2
  measured. Chaining is worse: `lcm` across all six classes =
  `lcm(4,5,6,8,12,16)` = 240 units = 15 MiB, far exceeding the 4 MiB segment,
  so no offset supports a full six-stage walk. Even three consecutive stages
  (256→320→384) fail: `lcm(4,5,6)` = 60 units = 3.75 MiB is the only
  candidate offset within a 4 MiB segment, but the resulting 384 KiB grow
  then needs `off + 384 KiB = 3.75 MiB + 0.375 MiB = 4.125 MiB > SEGMENT
  (4 MiB)` — precondition 5 fails for that chained case. **The medium ladder
  allows at most one cross-class hop per segment lifetime under OPT-H's real
  preconditions, and only from a small fixed set of carve positions** — this
  is what the size-class table's own factorization mathematically predicts,
  independent of which harness is used to observe it. R21-2's 0% measurement
  is consistent with (not merely "not yet contradicted by") this bound.

  This closes item 1 as NO-GO on geometric grounds for the medium ladder
  specifically. It does NOT foreclose OPT-H as a mechanism (§2's soundness
  argument is untouched) nor the sub-16 KiB geometric ladder, which has much
  friendlier ratios — see the new [L]-tier item 12 above for that narrower,
  optional, low-priority probe. Evidence: `R20_3_INPLACE_MEDIUM_GROW_DESIGN.md`
  §2.1 (preconditions); `src/alloc_core/size_classes.rs:106-111` (`EXTRAS`);
  the LCM table above (re-derived independently for this closure, not copied
  from R20-3, which only worked one transition as an example); R21-2's
  `R21_2_OPT_H_STAGE1_HIT_RATE.md` (the empirical 0% that this arithmetic now
  explains rather than merely reports).
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
  gate. (Resolved: this entry originally said "the one remaining lever for
  R10-2's gate is unchanged: item 1 above (in-place medium-class grow), still
  not designed" — stale on two counts by the time R22-6 touched this file:
  (a) content-stale, since R20-3 designed that lever as OPT-H two commits
  after this entry was written, and R21-2 subsequently measured it; (b)
  reference-stale, since R22-6's own renumbering of the `[A]` tier (task
  #357, 2026-07-26) shifted what "item 1" refers to, so leaving the original
  wording untouched would have made it silently point at the WRONG item
  (the mimalloc arm) instead of OPT-H. Fixed in place — per this project's
  established convention of appending a resolution note to historical
  point-in-time prose rather than rewriting it, R20-1/task #346's precedent
  — rather than left for a separate pass. See the "R10-2 §5 #1" closure
  entry earlier in this same "Recently resolved" trail for OPT-H's own
  closure detail.) Recorded in
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
