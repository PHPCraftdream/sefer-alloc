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
   not just forgotten again. If the entry carries a current-state card (see rule 3),
   update its Status / Current-number to reflect the closure as part of the move.
3. **When a new gate report flags an open item:** add it here in the same commit
   that lands the report (or the report's own follow-up commit), with a
   `file:line`/section pointer back to the report's own "Open items" / §6 /
   "Follow-up" section. A flag that lives only inside a single report's prose is
   exactly the failure mode this index exists to prevent. **Current-state card
   (added R24-9):** every item carries a compact block right after its title —
   Status / Current number-or-verdict / Next trigger / Evidence — that states the
   LATEST correct headline first, before the historical narrative. Fill it in when
   adding an item, and update the Current-number / Status whenever a later round
   supersedes the headline (the card is the first thing a reader sees; the
   historical narrative below it is preserved unchanged, per the append-don't-
   rewrite convention).


**Archive (added R29-6, task #437).** This file keeps only CURRENT-STATE
material — the tier structure, each item's current-state card, and a
`Full history:` pointer. The long dated round-by-round closure narratives
that used to sit inline under each item (and the full "Recently resolved"
write-ups) now live in the sibling file `docs/perf/OPEN_ITEMS_ARCHIVE.md`,
organized by the same `<tier-letter><item-number>` anchors used here — e.g.
item 1 in the `[A]` tier archives to `OPEN_ITEMS_ARCHIVE.md` § `A1`. No text
was deleted in that split (R29-6); every word of history survives there,
just relocated out of the round-start read path. Item numbers and tier
letters, and every current-state card, are unchanged by the split.

**Scope.** This index covers `docs/perf/*.md` only (gate reports + perf design
docs). It is NOT a general issue tracker — code `TODO`/`FIXME` comments, roadmap
wishes, and `docs/reviews/*` plan items are out of scope unless a perf gate
report explicitly flags them as a follow-up. For the analogous durable index
covering correctness bugs, flaky tests, and CI-coverage gaps (the class of
item this file's own scope deliberately excludes), see the sibling document
`docs/CORRECTNESS_OPEN_ITEMS.md` (added R22-3, task #354, after two
independent reviews found R19-1's flaky-test and clippy-dead-code follow-ups
tracked nowhere durable).

**Named in-scope source (added R29-11, task #442):** `docs/perf/IAI_BASELINE.md`
is explicitly an in-scope source document for this index — not merely an
instance of the abstract `docs/perf/*.md` glob above. It is the densest
`honest-reject` source in the perf-doc corpus: eight trigger-bearing reject
sections (X4, X5, X6, G1, T10, R1, R3, R5-R2b). The scope rule was technically
broad enough to cover it from R14 on, but no round's start ritual ever surfaced
the file because it was never *named* here — so those eight documented rejects
went un-indexed across every round from R3's first appearance (2026-07-13) until
R29-11 migrated the seven still-unindexed ones below (items 18–24; R3 itself was
partially closed the prior task by R29-10's item 17). Naming the file explicitly
closes that loophole: the round-start "read this file end-to-end" pass must
treat `IAI_BASELINE.md`'s `honest-reject` sections as first-class open-item
candidates, not background prose. The structural regression guard
`tests/no_stale_doc_references.rs::honest_reject_sections_are_indexed` enforces
this going forward — any new `## <TOKEN> honest-reject` heading in a
`docs/perf/*.md` file whose `TOKEN` does not appear in this index fails the
build.

**Tier key.** **[A]** active / high-value — a real next step a round should
consider taking. **[D]** deferred design — a complete CONDITIONAL-GO design
exists; implement only if its trigger/victim materializes. **[L]** low-priority
— an "honest reject with a revisit trigger"; not recommended now but documented
for completeness.

---

## Open items

### [A] Active / high-value

1. **`contains_base`'s share of a real free's `Ir` — measured MATERIAL
   (18.6%).**

   > **Current state**
   > - **Status:** `flush_class` isolation measured (R28-1) — the "Next trigger" question below is ANSWERED; region judged likely exhausted for further micro-optimization at the per-block-cost scope (no 5th attempt opened).
   > - **Current number/verdict:** `contains_base`-only share of a real free's `Ir` = **8.8% (523/5,920)**, NOT the original 18.6% (R23-1). The item was then reframed: the routing prefix is NOT the free path's dominant cost — the magazine-overflow mechanic is. Bitmap-clear coalescing was tried twice (R24-3, R24-4) → both NO-GO; STAGE_CAP 512→64 is a GO (−4,065 Ir/call, R24-8); FLUSH_N sweep NO-GO (R25-3); STAGE_CAP=64 boundary re-confirmed clean N=16→1024 (R25-7); lazy `Option<[..]>` staging array NO-GO — crossover at N=17, the 4th consecutive NO-GO in this region (R26-7). **`flush_class(8 blocks)`'s own standalone Ir is now measured (R28-1, task #430): 449 Ir (56.1 Ir/block) — 77.3% of one overflow event's 581 Ir total, 90.3% of R24-2's ~487 Ir fused remainder estimate (reconciles to within 2.1%).**
   > - **Next trigger:** ANSWERED, not open. R28-1 isolated `flush_class` (the overflow's larger untried lever) and judged the region likely exhausted for further per-block-cost micro-optimization at this scope (see `R28_1_FLUSH_CLASS_ISOLATION_GATE.md` §5 for the full reasoning) — `flush_run`'s per-block work is already minimal/mostly-necessary (2 cheap guards + 1 M2 correctness guard + 1 freelist write + 1 bitmap write, metadata already hoisted per-run), and the compaction+push residual is now measured small (~48 Ir), so there is no hidden larger target left to chase in this immediate function family. Five consecutive NO-GO-or-exhausted findings now cover this region (R24-3/R24-4/R25-3/R26-7/R28-1). If a future round revisits magazine-overflow cost, the more promising angle (not explored by R28-1) is reducing HOW OFTEN overflow fires (workload-shape/`FLUSH_N`/`TCACHE_CAP` — already NO-GO'd once in R25-3) or a structural redesign of the fixed bitmap-clear+flush+compact+push sequence, not another per-block `flush_class` tuning attempt. Separately, Tier-2-hash-probe-heavy workloads might show `contains_base` > 8.8% (open, not a proven floor).
   > - **Evidence:** `R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE.md` §7 (8.8%); `R23_3_HOT_PATH_ATTRIBUTION_GATE.md`; `R24_2_FREE_BY_MAGAZINE_STATE_GATE.md`; `R24_5_COLD_ALLOC_FREE_SPLIT_GATE.md`; `R24_8_DEALLOC_BATCH_INTERNALS_GATE.md`; `R26_7_LAZY_STAGE_ARRAY_GATE.md` (4th NO-GO; isolated zero-init = ~54 Ir, not ~581); `R28_1_FLUSH_CLASS_ISOLATION_GATE.md` (flush_class isolated at 449 Ir/8 blocks; region judged exhausted).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `A1`.

13. **R24-11 — `bench_global_alloc_churn_with_teardown`@1024B residual
    re-measured post-Mechanism-2: verdict (i) pool-cap-exceeded.**

   > **Current state**
   > - **Status:** latency/decommit axis of R25-5 CONFIRMED (cap 4→8 eliminates the decommit cliff, self-verified via `AllocCore::dbg_pool_cap()`, re-confirmed through the real `#[global_allocator]` in R26-3); RSS/commit axis of R25-5 REMEASURED under subprocess-per-arm isolation (R26-1, task #410) — the R25-5 "cap 4→8 wins on RSS too" claim does NOT reproduce (under isolation all four caps produce statistically identical PEAK-live-set RSS), refuting R25-5's "cap=8 is 34% cheaper on RSS at 8T" finding as an artifact of sequential single-process slot reuse. BUT that peak-live-set flatness does NOT prove "no cap-specific RSS cost": R26-1's RSS probe ran at the LOWER-pressure `RSS_BATCH_SIZE=50` (≈12.5 MiB logical prefill, fits inside the current 4-segment/16 MiB retention region) and never recorded `dbg_pooled_count`/pool-occupancy high-water or decommit counters — i.e. victim activation (did cap 4 saturate, did cap 8 retain a 5th segment) was never proven at this probe's batch size. R26-3's OWN committed raw log (`docs/perf/_raw_r26_3_production_teardown_ab.log`, `rss_after_kib=`) shows cap8 arms deterministically retaining ~4,100 KiB more (~one 4 MiB segment) than cap4 arms AFTER teardown at the pressure-producing batch-120 workload — a REAL retention cost, now QUANTIFIED by R27-3 (task #421): ~+8 MiB/heap post-teardown (~2 segments), proven with victim activation (cap-4 saturates with decommit_delta>0; cap-8 retains 6 pooled segments high-water vs cap-4's 4, decommit_delta=0), scaling linearly to ~+255 MiB at 32 heaps; ~+4 MiB/heap of it is pooled/drainable, ~+4 MiB committed-non-pooled, and it does NOT decay during idle (event-driven decay only). R25-6 / R26-9's closure (task #418) is therefore REOPENED (task #423 / R27-5): it rested on the now-falsified "no cap-specific RSS cost" premise. **R27-4 (task #422) CONFIRMED the latency win at the REAL paired byte cap (16/32 MiB, not the 256 MiB ceiling) through the real `#[global_allocator]`** (cap8 ~22% faster, t=8.114 ≫ crit 2.101, sign 19/20, decommit 9→0 deterministic) — so BOTH halves of the paired-default decision are now measured at the REAL config. No production change.
   > - **Current number/verdict:** latency axis — GO-CANDIDATE for `pool_segments=8` STANDS (cap 4→8: 20→0 decommits/run, self-verified; re-confirmed cap8 ~16% faster through the real `#[global_allocator]` in R26-3; R27-4 RE-CONFIRMED ~22% faster at the REAL paired byte cap (16/32 MiB, not 256 MiB) — t=8.114, sign 19/20, per-batch delta 2.68 ms vs R26-3's 2.63 ms, constant). PEAK-live-set RSS axis (R26-1, median of 3 reps, all 36 arms self-verified `verified_cap == pool_segments` AND `cfg_conflicts_delta == 0`) — flat across caps AT THIS PROBE'S LOWER-PRESSURE BATCH-50 SHAPE: cap=4 1T=13,368 KiB vs cap=8 1T=13,372 KiB (0.03% diff, within noise); cap=4 32T=423,448 vs cap=8 32T=423,444. BUT peak-live-set RSS is the wrong sole metric for a retention policy and this flatness is NOT a proof of zero retention cost (victim activation was never verified at batch 50 — see the R27-2 note below). POST-TEARDOWN RSS (R26-3's own raw log, batch-120 pressure workload): cap8 retains ~4,100 KiB more than cap4 (`rss_after_kib=34576` vs `30476`, deterministic across the A/B/B/A pairs) — a REAL retention cost, now QUANTIFIED (NOT RSS-neutral). The "wins on BOTH axes simultaneously, no tradeoff" conclusion is REFUTED for RSS; the corrected RSS headline is "flat at peak under R26-1's lower-pressure batch-50 shape (which never proved victim activation), but ~+8 MiB/heap post-teardown under the pressure-producing batch-120 workload (R27-3, task #421, victim-activation-PROVEN: cap8 retains 6 pooled segments high-water vs cap4's 4; scales linearly to ~+255 MiB at 32 heaps; does not decay during idle)."
   > - **Next trigger:** the reservation-only overflow tier alternative (task
   >   #429/R27-11) was evaluated and NOT opened — see item 15 in the `[D]`
   >   tier below for the full trigger evaluation (trigger 1 fires, trigger 2
   >   is unmeasured). The remaining open question for THIS item is unchanged:
   >   the DEFAULT-CHANGE decision is a **PAIRED** knob change `(pool_segments, pool_byte_cap) = (4, 16 MiB) → (8, 32 MiB)`, NOT the one-knob "promote `DEFAULT_POOL_SEGMENTS` 4→8" this entry previously stated — which is a literal NO-OP under the current byte cap (see the 2026-07-28 R27-1 note below for the `min()` mechanism). The paired change DOUBLES the documented maximum retained committed pool memory per materialised heap (16 MiB → 32 MiB; at 32 concurrent heaps that is up to 1 GiB), so it is a genuine RSS-vs-throughput trade, not the cost-free change the one-knob phrasing implied. The corrected evidence for the paired change is "eliminates 20 decommits/run, confirmed through the real global allocator," at a post-teardown retention cost now QUANTIFIED by R27-3 (task #421): ~+8 MiB/heap (~2 segments; ~+4 MiB of it pooled/drainable via `dbg_drain_small_pool`, ~+4 MiB committed-non-pooled), scaling linearly to ~+255 MiB at 32 heaps — a genuine RSS-vs-throughput trade, NOT "RSS-neutral cost." The proper retention gate (R27-3) HAS LANDED: subprocess isolation + R26-1's config self-verification, at the pressure-producing batch 120, recording peak-live AND post-teardown AND post-idle RSS/commit, final/max `dbg_pooled_count`, and decommit counters, with cap4 PROVEN to saturate (decommit_delta>0, 274/1226/1446 at 1/8/32T) and cap8 PROVEN to retain 6 pooled segments high-water (vs cap4's 4) with decommit_delta=0. The default-change decision (R27-4/#422) and the adaptive-design re-evaluation (R27-5/#423) is now DONE — see `R27_5_ADAPTIVE_POOL_BUDGET_DESIGN.md`: the design is sound but its headline benefit (bound aggregate RSS while granting hot heaps the latency win) is unproven under the measured uniform-pressure workloads (the global token budget is either never-binding = cap-8-for-all, or splits the win into a bimodal fleet), and its idle-shrink-back sub-problem is unsolved within the project's no-background-thread constraint (a once-grown heap stays grown until thread-exit); recommendation is Option 1 (keep 4/16 MiB default, document an 8/32 MiB throughput recipe), Option 3 deferred as CONDITIONAL-GO pending a measured uneven-pressure victim + a stage-1 counter calibration. The default-change decision itself (R27-4/#422) can proceed on this data — and as of R27-4 (task #422) BOTH halves are measured at the REAL config: latency (cap8 ~22% faster, decommit 9→0, through the real entry point at 16/32 MiB) + retention (~+8 MiB/heap, R27-3). The default-change decision is a genuine RSS-vs-throughput trade, now fully quantified. Task #418 (R26-9, adaptive/process-wide pool budget design) closure is REOPENED — tracked as task #423 (R27-5), which redoes the adaptive-design evaluation once task #421 (R27-3)'s proper retention gate lands; see the R27-2 note below.
   > - **Evidence:** `R24_11_TEARDOWN_RESIDUAL_ROOTCAUSE.md` + `R24_11_TEARDOWN_RESIDUAL_ROOTCAUSE_summary.csv` + `docs/perf/_raw_r24_11_churn_with_teardown.log` / `_raw_r24_11_working_set_cycle.log` / `_raw_r24_11_churn_no_teardown_sefer.log`; **R25-5:** `R25_5_POOL_CAP_SWEEP_GATE.md` (§8 correction) + `R25_5_POOL_CAP_SWEEP_GATE_summary.csv` (trailing UNCONFIRMED_PENDING_R26_1 section) + `docs/perf/_raw_r25_5_pool_cap_sweep_probe.log`; **R26-1 (corrected peak-live RSS):** `R26_1_POOL_CAP_RSS_SUBPROCESS_GATE.md` (§9 methodological-gap correction) + `R26_1_POOL_CAP_RSS_SUBPROCESS_GATE_summary.csv` + `docs/perf/_raw_r26_1_pool_cap_rss_subprocess_probe.log`; **R26-3 (post-teardown RSS showing cap8 retains ~4,100 KiB more):** `docs/perf/_raw_r26_3_production_teardown_ab.log` (grep `rss_after_kib=`); **R26-2 / R27-2 correction provenance:** `docs/reviews/2026-07-28-r25-readonly-review.md` (R26-2 P0) + `docs/reviews/2026-07-28-r26-readonly-review.md` (R27-2 P0: "R26-1 does not prove cap 8 has no retention/RSS cost", project-improvement #6); **R27-3 (the proper retention gate, victim-activation-PROVEN):** `R27_3_POOL_RETENTION_GATE.md` + `R27_3_POOL_RETENTION_GATE_summary.csv` + `docs/perf/_raw_r27_3_pool_retention_gate.log`; **R27-4 (latency at the REAL byte cap through the real `#[global_allocator]`):** `R27_4_REAL_DEFAULT_AB_GATE.md` + `R27_4_REAL_DEFAULT_AB_GATE_summary.csv` + `docs/perf/_raw_r27_4_real_default_ab.log` + `docs/perf/paired_ab_runs/2026-07-28T23-55-35-517Z.json`.
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `A13`.

### [D] Deferred designs — implement only if trigger/victim materializes

2. **R17-10 — batched deferred reclaim (sub-design A + B).**

   > **Current state**
   > - **Status:** design-only, deferred.
   > - **Current number/verdict:** CONDITIONAL — sub-design A (batch the per-block decommit check) is independent and small; sub-design B (deferred cross-segment finalization) is conditional on a §5.1 stage-1 finding that a non-negligible fraction of `drain_dirty_segments` sweeps empty >1 segment.
   > - **Next trigger:** a future round chooses to implement sub-design A; sub-design B is gated on its §5.1 stage-1 finding (check BEFORE writing B's code).
   > - **Evidence:** `R17_10_BATCHED_DEFERRED_RECLAIM_DESIGN.md` §6 + §7 (lines 555–668).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `D2`.

3. **R11-7 page-run layer (R12-13 deferred).**

   > **Current state**
   > - **Status:** NO-GO now; kept as a reusable CONDITIONAL-GO starting point.
   > - **Current number/verdict:** NO-GO — no demonstrated victim exists today.
   > - **Next trigger:** a real workload allocating thousands of simultaneously-live 1.25–2.0 MiB (or larger uniform-size) objects that is `MAX_SEGMENTS`-bound or OS-reservation-syscall-bound (not RSS-bound — solved wherever `exact-span-large` is enabled).
   > - **Evidence:** `R12_13_PAGE_RUN_LAYER_DEFERRED.md` §4 (lines 188–237).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `D3`.

4. **R14-7 expandable / chained `SegmentTable`.**

   > **Current state**
   > - **Status:** design-only, deferred.
   > - **Current number/verdict:** design-only — implement only when one of three triggers fires.
   > - **Next trigger:** (1) a workload needing >`MAX_SEGMENTS`−1 (4095) simultaneously-live Large objects, OR (2) a `MAX_SEGMENTS` raise stops being "cheap" by §1's criteria, OR (3) page-run (item 3) is pursued (then re-evaluate this doc's tagged-`SegmentId` widening alongside it).
   > - **Evidence:** `R14_7_EXPANDABLE_SEGMENT_TABLE_DESIGN.md` §5 (lines 374–391).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `D4`.

5. **R10-4 run-origin oracle (class-align carve).**

   > **Current state**
   > - **Status:** design-only, CONDITIONAL GO.
   > - **Current number/verdict:** CONDITIONAL GO — sound with a real density gain (wide classes 2/1/1 → 3/2/2), but only worth it if `medium-classes-wide` is pursued (itself NO-GO'd for `production` on a large-realloc regression).
   > - **Next trigger:** `medium-classes-wide` re-opened.
   > - **Evidence:** `R10_4_RUN_ORIGIN_ORACLE_DESIGN.md` §0/§7/§8.
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `D5`.

6. **R22-16 — remap-instead-of-copy for the medium→Large promotion memcpy
   (MediumExtent sub-path).**

   > **Current state**
   > - **Status:** design-only; verdict corrected in R23-4 (the original whole-NO-GO framing is superseded). MediumExtent's Stage-1 workload-shape trigger MEASURED in R29-5 (task #436) — result: NO VICTIM, item stays deferred.
   > - **Current number/verdict:** **NO-GO** for whole-segment remap (base-address stability, unaffected) and for Windows (separate section-object blocker); **CONDITIONAL-GO** for Linux sub-region remap pending a correctness prototype (still unrun — unaffected by R29-5). The MediumExtent one-object-per-segment redesign's own CONDITIONAL-GO precondition is now MEASURED and does NOT clear the bar: R29-5's realistic Vec-growth workload (4,000 small + 40 large objects + 20,000 background allocs) found promotions fire on only **0.054%** of total allocation activity (33/60,722) and **0.82%** of growth objects (33/4,040) ever promote even once — RARE by every denominator tried; aggregate bytes moved by promotion across the whole workload is only ~4.1 MiB (33 events × a fixed 128 KiB each — every single event lands in the SAME histogram bucket, a structural property of pure-doubling growth, not workload noise). Per the design doc's own "No victim, no implementation" rule: **NO VICTIM under this realistic workload shape** — MediumExtent stays deferred, not opened.
   > - **Next trigger:** a Linux sub-region `mremap` correctness prototype (adds the FFI surface + the "never free-list-push a remap-vacated offset" discipline §10.3 identifies) remains open for the 4b sub-region-remap direction specifically. MediumExtent (4a) has NO live trigger after R29-5 — it would need a DIFFERENT, more promotion-heavy workload shape to be shown material (a hypothetical future finding, not something R29-5 asserts) before its own Stage-1 precondition could be reconsidered.
   > - **Evidence:** `R22_16_PROMOTION_REMAP_DESIGN.md` §10 (the R23-4 correction; original §0–§9 preserved verbatim). `R29_5_PROMOTION_FREQUENCY_GATE.md` (the Stage-1 workload-shape measurement, task #436).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `D6`.

14. **R25-8 — run-encoded free batch (arithmetic free list).**

    > **Current state**
    > - **Status:** design-only, deferred (CONDITIONAL-GO).
    > - **Current number/verdict:** CONDITIONAL-GO — design is sound (arithmetic runs are exactly how mimalloc structures its per-page free lists) but the mechanism's natural target (the magazine-overflow free path, R24-5's 3.60× free-only gap, 61.5% overflow-attributable) does NOT satisfy its own contiguity precondition: the magazine is a LIFO stack in FREE-order, and `slots[0..FLUSH_N]` (the flushed 8) are arbitrary offsets, NOT offset-contiguous, so a `(first_off, count, stride)` run-descriptor cannot encode them without an O(n) offset-sort that would exceed the per-block savings. The M2 double-free guard (`AllocBitmap::mark_free`) cannot be eliminated for run-blocks (the bitmap is the only per-block free-state record when no node is materialized), collapsing the free-side win to just `Node::write_next` — the same "cheap hot-cache-line store" class R24-4 measured as a +14 Ir/block net REGRESSION to coalesce. The one genuinely new lever (none of the three prior NO-GOs touched it) is the ALLOC side: a contiguous run lets `drain_freelist_batch` skip the per-block `read_next` dependent load (the chain walk that path's own doc calls "irreducible, no way to hoist it" — true for a scattered list, FALSE for an arithmetic run).
    > - **Next trigger:** BOTH required — (a) R23-7's `dealloc_batch` promoted from P2/no-downstream-consumer (the ONLY shape with guaranteed contiguity, by `carve_batch` construction); the magazine-overflow path is explicitly OUTSIDE the conditional. AND (b) a Stage-1 Ir measurement on THAT consumer confirming the alloc-side `read_next` chain is the dominant remaining cost (the §4 judge is the instrument; run it ONLY after (a) fires).
    > - **Evidence:** `R25_8_RUN_ENCODED_FREE_BATCH_DESIGN.md` (full design doc, §3.1 the contiguity finding, §3.3 the double-free-boundary finding, §5 the verdict + triggers, §4 the isolated-victim-judge spec). Triggered by R25-3's NO-GO (`R25_3_FLUSH_N_SWEEP_GATE.md`) — the third NO-GO in this exact free-path/magazine-overflow region (after R24-3, R24-4).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `D14`.

15. ~~**R27-11 — reservation-only overflow tier for the small-segment pool
    (evaluated, NOT opened).**~~ **MOVED to `[L]` (task R29-3, #434) — trigger 2
    measured, does NOT fire; honest reject with evidence.**

    > **Current state**
    > - **Status:** CLOSED (R29-3, task #434) — trigger 2 measured and does NOT
    >   fire. Moved from `[D]` to `[L]` below (item 16) with the measured
    >   1.0-1.3% avoidable share as the documented reason.
    > - **Current number/verdict:** (1+2+3) avoidable = ~24K ns = **1.0-1.3%**
    >   of the segment-lifecycle cycle (across 2 saved runs); (4+5) irreducible
    >   page-fault cost = **98.7-99.0%**. Additionally, `MADV_DONTNEED` decommit
    >   costs ~196-217K ns — MORE than the entire avoidable overhead, so the
    >   reservation-only design would be a NET LOSS on Linux.
    > - **Evidence:** `docs/perf/R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md`
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `D15`.

25. **R9-5 / R11-8 / R13-3 — `virgin-zero-skip` promotion decision is
    NEVER-DECIDED (the design's own Stage-3 promotion gate was never run).**

   > **Current state**
   > - **Status:** deferred promotion decision — feature is BUILT and
   >   CI-tested (`ci.yml` runs a `production virgin-zero-skip alloc-stats`
   >   step), but NOT in `production`; the promotion verdict the feature's own
   >   design docs queue was never produced.
   > - **Current number/verdict:** two independent CONDITIONAL-GO designs
   >   (`R9_5_VIRGIN_ZERO_SKIP_DESIGN.md` §11 Stage 3, lines 563–568;
   >   `R11_8_SMALL_VIRGIN_ZERO_SKIP_DESIGN.md` §8) — both
   >   GO-for-staged-implementation, NEITHER a promotion verdict. The only
   >   later measurement (`R13_3_VIRGIN_ZERO_SKIP_MAGAZINE_GATE.md`) is a
   >   was/now gate for the R13-3 *magazine fix* (NOT a promotion gate) and
   >   explicitly states *"No scenario shows a statistically significant
   >   difference at this sample size"* + that its single-threaded loop does
   >   not capture the cold-first-touch shape the feature targets. So the
   >   existing evidence shows no measured win AND no measured loss, on a
   >   workload shape the report itself says is wrong for the question — it
   >   does not support a GO or a NO-GO.
   > - **Next trigger:** run the design's own Stage-0/Stage-3 measurement — a
   >   `calloc`-shaped iai arm (`alloc_zeroed` on virgin pages vs recycled
   >   pages, ≥ 64 KiB where `memset` dominates) with paired-prefix
   >   subtraction, plus one wall-clock arm at a memset-dominated size. Cheap:
   >   the feature already exists; only the judge is missing (also
   >   independently requested by the R28 review §1.3,
   >   `docs/reviews/2026-07-29-oh-acceleration-code-project-review.md`). A
   >   green Stage 3 is the design docs' stated precondition for even
   >   *considering* promotion.
   > - **Evidence:** `R9_5_VIRGIN_ZERO_SKIP_DESIGN.md` §11 (Stage 3, lines
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `D25`.

26. **R12-9 — `small-segment-lazy-commit` (and the `alloc-lazy-commit` alias)
    deliberately left opt-in; deferral recorded only in R12-9 + a
    `Cargo.toml` comment, indexed nowhere until now.**

   > **Current state**
   > - **Status:** deferred promotion decision — deliberately NOT promoted;
   >   the decision EXISTS (unlike `virgin-zero-skip`'s never-run gate, item
   >   25) but was tracked only in `R12_9_PRIMORDIAL_LAZY_COMMIT.md` §6 and
   >   `Cargo.toml`, not in either index.
   > - **Current number/verdict:** R12-9 split the old `alloc-lazy-commit`
   >   into `primordial-lazy-commit` (GO, promoted into `production`) and
   >   `small-segment-lazy-commit` (explicitly NOT part of the
   >   recommendation). Stated reason: `small-segment-lazy-commit`'s
   >   decommit/recommit correctness surface on every pool eviction is
   >   "materially larger" than the primordial's one-time bootstrap
   >   reservation; R8-10 (task #223, `852828e`) measured
   >   empty→pool→reuse→refill cycles under this policy at 50–75× more
   >   commit/decommit syscalls before its admission-side fix. The fix is
   >   permanent, but the surface-size concern is qualitative, not a missing
   >   number — so this is a reasoned CONDITIONAL-keep-opt-in, not a
   >   never-decided gap.
   > - **Next trigger:** re-evaluate ONLY if a future round wants to quantify
   >   the net steady-state win/loss of `small-segment-lazy-commit` now that
   >   R8-10's admission fix is in place (the R8-10 regression was measured;
   >   the post-fix net effect on a long-lived small-segment churn workload
   >   was not).
   > - **Evidence:** `R12_9_PRIMORDIAL_LAZY_COMMIT.md` §6 (lines 231–238, the
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `D26`.

### [L] Low-priority — "honest reject" with a documented revisit trigger

7. **R14-5 §4 — dedicated timing gate for O(40) vs O(8) Large-cache scan on a
   narrow working-set-after-burst shape.**

   > **Current state**
   > - **Status:** deferred, low-priority.
   > - **Current number/verdict:** deferred — no number attached yet to the O(40) vs O(8) "cheap" claim for N=1/2/4.
   > - **Next trigger:** a future review wants a number for the narrow working-set-after-burst shape (R13-8 already measured the 24-distinct-size turnover shape).
   > - **Evidence:** `R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` (lines 240–248).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L7`.

8. **R14-6 §1.1 — compounding reserved-capacity growth factor (beyond 4×).**

    > **Current state**
    > - **Status:** deferred, low-priority.
    > - **Current number/verdict:** deferred — the 4× reserved-capacity growth factor's real numbers are still enough.
    > - **Next trigger:** 4×'s real numbers ever stop being enough (would need new per-segment chain-identity state or a threaded hint through the shared `alloc_large_slow` path).
    > - **Evidence:** `R14_6_ADAPTIVE_RESERVED_CAPACITY_GATE.md` (lines 89–95).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L8`.

9. **R15-1 §7 — nonempty-summary-word optimisation for `drain_dirty_segments`.**

    > **Current state**
    > - **Status:** honest reject — NOT recommended now.
    > - **Current number/verdict:** the ceiling is below this task's own noise floor; not worth it.
    > - **Next trigger:** revisit ONLY if `MAX_SEGMENTS` is raised again by a large factor (toward item 4's expandable table) OR a much-higher producer-class fan-in than N=8 becomes a real target.
    > - **Evidence:** `R15_1_MAX_SEGMENTS_DRAIN_SCAN_COST.md` §7 (lines 519–555).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L9`.

10. ~~**R9-9 — warm-batch-on-`SeferAlloc`-heap arm.**~~ **DONE (task R10-7,
    2026-07-21, commit `9611a56`).**

    > **Current state**
    > - **Status:** DONE — resolved by R10-7; deliberately left struck-through in place (not moved to "Recently resolved") so the original ask stays visible next to its closure.
    > - **Current number/verdict:** resolved — the fourth warm-batch arm R9-9 asked for (`batch_core_warm`/`scalar_core_warm` + `batch_tcache`) was built and measured against the real warm scalar path.
    > - **Next trigger:** none (closed).
    > - **Evidence:** `R10_7_BATCH_WARM_ARM.md` (closure); `R9_9_BATCH_BENCH_FOLLOWUP.md` (original ask).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L10`.

11. **R11-3 — joint threshold×pad-target sweep.**

   > **Current state**
   > - **Status:** low-priority, conditional.
   > - **Current number/verdict:** only relevant if `medium-classes-wide` promotion is re-opened.
   > - **Next trigger:** `medium-classes-wide` promotion re-opened.
   > - **Evidence:** `R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md` (lines 483–485).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L11`.

12. **R22-6 — sub-16 KiB geometric-ladder OPT-H probe (optional, ~1 hour).**

    > **Current state**
    > - **Status:** low-priority, optional curiosity probe (~1 hour if ever revisited).
    > - **Current number/verdict:** optional, NOT a next step a round should plan around — the sub-16 KiB tail is already cheapest (OPT-G in-place Large-grow is ~40× faster than mimalloc), so marginal payoff is small even at a favorable hit rate.
    > - **Next trigger:** none named (explicitly low-value, low-cost); a Vec-push-shaped 16 B→16 KiB harness would be the probe if ever run.
    > - **Evidence:** `R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §5.3 + this file's item-1 closure entry (the LCM argument distinguishing the two ladders).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L12`.

13. **R25-3 — `FLUSH_N` sweep (4/8/12/16) at fixed `TCACHE_CAP`=16.**

    > **Current state**
    > - **Status:** NO-GO, fully explored — all 4 swept values measured against all 5 required gates; none beats the current baseline (`FLUSH_N=8`).
    > - **Current number/verdict:** `FLUSH_N=16` shows the only gate-1 (bulk-free Ir) win (−1.5% at N=1024) but triggers the kill condition on gate 3 (oscillating live-set): 2.42× Ir regression, 20× refill-event-count regression (1→20 refills per 20 rounds), independently confirmed via both an Ir judge and a native tcache-hit-rate counter. `FLUSH_N=4`/`FLUSH_N=12` show no gate-1 win at all (+14.4%/+0.7%) with gate 2/3 also flat-or-worse.
    > - **Next trigger:** none named as promising — a genuinely different mechanism (not a half-flush-RATIO tuning) would be needed; R25-8's conditional run-encoded free-batch design study is the only currently-planned follow-up touching this region, and it is a different mechanism, not a FLUSH_N retune. If `FLUSH_N=16` (or any `FLUSH_N == TCACHE_CAP`) is ever revisited for any reason, first fix the independent `virgin_mask >>= FLUSH_N` compile-time overflow this task found at that exact boundary (release-profile-only; `cargo check` in dev profile does NOT catch it) — see the report §6.
    > - **Evidence:** `R25_3_FLUSH_N_SWEEP_GATE.md` (full report); `R25_3_FLUSH_N_SWEEP_GATE_summary.csv`.
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L13`.

16. **R27-11 — reservation-only overflow tier (MOVED here from `[D]` item 15;
    R29-3/task #434).**

    > **Current state**
    > - **Status:** honest reject — NOT recommended; trigger 2 measured and does NOT fire.
    > - **Current number/verdict:** (1+2+3) avoidable = ~24K ns = **1.0-1.3%** of
    >   the decommit→reserve segment-lifecycle cycle (across 2 saved runs); (4+5)
    >   irreducible page-fault cost = **98.7-99.0%**. Additionally, `MADV_DONTNEED`
    >   decommit costs ~196-217K ns — MORE than the entire avoidable overhead — so
    >   the reservation-only design would be a NET LOSS on Linux (per-page PTE walk
    >   of 1,006 pages > bulk VMA teardown of `munmap`).
    > - **Next trigger:** revisit ONLY if (a) segment size shrinks dramatically
    >   (fewer pages → MADV_DONTNEED cheaper relative to munmap), or (b) the
    >   OS-backend changes to one where recommit is a real separate syscall
    >   (Windows `MEM_DECOMMIT`+`MEM_COMMIT`, where the VMA-teardown-vs-page-walk
    >   trade-off may differ).
    > - **Evidence:** `R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md` (the
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L16`.

17. **R29-10 — alloc-hit `clear_magazine` block's per-pop cost (R3's
    never-isolated honest-reject cost, finally measured).**

    > **Current state**
    > - **Status:** honest reject — measured & CLOSED; NOT recommended, no standalone follow-up.
    > - **Current number/verdict:** the alloc-hit RAD-5 E4 `clear_magazine` block (`segment_base_of_ptr` + `SegmentMeta::new` + `magazine_bitmap` + `clear_magazine`, runs on EVERY magazine hit under `production`) = **12.19 Ir/hit** (195 Ir / 16 hits; two independent `npm run iai` runs, byte-identical). Decomposition: **~9.03 Ir `segment_base_of_ptr`** (R23-1's isolated figure for the same function, reproduced byte-identical) + **~3.16 Ir** bitmap-RMW residual. That is **54.5% of a magazine hit** (the hit reproduced at R23-3's 22.4 Ir/op exactly).
    > - **Next trigger:** NONE (closed). R3's correctness NO-GO on *deferring* the clear stands (the 12.19 Ir is a fixed per-hit cost, not a tunable one), and the dominant sub-cost (`segment_base_of_ptr`, ~9 of the 12 Ir) overlaps item 1's R22-17 header-first-design open item — there is no NEW standalone lever specific to `clear_magazine`. The one theoretical lever (cache the segment base alongside the slot pointer in the tcache) is speculative, doubles magazine per-slot footprint, and per R26-7's Heisenberg lesson would need an in-context A/B on `small_churn_16b` — explicitly a SEPARATE task if ever pursued, NOT opened here.
    > - **Evidence:** `R29_10_ALLOC_HIT_CLEAR_MAGAZINE_ISOLATION_GATE.md` + `_summary.csv` + `_raw_r29_10_run1.log` / `_raw_r29_10_run2.log`; `IAI_BASELINE.md`'s R3 honest-reject (the origin — "no iai baseline was taken; there is nothing to measure", now corrected in place by a dated append-note).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L17`.

18. **X4 (2026-07-05) — two recycle experiments (`TCACHE_CAP` 16→32 and a 64-bit
    bloom signature gating the M2 scan), both rejected.**

    > **Current state**
    > - **Status:** honest reject — NOT recommended; both sub-experiments declined.
    > - **Current number/verdict:** **A — `TCACHE_CAP` 16→32: REJECT.** Every bench regressed, including the explicit target (recycle **+32,305** Ir; churn +22.3k; cold +25.3k; large +18.3k) — the bench shapes don't refill-miss enough to amortize a doubled cap (each refill/flush just got twice as large). Confirms the FASTBIN P6 sweep's "CAP=32+ materially worse". **B — 64-bit bloom signature gating the M2 in-magazine scan: REJECT (the won-front rule).** Recycle won big (−19,147 / −14,235; cold −8,733 / −6,997) but ALL THREE churn benches regressed ~+980 Ir — far past the ±10 hot-path kill threshold (on churn the just-popped block's signature bit is still set, so the gate never skips the scan and is pure overhead; churn is the won front, which the project does not trade).
    > - **Next trigger:** per B's own text — "If a future arc revisits this, the shape to try is a signature that is CLEARED on pop (pop knows the slot index; clearing exactly one bit is sound only with per-slot bits, not a shared bloom — i.e. a 16-bit occupancy mask keyed by slot, which is just the scan again)."
    > - **Evidence:** `docs/perf/IAI_BASELINE.md` "X4 honest-rejects (2026-07-05)" section (lines 219–244). Final tree after X4 = pristine `2a23878` (zero diff; nothing shipped).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L18`.

19. **X6 (2026-07-05) — clz `class_for` vs the 16 KiB `SIZE2CLASS` LUT.**

    > **Current state**
    > - **Status:** honest reject — NOT recommended.
    > - **Current number/verdict:** REJECT. A clz-based `class_for` (14-byte `CLZ_BASE` per-pow2-bucket table + ≤6-step forward scan — the 49-class geometry has 1–5 irregular classes per log2 bucket, no closed form), proven bitwise-identical to the LUT over 8,280,074 (size, align) pairs, measured: churn Ir 0 delta (the compiler const-evals `class_for` for the benches' fixed sizes), `realloc_grow` (the one dynamically-sized path) **+658 Ir** (clz+scan costs more than one indexed load), and **Estimated Cycles regressed on 10/11 benches** (churn +72…+208; recycle +72/+140; multiseg +76; only cold_64b −64). RAM hits unchanged (±4), so the LUT's 16 KiB footprint never surfaced as misses; the scan's extra loads did.
    > - **Next trigger:** per the section's own text — "If a future arc revisits, the trigger should be a REAL-application cache profile (not microbenches) showing SIZE2CLASS lines contending." (The clz implementation and the exhaustive differential test are recoverable from the source section's description.)
    > - **Evidence:** `docs/perf/IAI_BASELINE.md` "X6 honest-reject (2026-07-05)" section (lines 246–268).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L19`.

20. **X5 (2026-07-05) — per-class segment-queue bitmap (cheapest variant).**

    > **Current state**
    > - **Status:** honest reject — NOT recommended *for the measured regime* (correctness-proven and recoverable; not a refutation of the idea).
    > - **Current number/verdict:** REJECT. The cheapest sound variant (a per-segment `u64` bitmap of non-empty classes, bit `c` set ⟺ `BinTable.head(c) != FREE_LIST_NULL`, maintained at every empty↔nonempty transition, consulted by `find_segment_with_free` instead of loading the BinTable head cache line) was implemented, correctness-proven by 8 dedicated regression tests (counterfactual-verified: disabling any one transition makes the invariant test FAIL), and measured: it regressed the designated judge (`multiseg_cold_256k` +273 Ir) AND the won front (the four churn benches **+9 Ir** each, just under the ±10 kill threshold; recycle +810; cold +400). Mechanism: at n=3 segments the maintenance RMW (load `free_classes`, OR/AND a mask, store) on every empty↔nonempty dealloc transition is a net cost, and the `free_classes` load sits in the SAME cache line as the header already read for `kind_at` — the "avoid a BinTable-line load" premise does not hold here (no extra cache line to avoid).
    > - **Next trigger:** per the section's own text — "a future arc that adds a ≥64-segment bench (or profiles a real application) may flip the verdict. The shape to revisit is the FULL per-class queue (skip non-matching segments entirely, not just a per-segment bit probe)." (The structural argument only materialises at n_segments ≫ 3, which no current bench models — `multiseg_cold_256k` spans only 3.)
    > - **Evidence:** `docs/perf/IAI_BASELINE.md` "X5 honest-reject (2026-07-05)" section (lines 270–365, full measurement table included there). Final tree after X5 = pristine `490974d` (zero diff; nothing shipped).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L20`.

21. **G1 (2026-07-10) — magazine double-free oracle fold into `AllocBitmap`.**

    > **Current state**
    > - **Status:** honest reject — NOT recommended; not implemented (zero diff for G1 specifically; task #50's other sub-parts landed independently).
    > - **Current number/verdict:** REJECT. Folding the in-magazine double-free scan into `AllocBitmap` requires *inverting* existing load-bearing optimizations at multiple call sites (not a free relabeling): `refill_class_bump_impl`'s freelist-drain leg + `refill_class_bump`'s bump-carve leg both call `mark_alloc` on a premise that becomes false once the destination can be the magazine instead of the user; `reclaim_offset_checked`'s cross-thread ring-drain path already runs `is_free(off)` PLUS a separate `is_in_magazine` O(count) scan specifically because today's bitmap is blind to magazine residency — folding residency into the bit would make `is_in_magazine` redundant, a real behavior change to the H1-adjacent cross-thread reclaim protocol. A single alloc can legitimately set up to 32 consecutive bits (1 requested + `REFILL_BATCH`=31 refilled), which the simple "set on push, clear on pop" framing did not account for. Measured: the magazine-hit benches targeted show **exactly 0.0 Ir/op delta** (no code changed). M2 counterfactual tests confirmed non-vacuous (temporarily broke → went RED as expected).
    > - **Next trigger:** per the section's own text — "the shape to try is NOT a simple bit redefinition but a design that (a) audits and updates every `mark_alloc`/`mark_free` call site's semantics consistently (the four sites named, at minimum), and (b) resolves whether `is_in_magazine`'s separate scan in `reclaim_offset_checked` becomes provably redundant or must be kept for the cross-thread case specifically — that analysis was not completed here … and is the actual blocker, not a fundamental soundness objection to the idea."
    > - **Evidence:** `docs/perf/IAI_BASELINE.md` "G1 honest-reject (2026-07-10)" section (lines 530–591).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L21`.

22. **T10 (2026-07-12) — per-class "last found segment" hint for `find_segment_with_free` (NO-GO, reverted).**

    > **Current state**
    > - **Status:** NO-GO, honest reject — reverted (one orthogonal sub-finding KEPT; see below).
    > - **Current number/verdict:** NO-GO. A per-class `find_hint: [u16; SMALL_CLASS_COUNT]` "last found segment" hint (verified pre-check at scan top, written ONLY on a successful full scan — zero hot-path maintenance) failed the churn kill gate (±10 raw Ir, X4-B precedent): the `[u16; 49]` array init at `AllocCore::new` costs a constant **+44 Ir on every heap construction** (isolated cleanly by `large_alloc_free_cycle`'s raw delta — that bench touches no small class, so its entire +44 Ir IS the array init), and the four churn benches landed at **+46 raw Ir** (~5× the threshold). Cold/recycle landed flat (+0.1 Ir/op) — far below the −15…−25 Ir/op GO target; the O(n) scan at n=3 is 3 cache-hot iterations. Only the two multi-segment judges moved (`multiseg_cold_256k` −4.2 Ir/op, `seg_cycle_decommit_256k` −6.6 Ir/op) — the SAME figures X5/R1 reached.
    > - **Next trigger:** per the section's own text — "A future arc that adds a ≥64-segment bench (or profiles a real application with 100+ long-lived small segments) may flip this verdict; the correctness-proven hint shape is recoverable from this entry's description. The shape to revisit is the FULL per-class queue (skip non-matching segments entirely), since a per-class hint alone already loses to the bootstrap cost at n=3." NOTE (kept sub-finding): T10's other, lower-risk sub-finding (`class_for` align>16 jump-ahead walk over `SIZE2CLASS`, perf#9) is **KEPT** — orthogonal to this NO-GO, pure integer arithmetic, correctness-pinned by `tests/size_classes_slow_path_equivalence.rs`.
    > - **Evidence:** `docs/perf/IAI_BASELINE.md` "T10 honest-reject (2026-07-12)" section (lines 1088–1204, full measurement table included there).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L22`.

23. **R1 (2026-07-13) — per-segment availability hint for `find_segment_with_free` (NO-GO, clean revert).**

    > **Current state**
    > - **Status:** NO-GO, honest reject — clean revert (working tree byte-identical to pre-experiment).
    > - **Current number/verdict:** NO-GO — the **fourth independent attempt** at this scan (after X5's per-segment bitmap and T10's per-class hint array). A single verified pre-check hint (`find_hint_slot: u32`, init `u32::MAX` = none, written on successful full scan, zero hot-path maintenance, sound-by-construction false-positive-only failure mode) PASSED the churn kill-gate (±10 Ir) at **+3 raw Ir** (the best of all three attempts — better than T10's +46 and X5's +9), but MISSED the cold/recycle target by a wide margin (+0.0…+0.1 Ir/op vs the campaign's −15…−25 Ir/op GO target): those benches fit entirely in the primordial segment (n=1, a one-iteration scan), so no scan optimization of any shape can help them. Only the two multi-segment judges moved (`multiseg_cold_256k` −4.3 Ir/op, `seg_cycle_decommit_256k` −6.6 Ir/op) — the SAME −4.3/−6.6 T10 already reached and was rejected for.
    > - **Next trigger:** per the section's own text — "A future arc that adds a genuine ≥64-segment bench (or profiles a real long-lived-process workload with 100+ simultaneously-live small segments) is the prerequisite for re-opening R1/X5/T10 — not a new algorithmic attempt at the current bench scale. The correctness-proven hint shape here (verified pre-check, zero hot-path cost, sound-by-construction false-positive-only failure mode) is the recommended starting point if that day comes." The structural barrier (every current bench models ≤3 live segments) is now confirmed a fourth time (X5, T10, R1's design-time Tier-A analysis, and this measured result).
    > - **Evidence:** `docs/perf/IAI_BASELINE.md` "R1 honest-reject (2026-07-13)" section (lines 1285–1354, full measurement table included there).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L23`.

24. **R5-R2b (2026-07-14) — the wall-clock churn regression signal is NOT an algorithmic/Ir regression (honest reject of the regression hypothesis).**

    > **Current state**
    > - **Status:** honest reject of the "algorithmic regression" hypothesis — the planned IAI-based bisection is moot by construction (closed without a source change).
    > - **Current number/verdict:** R5-R2 (the parent finding, `docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md`) used a rigorous paired A/B wall-clock protocol (20 alternating process-level reps, paired t-stat 3.94–5.27, sign test 17–19/20) to confirm a REAL, non-noise ~14–29% wall-clock slowdown on `global_alloc_churn`/SeferAlloc between baseline `e6b9b3a` and then-`HEAD`. R5-R2b re-measured the SAME window with `npm run iai` (the project's designated deterministic judge) and found `Ir` got FASTER, not slower: `small_churn_16b`/`churn_256b` 42,880 → 34,036 (−8,844 / **−20.6%**), `churn_write_256b` −20.3%; `EstCycles` and RAM hits moved the same direction by a similar/larger margin (e.g. `churn_256b` RAM hits 4,870 → 781). `Ir` is deterministic (byte-identical back-to-back at the same commit), so there is no `Ir` regression in this window to bisect.
    > - **Next trigger:** **no revisit trigger for the closed hypothesis** (it was refuted, not deferred). The section explicitly calls the one adjacent open thread — a possible Windows-native effect invisible to Ir (real page-fault/`VirtualAlloc`/decommit costs, TLB behavior, ASLR/base-address-dependent cache conflicts, or a codegen divergence between the `x86_64-pc-windows-msvc` and WSL/Linux target triples, since R5-R2's wall-clock numbers came from a native Windows release build while `npm run iai` drives a Linux/Valgrind-simulated binary) — a "NEW investigation, not a continuation of R5-R2b's now-closed algorithmic-regression hypothesis", which would need Windows-native tooling (ETW / a Windows perf-counter harness) this project does not currently have wired up.
    > - **Evidence:** `docs/perf/IAI_BASELINE.md` "R5-R2b honest-reject (2026-07-14)" section (lines 1356–1430); parent `docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md` (the wall-clock finding this entry closes).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L24`.

27. **R29-13 — large-cache `headroom_bytes` (default 256 MiB/heap) idle-RSS
    floor measured for the first time; confirmed-by-design, no action taken.**

    > **Current state**
    > - **Status:** honest confirmation — design behaves exactly as documented; NOT a bug, NOT recommended for a default change (that question was not asked here).
    > - **Current number/verdict:** the shipped 256 MiB default headroom converges, under maximum FORCED decay pressure (`dbg_force_decay_tick` looped to a fixed point), to a **measured floor of ~238–241 MiB/heap retained** (12.4–12.5% of an 8×34 MiB / 288 MiB fill reclaimed, the rest permanently held) — **30x the small pool's proven ~8 MiB/heap** (R27-3). Under PURE IDLE (100 ms/1 s/2 s, zero allocation activity), the idle delta is **exactly 0 KiB in all 36 measured arms** (4 headroom values × 3 thread counts × 3 reps) — idle reclaims nothing at ANY headroom setting, not only at 256 MiB. The natural fill/teardown workload never drives even one real decay tick regardless of headroom (`maybe_decay_large_cache`'s first-call timer-priming rule means a tight teardown loop never lets the 1000 ms interval elapse mid-loop) — this is read from source, not inferred, and matches the doc's "does not decay below this level" claim precisely once forced convergence is used to actually observe the floor.
    > - **Next trigger:** none named as a next step for THIS finding (design confirmed, no discrepancy to chase). If a future round wants to weigh changing `DEFAULT_HEADROOM_BYTES`, the missing piece is a throughput/hit-rate A/B at a smaller headroom through the real `#[global_allocator]` (the large-cache analogue of R27-4) — NOT measured here; this task's scope was the retention-cost side only, mirroring R27-3's own scope boundary.
    > - **Evidence:** `docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md` + `_summary.csv` + `docs/perf/_raw_r29_13_large_cache_retention_gate.log`.
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L27`.

## Recently resolved (closure trail — do not re-list as open)

**Full write-ups moved to the archive (R29-6, task #437).** Each entry below
is a one-line pointer; the complete closure text (root cause, verification,
files changed) lives in `docs/perf/OPEN_ITEMS_ARCHIVE.md` §
"Recently resolved — full closure trail", in the same order as below.

- **R9-9 §5 — warm-batch-on-`SeferAlloc`-heap arm (this index's own item 10).** DONE (task R10-7, 2026-07-21, commit `9611a56`) — built the fourth warm-batch arm plus a realistic tcache-aware design; warm-batch beats warm-scalar by 1.3x-3.3x.
- **R22-readonly-review §4.6 — batch API real downstream consumer?** DONE — decision recorded, not measured further (task #376/R23-7, 2026-07-27); no in-tree production caller confirmed, falsifiability clause recorded.
- **R18-7 §3b — add a `mimalloc` comparison arm to `perf-gate.yml`/`perf_gate_iai.rs`.** Implemented by R22-15 (task #366), 2026-07-26; corrected by R23-2 (task #371), 2026-07-27 — direction flips on hot-churn (0.896, SeferAlloc cheaper) once the asymmetric bootstrap proxy is removed.
- **Product fate of `medium-classes` — should it ship, in any form?** Resolved by R22-18 (task #369), 2026-07-26 — decision recorded: (b) a named opt-in workload profile, not ship-in-production, not reject-and-remove.
- **R10-2 §5 #1 — in-place medium-class grow within a segment (OPT-H).** Closed by R22-6 (task #357), 2026-07-26, with a closed-form LCM arithmetic proof (NO-GO on the medium ladder specifically; does not foreclose the sub-16 KiB ladder, see `[L]` item 12).
- **R18-9 §9 — execute the §3 coordinated Large-policy matrix, cell C4.** Resolved by R20-2 (task #347), 2026-07-26 — NULL verdict; reserved-capacity headroom does not reduce the structural promotion-copy cost.
- **R18-7 §6 (docs fix) — correct the stale "pending the Linux Ir gate" wording.** Resolved by R20-1 (task #346), 2026-07-26 — added "(Resolved: ...)" notes in `CHANGELOG.md`/`docs/ALLOC_BENCH.md`.
- **R14-4 §6 item 2 — "re-run `scripts/r10_2_medium_gate.mjs` once R14-5 lands."** Resolved by R18-2 (task #331), 2026-07-26 — re-run confirms structural promotion-copy cost, not the leak R17-4 fixed.
- **R14-4 §6 item 1 — pad-target probe commit-cost discrepancy.** Resolved by R17-4 (task #321, commit `1b761f4`) — fixed a fastbin magazine dealloc-dispatch bug keyed on size instead of segment kind.
- **PERF_PLAN_beat_mimalloc_small_medium — "can we beat mimalloc at cold 16 B?"** Resolved by R18-7 (task #335) — the plan's named eurekas are exhausted; residual gap is honest per-block cost or unverifiable without the cross-allocator `Ir` number.
- **R10-6 — NUMA node-aware bit selection for the segment directory.** Resolved by R11-6 (task #234) — added `class_nonempty_by_node`; re-verified still-resolved by R25-9 (task #403), 2026-07-28, against an independent review's stale re-flag.
