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
   (18.6%).**

   > **Current state**
   > - **Status:** `flush_class` isolation measured (R28-1) — the "Next trigger" question below is ANSWERED; region judged likely exhausted for further micro-optimization at the per-block-cost scope (no 5th attempt opened).
   > - **Current number/verdict:** `contains_base`-only share of a real free's `Ir` = **8.8% (523/5,920)**, NOT the original 18.6% (R23-1). The item was then reframed: the routing prefix is NOT the free path's dominant cost — the magazine-overflow mechanic is. Bitmap-clear coalescing was tried twice (R24-3, R24-4) → both NO-GO; STAGE_CAP 512→64 is a GO (−4,065 Ir/call, R24-8); FLUSH_N sweep NO-GO (R25-3); STAGE_CAP=64 boundary re-confirmed clean N=16→1024 (R25-7); lazy `Option<[..]>` staging array NO-GO — crossover at N=17, the 4th consecutive NO-GO in this region (R26-7). **`flush_class(8 blocks)`'s own standalone Ir is now measured (R28-1, task #430): 449 Ir (56.1 Ir/block) — 77.3% of one overflow event's 581 Ir total, 90.3% of R24-2's ~487 Ir fused remainder estimate (reconciles to within 2.1%).**
   > - **Next trigger:** ANSWERED, not open. R28-1 isolated `flush_class` (the overflow's larger untried lever) and judged the region likely exhausted for further per-block-cost micro-optimization at this scope (see `R28_1_FLUSH_CLASS_ISOLATION_GATE.md` §5 for the full reasoning) — `flush_run`'s per-block work is already minimal/mostly-necessary (2 cheap guards + 1 M2 correctness guard + 1 freelist write + 1 bitmap write, metadata already hoisted per-run), and the compaction+push residual is now measured small (~48 Ir), so there is no hidden larger target left to chase in this immediate function family. Five consecutive NO-GO-or-exhausted findings now cover this region (R24-3/R24-4/R25-3/R26-7/R28-1). If a future round revisits magazine-overflow cost, the more promising angle (not explored by R28-1) is reducing HOW OFTEN overflow fires (workload-shape/`FLUSH_N`/`TCACHE_CAP` — already NO-GO'd once in R25-3) or a structural redesign of the fixed bitmap-clear+flush+compact+push sequence, not another per-block `flush_class` tuning attempt. Separately, Tier-2-hash-probe-heavy workloads might show `contains_base` > 8.8% (open, not a proven floor).
   > - **Evidence:** `R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE.md` §7 (8.8%); `R23_3_HOT_PATH_ATTRIBUTION_GATE.md`; `R24_2_FREE_BY_MAGAZINE_STATE_GATE.md`; `R24_5_COLD_ALLOC_FREE_SPLIT_GATE.md`; `R24_8_DEALLOC_BATCH_INTERNALS_GATE.md`; `R26_7_LAZY_STAGE_ARRAY_GATE.md` (4th NO-GO; isolated zero-init = ~54 Ir, not ~581); `R28_1_FLUSH_CLASS_ISOLATION_GATE.md` (flush_class isolated at 449 Ir/8 blocks; region judged exhausted).

   R22-17 (task #368), 2026-07-26: `HeapCore::dealloc_routing`'s
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
   **2026-07-27 update — DONE (task #372, R23-3):** the read-only review's
   own P0 recommendation (`docs/reviews/2026-07-26-r22-readonly-review.md`
   §4.1/§6, "R23-3: split hot alloc / hot free / cold alloc / cold free")
   is executed: a fuller orthogonal decomposition of the WHOLE hot alloc/free
   path, not just the routing prefix this item's history covers.
   **Headline finding: the routing prefix (`contains_base` 8.8% +
   `segment_base_of_ptr` 9.8% = 18.6% combined) is NOT the dominant free-path
   cost.** The own-thread free BODY that runs once ownership is confirmed
   (the M2 double-free oracle checks fused with the magazine push,
   `dealloc_own_thread_with_base`) is **80.8% of a real free's `Ir`** — more
   than 4x the routing prefix's combined share, and previously un-isolated.
   Investigated whether Tier-1 vs Tier-2 `contains_base` can be split by
   workload shape (touching >`OWN_CACHE_SIZE`=4 segments): found this is NOT
   portably forceable (`cache_index` depends on OS-assigned segment
   addresses, not anything this allocator or a benchmark controls), and
   built a direct-call hook (`dbg_hash_contains_only`) instead — Tier-2's
   own cost, IF it fired, is 13.0% of this workload's total (vs Tier-1's
   8.8%), but this gate's own single-hot-segment workload never actually
   exercises Tier-2 in the real routing path. On the alloc side, the
   magazine-hit pop is 22.4 Ir/op (32.4% of `small_churn_16b`'s combined
   alloc+free marginal cost). On the cold path: pure bump-carving
   (standalone, no magazine/refill/BinTable-push) is 23.05 Ir/op; the
   freelist-pop round of `recycle_alloc_free_256x16b` (isolated via
   shared-prefix subtraction against the existing `cold_alloc_free_256x16b`
   row — no new bench arm needed) is 188.2 Ir/op, comparable to (slightly
   below) virgin-carve's own 203.86 Ir/op full-path marginal (R23-2) — i.e.
   recycle is NOT the costlier-than-carving mechanism a purely
   mechanism-level read would suggest once matched to the same full-path
   denominator. **Two self-caught methodology bugs disclosed in the report,
   not silently fixed:** a missing `#[inline(always)]` on a new hook
   initially inflated the own-thread-body measurement past the total free
   loop's own cost (impossible for a sub-component); and two N/2N bench
   pairs (magazine-hit, recycle-pop) were invalid because doubling the loop
   count doubled a whole setup+signal cycle, not the isolated signal alone
   — both replaced with shared-prefix subtraction (one new pair of arms;
   the other needed no new arm since an existing bench row already served
   as the shared prefix). **Recommendation for the next remediation task:
   the own-thread free body (M2 oracles + magazine push), 80.8% of the free
   path, not cold-carve/recycle** (which, per this task's finding, is
   roughly on par with virgin-carve once matched apples-to-apples) — this
   revises R22-15/R23-2's "cold-carve/recycle is the main remaining
   candidate" framing. Measurement only, no remediation attempted. Full
   decomposition, honest "not cleanly isolable" notes, and the ranked
   table: `R23_3_HOT_PATH_ATTRIBUTION_GATE.md` (full report) +
   `R23_3_HOT_PATH_ATTRIBUTION_GATE_summary.csv` +
   `docs/perf/_raw_r23_3_hot_path_attribution_run1.log` /
   `_raw_r23_3_hot_path_attribution_run2.log`.
   **2026-07-27 update — CORRECTED (task #379, R24-1):** the R23-3 DONE note
   directly above (and R23-3's own report §0) name the measured 74.70 Ir/free
   (80.8%) as "the own-thread free body: M2 oracles + magazine push (fused)"
   and frame it as the dominant cost of an ordinary hot free. That framing is
   wrong: the bench arms free 64 DISTINCT pointers in one sequential pass,
   which hits the magazine overflow arm (`cnt == TCACHE_CAP = 16`) six times
   — so 74.70 Ir/free is an average over 58 non-overflow pushes AND 6
   overflow events (bitmap-clear pass + `flush_class` on 8 blocks each +
   8-pointer compaction), i.e. a 64-block batch-free-with-overflow workload,
   NOT an isolated "M2 oracles + magazine push" cost and NOT representative
   of the interleaved `small_churn_16b` hot pair. Cross-check: 22.38 (alloc
   hit) + 92.50 (this free) = 114.88 > the entire 69.0 Ir hot pair —
   impossible if 92.50 were the free half of that pair; the workloads measure
   different magazine states. **Corrected characterization:** the free path's
   real dominant cost is NOT isolated by R23-3; whether the cheap non-overflow
   push or the overflow event dominates ordinary hot free is unmeasured. The
   actual next step is the follow-up measurement split **R24-2 (task #380)**,
   NOT immediate remediation of a still-incorrectly-named mechanism. Full
   arithmetic and the falsified "consistent with the free-path table"
   sentence: `R23_3_HOT_PATH_ATTRIBUTION_GATE.md` §9 (the correction section;
   original §0–§8 preserved verbatim).
   **2026-07-27 update — DONE (task #380, R24-2):** the magazine-state split
   R24-1 queued is executed. The free path's two magazine states are now
   cleanly isolated. **Headline finding: overflow is a BATCH phenomenon;
   ordinary interleaved hot free has NO overflow at all.** Cheap non-overflow
   push ≈ 43–44 Ir/full-free (one via dedicated n8→n9 pair, confirmed by
   16-push amortization); as R23-3's "cheap arm" scope (oracle+push, routing
   subtracted) ≈ 26 Ir. One overflow event = 571 Ir = **12.9× a cheap push**
   (the 17th free alone, via n16→n17 pair). The overflow's ONE cleanly-isolable
   sub-cost — the 8-block bitmap-clear pass (`heap_core_free.rs:762-768`,
   R24-3's exact target) — is **84 Ir** via a new hook
   (`dbg_overflow_bitmap_clear_pass`; re-gated `bench-internals` and made
   `unsafe fn` by R25-1/task #395 after a soundness review — see
   `src/registry/heap_core_diag.rs`'s doc comment on that function; the
   measured Ir figure itself is unaffected, only the hook's type/gate
   changed); the remaining ~470 Ir (flush_class + 8-
   pointer compaction + final push) is fused in one straight-line block with no
   workload-level separation point, reported as a single non-isolable remainder
   per the "measured, not spun" convention (a flush_class-standalone hook would
   run a mechanism production never runs outside the overflow arm — Heisenberg
   risk). **Reconciliation:** R23-3's 74.70 Ir/free decomposes as
   (58×25.8 + 6×553.8)/64 = 75.30 (0.8% reconstruction) — i.e. **~69% overflow
   + ~31% cheap push** within the own-thread body, NOT the "fused oracle+push"
   the original §0 named. N-sweep (N=1/8/16/17/32/64): per-free cost is FLAT at
   ~44 Ir for N≤16 (zero overflow), steps to 75 at N=17 (first overflow),
   climbs to 93 at N=64; overflow is **57.9% of the N=64 batch** (6 events =
   9.4% of the frees). **Interleaved comparison:** ordinary interleaved hot
   free (`small_churn_16b` shape) ≈ 46.6 Ir/free (69.0 pair − 22.38 alloc-hit),
   matching the isolated cheap push within refill amortization — the 92.50
   Ir/free batch figure is NOT ordinary hot free. **Prioritization implication
   for R24-3 (flush_magazine_class bitmap-clear merge): saves 8.5% of
   batch-free cost (6×84/5920), 0% of ordinary interleaved hot free (overflow
   never fires there).** One new safe `#[doc(hidden)]` hook added (for R24-6
   tracking: lives in the production unsafe-audit surface, gate
   `alloc-global + fastbin`); 7 new bench arms. Measurement only, no production
   behavior changed. Full decomposition, honest "not cleanly isolable" notes,
   the N-sweep table, and the 74.70 reconciliation:
   `R24_2_FREE_BY_MAGAZINE_STATE_GATE.md` (full report) +
   `R24_2_FREE_BY_MAGAZINE_STATE_GATE_summary.csv` +
   `docs/perf/_raw_r24_2_run1.log` / `_raw_r24_2_run2.log`.
   **2026-07-27 update — NO-GO (task #381, R24-3):** the `flush_magazine_class`
   merge was implemented (shape (a): unconditional clear loop inside `flush_run`,
   opt-in via a new `flush_magazine_class` wrapper; `dealloc_batch` unchanged via
   `flush_class` → `flush_class_inner(..., false)`), correctness-verified (full
   test suite green; 3 new counterfactual tests including the M2-hazard mutation
   passed), but the Ir gate measured a **+37 Ir/overflow-event REGRESSION**
   (expected: -84 Ir improvement). Root cause: the pre-pass was a fixed-length
   loop (`FLUSH_N` = 8, `const`) that the compiler fully unrolls and CSE's with
   `flush_class`'s run-grouping (which calls `segment_base_of_ptr` on the same 8
   pointers); the merged clear loop inside `flush_run` is dynamic-length
   (`run: &[*mut u8]`) and cannot be unrolled, adding loop overhead that exceeds
   the saved CSE'd cost. **The standalone-measured 84 Ir (R24-2) overstated the
   real in-context cost** — the exact Heisenberg risk R24-2 §5.1 warned about.
   All production code reverted; tree clean at HEAD (`3bc9c91`). **R24-4 (task
   #382, bulk-mask primitives) remains BLOCKED** — the 84 Ir is not actionable as
   a savings target. Full evidence and root cause:
   `R24_3_FLUSH_MAGAZINE_CLASS_GATE.md` +
   `R24_3_FLUSH_MAGAZINE_CLASS_GATE_summary.csv` +
   `docs/perf/_raw_r24_3_merged_run1.log`.
   **2026-07-27 update — NO-GO (task #382, R24-4):** the bulk-mask primitive
   (`SegmentBitmap::clear_many`/`set_many` accumulator + domain wrappers) was
   implemented, fully correctness-verified (unit test 10/10 + mutation-nonvacuous;
   single- AND multi-segment `alloc_batch` integration tests + mutation-nonvacuous;
   clippy clean on `""`/`production`/`--all-features`), and applied at site #1
   (`heap_core_alloc.rs`'s `alloc_batch` deferred-clear — the loop whose own
   comment named the primitive as "the natural follow-up"). The in-context Ir
   gate — the SAME `alloc_batch_drain*` arm measured BEFORE (old per-block loop)
   vs AFTER (bulk clear), NOT a standalone hook, under `production batch-api`
   (`alloc_batch` is `fastbin + batch-api`, NOT in `production`; no in-tree
   production caller per R23-7) — measured a **+14 Ir/block REGRESSION**
   (`alloc_batch_drain15_16b` 3,685→3,894 = +209 Ir; `alloc_batch_drain8_16b`
   3,640→3,752 = +112 Ir), scaling linearly with drain count. All reference arms
   byte-identical (same toolchain). **Root cause:** the bulk primitive's
   per-offset bookkeeping (a stack-array store + the accumulator's
   compare/branch/accumulate) costs more in-context than the HOT-CACHE-LINE RMWs
   it coalesces (each ~3–4 Ir; the "8 consecutive 16 B blocks = 1 byte" ceiling
   treated an RMW as a costly unit, but here it is cheap). This is the SAME
   Heisenberg CLASS as R24-3 (operation-count ceiling ≠ in-context instruction
   cost) via a DIFFERENT mechanism — site #1's loop IS dynamic-length (R24-3's
   constant-unroll trap does not apply), yet a different risk materialized: the
   bulk replacement's own per-offset overhead. **"Dynamic-length loop" was
   necessary but NOT sufficient** to avoid the class. Site #2
   (`flush_all_tcache` teardown) was NOT attempted (per the "STOP at first-site
   regression" gate; it uses the same hot bitmap and would likely reproduce).
   All `src/`/`tests/`/`benches/`/`ARCHITECTURE.md` changes reverted; tree
   byte-identical to HEAD (`e530a9f`). **Two bitmap-clear NO-GOs in a row
   (R24-3, R24-4) indicate per-segment bitmap-clear loops are already efficiently
   compiled and NOT a fruitful target for RMW-coalescing primitives; the
   arithmetic ceiling should not be cited as a savings target for these sites
   without a fresh in-context measurement.** Full evidence and root cause:
   `R24_4_BULK_MASK_PRIMITIVES_GATE.md` +
   `R24_4_BULK_MASK_PRIMITIVES_GATE_summary.csv` +
   `docs/perf/_raw_r24_4_baseline.log` / `_raw_r24_4_after.log`.
   **2026-07-27 update — DONE (task #383, R24-5):** `cold_alloc_free_256x16b`'s
   ~2× cold-gap headline (R23-2: SeferAlloc 203.86 vs mimalloc 101.81 Ir/op) is
   now split into its alloc-only and free-only halves, localizing where the gap
   actually lives. **Outcome (a) confirmed: the gap is OVERWHELMINGLY in the
   FREE half.** Per-half ratios (mimalloc's own split built alongside via three
   new `mimalloc_cold_alloc_only` arms so the comparison is alloc-vs-alloc /
   free-vs-free, not full-round-only): alloc-only Sefer 91.05 vs mi 71.81 =
   **1.27×**; free-only Sefer 108.77 vs mi 30.24 = **3.60×**. The full-round
   2.0× is a BLEND masking how lop-sided the gap is (R23-2's "2.002×" is the
   average of a 1.27× alloc gap and a 3.60× free gap — NOT a uniform gap).
   **R24-2 reconciliation HOLDS within 2.6%:** the free half at N=256 is
   exactly the overflow mechanic R24-2 isolated at N≤64 — `floor((256-17)/8)+1
   = 30` overflows (period 8, `FLUSH_N=8`) × 571 Ir + 226 cheap × 44.25 =
   27,130.5 predicted vs 27,846 measured; **overflow is 61.5% of the free-half
   cost** (vs 57.9% at N=64 — the share grows as overflows accumulate). The
   task brief's own "~15 overflows at N=256" estimate was an under-count (R24-2's
   confirmed period-8 pattern implies 30; the "~15" model would miss by 31%).
   The two refill numbers: virgin ≈ 1099 Ir/event, recycled ≈ 961 Ir/event
   (both DERIVED via `c_pop=22.38` from R23-3, NOT directly isolated — fused
   with the pop, no standalone-refill hook added to avoid the R24-2 §5.1
   Heisenberg risk); recycled is 12.5% CHEAPER than virgin (freelist-drain
   skips the commit-frontier-grow), and round-2 alloc+free sums to 188.20
   Ir/op — matching R23-3's independently-published round-2 total EXACTLY.
   `carve_batch_only_16b` (23.05 Ir/block, reproduced R23-3) is the pure-carve
   primitive FLOOR, NOT a refill (the real `refill_magazine_slow` is ~3× it —
   carve + magazine-load + P4 stamp-dedupe + 15 `mark_magazine` RMWs + 2
   drains). **Caveat the task's own framing was prescient about: the root cause
   (free-side overflow) is now understood, but the two approaches already tried
   — R24-3 (flush_magazine_class bitmap-clear merge) and R24-4 (bulk-mask
   primitive), both targeting the overflow's 84-Ir bitmap-clear sub-cost —
   measured NO-GO regressions. The overflow's larger UNTRIED lever is
   `flush_class` itself (~487 Ir/event, the non-isolable remainder R24-2 §5.1
   flagged), NOT further bitmap-clear work; a flush_class isolation measurement
   would be the next measurement task.** Measurement only, zero `src/` changes,
   **zero new hooks** (pure shared-prefix subtraction + N/2N; nothing for R24-6
   to track); 7 new bench arms (50 total). Full decomposition, the three-outcome
   verdict table, and the 30-overflow reconciliation arithmetic:
   `R24_5_COLD_ALLOC_FREE_SPLIT_GATE.md` (full report) +
   `R24_5_COLD_ALLOC_FREE_SPLIT_GATE_summary.csv` +
   `docs/perf/_raw_r24_5_run1.log` / `_raw_r24_5_run2.log` (byte-identical `Ir`
   across both runs; 12 reference arms reproduced R23-2/R24-2/R23-3 exactly).
   **2026-07-28 update — DONE (task #384, R24-6), CLOSES the "for R24-6
   tracking" note above (R24-2's `dbg_dealloc_own_thread_with_base` entry)
   and the R23-3-era review flag that motivated this task:** re-scoped
   narrowly after a first attempt (via a different tool) tried to gate
   essentially every `dbg_*` hook in the crate, exploded into a 130+-file
   diff, and was reverted (nothing committed). The actual short list of
   `unsafe fn dbg_*` hooks BOTH marked `unsafe fn` (tier-2 `#[allow(unsafe_code)]`)
   AND reachable from plain `--features production` alone (their own `#[cfg]`
   gate is a subset of `production`'s feature list) is **4 items**, all in
   `src/registry/heap_core_diag.rs` / `src/alloc_core/alloc_core_small_reclaim.rs`:
   `HeapCore::dbg_dealloc_own_thread_with_base` (R23-3, task #372 — NEW, the
   direct motivator), `HeapCore::dbg_push_coarse_only_entry` (R13-1, task
   #271 — pre-existing), and `HeapCore::dbg_push_to_ring` /
   `AllocCore::dbg_push_to_ring` (R6-MS-4 — pre-existing, the oldest tier-2
   entries in the file). **Split decision, not uniform:** the first two each
   have exactly ONE call site (`benches/perf_gate_iai.rs` and
   `tests/class_aware_dirty_oom_latch.rs`) and are now gated behind a new
   `bench-internals` Cargo feature (Option A) — `production` alone no longer
   compiles them in. `dbg_push_to_ring` (both variants) was LEFT AS-IS
   (Option B, doc-only): it has ~20 existing callers across the whole
   `alloc-xthread` test suite (pre-existing R6-MS-4 debt, not the new
   regression), so re-gating it would reproduce the same disproportionate
   diff explosion the reverted first attempt hit, for a documentation-
   precision concern rather than a real production-surface change. Doc notes
   added at both `dbg_push_to_ring` definitions plus a new "R24-6 note"
   paragraph in README.md's "Where unsafe lives" section explaining the
   split. **Verification:** the tier-2 unsafe-audit grep
   (`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`) count is
   UNCHANGED (still counts the `#[allow(unsafe_code)]` line regardless of
   what `#[cfg]` surrounds it — textual, feature-gate-blind by construction,
   confirmed correct before this task started rather than assumed), so
   README's **61** tier-2 figure needed no update and
   `tests/no_stale_doc_references.rs`'s
   `readme_unsafe_inventory_counts_match_reality` tripwire passes untouched.
   Files touched: `Cargo.toml` (new `bench-internals` feature +
   `perf_gate_iai`'s `required-features`), `src/registry/heap_core_diag.rs`
   (2 `#[cfg]` gates widened + 2 doc notes), `src/alloc_core/alloc_core_small_reclaim.rs`
   (1 doc note), `benches/perf_gate_iai.rs` (1 arm's `#[cfg]` widened),
   `tests/class_aware_dirty_oom_latch.rs` (file-level `#![cfg]` widened),
   `.github/workflows/perf-gate.yml` + `.github/workflows/ci.yml` +
   `scripts/check-all.mjs` (feature strings updated so the two re-gated hooks
   keep running under CI/`npm run check`), `README.md` (feature table row +
   "Where unsafe lives" note). No `Cargo.toml`/crate version bump.
   **2026-07-28 update — DONE (task #386, R24-8):** two independent
   investigations into `dealloc_batch` internals. **Inv 1 (ownership cache):
   NO-GO** — a `last_base`/`last_is_owned` cache to skip redundant
   `contains_base` probes measured +3/−44 Ir (inconsistent sign — codegen noise,
   not a real win); the Tier-1 `own_cache` hit is already a single compare, so
   the cache trades one compare for another plus register pressure. Same
   Heisenberg class as R24-3/R24-4. **Inv 2 (STAGE_CAP reduction): GO** —
   LLVM-IR proof confirmed the 4096-byte staging-array zero-init is NOT elided
   (array address escapes into `flush_class`, blocking DSE); reducing STAGE_CAP
   512→64 saves a constant **−4,065 Ir/call** (−47.7% of a 16-block batch-free).
   Implemented with correctness test (`r24_8_dealloc_batch_multi_flush.rs`,
   mutation-confirmed) + 2 new iai arms. Full evidence:
   `R24_8_DEALLOC_BATCH_INTERNALS_GATE.md` +
   `R24_8_DEALLOC_BATCH_INTERNALS_GATE_summary.csv` +
   `docs/perf/_raw_r24_8_baseline.log` / `_raw_r24_8_inv1_after.log` /
   `_raw_r24_8_inv2_stage64.log`.
   **2026-07-28 update — DONE (task #401, R25-7):** the N>64 evidence gap the
   R24 readonly review's P4 flagged (R24-8 measured only N≤64, both of which
   fit in a single flush at either STAGE_CAP — the multi-flush path R24-8
   *introduced* was never measured for Ir) is closed by a real A/B sweep at
   N = 80/81/128/200/512/1024 (six new iai arms) under both STAGE_CAP=64
   (current) and STAGE_CAP=512 (the value R24-8 changed from). **Verdict:
   CONFIRMED CLEAN — STAGE_CAP=64 beats STAGE_CAP=512 at EVERY measured N on
   both Ir (+2,539 to +4,065) and Estimated Cycles (+6,076 to +8,168).** No
   crossover in range. The ΔIr shrinks linearly as N grows (STAGE_CAP=64 does
   more intermediate flush_class calls), at exactly **+109 Ir per extra
   intermediate flush** (linear fit verified to the unit at all 5 multi-flush
   data points: 4065−ΔIr = 109×extra_flushes). Crossover projects at
   **N≈2,700** — far beyond the "tens to low hundreds" R23-7 frames as this
   project's realistic batch size. `git diff HEAD -- src/` is empty (STAGE_CAP
   kept at 64); only 6 new bench arms added (reusable regression infra, same
   precedent as R24-2/R24-8/R25-3). Full evidence:
   `R25_7_STAGE_CAP_BOUNDARY_GATE.md` +
   `R25_7_STAGE_CAP_BOUNDARY_GATE_summary.csv` +
   `docs/perf/_raw_r25_7_stage64.log` / `_raw_r25_7_stage512.log`.
   **2026-07-28 update — NO-GO (task #416, R26-7):** the R25 readonly review's
   suggested prototype — replace `dealloc_batch_small`'s eager
   `[*mut u8; STAGE_CAP]` (512 B zero-init on EVERY call) with a lazily-
   materialized `Option<[*mut u8; STAGE_CAP]>` (materialized only on the first
   overflow block) — was built as a `bench-internals`-gated A/B sibling
   (byte-for-byte copy of `dealloc_batch`/`dealloc_batch_small`, ONLY the `stage`
   representation changed) and measured at N=0/1/8/16/17/64/81/200/1024 via a
   single-binary iai A/B (eager and lazy arms in ONE callgrind pass — cleaner
   than R25-7's two-run protocol since both are functions in the same binary).
   **Verdict: NO-GO — crossover at N=17 (the first overflow block).** The
   hypothesis was DIRECTIONALLY correct (lazy IS cheaper when never materialized:
   ~53 Ir / ~1–2% win, constant across N=0/1/8/16) but the win is tiny AND is
   immediately dominated by per-overflow-block `Option`-discriminant-check
   overhead the moment the magazine overflows: lazy is +55 Ir at N=17, growing
   linearly at ~3 Ir/overflow-block to +3,076 Ir (+1.67%) at N=1024. Realistic
   batch sizes (tens–low hundreds per R23-7) are ALL in the loss regime. **Key
   calibration finding:** the direct A/B isolates the 64-entry (512 B) stack
   zero-init at **~54 Ir (N=0)** — NOT the ~581 Ir a linear extrapolation from
   R24-8/R25-7's 4,065 (the 512-vs-64 array-SIZE delta) would predict; that 4,065
   measured two different array sizes in a shared codegen, not the isolated
   64-entry init (the ~10× Heisenberg gap the direct A/B surfaces — same lesson
   as R24-2 §5.1 / R24-3 / R24-4). **This is the 4th consecutive NO-GO in this
   exact region (R24-3/R24-4/R25-3/R26-7)**, all the "added per-block bookkeeping
   costs more than the one-time savings" class — R26-7 entered it through a
   different door (a FIXED per-call cost, not per-block work), confirming the
   class now spans both cost axes. `dealloc_batch_small` is byte-identical to
   HEAD; the lazy variant stays as `bench-internals`-gated experimental bench
   infra (same retained-arm precedent as R25-7). Full evidence:
   `R26_7_LAZY_STAGE_ARRAY_GATE.md` +
   `R26_7_LAZY_STAGE_ARRAY_GATE_summary.csv` +
   `docs/perf/_raw_r26_7_lazy_stage.log`.
   **2026-07-29 correction (R27-6, task #424):** the retained lazy
   implementation (`dbg_dealloc_batch_lazy` / `dealloc_batch_small_lazy`) and
   its 9 iai bench arms were REMOVED from the tree — retaining a full duplicate
   copy of correctness-sensitive unsafe shipping code as a bench-only duplicate
   creates logic-drift risk with no compensating benefit (the arms measured the
   COPY, not the shipping code). The 4 eager baseline arms
   (`dealloc_batch_fresh_{0,1,8,17}_16b`) measuring the SHIPPING
   `dealloc_batch` were retained (genuine N-grid gap fill below N=16). The
   measurement remains valid; the code is recoverable at commit `8679105`.
   **2026-07-29 update — DONE (task #428, R27-10):** the bitmap-clear
   isolation hook `HeapCore::dbg_overflow_bitmap_clear_pass`
   (`src/registry/heap_core_diag.rs`, R24-2/task #380; re-gated + made
   `unsafe fn` by R25-1/task #395) and its single iai bench arm
   (`dealloc_overflow_bitmap_clear_only_16b`, the 84-Ir/7,451 isolation in
   `R24_2_FREE_BY_MAGAZINE_STATE_GATE.md` §3/§4) were REMOVED. The four
   consecutive NO-GOs above (R24-3/R24-4/R25-3/R26-7) made the hook dead
   measurement infrastructure, and its only caller left a temporary
   magazine-state invariant broken on return (R26 readonly review P2) —
   deleted rather than keep an `unsafe fn` with an implicit process-isolation
   precondition. The measurements stay valid historical evidence at their
   commits; the hook/arm are recoverable in git history. This is a code
   cleanup, NOT a new verdict on the region (still open — next lever is
   `flush_class` isolation per the card above).
   **2026-07-29 update — DONE, MEASUREMENT-ONLY (task #430, R28-1):**
   `flush_class`'s own standalone Ir cost — the ~487 Ir "non-isolable
   remainder" R24-2 §5.1 flagged (`flush_class` + the 8-pointer compaction
   shift + the final magazine push, fused with no workload-level separation
   point at the time) — is finally isolated. A new `bench-internals`-gated
   `pub unsafe fn` hook (`HeapCore::dbg_flush_class_only`,
   `src/registry/heap_core_diag.rs`, gated `bench-internals` **from
   creation**, per CLAUDE.md's benchmark-hook rule — the positive
   `dbg_dealloc_own_thread_with_base` pattern, not the R24-2-era
   safe-`pub-fn`-then-retrofit mistake R25-1 had to fix) calls the real
   `AllocCore::flush_class` standalone on 8 live blocks. Two new bench arms
   (`dealloc_flush_class_only_16b_prefix` / `dealloc_flush_class_only_16b`),
   shared-prefix subtraction, two independent `npm run iai` runs
   (byte-identical Ir): **`flush_class(8 blocks)` = 449 Ir (56.1 Ir/block)**.
   Reconciliation: one overflow event re-measured today at 581 Ir (vs R24-2's
   571 — ~10 Ir/1.8% cross-round drift, unrelated code churn over 5 rounds);
   subtracting the historical 84 Ir bitmap-clear figure (mechanism unchanged
   post-R27-10, just no longer behind a standalone hook) gives a 497 Ir
   remainder, reconciling with R24-2's 487 Ir estimate to within 2.1%.
   `flush_class` alone is 90.3% of that remainder (449/497); compaction +
   final push is a small derived residual (~48 Ir, 9.7%). **Verdict: the
   region is judged likely exhausted for further per-block-cost
   micro-optimization at this scope** — `flush_run`'s per-block work (2
   cheap guards, 1 M2 correctness guard, 1 freelist write, 1 bitmap write,
   with metadata already hoisted once per same-segment run) has no
   un-hoisted cost left to cut without weakening the M2 double-free guard or
   the Э8 run-batching invariant, and the compaction+push residual is now
   confirmed small rather than a hidden larger target. No 5th optimization
   attempt was opened — this measurement's value is closing out the "Next
   trigger" question, joining the 4 prior NO-GOs in this region
   (R24-3/R24-4/R25-3/R26-7) as a 5th data point that the immediate overflow
   arm is tightly compiled. If a future round revisits magazine-overflow
   cost, the more promising angle is reducing how often overflow fires or a
   structural redesign of the fixed bitmap-clear+flush+compact+push
   sequence, not further per-block `flush_class` tuning. No production
   change (`AllocCore::flush_class`/`flush_run` byte-identical; the new hook
   calls the existing function verbatim). Evidence:
   `R28_1_FLUSH_CLASS_ISOLATION_GATE.md` +
   `R28_1_FLUSH_CLASS_ISOLATION_GATE_summary.csv` +
   `docs/perf/_raw_r28_1_run1.log` / `_raw_r28_1_run2.log`.
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
   > - **2026-07-28 (R26-3, task #412) — latency axis CONFIRMED through the REAL `#[global_allocator]`:** R25-5/R26-1 measured the latency/decommit axis via an `AllocCore::new_with_config` bypass; R26-3 re-confirms it through the *un-bypassed* production entry point with an A/B/B/A alternating-process paired judge (20 pairs, `scripts/paired-ab-runner.mjs`): cap8 is statistically-significantly faster than cap4 (paired t=12.212 ≫ crit 2.101, sign test 20/20 favoring cap8; mean Δ=23.667 ms ≈ 16% faster at cap8), and the decommit cliff reproduces *deterministically* at the process level (cap4 `decommit_calls_total=9` in all 40 launches; cap8=0 in all 40). A same-vs-same control (cap4 vs cap4) shows t=−0.282 (well under crit), 8/12 sign split — harness honest. The per-run decommit count (9) is lower than R25-5's bypass (~20); the direction and the 4→8 cliff reproduce exactly (see `R26_3_PRODUCTION_TEARDOWN_AB_GATE.md` §2 for the hypothesis). **No production change** (`DEFAULT_POOL_SEGMENTS` stays 4); the promote-4→8 decision remains pending. Evidence: `R26_3_PRODUCTION_TEARDOWN_AB_GATE.md` + `R26_3_PRODUCTION_TEARDOWN_AB_GATE_summary.csv` + `docs/perf/_raw_r26_3_production_teardown_ab.log` + `docs/perf/paired_ab_runs/2026-07-28T16-25-31-476Z.json`.
   > - **2026-07-28 (R26-9, task #418) — CLOSED without a design attempt.** R26-9's own explicitly-recorded conditional gate: "design (not implement) an adaptive/process-wide pool budget, ONLY if R26-1's corrected RSS gate exposes a real cap-8 memory penalty." R26-1's corrected data does NOT expose a cap-8 penalty — it is RSS-NEUTRAL (identical to cap=4 at every thread count measured, §0/§3 of `R26_1_POOL_CAP_RSS_SUBPROCESS_GATE.md`). There is no cap-specific RSS cost for an adaptive/process-wide budget to manage relative to the current fixed cap=4 default; the case for such a design would have to rest on the linear-in-thread-count multi-thread scaling ALONE, but that scaling is present IDENTICALLY at every cap including the current default (R25-5's original finding, reconfirmed unchanged by R26-1) — it is not something raising `pool_segments` introduces or worsens, so it is not a NEW finding this task's gate was scoped to react to. Dispatching a design task against a gate its own trigger condition explicitly does not meet would be speculative work, the same reasoning R25-6's original closure (and R25-9's) already established this project's practice for. If the multi-thread linear-scaling concern itself (independent of any cap-8-specific penalty) is judged worth a design in a future round, that would be a new task with its own trigger, not a reinterpretation of this one's.

   R24-10 (task #388) established the *mechanism* behind the 1024B teardown
   residual (the segment decommit/release/re-reserve lifecycle Mechanism-2's
   pool was built to absorb) but did not re-measure this specific bench's 1024B
   number against mimalloc *after* Mechanism-2 landed, nor decide which of three
   candidates (i cap-exceeded / ii decay / iii batch-flush) dominates the
   *current* residual. R24-11 found that **neither this perf index nor the
   correctness sibling had tracked "was Mechanism-2's effectiveness against
   this bench's 1024B number ever re-measured after it landed"** — exactly the
   silently-dropped-follow-up class the CLAUDE.md "Phased delivery" convention
   (R18-8 / R22-3 lessons) exists to prevent; this entry closes that gap.
   Measured verdict: **(i)** — the 4-segment / 16 MiB per-thread pool cap
   (`small_segment_pool_config.rs`, default `pool_segments=4`,
   `pool_byte_cap=16 MiB`) is exceeded by this bench's full-teardown-every-
   iteration shape at 1024B (248 decommit/release events), where smaller sizes
   fit in one segment and never trip the cap (0 events, parity with mimalloc).
   The pool cap exists to bound retained RSS, so raising it is an RSS-vs-
   throughput trade requiring an RSS gate — **flagged as the next step, NOT
   attempted in R24-11** (no default changed, no production file touched). The
   bench's doc comment was simultaneously rewritten from its stale "until
   task #51 lands Mechanism-2" framing to its current **regression-canary**
   role (it is the only churn bench that times teardown inline; both siblings
   use `ChurnTeardownGuard`). Full evidence, the cross-size teardown-cost
   decomposition, and the per-event-cost caveat:
   `R24_11_TEARDOWN_RESIDUAL_ROOTCAUSE.md`.

   **R25-5 (task #399) — the sweep itself.** Ran `pool_segments` = 4/8/16/32
   with a generous 256 MiB `pool_byte_cap` against the exact
   `bench_global_alloc_churn_with_teardown`@1024B shape, per the two-axis
   ("do not promote from the single-thread latency result alone")
   constraint the R24 readonly review's P2 section and this item's own
   "Next trigger" both required. A new standalone probe
   (`examples/r25_5_pool_cap_sweep_probe.rs`) was needed rather than
   extending the criterion bench directly (only Sefer has a `pool_segments`
   knob, not mimalloc/System) — it copies `bench_global_alloc_churn_with_teardown`'s
   `churn_prefill`/`churn_step`/`churn_teardown` primitives byte-for-byte and
   measures the latency/decommit axis via `AllocCore::new_with_config`
   directly (mirroring `pool_cap_sweep_spread_and_drain`'s own established
   pattern) and the RSS axis via `SeferAlloc::with_config` on N concurrent
   OS threads (mirroring `first_alloc_process.rs`'s N-concurrent-heap RSS
   pattern), reading peak RSS/commit through `proc_probe::snapshot()`
   (this project's established same-instant memory probe). **A real
   methodology pitfall was caught and fixed mid-task**: a naive sequential
   prefill→churn→teardown loop measured ZERO decommits at every swept
   value including the cap=4 baseline — directly contradicting R24-11's own
   248-decommit finding — because criterion's actual
   `iter_batched(.., BatchSize::SmallInput)` semantics batch MANY `setup`
   calls concurrently-live before timing `routine` (`criterion-0.5.1/src/bencher.rs:236`),
   a fundamentally different memory-pressure shape than one cycle at a
   time; the probe was fixed to reproduce that exact batched-setup shape,
   after which the decommit signal reproduced correctly (verified via
   `AllocCore::dbg_pool_cap()`/`dbg_pooled_count()` self-checks — closing
   R24-11 §1's own documented `SeferAlloc`-reachability gap for this
   counter). **Result:** cap=4→8 eliminates the entire decommit residual
   (20→0 in R25-5's own harness units) at LOWER RSS/commit cost (cap=4's
   residual decommit-then-reserve churn itself costs real RSS/commit that
   steady-state cap≥8 avoids); cap=16/32 add nothing further at either axis
   (this workload's demand tops out at 6-7 concurrently-touched segments,
   confirmed via the self-verifying diagnostic). The multi-thread (8T/32T)
   axis confirms the review's warned-about linear-in-thread-count RSS
   multiplication IS present (~8× at 8 threads, ~32× at 32 threads,
   relative to the 1-thread delta) but that multiplication is present
   IDENTICALLY at every swept cap including the current default — cap=8
   does not make it worse, and in fact its per-thread footprint is smaller
   than cap=4's. **Verdict: GO-CANDIDATE for `pool_segments=8`**, flagged
   for a future default-raise decision (task brief's explicit constraint:
   measure and report, do not change the default in this task). Full
   evidence, the batching-pitfall counterfactual, and the RSS-cost
   mechanism explanation: `R25_5_POOL_CAP_SWEEP_GATE.md`.

   **2026-07-28 (R26-2, task #411) — CORRECTION.** The R25-5 paragraph above
   claims cap 4→8 wins on BOTH the latency/decommit axis AND the RSS/commit axis
   simultaneously with no tradeoff. That claim is NOT proven: only the
   latency/decommit axis is confirmed. The RSS axis is invalidated (not merely
   uncertain) by a methodological bug — `examples/r25_5_pool_cap_sweep_probe.rs`'s
   `measure_rss_axis` ran the cap 4/8/16/32 arms and 1/8/32 thread counts
   sequentially in ONE `--release` process, but the registry's slot lifecycle
   (`src/registry/heap_registry.rs`) means an already-materialised slot keeps its
   first-claim config: `claim_with_config` (~:209, first materialisation
   ~:226-246; re-claim ~:247-300 "silently wins"), `recycle` (~:342) returns
   slots to `free_slots` on thread exit, `pick_slot` (~:316-322) pops them first,
   and the only always-compiled mismatch signal is the `CONFIG_CONFLICTS` counter
   (~:263) — the `debug_assert!` (~:285) is compiled out of `--release`. So rows
   labelled cap=8/16/32 may have executed under cap=4. The latency/decommit axis
   is unaffected: it uses `AllocCore::new_with_config` directly (no registry) and
   self-verifies via `assert_eq!(resolved_cap, pool_segments)`. Full detail, the
   list of now-unreliable vs still-reliable report sections, and the re-stated
   (latency-axis-only) GO-CANDIDATE: `docs/perf/R25_5_POOL_CAP_SWEEP_GATE.md` §8.
   Remeasurement is tracked as task #410 (R26-1, subprocess-per-arm isolation);
   R25-6's closure (which rested entirely on the "wins on both axes, no tradeoff"
   claim) is reopened and tracked as task #418 (R26-9), conditional on #410.

   **2026-07-28 (R26-1, task #410) — RSS AXIS REMEASURED (subprocess-per-arm).**
   The remeasurement the R26-2 correction pointed to is now complete. A new
   probe (`examples/r26_1_pool_cap_rss_subprocess_probe.rs`) runs each
   `(pool_segments, thread_count, repetition)` tuple in its OWN freshly-spawned
   OS process, eliminating the registry-slot-reuse bug by construction. Each
   child claims heaps via `HeapRegistry::claim_with_config` (not `SeferAlloc`,
   sidestepping its private TLS), then hard-`assert!`s BOTH (i) every claimed
   heap's `HeapCore::dbg_pool_cap()` (new thin delegation added to
   `src/registry/heap_core_diag.rs` this task) equals the requested
   `pool_segments`, AND (ii) `config_conflicts_total()` delta is exactly 0 —
   per the review's "Required correction" recipe. All 36 child runs (4 caps ×
   3 thread-counts × 3 reps) passed both self-checks. **Result: the R25-5
   "cap 4→8 wins on RSS too (lower RSS)" claim does NOT reproduce.** Under
   isolation, all four caps produce statistically identical RSS at every thread
   count (cap=4 1T=13,368 KiB vs cap=8 1T=13,372 — 0.03% diff; cap=4 32T=
   423,448 vs cap=8 32T=423,444 — identical). The inter-cap difference at every
   cell is smaller than the intra-cap spread across 3 reps. R25-5's dramatic
   "cap=8 is 34% cheaper on RSS at 8T" was an artifact of cap=8's threads
   silently reusing cap=4's already-committed segments (measuring incremental-
   over-warm, not fresh footprint). **Re-stated verdict:** the GO-CANDIDATE
   for `pool_segments=8` survives on the latency/decommit axis ALONE
   (eliminates 20 decommits/run at RSS-NEUTRAL cost — no RSS benefit, but also
   no RSS penalty). The "wins on BOTH axes simultaneously, no tradeoff"
   framing is refuted for the RSS axis. The DEFAULT-CHANGE decision remains
   pending — the corrected case is positive but weaker than R25-5's
   invalidated "faster AND cheaper." Full evidence:
   `R26_1_POOL_CAP_RSS_SUBPROCESS_GATE.md` +
   `R26_1_POOL_CAP_RSS_SUBPROCESS_GATE_summary.csv` +
   `docs/perf/_raw_r26_1_pool_cap_rss_subprocess_probe.log`.

   **2026-07-28 (R27-1, task #419) — CORRECTION: the pending DEFAULT-CHANGE decision was phrased as a one-knob no-op.** Every prior paragraph in this entry phrased the pending decision as "promote `DEFAULT_POOL_SEGMENTS` 4→8". That phrasing is a literal NO-OP: `AllocCore::new_with_config` resolves the effective pool cap as `min(pool_segments, pool_byte_cap / SEGMENT)` (`src/alloc_core/alloc_core.rs:837-839`), and the current `DEFAULT_POOL_BYTE_CAP = 16 MiB` (`src/alloc_core/small_segment_pool_config.rs:117`, with `SEGMENT = 4 MiB`) already resolves to `16 MiB / 4 MiB = 4`, so `min(8, 4) = 4` — editing ONLY `DEFAULT_POOL_SEGMENTS` from 4 to 8 leaves the allocator's behaviour byte-identical. To obtain an effective default cap of 8 the decision must change BOTH knobs: `(pool_segments, pool_byte_cap) = (4, 16 MiB) → (8, 32 MiB)`, which doubles the documented maximum retained committed pool memory per materialised heap (16 MiB → 32 MiB; at 32 concurrent heaps, up to 1 GiB) — an RSS-vs-throughput trade, not the cost-free change the one-knob phrasing implied. None of the R25/R26 A/B arms (R25-5, R26-1, R26-3 — all run at a generous 256 MiB `pool_byte_cap` precisely so the byte ceiling would never bind) ever measured the one-constant default edit the docs proposed; they measured the EFFECTIVE cap (an explicit config with both knobs raised together), so their latency/decommit GO-CANDIDATE survives, but the *default-promotion* task was never actually measured as written. This is purely a task-framing correction — no `src/` default is changed in R27-1. Note also that `SmallSegmentPoolConfig` is builder-only today (`SmallSegmentPoolConfig::new().pool_segments(n).pool_byte_cap(bytes)`); there is NO named preset constant, so any future "throughput recipe" (8/32, 16/64) is documentation guidance, not a shippable constant, unless a later task adds one. Verified by a new test (`tests/small_segment_pool.rs::paired_knob_promotion_is_not_a_noop`) asserting `(pool_segments=8, pool_byte_cap=32 MiB)` resolves to 8 while the malformed `(pool_segments=8, pool_byte_cap=16 MiB)` resolves to 4.

   **2026-07-28 (R27-2, task #420) — CORRECTION: R26-1's "RSS-NEUTRAL" verdict and R26-9's closure rest on an incomplete premise; both are REOPENED.** The Status/Current-number/Next-trigger headline bullets above were updated this round (they are this entry's designed-mutable summary; the historical narrative below is preserved unchanged per the append-don't-rewrite convention) to stop carrying the "RSS-neutral, no cap-specific RSS cost" framing, because two independent reasons show that framing is false:

   **(a) R26-1's RSS probe never proved victim activation.** `examples/r26_1_pool_cap_rss_subprocess_probe.rs:124` sets `RSS_BATCH_SIZE = 50`, i.e. `50 × 256 × 1024 bytes ≈ 12.5 MiB` logical prefill — this fits inside the current 4-segment/16 MiB retention region. The probe records `verified_cap` and `config_conflicts_delta` (proving it RAN cap 8, not that it USED slots 5–8) but records NO `dbg_pooled_count` / pool-occupancy high-water mark and NO decommit counters, and asserts nothing about whether cap 4 was ever saturated or whether cap 8 ever retained a 5th segment. Its "demand tops out at 6 segments" framing was measured on the SEPARATE `LATENCY_BATCH_SIZE=120` axis (the R25-5/R26-3 latency probe), not this RSS axis's batch-50 workload — the demand proof does not transfer across different batch sizes. So identical peak-live-set RSS across caps 4/8/16/32 in R26-1's table is EXPECTED regardless of whether a real retention cost exists, because the extra capacity was never proven to be exercised.

   **(b) R26-3's own committed raw log directly contradicts "no cost."** `grep 'rss_after_kib=' docs/perf/_raw_r26_3_production_teardown_ab.log` shows cap8 arms reporting `rss_after_kib=34576` (~33 occurrences) and cap4 arms reporting `rss_after_kib=30476` (~32 occurrences), a deterministic, repeatable **+4,100 KiB ≈ exactly one 4 MiB segment** retained after teardown, at the pressure-producing batch-120 workload (the SAME workload R26-3 uses for its latency claim, and the one whose "demand tops out at 6 segments" finding actually holds). This is a real, measured retention cost sitting in this project's own committed evidence, and it directly contradicts the "no cap-specific RSS cost" framing task #418 used to close the adaptive-design task.

   **Peak-live-set RSS is also the wrong sole metric for a retention policy** — the pool's actual trade-off appears AFTER teardown (does the cache stay warm/committed while the live working set is gone), which is exactly the axis R26-1's probe never measured (it took only one peak-during-load snapshot) and R26-3 happens to expose as a side effect of its own instrumentation. Per the review's project-improvement #6 (`docs/reviews/2026-07-28-r26-readonly-review.md`), this is fixed by correcting the mutable current-state HEADLINE bullets (done above) rather than leaving the misleading "RSS-neutral, no adaptive trade-off" headline active with only a caveat appended below. The R25-5/R26-2/R26-1/R27-1 narrative paragraphs and the R26-3/R26-9 card bullets are all preserved unchanged (append-only); this R27-2 paragraph is the correction. **Accordingly: the R26-9 closure bullet above ("2026-07-28 (R26-9, task #418) — CLOSED without a design attempt", in the current-state card) is now known to rest on an incomplete premise — its "there is no cap-specific RSS cost" justification is contradicted by R26-3's own committed raw log — and is REOPENED, tracked as task #423 (R27-5), which redoes the adaptive-design evaluation once task #421 (R27-3)'s proper retention gate lands.** Docs-only; no `src/`/`examples/`/`benches/` file touched; no re-measurement attempted (that is task #421 / R27-3, not this task).

   **2026-07-29 (R27-3, task #421) — DONE: the proper RETENTION gate has landed; the cap-specific RSS cost is now QUANTIFIED and victim-activation-PROVEN.** This closes the measurement debt R27-2 flagged. A new probe (`examples/r27_3_pool_retention_gate.rs`) runs each `(pool_segments, pool_byte_cap, thread_count, repetition)` = `(4,16MiB)/(8,32MiB)` × `1/8/32` × 3 reps (18 child processes, subprocess-per-arm isolation) at the **pressure-producing batch 120** (NOT R26-1's batch 50), recording peak-live AND post-teardown AND post-idle (100ms/1s/2s) RSS/commit, final AND high-water `dbg_pooled_count` per heap, decommit/release/reserve counters, and an explicit `dbg_drain_small_pool` reclaimability demonstration. Every arm hard-asserted the R26-4 config-identity contract (`verified_cap == pool_segments`, `config_conflicts_delta == 0`, `process_identity = subprocess`); all 18 passed. **Victim activation PROVEN for both arms** (the counterfactual R26-1's batch-50 RSS axis lacked): cap-4 arms asserted `decommit_delta > 0` (measured 274/1226/1446 at 1/8/32T — the 6-segment demand provably overflows the 4-segment pool); cap-8 arms asserted `pooled_hw_max > 4` (measured 6, retention beyond cap-4's bound) AND `decommit_delta == 0` (cap 8 absorbs the demand). **Result: cap 8 DOES retain more than cap 4 — REAL and PROVEN, not RSS-neutral.** Post-teardown RSS Δ (cap8−cap4, median): 1T=+8,096 KiB; 8T=+65,560 KiB; 32T=+261,424 KiB — **~+8 MiB/heap (~2 segments), scaling linearly** (8,096 / 8,195 / 8,170 per heap). It decomposes into ~+4 MiB/heap pooled (5 vs 4 segments, drainable via `dbg_drain_small_pool`, proven by the post-drain RSS drop) + ~+4 MiB/heap committed-non-pooled (residual after drain). **Decay (§3): the small-pool decay shares the large-cache 1000ms interval but is EVENT-DRIVEN (fires only on `reserve_small_segment`, no background thread) — pure idle does NOT reclaim it; RSS and `dbg_pooled_count` are flat across the 2s idle window (empirically confirmed, `pooled_predrain == pooled_final`); the retention persists until explicit drain / thread-exit.** This is materially larger than R26-3's +4,100 KiB side-channel observation (a lower bound from a non-controlled measurement; cap8's absolute ~34.7 MiB matches R26-3's 34.6 MiB closely, but this gate's cleaner subprocess-isolated cap4 measures 26.7 MiB vs R26-3's 30.5 MiB). **Implications:** the paired default-change (R27-4/#422) is now a quantified trade (eliminate 20 decommits/run at ~+8 MiB/heap, ~+255 MiB at 32 heaps); R26-9's closure condition ("ONLY if R26-1 exposes a real cap-8 penalty") is now MET, so R27-5/#423 (adaptive/process-wide budget re-evaluation) proceeds — and the idle-no-decay finding means an adaptive design wanting idle reclamation cannot rely on the existing decay interval (needs an active drain/scavenge). **No production change** (`DEFAULT_POOL_SEGMENTS`/`DEFAULT_POOL_BYTE_CAP` remain `4`/`16 MiB`; no `src/` file touched at all). Evidence: `R27_3_POOL_RETENTION_GATE.md` + `R27_3_POOL_RETENTION_GATE_summary.csv` + `docs/perf/_raw_r27_3_pool_retention_gate.log`.

   **2026-07-29 (R27-4, task #422) — DONE: the LATENCY half of the paired-default decision is measured at the REAL byte cap through the REAL `#[global_allocator]`; the latency win SURVIVES.** This is the missing measurement R27-1 flagged: every prior A/B (R25-5, R26-1, R26-3) measured `pool_segments=4` vs `8` at a generous 256 MiB `pool_byte_cap` ceiling nobody would ship; the REAL default change is the PAIRED `(pool_segments, pool_byte_cap) = (4,16 MiB) → (8,32 MiB)` (task #419), and nothing had measured THAT exact pair through the real un-bypassed entry point. Two new example binaries (`examples/r27_4_real_default_ab_cap4.rs` / `_cap8.rs`) install a REAL `#[global_allocator]` `SeferAlloc` via the const-fn `with_config` path with the actual byte caps (16/32 MiB) and run the exact churn-with-teardown@1024B shape, judged by the runner's documented real-claim default of 20 A/B/B/A pairs (80 process launches). The byte cap resolves to the SAME effective cap as R26-3's 256 MiB arms (`min(4,4)=4`, `min(8,8)=8` — `pool_byte_cap` is consumed ONLY by that `min()`, `src/alloc_core/alloc_core.rs:839`), so the win was expected to reproduce; it does, which closes the "never measured at the actual shipping config" gap. **Verdict: cap8 is statistically-significantly faster than cap4 at the REAL paired default** (paired t=8.114 ≫ crit 2.101, sign test 19/20 favoring cap8, zero ties; mean Δ=21.404 ms, cap4 mean 96.71 ms vs cap8 mean 75.31 ms ≈ **22% faster** at cap8), and the decommit cliff reproduces deterministically (cap4 `decommit_calls_total=9` in all 40 launches; cap8=0 in all 40). A same-vs-same control (cap4 vs cap4) shows t=−0.434 (well under crit), 11/9 sign split — harness honest. **Methodological note: these binaries fix R26-3's warm-up-placement timing bug from the start** — the warm-up batch runs BEFORE `t0` so `elapsed_ns` covers exactly 8 timed batches × 120 cycles = 960 cycles (NOT R26-3's 9/1080, where `t0` preceded the warm-up and timed all 9); this is why R27-4's absolute ms are NOT directly comparable to R26-3's raw ms, though the per-batch delta is constant (R26-3: 2.63 ms/batch; R27-4: 2.68 ms/batch — within noise). **Combined latency+retention picture for the pending default decision:** the trade is now fully quantified on BOTH axes at the REAL config — cap8 trades ~+8 MiB/heap post-teardown retention (R27-3, cited directly: ~+4 MiB pooled/drainable, ~+4 MiB committed-non-pooled, linearly scaling to ~+255 MiB at 32 heaps, does not decay during idle) for ~22% lower latency and a fully-eliminated decommit churn, both measured at the REAL config through the real entry point. Whether that trade is net-positive is the deployment-context judgment a SEPARATE decision task makes — this task is measurement-only (no `src/` default changed; `DEFAULT_POOL_SEGMENTS`/`DEFAULT_POOL_BYTE_CAP` remain `4`/`16 MiB`). Evidence: `R27_4_REAL_DEFAULT_AB_GATE.md` + `R27_4_REAL_DEFAULT_AB_GATE_summary.csv` + `docs/perf/_raw_r27_4_real_default_ab.log` + `docs/perf/paired_ab_runs/2026-07-28T23-55-35-517Z.json` (main) + `docs/perf/paired_ab_runs/2026-07-28T23-56-02-705Z.json` (control).

   **2026-07-29 (R27-5, task #423) — DONE (DESIGN-ONLY): the adaptive/process-wide pool-budget design is written; recommendation is Option 1 (keep the paired 4/16 MiB default, document an 8/32 MiB throughput recipe), NOT the adaptive design.** R26-9's closure condition ("ONLY if a corrected RSS gate exposes a real cap-8 memory penalty") was met by R27-3 (task #421): cap 8 retains ~+8 MiB/heap post-teardown, scaling linearly to ~+255 MiB at 32 heaps, victim-activation-proven, and does not decay during idle. The design (`docs/perf/R27_5_ADAPTIVE_POOL_BUDGET_DESIGN.md`) develops the review's adaptive-growth sketch in full (per-heap `effective_pool_cap` grown on the cold `reserve_small_segment`/`release_or_pool_empty_segment` path after a calibrated cap-full-event threshold derived from R27-3's 274/1226/1446 decommit counts; a process-wide `GROWTH_TOKENS_REMAINING` atomic touched ONLY on that cold path + trim, satisfying the no-hot-path-atomic hard constraint; decline-on-budget-exhaustion; reset-on-recycle in `trim_for_recycle`), then critiques it honestly against R27-3/R27-4's data: (1) the latency win is BINARY (cap ≥ demand ⇒ 0 decommits, else N), so a global budget that grows only SOME heaps yields a bimodal fleet worse-aggregate than cap-8-for-all; (2) under the uniform-pressure workloads this project has measured (every heap saturates cap 4), the budget is either never binding (= cap-8-for-all = Option 2 with extra steps) or splits the win — the budget only helps under UNEVEN pressure, and no committed workload exhibits that shape; (3) the idle-shrink-back sub-problem is UNSOLVED within this project's documented no-background-thread anti-precedent (R27-3 proved idle does not decay the pool), so a once-grown heap stays grown until thread-exit — for long-lived thread pools this converges to cap-8-for-all's retention footprint anyway. Verdict: CONDITIONAL-GO-on-paper / RECOMMEND-OPTION-1 — the design is deferred (matching R17-10/R25-8's convention) until a measured uneven-pressure victim + a stage-1 counter calibration justifies the heuristic, and Option 1 (document the recipe) is the recommendation for THIS round because it is near-zero complexity, preserves the safety-first default for memory-constrained/high-thread-count deployments, and makes the 22% latency win discoverable via a one-line builder opt-in with fully-measured numbers. Option 2 (promote 8/32) is held as a credible alternative if future deployment evidence shows throughput dominates the general-purpose case. **No `src/` file touched (design-only); no default changed** (`DEFAULT_POOL_SEGMENTS` / `DEFAULT_POOL_BYTE_CAP` remain `4` / `16 MiB`); no commit made (tree left unstaged for review). Evidence: `docs/perf/R27_5_ADAPTIVE_POOL_BUDGET_DESIGN.md` (synthesizes R27-3's retention data + R27-4's latency data; no re-measurement).

   **2026-07-29 (R27-11, task #429) — EVALUATED, NOT OPENED: the reservation-only
   overflow tier conditional (`docs/reviews/2026-07-28-r26-readonly-review.md`'s
   "conditional alternative") was checked against its own two required triggers.**
   Trigger 1 (committed retention cost unacceptable for a default) **fires** —
   already established by R27-3/R27-4 above, and independently corroborated by
   R27-5's unrelated design route reaching the same "keep the conservative
   default" conclusion. Trigger 2 (an isolated cost breakdown showing OS-reserve +
   segment-table-setup + metadata-reinit is a MATERIAL fraction of the segment
   lifecycle cost, AFTER the unavoidable recommit/page-fault cost is accounted
   for) is **unmeasured** — no report in this project's history decomposes the
   decommit→reserve churn cost into those two shares; R27-4 measured only the
   aggregate win from eliminating the churn entirely. Per the task's own explicit
   rule ("do NOT start unless BOTH triggers fire" / "if trigger 2 fails, this
   design must not be opened"), an unmeasured trigger cannot be treated as fired,
   so **no design document was written and no `src/` file was touched.** Filed as
   item 15 in the `[D]` "Deferred designs" tier below (was previously an unfiled
   review idea) with the exact Stage-1 cost-breakdown measurement a future round
   would need to run to actually resolve trigger 2, so this does not become a
   silently-dropped follow-up. Evidence: item 15 in `[D]` below.

### [D] Deferred designs — implement only if trigger/victim materializes

2. **R17-10 — batched deferred reclaim (sub-design A + B).**

   > **Current state**
   > - **Status:** design-only, deferred.
   > - **Current number/verdict:** CONDITIONAL — sub-design A (batch the per-block decommit check) is independent and small; sub-design B (deferred cross-segment finalization) is conditional on a §5.1 stage-1 finding that a non-negligible fraction of `drain_dirty_segments` sweeps empty >1 segment.
   > - **Next trigger:** a future round chooses to implement sub-design A; sub-design B is gated on its §5.1 stage-1 finding (check BEFORE writing B's code).
   > - **Evidence:** `R17_10_BATCHED_DEFERRED_RECLAIM_DESIGN.md` §6 + §7 (lines 555–668).

   Design-only; proposes a future-round implementation + dual-axis wall-clock
   gate. Sub-design
   A (batch the per-block decommit check) is independent and small; sub-design B
   (deferred cross-segment finalization within one `drain_dirty_segments` sweep)
   is CONDITIONAL on a §5.1 stage-1 finding that a non-negligible fraction of
   sweeps empty >1 segment — check BEFORE writing B's code. Evidence:
   `R17_10_BATCHED_DEFERRED_RECLAIM_DESIGN.md` §6 + §7 (lines 555–668).
3. **R11-7 page-run layer (R12-13 deferred).**

   > **Current state**
   > - **Status:** NO-GO now; kept as a reusable CONDITIONAL-GO starting point.
   > - **Current number/verdict:** NO-GO — no demonstrated victim exists today.
   > - **Next trigger:** a real workload allocating thousands of simultaneously-live 1.25–2.0 MiB (or larger uniform-size) objects that is `MAX_SEGMENTS`-bound or OS-reservation-syscall-bound (not RSS-bound — solved wherever `exact-span-large` is enabled).
   > - **Evidence:** `R12_13_PAGE_RUN_LAYER_DEFERRED.md` §4 (lines 188–237).

   NO-GO now; the complete design remains a reusable CONDITIONAL-GO starting
   point IF a real workload
   materializes that allocates thousands of simultaneously-live 1.25–2.0 MiB (or
   larger uniform-size) objects and is measured `MAX_SEGMENTS`-bound or
   OS-reservation-syscall-bound (not RSS-bound — that is solved wherever
   `exact-span-large` is enabled). No demonstrated victim exists today.
   Evidence: `R12_13_PAGE_RUN_LAYER_DEFERRED.md` §4 (lines 188–237).
4. **R14-7 expandable / chained `SegmentTable`.**

   > **Current state**
   > - **Status:** design-only, deferred.
   > - **Current number/verdict:** design-only — implement only when one of three triggers fires.
   > - **Next trigger:** (1) a workload needing >`MAX_SEGMENTS`−1 (4095) simultaneously-live Large objects, OR (2) a `MAX_SEGMENTS` raise stops being "cheap" by §1's criteria, OR (3) page-run (item 3) is pursued (then re-evaluate this doc's tagged-`SegmentId` widening alongside it).
   > - **Evidence:** `R14_7_EXPANDABLE_SEGMENT_TABLE_DESIGN.md` §5 (lines 374–391).

   Design-only; implement ONLY when (1) a real workload
   needs >`MAX_SEGMENTS`−1 (4095) simultaneously-live
   Large objects, OR (2) a future `MAX_SEGMENTS` raise stops being "cheap" by
   §1's criteria, OR (3) page-run is pursued (then re-evaluate this doc's
   tagged-`SegmentId` widening alongside it — both touch the same header field).
   Evidence: `R14_7_EXPANDABLE_SEGMENT_TABLE_DESIGN.md` §5 (lines 374–391).
5. **R10-4 run-origin oracle (class-align carve).**

   > **Current state**
   > - **Status:** design-only, CONDITIONAL GO.
   > - **Current number/verdict:** CONDITIONAL GO — sound with a real density gain (wide classes 2/1/1 → 3/2/2), but only worth it if `medium-classes-wide` is pursued (itself NO-GO'd for `production` on a large-realloc regression).
   > - **Next trigger:** `medium-classes-wide` re-opened.
   > - **Evidence:** `R10_4_RUN_ORIGIN_ORACLE_DESIGN.md` §0/§7/§8.

   DESIGN-ONLY, CONDITIONAL GO. Sound and real density gain (wide classes
   2/1/1 → 3/2/2), but only worth it
   if `medium-classes-wide` is pursued — which is itself NO-GO'd for
   `production` (large realloc regression). Re-evaluate only if wide classes are
   re-opened. Evidence: `R10_4_RUN_ORIGIN_ORACLE_DESIGN.md` §0/§7/§8.
6. **R22-16 — remap-instead-of-copy for the medium→Large promotion memcpy
   (MediumExtent sub-path).**

   > **Current state**
   > - **Status:** design-only; verdict corrected in R23-4 (the original whole-NO-GO framing is superseded).
   > - **Current number/verdict:** **NO-GO** for whole-segment remap (base-address stability, unaffected) and for Windows (separate section-object blocker); **CONDITIONAL-GO** for Linux sub-region remap pending a correctness prototype. The MediumExtent one-object-per-segment redesign is a separate CONDITIONAL-GO, gated on an unrun Stage-1 workload-shape measurement.
   > - **Next trigger:** a Linux sub-region `mremap` correctness prototype (adds the FFI surface + the "never free-list-push a remap-vacated offset" discipline §10.3 identifies); MediumExtent needs its Stage-1 measurement.
   > - **Evidence:** `R22_16_PROMOTION_REMAP_DESIGN.md` §10 (the R23-4 correction; original §0–§9 preserved verbatim).

   DESIGN-ONLY. **Status pending correction (task
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
   **2026-07-27 update — DONE (task #373, R23-4):** the correction landed.
   Independently re-verified §2.4's blocker against the CURRENT source (not
   just re-trusting the flagged premise): `carve_block`
   (`alloc_core_small.rs:1429-1557`) and `carve_batch` (`:1608-1721`) are
   both monotonically forward-only bump advances, and a full `grep -rn
   "set_bump" src/` found the ONLY backward reset
   (`decommit_empty_segment_impl`, `alloc_core_small_pool.rs:751,812`) is
   reachable, on every production path, only after
   `dec_live_and_maybe_decommit` has confirmed the WHOLE segment's
   `live_count == 0` — so a live medium block's byte range is provably
   exclusive for its entire lifetime, exactly as this update's prior
   paragraph expected. §2.4's neighbor-liveness blocker is retracted;
   §3.1-3.3's base-address-stability blocker for whole-segment remap is
   confirmed independent and unaffected. **New finding beyond what was
   expected going in:** traced `try_promote_to_large`
   (`src/registry/heap_core_free.rs:1276-1343`) and found today's
   memcpy-based promotion frees the source block through the perfectly
   ordinary `dealloc` → `BinTable` free-list path (medium classes share the
   same `BinTable`/`SMALL_CLASS_COUNT` indexing as small classes,
   `size_classes.rs:138,165,95-111`) — so the "permanent hole" bookkeeping
   concern §3.3 originally flagged is **only partially solved by
   monotonicity**: bump will never re-carve a vacated span (solved), but
   nothing stops that span's offset from being pushed onto `BinTable` and
   reissued via ordinary free-list reuse (NOT solved — this is a new design
   discipline a remap implementation must observe, not a currently-existing
   blocker). Also disclosed, not silently fixed: a `#[doc(hidden)]`
   test-only hook (`dbg_force_decommit_retain_for`,
   `alloc_core_small_pool.rs:694-707`) resets bump without itself checking
   `live_count` (trusting its one test caller to have emptied the segment
   first) — unreachable from any production alloc/dealloc/realloc path, so
   it does not weaken the production-path argument, but is exactly the kind
   of edge case this task was told to look for and report honestly rather
   than paper over. **Revised verdict:** NO-GO for whole-segment remap
   (base-address stability, unaffected) and NO-GO for Windows specifically
   (a third, separate blocker — placeholder-VA/`MEM_REPLACE_PLACEHOLDER`
   only moves section-object-backed mappings, and `crates/vmem` uses plain
   anonymous `VirtualAlloc` with no section handle, §1.2 — untouched by
   this correction); **CONDITIONAL-GO for LINUX SUB-REGION remap
   specifically**, pending a correctness prototype that adds the `mremap`
   FFI surface AND builds the still-missing "never free-list-push a
   remap-vacated offset" discipline §10.3 identifies as the one remaining
   unbuilt piece. A DESIGN-ONLY (not implemented) sketch of that prototype's
   minimal scope (Linux-only, page-aligned medium block only, exact-span
   remap, new Large/extent registration, vacated-range exclusion from
   `BinTable`, mandatory memcpy fallback on any remap error) is recorded.
   Full derivation: `R22_16_PROMOTION_REMAP_DESIGN.md` §10 (the correction
   section) — original §0-§9 content preserved verbatim per this project's
   "append, don't rewrite" convention.
14. **R25-8 — run-encoded free batch (arithmetic free list).**

    > **Current state**
    > - **Status:** design-only, deferred (CONDITIONAL-GO).
    > - **Current number/verdict:** CONDITIONAL-GO — design is sound (arithmetic runs are exactly how mimalloc structures its per-page free lists) but the mechanism's natural target (the magazine-overflow free path, R24-5's 3.60× free-only gap, 61.5% overflow-attributable) does NOT satisfy its own contiguity precondition: the magazine is a LIFO stack in FREE-order, and `slots[0..FLUSH_N]` (the flushed 8) are arbitrary offsets, NOT offset-contiguous, so a `(first_off, count, stride)` run-descriptor cannot encode them without an O(n) offset-sort that would exceed the per-block savings. The M2 double-free guard (`AllocBitmap::mark_free`) cannot be eliminated for run-blocks (the bitmap is the only per-block free-state record when no node is materialized), collapsing the free-side win to just `Node::write_next` — the same "cheap hot-cache-line store" class R24-4 measured as a +14 Ir/block net REGRESSION to coalesce. The one genuinely new lever (none of the three prior NO-GOs touched it) is the ALLOC side: a contiguous run lets `drain_freelist_batch` skip the per-block `read_next` dependent load (the chain walk that path's own doc calls "irreducible, no way to hoist it" — true for a scattered list, FALSE for an arithmetic run).
    > - **Next trigger:** BOTH required — (a) R23-7's `dealloc_batch` promoted from P2/no-downstream-consumer (the ONLY shape with guaranteed contiguity, by `carve_batch` construction); the magazine-overflow path is explicitly OUTSIDE the conditional. AND (b) a Stage-1 Ir measurement on THAT consumer confirming the alloc-side `read_next` chain is the dominant remaining cost (the §4 judge is the instrument; run it ONLY after (a) fires).
    > - **Evidence:** `R25_8_RUN_ENCODED_FREE_BATCH_DESIGN.md` (full design doc, §3.1 the contiguity finding, §3.3 the double-free-boundary finding, §5 the verdict + triggers, §4 the isolated-victim-judge spec). Triggered by R25-3's NO-GO (`R25_3_FLUSH_N_SWEEP_GATE.md`) — the third NO-GO in this exact free-path/magazine-overflow region (after R24-3, R24-4).

    Design-only; triggered by R25-3's NO-GO (the third consecutive NO-GO in
    the magazine-overflow free-path region, after R24-3's `flush_magazine_class`
    merge and R24-4's bulk-mask primitives). Explores a run-descriptor
    alternative to the intrusive per-block free list: record
    `(segment, first_offset, count, stride)` for a homogeneous contiguous
    batch, allocate FROM it arithmetically (no free-list walk while the run is
    intact), materialize ordinary free-list nodes only on split/escape.
    CONDITIONAL-GO: the design is sound, but the magazine-overflow free path
    (the region that motivated the study) fails the run-descriptor's own
    contiguity precondition (the magazine is LIFO in FREE-order, not
    offset-order), and the M2 double-free guard forces `mark_free` to stay
    per-block — so the free-side win collapses to a single hot store R24-4
    already proved net-negative to coalesce. The one new lever is the alloc
    side (`read_next` chain elimination), reachable only via a contiguous-batch
    consumer that does not exist today (R23-7's `dealloc_batch`, P2).
    Implement ONLY if BOTH triggers fire: (a) a real contiguous-batch consumer
    emerges, AND (b) a Stage-1 Ir measurement on it confirms the `read_next`
    chain is the dominant remaining cost. No `src/` change; design doc only.
15. **R27-11 — reservation-only overflow tier for the small-segment pool
    (evaluated, NOT opened).**

    > **Current state**
    > - **Status:** evaluated (task #429, R27-11), NOT opened — one of two
    >   required triggers is unmeasured.
    > - **Current number/verdict:** Trigger 1 (the committed retention cost of
    >   effective cap 8 is unacceptable for a default) **FIRES** — R27-3
    >   measured ~+8 MiB/heap post-teardown, linearly scaling to ~+255 MiB at
    >   32 heaps, not decaying during idle, and R27-5's design task
    >   independently reached the same conclusion (Option 1: keep the 4/16 MiB
    >   default, do not promote to 8/32). Trigger 2 (an isolated cost
    >   breakdown showing OS-reserve + segment-table-setup + metadata-reinit
    >   is a MATERIAL fraction of the segment lifecycle cost AFTER
    >   recommit/page-fault cost is accounted for) is **UNMEASURED** — R27-4
    >   measured the aggregate latency win from eliminating decommit→reserve
    >   churn (22%, 9→0 decommits) but no report decomposes that churn into
    >   its reserve/setup/metadata-reinit share vs its recommit/page-fault
    >   share. Per this task's own rule ("do NOT start unless BOTH triggers
    >   fire" / "if trigger 2 fails, this design must not be opened"), an
    >   unmeasured trigger 2 cannot be treated as fired — the design is NOT
    >   opened this round.
    > - **Next trigger:** a Stage-1 isolated cost-breakdown probe (mirroring
    >   R17-10 §5.1's "measure before design" discipline) that decomposes one
    >   decommit→reserve segment-lifecycle cycle into (a) OS reserve +
    >   segment-table setup + metadata reinitialization vs (b) recommit +
    >   page-fault cost, on the exact `release_or_pool_empty_segment` /
    >   `reserve_small_segment` cold-path pair R27-5 §3.1 already identified
    >   as the correct measurement site. If (a) is a material fraction of the
    >   cycle, trigger 2 fires and the design may be opened; if (a) is
    >   dominated by (b), the second-tier reservation cannot help (it only
    >   ever saves (a)) and this item should move to `[L]` instead.
    > - **Evidence:** `docs/reviews/2026-07-28-r26-readonly-review.md`
    >   §"A reservation-only overflow tier is a conditional alternative"
    >   (source of the idea); `docs/perf/R27_3_POOL_RETENTION_GATE.md` +
    >   `docs/perf/R27_4_REAL_DEFAULT_AB_GATE.md` (trigger 1 evidence);
    >   `docs/perf/R27_5_ADAPTIVE_POOL_BUDGET_DESIGN.md` §4.2 (independent
    >   confirmation that the default should stay conservative).

    The idea: a SECOND tier beyond the four committed hot segments — decommit
    the payload pages but retain the VA reservation and segment-table
    identity for a short window, recommit on reuse, release on decay or
    budget pressure. This is explicitly NOT a replacement for the hot
    committed pool (that shape was already correctly rejected elsewhere in
    this project); it would only be worth building if avoiding the
    reserve/release + metadata-reinit cost (as opposed to the unavoidable
    recommit/page-fault cost) is itself a material win. **Evaluated this
    round and NOT opened**: trigger 1 (retention cost too high for a default)
    fires — R27-3/R27-4 measured it directly, and R27-5's independent design
    task reached the same "keep the conservative default" conclusion by a
    different route. Trigger 2 (the reserve/setup/metadata-reinit cost is a
    material fraction of the segment-lifecycle cost, isolated from
    recommit/page-fault) has never been measured by any report in this
    project's history — R27-4 measured the AGGREGATE win from eliminating
    decommit→reserve churn, not its internal cost breakdown. Since both
    triggers are required and one is unmeasured (not "failed," genuinely
    unknown), opening the design now would violate the task's own explicit
    conditional-entry rule. No design doc written; no `src/` change. The next
    trigger is a bounded, cheap Stage-1 measurement, not a full design.

### [L] Low-priority — "honest reject" with a documented revisit trigger

7. **R14-5 §4 — dedicated timing gate for O(40) vs O(8) Large-cache scan on a
   narrow working-set-after-burst shape.**

   > **Current state**
   > - **Status:** deferred, low-priority.
   > - **Current number/verdict:** deferred — no number attached yet to the O(40) vs O(8) "cheap" claim for N=1/2/4.
   > - **Next trigger:** a future review wants a number for the narrow working-set-after-burst shape (R13-8 already measured the 24-distinct-size turnover shape).
   > - **Evidence:** `R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` (lines 240–248).

   Deferred "if a future review wants a
   number attached to the 'cheap' claim" specifically for N=1/2/4 (R13-8 already
   measured the 24-distinct-size turnover shape). Evidence:
   `R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` (lines 240–248).
8. **R14-6 §1.1 — compounding reserved-capacity growth factor (beyond 4×).**

    > **Current state**
    > - **Status:** deferred, low-priority.
    > - **Current number/verdict:** deferred — the 4× reserved-capacity growth factor's real numbers are still enough.
    > - **Next trigger:** 4×'s real numbers ever stop being enough (would need new per-segment chain-identity state or a threaded hint through the shared `alloc_large_slow` path).
    > - **Evidence:** `R14_6_ADAPTIVE_RESERVED_CAPACITY_GATE.md` (lines 89–95).

    Deferred "if 4×'s real numbers ever stop being enough"; would need new
    per-segment chain-identity state or a threaded hint through the shared
    `alloc_large_slow` path. Evidence:
    `R14_6_ADAPTIVE_RESERVED_CAPACITY_GATE.md` (lines 89–95).
9. **R15-1 §7 — nonempty-summary-word optimisation for `drain_dirty_segments`.**

    > **Current state**
    > - **Status:** honest reject — NOT recommended now.
    > - **Current number/verdict:** the ceiling is below this task's own noise floor; not worth it.
    > - **Next trigger:** revisit ONLY if `MAX_SEGMENTS` is raised again by a large factor (toward item 4's expandable table) OR a much-higher producer-class fan-in than N=8 becomes a real target.
    > - **Evidence:** `R15_1_MAX_SEGMENTS_DRAIN_SCAN_COST.md` §7 (lines 519–555).

    Explicitly NOT recommended now (ceiling below this task's own noise floor).
    Revisit ONLY if `MAX_SEGMENTS` is raised again by a large factor (toward the
    R14-7 expandable table) OR a much-higher producer-class fan-in than N=8
    becomes a real target. Evidence: `R15_1_MAX_SEGMENTS_DRAIN_SCAN_COST.md` §7
    (lines 519–555).
10. ~~**R9-9 — warm-batch-on-`SeferAlloc`-heap arm.**~~ **DONE (task R10-7,
    2026-07-21, commit `9611a56`).**

    > **Current state**
    > - **Status:** DONE — resolved by R10-7; deliberately left struck-through in place (not moved to "Recently resolved") so the original ask stays visible next to its closure.
    > - **Current number/verdict:** resolved — the fourth warm-batch arm R9-9 asked for (`batch_core_warm`/`scalar_core_warm` + `batch_tcache`) was built and measured against the real warm scalar path.
    > - **Next trigger:** none (closed).
    > - **Evidence:** `R10_7_BATCH_WARM_ARM.md` (closure); `R9_9_BATCH_BENCH_FOLLOWUP.md` (original ask).

    Moved to "Recently resolved" below —
    left here struck through rather than deleted so the original ask stays
    visible next to its closure. R10-7 built exactly the fourth arm this
    item asked for (`batch_core_warm`/`scalar_core_warm`, Part 1) plus the
    realistic tcache-aware design (`batch_tcache`, Part 2) and measured both
    against the real warm `SeferAlloc` scalar path. Evidence:
    `R9_9_BATCH_BENCH_FOLLOWUP.md` (lines 334–343, the original ask);
    `R10_7_BATCH_WARM_ARM.md` (the closure). **Caught late** (2026-07-27,
    task #376/R23-7) — this item sat marked "open" in this index for two
    full rounds (R11 through R23) after R10-7 had already resolved it in
    the very next round after R9-9, because R10-7's own commit did not
    touch `OPEN_ITEMS.md` — the same "report lands, index not updated in
    the same commit" failure mode this file's own convention (§ "When you
    close an item") exists to prevent.
11. **R11-3 — joint threshold×pad-target sweep.**

   > **Current state**
   > - **Status:** low-priority, conditional.
   > - **Current number/verdict:** only relevant if `medium-classes-wide` promotion is re-opened.
   > - **Next trigger:** `medium-classes-wide` promotion re-opened.
   > - **Evidence:** `R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md` (lines 483–485).

   The R11-3 probe fixed the
    pad-target at 2 MiB; "a joint threshold×pad-target sweep is future work."
    Only relevant if `medium-classes-wide` promotion is re-opened. Evidence:
    `R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md` (lines 483–485).
12. **R22-6 — sub-16 KiB geometric-ladder OPT-H probe (optional, ~1 hour).**

    > **Current state**
    > - **Status:** low-priority, optional curiosity probe (~1 hour if ever revisited).
    > - **Current number/verdict:** optional, NOT a next step a round should plan around — the sub-16 KiB tail is already cheapest (OPT-G in-place Large-grow is ~40× faster than mimalloc), so marginal payoff is small even at a favorable hit rate.
    > - **Next trigger:** none named (explicitly low-value, low-cost); a Vec-push-shaped 16 B→16 KiB harness would be the probe if ever run.
    > - **Evidence:** `R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §5.3 + this file's item-1 closure entry (the LCM argument distinguishing the two ladders).

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
13. **R25-3 — `FLUSH_N` sweep (4/8/12/16) at fixed `TCACHE_CAP`=16.**

    > **Current state**
    > - **Status:** NO-GO, fully explored — all 4 swept values measured against all 5 required gates; none beats the current baseline (`FLUSH_N=8`).
    > - **Current number/verdict:** `FLUSH_N=16` shows the only gate-1 (bulk-free Ir) win (−1.5% at N=1024) but triggers the kill condition on gate 3 (oscillating live-set): 2.42× Ir regression, 20× refill-event-count regression (1→20 refills per 20 rounds), independently confirmed via both an Ir judge and a native tcache-hit-rate counter. `FLUSH_N=4`/`FLUSH_N=12` show no gate-1 win at all (+14.4%/+0.7%) with gate 2/3 also flat-or-worse.
    > - **Next trigger:** none named as promising — a genuinely different mechanism (not a half-flush-RATIO tuning) would be needed; R25-8's conditional run-encoded free-batch design study is the only currently-planned follow-up touching this region, and it is a different mechanism, not a FLUSH_N retune. If `FLUSH_N=16` (or any `FLUSH_N == TCACHE_CAP`) is ever revisited for any reason, first fix the independent `virgin_mask >>= FLUSH_N` compile-time overflow this task found at that exact boundary (release-profile-only; `cargo check` in dev profile does NOT catch it) — see the report §6.
    > - **Evidence:** `R25_3_FLUSH_N_SWEEP_GATE.md` (full report); `R25_3_FLUSH_N_SWEEP_GATE_summary.csv`.

    Task #397, 2026-07-28. Swept the magazine-overflow half-flush constant
    `FLUSH_N` across `{4, 8 (baseline), 12, 16}` with `TCACHE_CAP` held fixed,
    gated on all 5 of: in-context Ir bulk-free sweep (N=17/32/64/256/1024),
    free-then-immediate-realloc burst Ir, oscillating live-set (8..24)
    boundary-stress Ir, ordinary interleaved-churn regression check, and a
    non-Ir refill-count/tcache-hit-rate cross-check (a new example,
    `examples/r25_3_flush_n_oscillating_probe.rs`, reusing the existing
    `alloc-stats` `tcache_hits` counter). The oscillating gate's refill-thrash
    finding at `FLUSH_N=16` (every overflow event during a shrink phase
    empties the magazine completely, so the immediately-following growth
    phase's first alloc is GUARANTEED to miss and pay the cold
    `refill_magazine_slow` path) is the third NO-GO in this exact free-path
    magazine-overflow code region this round cluster, after R24-3
    (`flush_magazine_class` bitmap-clear merge) and R24-4 (bulk-mask
    `clear_many`/`set_many` primitives) — all three confirm this region
    resists optimization by the mechanisms tried so far. `git diff HEAD --
    src/` is empty; the new bench arms (`benches/perf_gate_iai.rs`) and probe
    example are kept as reusable measurement infrastructure (R24-2
    precedent), since they measure whatever `FLUSH_N` the tree is currently
    built with rather than hardcoding a value.

---

## Recently resolved (closure trail — do not re-list as open)

- **R9-9 §5 — warm-batch-on-`SeferAlloc`-heap arm (this index's own item 10,
  above).** **DONE (task R10-7, 2026-07-21, commit `9611a56`).** R10-7 built
  the fourth arm R9-9 flagged as missing (`batch_core_warm`/
  `scalar_core_warm`) AND the realistic tcache-aware design (`batch_tcache`,
  draining the warm magazine + batch-refilling the remainder — the design a
  real `SeferAlloc`-based public API would actually ship) and measured both
  against the real warm `SeferAlloc` scalar path. Result: R9-9's
  "no-daylight-even-warmed" inference (§3.2 of that report) is **empirically
  refuted** — warm-batch beats warm-scalar by 1.3×-3.3× (the `AllocCore`
  ceiling arm) and the realistic `batch_tcache` design beats it by
  1.1×-1.6× at every measured (size, N) including N=8. Evidence:
  `R10_7_BATCH_WARM_ARM.md` (full report, §1-§3). **Caught late**
  (2026-07-27, task #376/R23-7): this closure sat unrecorded in this index
  for 12 rounds because R10-7's own commit did not update `OPEN_ITEMS.md`.

- **R22-readonly-review §4.6 — batch API: is there a real downstream
  consumer, and does the measured win translate to a real workload?**
  **DONE — decision recorded, not measured further (task #376/R23-7,
  2026-07-27).** Investigated whether a cheaper/more-realistic benchmark
  than what already exists (R8-7/R9-9/R10-7's batch-size sweep + real
  `SeferAlloc`-scalar comparison arms) could be built; concluded no —
  R10-7's `batch_tcache` arm already IS the realistic-consumer-shaped
  measurement the review asked for (goes through the warm magazine,
  compared against the real scalar entry point, swept across realistic
  batch sizes). Confirmed by grep: `alloc_batch`/`dealloc_batch`
  (`src/global/sefer_alloc.rs`) have exactly one call chain in `src/`
  (`SeferAlloc` → `HeapCore`, both under the `batch-api` feature, which is
  `["experimental", "alloc-core"]` and NOT part of `production`) — no
  in-tree production caller exists, confirming the review's premise
  exactly. Wrote a decision record (not a new benchmark) with an explicit
  falsifiability clause (three concrete triggers: a real internal consumer
  emerges; a downstream project adopts/requests batch-shaped allocation;
  `dealloc_batch` gets batch-optimized closing the R10-7 §2.4 gap). Evidence:
  `docs/perf/R23_7_BATCH_API_CONSUMER_STATUS.md` (full report).

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
  **2026-07-28 note (R25-9, task #403):** the R24 readonly review
  (`docs/reviews/2026-07-28-r24-readonly-review.md`, "Conditional — NUMA
  directory") independently recommended "a node-indexed directory" as a
  still-open ~140× high-segment-count `numa-aware` cliff, citing R10-6's
  own pre-fix measurement. Re-verified against current source before acting
  on it: this is **already resolved by R11-6** (14 rounds earlier) — the
  cliff the review describes (O(S) linear scan under `numa-aware`) is
  exactly what `class_nonempty_by_node`'s node-indexed bitmap replaced. No
  work item opened; the review's recommendation was based on the
  pre-R11-6 report without checking whether it had since landed. Task
  #403 closed with this note as its resolution — no design work needed,
  no new trigger to track.
