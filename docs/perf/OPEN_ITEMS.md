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
    >   semantics independent) WAS attempted — see item 38 below (F1b,
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
    > - **Evidence:** `docs/perf/IAI_BASELINE.md` "R5-R2b honest-reject (2026-07-14)" section (lines 1356–1430); parent `docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md` (the wall-clock finding this entry closes).
   Full history: `docs/perf/OPEN_ITEMS_ARCHIVE.md` § `L24`.

34. **The missing artifact: a realistic ≥64-live-segment / long-lived-process
    macro-bench — ONE canonical precondition for FOUR independently-filed
    items (X5/item 20, T10/item 22, R1/item 23, R15-1/item 9), consolidated
    here per the R30-post-response review's own observation that they all
    wait on the identical nonexistent thing (filed 2026-07-31,
    R31-7d1+R31-13/task #479).**

   > **Current state**
   > - **Status:** [L] low-priority / structural blocker — no macro-bench
   >   built; this entry does not build one (docs/index reorganization only).
   >   Cross-referenced FROM items 9, 20, 22, 23 below, which each keep their
   >   own full independent history untouched (append-only) and now point
   >   HERE for the shared precondition instead of separately restating it.
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
   >   this task (infrastructure-only, per this task's own scope); #501
   >   (`OWN_CACHE_SIZE`, F2) is the next task expected to actually use it.
   >   This item stays open (a macro-bench existing is not the same as
   >   X5/T10/R1/R15-1 being re-judged and closed) but its own blocking
   >   precondition — "does the missing artifact exist" — is now resolved.

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
   >   `src/registry/heap_core_dealloc_batch.rs`, `crates/region/src/region.rs`,
   >   `crates/ring-mpsc/src/lib.rs`, `crates/tagged-index-stack/src/lib.rs`
   >   (the files read for this reconfirmation).

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

38. **F1b (2026-08-02, task #497) — merge `AllocBitmap`/`MagazineBitmap` into
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
