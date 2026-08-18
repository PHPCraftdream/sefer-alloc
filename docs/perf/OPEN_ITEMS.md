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

1. **`contains_base`'s share of a real free's `Ir` — region judged likely
   exhausted (current share 8.8%, supersedes the original 18.6%).**

   > **Current state**
   > - **Status:** `flush_class` isolation measured (R28-1) — the "Next trigger" question below is ANSWERED; region judged likely exhausted for further micro-optimization at the per-block-cost scope (no 5th attempt opened).
   > - **Current number/verdict:** `contains_base`-only share of a real free's `Ir` = **8.8% (523/5,920)**, NOT the original 18.6% (R23-1). The item was then reframed: the routing prefix is NOT the free path's dominant cost — the magazine-overflow mechanic is. Bitmap-clear coalescing was tried twice (R24-3, R24-4) → both NO-GO; STAGE_CAP 512→64 is a GO (−4,065 Ir/call, R24-8); FLUSH_N sweep NO-GO (R25-3); STAGE_CAP=64 boundary re-confirmed clean N=16→1024 (R25-7); lazy `Option<[..]>` staging array NO-GO — crossover at N=17, the 4th consecutive NO-GO in this region (R26-7). **`flush_class(8 blocks)`'s own standalone Ir is now measured (R28-1, task #430): 449 Ir (56.1 Ir/block) — 77.3% of one overflow event's 581 Ir total, 90.3% of R24-2's ~487 Ir fused remainder estimate (reconciles to within 2.1%).**
   > - **Next trigger:** ANSWERED, not open. R28-1 isolated `flush_class` (the overflow's larger untried lever) and judged the region likely exhausted for further per-block-cost micro-optimization at this scope (see `R28_1_FLUSH_CLASS_ISOLATION_GATE.md` §5 for the full reasoning) — `flush_run`'s per-block work is already minimal/mostly-necessary (2 cheap guards + 1 M2 correctness guard + 1 freelist write + 1 bitmap write, metadata already hoisted per-run), and the compaction+push residual is now measured small (~48 Ir), so there is no hidden larger target left to chase in this immediate function family. Five consecutive NO-GO-or-exhausted findings now cover this region (R24-3/R24-4/R25-3/R26-7/R28-1). If a future round revisits magazine-overflow cost, the more promising angle (not explored by R28-1) is reducing HOW OFTEN overflow fires (workload-shape/`FLUSH_N`/`TCACHE_CAP` — already NO-GO'd once in R25-3) or a structural redesign of the fixed bitmap-clear+flush+compact+push sequence, not another per-block `flush_class` tuning attempt. ~~Separately, Tier-2-hash-probe-heavy workloads might show `contains_base` > 8.8% (open, not a proven floor).~~ **ANSWERED (2026-08-02, R32-10/task #501, F2):** this clause is now closed. A new `bench-internals`-gated Tier-1 hit/miss counter (`CONTAINS_BASE_TIER1_HITS`/`_MISSES`, `src/alloc_core/segment_table.rs`) plus a Large-heavy in-place-`realloc` rotation workload (`examples/r32_10_own_cache_tier1_thrash_gate.rs`) measured, DIRECTLY (not estimated): at the OLD `OWN_CACHE_SIZE=4`, a workload with as few as 4 concurrently-"hot" Large objects thrashes Tier-1 **completely** (0.00% hit rate, i.e. `contains_base`'s cost on that workload is dominated by the ~12.0 Ir Tier-2 path, not the ~8.2 Ir Tier-1 path this item's 8.8% figure is based on) — confirming the clause's suspicion was correct, not just theoretically possible. `OWN_CACHE_SIZE` was raised 4→16 in response (K≤8 now hits ~99.99%); K≥16 still thrashes at the new size too (the cache is direct-mapped, not associative, so K==cache-size alone does not guarantee distinct buckets). See `docs/perf/R32_10_OWN_CACHE_TIER1_THRASH_GATE.md` for the full sweep, both false-start designs this task ruled out en route, and the honest latency-null finding (the hit-rate win did not translate into a measurable wall-clock win on this harness).
   > - **Evidence:** `R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE.md` §7 (8.8%); `R23_3_HOT_PATH_ATTRIBUTION_GATE.md`; `R24_2_FREE_BY_MAGAZINE_STATE_GATE.md`; `R24_5_COLD_ALLOC_FREE_SPLIT_GATE.md`; `R24_8_DEALLOC_BATCH_INTERNALS_GATE.md`; `R26_7_LAZY_STAGE_ARRAY_GATE.md` (4th NO-GO; isolated zero-init = ~54 Ir, not ~581); `R28_1_FLUSH_CLASS_ISOLATION_GATE.md` (flush_class isolated at 449 Ir/8 blocks; region judged exhausted); `R32_10_OWN_CACHE_TIER1_THRASH_GATE.md` (the Tier-2-heavy clause, closed).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `A1`.

13. **R24-11 — `bench_global_alloc_churn_with_teardown`@1024B residual
    re-measured post-Mechanism-2: verdict (i) pool-cap-exceeded.**

   > **Current state**
   > - **Status:** both halves of the paired `pool_segments`/`pool_byte_cap` default-change decision (`(4, 16 MiB) → (8, 32 MiB)`) are now measured at the REAL config through the real `#[global_allocator]`; owner is R27-5/task #423's design (see Next trigger) pending a deployment-context decision. No production change made.
   > - **Current number/verdict:** latency — cap8 is **~22% faster** than cap4 at the real paired byte cap (R27-4/task #422: t=8.114 ≫ crit 2.101, sign 19/20, decommit calls 9→0 deterministic). Retention — cap8 genuinely retains **~+8 MiB/heap post-teardown** (~2 segments; ~4 MiB pooled/drainable + ~4 MiB committed-non-pooled), victim-activation-proven, scaling linearly to ~+255 MiB at 32 heaps, and does NOT decay during idle (R27-3/task #421; supersedes the earlier R26-1 "RSS-neutral" reading, which never proved cap-4 saturation at its lower-pressure probe batch size). This is a genuine, fully-quantified RSS-vs-throughput trade, not a free win.
   > - **Next trigger:** R27-5/task #423 designed (not implemented) an adaptive/process-wide pool budget as the alternative to a flat default change; verdict CONDITIONAL-GO-on-paper, recommendation is Option 1 — keep the 4/16 MiB default, document the 8/32 MiB throughput recipe (shipped as `Profile::Throughput` by R30-7/task #456; `Profile::Throughput` was later split by R31-9/task #473 into `SmallPoolPolicy::Throughput` + `LargeCachePolicy::Trimmed64MiB` — this recipe's small-pool half is now `SmallPoolPolicy::Throughput`, corrected 2026-07-31, R31-14a/task #483) — because the adaptive design's benefit is unproven under uniform-pressure workloads and its idle-shrink-back sub-problem is unsolved within the no-background-thread constraint. A reservation-only overflow-tier alternative was separately evaluated and NOT opened (item 15 in `[L]` below — trigger 2 measured, does not fire). Re-open ONLY if a future round has (a) a measured uneven-pressure victim workload, or (b) wants to revisit the flat 8/32 default-promotion decision itself.
   > - **Evidence:** `R24_11_TEARDOWN_RESIDUAL_ROOTCAUSE.md`; `R27_3_POOL_RETENTION_GATE.md` (retention, victim-activation-proven); `R27_4_REAL_DEFAULT_AB_GATE.md` (latency at the real paired config); `R27_5_ADAPTIVE_POOL_BUDGET_DESIGN.md` (the adaptive-design evaluation + Option-1 recommendation). Full dated round-by-round narrative (R25-5 → R26-1 → R26-2 → R26-3 → R27-1 → R27-2 → R27-3 → R27-4 → R27-5), including every intermediate correction and its raw-log citations, preserved in the archive below.
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `A13`.
   >
   > **Dated addition (2026-07-30, R31-2/task #465).** R30-7 (task #456)
   > already found the cap4-vs-cap8 latency win did not reproduce as a
   > statistically distinguishable effect on an 8-thread/4-size-mix
   > continuous-churn server-shaped workload, with the mechanism
   > (`decommit_calls_total`) bit-identical (40=40) between cap4 and cap8 —
   > leaving open whether cap8 simply wasn't large enough. R31-2 swept the
   > SAME workload shape through cap16 and cap32 and found the mechanism
   > delta STAYS ZERO all the way to cap 32 (`decommit_calls_total = 40` in
   > every one of 320 process launches across all four caps), at a tighter
   > ~4-5% minimum-detectable-effect than R30-7's own 18.8% — a clean
   > reject, not an underpowered null. **This does not change the Option-1
   > recommendation above** (keep the 4/16 MiB default, document the 8/32
   > MiB recipe — `SmallPoolPolicy::Throughput` as of R31-9/task #473's axis
   > split, `Profile::Throughput` at the time this note was written;
   > corrected 2026-07-31, R31-14a/task #483) — R27-4's original
   > single-threaded win is unaffected, and the recipe's `(8, 32 MiB)` value
   > is unchanged — but it materially narrows the recipe's known-applicable
   > scope: a caller
   > whose workload resembles R30-7/R31-2's 8-way-concurrent, mixed-size,
   > continuous-churn shape should not expect ANY tested small-pool cap
   > (8 through 32) to reduce decommit churn or improve latency, based on
   > this evidence. See `docs/perf/R31_2_POOL_CAP_THRESHOLD_SWEEP_GATE.md`
   > for the full sweep + a candidate (not proven) explanation.

### [D] Deferred designs — implement only if trigger/victim materializes

2. **R17-10 — batched deferred reclaim (sub-design A + B).**

   > **Current state**
   > - **Status:** design-only, deferred.
   > - **Current number/verdict:** CONDITIONAL — sub-design A (batch the per-block decommit check) is independent and small; sub-design B (deferred cross-segment finalization) is conditional on a §5.1 stage-1 finding that a non-negligible fraction of `drain_dirty_segments` sweeps empty >1 segment.
   > - **Next trigger:** a future round chooses to implement sub-design A; sub-design B is gated on its §5.1 stage-1 finding (check BEFORE writing B's code).
   > - **Evidence:** `R17_10_BATCHED_DEFERRED_RECLAIM_DESIGN.md` §6 + §7 (lines 555–668).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `D2`.

3. **R11-7 page-run layer (R12-13 deferred; R34-26 re-confirmed with in-place-grow angle).**

   > **Current state**
   > - **Status:** NO-GO now; kept as a reusable CONDITIONAL-GO starting point.
   > - **Current number/verdict:** NEED-MORE-DATA, lean NO-GO — no demonstrated victim exists today. R34-26 (task #545, 2026-08-05) re-confirmed R12-13's finding that no workload/bench/example exercises the 256 KiB–2 MiB range with realistic patterns (larson/mstress sizes are 16 B–8 KiB; R29-5 found promotion is 0.054% of allocations), AND added the in-place-adjacent-run-grow angle: the page-run layer with a buddy/run bitmap CAN be designed to support in-place grow (the LCM arithmetic that blocked OPT-H in a 4 MiB segment — R22-6's 15 MiB chain — is satisfiable in a 16 MiB arena), but building a prototype without a real consumer violates the project's measured-pain standard. The architectural thesis (the medium-classes failure was in the carve/grow architecture, not the absence of size classes) is confirmed by R10-2 §4.2 and R22-6's closed-form proof.
   > - **Next trigger:** (1) a real profiling trace showing material alloc AND realloc volume in 256 KiB–2 MiB (the R29-5 0.054% bar this must clear), OR (2) a `MAX_SEGMENTS`-bound workload (thousands of simultaneously-live 1.25–2.0 MiB objects), OR (3) a change to the carve/grow model altering R22-6's LCM arithmetic. Any prototype must pass a realloc WIN gate (not merely parity) vs the Large baseline before `production` promotion is considered (R34-26 §8).
   > - **Evidence:** `R12_13_PAGE_RUN_LAYER_DEFERRED.md` §4 (lines 188–237); `R11_7_PAGE_RUN_LAYER_DESIGN.md` (full design); `docs/design/R34_26_PAGE_RUN_LAYER_DESIGN_GATE.md` (the in-place-grow design-gate, task #545).
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

25. **R9-5 / R11-8 / R13-3 / R29-16 / R30-3 / R31-0 / R32-0 — `virgin-zero-skip`
    promotion decision — REOPENED: R30-3's NO-GO measured the wrong
    allocator layer (bare `AllocCore`, not the production `HeapCore`
    magazine). R31-0 (task #471) re-measured through the real production
    call chain (the BENEFIT side). R32-0 (task #490) closed the missing
    COST side (plain `alloc`, recycled `alloc_zeroed` cited from R31-0,
    `realloc`) in the SAME regime. Verdict: no blanket `production`
    promotion (data does not support one for the touch-heavy majority
    case), but a real, reproducible, mechanistically-explained win exists
    for a narrower touch-light/deferred-touch workload shape, and the
    worst-case cost on non-benefiting paths is confirmed negligible
    (~8% relative / ~1 ns absolute on the worst-case wall-clock cell) —
    still opt-in, pending explicit user sign-off for any composition
    change.**

   > **Current state**
   > - **Status:** REOPENED. R30-3 (task #452) built an activation-proven
   >   judge but drove it through a BARE `AllocCore` (`AllocCore::new()` +
   >   `core.alloc_zeroed`), never through `HeapCore`/`SeferAlloc`/a real
   >   `#[global_allocator]` — it measured the magazine-BYPASS substrate, not
   >   the actual `production + virgin-zero-skip` call chain
   >   (`HeapCore::alloc_zeroed` → `alloc_small_zeroed_via_magazine` → on a
   >   miss, `refill_magazine_slow_virgin`, which retains virginity across an
   >   entire freshly-carved MAGAZINE refill via `PerClass::virgin_mask` —
   >   see `tests/r13_3_magazine_virgin_hit_skips_zero.rs`). R30-3's own
   >   ~1-in-32 same-class-burst activation ceiling is a real, correctly
   >   diagnosed property of the bare-`AllocCore` FREE-LIST refill it used —
   >   it does not describe the magazine-backed production path, which was
   >   never exercised. R31-0 (task #471) rebuilt the judge through
   >   `HeapCore::alloc_zeroed` on freshly `HeapRegistry::claim()`'d heaps
   >   (never recycled) and measured 100% same-class-burst activation.
   > - **Current number/verdict:** Path-activation oracle (R31-0): **4/4
   >   retention-probe PASS** (per-size smoking-gun proof the magazine
   >   retains virginity across a refill, `dbg_tcache_virgin_mask`) + **24/24
   >   ON-binary activation cells PASS at 100.00% minimum** (both virgin and
   >   recycled scenarios, all 4 sizes × 3 touches). Native wall-clock: the
   >   `notouch` consumer category shows a **material, reproducible win of
   >   −89% to −98.6% across all 4 swept sizes**, stable in sign and rough
   >   magnitude across an independent repeat run — the cleanest measurement
   >   this layer permits, since it isolates the skipped `Node::zero` memset
   >   from page-fault noise. The `onebyte`/`full` touch categories remain
   >   SIGN-INCONSISTENT and noise-dominated (matching R30-3's own finding
   >   for its comparable touch-heavy cells) — no reproducible win there.
   >   Recycled (non-virgin control) scenario: small, sign-inconsistent
   >   deltas in both directions (7/12 ON-faster, 5/12 ON-slower) — no
   >   consistent regression reproduces R30-3's own noisy majority-direction
   >   finding on a differently-shaped recycled loop.
   > - **Verdict:** the `notouch` finding is a GO-supporting result for a
   >   NARROW, workload-shape-specific case (calloc'd buffers that are
   >   sparse or lazily touched) — **not** a blanket `production` promotion,
   >   since the touch-heavy majority case (a calloc'd buffer populated
   >   shortly after allocation) shows no reproducible win. Per this
   >   project's standing rule, no composition change was made without
   >   explicit user sign-off; the feature remains opt-in, now correctly
   >   characterized (genuinely ~100%-active on same-class bursts through the
   >   real production magazine, benefit concentrated in the
   >   touch-light/deferred-touch consumer shape) rather than the "structurally
   >   useless for any same-class burst" characterization R30-3 shipped.
   > - **Cost side (R32-0, task #490):** the BENEFIT side (above) was
   >   proven but the COST side — what does turning the feature ON cost on
   >   paths that collect NO benefit (plain `alloc`, recycled
   >   `alloc_zeroed`, `realloc`) — had never been measured, in violation of
   >   CLAUDE.md's same-workload-regime cost/benefit rule. R32-0 closed this
   >   gap at the SAME `HeapCore` layer/regime as R31-0. Source-confirmed
   >   (`heap_core_alloc.rs`/`heap_core_free.rs`/`heap_core_dealloc_batch.rs`)
   >   that every non-`alloc_zeroed` site the feature touches is a small,
   >   fixed, unconditional bitmask op (never a branch on content, never
   >   proportional to size); `realloc` itself has ZERO direct feature code
   >   (100% inherited from `alloc`/`dealloc`). Deterministic in-process gate
   >   (mean-of-15) could not resolve a signal this small against host noise
   >   (every cell flips sign run-to-run); a 20-pair wall-clock paired-AB via
   >   `paired-ab-runner.mjs` on the single worst-case cell (4 KiB
   >   recycled/steady-state-hit, 100% magazine-hit activation) DID resolve a
   >   real signal: t=-3.240 (crit=2.101), sign 17/20, **~8.2% relative /
   >   ~1 ns/round absolute cost, ON slower** — confirmed real against a
   >   same-vs-same control (t=0.101, noise). Recycled `alloc_zeroed` itself
   >   was NOT re-measured (R31-0 §3.2 already covers it: no material,
   >   consistent regression). **Verdict: GO on the cost side specifically —
   >   the worst-case cost (~8%) is two orders of magnitude smaller than the
   >   confirmed benefit (-89% to -98.6%)** — this does not change the
   >   blanket-promotion verdict above (R31-0's touch-heavy-majority gap is
   >   untouched by this finding) but removes cost as an open unknown for any
   >   future narrow-promotion decision. A known confound (`alloc_zeroed`'s
   >   magazine-hit arm pays an extra `stamp_segment_owner` call plain
   >   `alloc`'s hit arm does not, unrelated to `virgin-zero-skip`, filed as
   >   task #495) means R32-0's plain-`alloc` absolute-ns figures are not
   >   directly comparable to R31-0's `alloc_zeroed` absolute-ns figures —
   >   noted explicitly in R32-0 so this is not misread.
   > - **Confound resolved (R32-4, task #495):** the redundant
   >   `stamp_segment_owner` call was removed from
   >   `alloc_small_zeroed_via_magazine`'s magazine-hit arm after enumerating
   >   all three producers of a magazine-resident block and confirming none
   >   can place an unstamped segment's block there (same P4 guarantee plain
   >   `alloc`'s hit arm already relies on). Measured Ir saving: −12.00/hit
   >   (16-hit delta −192 Ir, WSL/callgrind, four plain-`alloc` kill-gate
   >   benches confirmed exactly flat). See
   >   `docs/perf/R32_4_ALLOC_ZEROED_MAGAZINE_HIT_STAMP_REMOVAL_GATE.md` and
   >   `docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md`'s §9
   >   addendum (direction of bias: the confound was AGAINST the ON arm, so
   >   R31-0/R32-0's published GO verdicts are unaffected, if anything
   >   slightly conservative). R32-0's plain-`alloc` vs R31-0's `alloc_zeroed`
   >   absolute-ns comparability caveat above is now historical (described
   >   the pre-fix state at the time R32-0 was measured) — it is left
   >   as-is per the append-only convention, not deleted.
   > - **Next trigger:** if a future round wants to pursue promotion: (a) a
   >   caller-facing knob distinguishing sparse/lazily-touched calloc buffers
   >   from immediately-populated ones (§5's "recommended narrower framing" in
   >   the R31-0 report), since a blanket default would apply the proven
   >   `notouch` win uniformly to the touch-heavy majority where it does not
   >   reproduce; or (b) a larger sample count / quieter host to try to
   >   resolve the `onebyte`/`full` categories' sign-inconsistent noise into a
   >   real signal one way or the other. Cost is no longer a reason to defer
   >   (R32-0 confirms it negligible even in the worst-case arm).
   > - **Evidence:** `R9_5_VIRGIN_ZERO_SKIP_DESIGN.md` §11 (Stage 3, lines
   >   563–568); `R11_8_SMALL_VIRGIN_ZERO_SKIP_DESIGN.md` §8;
   >   `R13_3_VIRGIN_ZERO_SKIP_MAGAZINE_GATE.md` (original null finding);
   >   `R29_16_VIRGIN_ZERO_SKIP_CALLOC_GATE.md` (iai isolation §3 STILL VALID —
   >   3,067 vs 65,624 Ir, ~21.4×, confirms real skipped work, NOT a wall-clock
   >   speed claim; its own §4/§8 wall-clock portion is SUPERSEDED, kept
   >   append-only for history) + its summary CSV + raw logs;
   >   `R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md` (task #452, activation-proven
   >   but WRONG-LAYER judge — §8 dated correction now points to R31-0; its
   >   own Ir-level evidence, recycled-scenario, and lazy-commit-crossing
   >   discussions remain valid supplementary context) +
   >   `R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE_summary.csv` +
   >   `docs/perf/_raw_r30_3_off_eager.log` / `_raw_r30_3_on_eager.log` /
   >   `_raw_r30_3_off_lazy.log` / `_raw_r30_3_on_lazy.log` /
   >   `_raw_r30_3_off_eager_run2.log` / `_raw_r30_3_on_eager_run2.log`;
   >   **`R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md`** (task #471, the
   >   corrected production-layer judge and the benefit-side operative
   >   verdict) + `R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE_summary.csv` +
   >   `docs/perf/_raw_r31_0_off.log` / `_raw_r31_0_on.log` /
   >   `_raw_r31_0_off_run2.log` / `_raw_r31_0_on_run2.log`;
   >   **`R32_0_VIRGIN_ZERO_SKIP_COST_SIDE_GATE.md`** (task #490, the
   >   cost-side gate) + `R32_0_VIRGIN_ZERO_SKIP_COST_SIDE_GATE_summary.csv` +
   >   `docs/perf/_raw_r32_0_off.log` / `_raw_r32_0_on.log` /
   >   `_raw_r32_0_off_run2.log` / `_raw_r32_0_on_run2.log` /
   >   `_raw_r32_0_cost_probe_ab.log` / `_raw_r32_0_cost_probe_ab_same_vs_same.log`.
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

28. **R13-6 — `exact-span-large` CONDITIONAL-GO, not promoted; owner entry
    added R30-14 (task #463) to close a zero-owner gap `FEATURE_PROMOTION_STATUS.md`
    itself flagged.**

   > **Current state**
   > - **Status:** CONDITIONAL-GO, not promoted — R13-6 explicitly declined an
   >   unconditional GO. Previously tracked ONLY as a passing reference inside
   >   item 3's narrative (never as its own owned item) and in
   >   `docs/FEATURE_PROMOTION_STATUS.md`'s survey table — this is the first
   >   dedicated `OPEN_ITEMS.md` entry.
   > - **Current number/verdict:** the RSS win is real and large (15.8×→1.06×
   >   at 260 KiB, §2) and unregressed, but paired with `large-reserved-capacity`
   >   it is NET SLOWER than plain `production` on a doubling-cadence realloc
   >   workload — iai `realloc_grow`: **+102.3% instructions, +52.7% Estimated
   >   Cycles**, deterministic (§3.3) — which this project's own iai-authoritative
   >   policy treats as decisive over the smaller-magnitude wall-clock deltas.
   > - **Next trigger:** per R13-6 §7's own ordered list — (1) a follow-up
   >   investigating whether `LARGE_RESERVED_CAP_GROWTH_FACTOR` (fixed 2×) can be
   >   widened or made to compound across relocations, judged by the SAME
   >   pre-existing iai `realloc_grow` bench; (2) confirmation no
   >   `production`-shipped workload in this repo's own benches exhibits the
   >   doubling-cadence pattern at a user-visible scale; (3) cross-platform
   >   (real Linux/macOS, not just WSL2) confirmation the RSS win and the
   >   realloc regression hold their relative shape.
   > - **Evidence:** `R13_6_EXACT_SPAN_RESERVED_CAPACITY_PRODUCTION_GATE.md` §7
   >   (the CONDITIONAL-GO recommendation + the 3-condition GO path);
   >   `docs/FEATURE_PROMOTION_STATUS.md` (survey row).

51. **R828 — sefer-region structural levers (P-perf-1/2/4/5).**

   > **Current state**
   > - **Status:** measured, all DEFER except P-perf-2 (GO as opt-in).
   > - **Current number/verdict:**
   >   - **P-perf-1 (DenseRegion):** Iteration win is real (9.45× faster, 13.5µs vs 127.3µs mean per pass at 100k→10k live scale), churn regression is also real (2.9× slower, 60.8M vs 176.2M ops/sec). Tradeoff confirmed, not a free upgrade.
   >   - **P-perf-2 (batch/guard API):** Closure wrapper shows no reliable overhead vs manual guard (both single-digit-ns, within noise at this scale). One-shot penalty confirmed at 9.15× (materially smaller than the original audit's cited 31.6× or the first measurement attempt's DCE-inflated 59.3× — see report's zero-trust correction note). GO for opt-in convenience implementation; re-measure under a realistic workload before citing a multiplier in user docs.
   >   - **P-perf-4 (drop outside write-lock):** Real, large, reproducible benefit once the probe's race bug was fixed — contending reader blocked for the FULL baseline clear (~4,849.6ms mean) vs ~0.0019ms under two-phase. Semantic design questions (region_id/generation survival across the swap, panic safety, landing the fix inside `SyncRegion::clear()` itself) remain open and are the actual blocker for implementation, not the measurement.
   >   - **P-perf-5 (Sharding):** Not remeasured per task scope (defers to confirmed production bottleneck). Open design fork (Shape A vs B) unresolved.
   > - **Next trigger:** P-perf-1: production bottleneck on holey iteration identified; open design questions (handle identity, generic backing) resolved. P-perf-2: implementation task filed; naming convention decided; one-shot ratio re-measured under a realistic consumer workload. P-perf-4: region_id/generation survival + panic safety + landing-inside-`clear()` semantic design completed. P-perf-5: production bottleneck on concurrent readers identified; design fork resolved.
   > - **Evidence:** `docs/perf/R828_STRUCTURAL_LEVERS_GATE.md` (full report with verdicts and a zero-trust correction note documenting two methodology bugs found and fixed during review — a missing `black_box` that produced a fabricated "0ns/infinite speedup" result for P-perf-1, and a synchronization race that made the first P-perf-4 measurement meaningless); `docs/perf/_raw_r828_dense_iteration.log`, `R828_DENSE_ITERATION_summary.csv` (P-perf-1 data); `docs/perf/_raw_r828_batch_guard.log`, `R828_BATCH_GUARD_summary.csv` (P-perf-2 data); `docs/perf/_raw_r828_drop_outside_lock.log`, `R828_DROP_OUTSIDE_LOCK_summary.csv` (P-perf-4 data). Design docs: `docs/perf/SEFER_REGION_DENSE_AND_SHARDED_DESIGN.md`, `docs/perf/SEFER_REGION_BATCH_READ_API_DESIGN.md`. Harness commit: `54bfe96f7ae4649ae9813cc4b6908fae1d40aec0`.
   Full history: task #828 (this measurement round; no prior rounds). The harness's own first-draft measurement attempt (commit `efed284`, amended out) is not a valid citation — see the report's zero-trust correction note. **Task #832's closing review (`docs/reviews/2026-08-11-sefer-region-f1-f13-perf-closing-review.md`) found and fixed 8 label/attribution/wording defects in this item's own evidence** (report headers mislabeling constants/units, a backwards reading of the 8-reader contention data, R827's baseline arm conflating two costs F1 itself changed at once — since fixed with a third `shared_fetch_add` decomposition arm, a churn-mechanism attribution not exercised by the one-element workload measured, and 5 stale doc references) — **none of it changed any verdict above**, all four verdicts (DEFER/GO-opt-in/DEFER/DEFER) survive unchanged. One item from that review is deliberately NOT yet closed: **F-C8** — both this report and R827's cite a "derived by a small script" summary-CSV process, but no such script is committed anywhere in the tree (the probes' own printed summaries happen to make every cell independently checkable today, which is why no number was found wrong — but the reproducibility claim as stated is stronger than what's committed). Next trigger for F-C8: commit the ~30-line derivation script alongside the existing CSVs, or replace the "by a small script" phrasing in both reports with an accurate description of how the numbers are actually derivable (reading them off each probe's own raw-log summary section).

29. **R14-6 / R20-2 — `large-reserved-capacity` CONDITIONAL-GO, not promoted;
    owner entry added R30-14 (task #463) to close a zero-owner gap.**

   > **Current state**
   > - **Status:** CONDITIONAL-GO (R14-6 §5), contingent on `exact-span-large`
   >   (item 28); R20-2's later, more direct measurement found NO benefit on
   >   the specific axis it targeted. Previously tracked ONLY as a deferred
   >   growth-factor sub-finding inside item 8, never as its own owned item.
   > - **Current number/verdict:** R20-2's paired C1-vs-C4 comparison (§6.1)
   >   found `large-reserved-capacity`'s geometric growth headroom does **NOT**
   >   measurably reduce the medium→Large realloc-promotion cost on top of
   >   `medium-classes` alone (t=1.209 ≪ crit 2.101, sign test dead-even
   >   10/20) — **verdict NULL** for that specific axis, by mechanism (§6.2):
   >   `reserved_capacity` is set on the fresh Large segment AFTER the
   >   promotion memcpy already ran, so it structurally cannot cheapen that
   >   copy. Separately, `exact-span-large` (which this feature requires) DOES
   >   show a real, reproducible commit-charge win (~50.5→~23.9 MiB, §6.3) at
   >   identical hit rate — orthogonal to the realloc-time question, not
   >   invalidated by the NULL verdict.
   > - **Next trigger:** promotion is gated on `exact-span-large` (item 28)
   >   first clearing its own CONDITIONAL-GO path; even then, R20-2's NULL
   >   result means the specific "helps realloc-promotion cost" justification
   >   for `large-reserved-capacity` no longer applies — any future promotion
   >   case would need to rest on the commit-charge benefit alone (§6.3) or a
   >   different, unmeasured workload shape where the reserved headroom IS
   >   consulted before a promotion event.
   > - **Evidence:** `R14_6_ADAPTIVE_RESERVED_CAPACITY_GATE.md` §5 (original GO
   >   recommendation); `R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE.md` §6 (the
   >   NULL verdict + mechanism); `docs/FEATURE_PROMOTION_STATUS.md` (survey
   >   row).
   > - **R34-23 update (task #542):** a subprocess-isolated A/B on the
   >   `geometric_x2_4mib` realloc grow chain (64 B → 4 MiB, 16 doublings)
   >   found LRC does NOT improve in-place rate (path-activation oracle
   >   identical: 3 in-place + 13 declines per chain with and without LRC)
   >   and is 3.4× SLOWER (893 µs vs 261 µs median) — root cause: LRC implies
   >   `exact-span-large`, which shrinks initial `span_usable` from 4 MiB
   >   (SEGMENT-rounded) to page-exact, hurting large-cache reuse; the 4×
   >   reserved factor is outgrown within 2 doublings. **Verdict NO-GO for
   >   the geometric-realloc-grow axis** (consistent with R20-2's NULL for
   >   the promotion axis). See `R34_23_REALLOC_AND_VEC_GATE.md` §4.

30. **R14-5 — `large-cache-extended` CONDITIONAL-GO, not promoted; owner entry
    added R30-14 (task #463) to close a zero-owner gap. RE-VERIFIED R31-3
    (task #466): all six checkpoints still hold on current `HEAD`, A/B
    refreshed, two precondition gaps closed (N=1/2/4 timing, multi-heap RSS)
    — a promotion PROPOSAL now exists, pending explicit user sign-off.**

   > **Current state**
   > - **Status:** CONDITIONAL-GO (R14-5 §9), not promoted. R31-3 (task #466)
   >   independently re-verified all six R14-5 hardening checkpoints against
   >   present-day source, refreshed the turnover A/B, and closed the two
   >   precondition gaps a prior review named as missing (N=1/2/4
   >   narrow-working-set TIMING regression check, multi-heap RSS
   >   accounting) — both come back clean. A promotion PROPOSAL is now
   >   written (`R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md` §5),
   >   explicitly NOT self-authorized, awaiting user sign-off.
   > - **Current number/verdict:** all six of R14-5's required hardening items
   >   STILL hold on current `HEAD` (budget-vs-materialisation ordering
   >   fixed, unchanged; finite default budget mechanism unchanged — the
   >   NUMERIC value moved 5x/1280 MiB → 1x/256 MiB per R17-9, already
   >   disclosed, confirmed current; N=1/2/4 hit-path and mixed-size/FIFO
   >   correctness tests still pass; the turnover-profile A/B REPRODUCES on
   >   current code at the current 256 MiB default, t=127.776 n=20 sign
   >   20/20, mechanism confirmed 33.3%→100%). NEW evidence (R31-3): the
   >   N=1/2/4 narrow-working-set TIMING question R14-5 §4 deferred is now
   >   measured — NO regression found; the extended cache measured FASTER at
   >   every N (t=7.1-17.8, sign 19-20/20), mechanistically explained by the
   >   base cache's own FIFO-eviction-and-refill cost during materialisation,
   >   not the wider scan bound itself. NEW evidence (R31-3): multi-heap RSS
   >   accounting at 1/8/32 concurrently-claimed heaps confirms the finite
   >   256 MiB default bounds per-heap retention with EXACT linear scaling
   >   (no shared/amortized-state surprise in the measured workload shape) —
   >   ~248 MiB/heap capped (ON) vs ~432 MiB/heap unbounded (OFF).
   > - **Next trigger:** R31-3's own §5 proposal — user explicit sign-off on
   >   promoting `large-cache-extended` (with its current, unchanged 256 MiB
   >   default budget), coordinated with R31-9/#473's `Profile` API rework
   >   (a named `Profile` variant may be the cleaner integration point than a
   >   blanket `production` composition change, since R13-8's static-live-set
   >   no-benefit caveat still applies) — if accepted, `npm run bench:table` +
   >   `npm run iai` must be re-run and committed in the SAME PR per
   >   CLAUDE.md's composition-change rule.
   > - **R31-9/task #473 update (2026-07-30):** `Profile`'s rework reserved,
   >   but deliberately did NOT implement, exactly the integration point this
   >   note anticipated — `LargeCachePolicy` is `#[non_exhaustive]` with a
   >   documented (not-yet-added, not-constructible) slot for a future
   >   `large-cache-extended`-backed variant, so if/when this item's sign-off
   >   trigger fires, it slots into that ONE axis without touching
   >   `SmallPoolPolicy` or requiring a `production` composition change. The
   >   user sign-off itself is still pending — this item stays open.
   > - **Evidence:** `R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` §9 (the
   >   original CONDITIONAL-GO + the explicit GO condition);
   >   `R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md` (the re-verification
   >   + refreshed A/B + two new regression gates + the promotion proposal) +
   >   its summary CSV + 9 raw logs; `docs/FEATURE_PROMOTION_STATUS.md`
   >   (survey row, not yet updated by R31-3 — a follow-up if promotion is
   >   accepted).

   > **CORRECTION — 2026-08-02 (task #489, ledger reconciliation after tasks
   > #487/#488): the N=1/2/4 narrow-working-set TIMING finding cited above
   > ("NO regression found ... extended cache measured FASTER") is INVALID
   > and its verdict is now the OPPOSITE — NO-GO. Do not read the paragraph
   > above as current evidence for that sub-question; this note supersedes
   > only the narrow-timing sub-finding, nothing else in this item's card.**
   > An independent review found the workload behind that finding
   > (`examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`)
   > had two defects, fixed in commit `ac7845d` (task #487): (1) a hardcoded
   > `2 * 1024 * 1024usize` segment constant claimed to equal
   > `SegmentLayout::SEGMENT` when the real constant is 4 MiB, silently
   > halving every one of the 9 materialisation-burst sizes; (2) no in-run
   > materialisation oracle — the ON arm's claim of having actually
   > widened to 40 slots was never checked inside the timed run itself, a
   > CLAUDE.md R30-8 path-activation-oracle violation. Fixing the oracle
   > surfaced a third defect: under the original 256 MiB default budget,
   > the ON arm's materialisation sizes (corrected to real 4 MiB units)
   > individually exceeded the budget for 3 of 9 sizes, so the ON arm never
   > actually widened to 40 slots at all — it silently measured the SAME
   > 8-slot code path as OFF. This is also why the previously-unexplained
   > `segments_reserved_total` contradiction (10 OFF vs 14 ON at N=4, flagged
   > by the R31/R32 review §3.2 "Ошибка 3") existed: a budget-rejection-driven
   > admission divergence between arms, not a real mechanism — task #488
   > (commit `4f89723`) proved this empirically: with the corrected workload,
   > `segments_reserved_total` grows by an IDENTICAL, hard-asserted delta
   > (exactly `MATERIALIZE_N`=9) in both arms at every N; the contradiction
   > does not reproduce. Task #488 then re-derived the narrow-timing verdict
   > from scratch, in matched state, with two complementary measurements:
   > (a) real-process A/B (`SeferAlloc` global allocator, n=20 paired
   > A/B/B/A) at N=1/2/4 — ON is measurably, reproducibly SLOWER at every N
   > (t=-11.6/-7.8/-13.5, all past crit=2.101, clean same-vs-same noise-floor
   > controls); (b) a decomposed scan-isolation microjudge (bare `AllocCore`,
   > worst-case fixed scan position, 8 vs 40 slots, cache-hit oracle) isolates
   > the `alloc_large` best-fit scan loop itself and shows a 5.01x ns/round
   > ratio (79.7 ns @ 8 slots vs 399.3 ns @ 40 slots, n=20 paired, t=-29.3).
   > **Verdict: NO-GO for "the widened O(40) scan bound is free/negligible
   > on a narrow working set"** — a real, reproducible, matched-state-proven
   > cost exists, small in absolute per-operation terms (order 100-500 ns per
   > alloc+dealloc pair at these N) but not noise. This does NOT touch or
   > reopen the turnover-win finding elsewhere in this same card (hit rate
   > 33.3%→100%, t=127.776, sign 20/20) — that used a different, unmodified
   > harness (`examples/paired_ab_large_cache_extended_{off,on}.rs`) never
   > touched by tasks #487/#488, and stands unaffected. This item's overall
   > **Status stays CONDITIONAL-GO / not promoted** (unchanged) — the
   > promotion sign-off this item's "Next trigger" names must now weigh a
   > confirmed narrow-working-set cost, not the withdrawn "free" claim. Full
   > re-derivation: `R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md` §3.4
   > (documents the task #487 workload defects) and §8 (task #488's full
   > matched-state re-measurement, mechanism explanation, and verdict).
   > Follow-up promotion-policy work (how to weigh this cost against the
   > turnover win) is tracked separately as task #491, not by this item.
   >
   > **Merge note:** this correction also folds in and closes out the older,
   > separate `[L]` item 7 below ("R14-5 §4 — dedicated timing gate for
   > O(40) vs O(8) ... deferred, low-priority, no number attached yet"),
   > which asked for exactly the number this correction now supplies. The
   > two entries had drifted out of sync — item 30 (this one) read as
   > CLOSED with a "FASTER" number while item 7 still read as DEFERRED with
   > "no number attached yet" — a ledger contradiction identified by the
   > 2026-07-31 R31/R32 readonly review §4 ("P2: привести open-item ledgers
   > к единому состоянию") and task #489. Item 7 is left in place below,
   > struck through, per this file's append-only convention (see rule 2:
   > "Do NOT delete the entry"), pointing back here as the single current
   > source of truth for this question.

   > **UPDATE — 2026-08-02 (task #491): named opt-in policy shipped
   > (`LargeCachePolicy::DiverseTurnover`); process-wide retention story
   > resolved as DOC-ONLY, not a shared-budget mechanism.**
   > Two blockers were named for this item's promotion question: (1) the
   > scan-cost question (resolved NO-GO by task #488, folded in above), and
   > (2) the per-heap-not-process-wide RSS retention question. Task #491
   > closes (2) and ships the policy scaffolding this item's own "Next
   > trigger" anticipated, WITHOUT changing `production`'s composition or
   > `Profile::DEFAULT` (both remain untouched — this is additive):
   > - **Process-wide retention decision: DOC-ONLY (option 3b from the
   >   task brief), not a shared-budget mechanism (option 3a).** A
   >   process-wide shared budget was weighed and explicitly declined: it
   >   would be a brand-new cross-heap synchronization point on a path that
   >   has none today (the R17-9 design note in
   >   `src/alloc_core/large_cache_config.rs` already rejected this same
   >   option for the same reason when the 256 MiB default was chosen), and
   >   its own contention/coordination cost would itself need a real
   >   multi-thread A/B under genuine concurrent pressure before it could
   >   be trusted — a second gate-report-sized undertaking with no standing
   >   evidence yet to justify building speculatively, per this item's own
   >   evidence-first discipline. Building it "just in case" without that
   >   evidence would be exactly the kind of half-built, unmeasured
   >   mechanism CLAUDE.md's phased-delivery rules warn against. Instead,
   >   the linear per-heap worst case (`N heaps × 256 MiB`) is now stated
   >   explicitly, inline, everywhere a caller would encounter this policy
   >   before choosing it: `LargeCachePolicy::DiverseTurnover`'s own doc
   >   comment (`src/alloc_core/profile.rs`), the `Profile` module doc's
   >   new R31-3/task #491 section, and README's Named-profiles table —
   >   each states the 32-heap ≈ 7.75 GiB worst case by name, not just "no
   >   multi-heap RSS blow-up" (this item's own filed concern about R31-3's
   >   original phrasing being too soft).
   > - **Named opt-in policy:** `LargeCachePolicy::DiverseTurnover` (the
   >   filer's suggested name, unchanged) added to the existing
   >   `#[non_exhaustive]` `LargeCachePolicy` enum
   >   (`src/alloc_core/profile.rs`) — requires `large-cache-extended`
   >   compiled in to have any runtime effect (a `Profile` axis value
   >   cannot itself select a compile-time Cargo feature); without that
   >   feature, resolves byte-identical to `Default`. Its own doc comment
   >   states all three costs/benefits together (turnover win — §2; narrow
   >   cost — §8; per-heap-not-process-wide retention — §4), per this
   >   item's and CLAUDE.md's same-regime cost/benefit discipline. `#[non_exhaustive]`
   >   is preserved for a genuinely new axis point going forward.
   >   `production`'s feature list and `Profile::DEFAULT` are unchanged —
   >   this does not reopen or resolve this item's own promotion question,
   >   which stays CONDITIONAL-GO / not promoted, pending the SAME explicit
   >   user sign-off this item's "Next trigger" already named.
   > - **Test coverage:** `tests/profile.rs` extended from a 2×3 to a 2×4
   >   axis cross-product (`all_axis_combinations_resolve_independently`)
   >   plus a new dedicated test
   >   (`diverse_turnover_without_the_feature_matches_default`) proving the
   >   feature-OFF byte-identical-to-`Default` contract holds.
   > - **What was NOT done, and why:** the O(40) scan LAYOUT itself
   >   (compact occupancy bitmap / size-ordered index) was explicitly out
   >   of scope for this task (separate, already-filed backlog item —
   >   `docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` finding F8,
   >   task #503) — this task is POLICY (when/how a user opts in), not
   >   scan-structure redesign.
   > - **Files:** `src/alloc_core/profile.rs` (`LargeCachePolicy::DiverseTurnover`
   >   + module-doc section), `tests/profile.rs` (2×4 cross-product +
   >   feature-OFF-parity test), `README.md` (Named-profiles table +
   >   DiverseTurnover disclosure paragraph), this entry.

31. **UNVERIFIED-BY-ME findings from the Round 30 full independent review
    (`docs/reviews/2026-07-30-r30-full-review.md` §5, P2-3 through P2-11) —
    measurement-methodology defects in Round 30's own gate reports and
    process docs, filed 2026-07-30 during the same round's P1 review-response
    task, not independently re-verified before filing.**

   > **Current state**
   > - **Status:** filed, not fixed or independently re-verified — flagged
   >   here at the review's own confidence/severity for a future round to
   >   check and either action or dismiss, mirroring the exact "filed, not
   >   fixed" pattern `docs/CORRECTNESS_OPEN_ITEMS.md` item 5 (and its
   >   sibling item 8, filed the same day for this review's P2-1/P2-2) uses
   >   for the correctness-side counterpart.
   > - **Current number/verdict:** nine sub-findings, as the review's own §5
   >   states them (this entry restates, does not re-derive):
   >   - **P2-3** — `R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md` §0.1's idle-RSS
   >     claim ("`rss_idle_kib - rss_burst2_kib` is 0 or within single-digit
   >     KiB noise in every row") is false as written for all 36 CSV rows per
   >     the review's recomputation (`|rss_idle - rss_burst2|` ranges 16-28
   >     KiB at 1 thread up to 756-848 KiB at 32, plus one row the review
   >     reports as −1,574,932 KiB) — the review states the comparison is
   >     structurally wrong (`burst2` is measured AFTER the idle window, so a
   >     difference is expected) and that the claim the report actually wants
   >     (`rss_idle - rss_burst1 == 0`) IS satisfied in 33/36 rows per its
   >     recomputation. The review states the finding is right, only the
   >     cited arithmetic is not.
   >   - **P2-4** — R30-6's raw log row `67108864,64,32,2`
   >     (`rss_burst1_kib=1,580,920`, `rss_idle_kib=424`) is, per the review,
   >     a physically impossible sample (a 32-thread process cannot drop to
   >     424 KiB RSS across a 1.2s sleep) that is neither excluded nor
   >     flagged in the report; the review notes medians protect the §0.1
   >     headline so no conclusion changes, but suggests a sanity assertion
   >     in the harness.
   >   - **P2-5** — R30-6's headline ("64 MiB preserves the FULL measured
   >     hit-rate benefit... while RSS retention drops... ~7x smaller") per
   >     the review joins two findings from workloads at different cache
   >     occupancy (hit-rate parity measured at 48 MiB/burst, below where
   >     64 MiB's cap binds at all; the ~7x RSS saving is R29-13's, measured
   >     at 272 MiB/burst, where the cap DOES bind and hit-rate cost is
   >     unmeasured) — the review states the report body itself is honest
   >     about this, but that `Profile::Balanced`'s/`Profile::Throughput`'s
   >     shipped doc comments (`src/alloc_core/profile.rs:38-45,68-84`) and
   >     the README profile table carry the headline without the regime
   >     caveat.
   >   - **P2-6** — per the review, `Profile::Throughput`'s doc comment cites
   >     its "~22%" win but has no pointer to
   >     `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md`'s null result
   >     from the SAME task/commit (README does disclose it; the API doc a
   >     caller reads at the call site does not, per the review).
   >   - **P2-7** — the review states `.github/workflows/ci.yml`'s `clippy`
   >     job header comment claims the `check-matrix` job would independently
   >     catch clippy-step drift, but `run-check-matrix.mjs`'s `--kind`
   >     filter excludes every `clippy` row by construction, so per the
   >     review no CI mechanism actually asserts the five hand-transcribed
   >     clippy steps match the manifest (local `npm run check` does, per
   >     the review's own reading of `check-all.mjs`).
   >   - **P2-8** — the review states
   >     `docs/design/R30_10_MEASUREMENT_HOOK_ISOLATION_DESIGN.md` §2.1's
   >     "160 total hooks" / §2.3's "all 160 hooks" is the CLASSIFIED total
   >     (`PURE_OBSERVERS` + `SAFE_MUTATORS` + `UNSAFE_HOOKS`) excluding the
   >     safe-and-`bench-internals`-gated hooks; the review's own Python
   >     reproduction of the tripwire's scanner over `src/`+`crates/` found
   >     179 total `pub fn dbg_*`/`pub unsafe fn dbg_*` definitions, and a
   >     "139 files touch all three buckets" figure the review recomputed as
   >     151. The review notes both discrepancies understate R30-10's own
   >     relocation-cost estimate, so its NO-GO decision is safe either way.
   >   - **P2-9** — the review states R30-14's landing commit message
   >     (`4c52c26`) claims removed OPEN_ITEMS.md lines are "byte-identical"
   >     to `OPEN_ITEMS_ARCHIVE.md` §A13, but its own diff of the 16 removed
   >     lines against the archive found none byte-identical anywhere; the
   >     review states the SUBSTANCE does hold (every load-bearing fact it
   >     checked is present in the archive and/or the rewritten card) — a
   >     wording defect in a commit message, not lost history, per the
   >     review.
   >   - **P2-10** — the review states
   >     `R30_6_LARGE_CACHE_HEADROOM_AB_GATE_summary.csv`'s `commit_sha`
   >     column holds a prose placeholder ("<see report header, this file is
   >     committed alongside...>") rather than the actual SHA, defeating the
   >     summary-CSV rule's point (a script should not need to parse prose);
   >     the review notes the report header itself does carry the real SHA
   >     (`97c2f07`, via follow-up `1272a52`), so this is a one-token fix.
   >   - **P2-11** — the review states `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md`
   >     §0/§6 say "20 pairs, 40 process launches" when the raw log contains
   >     80 launches per comparison (40 per arm) — its sibling `R30_6_...`
   >     §1.6 states the equivalent correctly ("20 pairs = 80 process
   >     launches"); plus two smaller non-functional inaccuracies the review
   >     names in the same bullet (`scripts/check-matrix.mjs`'s JSDoc
   >     `@property` type not matching its actual object shape; a note that
   >     `R30_10_...`'s `awk` reproduction commands did reproduce correctly).
   > - **Next trigger:** independent re-verification of each sub-finding
   >   against its cited source (raw logs/CSVs for P2-3/P2-4/P2-8/P2-11, the
   >   cited `src/`/doc files for P2-5/P2-6, `ci.yml`/`run-check-matrix.mjs`
   >   for P2-7, the commit diff for P2-9, the CSV file directly for P2-10),
   >   then either apply the review's suggested fixes (each stated inline in
   >   the review's §5) or record a reasoned dismissal, in a future round.
   >   None of these block or change any P0/P1 verdict from Round 30 — the
   >   review's own text states none of the eleven P2s "threatens
   >   correctness."
   > - **2026-07-30 update (R31-12, task #476):** P2-3, P2-4, and P2-10
   >   independently RE-VERIFIED (not merely re-stated) against the raw
   >   sources and REPAIRED. P2-3: confirmed `rss_idle_kib - rss_burst2_kib`
   >   is `0` in **0 of 36 rows** (the wrong column pair, structurally —
   >   `burst2` is sampled after the idle window); confirmed the intended
   >   `rss_idle_kib - rss_burst1_kib` claim IS exact in **33 of 36 rows**.
   >   P2-4: confirmed row `67108864,64,32,2` is the ONLY row failing a
   >   `rss_burst1 - rss_idle <= rss_burst1/10 + 4096` sanity bound (drop of
   >   ~1.58 GiB across a pure-idle window with zero deallocation activity);
   >   confirmed excluding it changes no §0.1 headline (`burst2_hits_sum`
   >   median unchanged, 256 either way). P2-10: confirmed and fixed — the
   >   summary CSV's `commit_sha` header now reads the actual landing SHA
   >   (`97c2f07b`). All three repairs are append-only additions to
   >   `R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md` §8 (new), derived by the
   >   checked script `scripts/r31_12_repair_r30_6_data.mjs`, not
   >   hand-transcribed. P2-5 independently RE-VERIFIED as accurate (see
   >   item 27's narrowed parity claim below, same task) — its stated fix
   >   (add the regime caveat to `Profile::Balanced`'s/`Profile::Throughput`'s
   >   doc comments in `src/alloc_core/profile.rs`) is NOT applied by this
   >   task (measurement/docs-only per this task's own scope; `profile.rs`
   >   is mid-rework under R31-9/task #473, which is the more coordinated
   >   landing point for that specific doc-comment edit — flagged as an
   >   explicit input to that task, not left silently unowned). **UPDATE
   >   (2026-07-30, R31-9/task #473): applied.** `Profile::Balanced`/
   >   `Profile::Throughput` no longer exist (split into `SmallPoolPolicy` /
   >   `LargeCachePolicy` axes); the regime caveat now lives on
   >   `LargeCachePolicy::Trimmed64MiB`'s doc comment, the axis value that
   >   replaced the old bundled 64 MiB setting. P2-6 through
   >   P2-9, P2-11 remain unverified by this task — still open for a future
   >   round per the original "Next trigger" above.
   > - **Evidence:** `docs/reviews/2026-07-30-r30-full-review.md` §5 (P2-1
   >   through P2-11 in full; P2-1/P2-2 filed separately in
   >   `docs/CORRECTNESS_OPEN_ITEMS.md` item 8 as the correctness-side
   >   counterpart, per this file's own scope boundary with that sibling
   >   index) — the review's own text is the only source cited here for the
   >   unverified sub-findings; P2-3/P2-4/P2-5/P2-10 additionally cite
   >   `R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md` §8 (the 2026-07-30 addendum)
   >   and `scripts/r31_12_repair_r30_6_data.mjs` as independent verification.

32. **The "wrong allocator layer" defect class — a gate report must name the
    exact entry point under test and why that layer is the decision-relevant
    one (P1-3, `docs/reviews/2026-07-31-r31-full-review.md` §7).**

   > **Current state**
   > - **Status:** CLOSED this round — codified as a CLAUDE.md rule (see
   >   "Active rules", sibling to the R26-4 config-evidence rule and the
   >   R30-8 mechanism-activation rule), not left as a narrative-only finding.
   > - **Current number/verdict:** third instance of one meta-pattern —
   >   R25-5 measured the wrong CONFIG (→ R26-4 rule); R29-16 measured the
   >   wrong CODE PATH (→ R30-8 rule); R30-3 measured the wrong LAYER (→ this
   >   rule). R30-3 (task #452) built a judge that satisfied every rule that
   >   existed at the time — including R30-8's own path-activation oracle,
   >   honestly reporting ~3% activation and correctly diagnosing
   >   `carve_block_with_refill`'s 31-block free-list dilution — and still
   >   shipped a wrong NO-GO verdict, because it measured
   >   `AllocCore::alloc_zeroed` (bypassing the magazine) instead of
   >   `HeapCore::alloc_zeroed` (the chain `SeferAlloc`'s
   >   `#[global_allocator]` actually uses, which retains virginity across an
   >   entire magazine refill via `PerClass::virgin_mask`). Caught and
   >   reopened in R31-0 (task #471, commit `dece4a7`), see this file's item
   >   25 (REOPENED).
   > - **Next trigger:** none — this is a standing rule, not a pending
   >   remeasurement. A future gate report that measures below the layer a
   >   feature actually ships at (e.g. bare `AllocCore` instead of
   >   `HeapCore`/`SeferAlloc`) without stating why that lower layer is still
   >   decision-relevant violates the new CLAUDE.md rule and should be
   >   caught in that task's own zero-trust review, not deferred here.
   > - **Evidence:** `docs/reviews/2026-07-31-r31-full-review.md` §7 P1-3;
   >   `docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md`; CLAUDE.md
   >   "Active rules" (the new rule this item's closure added).

33. **[T, filed 2026-07-31, UNVERIFIED-BY-ME findings from the Round 31 full
    independent review (`docs/reviews/2026-07-31-r31-full-review.md` §7
    P2-1, P2-2, P2-3, P2-7, P2-8, P2-9, P2-10)]** The following seven P2
    findings were NOT independently re-verified before filing — flagged
    here at the review's own confidence/severity, for a future round to
    check and either action or dismiss, per this file's own convention
    (item 31 above is the direct precedent for this exact "filed, not
    fixed" pattern, one round earlier).

   > **Current state**
   > - **Status:** filed, not fixed or independently re-verified.
   > - **Current number/verdict:** seven sub-findings, as the review's own
   >   §7 states them (this entry restates, does not re-derive):
   >   - **P2-1** — R31-0's summary CSV is structurally ragged: 49 rows with
   >     24 fields under a single 24-column header, but 4 `retention` rows
   >     with only 16 fields interleaved mid-file with no section marker
   >     (the review's own `awk -F',' '{c[NF]++}'` recount) — `fmtRetention`
   >     emits against a `RETENTION_HEADER` constant defined in
   >     `scripts/r31_0_summary.mjs` but never actually written to the file,
   >     so a standard CSV reader mis-keys the 4 retention rows (e.g.
   >     `expected_hits` reads as `true`). Suggested fix per the review: emit
   >     two files, write the second header line, or pad to the wide schema.
   >   - **P2-2** — R31-0's CSV publishes a knowingly-vacuous statistic
   >     without marking it: all 24 OFF-binary rows carry `mean_act_pct`/
   >     `min_act_pct` of 100.00 or 0.00, derived from a counter the report's
   >     own §2.2 proves is never incremented on the OFF binary at all; only
   >     the separate `oracle=NA` column signals this, so a script reading
   >     just the percentage columns sees a meaningless "100% activation."
   >     Suggested fix per the review: emit `NA` in those two columns on the
   >     OFF arm too.
   >   - **P2-3** — R31-0 §3.3 cites four specific wall-clock percentages
   >     from a third, uncommitted, unreproducible ad-hoc re-run ("landed in
   >     the same −91%/−97%/−99%/−99% range"), explicitly labelled
   >     "corroborating, not part of the cited evidence set" — the review
   >     calls this honest (materially better than the R29-3 pattern
   >     CLAUDE.md's R30-9 rule was written against) but still four numbers
   >     in the report with no committed artifact behind them. Suggested fix
   >     per the review: commit the log, or drop the figures and keep only
   >     the qualitative statement.
   >   - **P2-7** — R31-1 misattributes "36 rows" to R30-6's own committed
   >     CSV ("confirmed directly by R30-6's own committed CSV... in every
   >     one of its 36 rows") when R30-6's committed CSV has 12 section-1
   >     data rows (each a median of 3 reps); the 36 rows live in the RAW
   >     LOG, which `scripts/r31_12_repair_r30_6_data.mjs:56` correctly cites
   >     as "36 rows in R30-6 raw log" — the claim is true of the raw log,
   >     wrong about the artifact named in R31-1's prose. Suggested fix per
   >     the review: cite the raw log, or say "12 CSV rows / 36 underlying
   >     arms."
   >   - **P2-8** — a unit error inside R31-3's summary CSV: one row's note
   >     reads "3280892/8 = ~410 MiB/heap" for a `rss_post_kib_per_heap`
   >     value of 410,112 — which is 400.5 MiB, not ~410 MiB (KiB
   >     misread as MiB in the note string); its two sibling rows
   >     (threads=1 "~403 MiB", threads=32 "~400 MiB") are correct per the
   >     review. Exactly the data-hygiene class R31-12/item 27 spent Round
   >     31 repairing in R30-6's report.
   >   - **P2-9** — immutable source identity is produced AFTER measurement
   >     in all four R31 gate reports (each cites its own landing commit
   >     SHA; all four provenance JSONs record `git_dirty: true` against the
   >     pre-task base) — the review states CLAUDE.md's R30-9 point 7
   >     requires the identity to be produced BEFORE measurement, from
   >     something that exists AT measurement time, and a landing commit
   >     assembled after the fact assumes without proving that the measured
   >     working tree equals the eventually-committed tree. The review calls
   >     this a round-wide inherited pattern, strictly stronger than the
   >     R27-3/R27-4 baseline the original rule was written against.
   >     Suggested fix per the review: one `git write-tree` (or `git diff |
   >     sha256sum`) immediately before each measurement run, cited alongside
   >     the landing SHA.
   >   - **P2-10** — intra-round doc drift: R31-2's own NEW comments
   >     reference `Profile::Throughput` (`Cargo.toml:1792`,
   >     `docs/perf/OPEN_ITEMS.md:123` as it existed when R31-2 landed),
   >     which R31-9 removed later in the SAME round — a fresh stale
   >     reference introduced and then outdated within one round, not
   >     inherited debt (the review separately notes `Cargo.toml:1774`
   >     carries the same stale reference inherited from R30-7). Cosmetic
   >     per the review.
   > - **Next trigger:** independent re-verification of each sub-finding
   >   against its cited source (the raw CSV files directly for P2-1/P2-2/
   >   P2-8, the report prose + raw logs for P2-3/P2-7, the provenance JSONs
   >   + `git log` for P2-9, `Cargo.toml`/`OPEN_ITEMS.md` grep for P2-10),
   >   then either apply the review's suggested fixes or record a reasoned
   >   dismissal, in a future round. None of these change any Round 31
   >   P0/P1 verdict per the review's own text (§0: "nothing shipped a wrong
   >   number").
   > - **Evidence:** `docs/reviews/2026-07-31-r31-full-review.md` §7 P2-1,
   >   P2-2, P2-3, P2-7, P2-8, P2-9, P2-10 (the review's own text is the
   >   only source cited here — this entry is a filing, not an independent
   >   confirmation). P2-4 through P2-6, P2-11, P2-12 are filed separately
   >   in `docs/CORRECTNESS_OPEN_ITEMS.md` item 9 as the correctness-side
   >   counterpart (P2-6 fixed directly, not filed), per this file's own
   >   scope boundary with that sibling index.
   > - **2026-07-31 update (R31-14a, task #483): P2-1, P2-2, P2-8, P2-10
   >   independently RE-VERIFIED (not merely re-stated) against the actual
   >   committed files and REPAIRED.**
   >   - **P2-1** — confirmed exactly: `awk -F',' '{c[NF]++}'` over
   >     `R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE_summary.csv` before any
   >     edit reproduced 49 rows at 24 fields + 4 rows at 16 fields;
   >     confirmed in `scripts/r31_0_summary.mjs` that `RETENTION_HEADER` was
   >     defined but never written. Fixed by routing the 4 retention rows to
   >     their own file (`..._retention.csv`, under `RETENTION_HEADER`)
   >     instead of interleaving them into the 24-column file. Re-running the
   >     script against the already-committed raw logs (no re-measurement)
   >     regenerated both files; the script's own hard-assert (all 4
   >     `notouch`/virgin headline deltas within 0.1pp of the published
   >     report) PASSED — no data value changed. Dated correction appended to
   >     `R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md` §7.
   >   - **P2-2** — confirmed exactly: the report's own §2.2 states
   >     `SMALL_ZERO_PASS_CALLS` is never incremented on the OFF binary; the
   >     pre-fix CSV's 24 OFF-binary rows nonetheless carried numeric
   >     `mean_act_pct`/`min_act_pct` (100.00 or 0.00) with only `oracle=NA`
   >     signaling the vacuity. Fixed in `scripts/r31_0_summary.mjs`:
   >     `fmtVirginRecycled` now emits `NA` in both columns whenever
   >     `oracle == "NA"`; all 24 ON-binary rows are unchanged. Same dated
   >     correction, `R31_0_..._GATE.md` §7.
   >   - **P2-8** — confirmed exactly: 410,112 KiB / 1024 = 400.5 MiB, not
   >     ~410 MiB; the CSV row's note string self-contradicted ("~410 MiB/heap
   >     (400.5 rounded)"). The report body itself (§4.2 table, §4.3 trend)
   >     already stated 400.5 MiB correctly — only the CSV note string was
   >     wrong. Fixed: the note string corrected in
   >     `R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE_summary.csv` with an
   >     inline `CORRECTED 2026-07-31` marker (the `value`/`unit` columns were
   >     always correct, unchanged); dated correction also appended to
   >     `R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md` §7.
   >   - **P2-10** — confirmed, with corrected scope: `grep -rn
   >     "Profile::Throughput" Cargo.toml docs/perf/OPEN_ITEMS.md` found the
   >     two live/prescriptive mentions the review named
   >     (`Cargo.toml:1792`/`1774`, `OPEN_ITEMS.md`'s item-13 "Next
   >     trigger"/dated-addition text) plus several MORE mentions inside this
   >     file's own already-dated historical narrative blocks (items 26/27's
   >     "Current state" quoting past report text, item 33's own restatement
   >     of the review) — those are legitimately preserved history describing
   >     what was true when written, not live stale references, and are left
   >     as-is. Fixed the two live spots: both `Cargo.toml` comments
   >     (R30-7's original + R31-2's own new one) now name
   >     `SmallPoolPolicy::Throughput`/`LargeCachePolicy::Trimmed64MiB` with
   >     an inline correction note; item 13's "Next trigger" and its R31-2
   >     dated-addition text similarly corrected in place (this file is an
   >     explicitly mutable living index per this file's own convention, so
   >     no dated-addendum gate-report ceremony was needed here — a direct
   >     in-place fix with a one-line "corrected" note was sufficient, same
   >     treatment R31-9 already gave this file's item 26/27 blocks for the
   >     identical rename). P2-3, P2-7, P2-9 remain open for a future round
   >     (tracked as task #484).
   > - **2026-07-31 update (R31-14b, task #484): P2-3, P2-7 independently
   >   RE-VERIFIED (not merely re-stated) against the actual committed
   >   reports and REPAIRED. P2-9 remains open (out of this task's assigned
   >   scope — task #484 was scoped to P2-3, P2-4, P2-5, P2-7, P2-11, P2-12
   >   only).**
   >   - **P2-3** — confirmed exactly: `docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md`'s
   >     pre-fix §3/§5 prose cited four specific figures
   >     ("−91%/−97%/−99%/−99%") from a third ad-hoc re-run explicitly
   >     labelled "not saved as a cited raw log"; confirmed no such raw log
   >     exists alongside the two already-committed `_run2` pairs. Committing
   >     a log for a run that was never saved is not possible after the fact
   >     without re-measuring (which would produce a fourth number, not
   >     recover the original third run), so the qualitative-statement path
   >     was chosen per the task brief's own guidance. Fixed: both citations
   >     now state the third re-run "corroborated the same direction and
   >     order of magnitude" without restating the four specific
   >     percentages; the `_run2` figures that DO have a committed raw-log
   >     artifact are unchanged. Dated correction appended to
   >     `R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md` §8.
   >   - **P2-7** — confirmed exactly: `grep -c '^[0-9]'
   >     docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE_summary.csv` returns
   >     12 (section-1 data rows, each a median of 3 reps), not 36; the
   >     36-row count is real but lives in
   >     `docs/perf/_raw_r30_6_large_cache_headroom_ab_gate.log` (confirmed
   >     against `scripts/r31_12_repair_r30_6_data.mjs:56`'s own hard-assert
   >     `expected 36 rows in R30-6 raw log`), which R31-1's prose
   >     misattributed to "R30-6's own committed CSV." Fixed: the citation
   >     in `docs/perf/R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE.md`
   >     now reads "R30-6's own raw log... 36 rows" with an inline
   >     correction marker; no measured value or verdict changed. Dated
   >     correction appended to
   >     `R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE.md` §5.
   >   - P2-4, P2-5, P2-11, P2-12 (filed in `docs/CORRECTNESS_OPEN_ITEMS.md`
   >     item 9, not this file) were independently re-verified and fixed in
   >     the same task — see that file's own dated update for detail.

38. **R32-1 (task #492) — tiered/partial `trim_current_thread()` design
    (`TrimOptions`/`TrimReport`-shaped API: flush-tcache-only / drain-small-
    pool-to-a-target / trim-large-cache-to-a-budget-headroom, instead of the
    current all-or-nothing `evict_all` semantics).**

    Explicitly scoped OUT of task #492 (which shipped the passive no-bind
    resolver fix and the cost-side gate, both required deliverables) as an
    optional stretch goal too large to build soundly in the same task — per
    the task's own explicit instruction not to half-build a partial/unsound
    API rather than scope it down. No design doc, no code, no tests exist
    for this yet; `docs/design/R30_7_TRIM_SCAVENGE_API_DESIGN.md` §3.4
    already named a "headroom-floor variant" as explicitly out of scope at
    design time, so this item is that same identified gap, now also
    including the tcache-only/pool-to-target axes task #492's own brief
    named.

   > **Current state**
   > - **Status:** not started — no design doc, no code.
   > - **Current number/verdict:** N/A — nothing measured yet; this is a
   >   scope note, not a rejected idea.
   > - **Next trigger:** a future round chooses to spend a full task on the
   >   design-first pass (mirroring `R30_7_TRIM_SCAVENGE_API_DESIGN.md`'s own
   >   design-before-code discipline) — concrete signatures, explicit
   >   invariants for what "trim to a budget-headroom" means when the cache
   >   is already below that headroom, and an honest scope of what it does
   >   NOT claim, before any implementation.
   > - **Evidence:** `docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md` §5
   >   (the cost-side gate task #492 DID complete — its ~83.3× cold-start
   >   penalty finding is the concrete motivating data a tiered
   >   API would exist to soften, since a partial trim could avoid paying
   >   the full re-materialisation cost when only partial headroom is
   >   actually needed).
   > - **BENCH-REVIEW CROSS-REF (2026-08-04, R34-2/task #521):** the
   >   `docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md` §7
   >   names this item's exact API shape — `trim_current_thread_to_headroom(bytes)`
   >   — and quantifies the cliff it would soften: full `trim_current_thread`
   >   ≈ 24.2 ms pause for 4×32 MiB, next burst ≈ 65.2 ms vs ~0.8 ms without
   >   trim (R31-10's ~83× cold-penalty). Evidence needed to turn this into a
   >   real task: a design-first pass with a time/segment budget, oldest/largest-
   >   first release policy, explicit "released bytes / pause / refill penalty"
   >   telemetry, and a gate showing substantially less than the 24 ms / 65 ms
   >   cliff at controlled RSS — NOT another synthetic ceiling.

40. **R30_7 CSV-naming mismatch — `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md`
    cites `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_summary.csv` (missing the
    `_GATE` suffix); same defect class as R32-4/R32-5 (F8), left unfixed when
    R33-11's check (h) surfaced it.**

    > **Current state**
    > - **Status:** [A] active — trivially fixable; filed because R33-11's
    >   `verify-gate-report.mjs` check (h) surfaced this third instance of the
    >   same-base-name defect class but R33-11 (task #516) declined to rename it
    >   unasked (it was pre-existing, not introduced by R32). The Round-33
    >   review's G4 [P3] notes the CHANGELOG rounded all three check (h) warnings
    >   off as "legitimate cross-references" when one of the three is not.
    > - **Current number/verdict:** `ls docs/perf/ | grep R30_7` shows exactly one
    >   report and one CSV whose basenames differ only by the missing `_GATE` —
    >   i.e. this is the report's OWN companion, misnamed (not a legitimate
    >   cross-reference to another report's CSV).
    > - **Next trigger:** rename `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_summary.csv`
    >   → `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE_summary.csv` and update
    >   its two citations — a one-commit fix. The lowest-effort task in this index.
    > - **Evidence:** `docs/reviews/2026-08-03-round33-readonly-review.md` §6 G4;
    >   `docs/reviews/2026-08-04-release-stabilization-audit.md` (confirms the
    >   mismatch persists at audit time).

41. **R33-8's live-`git rev-parse HEAD` fallback silently emits the PARENT
    commit for a new report generated inside its own landing commit — a
    convention gap, not yet codified.**

    > **Current state**
    > - **Status:** [A] active — convention needs documenting; the one observed
    >   instance (R33-12's CSV `doc_commit` = parent `f51ec37`, corrected to
    >   landing `96ae245` in R34-2/task #521) is fixed, but the underlying
    >   mechanism will recur silently on every future same-commit report.
    > - **Current number/verdict:** R33-8 (task #513, commit `b537770`) replaced
    >   the loud `'UNFILLED_PLACEHOLDER_40_HEX'` sentinel with
    >   `process.argv[2] || execSync('git rev-parse HEAD')`. For HISTORICAL CSVs
    >   this is strictly better (15/15 re-derive CLEAN, Round-33 review §5). But
    >   for a NEW report generated inside its own landing commit, `git rev-parse
    >   HEAD` returns the pre-commit parent — a plausible 40-hex SHA that passes
    >   check (b) (40-hexness) and check (g) (sentinel-scan), so nothing detects
    >   the off-by-one. R33-12 was the first new report after the change and
    >   exhibited exactly this (`doc_commit` = `f51ec37` = parent of `96ae245`).
    > - **Convention (decided R34-2/task #521):** for a NEW report, the
    >   recommended sequence is R33-6's pattern — commit the harness/example
    >   FIRST (`5bd7c04`), measure at that HEAD, then commit the report — so
    >   `git rev-parse HEAD` at derive time is already the correct (harness)
    >   commit, not a pre-report parent. If a same-commit report is unavoidable,
    >   pass the eventual landing SHA explicitly as `argv[2]` in a follow-up
    >   correction commit (the old workflow, now without the sentinel). The one
    >   combination to avoid is a `landing_commit`/`doc_commit` column populated
    >   by the `git rev-parse HEAD` fallback inside a report's own landing commit
    >   — that is the off-by-one state.
    > - **Next trigger:** a future round either (a) codifies this convention in
    >   CLAUDE.md's R14-10 summary-CSV section (one sentence: "for a new report,
    >   commit the harness first or pass the SHA explicitly"), or (b) adds a
    >   check to `verify-gate-report.mjs` that flags a `doc_commit`/`landing_commit`
    >   equal to `HEAD^` (the parent) — cheap to compute, catches the exact
    >   off-by-one class.
    > - **Evidence:** `docs/reviews/2026-08-03-round33-readonly-review.md` §5 G3;
    >   R34-2/task #521's correction of `R32_3_REALLOC_REDUNDANT_CONTAINS_BASE_GATE_summary.csv`'s
    >   `doc_commit` (`f51ec37` → `96ae245`, via re-running
    >   `scripts/r32_3_realloc_redundant_contains_base_summary.mjs 96ae245…`).

42. **R32-8's `DECAY_CLOCK_CHECK_STRIDE = 64` retention bound does NOT hold
    over consecutive sparse decay intervals — PARTIALLY RESOLVED by R34-11
    catch-up loop; peak gap still stride-bound.**

    > **Current state**
    > - **Status:** [P] partially resolved — R34-11 (task #530) added a bounded
    >   catch-up loop (`DECAY_CATCHUP_MAX_STEPS = 8`) that substantially reduces
    >   the gap persistence and final gap. The PEAK gap (4 segments at
    >   events=1) remains stride-bound (the throttled arm cannot read the clock
    >   until op 64 ≈ interval 30); closing the peak would require an adaptive
    >   stride, which is a separate, more complex change.
    > - **Current number/verdict (R34-11, task #530):** at events=1/interval
    >   (the R34-10 primary case), the catch-up loop drops the FINAL gap from
    >   3 → **1 segment** (67% reduction), persistence at ≥3 segments from
    >   95.0% → **72.5%** of the run, and total released from 1 → **3
    >   segments** (from 25% to 75% of the unthrottled arm's 4). R32-8's
    >   throughput benefit is preserved (67.6%, consistent with the original
    >   ~61% — the catch-up loop is never reached in high-throughput). See
    >   `docs/perf/R34_11_CATCHUP_DECAY_GATE.md` for the full gate report.
    > - **Remaining:** the peak gap of 4 segments (16 MiB) opens at interval 2
    >   and persists through interval 29 because the throttled arm cannot read
    >   the clock until op 64. An adaptive stride (read the clock more often
    >   when events are sparse) would close the peak; the catch-up loop alone
    >   cannot. Filed as a potential future task.
    > - **Evidence:** `docs/perf/R34_11_CATCHUP_DECAY_GATE.md`;
    >   `docs/perf/R34_11_CATCHUP_DECAY_GATE_summary.csv`;
    >   `examples/r34_11_catchup_decay_gate.rs`; source-identity tree
    >   `8b657703084f10aeadebe52f3302b63a965eac5a`.
    >   Prior (R34-10): `docs/perf/R34_10_SPARSE_DECAY_GATE.md`;
    >   `docs/perf/R34_10_SPARSE_DECAY_GATE_summary.csv`;
    >   `examples/r34_10_sparse_decay_gate.rs`; source-identity tree
    >   `bb67abc538d5570e45fba42d8613470838934a2f`.

48. **`aligned-vmem`: 64-bit Unix `reserve` over-reserves `align` bytes of VA per reservation.**

    > **Current state**
    > - **Status:** design-only, deferred — reasoned-but-unmeasured performance idea.
    > - **Current number/verdict:** NEED-MEASUREMENT — no measured victim exists today. On 64-bit Unix, `crates/aligned-vmem/src/os/unix.rs`'s `unix_reserve` (task #944/P-1 compiled out the exact-size fast path) usually over-reserves `size + align` bytes in one `mmap` call and keeps the whole mapping, except on Linux when `align == LINUX_HUGE_PAGE_SIZE` with huge pages requested, where an exact-size `MAP_HUGETLB` fast path avoids the over-reserve (kernel guarantees huge-page-aligned base). The cost of over-reserving is extra virtual address space held per reservation (cheap for small aligns like 4 KiB, larger for big aligns like 4 MiB). The 32-bit exact-size fast path (`try_reserve_aligned_exact` in `crates/aligned-vmem/src/os/unix.rs`) shows the shape of a possible fix: try an exact-size `mmap` first, check if the base is already `align`-aligned (hit), and fall back to the over-reserve on a miss. However, porting this to 64-bit is a real measured-tradeoff decision, not a one-line fix: on a miss, the fast path costs 3 syscalls (mmap + munmap + over-reserve mmap) vs the current flat 1 syscall. The break-even analysis between syscall savings (on hits) and retry cost (on misses) needs a real measured gate before implementation — the same class of question that led to the 32-bit fast path's own retreat-to-64-bit-removal (see `unix_reserve`'s own doc comment in `crates/aligned-vmem/src/os/unix.rs` for the documented reasoning).
    > - **Pool-cost addition (2026-08-18, task #1069, F9 half 1 of `docs/reviews/2026-08-17-aligned-vmem-fxx-audit.md`):** this card's VA-only pricing understates the case where Linux huge pages are actually GRANTED through the over-reserve path — every granted `align > LINUX_HUGE_PAGE_SIZE` request (the II-4 exact-size fast path covers only `align == 2 MiB`; II-4's fix is the closed MEDIUM that landed as commits `539e1ae`/`a088a0c`), plus an `align == 2 MiB` request whose fast-path attempt missed. `libc_mmap` passes no `MAP_NORESERVE`, and Linux reserves hugetlb pool pages for a private `MAP_HUGETLB` mapping's entire length at `mmap` time, so the whole `size + align` span — including the exactly-`align`-byte slack — is charged against the bounded `nr_hugepages` pool for the reservation's lifetime, not merely held as cheap VA. For `size == align == 4 MiB` that is 4 pool pages charged per segment for 2 needed (2×, II-4's own "up to 2x" arithmetic; the F9 audit's "roughly 33%" headline is arithmetically wrong — the slack is 50% of the charge in that shape). Still deliberately not changed, same verdict as above: a head/tail trim would be provably munmap-conformant for this case (the `unix_reserve` guard forces 2-MiB multiples and the kernel guarantees a 2-MiB-aligned base, so every trim boundary is huge-page-aligned), but re-adding trims partially reverses task #842's deliberate keep-whole-mapping soundness design, and the win is unmeasurable on any host this project has. The pool dimension is now documented at the code site (`unix_reserve`'s keep-whole-mapping comment) and in `reserve_aligned_huge`'s rustdoc.
    > - **Next trigger:** a round with access to a 64-bit Unix target and a reservation-heavy workload pattern that demonstrates `align`-amplified VA pressure is a real problem (not merely theoretical), measured via `aligned-vmem`'s `UNIX_EXACT_RESERVE_HITS`/`_ATTEMPTS` bench-internals counters to determine the actual hit rate of the huge-page exact-size path on 64-bit (these counters track the huge-page path on 64-bit, not the general over-reserve path). For the POOL dimension specifically, the trigger additionally requires a hugetlb-configured host (`nr_hugepages > 0`), which correctness item 59 records as absent from this project's CI.
    > - **Evidence:** `unix_reserve` doc comment in `crates/aligned-vmem/src/os/unix.rs` (32-bit fast path gating), `try_reserve_aligned_exact` in `crates/aligned-vmem/src/os/unix.rs` (32-bit exact-size fast path implementation), `unix_reserve` in `crates/aligned-vmem/src/os/unix.rs` (64-bit over-reserve path). Filed from `docs/reviews/2026-08-16-aligned-vmem-fxx-prerelease-audit.md`, Part I finding 4 (P2-4); pool-cost addition from `docs/reviews/2026-08-17-aligned-vmem-fxx-audit.md` F9 (task #1069), which re-raises II-4's residual `align > 2 MiB` half.

### [L] Low-priority — "honest reject" with a documented revisit trigger

7. ~~**R14-5 §4 — dedicated timing gate for O(40) vs O(8) Large-cache scan on a
   narrow working-set-after-burst shape.**~~

   > ~~**Current state**~~
   > ~~- **Status:** deferred, low-priority.~~
   > ~~- **Current number/verdict:** deferred — no number attached yet to the O(40) vs O(8) "cheap" claim for N=1/2/4.~~
   > ~~- **Next trigger:** a future review wants a number for the narrow working-set-after-burst shape (R13-8 already measured the 24-distinct-size turnover shape).~~
   > ~~- **Evidence:** `R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` (lines 240–248).~~
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L7`.

   > **MERGED — 2026-08-02 (task #489, ledger reconciliation):** this item
   > asked for exactly the number item 30 (`[A]` section above) now
   > supplies — the requested N=1/2/4 narrow-working-set-after-burst timing
   > number for the O(40) vs O(8) scan bound now exists (task #488, commit
   > `4f89723`): **NO-GO, ON is measurably slower at every N**, not the
   > "cheap"/negligible outcome this item's phrasing anticipated as a
   > possibility. This item is no longer separately tracked as deferred —
   > item 30 above is the single current source of truth for this question
   > going forward (its own card carries the full corrected verdict, the
   > two prior invalid-numbers episodes, and the pointer to
   > `R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md` §3.4/§8). Struck
   > through rather than deleted, per this file's append-only convention
   > (rule 2: "Do NOT delete the entry").

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
    > - **Cross-reference (added 2026-07-31, task #479):** this item's trigger
    >   is one instance of the shared "no realistic ≥64-segment / high-fan-in
    >   macro-bench exists" precondition that item 34 below consolidates
    >   across four independently-filed items (this one, X5/item 20,
    >   T10/item 22, R1/item 23) — see item 34 for the canonical statement;
    >   this entry's own history and evidence stay here unchanged.
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
    > - **Status:** low-priority, optional curiosity probe (~1 hour if ever revisited) — **CORRECTED (R34-review F2/task #548, 2026-08-05): the ratio this verdict cited is stale; still low-priority, but on weaker grounds than previously stated (see below).**
    > - **Current number/verdict:** the item's original reasoning ("the sub-16 KiB tail is already cheapest, so marginal payoff is small") rested on `realloc_grow_geometric` being **~40× faster than mimalloc** via OPT-G's Large in-place grow. R34-23 (`docs/perf/R34_23_REALLOC_AND_VEC_GATE.md`, task #542, same-round finding) re-measured that exact bench and found the ~40× figure (sourced from a 9.7 µs sefer number) "physically impossible" — a 2 MiB copy alone takes ~50-100 µs, and the chain's final grow step forces one. The corrected ratio is **~1.8× (criterion, ~238 µs sefer vs ~431 µs mimalloc) to ~2.1× (direct gate, ~210 µs vs ~444 µs)** — off by roughly 20× from what this card asserted. **Re-deriving the verdict, not just the number:** the corrected ratio *weakens* the original argument — a 40× margin over mimalloc plausibly meant sefer's realloc/move-leg machinery was already near-optimal, making a new sub-16 KiB fast path low-value by analogy; a ~2× margin means far less headroom has already been captured by existing mechanisms, so the analogy no longer supports "already cheapest, nothing to gain." That said, the verdict does **not** flip to high-value either: item 12's OWN decisive datum was never the OPT-G ratio, it is the sub-16 KiB ladder's Stage-1 hit rate, which the item's evidence explicitly says is "currently-unmeasured" (plausibly 20-50% by the LCM argument, but never actually run) — a different, independent gap that the OPT-G correction does not touch. Net: **downgrade from "confidently low-value" to "genuinely unmeasured, still low-priority pending that measurement"** — the honest posture is closer to NEED-MORE-DATA-if-ever-revisited than to a settled low-value reject; the ~1-hour-probe framing and optional/low-priority tier stand, but no longer because the payoff was shown small — because it was never measured and the stated reason to skip measuring it does not hold up.
    > - **Next trigger:** none named as *urgent* (still optional, low-cost) — unchanged from before this correction. If ever revisited, the probe is the same one already named: a Vec-push-shaped 16 B→16 KiB hot-buffer harness measuring the sub-16 KiB ladder's own Stage-1 hit rate directly (not inferred from the unrelated OPT-G/Large-grow ratio).
    > - **Evidence:** `R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §5.3 + this file's item-1 closure entry (the LCM argument distinguishing the two ladders) + `docs/perf/R34_23_REALLOC_AND_VEC_GATE.md` §5 (the ~40×→~1.8-2.1× correction that prompted this re-derivation).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L12`.

50. **Follow-up from commit 66b8508 (task #1030): aligned-vmem package gates appear AFTER root test rows, so a root-test failure masks them.**

   > **Current state**
   > - **Status:** OPEN — cheap clippy rows sit AFTER four expensive root test rows in `scripts/check-all.mjs`, so a root-test failure causes the script to stop before reaching the aligned-vmem gates, masking their failure state.
   > - **Current number/verdict:** structurally observable — the aligned-vmem group runs AFTER the four root-test rows (steps 8-11). If any root test fails, `npm run check` exits immediately and never reports whether the aligned-vmem clippy rows would have passed. The group is now **16 steps at runtime indices 12-27** (task #1047 grew it from 6 by adding a default-feature test row, a warnings-as-errors doc row and an optional semver row; task #1071 added the two invalidation cleans, a bench-internals-only cross-target clippy row and the mock clippy row; task #1082 added the mock test row — this card's "9 steps at indices 12-20" figure had itself gone stale, since #1071's six-step growth was never reflected here); the "14-19" figure this card carried until 2026-08-17 was doubly wrong — stale in size AND never right in position, since the group has sat at 12 onward, before the two PER_PR_ROWS rows, ever since commit 66b8508 introduced it.
   > - **Next trigger:** reorder the `steps` array in `scripts/check-all.mjs` so cheap clippy checks run BEFORE expensive test rows — placing the aligned-vmem clippy rows before the four root-test rows would unmask both failure classes in a single run.
   > - **Evidence:** commit 66b8508 body ("Two follow-ups this commit does NOT do, recorded so they are not lost: (1) the new group sits AFTER the four root test rows, so a root-test failure masks it -- cheap clippy rows would be better placed before expensive test rows"); `scripts/check-all.mjs` current structure (aligned-vmem steps at runtime indices 12-27, root-test steps at 8-11; re-derived from the `steps` array itself on 2026-08-18 for the 16-step group, not copied from the file's own header comment, which had the group's position transposed).
   Full history: this entry (filed 2026-08-16, task #1030 follow-up).

52. **Linux/Android huge exact-reserve failure followed by a second huge attempt — NULL verdict, twice. Recorded so a third review does not re-raise it.**

   > **Current state**
   > - **Status:** NULL — deliberately NOT changed, after two independent reviews raised it (R6-8 → task #1040, commit `84bc9ac`; R7-4 → task #1048, which re-confirmed the verdict rather than re-litigating it).
   > - **Current number/verdict:** the finding observes that for `huge && align == 2 MiB`, an exact `mmap(size, MAP_HUGETLB)` that returns NULL is followed by the general path's `mmap(size + align, MAP_HUGETLB)` before the ordinary fallback — two failing huge attempts per logical reserve on a host with no hugetlb pool. The premise "the second attempt is guaranteed to fail too" is FALSE in general: the two calls request DIFFERENT sizes (`size` vs `size + align`), so a fragmented or bounded pool can satisfy one and refuse the other. Skipping straight to the ordinary fallback would trade a rare-but-real success for one saved syscall on an already-cold path. Both reports state the premise without addressing this.
   > - **Next trigger:** a syscall-count or latency measurement ON A LINUX HOST WITH AND WITHOUT a configured hugetlb pool, showing the saved syscall outweighs the lost success case. Until such a measurement exists, the answer stays NULL. Note the measurement is structurally unreachable from this project's current dev host (Windows) and CI (no hugetlb runner — correctness item 59), which is why it has not been produced.
   > - **Evidence:** commit `84bc9ac`'s body (the counterexample, written out in full); `unix_reserve` in `crates/aligned-vmem/src/os/unix.rs` (the exact-size attempt and the over-reserve attempt, with their differing size arguments); `docs/reviews/2026-08-16-aligned-vmem-prerelease-audit-r6.md` § R6-8 and `...-r7.md` § R7-4 (the two raisings).
   > - **Duplicate record (2026-08-18, task #1069):** half 2 of finding F9 (`docs/reviews/2026-08-17-aligned-vmem-fxx-audit.md`, "the pool-exhausted path also pays a guaranteed-doomed extra syscall") re-raised this exact observation, repeating the "guaranteed to fail for the same reason" premise the counterexample above refutes — the third raising, as this entry's title anticipated. Recorded as a duplicate; no change, verdict stays NULL.
   Full history: this entry (filed 2026-08-17, task #1048).

53. **R7-5 — 64-bit Unix `reserve` retains `size + align` of VA per reservation (aligned-vmem). INFO, deliberate trade-off, not a bug.**

   > **Current state**
   > - **Status:** OPEN as a recorded trade-off — deliberately NOT changed.
   > - **Current number/verdict:** the generic exact-size probe is compiled out on
   >   64-bit Unix (`#[cfg(all(unix, not(miri), target_pointer_width = "32"))]` on
   >   the exact-size helper, and the `#[cfg(target_pointer_width = "32")]` arm
   >   inside `unix_reserve`), so an ordinary 64-bit reservation keeps the whole
   >   over-reserved `size + align` mapping instead of trimming to `size`. That is
   >   syscall-optimal for the typical 64-bit case — one `mmap`, versus the exact
   >   probe's `mmap + munmap + over-reserve mmap` whenever the probe misses — at
   >   the cost of VA that a large `align` with many live reservations makes
   >   theoretically noticeable. The Linux huge exact-size exception does NOT
   >   cover this generic case.
   > - **Next trigger:** a 64-bit reservation-heavy workload measurement showing
   >   the VA pressure is real and outweighs the extra syscalls. The existing
   >   exact-path counters cannot answer it — they instrument the Linux huge
   >   attempt, not the generic 64-bit ordinary path — so a new counter or probe
   >   is part of the trigger. Needs a Linux host; structurally unreachable from
   >   this project's Windows dev host.
   > - **Evidence:** `crates/aligned-vmem/src/lib.rs`'s module doc — the sentence "On 64-bit
   >   Unix, the exact-size fast path is compiled out entirely" (the module doc
   >   stayed in `lib.rs` through task #1055's split; post-split update, task
   >   #1082) — and, in `crates/aligned-vmem/src/os/unix.rs` (post-split home of
   >   the implementation), `unix_reserve`'s 32-bit-gated exact arm and the
   >   `target_pointer_width = "32"` gate on the exact-size helper (cited by SYMBOL, not by line number: this round's own
   >   R7-9 finding is about stale line citations, and batch G's draft of this
   >   card cited `unix_reserve` at `:3258` when it is at `:3322` — both
   >   pre-split monolith line numbers, historical);
   >   `docs/reviews/2026-08-16-aligned-vmem-prerelease-audit-r7.md` § R7-5.
   Full history: this entry (filed 2026-08-17, task #1049).

54. **R7-10 — backend functions return unnamed tuples and `lib.rs` is monolithic (aligned-vmem). INFO, deferred to a future MAJOR.**

   > **Current state**
   > - **Status:** OPEN, deferred by decision — explicitly NOT for 0.2.0.
   > - **Current number/verdict:** one `lib.rs` holds the public API, three
   >   backends, the mock/cfg branches and the local FFI declarations. The
   >   private `RawReservation` struct already removes the `base`/`reservation`
   >   transposition risk AT CALL SITES (that is what its own doc comment says it
   >   exists for), but the backend boundary itself still hands back positional
   >   values. Not an error today; it raises audit cost and cfg-specific
   >   regression risk.
   > - **Post-split update (task #1082, 2026-08-18):** the "monolithic `lib.rs`"
   >   half of this item is now moot — task #1055 (commit `a4b8e50`) split the
   >   monolith into per-file modules (`src/os/*`, `src/api/*`,
   >   `src/reservation*.rs`, `src/bench_internals/*`), leaving `lib.rs` a
   >   ~210-line re-export surface, which also fulfils this card's own
   >   "split private `os_windows`/`os_unix`/`os_miri` modules" next trigger.
   >   The unnamed-tuple half REMAINS OPEN: the backend boundary
   >   (`reserve_aligned_raw` in `src/os/{unix,windows,miri}.rs`,
   >   `win_reserve_commit` in `src/os/windows.rs`) still returns positional
   >   tuples — exactly what `RawReservation`'s own doc comment in
   >   `src/api/internal.rs` still records ("the backend functions themselves
   >   still return unnamed tuples"). The deferral verdict is unchanged.
   > - **Next trigger:** planning a future major version — split private
   >   `os_windows`/`os_unix`/`os_miri` modules and return named raw results from
   >   the backend boundary. R7 states directly that mixing this refactor into
   >   0.2.0's release fixes is the wrong trade.
   > - **Evidence:** `crates/aligned-vmem/src/api/internal.rs` (post-split home
   >   of the struct, task #1082; `crates/aligned-vmem/src/lib.rs` at filing) —
   >   the `RawReservation` struct and
   >   its doc comment ("Private struct for raw reservation results from backend
   >   functions / Named to prevent transposing `base` and `reservation`"), cited
   >   by symbol: batch G's draft called those doc lines a "module doc" at
   >   `:1925-1931` and placed the struct at `:1924`, when the struct is at
   >   `:1932` (pre-split monolith line numbers, historical) and those lines are its OWN doc;
   >   `docs/reviews/2026-08-16-aligned-vmem-prerelease-audit-r7.md` § R7-10.
   Full history: this entry (filed 2026-08-17, task #1049).

49. **R4-5 — Windows huge-page speculative fast-path may miss expensively,
    and existing counters do not show syscall cost (aligned-vmem).**

    > **Current state**
    > - **Status:** deferred, low-priority — observability added; measurement of real-world alignment/privilege/size distributions on Windows workloads is required before any further speculative-window optimization.
    > - **Current number/verdict:** design-note filed — the `win_reserve_commit` fast-path threshold was extended to `GetLargePageMinimum()` to speculative-try large pages, but the existing `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS`/`WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS` counters only count SUCCESSFUL completions, not the syscall cost of failed large-page attempts. The retry fallback can incur up to two additional `VirtualAlloc` calls and one `VirtualFree` on alignment failure before falling through to the two-call path, but this syscall overhead was not observable. **Updated R5-4 (task #1028):** the failure surface is now split across TWO `bench-internals`-gated counters, because a single counter could not honestly cover both modes — `WINDOWS_LARGE_PAGE_RETRY_FAILURES` (`aligned_vmem::windows_large_page_retry_failures()`) counts ONLY the case where the initial large-page `VirtualAlloc` failed AND the ordinary-page retry ALSO returned NULL, and `WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES` (`aligned_vmem::windows_large_page_alignment_failures()`) counts the case where an allocation succeeded but returned a misaligned base, forcing a `VirtualFree` and fallthrough. Before R5-4 the second mode was documented as covered but never actually incremented (its guard tested `!huge_granted`, which is false exactly when the initial large-page attempt succeeded).
    > - **Next trigger:** measure real-world alignment/privilege/size distributions on target Windows workloads; ONLY if **the SUM of `WINDOWS_LARGE_PAGE_RETRY_FAILURES` and `WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES`** over `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` shows a material failure rate should the speculative window be narrowed or removed. Reading either counter alone under-reports the speculative window's true cost. The current threshold (`GetLargePageMinimum()`) remains in place — this is observability-only, not a premature optimization removal.
    > - **Evidence:** `docs/reviews/2026-08-16-aligned-vmem-independent-prerelease-audit-r4.md` (finding R4-5); `docs/reviews/2026-08-16-aligned-vmem-prerelease-audit-r5.md` (finding R5-4, which caught the never-incremented second mode); `crates/aligned-vmem/src/bench_internals/windows.rs` (post-split home, task #1082, of both counter declarations `WINDOWS_LARGE_PAGE_RETRY_FAILURES` and `WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES` and of accessors `windows_large_page_retry_failures()` / `windows_large_page_alignment_failures()`; the two distinct increment sites are in `win_reserve_commit` in `crates/aligned-vmem/src/os/windows.rs` — the both-returned-NULL branch and the post-call alignment check respectively; both counters are reset in `reset_bench_internals_counters()` in `crates/aligned-vmem/src/bench_internals/reset.rs`); `crates/aligned-vmem/tests/bench_internals_counters.rs` (diagnostic-surface + reset test); `crates/aligned-vmem/Cargo.toml` (bench-internals feature documentation).
    Full history: tasks #1022, #1028.

44. **R25-3 — `FLUSH_N` sweep (4/8/12/16) at fixed `TCACHE_CAP`=16.**

    > **Current state**
    > - **Status:** NO-GO, fully explored — all 4 swept values measured against all 5 required gates; none beats the current baseline (`FLUSH_N=8`).
    > - **Current number/verdict:** `FLUSH_N=16` shows the only gate-1 (bulk-free Ir) win (−1.5% at N=1024) but triggers the kill condition on gate 3 (oscillating live-set): 2.42× Ir regression, 20× refill-event-count regression (1→20 refills per 20 rounds), independently confirmed via both an Ir judge and a native tcache-hit-rate counter. `FLUSH_N=4`/`FLUSH_N=12` show no gate-1 win at all (+14.4%/+0.7%) with gate 2/3 also flat-or-worse.
    > - **Next trigger:** none named as promising — a genuinely different mechanism (not a half-flush-RATIO tuning) would be needed; R25-8's conditional run-encoded free-batch design study is the only currently-planned follow-up touching this region, and it is a different mechanism, not a FLUSH_N retune. If `FLUSH_N=16` (or any `FLUSH_N == TCACHE_CAP`) is ever revisited for any reason, first fix the independent `virgin_mask >>= FLUSH_N` compile-time overflow this task found at that exact boundary (release-profile-only; `cargo check` in dev profile does NOT catch it) — see the report §6.
    > - **Evidence:** `R25_3_FLUSH_N_SWEEP_GATE.md` (full report); `R25_3_FLUSH_N_SWEEP_GATE_summary.csv`.
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L13`.

16. **R27-11 — reservation-only overflow tier (MOVED here from `[D]` item 15;
    R29-3/task #434). Trigger (b) FIRED and ANSWERED by R32-13/task #504 —
    still does not open the design.**

    > **Current state**
    > - **Status:** honest reject — NOT recommended; trigger 2 measured and does NOT fire (Linux). Trigger (b) below (Windows) fired R32-13/task #504 and ALSO does not fire.
    > - **Current number/verdict:** LINUX (R29-3): (1+2+3) avoidable = ~24K ns = **1.0-1.3%** of
    >   the decommit→reserve segment-lifecycle cycle (across 2 saved runs); (4+5)
    >   irreducible page-fault cost = **98.7-99.0%**. Additionally, `MADV_DONTNEED`
    >   decommit costs ~196-217K ns — MORE than the entire avoidable overhead — so
    >   the reservation-only design would be a NET LOSS on Linux (per-page PTE walk
    >   of 1,006 pages > bulk VMA teardown of `munmap`). WINDOWS (R32-13, task
    >   #504, native measurement — first Windows-native artifact in this corpus):
    >   avoidable share is **4.3-4.8% (median 4.60%)** across 3 runs — LARGER than
    >   Linux's 1.0-1.3% (mechanistically explained: Windows pays 2 real syscalls
    >   per reserve+commit, `VirtualAlloc(MEM_RESERVE)` then `VirtualAlloc(MEM_COMMIT)`,
    >   plus a 2x VA over-reservation Linux's single eager `mmap` avoids) but STILL
    >   well under the 20% materiality threshold — page-fault cost dominates at
    >   95.2-95.7% on Windows too. A NEW finding along the way: on Windows,
    >   `VirtualAlloc(MEM_COMMIT)` costs ~2x MORE than `VirtualAlloc(MEM_RESERVE)`
    >   (median 9,133 ns vs 4,580 ns, consistent across all 3 runs) — the OPPOSITE
    >   of what a "reserve must search/carve, commit is just accounting" mental
    >   model would predict.
    > - **Next trigger:** trigger (a) (segment size shrinks dramatically) remains
    >   untested. Trigger (b) (Windows) is now CLOSED — fired, measured, does not
    >   change the verdict. No new trigger opened by R32-13; the design stays
    >   deferred on both platforms now measured.
    > - **Evidence:** `R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md` (the
    >   Linux decomposition) + `R32_13_WINDOWS_RESERVE_COMMIT_DECOMPOSITION_GATE.md`
    >   (task #504, the Windows decomposition + reserve-vs-commit split +
    >   `_summary.csv` + `_raw_r32_13_run{1,2,3}.log`).
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
    > - **Next trigger:** per the section's own text — "If a future arc revisits, the trigger should be a REAL-application cache profile (not microbenches) showing SIZE2CLASS lines contending." (The clz implementation and the exhaustive differential test are recoverable from the source section's description.) **NARROWED (2026-08-03, F5/task #505):** the original wording is superseded — a "real-application cache profile showing SIZE2CLASS lines contending" overstates the risk, because the index (`(size-1) >> 4`) is dense from zero, so the table's hot region is exactly as small-size-dominated as the workload itself: sizes ≤1 KiB touch only 1 cache line, ≤4 KiB touch 4, ≤64 KiB touch 64 — the full ~15.8 KiB footprint is reached only by a workload dominated by *large, widely-scattered* small-class sizes (tens of KiB apart, spread across many distinct 1 KiB-wide index bands), and such a workload is inherently allocation-rate-limited elsewhere (each op already moves ≥16 KiB of payload, so one extra L2 hit on a class lookup is noise). The narrowed trigger: **"a real application whose size distribution is dominated by scattered ≥16 KiB small-class sizes"** — not any cache profile naming SIZE2CLASS, only one with that specific distribution shape. A hybrid variant (direct-indexed LUT for small sizes + clz-computed for large, mirroring mimalloc's `pages_free_direct[]` + `_mi_bin()` split) was considered and explicitly NOT recommended even if the trigger fires: per the density argument, the ~15.75 KiB the hybrid would remove is exactly the part that was never hot, so its expected win is ≈0, while it adds a branch on `size` to the hottest path in the allocator — the shape X4-B's "won-front" rule (item 18 above) already rejects.
    > - **Evidence:** `docs/perf/IAI_BASELINE.md` "X6 honest-reject (2026-07-05)" section (lines 246–268); `docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` §F5 (2026-08-03, task #505 — re-assessment confirming REJECT still holds and "deader" than originally judged; trigger-narrowing rationale and the hybrid-variant analysis in full).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L19`.

20. **X5 (2026-07-05) — per-class segment-queue bitmap (cheapest variant).**

    > **Current state**
    > - **Status:** honest reject — NOT recommended *for the measured regime* (correctness-proven and recoverable; not a refutation of the idea).
    > - **Current number/verdict:** REJECT. The cheapest sound variant (a per-segment `u64` bitmap of non-empty classes, bit `c` set ⟺ `BinTable.head(c) != FREE_LIST_NULL`, maintained at every empty↔nonempty transition, consulted by `find_segment_with_free` instead of loading the BinTable head cache line) was implemented, correctness-proven by 8 dedicated regression tests (counterfactual-verified: disabling any one transition makes the invariant test FAIL), and measured: it regressed the designated judge (`multiseg_cold_256k` +273 Ir) AND the won front (the four churn benches **+9 Ir** each, just under the ±10 kill threshold; recycle +810; cold +400). Mechanism: at n=3 segments the maintenance RMW (load `free_classes`, OR/AND a mask, store) on every empty↔nonempty dealloc transition is a net cost, and the `free_classes` load sits in the SAME cache line as the header already read for `kind_at` — the "avoid a BinTable-line load" premise does not hold here (no extra cache line to avoid).
    > - **Next trigger:** per the section's own text — "a future arc that adds a ≥64-segment bench (or profiles a real application) may flip the verdict. The shape to revisit is the FULL per-class queue (skip non-matching segments entirely, not just a per-segment bit probe)." (The structural argument only materialises at n_segments ≫ 3, which no current bench models — `multiseg_cold_256k` spans only 3.)
    > - **Cross-reference (added 2026-07-31, task #479):** this item's
    >   "≥64-segment bench" trigger is now the CANONICAL wording for the
    >   shared precondition item 34 below consolidates across four
    >   independently-filed items (this one, T10/item 22, R1/item 23,
    >   R15-1/item 9) — see item 34 for the single cross-item statement; this
    >   entry's own history and evidence stay here unchanged.
    > - **Evidence:** `docs/perf/IAI_BASELINE.md` "X5 honest-reject (2026-07-05)" section (lines 270–365, full measurement table included there). Final tree after X5 = pristine `490974d` (zero diff; nothing shipped).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L20`.

21. **G1 (2026-07-10) — magazine double-free oracle fold into `AllocBitmap`.**

    > **Current state**
    > - **Status:** honest reject — NOT recommended; not implemented (zero diff for G1 specifically; task #50's other sub-parts landed independently).
    > - **Current number/verdict:** REJECT. Folding the in-magazine double-free scan into `AllocBitmap` requires *inverting* existing load-bearing optimizations at multiple call sites (not a free relabeling): `refill_class_bump_impl`'s freelist-drain leg + `refill_class_bump`'s bump-carve leg both call `mark_alloc` on a premise that becomes false once the destination can be the magazine instead of the user; `reclaim_offset_checked`'s cross-thread ring-drain path already runs `is_free(off)` PLUS a separate `is_in_magazine` O(count) scan specifically because today's bitmap is blind to magazine residency — folding residency into the bit would make `is_in_magazine` redundant, a real behavior change to the H1-adjacent cross-thread reclaim protocol. A single alloc can legitimately set up to 32 consecutive bits (1 requested + `REFILL_BATCH`=31 refilled), which the simple "set on push, clear on pop" framing did not account for. Measured: the magazine-hit benches targeted show **exactly 0.0 Ir/op delta** (no code changed). M2 counterfactual tests confirmed non-vacuous (temporarily broke → went RED as expected).
    > - **Next trigger:** per the section's own text — "the shape to try is NOT a simple bit redefinition but a design that (a) audits and updates every `mark_alloc`/`mark_free` call site's semantics consistently (the four sites named, at minimum), and (b) resolves whether `is_in_magazine`'s separate scan in `reclaim_offset_checked` becomes provably redundant or must be kept for the cross-thread case specifically — that analysis was not completed here … and is the actual blocker, not a fundamental soundness objection to the idea."
    > - **Evidence:** `docs/perf/IAI_BASELINE.md` "G1 honest-reject (2026-07-10)" section (lines 530–591).
    > - **Cross-reference (added 2026-08-02, task #497):** the non-semantics-
    >   changing sibling this item's own "next trigger" implicitly ruled out
    >   as sufficient (a shared-storage form that keeps both oracles'
    >   semantics independent) WAS attempted — see item 45 below (F1b,
    >   `docs/perf/R32_6_DUAL_BITMAP_GATE.md`). It confirmed the correctness
    >   distinction from G1 is real (no semantics inversion, all four named
    >   call sites unaffected in meaning) but was independently rejected on
    >   MEASURED COST: every bitmap-touching bench regressed past the ±10 Ir
    >   kill gate. So G1's specific semantics objection is closed as "not the
    >   only blocker in this region" — a correctness-safe merge is possible,
    >   it is simply not cheap enough to ship, for a different, orthogonal
    >   reason (single-plane call-site addressing cost).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L21`.

22. **T10 (2026-07-12) — per-class "last found segment" hint for `find_segment_with_free` (NO-GO, reverted).**

    > **Current state**
    > - **Status:** NO-GO, honest reject — reverted (one orthogonal sub-finding KEPT; see below).
    > - **Current number/verdict:** NO-GO. A per-class `find_hint: [u16; SMALL_CLASS_COUNT]` "last found segment" hint (verified pre-check at scan top, written ONLY on a successful full scan — zero hot-path maintenance) failed the churn kill gate (±10 raw Ir, X4-B precedent): the `[u16; 49]` array init at `AllocCore::new` costs a constant **+44 Ir on every heap construction** (isolated cleanly by `large_alloc_free_cycle`'s raw delta — that bench touches no small class, so its entire +44 Ir IS the array init), and the four churn benches landed at **+46 raw Ir** (~5× the threshold). Cold/recycle landed flat (+0.1 Ir/op) — far below the −15…−25 Ir/op GO target; the O(n) scan at n=3 is 3 cache-hot iterations. Only the two multi-segment judges moved (`multiseg_cold_256k` −4.2 Ir/op, `seg_cycle_decommit_256k` −6.6 Ir/op) — the SAME figures X5/R1 reached.
    > - **Next trigger:** per the section's own text — "A future arc that adds a ≥64-segment bench (or profiles a real application with 100+ long-lived small segments) may flip this verdict; the correctness-proven hint shape is recoverable from this entry's description. The shape to revisit is the FULL per-class queue (skip non-matching segments entirely), since a per-class hint alone already loses to the bootstrap cost at n=3." NOTE (kept sub-finding): T10's other, lower-risk sub-finding (`class_for` align>16 jump-ahead walk over `SIZE2CLASS`, perf#9) is **KEPT** — orthogonal to this NO-GO, pure integer arithmetic, correctness-pinned by `tests/size_classes_slow_path_equivalence.rs`.
    > - **Cross-reference (added 2026-07-31, task #479):** consolidated with
    >   X5/item 20, R1/item 23, and R15-1/item 9 under the single shared
    >   "≥64-segment macro-bench" precondition — see item 34 below for the
    >   canonical cross-item statement; this entry's own history and
    >   evidence stay here unchanged.
    > - **Evidence:** `docs/perf/IAI_BASELINE.md` "T10 honest-reject (2026-07-12)" section (lines 1088–1204, full measurement table included there).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L22`.

23. **R1 (2026-07-13) — per-segment availability hint for `find_segment_with_free` (NO-GO, clean revert).**

    > **Current state**
    > - **Status:** NO-GO, honest reject — clean revert (working tree byte-identical to pre-experiment).
    > - **Current number/verdict:** NO-GO — the **fourth independent attempt** at this scan (after X5's per-segment bitmap and T10's per-class hint array). A single verified pre-check hint (`find_hint_slot: u32`, init `u32::MAX` = none, written on successful full scan, zero hot-path maintenance, sound-by-construction false-positive-only failure mode) PASSED the churn kill-gate (±10 Ir) at **+3 raw Ir** (the best of all three attempts — better than T10's +46 and X5's +9), but MISSED the cold/recycle target by a wide margin (+0.0…+0.1 Ir/op vs the campaign's −15…−25 Ir/op GO target): those benches fit entirely in the primordial segment (n=1, a one-iteration scan), so no scan optimization of any shape can help them. Only the two multi-segment judges moved (`multiseg_cold_256k` −4.3 Ir/op, `seg_cycle_decommit_256k` −6.6 Ir/op) — the SAME −4.3/−6.6 T10 already reached and was rejected for.
    > - **Next trigger:** per the section's own text — "A future arc that adds a genuine ≥64-segment bench (or profiles a real long-lived-process workload with 100+ simultaneously-live small segments) is the prerequisite for re-opening R1/X5/T10 — not a new algorithmic attempt at the current bench scale. The correctness-proven hint shape here (verified pre-check, zero hot-path cost, sound-by-construction false-positive-only failure mode) is the recommended starting point if that day comes." The structural barrier (every current bench models ≤3 live segments) is now confirmed a fourth time (X5, T10, R1's design-time Tier-A analysis, and this measured result).
    > - **Cross-reference (added 2026-07-31, task #479):** this item's own
    >   text already names X5/T10 as sharing this precondition; item 34 below
    >   makes that explicit as a standalone consolidated entry and adds
    >   R15-1/item 9 as a fourth instance of the same family — see item 34
    >   for the single cross-item statement; this entry's own history and
    >   evidence stay here unchanged.
    > - **Evidence:** `docs/perf/IAI_BASELINE.md` "R1 honest-reject (2026-07-13)" section (lines 1285–1354, full measurement table included there).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L23`.

24. **R5-R2b (2026-07-14) — the wall-clock churn regression signal is NOT an algorithmic/Ir regression (honest reject of the regression hypothesis).**

    > **Current state**
    > - **Status:** honest reject of the "algorithmic regression" hypothesis — the planned IAI-based bisection is moot by construction (closed without a source change).
    > - **Current number/verdict:** R5-R2 (the parent finding, `docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md`) used a rigorous paired A/B wall-clock protocol (20 alternating process-level reps, paired t-stat 3.94–5.27, sign test 17–19/20) to confirm a REAL, non-noise ~14–29% wall-clock slowdown on `global_alloc_churn`/SeferAlloc between baseline `e6b9b3a` and then-`HEAD`. R5-R2b re-measured the SAME window with `npm run iai` (the project's designated deterministic judge) and found `Ir` got FASTER, not slower: `small_churn_16b`/`churn_256b` 42,880 → 34,036 (−8,844 / **−20.6%**), `churn_write_256b` −20.3%; `EstCycles` and RAM hits moved the same direction by a similar/larger margin (e.g. `churn_256b` RAM hits 4,870 → 781). `Ir` is deterministic (byte-identical back-to-back at the same commit), so there is no `Ir` regression in this window to bisect.
    > - **Next trigger:** **no revisit trigger for the closed hypothesis** (it was refuted, not deferred). The section explicitly calls the one adjacent open thread — a possible Windows-native effect invisible to Ir (real page-fault/`VirtualAlloc`/decommit costs, TLB behavior, ASLR/base-address-dependent cache conflicts, or a codegen divergence between the `x86_64-pc-windows-msvc` and WSL/Linux target triples, since R5-R2's wall-clock numbers came from a native Windows release build while `npm run iai` drives a Linux/Valgrind-simulated binary) — a "NEW investigation, not a continuation of R5-R2b's now-closed algorithmic-regression hypothesis", which would need Windows-native tooling (ETW / a Windows perf-counter harness) this project does not currently have wired up.
    > - **Cross-reference (2026-08-03, R32-13/task #504):** R32-13 supplies
    >   the first real Windows-native OS-interface (`VirtualAlloc`/`VirtualFree`)
    >   measurement since this item was filed, decomposing a fresh-segment
    >   reserve/commit/decommit/release cycle and finding the avoidable
    >   (non-page-fault) share is 4.3-4.8% on Windows (vs 1.0-1.3% on Linux,
    >   R29-3) — a real, mechanistically-explained difference (2 Windows
    >   syscalls per reserve+commit + 2x VA over-reservation vs Linux's single
    >   eager `mmap`), but measured on a DIFFERENT workload regime (fresh
    >   4 MiB segment cycles, not small-object churn) from this item's own
    >   `global_alloc_churn` signal. **Explicitly NOT claimed to explain this
    >   item's signal** — per the survey that triggered R32-13
    >   (`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` F11:
    >   "I am not claiming this finding explains that... I am pointing out
    >   that the Windows OS-interface layer is the single largest unmeasured
    >   surface in this codebase") — recorded here only as the first
    >   quantified data point on that surface, not a resolution of this
    >   item's still-open Windows-native investigation.
    > - **Evidence:** `docs/perf/IAI_BASELINE.md` "R5-R2b honest-reject (2026-07-14)" section (lines 1356–1430); parent `docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md` (the wall-clock finding this entry closes); `docs/perf/R32_13_WINDOWS_RESERVE_COMMIT_DECOMPOSITION_GATE.md` (task #504, the cross-reference above).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L24`.

34. **The ≥64-live-segment macro-bench precondition — harness NOW EXISTS
   (R32-9/task #500, 2026-08-02) but does not yet model the fragmented/holey
   multi-class Small-directory state the four items actually need
   (R34-2/task #521 cross-ref, 2026-08-04). ONE canonical precondition for
   FOUR independently-filed items (X5/item 20, T10/item 22, R1/item 23,
   R15-1/item 9), consolidated here per the R30-post-response review's own
   observation that they all wait on the identical thing (filed 2026-07-31,
   R31-7d1+R31-13/task #479).**

   > **Current state**
   > - **Status:** [L] low-priority — the shared macro-bench harness NOW
   >   EXISTS (`benches/macro_multiseg_steady_state.rs` +
   >   `examples/r32_9_macro_multiseg_steady_state_ab_gate.rs`, R32-9/task
   >   #500, 2026-08-02 — see the dated UPDATE below), so this item's own old
   >   "does the missing artifact exist" blocking question is RESOLVED. The
   >   harness does NOT yet model the fragmented/holey multi-class Small-
   >   directory state X5/T10/R1/R15-1 actually need (R34-2/task #521 cross-
   >   ref below), and none of the four mechanisms has been re-judged under
   >   it — so the item stays open as "precondition met, re-judgment not yet
   >   done", NOT as "structural blocker / no harness". Cross-referenced FROM
   >   items 9, 20, 22, 23 below, which each keep its own full independent
   >   history untouched (append-only) and points HERE for the shared
   >   precondition instead of separately restating it.
   > - **Current number/verdict:** four independent NO-GO/deferred findings,
   >   spanning three separate scan/hint mechanisms plus one drain-scan
   >   design study, all bottomed out on the SAME structural wall: every
   >   bench this project currently runs models **at most 3 simultaneously
   >   live small segments** (`multiseg_cold_256k` — the widest one — spans
   >   only 3), so any optimization whose payoff scales with segment COUNT
   >   (a per-segment/per-class scan, hint, or queue) is structurally
   >   invisible to measurement here regardless of its real-world value:
   >   - **X5 (item 20)** — per-class segment-queue bitmap: REJECT at n=3
   >     (maintenance RMW cost dominates; no cache line actually avoided at
   >     this scale). Its own text: "a future arc that adds a ≥64-segment
   >     bench... may flip the verdict."
   >   - **T10 (item 22)** — per-class "last found segment" hint: NO-GO (the
   >     `[u16; 49]` init cost alone exceeds the churn kill-gate at n=3; only
   >     the two multi-segment judges moved, and only by the same
   >     −4.2/−6.6 Ir/op X5 and R1 also found). Its own text: "a future arc
   >     that adds a ≥64-segment bench (or profiles a real application with
   >     100+ long-lived small segments) may flip this verdict."
   >   - **R1 (item 23)** — per-segment availability hint: NO-GO, the
   >     FOURTH independent attempt at this exact scan, explicitly framed as
   >     confirming the barrier "a fourth time (X5, T10, R1's design-time
   >     Tier-A analysis, and this measured result)." Its own text is the
   >     most direct statement of the shared precondition: "a future arc
   >     that adds a genuine ≥64-segment bench (or profiles a real
   >     long-lived-process workload with 100+ simultaneously-live small
   >     segments) is the prerequisite for re-opening R1/X5/T10 — not a new
   >     algorithmic attempt at the current bench scale."
   >   - **R15-1 (item 9)** — nonempty-summary-word optimisation for
   >     `drain_dirty_segments`: honest reject, ceiling below the task's own
   >     noise floor at the current scale. Its own trigger is worded
   >     slightly differently (`MAX_SEGMENTS` raised by a large factor, OR a
   >     much-higher producer-class fan-in than N=8) but is the SAME family
   >     of precondition — a macro-bench with far more live segments/higher
   >     fan-in than any current bench models — not an independent,
   >     unrelated trigger.
   >   Three of the four (X5, T10, R1) use IDENTICAL "≥64-segment" wording
   >   independently arrived at across three separate rounds (2026-07-05,
   >   2026-07-12, 2026-07-13) — strong convergent evidence this is one real
   >   gap, not four coincidentally-similar ones. `docs/reviews/2026-07-30-fm-acceleration-review.md`
   >   §2.3, §4 item 5, and §5 action 7 independently name this same
   >   consolidation as worth doing ("four of them wait on ONE AND THE SAME
   >   nonexistent artifact... worth recording as a standalone item, not
   >   four unconnected triggers").
   > - **Next trigger (the ONE shared precondition all four items above now
   >   point to):** a macro-bench (or a profile of a real long-lived
   >   application) that models **≥64 simultaneously-live small segments**
   >   in a long-lived-process shape (R1's phrasing: "100+
   >   simultaneously-live small segments"), built as its own scoped task —
   >   NOT attempted by this entry, which is a docs/index reorganization
   >   only, per this task's explicit instruction. Building that one
   >   macro-bench either (a) reopens all four items for a fresh
   >   re-attempt at their respective mechanisms under conditions where the
   >   payoff would finally be visible, or (b) if the mechanisms still show
   >   no material win at realistic scale, closes the whole family
   >   permanently with one measurement instead of leaving four separate
   >   "someday" triggers that a future round could re-open one at a time
   >   without ever seeing the full multi-segment picture. Scoping the
   >   macro-bench itself (workload shape, segment-count target, whether it
   >   is a criterion bench vs. a `paired-ab-runner.mjs` process-level judge,
   >   which of the four mechanisms to re-attempt first) is deliberately left
   >   to whichever future round takes this on — out of scope for this
   >   consolidation task.
   > - **Evidence:** `docs/perf/IAI_BASELINE.md` "X5 honest-reject
   >   (2026-07-05)" / "T10 honest-reject (2026-07-12)" / "R1 honest-reject
   >   (2026-07-13)" sections; `R15_1_MAX_SEGMENTS_DRAIN_SCAN_COST.md` §7;
   >   `docs/reviews/2026-07-30-fm-acceleration-review.md` §2.3 ("Нет
   >   бенча с ≥64 живыми сегментами..."), §4 item 5, §5 action 7.
   > - **UPDATE (2026-08-02, R32-9/task #500): the missing artifact now
   >   EXISTS.** `benches/macro_multiseg_steady_state.rs` (Linux-only
   >   `iai-callgrind` bench, same platform-gating shape as
   >   `benches/perf_gate_iai.rs`) and its portable wall-clock companion
   >   `examples/r32_9_macro_multiseg_steady_state_ab_gate.rs` both
   >   establish an 80-segment floor (>= 64, oracle-verified via the new
   >   `HeapCore::dbg_table_count`, `src/registry/heap_core_diag.rs`) held
   >   live through a steady-state mixed Small+Large churn region, in
   >   single-thread (`multiseg_steady_state_1t`) and 4-thread
   >   (`multiseg_steady_state_mt4`) variants. Full design, path-activation
   >   oracle rationale, and first smoke-test numbers (wall-clock only — no
   >   Linux host was available to obtain real `Estimated Cycles`/RAM-hit
   >   numbers this task; that remains for a Linux-side follow-up) in
   >   `docs/perf/R32_9_MACRO_MULTISEG_STEADY_STATE_HARNESS.md`. Per that
   >   report's own §1: this DIRECTLY satisfies X5/T10/R1's own stated
   >   ">=64-segment bench" trigger; R15-1's trigger is only PARTIALLY
   >   satisfied (the live-segment-count half, not its separate
   >   producer-class-fan-in half — see that report's §1 for the
   >   distinction). No mechanism was re-attempted under the new harness in
   >   this task (infrastructure-only, per this task's own scope).
   >   This item stays open (a macro-bench existing is not the same as
   >   X5/T10/R1/R15-1 being re-judged and closed) but its own blocking
   >   precondition — "does the missing artifact exist" — is now resolved.
   > - **CORRECTION (2026-08-02, R32-10/task #501):** this item's own text
   >   said "#501 (`OWN_CACHE_SIZE`, F2) is the next task expected to
   >   actually use it" — that expectation did NOT hold. F2's own workload
   >   requirement (repeated in-place `realloc`/probe traffic on K
   >   concurrently-LIVE Large objects, never freed) is structurally
   >   DIFFERENT from this item's `macro_multiseg_steady_state` harness
   >   (whose churn step frees+reallocates only ONE Large object per round,
   >   rotating through the large-cache's slots — nowhere near enough
   >   simultaneously-"hot" bases to exercise `OWN_CACHE_SIZE` thrashing).
   >   R32-10 built a SEPARATE, purpose-built K-sweep harness instead
   >   (`examples/r32_10_own_cache_tier1_thrash_gate.rs`) — see
   >   `docs/perf/R32_10_OWN_CACHE_TIER1_THRASH_GATE.md` §2 for the full
   >   reasoning (including two false starts that tried adapting a
   >   free+realloc rotation shape closer to this item's harness and found
   >   it structurally incapable of showing any `OWN_CACHE_SIZE` effect).
   >   This item's X5/T10/R1/R15-1 family remains the correct target for
   >   `macro_multiseg_steady_state`; F2 was never actually in its scope.
   > - **BENCH-REVIEW CROSS-REF (2026-08-04, R34-2/task #521):** the
   >   `docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md` §8
   >   independently confirms the harness does not yet satisfy the SPECIFIC
   >   state requirement — the R32-9 harness models 80 dedicated **Large**
   >   segments + one Small churn class, not the "64+ live **Small** segments
   >   with holes, multiple classes, controlled directory misses / remote frees /
   >   pool transitions" shape X5/T10/R1/R15-1 actually need. So the harness
   >   EXISTS (resolving this item's own old "does the missing artifact exist"
   >   question) but has not yet re-judged any of the four mechanisms. Evidence
   >   needed to turn this into a real re-judgment task: a variant of the harness
   >   that establishes ≥64 simultaneously-live SMALL segments across several
   >   classes with inter-class holes, under `production` features, profile-first
   >   then A/B.

35. **`batch-api` real-downstream-consumer scouting pass #2 — R23-7's
    NO-CONSUMER decision RECONFIRMED (2026-07-31, R31-7d1+R31-13/task #479);
    no new candidate found, no public API expanded, no further batch
    micro-tuning done.**

   > **Current state**
   > - **Status:** RECONFIRMED — same verdict as R23-7 (`docs/perf/R23_7_BATCH_API_CONSUMER_STATUS.md`,
   >   task #376, 2026-07-27), independently re-checked against the CURRENT
   >   tree rather than assumed still true. Triggered by
   >   `docs/reviews/2026-07-30-r30-post-response-readonly-review.md`'s "What
   >   can still be accelerated strongly" item 4 + "Recommended next wave"
   >   item 6, which explicitly asked whether a real downstream owner (object
   >   pool, arena, ECS/storage slab, runtime task allocation, or any crate
   >   already allocating/freeing homogeneous groups) has emerged since R23-7 —
   >   NOT a request to reopen R23-7's decision itself.
   > - **What was checked (against the tree at this task's start, `HEAD` =
   >   `0a34ba1`):**
   >   1. **In-tree growth since R23-7.** Grepped every `src/`, `crates/`,
   >      `examples/` file for `.alloc_batch(`/`.dealloc_batch(` call sites
   >      outside the API's own definition/forwarding/test files — the only
   >      call sites remain `src/global/sefer_alloc.rs`'s four forwarding
   >      calls into `HeapCore::alloc_batch`/`dealloc_batch` (the
   >      `#[doc(hidden)]` registry layer) and the `fallback::with_heap` path
   >      — structurally identical to R23-7's own §2 finding, not a new
   >      caller.
   >   2. **Every current `crates/` workspace member read for a
   >      homogeneous-group alloc/free shape**, since three (`region`,
   >      `ring-mpsc`, `tagged-index-stack`) postdate or were not enumerated
   >      in R23-7's own file list: `region` is a `slotmap`-backed handle
   >      store (`slotmap::SlotMap` does its own internal storage
   >      management, never calls into `SeferAlloc::alloc_batch`); `ring-mpsc`
   >      is `no_std` + allocation-free by design (fixed-capacity ring,
   >      caller-supplied or owned-array backing, no heap allocation at all
   >      in its hot path); `tagged-index-stack` is `no_std` +
   >      `#![forbid(unsafe_code)]` + explicitly allocation-free (a bare
   >      index recycler with no backing storage of its own) — its own doc
   >      comment MENTIONS "object pools, entity-component stores" as
   >      prior art this primitive is *for*, but the crate itself is not
   >      such a consumer; it is infrastructure a future consumer could be
   >      built on, not one that exists today. None of the three calls
   >      `alloc_batch`/`dealloc_batch`, or would naturally: none owns a
   >      homogeneous-group allocate/free lifecycle that isn't already
   >      served by its own fixed/caller-supplied storage.
   >   3. **R23-7's own three named falsifiability triggers, re-checked
   >      individually:**
   >      - **Trigger 1 (a real internal consumer emerges)** — NOT fired. No
   >        bulk-deserialize path, batch node-construction step, or
   >        `Vec::with_capacity`-style bulk-reservation helper exists or is
   >        seriously scoped anywhere in this tree.
   >      - **Trigger 2 (a downstream project adopts/requests batch-shaped
   >        allocation)** — NOT fired; no issue/PR/reported workload of that
   >        shape exists in this repository to check against (this is a
   >        library repo, not one with a live external-user feedback
   >        channel visible in-tree).
   >      - **Trigger 3 (`dealloc_batch` gets batch-optimized)** — this ONE
   >        trigger DID fire, but earlier and for an unrelated reason:
   >        R24-8 (task #386, `docs/perf/R24_8_DEALLOC_BATCH_INTERNALS_GATE.md`)
   >        already amortizes `dealloc_batch`'s magazine-overflow flush into
   >        batched `AllocCore::flush_class` calls (`src/registry/heap_core_dealloc_batch.rs`,
   >        confirmed by reading `dealloc_batch_small`'s current body — the
   >        `STAGE_CAP`-chunked staging buffer flushes via `flush_class`, not
   >        a per-block loop). This closes trigger 3's MECHANISM condition,
   >        but per trigger 3's own text the point of firing it was to make
   >        "worth re-measuring end-to-end" — and there is still no end-to-end
   >        consumer to measure it against. A mechanism improving in isolation
   >        does not manufacture the missing caller; this is the same
   >        Box/Vec-path-gains-nothing-without-adoption point R23-7 §2's
   >        headline and the R30-post-response review's own item 4 both make.
   > - **Verdict:** **NO new candidate found. R23-7's decision stands,
   >   unchanged, reconfirmed by independent re-check rather than assumed.**
   >   Per this task's explicit scope, the public `batch-api` surface was NOT
   >   expanded and no further batch micro-tuning was performed under either
   >   outcome — this entry is a scouting-pass record, not an implementation
   >   task.
   > - **Next trigger:** unchanged from R23-7 §4 — any ONE of: (1) a real
   >   in-tree consumer is implemented or seriously scoped; (2) a downstream
   >   user demonstrably requests/adopts batch-shaped allocation with a
   >   concrete size/batch distribution; (3) [ALREADY FIRED, see above —
   >   retained here only as a historical marker, not an open sub-trigger].
   >   If a future round finds a real candidate under (1) or (2), the
   >   required judge per the R30-post-response review's own framing is
   >   **end-to-end latency AND retained memory** for the actual downstream
   >   workload — not another allocator-only micro-timing arm (R8-7/R9-9/
   >   R10-7's per-op-in-isolation numbers already answer the mechanism
   >   question; what would be new is the adoption's real effect on the
   >   consumer's own workload).
   > - **Evidence:** `docs/perf/R23_7_BATCH_API_CONSUMER_STATUS.md` (the
   >   original decision + falsifiability clause, re-confirmed rather than
   >   re-derived here); `docs/reviews/2026-07-30-r30-post-response-readonly-review.md`
   >   "What can still be accelerated strongly" item 4, "Recommended next
   >   wave" item 6 (the trigger for this reconfirmation);
   >   `docs/perf/R24_8_DEALLOC_BATCH_INTERNALS_GATE.md` (trigger 3's actual
   >   firing, pre-dating this task); `src/global/sefer_alloc.rs`,
   >   `src/registry/heap_core_dealloc_batch.rs`, `crates/sefer-region/src/region.rs`,
   >   `crates/ring-mpsc/src/lib.rs`, `crates/tagged-index-stack/src/lib.rs`
   >   (the files read for this reconfirmation).
   > - **BENCH-REVIEW CROSS-REF (2026-08-04, R34-2/task #521):** the
   >   `docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md` §6
   >   reiterates this item's own verdict — the batch mechanism gives 1.5–2.1×
   >   on warm AllocCore but only 1.1–1.6× through the production surface, and
   >   "Большой практический выигрыш появится только при реальном потребителе."
   >   Evidence needed to turn this into a real task (the bench review's
   >   framing, not a new trigger): ONE end-to-end consumer pilot — integration
   >   into an arena/slab/object-pool, batch 8–64 with mixed lifetimes and
   >   partial failure, measured end-to-end latency/throughput AND RSS — not
   >   another allocator-only micro-timing arm. The API stays `#[doc(hidden)]`
   >   until a proven consumer exists.

27. **R29-13 — large-cache `headroom_bytes` (default 256 MiB/heap) idle-RSS
    floor measured for the first time; confirmed-by-design, no action taken.
    R30-6 (2026-07-30) closed the "missing benefit-side" trigger this item
    named — see the dated update below. R31-1/R31-12 (2026-07-30, same
    round) NARROWED R30-6's parity claim to "parity at a 64 MiB rounded
    working set" after confirming that claim's workload actually sat AT the
    64 MiB boundary, not below it — see the dated updates below.**

    > **Current state**
    > - **Status:** retention cost CONFIRMED (R29-13); benefit side ALSO measured (R30-6) — a real, evidence-backed candidate headroom value (64 MiB) was identified and is now SHIPPED (not as a default change — `SeferAlloc::new()`'s 256 MiB default is untouched) as the `headroom_bytes` for both `Profile::Balanced` and `Profile::Throughput` (R30-7/task #456, `src/alloc_core/profile.rs`).
    > - **Current number/verdict (retention, R29-13):** the shipped 256 MiB default headroom converges, under maximum FORCED decay pressure (`dbg_force_decay_tick` looped to a fixed point), to a **measured floor of ~238–241 MiB/heap retained** (12.4–12.5% of an 8×34 MiB / 288 MiB fill reclaimed, the rest permanently held). Under PURE IDLE (100 ms/1 s/2 s, zero allocation activity), the idle delta is **exactly 0 KiB in all 36 measured arms** (4 headroom values × 3 thread counts × 3 reps) — idle reclaims nothing at ANY headroom setting, not only at 256 MiB. The natural fill/teardown workload never drives even one real decay tick regardless of headroom (`maybe_decay_large_cache`'s first-call timer-priming rule means a tight teardown loop never lets the 1000 ms interval elapse mid-loop) — this is read from source, not inferred, and matches the doc's "does not decay below this level" claim precisely once forced convergence is used to actually observe the floor.
    > - **Current number/verdict (benefit, R30-6, 2026-07-30):** at a representative 48 MiB/burst mixed small+large workload (burst→idle(1200ms, > the 1000ms decay interval)→burst, so a real non-forced decay tick can fire), **64 MiB headroom achieves the IDENTICAL 100.0% hit rate as 256 MiB** (byte-exact across 1/8/32 threads: 8/8, 64/64, 256/256) — the 256 MiB default buys ZERO measured hit-rate benefit over 64 MiB at this scale, while R29-13's own retention floor for 64 MiB (~34-37 MiB/heap) is ~7× smaller than 256 MiB's (~238-241 MiB/heap). 16 MiB and 0 MiB both cost a real, reproducible **12.5-percentage-point hit-rate loss** (87.5% vs 100.0%, exact across all thread counts, not noise) — NOT a free reduction. Latency: through the REAL `#[global_allocator]` (`paired-ab-runner.mjs`, A/B/B/A, n=20 pairs), **no headroom value in {0, 16, 64} MiB shows a statistically significant latency difference from 256 MiB** in this workload (all `|t| < crit(p<0.05)=2.101`; same-vs-same control confirms the harness is not manufacturing a false positive).
    > - **PARITY CLAIM NARROWED (2026-07-30, R31-12/task #476):** independently
    >   confirmed, by reading `AllocCore::alloc_large`'s rounding arithmetic
    >   (`src/alloc_core/alloc_core_large.rs:127-194`, whole-`SEGMENT` rounding,
    >   `SEGMENT = 4 MiB`) against R30-6's OWN committed CSV
    >   (`burst1_used_max_bytes = 67108864` = exactly 64 MiB in all 36 rows,
    >   not the 48 MiB the report's prose names), that R30-6's "8 x 6 MiB = 48
    >   MiB" workload actually rounds to a **64 MiB working set** — i.e. R30-6
    >   measured EXACTLY AT the 64 MiB headroom boundary, not below it. The
    >   64-vs-256 MiB tie is therefore **parity at a 64 MiB rounded working
    >   set specifically, NOT general throughput/hit-rate equivalence between
    >   64 MiB and 256 MiB headroom.** R31-1 (task #464, same round,
    >   `docs/perf/R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE.md`)
    >   measured hit rate at two burst sizes that GENUINELY exceed 64 MiB
    >   (128 MiB and 288 MiB, the latter at R29-13's own 34 MiB/object size)
    >   and found the tie BREAKS: 64 MiB headroom costs the same real,
    >   reproducible 12.5-percentage-point hit-rate loss (87.5% vs 100.0%)
    >   that 16 MiB/0 MiB already paid in R30-6, exact and identical at
    >   1/8/32 threads and at both crossing-regime sizes. `Profile::Balanced`
    >   and `Profile::Throughput` (R30-7/task #456, shipped BEFORE this
    >   narrowing) both carry 64 MiB headroom in their doc comments citing
    >   R30-6's now-narrowed parity claim without this regime caveat — see
    >   `src/alloc_core/profile.rs:38-45` — flagged here as an input for
    >   R31-9/task #473 (already reworking `Profile`'s doc comments) to
    >   incorporate, not fixed by this measurement-only task.
    > - **Next trigger:** R31-9/task #473 should add the regime caveat (64
    >   MiB headroom preserves full hit-rate parity ONLY up to ~64 MiB burst
    >   occupancy; past that, it costs the same measured 12.5-percentage-point
    >   hit-rate loss as 16 MiB) to `Profile::Balanced`'s/`Profile::Throughput`'s
    >   doc comments and the README profile table. Re-open the underlying
    >   256 MiB SeferAlloc::new() default question only if a future round
    >   wants to change it (not attempted by either measurement task, R30-7,
    >   or R31-1/R31-12).
    > - **CLOSED (2026-07-30, R31-9/task #473):** the trigger fired.
    >   `Profile` was restructured from the flat `{Rss, Balanced, Throughput}`
    >   enum into two independent axes; the old bundled 64 MiB value is now
    >   `LargeCachePolicy::Trimmed64MiB`, whose doc comment
    >   (`src/alloc_core/profile.rs`) states the regime caveat explicitly:
    >   "parity...at a 64 MiB rounded working set" + "R31-1...measured BEYOND
    >   that boundary and found the tie BREAKS." README's "Named profiles"
    >   section table carries the same caveat. No further action needed on
    >   this specific trigger; the underlying 256 MiB `SeferAlloc::new()`
    >   default remains unchanged, as this item's own trigger scoped it.
    > - **Evidence:** `docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md` (retention) + `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md` (benefit, + its 2026-07-30 §8 addendum) + `docs/perf/R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE.md` (crossing-regime benefit) + all three reports' `_summary.csv`/`_raw_*.log` companions.
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L27`.

36. **[T, filed 2026-07-31, UNVERIFIED-BY-ME findings from the Round 32 full
    independent review (`docs/reviews/2026-07-31-r32-full-review.md` §11
    P2-2, P2-3, P2-4, P2-5, P2-10)]** Five P2 findings against the round's
    new perf-report tooling (`scripts/capture-measurement-identity.mjs`,
    `scripts/verify-gate-report.mjs`), all against the R31-10 gate report's
    own first real use of that tooling — NOT independently re-verified
    before filing, per this file's own "filed, not fixed" convention (items
    31/33 are the direct precedent). Note: the review's P2-9 (a mechanism
    misattribution in `docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md`
    §0.3) was independently re-verified and fixed directly in the same
    session that filed this item (see that report's own §4 dated
    correction) — NOT included here.

   > **Current state**
   > - **Status:** filed, not fixed or independently re-verified.
   > - **Current number/verdict:** five sub-findings, as the review's own §11
   >   states them (this entry restates, does not re-derive):
   >   - **P2-2** — `scripts/capture-measurement-identity.mjs`'s printed
   >     recovery command (`git show <tree>: -- <path>`) is invalid: git
   >     silently ignores the `-- <path>` and prints the root tree listing,
   >     exiting 0 — a worse failure mode than an error. This exact string
   >     shipped verbatim into `docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md`.
   >     Suggested fix per the review: `git show <tree>:<path>` (no `--`,
   >     verified working by the review).
   >   - **P2-3** — the helper's two identity forms (`treeSha` from
   >     `git write-tree`, i.e. the INDEX; `patchSha256` from `git diff HEAD`,
   >     i.e. the WORKING TREE) are computed from different snapshots, so
   >     they can describe different content if the index and working tree
   >     differ at capture time. R31-10's published `patch_sha256` does not
   >     reproduce via the obvious route. The helper's own
   >     `patchReproduceCommand` also instructs `git apply <saved-patch>`
   >     but the script never saves the patch. Mitigating: the primary form
   >     (tree SHA) is valid and was verified end-to-end by the review — only
   >     the secondary/weaker form is broken.
   >   - **P2-4** — `verify-gate-report.mjs`'s check (d) (prose↔CSV headline
   >     cross-check, R31-5b's headline feature) SKIPped on R31-10's own
   >     report and on 66 of 88 reports corpus-wide, because
   >     `HEADLINE_KEYWORD_RE` requires a headline keyword on the SAME line
   >     as the number, and is unit-agnostic (would likely have false-WARNed
   >     on R31-10's MiB-vs-KiB unit mismatch had it matched at all).
   >     Suggested fix per the review: anchor on a `## Headline`-shaped
   >     section instead of same-line keywords, and add a KiB↔MiB
   >     normalisation pass.
   >   - **P2-5** — check (e) (allocator layer under test) WARNs on 86 of 88
   >     reports, check (f) on 24 — a gate that warns on nearly every input
   >     trains readers to ignore it, and a genuinely new WARN is
   >     indistinguishable from the ~350 pre-existing ones. Suggested fix per
   >     the review: extend the existing (b)/(c) retroactive-exemption
   >     mechanism to (e)/(f), or scope them to reports created after the
   >     rule commit (the pattern `verify-commit-prefixes.mjs` already uses
   >     via `merge-base --is-ancestor`).
   >   - **P2-10** — R31-10's summary-CSV provenance header
   >     (`# commit_sha=`/`# tree_sha=`/`# patch_sha256=`/`# captured_at=`/
   >     `# platform=`) was typed by hand in `examples/r31_10_trim_rss_gate.rs`
   >     rather than emitted by `capture-measurement-identity.mjs --json`,
   >     which already produces these fields machine-readably — exactly
   >     where P2-3's one unverifiable number (the patch hash) sits.
   >     Suggested fix per the review: wire the probe to consume the
   >     helper's own JSON output instead of hand-typing the header.
   > - **Next trigger:** any future task that touches
   >   `scripts/capture-measurement-identity.mjs` or
   >   `scripts/verify-gate-report.mjs` should fix the sub-finding(s) it
   >   touches in the same pass; otherwise a dedicated small tooling-fix task.
   > - **Evidence:** `docs/reviews/2026-07-31-r32-full-review.md` §11
   >   (P2-2 through P2-5, P2-10) — the review's own source, not
   >   independently re-derived by this filing.

   > **RESOLVED — 2026-08-02 (task #493).** All five sub-findings fixed
   > directly in `scripts/capture-measurement-identity.mjs` and
   > `scripts/verify-gate-report.mjs`, independently re-verified by the
   > orchestrator (not just the implementing agent's claim):
   > - **P2-2** — `recoverCommand` now omits `--` (`git show <tree>:<path>`);
   >   personally confirmed via `node scripts/capture-measurement-identity.mjs
   >   --json` and a live `git show <treeSha>:<path>` run.
   > - **P2-3** — `patchSha256` is now `git diff <headSha> <treeSha>` (two
   >   tree-ish objects), never `git diff HEAD` (the live working tree) — the
   >   two forms cannot diverge by construction. The script also now saves
   >   the patch to `docs/perf/_raw_identity_<tree-prefix>.patch`
   >   (`.gitignore`d by a new rule added in the same commit, matching the
   >   existing `_raw_*.log` policy) so `patchReproduceCommand` names a real
   >   file. Personally ran `--patch-hash` and confirmed the file was written
   >   and its hash matched the empty-diff case exactly (nothing was staged
   >   at capture time).
   > - **P2-4** — `extractHeadlineNumbers` now also scans a forward paragraph
   >   window (not just the same line), and `matchesWithRounding` is now
   >   unit-aware (KiB/MiB/GiB normalized to bytes via the CSV column
   >   header's own unit suffix, with the original bare-number match kept as
   >   a fallback).
   > - **P2-5** — checks (e)/(f) are now scoped NON-RETROACTIVELY (same
   >   `git merge-base --is-ancestor` technique `verify-commit-prefixes.mjs`
   >   already uses), keyed to the exact commit each check's CLAUDE.md rule
   >   landed at. Personally re-ran the full scan: the terminal verdict went
   >   from a bare `FAILED`/`ALL GREEN` (masking up to ~350 WARNs) to
   >   `PASS WITH 38 WARNINGS (d=28, e=1, identity=9)` — a >90% reduction,
   >   and the line itself now always states the count instead of going
   >   silent when WARNs are outstanding.
   > - **P2-10** — documented (not retrofitted — out of scope per the task's
   >   own instruction) as the intended flow for any NEW derive script going
   >   forward; `scripts/r31_10_derive_cost_report_data.mjs` and
   >   `scripts/r32_0_derive_report_data.mjs` (both landed just before this
   >   fix) still hand-type their provenance header and are not retrofitted.
   > **One live bug caught during this verification pass, not by the
   > implementing agent:** the full-corpus re-scan initially reported
   > `FAILED`, not the expected `PASS WITH N WARNINGS` — `verify-gate-report.mjs`
   > check (b) correctly caught a genuine pre-existing defect, unrelated to
   > this task's own diff: `R32_0_VIRGIN_ZERO_SKIP_COST_SIDE_GATE_summary.csv`'s
   > `landing_commit` was a 7-char short SHA, not 40-hex (the same recurring
   > bug class fixed three other times this session — R31-8, R31-10 cost
   > gate, and now this). Fixed in a separate commit
   > (`docs(perf): fix R32-0's landing_commit...`) before this task's own
   > commit landed, confirmed via a normalized diff that no other CSV value
   > changed.

37. **Three reports cite raw logs that were never committed — discovered by
    `scripts/verify-gate-report.mjs`'s first two CI runs (check (c) is
    FAIL-capable; each pushed the round's landing commit through CI red).**
    `R10_7_BATCH_WARM_ARM.md` (created commit `9611a56`, 2026-07-21) cites
    `_raw_r10_7_warm_arm.log`, `_raw_r10_7_tcache_isolated.log`,
    `_raw_r10_7_d_vs_f.log`, `_raw_r10_7_tcache_arm.log` in its own §5;
    `R8_9_MEDIUM_CLASSES_VERDICT.md` (created `9afba66`) cites
    `_raw_baseline_off.log`, `_raw_medium_on.log`,
    `_raw_baseline_off_reduced.log`, `_raw_medium_on_reduced.log`;
    `R9_3_MEDIUM_CLASSES_PRODUCTION_GATES.md` (created `c8f5f32`) cites
    `_raw_iai_production.log`, `_raw_iai_medium.log`,
    `_raw_criterion_production.log`, `_raw_criterion_medium.log`,
    `_raw_firstalloc_production.log`, `_raw_firstalloc_medium.log`. None of
    these 14 logs were ever committed under `docs/perf/` — all three reports
    only appeared to pass locally because the missing files happened to exist
    on the local machine as untracked scratch files left over from whenever
    the reports were originally written (confirmed by temporarily moving them
    aside and re-running the script locally: still `ALL GREEN` without them,
    reproducing exactly what a clean CI checkout sees). The first CI run's
    FAIL for R10_7 was fixed alone in the first response commit; only after a
    SECOND CI run did R8_9/R9_3 surface, because they sort after `R10`–`R23`
    lexicographically and a `head -300`-truncated log inspection during the
    first response never reached them — a process gap (truncated evidence
    inspection), not a difference in the underlying defect. All three
    verified to predate the raw-log-is-scratch-by-default policy (R13-10/task
    #280, commit `1a2dd7d`) via `git merge-base --is-ancestor <creation-sha>
    1a2dd7d` (exit 0 for all three) — pre-existing debt the new script
    surfaced, not a new defect. Resolved by adding all three to
    `scripts/verify-gate-report.mjs`'s `RETROACTIVE_EXEMPT` map (check `c`),
    same mechanism already used for `R15_1_MAX_SEGMENTS_DRAIN_SCAN_COST`.

   > **Current state**
   > - **Status:** resolved (exempted, not regenerated — the underlying raw
   >   logs are not reproducible from anything committed; a true re-run of
   >   each report's harness would be a fresh measurement, not a recovery).
   >   Cross-checked exhaustively against `git ls-files docs/perf` (not local
   >   disk state) — no other report in the corpus has this defect.
   > - **Evidence:** `scripts/verify-gate-report.mjs`'s `RETROACTIVE_EXEMPT`
   >   entries for `R10_7_BATCH_WARM_ARM`, `R8_9_MEDIUM_CLASSES_VERDICT`,
   >   `R9_3_MEDIUM_CLASSES_PRODUCTION_GATES`.

45. **F1b (2026-08-02, task #497) — merge `AllocBitmap`/`MagazineBitmap` into
    one 2-bit-per-granule `DualBitmap` (honest reject, correctness-sound,
    cost-rejected).**

    > **Current state**
    > - **Status:** honest reject — NOT recommended; not implemented (zero
    >   diff — the working tree was reverted to the base commit's exact
    >   state after measurement).
    > - **Current number/verdict:** REJECT. Implemented, correctness-verified
    >   (full test tree green under `--features production` and
    >   `--all-features`, miri-verified on `regression_virgin_bitmap_skip.rs`,
    >   the four named pinned counterfactuals all green), then measured:
    >   **every bitmap-touching bench regressed**, the three churn kill-gates
    >   (`small_churn_16b`/`churn_256b`/`aligned_churn_640b_a128`) by
    >   +189…+254 raw Ir — 20-25× past the ±10 kill threshold —
    >   `cold_alloc_free_256x16b`/`recycle_alloc_free_256x16b` by +899/+2,111.
    >   The bootstrap-proxy bench (`large_alloc_free_cycle`, never touches the
    >   small-class bitmaps) measured an EXACT 0 delta, ruling out a
    >   process-bootstrap-codegen-shift explanation (the R32-5 pattern) — this
    >   is a genuine per-operation cost increase. Root cause: the survey's
    >   analysis correctly predicted a win at the TWO call sites that read
    >   both oracles together (the free path's dual-oracle read), but did not
    >   weigh the aggregate cost the SAME storage change imposes on the far
    >   more numerous SINGLE-plane call sites (`pop_free`, `carve_batch`,
    >   `drain_freelist_batch`, `flush_run`, `reclaim_offset*`) — the new
    >   4-granules-per-byte packing needs one more arithmetic step
    >   (`pair_shift = (granule & 3) << 1`) to locate a bit within a byte than
    >   the old 8-granules-per-byte layout's plain `bit & 7`, paid by every
    >   single-plane touch, which outweighs the two-call-site saving.
    >   Correctness distinction from G1 (item 21 below) CONFIRMED real: this
    >   design kept both oracles' semantics and every call-site meaning fully
    >   independent (only storage/addressing shared), so it is rejected
    >   purely on measured cost, not on G1's semantics problem.
    > - **Next trigger:** per the report's own text — "a variant that keeps
    >   the two bitmaps SEPARATE in storage/addressing (F1's pure-locality
    >   interleaving form, NOT F1b's bit-packing form) would not pay this
    >   per-call arithmetic tax... The survey's own F1 entry... is the correct
    >   starting point for that alternative, not a further-refined F1b." F1's
    >   own blocker (needs the missing ≥64-live-segment macro-benchmark to
    >   show a pure cache-locality effect — item 34 above / task #500) still
    >   applies to that alternative.
    > - **Evidence:** `docs/perf/R32_6_DUAL_BITMAP_GATE.md` (full report),
    >   `docs/perf/_raw_r497_dualbitmap_before_production.log` +
    >   `docs/perf/_raw_r497_dualbitmap_after_production.log` (raw logs),
    >   `docs/perf/R32_6_DUAL_BITMAP_GATE_summary.csv` (checked-script-derived
    >   summary, `scripts/r497_dualbitmap_summary.mjs`).

39. **F13 (2026-08-03, task #505) — three areas checked and found thin/already-
    minimal/out-of-scope: (a) over-alignment classification, (b) TLS/registry
    binding on the ordinary path, (c) NUMA. Recorded as a NEGATIVE RESULT —
    do not re-derive.**

    > **Current state**
    > - **Status:** dead / negative result, recorded deliberately (per this
    >   file's own convention that an un-recorded conclusion gets re-derived
    >   — see item 34's four-item consolidation and item 24's R25-9
    >   re-verification-of-a-stale-reflag for two prior instances of exactly
    >   this failure mode). None of the three sub-findings is a task; nothing
    >   was implemented; this entry exists purely so a future round does not
    >   re-walk this ground from scratch.
    > - **Current number/verdict — three independent sub-verdicts:**
    >   - **(a) `Layout` alignment > `MIN_BLOCK` (=16) on the classification
    >     hot path — verdict THIN, not worth a round.**
    >     `crates/size-classes/src/lib.rs:353-384` (`class_for`) and
    >     `src/alloc_core/size_classes.rs:74` (`SMALL_ALIGN_MAX = 16`): any
    >     request with `align > 16` (common — e.g. crossbeam/tokio's 64/128-byte
    >     cache-padded types) misses the O(1) fast path and enters a
    >     divisibility walk. This walk was **already optimized once** — item
    >     22 (T10)'s own KEPT sub-finding (perf#9) is exactly this walk,
    >     already moved from step-by-1 scan to a bitmask-round-up jump, and
    >     is correctness-pinned by `tests/size_classes_slow_path_equivalence.rs`.
    >     The remaining walk is typically 1-2 iterations (one table load, one
    >     `block & (align-1)` test, one lookup). A further 1-entry
    >     `(size, align) → class` memo was considered and rejected: it adds a
    >     branch to the hottest path in the allocator, exactly the shape
    >     X4-B's won-front rule (item 18 above) rejects. C1 (0.3.0) already
    >     captured the LARGE win in this area (before it, `align > 16`
    >     requests bypassed the magazine entirely on both alloc and free).
    >   - **(b) TLS binding / `HeapRegistry` on the ordinary alloc/free path —
    >     verdict ALREADY MINIMAL.** `src/global/tls_heap.rs`'s three
    >     resolvers (`current_for_alloc`, `current_for_alloc_with_config`,
    >     `current_for_dealloc`) each reduce to **one `LOCAL.try_with` load
    >     plus one unsigned compare** (the Э2/task #145 trick collapses the
    >     `null`/`TORN` sentinels into a single
    >     `p.addr().wrapping_sub(1) < usize::MAX - 1` branch). `LOCAL` is a
    >     `const`-initialised `Cell<*mut HeapCore>` with no `Drop`, the
    >     configuration where std's `thread_local!` lowers to a direct
    >     `#[thread_local]` static access with no lazy-init check and a
    >     statically-dead `Err` arm. `HeapRegistry` is untouched after the
    >     first bind (`bind_slow`/`claim` are `#[cold]`, once per thread).
    >     R6-OPT-P0-1 already removed the one remaining real cost (a
    >     bind-less thread's `dealloc` used to claim a whole registry slot +
    >     commit a 4 MiB primordial segment just to free one foreign
    >     pointer). **Nothing further found.** **One loose end, NOT an open
    >     task — cheap and optional if a future round wants it:** whether
    >     Windows-MSVC's lowering of `LOCAL.try_with` is genuinely the direct
    >     `#[thread_local]` form or carries a per-access indirection was NOT
    >     verified read-only; a `cargo asm` / disassembly check of
    >     `SeferAlloc::alloc`'s prologue on `x86_64-pc-windows-msvc` would
    >     settle it in minutes, worth doing once given item 24's standing
    >     unexplained Windows wall-clock signal (R32-13/task #504, F11,
    >     found the reservation-path share of that signal small — 4.3-4.8%
    >     — leaving the TLS-access-shape question still genuinely open as a
    >     cheap check, not as a backlog item).
    >   - **(c) NUMA — verdict OUT OF SCOPE for `production`.** `crates/numa-shim/`
    >     (833 lines) + `src/alloc_core/numa.rs` (125 lines) exist as the
    >     in-crate seam, but `numa-aware` is **not part of `production`**, so
    >     every NUMA-touching site compiles out of the shipped configuration
    >     entirely. Within the feature, the one plausibly-hot cost
    >     (`numa::current_node()` per large allocation) is already cached
    >     with a bounded refresh period (`AllocCore::current_node_cached`,
    >     R11-5/R12-5) and invalidated at `claim`. This area has already had
    >     **one round wasted on re-raising a settled item**
    >     (R10-6/R11-6's `class_nonempty_by_node` work, closed, then
    >     independently re-verified still-closed by R25-9 against a stale
    >     re-flag — see this file's "Recently resolved" trail) — this
    >     sub-entry exists to prevent a third pass at the same ground.
    > - **Next trigger:** nothing, except the one optional cheap check named
    >   in (b). For (a), the trigger would be a **measured** real workload
    >   whose allocation mix is dominated by `align > 16` requests — a
    >   workload-shape finding, not a fresh code reading. (b) and (c) have no
    >   stated trigger beyond the one optional disassembly check.
    > - **Evidence:** `docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`
    >   §F13 (full reasoning for all three sub-findings); item 22 (T10) above
    >   for (a)'s already-optimized-once history; item 24 (R5-R2b) above and
    >   `docs/perf/R32_13_WINDOWS_RESERVE_COMMIT_DECOMPOSITION_GATE.md` for
    >   (b)'s cited Windows wall-clock context; this file's "Recently
    >   resolved" trail (R10-6/R11-6, re-verified by R25-9) for (c).

43. **F7 (2026-08-05, task #551) — `docs/perf/r34_23_runs/` was a tier-2
    artifact-storage-policy violator; the more general glob-based-policy gap
    it exposed is not yet closed — RESOLVED for the specific file, open for
    the general gap.**

    > **Current state**
    > - **Status:** the specific violation is CLOSED (this task); the
    >   general policy gap it exposed is a documented, low-priority,
    >   trigger-bearing residual, not an open task.
    > - **Current number/verdict:** `ba716a0` (R34-23/task #542) created
    >   `docs/perf/r34_23_runs/` with no matching `.gitignore` rule (unlike
    >   the sibling `docs/perf/paired_ab_runs/` and `docs/perf/r34_7_runs/`
    >   directories, both of which ARE ignored under the same
    >   scratch-by-default policy) — so its two JSON files landed as
    >   ordinary tracked files rather than through the deliberate
    >   `git add -f`-when-cited gate. One of the two,
    >   `2026-08-04T22-03-44-381Z_direct_raw.json` (263,907 bytes ≈ 258
    >   KiB), exceeded the 200 KiB tier-2 force-add ceiling that R34-24
    >   (`4ba188a`, same round) set a few commits later. `9b06b56` found and
    >   honestly named this as "the first real tier-2 case" in CLAUDE.md's
    >   artifact-storage-policy text, but did not apply tier-2's required
    >   remedy (truncate or gzip) and did not index the deviation here —
    >   flagged as finding F7 (P3) by the Round-34 independent readonly
    >   review (`docs/reviews/2026-08-05-round34-readonly-review.md` §6).
    >   **Remediated in this task (#551):** the 258 KiB file is
    >   gzip-compressed to `docs/perf/r34_23_runs/2026-08-04T22-03-44-381Z_direct_raw.json.gz`
    >   (8,674 bytes, ~30× smaller, well under the 200 KiB ceiling;
    >   byte-identical roundtrip verified via `gunzip`/`diff` before the
    >   uncompressed original was removed from the tree). Gzip was chosen
    >   over truncation per CLAUDE.md's own tier-2 guidance ("choose (b)
    >   when the full log is genuinely needed") because the file is 1,080
    >   uniform per-sample records (30 samples × cells) that the report's
    >   own summary CSV derives from IN FULL — truncating to a cited excerpt
    >   would lose the derivation's reproducibility, not just trim
    >   boilerplate. `docs/perf/R34_23_REALLOC_AND_VEC_GATE.md` §6's
    >   artifact table is updated to cite the `.gz` path with a
    >   decompression note. The sibling `2026-08-04T22-03-52-053Z_vec_raw.json`
    >   (69,045 bytes ≈ 67 KiB) was already under the 200 KiB ceiling and
    >   needed no change. A `.gitignore` rule for `/docs/perf/r34_23_runs/`
    >   was added for consistency with `paired_ab_runs`/`r34_7_runs` (does
    >   not untrack the already-committed files; only prevents future files
    >   in that directory from bypassing the force-add gate).
    > - **Residual, general gap — NOT closed by this task, filed here as the
    >   reopening trigger:** CLAUDE.md's artifact-storage-policy tiers are
    >   keyed on the `_raw_*.log` **filename glob**, which is exactly why the
    >   R34-24 compliance census ("256/256, all under the ceiling") could not
    >   see `r34_23_runs/*.json` — it does not match that glob, and the
    >   census script never scanned for it. This is a naming-based blind
    >   spot in the policy itself, not just in one directory's missing
    >   `.gitignore` entry: any future report that writes a large raw
    >   artifact under a directory/filename convention other than
    >   `_raw_*.log` (as R34-23's harness scripts did) will again be
    >   invisible to the census by construction. Re-keying the policy on
    >   file size/role rather than filename was explicitly weighed as part
    >   of this task and deliberately deferred — a single-file remediation
    >   task is the wrong scope for a policy rewrite in CLAUDE.md, and the
    >   two concrete instances so far (`r34_7_runs`, `r34_23_runs`) were both
    >   closed by adding a `.gitignore` rule + fixing the one file, which is
    >   cheaper than a policy generalization with only two data points.
    > - **Next trigger:** if a THIRD tier-2/tier-3-sized raw artifact turns up
    >   outside the `_raw_*.log` naming convention (i.e., another
    >   `docs/perf/<task>_runs/`-shaped directory, or any other
    >   non-`_raw_*.log` large committed artifact), treat that as the signal
    >   to stop patching one directory at a time and instead re-key
    >   CLAUDE.md's artifact-storage-policy compliance census onto file
    >   size/role (e.g. `find docs/perf -type f -size +200k`, independent of
    >   filename) rather than the `_raw_*.log` glob.
    > - **Evidence:** `docs/reviews/2026-08-05-round34-readonly-review.md` §6
    >   (finding F7); `ba716a0` (creates the directory, task #542); `9b06b56`
    >   (names the tier-2 case, task #543); this task's commit (gzip fix +
    >   `.gitignore` rule + this entry, task #551).

47. **PERF-1 (aligned-vmem, 2026-08-14/15, task #957) — Windows single-call
    huge-page path pays a guaranteed-failing `VirtualAlloc(MEM_LARGE_PAGES)`
    syscall when `size` is not a multiple of `GetLargePageMinimum()`.**
    **Second doomed-syscall class added 2026-08-16 (task #960, pre-publish
    audit finding 4): the same path's unconditional ordinary-page retry pays
    a second doomed syscall whenever the first `MEM_LARGE_PAGES` call fails
    for a reason that dooms the retry too (e.g. `ERROR_NOT_ENOUGH_MEMORY`).
    Two classes, two DIFFERENT cut-off mechanisms — see card.**

    > **Current state**
    > - **Status:** deferred — documented here, not implemented this round.
    > - **Current number/verdict:** NOT MEASURED (no benchmark run this
    >   round; this is a code-reading-only finding from an independent review
    >   of `aligned-vmem`'s round-3 fix pass, not a gate report). `crates/aligned-vmem/src/os/windows.rs`'s
    >   `win_reserve_commit` (post-split home, task #1082; called from `try_reserve_aligned_huge` with
    >   `extra_commit_flags = MEM_LARGE_PAGES`, always through the single-call
    >   fast path since huge-page requests use `align <= WIN_ALLOCATION_GRANULARITY`)
    >   issues `VirtualAlloc(NULL, commit_len, MEM_RESERVE | MEM_COMMIT | MEM_LARGE_PAGES, ..)`
    >   unconditionally, then on failure retries once without the large-page
    >   flag. `GetLargePageMinimum()` (the Windows API that reports the actual
    >   minimum large-page size — typically 2 MiB on x86_64, but not
    >   guaranteed cross-arch/cross-SKU) is never called anywhere in the
    >   crate (confirmed by grep — zero matches; filing-time state — task
    >   #1028 later added a threshold call, see the Evidence post-split
    >   note). Windows large-page
    >   allocations fail outright unless `size` is an exact multiple of that
    >   value, so any `reserve_aligned_huge(size, ..)` call whose `size` is
    >   not a multiple of the true large-page minimum pays one syscall that
    >   is guaranteed to fail (by page-size mismatch, not transient OS
    >   refusal) before falling back to the ordinary-page retry — a
    >   deterministic wasted syscall on every such call, not just an
    >   occasional privilege-related miss.
    > - **Second doomed-syscall class (added 2026-08-16, task #960,
    >   pre-publish audit finding 4; same code site, DIFFERENT cut-off
    >   mechanism, NOT covered by the class-1 pre-check):** the ordinary-page
    >   retry in `win_reserve_commit`'s single-call path (the
    >   `if extra_commit_flags != 0` branch inside the `None` arm of
    >   `match NonNull::new(p ...)`) runs UNCONDITIONALLY after ANY failure
    >   of the first `VirtualAlloc(.. | MEM_LARGE_PAGES)` call — including a
    >   genuine `ERROR_NOT_ENOUGH_MEMORY` refusal, where the retry asks the
    >   same pressured system for the SAME byte count again and almost
    >   certainly fails too: two doomed syscalls instead of one exactly in
    >   the regime (memory pressure) where a huge-pages consumer is worst
    >   off. Unlike class 1, `size` here can be a perfect multiple of
    >   `GetLargePageMinimum()`, so the cached divisibility pre-check cannot
    >   cut this class — it is only reachable by inspecting the FIRST call's
    >   error code (captured immediately after the failed syscall, per task
    >   #713's `GetLastError` discipline) and retrying only on causes a
    >   plain-page retry can plausibly cure (missing large-page privilege,
    >   `ERROR_PRIVILEGE_NOT_HELD`, or an invalid-parameter class) — never on
    >   `ERROR_NOT_ENOUGH_MEMORY`. **Two classes, two mechanisms:** class 1 =
    >   cached `GetLargePageMinimum()` divisibility pre-check (no syscall);
    >   class 2 = error-code discrimination between the two calls. Closing
    >   ONE does NOT close the other — a future implementation must not
    >   treat the size pre-check as covering both. Same deferral rationale
    >   as class 1: the error-code check edits the same hardened fast path
    >   (`V-6`/`V-7`/`V-8`/`V-32`/`H2C6`) and needs the same dedicated
    >   Windows-covered round.
    > - **Why deferred, not implemented:** two designs were weighed. (a) A
    >   cached pre-check — call `GetLargePageMinimum()` once (cache the
    >   result the same way `page_size()` already caches `query_os_page_size()`),
    >   then skip straight to the ordinary-page path (or return early) when
    >   `size % large_page_minimum != 0`, instead of paying the doomed
    >   syscall. (b) Leave as-is and document. (a) is the real fix but
    >   touches the same `win_reserve_commit` fast path multiple prior
    >   rounds (`V-6`/`V-7`/`V-8`/`V-32`/`H2C6`) have already hardened with
    >   care around exact alignment/fallback semantics — a change there needs
    >   its own dedicated round with Windows CI coverage to verify the
    >   pre-check doesn't itself introduce a new false-skip (e.g. wrongly
    >   short-circuiting a `size` that IS a valid multiple due to a caching
    >   bug), not a bundled hygiene-pass task. The cost is also bounded and
    >   one-time-per-call (one extra failing syscall, not a loop or
    >   per-byte cost), and `reserve_aligned_huge` is documented as
    >   best-effort or already off the hot allocation path for typical
    >   callers (large upfront segment reservations, not per-allocation),
    >   making this a real but low-urgency inefficiency rather than a
    >   correctness bug.
    > - **Next trigger:** a measured wall-clock/syscall-count gate showing
    >   `reserve_aligned_huge` is called with non-multiple-of-large-page-minimum
    >   `size` on a hot path in a real downstream consumer, OR a future round
    >   already touching `win_reserve_commit`'s fast path for another reason
    >   (natural place to fold in BOTH cut-off mechanisms without a
    >   dedicated Windows-only round), OR (class 2 specifically) evidence
    >   that real consumers hit `ERROR_NOT_ENOUGH_MEMORY`-shaped first
    >   failures where the retry also fails.
    > - **Evidence:** `crates/aligned-vmem/src/os/windows.rs`'s `win_reserve_commit`
    >   (single-call fast path, `extra_commit_flags` branch; post-split home,
    >   task #1082) and `try_reserve_aligned_huge` in
    >   `crates/aligned-vmem/src/api/reserve_aligned_huge.rs` (its
    >   `MEM_LARGE_PAGES` caller); confirmed
    >   via `grep -rn GetLargePageMinimum crates/aligned-vmem/src/` returning zero
    >   matches as of this entry. (Post-split update, task #1082: that grep is
    >   no longer zero — task #1028/R5-4's speculative-window extension (item
    >   49) added a `GetLargePageMinimum()` call as the fast-path ALIGNMENT
    >   threshold in `src/os/windows.rs`. The class-1 finding is unaffected:
    >   that call is a threshold comparison, not the size-divisibility
    >   pre-check this card proposes — no such pre-check exists.) Class 2: the unconditional retry is the
    >   `if extra_commit_flags != 0` branch inside the `None` arm of the
    >   single-call path's `match NonNull::new(p ...)` (the retry's own
    >   failure surfaces as `VmemError::last_os_error()`); added to this
    >   card by task #960 (aligned-vmem 0.2.0 pre-publish audit,
    >   2026-08-16, finding 4).

55. **[D] Two wall-clock gaps vs mimalloc measured on a quiet host, and the
    profiling plan to attribute them — DEFERRED by the owner, nothing
    measured yet.** (Filed 2026-08-18. Full plan: `docs/perf/PROFILING_NEXT_STEPS_PLAN.md`.)

    - **Status:** OPEN / DEFERRED — owner explicitly parked this. No arm has
      been run; the plan file carries no measured numbers and therefore owes
      no raw logs or summary CSV (it is a plan, not a gate report). The
      moment any arm runs, its output becomes a gate report and inherits the
      full evidence obligations.
    - **Current-number-or-verdict:** `bench_direct_alloc` 16 B — **3.28×
      slower** than mimalloc (36.7 vs 11.2 ns/pair);
      `bench_global_alloc_churn_with_teardown` 1024 B — **2.25× slower**
      (99.0 vs 43.9 ns/pair). Both from the QUIET-host `npm run bench:table`
      run of 2026-08-18; the same day's earlier run is VOID (taken under a
      concurrent full gate; its own control-arm drift guard fired at
      `mimalloc median -25.20%, System median -27.70%`). Correction recorded
      in the plan file: on the loaded run the teardown gap read 1.40× and
      appeared to be mostly shared physics — on the quiet host our
      256 B→1024 B jump is ×3.22 vs mimalloc's ×1.97, so **pool-cap
      exhaustion is the dominant term, not a residual**. Everything else is
      at or ahead of parity (`bench_churn_alloc` 1024 B is 8.46× FASTER).
    - **Next trigger:** any decision to attack either gap; any revival of the
      pool-cap lever (which must repeat R26-1's subprocess-per-arm isolation
      + resolved-cap hard assert — R25-5's un-isolated "cap 4→8 wins on
      latency AND RSS" did not reproduce on the RSS axis); any refresh of
      `docs/PROFILE_FLAMEGRAPHS.md`, whose recipe stands but whose findings
      predate the segment pool and the large cache.
    - **Evidence:** `docs/perf/PROFILING_NEXT_STEPS_PLAN.md` (the plan, the
      tool-per-question split, and the three constraints that decide whether
      a flamegraph run is worth anything — chiefly that profiling criterion
      spends ~84% of CPU in criterion's own KDE statistics unless
      `--profile-time` is used); `benches/global_alloc.rs`'s own
      `decommit_calls` / `segments_released_total` stderr deltas and the
      zero-rule in its module doc; `docs/PROFILE_FLAMEGRAPHS.md` §0/§1/§5.

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
- **R29 post-round readonly review — perf-report methodology findings (this index's own former item 28).** Resolved by R30-4 (task #453), 2026-07-30 — all 8 sub-findings (a)-(h) independently re-verified: 5 CONFIRMED (dated corrections appended to the affected reports), 2 PARTIAL (corrected/narrowed framing), 1 REFUTED (R29-16's 21.4× Ir ratio is expected Callgrind behavior for a bulk memset, not an artifact).
- **F4 (`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`) — `PerClass` missing `#[repr(C)]`, documented one-cache-line magazine layout not in effect.** Opened and resolved same-round by R32-5 (task #496), 2026-08-02 — added `#[repr(C)]` + field reorder (`count`, `virgin_mask`, `slots`) plus compile-time `offset_of!` pins; `count` moved from offset 128 to offset 0 (`slots` now at offset 8), struct size unchanged (136 B). Isolated magazine hit/push Ir delta measured at exactly 0 in both feature configs (matching the survey's own "expect 0 Ir" prediction) — landed as a documentation/layout-correctness fix (`fix(perf)`), not a measured speedup. See `docs/perf/R32_5_PERCLASS_REPR_C_LAYOUT_FIX_GATE.md`.
- **F12 (`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`) — large-cache HIT arm rewrites the whole ~144-byte `SegmentHeader` when only 4 fields genuinely change.** Opened and resolved same-round by R32-7 (task #498), 2026-08-02 — replaced `Node::write_struct` with 4 targeted field writes (`set_magic_at`/`set_large_size_at`/`set_large_align_at`/`set_bump_at`); falsification `debug_assert_eq!` never fired (kept as a permanent pin); UBFIX-6's unregistered-window safety argument independently restated and confirmed to hold for the narrower write shape; `size_of::<SegmentHeader>() == 144` compile-time pin added. Discovered en route that the survey's own claim — `large_alloc_free_cycle` "already exercises exactly this alloc→cache-deposit→alloc-hit cycle" — was WRONG (that bench is a single alloc+free, never a cache hit); built a new bench pair (`large_cache_prefill_only_4mib`/`large_cache_hit_only_4mib`) plus a public-API path-activation oracle (`examples/r32_7_large_cache_hit_activation_oracle.rs`) instead. Measured **−32 Ir/hit (8.5% of the hit arm's own marginal cost)**, all kill-gates flat — small, real, `perf(runtime)` (the large-cache hit arm is reachable through `production`'s always-on `alloc-decommit`). See `docs/perf/R32_7_LARGE_CACHE_HIT_TARGETED_HEADER_WRITE_GATE.md`.
- **F9 (`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`) — `maybe_decay_large_cache`'s `Instant::now()` fast-path guard is a cliff that `LowHeadroom`/`Trimmed64MiB` are designed to sit on the wrong side of.** Opened and resolved same-round by R32-8 (task #499), 2026-08-02 — measurement-first per the survey's own posture: built a confound-free A/B (fixed headroom, a new `bench-internals`-gated `FORCE_DECAY_CLOCK_READ` switch isolates the clock-read cost from any headroom-driven hit-rate confound, per CLAUDE.md's R30-6/R31-1 same-regime rule) plus a path-activation oracle (`MAYBE_DECAY_GUARD_PASSED`). **Confirmed the effect reproduces**: ~74-138 ns/call raw clock-read cost (5 independent runs), consistent with task #95's own ~105 ns/call historical anchor. Shipped the survey's own structural fix: a monotonic op-counter (`large_cache_decay_op_count`, `DECAY_CLOCK_CHECK_STRIDE = 64`) throttles clock reads to ~1-in-64 once past headroom, trading decay-tick GRANULARITY (ticks may fire up to ~63 large ops late, never early) for fewer clock reads; `dbg_force_decay_tick` explicitly bypasses the stride so its pre-existing deterministic "each call produces exactly one decay step" contract (depended on by `tests/large_cache_decay.rs` and R29-13's forced-convergence measurement) is unchanged. Measured the fix's own benefit in the exact above-headroom regime `LowHeadroom`/`Trimmed64MiB` target: **61-73% reduction** in `maybe_decay_large_cache`'s elapsed contribution (3 independent runs), guard-passed call count down 128× (byte-identical across all runs) in the specific single-object workload measured (mechanism explained in the report, not assumed generalizable). Updated `LowHeadroom`'s/`Trimmed64MiB`'s doc comments (`src/alloc_core/profile.rs`) to disclose the (reduced but nonzero) residual cost alongside their existing RSS-vs-hit-rate tradeoff documentation. Discovered a pre-existing, unrelated flaky test during verification (`xthread_large_free_tiny_size_huge_align_is_reclaimed`, confirmed to reproduce on the commit before this task's changes) — filed as item 14 in `docs/CORRECTNESS_OPEN_ITEMS.md`. See `docs/perf/R32_8_LARGE_CACHE_DECAY_CLOCK_READ_GATE.md`.
- **F2 (`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`) — `OWN_CACHE_SIZE = 4` is a 4-entry direct-mapped Tier-1 ownership cache that a Large-heavy workload thrashes by construction; this also answers item 1's own last-open clause below.** Opened and resolved same-round by R32-10 (task #501), 2026-08-02 — built the missing instrument first (a `bench-internals`-gated process-wide Tier-1 hit/miss counter pair, `CONTAINS_BASE_TIER1_HITS`/`_MISSES`, inside `SegmentTable::contains_base`), then a Large-heavy workload to drive it. **Two false starts, both self-caught by the harness's own path-activation oracles before any wrong number was published**: a free+realloc rotation turned out to be structurally incapable of showing ANY `OWN_CACHE_SIZE` effect, for two compounding reasons — (1) a pre-existing redundant SECOND `contains_base` call inside `AllocCore::dealloc`'s Large fallthrough always turns into a guaranteed hit, and (2) every Large free unconditionally calls `unregister`, which evicts the base's OWN cache slot at the end of the very call that warmed it, so a repeatedly-freed base's cache entry can never survive to its next visit regardless of cache size. The CORRECT shape — repeated in-place `realloc` (same size, no free, no unregister) rotating across K concurrently-LIVE Large objects — measured **0.00% Tier-1 hit rate at `OWN_CACHE_SIZE=4` for EVERY tested K (4 through 64), including K=4**, rising to **99.99% at `OWN_CACHE_SIZE=16` for K∈{4,8}** (K≥16 still thrashes at cache=16 — direct-mapped, non-associative, so K==cache-size does not guarantee distinct buckets). Shipped `OWN_CACHE_SIZE` 4→16 plus a new compile-time power-of-two pin. **Latency (ns/op) delta was an HONEST NULL** — both before/after arms sit in the same ~24-29 ns/op noise band despite the dramatic hit-rate delta, consistent with OPEN_ITEMS item 1's own ~4 Ir component-cost pricing (Tier-1 hit ~8.2 Ir vs Tier-2 miss ~12.0 Ir) being small against `realloc`'s whole cost. `perf(runtime)` — `alloc-xthread` (which makes `contains_base` the always-on ownership check) is in `production`. Standing ±10 raw-Ir churn kill gate NOT run (no Linux/Valgrind on this dev host, same constraint task #500 documented) — argued, not measured, to stay flat (the change is a per-heap struct-size constant + `bench-internals`-gated counter increments only). **CORRECTED same-day (2026-08-02, zero-trust review of this task): the "no Linux/Valgrind" excuse was wrong — WSL was available on this machine.** Re-measured: the raw kill gate does NOT stay flat (+227 to +1,578 Ir), but decomposes cleanly into a benign one-time `OWN_CACHE_SIZE` bootstrap cost (36-43 Ir, near-constant regardless of bench shape — same signature as task #496's `PerClass` finding) plus a `bench-internals`-only Tier-1 counter cost (191-1,535 Ir, scales with call count, never ships in real `production`). See `docs/perf/R32_10_OWN_CACHE_TIER1_THRASH_GATE.md` §5.2 for the full three-arm decomposition and its self-asserting derive script.
- **F10 (`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`) — `RemoteFreeRing::push` reads the consumer's `head` cache line (cross-core coherence miss) on every cross-thread free, even though PERF-PASS-4 (task #52) already split it onto its own line.** Opened and resolved same-round by R32-11 (task #502), 2026-08-02 — implemented the survey's proposed shadow/cached-head fix (`cached_head: AtomicU32` in the ring's existing 56 B of unused cursor-block padding, offset 72, `CURSOR_BLOCK`/`FOOTPRINT` unchanged), formally verified correctness (a soundness argument proving a stale-low shadow can never cause a missed overflow/lost entry/premature slot reuse, restated in the module doc + this report's §1; a new `RingModelShadow`/`RingModelShadow1` loom model — 3 new tests among 8 total in `tests/loom_remote_ring.rs`, including a `#[should_panic]` counterfactual proving the real implementation's "always re-derive on the slow path" design is load-bearing; a new `tests/remote_ring_shadow_head.rs`, 3 tests) — **caveat added 2026-08-05 (Sol release readonly review, finding F7): "formally verified" here means verified under the Rust memory model plus a bounded-staleness scheduler/time assumption (the shadow refresh's `head`-load-then-`cached_head`-store window must not see ~2^32 real drain advances), not a proof holding under the abstract memory model alone — see `docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE.md` §11 and `src/alloc_core/remote_free_ring.rs`'s module doc for the full statement** — then built the missing cross-thread producer/consumer wall-clock harness (`examples/r32_11_remote_ring_shadow_head_gate.rs` — none existed in this project before). **Two false starts in the harness itself, both self-caught before a wrong number was published**: (1) an owner-thread-alloc/free-churn drain design showed 91% ring-overflow instead of near-0% (fixed with a new `bench-internals`-gated `SeferAlloc::dbg_drain_current_thread_rings` direct-drain hook); (2) a naive "owner never drains" adversarial design triggered `push_with_overflow_retry`'s stalled-round retry storm instead of measuring `push` itself (8,000 pushes took 5.4 SECONDS; fixed with a slow-bounded-cadence drain instead of zero). **A THIRD false result, this time in the actual before/after measurement**: the first complete comparison showed the fix making things SLOWER (t=-13.3, sign test 20/20, reproduced 3×) — root-caused to the harness's OWN path-activation oracle counters (`DBG_RING_PUSH_SHADOW_FAST`/`_SLOW`, needed to prove regime activation) adding a locked RMW to every push, contaminating the timing; fixed with a two-build-mode harness (oracle-bearing build proves the regime SEPARATELY, a `bench-internals`-free timing-only build — reaching the identical drain via `global::tls_heap::current_for_trim` + `HeapCore::dbg_drain_all_rings` directly — supplies the cited numbers). **Corrected measurement: favorable regime (owner drains promptly) −30% to −36% ns/push, 3/3 independent trials statistically significant, sign test 0/20 (before-faster) every time. Adversarial regime (owner drains rarely) −1% to −38% ns/push, direction consistent across all 5 trials (sign test always favors after), 3/5 trials reach t-test significance — the other 2 were captured under confirmed concurrent host contention (shared dev machine, not a dedicated benchmark box) that inflated variance enough to fail the magnitude-sensitive t-test despite a still-lopsided (28/30) sign test.** `perf(runtime)` — `alloc-xthread` is in `production`'s default feature set. See `docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE.md` for the full soundness argument, both harness false starts, the measurement-instrument-contamination finding, and the complete multi-trial evidence table.
- **F8 (`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`) — every large-cache scan walks a 56-byte-per-slot array-of-structs to read one 8-byte field; the survey splits its fix into a low-risk occupancy bitmask (sub-change (2)) and higher-risk `usable_size`/`seq` sidecars (sub-changes (1)/(3)) and explicitly recommends measuring the bitmask alone first.** Opened and resolved same-round by R32-12 (task #503), 2026-08-02 — **shipped the bitmask ALONE, per the survey's own "may be the whole shippable subset" framing; did NOT build the sidecars.** `AllocCore::large_cache_occupied: u64` replaces `large_cache_find_free_slot`'s linear `.position(|s| s.is_none())` scan with `trailing_ones()`; correctness argument is a complete two-site enumeration (`large_cache_slot_set`/`large_cache_slot_take` are the ONLY two functions in the crate that ever write a slot, verified by grep — every other mutation path funnels through one of these two) plus a falsification-first invariant test (`tests/large_cache_occupancy_bitmask_invariant.rs`, 4 tests, green under both `alloc-decommit` alone and with `large-cache-extended`) that caught two of its OWN false assumptions about admission-loop/budget-default behavior before either was mistaken for a bitmask bug. **Measured separately, same-regime discipline (cache genuinely near-full, 7/8 base slots permanently occupied, worst-case scan position):** native wall-clock A/B at `scan_bound=8` (production's actual base cache size) is a **confirmed noise-band NULL** (t=0.492 vs crit=2.101, 20-pair paired A/B/B/A) — exactly the survey's own honest prediction that an 8-element scan is already cheap enough to sit below a process-level timer's noise floor; same-vs-same control confirms harness sanity (t=-0.394). The Ir axis (much lower noise floor, via WSL/Valgrind, a new dedicated `large_cache_free_slot_search_{prefill,cycle}_only` iai-callgrind bench pair using R23-3's shared-prefix-subtraction pattern) shows the REAL, small, correctly-signed win the wall-clock probe couldn't resolve: **−5.0 Ir per admission** (−40 Ir over 8 rounds, prefill arm byte-identical before/after confirming the shared-prefix isolation). Standing ±10 raw-Ir churn kill gate stays flat (+1 to −6 Ir across the 5 small-object benches, well within bound). **Sidecars (1)/(3) deliberately NOT built**: the measured win at production's actual N=8 is real but small (Ir-only, invisible in wall-clock), while the sidecars would introduce a genuine REPLICATED-field hazard (`usable_size`/`seq` duplicated between `CachedLarge` and new parallel arrays) needing the SAME lockstep-maintenance discipline the survey names as the exact failure mode that killed X5 (`[L]` item 20 above: "at n=3 the maintenance RMW on every transition is a net cost") — not justified by a win this small at this scan width, and no current production workload regime drives N large enough to plausibly change that calculus. `perf(runtime)` — `alloc-decommit` is in `production`'s default feature set. See `docs/perf/R32_12_LARGE_CACHE_OCCUPANCY_BITMASK_GATE.md` for the full correctness enumeration, both false-start test fixes, and the complete Ir/wall-clock evidence tables.
- **F5 (`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`) — the 16 KiB `SIZE2CLASS` LUT is not the cache problem item 19 (X6)'s original revisit trigger implied; re-assessment only, no code change.** Opened and resolved same-round by task #505, 2026-08-03 — docs-only re-assessment of item 19 (X6) above: confirmed REJECT still holds ("confirmed dead, and deader"), and narrowed item 19's own revisit trigger from "a real-application cache profile showing SIZE2CLASS lines contending" to "a real application whose size distribution is dominated by scattered ≥16 KiB small-class sizes" — see item 19's own current-state card for the full density argument (the LUT's index is dense from zero, so its hot region is exactly as small-size-dominated as the workload). No `src/`/`benches/`/`examples/`/`tests/` change; `docs(perf)`.
- **R-V20-849 — Unix exact-reserve hit rate (aligned-vmem) — this index's own former item 46.** Resolved by `aligned-vmem` round 2 task #944 (P-1), 2026-08-14 — the item's own "next trigger" ("re-evaluate whether the fast path should be kept, disabled, or gated on a 32-bit target check") is exactly what shipped: `try_reserve_aligned_exact` is now `#[cfg(target_pointer_width = "32")]`-only, removed entirely on 64-bit. The item's own S12 sub-note (an unmeasured `mmap` hint-retry mechanism for the fast path's miss cost on Linux/macOS) is a SEPARATE, still-open idea, independently re-flagged (without knowledge of this item, per that review's isolation rules) as finding P-2 in `docs/reviews/2026-08-14-aligned-vmem-pre-release-review-round2.md` — see `CHANGELOG.md`'s round-2 entry, "deliberately deferred" paragraph, for P-2's current disposition (still needs a fresh measurement, not implemented). Full text (including the S12/T10 sub-notes) archived at `docs/perf/OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail".
- **F13 (`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`) — three areas checked and found thin/already-minimal/out-of-scope (over-alignment classification, TLS/registry binding, NUMA); negative result recorded so a future round does not re-derive it.** Opened and resolved same-round by task #505, 2026-08-03 — recorded as new item 39 `[L]` above with all three sub-verdicts and the reasoning needed to avoid re-deriving them; the one loose end (a Windows-MSVC `cargo asm` check of `LOCAL.try_with`'s lowering) flagged as a cheap optional future check, not a backlog task. No `src/`/`benches/`/`examples/`/`tests/` change; `docs(perf)`.

**Round-32 independent review (`docs/reviews/2026-08-03-round32-readonly-review.md`) — perf-scope findings F4/F5/F6/F8/F9/F10/F11, all closed by Round 33 (tasks #510–#517). Filed 2026-08-04 (R34-2/task #521) — Round 33 never touched this index (Round-33 review finding G5 [P2]), so these closures were recorded nowhere durable until now.**

- **F4 [P2] — R32-10 ships a `production` default change (`OWN_CACHE_SIZE` 4→16) on the weakest latency evidence in the round (null asserted, not demonstrated; no dispersion statistic, no same-vs-same control).** Closed by R33-5 (task #510, commits `81d24f9` + `b3b18bb`) — re-ran the latency axis through 20-pair A/B/B/A at all 7 K values with same-vs-same controls: max `|t|` = 1.729 vs crit = 2.101, no K significant, no sign test more lopsided than 13/7, all 14 same-vs-same controls non-significant. Honest null confirmed (the K=4 direction even reversed relative to §4.1's original single-run +8.8%, proving that was noise). See `docs/perf/R32_10_LATENCY_NULL_PAIRED_AB_GATE.md` + its summary CSV + derive script.
- **F5 [P3] — R32-10 §5.2's `isolate` arm provenance cross-reference points at §8's note about a DIFFERENT arm (the `OWN_CACHE_SIZE=4` "before" scratch edit, not the counter-disable scratch edit).** Closed by R33-9 (task #514, commit `454149e`) — appended a dedicated R29-6 exemption note for the isolate arm's unrecoverable scratch edit into §8, so the §5.2 cross-reference now lands on a note about the correct arm.
- **F6 [P3] — derive scripts are not idempotent against their own committed artifacts (`landing_commit = 'UNFILLED_PLACEHOLDER_40_HEX'`, filled by hand in a follow-up commit, so re-running the checked script destroys the column).** Closed by R33-8 (task #513, commit `b537770`) — derive scripts now take the landing SHA as `argv[2]` or fall back to `git rev-parse HEAD`; all 15 round-trippable (Round-33 review §5 re-verified 19/19 scripts CLEAN against committed raw data). The smaller residual — `git rev-parse HEAD` silently emits the PARENT for a new report generated inside its own landing commit — is filed as item 41 below.
- **F8 [P3] — two Round-32 reports break the same-base-name summary-CSV rule (`R495_STAMP_REMOVAL_GATE_summary.csv` and `R496_PERCLASS_REPR_C_LAYOUT_FIX_GATE_summary.csv` named by task-number, not report basename).** Closed by R33-11 (task #516, commits `998d373` + `f51ec37`) — renamed both to match their report's own basename; added `verify-gate-report.mjs` check (h) to catch the class going forward. (A third pre-existing instance — `R30_7_…_AB_summary.csv` missing the `_GATE` suffix — was surfaced by check (h) but left unfixed; filed as item 40 below.)
- **F9 [P2] — R32-8's decay-clock-throttle measures the BENEFIT (ns/call saved in a high-throughput regime) but argues the COST (retention in a low-throughput regime) qualitatively, violating CLAUDE.md's same-regime cost/benefit rule.** Closed by R33-6 (task #511, commits `5bd7c04` + `8a04452`) — built a subprocess-per-arm retention-cost harness with hard-asserted config evidence + path-activation oracle; measured the retention cost is bounded at ≤1 segment per missed interval in the low-throughput regime. See `docs/perf/R33_6_DECAY_THROTTLE_RETENTION_COST_summary.csv`.
- **F10 [P3] — R32-3 is the round's only `perf(runtime)` shipping change with no gate report, no CSV, no raw log (verdict-resting numbers existed only in the commit message).** Closed by R33-12 (task #517, commit `96ae245`) — backfilled `docs/perf/R32_3_REALLOC_REDUNDANT_CONTAINS_BASE_GATE.md` + `_summary.csv` + two `_raw_*.log` files + checked derive script; reproduces the original commit message's numbers exactly (−120 Ir `realloc_grow`, four kill-gates byte-exact at 0 delta). NOTE: the CSV's `doc_commit` column was initially the PARENT of the landing commit (R33-8's `git rev-parse HEAD` fallback, see item 41); corrected to `96ae245` in R34-2.
- **F11 [P2] — Round 32 has no `### Round 32` heading in CHANGELOG.md; a bolded "Runtime improvements this round: 0" sits directly above eight runtime improvements.** Closed (PARTIALLY) by R33-7 (task #512, commit `182b222`) — split Round 32's runtime improvements into their own `#### Runtime improvements` subsection with an accurate "Runtime improvements this round: 7" line. RESIDUAL (Round-33 review G6 [P3]): Round 31's section still carries the same collision shape ("Runtime improvements this round: 0" two lines above a heading listing R31-10's promoted runtime improvement), and Rounds 31/32 are out of section order (`grep -n "^### Round"` gives 33, 31, 32, 30…). The residual is filed in `docs/CORRECTNESS_OPEN_ITEMS.md` (reporting-honesty/process scope).

### Cross-reference — `docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`, all 14 findings (added task #505, 2026-08-03)

The survey's own summary/prioritized punch list (its own §"Summary / prioritized
punch list") ranked 14 entries (F1-F13 plus sub-finding F1b). All 14 now have a
permanent home in this index or in `docs/CHANGELOG.md`; this table is the
single place a reader can confirm that without re-reading the ~1,490-line
survey doc. Every commit SHA below is the full 40-character form, verified
against `git log --format=%H` (never `--oneline`'s abbreviated form), not
transcribed from memory or from a prior task's prose.

| Finding | One-line description | Disposition | Task # | Commit SHA (full) |
|---|---|---|---|---|
| F1 | Two per-segment bitmaps (alloc/magazine) laid out 32 KiB apart — pure-locality interleave | Superseded by F1b's strictly stronger form (never separately attempted); F1b's own "next trigger" leaves F1's pure-locality variant open, still blocked on the ≥64-segment macro-bench (item 34) | — (superseded, not actioned) | — | — |
| F1b | Single 2-bit-per-granule `DualBitmap` merging `AllocBitmap`+`MagazineBitmap` | Rejected-with-evidence — implemented, correctness-verified, then measured: every bitmap-touching bench regressed 20-25x past the ±10 Ir kill gate (per-call packing-arithmetic tax on single-plane call sites outweighs the two-call-site saving) | #497 | `2dfeaa30944fb73dedd2365bb90c41ff4c198c5d` |
| F2 | `OWN_CACHE_SIZE = 4` direct-mapped Tier-1 ownership cache thrashes under a Large-heavy workload | Shipped — raised 4→16 + built the Tier-1 hit/miss counter; hit-rate win confirmed (0.00%→99.99% at K∈{4,8}), latency delta an honest null (noise-band) | #501 | `5289c661877462f3caf6c4e136ad3c163f6fe15b` |
| F3 | `Ir` is structurally blind to cache/coherence effects; 6+ items blocked on one missing ≥64-segment macro-bench | Shipped (infrastructure) — built `benches/macro_multiseg_steady_state.rs` + `examples/r32_9_macro_multiseg_steady_state_ab_gate.rs`, 80-segment floor, oracle-verified; item 34 updated in place (macro-bench now exists; X5/T10/R1/R15-1 not yet re-judged under it) | #500 | `2ea920b98fbf5f75b9a92d74ed32fd8e96d04c65` |
| F4 | `PerClass` missing `#[repr(C)]` — documented one-cache-line magazine layout not actually in effect | Shipped — `#[repr(C)]` + field reorder, `count` moved offset 128→0, struct size unchanged (136 B); Ir delta measured at exactly 0 (matches survey's own prediction); `fix(perf)` (layout-correctness, not a measured speedup) | #496 | `5df56d376735933b3fb6c0097f5984771afab276` |
| F5 | 16 KiB `SIZE2CLASS` LUT is not the cache problem item 19 (X6)'s trigger implied | Docs-only re-assessment — REJECT re-confirmed; item 19's revisit trigger narrowed | #505 | `02f874b40ed2bad12260ea0fa1f559bf57ee4a72` |
| F6 | `realloc` move leg + `try_promote_to_large` re-derive `base` and re-run `contains_base` already proven earlier in the same call | Shipped — both call sites use `dealloc_own_thread_with_base`/`dealloc_own_thread` directly; correctness argument (live-segment enumeration) stated explicitly; judged by the pre-existing `realloc_grow` iai bench | #494 | `5d72bc633193938181e2d06f8c584617ebaecf42` |
| F7 | `alloc_zeroed`'s magazine-hit arm pays a `stamp_segment_owner` plain `alloc`'s hit arm deliberately omits — also an R31-0 A/B confound | Shipped — stamp removed after enumerating all magazine-block producers; R31-0's ON/OFF asymmetry corrected in place (item 25 above, "Confound resolved" note) | #495 | `cd5c634a29aba2e57a1a91ab84a9db42dbbbf023` |
| F8 | Large-cache scans walk a 56 B/slot array-of-structs to read one 8-byte field | Shipped, partial — occupancy-bitmask sub-change (2) only (survey's own recommended "shippable subset"); sidecars (1)/(3) deliberately NOT built (replicated-field hazard, X5's failure mode); −5.0 Ir/admission measured, wall-clock a confirmed noise-band null | #503 | `e88390bc88c863c8861d8bdda26fb49269cf9a89` |
| F9 | `maybe_decay_large_cache`'s `Instant::now()` guard is a cliff `LowHeadroom`/`Trimmed64MiB` are designed to sit on the wrong side of | Shipped — confirmed ~74-138 ns/call clock-read cost; shipped a stride-throttle (`DECAY_CLOCK_CHECK_STRIDE = 64`) trading decay-tick granularity for fewer clock reads; 61-73% reduction measured in the exact above-headroom regime; profile doc comments updated | #499 | `74345b8b3323f071b8bc45d38035163c3ac0ffef` |
| F10 | `RemoteFreeRing::push` reads the consumer-dirtied `head` line on every cross-thread free (one step short of PERF-PASS-4's own cache-line split) | Shipped — shadow/cached-head added in existing padding; formally verified subject to a bounded-staleness scheduler assumption (loom model + counterfactual; see §11 of the gate report for the caveat added 2026-08-05); new producer/consumer wall-clock harness built; −30% to −36% ns/push in the favorable regime, direction-consistent in the adversarial regime | #502 | `d38bf73c63fa989eace81e659a3844b98f6656c5` |
| F11 | Windows segment reservation over-reserves 2× VA, no aligned-reservation fast path; Unix fast-path hit rate unmeasured | Rejected-with-evidence (step 3 declined) — Unix/Windows counters shipped; first Windows-native reserve/commit decomposition found the avoidable share 4.3-4.8% (well under materiality), page-fault cost still dominant (~95.4%); `VirtualAlloc2` explicitly declined | #504 | `f6c3a61e1e0ac06916327a1f41162f0bed908c93` |
| F12 | Large-cache HIT path rewrites the whole ~144-byte `SegmentHeader` when only ~5 words changed | Shipped — replaced full `write_struct` with 4 targeted field writes; UBFIX-6's unregistered-window safety argument restated and holds; −32 Ir/hit (8.5% of the hit arm's marginal cost), kill-gates flat | #498 | `eb2463a449ca3497ce2761ee32f95cdc63bac321` |
| F13 | Over-alignment classification, TLS/registry binding, NUMA — three areas checked and found thin/minimal/out-of-scope | Negative result, docs-only — recorded as new item 39 `[L]` above; one optional cheap future check flagged (Windows-MSVC `cargo asm`), not a backlog task | #505 | `02f874b40ed2bad12260ea0fa1f559bf57ee4a72` |

Two rows have no "shipped/rejected" disposition because none applies: **F1**
was never separately attempted (superseded in the survey's own text by F1b
before any task targeted it — task #497 built F1b, not F1); its pure-locality
form remains blocked on the same ≥64-segment macro-bench precondition as item
34. **F3**'s disposition is "shipped (infrastructure)", not "shipped
(runtime)" — it built the missing measurement harness itself, per its own
scope; no mechanism was re-attempted under it in the same task, so item 34
(the four items F3's own text says it should unblock) stays open.
