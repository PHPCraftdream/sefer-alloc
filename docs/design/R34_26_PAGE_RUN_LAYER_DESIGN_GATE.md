# R34-26 (task #545) — Page-run layer with in-place adjacent-run grow: design GATE (GO/NO-GO on whether to build a prototype)

**Task:** a design-gate feasibility study of whether the project should build a
prototype "page-run layer" — a new `SegmentKind` variant for a multi-`SEGMENT`
arena (8–16 MiB) serving the 256 KiB–2 MiB size range — designed from the start
with **in-place adjacent-run grow** as a mandatory P0 property, to overcome the
realloc regression that killed `medium-classes` promotion. This is the item the
R32–R33 global bench review flagged as "the most likely remaining architectural
multiplier" (P1 item 3, `docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md`
§3, lines 265–277).

**Outcome: DESIGN-ONLY.** No `src/`, `Cargo.toml`, `tests/`, or `benches/` file
is modified. The deliverable is this doc. The verdict (§9) is **NEED-MORE-DATA,
lean NO-GO**: the mandatory precondition (a real consumer in the 256 KiB–2 MiB
range) is not met in this project, so building a prototype now is speculative
architecture. §10 names the evidence that would reopen the question.

**Date:** 2026-08-05. **Base revision read:** `main` @
`7758f7a5e3097adbbf9b5dbd6f0e07b5f3a91c8d`. **Scope:** pure reasoning from
already-measured numbers (R10-2, R10-4, R11-7, R12-13, R18-2, R20-3, R21-2,
R22-6, R22-16, R22-18, R29-5) and a line-by-line read of every cited gate report
and design doc, plus a repo-wide search for a real consumer workload. **No
measurement is performed** — the trigger that would justify a prototype (a real
victim workload) is not met (§7).

---

## 0. TL;DR

| question | answer |
|---|---|
| Was the `medium-classes` failure architectural (carve/grow), not just "missing size classes"? | **Yes — confirmed by two independent lines of evidence.** R10-2 §4.2 ("ANY allocator that packs densely must move on a cross-class realloc") and R22-6's closed-form LCM proof (in-place grow is geometrically impossible within a 4 MiB segment for the medium ladder: the LCM chain = 15 MiB ≫ 4 MiB). |
| Does the page-run layer address the architectural root cause? | **Yes, in principle.** A multi-`SEGMENT` arena (8–16 MiB) gives enough room for both density AND in-place adjacent-run grow if designed with a buddy/run bitmap from the start — the exact combination R10-2 §5 item 1 envisioned but the 4 MiB segment made geometrically impossible. |
| **Mandatory precondition: does this project have a real consumer in 256 KiB–2 MiB?** | **No.** Exhaustive search (§7): every workload in this repo touching that range is a synthetic adversarial harness purpose-built to interrogate the realloc axis, not a real-world consumer pattern. The larson/mstress workloads (`crates/malloc-bench-rs`) generate sizes "mostly 16..512 B, rarely up to ~8 KiB" — they never reach 256 KiB. |
| Should a prototype be built now? | **No — NEED-MORE-DATA, lean NO-GO.** Without a demonstrated consumer, the page-run layer is speculative architecture. The idea is sound and remains a reusable CONDITIONAL-GO starting point (R11-7), but the project's own consistent standard ("gate heavyweight subsystems on measured pain, not hypothetical pain" — R9-4/R10-4/R11-7/R12-13/R22-18) bars investing implementation effort now. |
| What evidence would reopen this? | §10: (1) a real profiling trace showing material allocation/realloc volume in 256 KiB–2 MiB, OR (2) a `MAX_SEGMENTS`-bound workload, OR (3) the carve/grow model itself changes. |

---

## 1. The `medium-classes` failure mode — precisely documented (R10-2)

### 1.1 What was measured

R10-2 (`docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md`, task #228, 2026-07-21)
ran a process-level A/B/B/A wall-clock judge: 240 independent fresh-process
launches (20 A/B/B/A blocks × 3 phases × 4 launches), comparing `production`
(baseline, Large path) vs `production,medium-classes` (treatment, small/medium
path) on a 256 KiB–1 MiB working set of 16 simultaneously-live objects (2×
`LARGE_CACHE_SLOTS = 8`, so the baseline cannot hide behind warm cache):

| Phase | Baseline (A) | medium-classes (B) | Ratio | Statistics |
|---|---:|---:|---:|---|
| **Alloc** | 9.6 µs/alloc | 310 ns/alloc | **~31× faster** | t=55.758, sign 20/20 |
| **Free** | 43.5 µs/free | 207 ns/free | **~211× faster** | t=88.289, sign 20/20 |
| **Realloc** | 39 ns/realloc | 82.3 µs/realloc | **~2,111× slower** | t=−53.607, sign 20/20 |

R18-2's re-run (after an unrelated leak fix) confirmed the realloc regression at
~1,180× (plain) / ~380× (with `large-cache-extended` 8→40 slots) — still RED.

### 1.2 The exact mechanism of the realloc failure

R10-2 §4.2 and §3.3 trace the mechanism precisely:

The **baseline** (Large path) gives every medium-range object a **dedicated
4 MiB committed span** (6% utilization at 256 KiB). Growing
256 → 384 → 512 → 768 KiB all fits within the 4 MiB span → **in-place header
update** → ~39 ns. The baseline "wins" realloc by **wasting memory**: 16 objects
at their final 768 KiB size reserve 16 × 4 MiB = 64 MiB of committed spans
holding 12 MiB of actual data (19% utilization).

`medium-classes` **packs densely**: 16 objects in ~2–3 shared 4 MiB segments
(~100% utilization). But a block carved at offset `off` with `block_size(old_class)`
**cannot grow past its carved slot** — the adjacent bytes are already occupied by
another live block. So every cross-class realloc-grow does a **full move-leg**:
magazine-alloc the new class's block + `copy_nonoverlapping` of the preserved
prefix + magazine-dealloc the old block. The copy dominates: 16 objects ×
(256 + 384 + 512) KiB per round = 18 MiB of `memcpy` per round, × 20 rounds =
360 MiB total — at the host's memory bandwidth this accounts for the bulk of the
79 ms.

R10-2 §4.2 states the architectural truth plainly:

> ANY allocator that packs densely (as `medium-classes` does) must move on a
> cross-class realloc, because the block cannot grow past its carved slot. The
> move-leg copies the preserved prefix — that memcpy is inherent to dense
> packing, not a bug.

### 1.3 R22-6's closed-form proof: in-place grow is geometrically impossible in a 4 MiB segment

R20-3 (`docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md`, task #348) designed
OPT-H — a tail-of-segment cross-class in-place grow mechanism. It was
CONDITIONAL-GO pending a hit-rate measurement. R21-2 measured **0% hit rate**
on both the adversarial harness and the single-hot-buffer harness. R22-6 then
closed the question with a **closed-form LCM proof**: OPT-H's two preconditions
(tail-adjacency + new-class alignment) jointly force the carve offset to be a
multiple of `lcm(block_size(old_class), block_size(new_class))`. For the six
medium classes:

| transition | lcm | legal offsets in one 4 MiB segment |
|---|---|---:|
| 256K→320K | 1.25 MiB | 2 |
| 320K→384K | 1.875 MiB | 1 |
| 384K→512K | 1.5 MiB | 2 |
| 512K→768K | 1.5 MiB | 2 |
| 768K→1M | 3.0 MiB | 1 |

Chaining across all six classes needs `lcm(4,5,6,8,12,16) = 240` units =
**15 MiB**, far exceeding the 4 MiB segment. **No single offset supports a full
ladder walk**, and even 3 consecutive stages fail capacity at the only surviving
offset. This is a **mathematical bound**, not an empirical one — no harness
redesign can move it.

**This is the definitive confirmation that the reviewer's thesis is correct:**
the problem was not the absence of size classes (those exist and gave real wins
on alloc/free), but the **carve/grow architecture** — specifically, the
combination of (a) dense packing in a fixed-size segment and (b) the absence of
any mechanism to grow a block into adjacent free space. In a 4 MiB segment, this
is geometrically impossible for the medium ladder.

---

## 2. The page-run layer alternative (R10-4 / R11-7)

### 2.1 What R10-4 proposed

R10-4 (`docs/perf/R10_4_RUN_ORIGIN_ORACLE_DESIGN.md`, task #237) was primarily a
run-origin oracle design for `class_align`-based carve alignment, but its §0/§9
sketched the page-run layer as the **strictly superior alternative**: a 16 MiB
arena would deliver density **11/9/8** for the three wide classes (1.25/1.5/1.75
MiB), vs the alignment change's 3/2/2 — with "ZERO guard breakage, ZERO new
metadata, and ZERO new correctness surface."

R10-4 §0's exact words:

> the page-run layer (R8-9 §5 K7, R9-4 recommendation #2) is a strictly superior
> solution: a 16 MiB medium arena would deliver density 11 / 9 / 8 for the three
> wide classes (vs the alignment change's 3 / 2 / 2) with ZERO guard breakage,
> ZERO new metadata, and ZERO new correctness surface — `off % block_size == 0`
> stays load-bearing because `block_size` is still the alignment in a page-run
> arena.

### 2.2 R11-7's full design (CONDITIONAL GO, then DEFERRED by R12-13)

R11-7 (`docs/perf/R11_7_PAGE_RUN_LAYER_DESIGN.md`, task #250) is the complete
design for the page-run layer. Key properties:

- **Arena sizing:** recommends a single fixed 8 MiB arena (2×`SEGMENT`),
  delivering density **5/4/3/3** for 1.25/1.5/1.75/2.0 MiB classes (vs today's
  2/1/1/1). A 16 MiB arena (4×`SEGMENT`) delivers 11/9/8/7 but was judged
  over-commit for a workload that only populates 1–2 blocks per arena.
- **Design surface:** §1's exhaustive inventory found **10 new call sites** that
  need page-run-aware logic (Category N), 11 that are safe no-ops, and 4 that
  must explicitly reject. The honest assessment: "closer to a second, smaller
  segment-table subsystem living alongside the existing one than to 'extend
  `SegmentKind` with a fourth case and reuse everything downstream.'"
- **Carve alignment invariant:** `off % block_size == 0` is **UNCHANGED** —
  page-run does NOT change carve alignment, only arena size. This is critical
  for the reclaim guard chain (G1/G2 sites) which rely on this invariant.
- **Address resolution:** §3.2 shows a `>SEGMENT`-sized, `SEGMENT`-aligned arena
  can still be found via a two-step resolution (`segment_base_of_ptr` finds the
  nearest `SEGMENT`-aligned candidate; a header-presence/backpointer check finds
  the TRUE page-run base).

R12-13 (`docs/perf/R12_13_PAGE_RUN_LAYER_DEFERRED.md`, task #264) then DEFERRED
the page-run layer with **NO-GO on implementing now** — no demonstrated
production victim. R12-3/R12-4 (opt-in `exact-span-large` / `large-reserved-
capacity`) closed the RSS/committed-bytes pain (problem (a)) under their opt-in
features, but the `MAX_SEGMENTS`-slot / OS-reservation-syscall pressure (problem
(b)) has no demonstrated victim anywhere in this codebase.

### 2.3 What R11-7 did NOT design: in-place adjacent-run grow

**This is the gap R34-26 is asked to assess.** R11-7 was designed for **DENSITY**
(multi-block packing per arena). It did NOT design **in-place grow** or
**coalescing** as a P0 property. Its carve model is the same bump-cursor +
`align_up(bump, block_size)` as today's small-segment carve, just in a bigger
arena — so a block carved at offset `off` still cannot grow past its carved slot
unless it happens to be the bump tail (the OPT-H case R22-6 proved impossible for
the medium ladder).

The reviewer's thesis — and R34-26's specific ask — is that a page-run layer
**designed from the start with in-place adjacent-run grow** (a buddy/run bitmap
or extent tree that tracks adjacent free runs and can merge/split them) would
solve the realloc problem that killed `medium-classes`, because the arena is
large enough (8–16 MiB) for the LCM arithmetic to work (§5).

---

## 3. The mandatory precondition — does a real consumer exist?

### 3.1 The project's own standard

This project has consistently gated heavyweight new subsystems on **measured
pain, not hypothetical pain**:
- R9-4: dropped 1.5/1.75 MiB tuning "not yet needed."
- R10-4 and R11-7: both reached only CONDITIONAL, not unconditional, GO absent
  real measurement.
- R12-13: deferred page-run for "no demonstrated production victim."
- R22-18 §5's falsifiability clause: the "should `medium-classes` ship" question
  is CLOSED until NEW evidence — defined as "a real downstream consumer whose
  actual measured realloc rate in the 256 KiB–1 MiB range is at or below the
  break-even threshold."

This standard is not a bureaucratic gate — it is the discipline that prevented
this project from investing multiple rounds into subsystems with no real user.
The page-run layer is the single largest design surface this project has ever
scoped (R11-7 §0: "closer to a second subsystem"), and investing that effort
without a real consumer would violate the standard every prior round upheld.

### 3.2 The search

Exhaustive search for a real consumer in the 256 KiB–2 MiB range:

**a) `crates/malloc-bench-rs` (larson/mstress workloads).** The shared workload
crate's `pick_size` function (`crates/malloc-bench-rs/src/lib.rs:140-147`) generates
"mostly 16..512 B, rarely up to ~8 KiB" allocation sizes:

```text
fn pick_size(rng: &mut XorShift64) -> usize {
    let r = rng.next_u64();
    if r & 0x7 == 0 {
        512 + (r >> 8) as usize % (8 * 1024 - 512)  // ~12.5%: 512 B..8 KiB
    } else {
        16 + (r >> 8) as usize % (512 - 16)          // ~87.5%: 16..512 B
    }
}
```

These workloads **never reach 256 KiB**. They are small-skewed server-churn /
batch-stress patterns, the standard allocator benchmark shape. This is the
project's ONLY multi-threaded macro-benchmark with a realistic workload model
(`examples/malloc_macro.rs`).

**b) `examples/paired_ab_medium_workload.rs`.** This is the R10-2 harness —
purpose-built to interrogate the `medium-classes` realloc axis. It is explicitly
described as "intentionally realloc-intensive (960 medium-range realloc-grow
operations) to expose the signal cleanly" (R10-2 §8). It is an **adversarial
probe**, not a real-world consumer pattern.

**c) `examples/r13_8_medium_working_set_judge.rs`.** Self-described as a
"THROWAWAY measurement harness — NOT a shipping artifact." Its purpose is to
test whether `MAX_SEGMENTS` is a real ceiling for 256–2048 simultaneously-live
objects in the 260 KiB–2 MiB range — a diagnostic probe, not a consumer.

**d) All other 256 KiB–2 MiB workloads** (`r11_3_promotion_probe`,
`r12_3_exact_span_measure`, `r12_4_reserved_capacity_measure`,
`r13_6`/`r13_7` large-cache measures, `r14_4_pad_target_probe`,
`r21_2_opt_h_stage1_probe`, `r29_5_promotion_frequency_gate`): every one is a
diagnostic/measurement probe purpose-built to interrogate a specific
medium-classes mechanism, not a real application's allocation pattern.

**e) R29-5's realistic Vec-growth workload** (`docs/perf/R29_5_PROMOTION_FREQUENCY_GATE.md`,
task #436). This is the closest thing to a real-world workload this project has
built: 4,000 small + 40 large objects + 20,000 background allocs, modeled on a
realistic Vec-growth pattern. Its finding: promotions fire on only **0.054%** of
total allocation activity (33/60,722) and **0.82%** of growth objects (33/4,040)
ever promote even once. This is the most direct evidence that even under a
deliberately growth-heavy workload, the medium-range promotion path is **rare**
by every denominator tried.

**f) R22-18 §1.7's explicit finding:** "A real consumer benchmark does not yet
exist — no `docs/perf/*.md` or `benches/*` file runs a benchmark modeled on an
actual downstream consumer's allocation pattern (only the project's own synthetic
adversarial harnesses exist)."

### 3.3 Honest conclusion: NO real consumer identified

No workload, benchmark, example, or documented use case in this repository
represents a real consumer allocating or reallocating in the 256 KiB–2 MiB
range with material volume. Every workload touching that range is a synthetic
adversarial harness or a throwaway diagnostic probe. The project's only
realistic workload model (larson/mstress) never reaches 256 KiB. The project's
one realistic Vec-growth workload (R29-5) found promotion to Large is rare
(0.054% of allocations). This finding is not invented for this task — it
confirms R12-13's identical finding from round 12 and R22-18 §1.7's identical
finding from round 22, now re-verified against the current (round 34) corpus.

---

## 4. Why the reviewer's architectural thesis is correct — but not sufficient

### 4.1 The thesis is confirmed

The reviewer's claim that "the problem was in the architecture of their
carve/grow, not in the absence of size classes as such" is **architecturally
correct and confirmed by two independent lines of evidence**:

1. **R10-2 §4.2** (mechanism): dense packing forces a move-leg on cross-class
   realloc — inherent to the carve model, not a tuning bug.
2. **R22-6** (closed-form proof): in-place grow is geometrically impossible
   within a 4 MiB segment for the medium ladder (LCM chain = 15 MiB ≫ 4 MiB).

A page-run layer with in-place adjacent-run grow addresses BOTH root causes:
the bigger arena (8–16 MiB) provides enough room for the LCM arithmetic to work
(§5), and a buddy/run bitmap provides the mechanism to grow into adjacent free
space without moving — the exact combination R10-2 §5 item 1 envisioned but the
4 MiB segment made geometrically impossible.

### 4.2 Why architectural soundness is not sufficient to proceed

Architectural soundness is necessary but not sufficient. This project's standard
(R12-13 §4, R22-18 §3) requires **measured pain** before investing in a
subsystem of this size. The page-run layer is not a small patch — R11-7 §1.3
counts 10 new call sites, R11-7 §0 calls it "closer to a second subsystem." No
prior round has ever invested implementation effort of this magnitude without a
demonstrated victim, and correctly so: the same standard that prevented
speculative investment in the `large-cache-extended` slot count (R18-2: measured,
found wanting), the OPT-H in-place grow (R21-2/R22-6: measured, found
impossible), and the promotion remap (R22-16: designed, found architecturally
blocked) must apply here too.

The reviewer themselves state this condition explicitly (review §3, line 271):
"This is the most likely remaining architectural multiplier... **but only if the
project has a real consumer in the 256 KiB–2 MiB range.**" §3.3's finding is
that it does not.

---

## 5. Design sketch — in-place adjacent-run grow (CONDITIONAL, since precondition unmet)

Since the mandatory precondition (§3) is not met, this section is a **hypothetical
sketch**, not a detailed design — per the task brief: "if precondition (1) is not
met, this point becomes a hypothetical outline, not a detailed design (don't
spend time on deep data-structure work if the workload itself is not justified)."

### 5.1 Why a bigger arena enables in-place grow (the LCM argument revisited)

R22-6 proved in-place grow is impossible in a 4 MiB segment because the LCM chain
for the full medium ladder = 15 MiB. But in a 16 MiB arena, 15 MiB < 16 MiB —
the full ladder chain **fits**, meaning there exist carve positions from which
the entire 256K→1M growth sequence can proceed in place without violating
alignment. This is the load-bearing arithmetic fact that makes the page-run layer
qualitatively different from medium-classes for realloc, not just quantitatively
denser.

An 8 MiB arena does NOT clear the full 15 MiB chain, but it does clear individual
transitions (e.g., 256K→320K needs lcm = 1.25 MiB, with 5 legal offsets in 8 MiB
vs 2 in 4 MiB). Whether 8 MiB is sufficient depends on the workload's actual
growth pattern — another reason a real consumer trace is the mandatory
precondition.

### 5.2 Buddy/run bitmap concept (sketch, not detailed design)

The mechanism that enables in-place grow in a page-run arena is a **run-level
free-space tracker** that knows which runs adjacent to a given block are free
and can be merged into it on grow:

- **Run bitmap:** one bit per `MIN_BLOCK` (16 B) granule, or coarser (per-page or
  per-class-block), marking free/used. On realloc-grow: check if the run
  immediately after the block is free; if yes, mark it used and extend the block
  — no copy, no move. This is exactly how a buddy allocator or a
  first-fit/best-fit extent tree works.
- **Coalescing on free:** when a block is freed, merge it with adjacent free
  runs (buddy merge or extent-tree coalesce), reducing fragmentation. This is
  standard allocator design (dlmalloc/tcmalloc/jemalloc/mimalloc all do this at
  the page-run level).
- **Alignment invariant:** the carve still uses `align_up(bump, block_size)`, so
  `off % block_size == 0` holds — the existing reclaim guard chain (G1/G2) is
  sound without modification, same as R11-7 §2.3 confirmed.

**The key property — how in-place adjacent-run grow works concretely:**

Given a block at offset `off` with `block_size(old_class)`, a realloc-grow to
`new_class` (where `block_size(new_class) > block_size(old_class)`):

1. Check the run bitmap: is the granule range
   `[off + block_size(old_class), off + block_size(new_class))` entirely free?
2. If yes: mark those granules used, return the same `ptr` — no alloc, no copy,
   no dealloc. The block has grown in place into the adjacent free run.
3. If no: fall through to the existing move-leg (alloc + copy + dealloc), same
   as today. In-place grow is a fast-path optimization, not a replacement.

This is structurally analogous to OPT-G (Large grow-in-span,
`alloc_core.rs:1693`) and OPT-H (tail-of-segment cross-class grow, R20-3), but
generalized: instead of checking "is this the bump tail" (OPT-H) or "does the
grown size fit the committed span" (OPT-G), it checks "are the specific adjacent
granules free" — a strictly more general check that subsumes both.

**Why this was impossible in a 4 MiB segment (R22-6's proof revisited):** even
if the adjacent granules were free, the new-class alignment precondition
(`off % block_size(new_class) == 0`) fails for most carve positions in a 4 MiB
segment because the LCM chain exceeds the segment. In a 16 MiB arena, this
precondition is satisfiable for the full medium ladder — the arithmetic changes
qualitatively, not just quantitatively.

### 5.3 What this sketch does NOT cover (deliberately deferred)

Since the precondition is unmet, the following are NOT designed here:
- The exact run-bitmap data structure (buddy vs. extent tree vs. per-page bitmap)
- The coalescing algorithm (buddy merge vs. red-black tree of free extents)
- The metadata cost (per-arena bitmap overhead, same order as R11-7 §4.1's
  ~32 KiB/segment estimate)
- The interaction with the magazine layer (R11-7 §1.2 row 13: whether page-run
  arenas get a magazine tier at all is a stage-2 tuning question)
- The cross-thread free ring encoding (R11-7 §1.2 row 24: offset field width for
  `off < N × SEGMENT`)

These are real design questions, but designing them in detail without a real
consumer would repeat the exact "invest then defer" cycle R11-7/R12-13 already
went through. They should be designed when — and only when — the precondition
is met.

---

## 6. Relationship to prior attempts and existing items

| Prior attempt | What it tried | Verdict | Relevance to this task |
|---|---|---|---|
| **R10-2** (medium-classes gate) | Direct `medium-classes` promotion | NO-GO (realloc 2,111×) | The failure mode this task diagnoses |
| **R10-4** (run-origin oracle) | `class_align` carve alignment | CONDITIONAL GO; page-run layer called "strictly superior" | Named the page-run layer as the real fix |
| **R11-7** (page-run layer design) | Multi-`SEGMENT` arena for density | CONDITIONAL GO; DEFERRED by R12-13 | The design this task extends with in-place grow |
| **R12-13** (page-run deferred) | Re-evaluation after R12-3/R12-4 | NO-GO now (no victim) | Same precondition; this task re-confirms |
| **R18-2** (medium realloc re-run) | Re-run after leak fix | Still RED (~1,180×) | Confirms the realloc axis is structurally closed for dense packing |
| **R20-3** (OPT-H design) | Tail-of-segment in-place grow | CONDITIONAL GO → NO-GO (R21-2/R22-6) | The in-place grow mechanism, proven impossible in 4 MiB |
| **R22-6** (LCM proof) | Closed-form in-place grow bound | NO-GO (geometric, LCM = 15 MiB ≫ 4 MiB) | The mathematical proof this task's §5 leverages |
| **R22-16** (remap design) | OS-level VA remap | NO-GO (base-address stability blocker) | A different lever; this task does not revisit |
| **R22-18** (product fate) | Ship / document / remove | (b) named opt-in profile | §5's falsifiability clause: "closed until new evidence" |
| **R29-5** (promotion frequency) | Realistic Vec-growth workload | NO VICTIM (0.054% promote) | The strongest evidence no real consumer exists |

**OPEN_ITEMS.md item 3** (R11-7 page-run layer) currently reads:
`Status: NO-GO now; kept as a reusable CONDITIONAL-GO starting point.`
`Next trigger: a real workload allocating thousands of simultaneously-live`
`1.25–2.0 MiB objects that is MAX_SEGMENTS-bound or OS-reservation-syscall-bound.`

This task does not change item 3's status — the precondition it names is still
unmet. This task ADDS the in-place-grow angle to the reopening criteria (§10),
updating the trigger to also name "a real profiling trace showing material
realloc volume in 256 KiB–2 MiB" alongside the existing MAX_SEGMENTS/syscall
trigger.

---

## 7. Required gates for promotion (if precondition is ever met)

If a future round meets the precondition and builds a prototype, the following
gates are required before any promotion to `production`:

| # | Gate | What it measures | Pass criterion |
|---|---|---|---|
| G1 | **RSS / committed-bytes** | Per-object RSS amplification vs Large baseline | Parity with `exact-span-large` (~1.00–1.05×), per R12-3 |
| G2 | **Fragmentation** | External fragmentation after sustained alloc/free/realloc churn | No runaway growth; bounded by a stated factor (design-time TBD) |
| G3 | **Alloc/free wall-clock** | Paired A/B/B/A process-level, medium-range objects, vs Large baseline | At least parity with today's Large path; ideally approaches medium-classes' 31×/211× wins (R10-2) |
| G4 | **Realloc wall-clock (the kill gate)** | Paired A/B/B/A, same harness as R10-2 | **MUST WIN, not merely tie, vs Large baseline** (§8) |
| G5 | **Cross-thread free correctness** | Loom + miri + proptest under `alloc-xthread` | Zero data races, zero leaks, zero corruption — same bar as existing small/large paths |
| G6 | **Path-activation oracle** (per CLAUDE.md R30-8/R26-4 rules) | Per-arm evidence that in-place grow actually fired | ≥95% activation for the target workload's grows; 0% is the R30-3/R29-16 failure mode |
| G7 | **Layer-correctness** (per CLAUDE.md entry-point rule) | Must measure at `HeapCore::alloc`/`realloc` (the real `#[global_allocator]` chain), not `AllocCore`-direct | R31-0/R30-3 lesson: a sub-layer judge can ship a wrong verdict |
| G8 | **Cost-benefit same-regime** (per CLAUDE.md regime rule) | Cost (RSS/overhead) and benefit (latency) measured in the SAME workload regime | R31-1/R31-12 lesson: don't combine parity-from-one-regime with savings-from-another |

**G4 is the load-bearing gate.** §8 formalizes the criterion.

---

## 8. The realloc criterion — NO default promotion until realloc WINS, not merely ties

This is the explicit criterion the task brief asks to be fixed in the verdict:

> **NO default promotion to `production` until a paired A/B/B/A wall-clock gate
> demonstrates that the page-run layer's realloc path is not merely at parity
> with the Large baseline but WINS — i.e., shows a statistically significant
> (paired t > crit, sign test lopsided, ≥2 independent repeats) REDUCTION in
> realloc-phase wall-clock versus `production` (Large path) for the target
> workload's actual realloc pattern.**

"Parity" is not sufficient because:
1. The Large baseline's realloc cost is **near-zero by design** (in-place header
   update within a dedicated 4 MiB span) — R10-2 §4.2. A page-run layer that only
   ties this is not an improvement worth a new subsystem.
2. The page-run layer's value proposition is **both** density AND realloc speed.
   If it cannot beat the Large path on realloc (which is the ONLY axis
   `medium-classes` lost on), the density win alone does not justify the
   subsystem — `exact-span-large` already closes the RSS axis (R12-3) at a
   fraction of the cost.
3. R22-18 §0 explicitly states the realloc axis is "structurally closed for the
   dense-packing design `medium-classes` uses" — the page-run layer must
   **structurally refute** this closure with a measured win, not a theoretical
   argument, before promotion is considered.

This criterion is deliberately stronger than R10-2's original >20% threshold
(which was a regression gate, not a win gate). The page-run layer is not being
asked to "not regress" — it is being asked to **demonstrate a real improvement**
on the axis that killed every prior attempt.

---

## 9. Verdict: NEED-MORE-DATA, lean NO-GO

### 9.1 The decision

**The mandatory precondition for a GO-to-prototype verdict — a real consumer in
the 256 KiB–2 MiB range — is not met.** No workload, benchmark, example, or
documented use case in this repository represents a real consumer with material
allocation/realloc volume in that range (§3). The project's one realistic
workload model (larson/mstress) never reaches 256 KiB; the project's one
realistic Vec-growth workload (R29-5) found promotion is rare (0.054%).

**Decision: NEED-MORE-DATA, lean NO-GO on building a prototype now.** The
architectural thesis is sound (§4 — the reviewer is correct that the problem was
in the carve/grow architecture), the page-run layer is the theoretically correct
direction (§2–§5), and in-place adjacent-run grow is the right P0 property (§5).
But without a real consumer, building it would violate this project's consistent
standard of gating heavyweight subsystems on measured pain. The same standard
that correctly deferred R11-7/R12-13 and closed OPT-H (R22-6) applies here.

### 9.2 Why "lean NO-GO" rather than a flat "NO-GO"

The architectural case is genuinely strong — stronger than any prior deferred
design in this region:
- The root cause is precisely diagnosed and architecturally confirmed (§1).
- The page-run layer with in-place grow addresses both root causes (bigger arena
  + run bitmap), not just the symptom (§5).
- The LCM arithmetic that blocked OPT-H in a 4 MiB segment is satisfiable in a
  16 MiB arena (§5.1) — this is a qualitative change, not a quantitative tweak.

"Lean NO-GO" rather than flat "NO-GO" reflects that the only barrier is the
precondition, not the architecture. The moment a real consumer is identified,
this becomes a GO — the design path is clear (R11-7 + §5's in-place-grow
extension), the gates are enumerated (§7), and the criterion is fixed (§8).

This mirrors R12-13's own framing: "This is not a permanent close. If a future
need materializes... `R11_7_PAGE_RUN_LAYER_DESIGN.md` remains a complete, reusable
CONDITIONAL-GO design for that scenario and should be the starting point."

---

## 10. Evidence that would reopen the question

The following new evidence would change the verdict from NEED-MORE-DATA to
GO-to-prototype:

1. **A real profiling trace** from an actual application (not a synthetic
   harness) showing material allocation AND realloc volume in the 256 KiB–2 MiB
   range — concretely: enough realloc-grow operations crossing medium-class
   boundaries that the 82.3 µs/op move-leg cost (R10-2) is a measurable fraction
   of the application's total allocation time. The R29-5 finding (0.054% promote
   rate under a realistic Vec-growth workload) is the bar this must clear.

2. **A `MAX_SEGMENTS`-bound workload** — thousands of simultaneously-live
   256 KiB–2 MiB objects that exhaust the segment table. This is the trigger
   OPEN_ITEMS.md item 3 already names.

3. **A change to the carve/grow model** that alters the LCM arithmetic R22-6
   derived — e.g. a redesigned medium-class ladder with friendlier size ratios,
   or a fundamentally different carve discipline. (This is also one of R22-18 §5's
   three named reopening triggers for the `medium-classes` question.)

Until one of these materializes, the correct action for a future round is to
**cite this document** (and the 10 prior reports it summarizes), not to
re-measure or re-design — matching R22-18 §5's falsifiability discipline.

---

## 11. Summary of what was read and verified

All cited documents were read in FULL, not excerpted from memory:

- `docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md` — the NO-GO gate (full §1–§8)
- `docs/perf/R10_4_RUN_ORIGIN_ORACLE_DESIGN.md` — the run-origin oracle + page-run
  layer discussion (full §0–§9, 500+ lines)
- `docs/perf/R11_7_PAGE_RUN_LAYER_DESIGN.md` — the full page-run layer design
  (§0–§8, 500+ lines read)
- `docs/perf/R12_13_PAGE_RUN_LAYER_DEFERRED.md` — the deferral verdict (full)
- `docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` — OPT-H design (full §0–§10,
  749 lines)
- `docs/perf/R22_18_MEDIUM_CLASSES_FATE_DECISION.md` — the product fate decision
  (full §0–§6, 500+ lines)
- `docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md` §3 (P1 item 3)
- `docs/perf/OPEN_ITEMS.md` item 3 (page-run layer), item 6 (remap), items 5
  (run-origin oracle)
- `crates/malloc-bench-rs/src/lib.rs` `pick_size` (lines 140–147 — size distribution)
- `examples/_shared/paired_ab_medium_workload.rs` (the R10-2 harness)
- `examples/r13_8_medium_working_set_judge.rs` (the MAX_SEGMENTS probe)
- `examples/malloc_macro.rs` (the larson/mstress driver)

---

## 12. Caveats

1. **This is a design-gate, not a prototype.** No code was written, no
   measurement was performed. The verdict rests entirely on reasoning from
   already-measured numbers and a repo-wide search for a real consumer.

2. **"No real consumer" is an absence-of-evidence finding** from this
   repository's own tests/benches/examples/docs, not a proof that no such
   workload could ever exist for a downstream user of this crate. This matches
   R12-13's own framing (§5). A downstream consumer with a real 256 KiB–2 MiB
   workload is the evidence that would reopen this (§10 item 1).

3. **The in-place-grow design sketch (§5) is intentionally shallow.** Per the
   task brief: "if the precondition is not met, this point becomes a
   hypothetical outline, not a detailed design." The buddy/run bitmap, coalescing
   algorithm, metadata cost, and magazine/ring interactions are NOT designed
   here — they should be designed when (and only when) the precondition is met,
   to avoid repeating the invest-then-defer cycle.

4. **Not retroactive.** This verdict does not change the status of any prior
   design doc or gate report. R11-7 remains a reusable CONDITIONAL-GO starting
   point; R12-13's deferral stands; R22-18's product-fate decision for
   `medium-classes` is unaffected (the realloc axis is still RED today, with no
   code change and no measurement showing otherwise).

5. **The page-run layer with in-place grow is NOT the same as `medium-classes`.**
   `medium-classes` packs densely in a 4 MiB segment (impossible for in-place
   grow). The page-run layer packs in an 8–16 MiB arena (possible for in-place
   grow). They are architecturally distinct: one failed, one is untried. This
   task's verdict is about the untried one.
