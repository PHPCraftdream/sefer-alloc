# R24-5 — cold alloc/free split: isolating the alloc-only and free-only halves of `cold_alloc_free_256x16b`, and reconciling the free half with R24-2's overflow model

**Task #383 (R24-5), Round 24.** The mandatory measurement gate before any
attempt to close the ~2× cold-carve `Ir` gap vs mimalloc
(`docs/perf/R23_2_WARM_N_2N_MIMALLOC_GATE.md`: SeferAlloc 203.86 Ir/op vs
mimalloc 101.81 Ir/op, ratio 2.002×). An independent read-only review
(`docs/reviews/2026-07-27-r23-readonly-review.md` P1, "cold headline ... does
not localise the cause") found `cold_alloc_free_256x16b`'s single number bundles
refill, carve, magazine push, BinTable, AND the magazine-overflow flush all into
one figure, so the ~2× gap could not be attributed to any one mechanism. R23-3
(`carve_batch_only_16b`) already isolated pure bump-carve at 23.05 Ir/block — far
too small to explain the gap — proving carve itself is NOT the bottleneck. This
task splits `cold_alloc_free_256x16b` into its alloc-only and free-only halves,
isolates one virgin refill and one recycled refill, and reconciles the free half
with R24-2's already-isolated overflow model to localize where the gap actually
lives.

**Date:** 2026-07-27. **Base revision measured:** `main` @
`9dc0e22` (working tree carrying only this task's own additive edits at
measurement time). **Platform measured:** WSL2 (Ubuntu, kernel
`6.18.33.2-microsoft-standard-WSL2`) under Windows 10 Pro x86-64, `valgrind
3.22.0`, `iai-callgrind-runner 0.14.2`, WSL rustc `1.98.0-nightly (bd08c9e71
2026-06-25)` — byte-identical toolchain/host to R22-15 through R24-4 (§8:
every reference arm reproduced its prior-report number exactly).

**Measurement only. No production behavior changed:** seven new
`#[library_benchmark]` arms in `benches/perf_gate_iai.rs`. **Zero new
`#[doc(hidden)]` hooks, zero `src/` changes** — every quantity is obtained by
shared-prefix subtraction (R22-17/R23-1/R24-2's technique) or N/2N bootstrap
cancellation (R23-2's technique). Nothing to track for R24-6 (task #384): this
task adds no new hook to the production unsafe-audit surface.

---

## 0. Headline: the ~2× cold gap is OVERWHELMINGLY in the free half — and the free half is 61.5% magazine overflow

| half (16 B cold, N=256, SCOPE A = full alloc/free through `SeferAlloc`) | SeferAlloc | mimalloc | Sefer/mi ratio |
|---|---:|---:|---:|
| **ALLOC-only** marginal `c = (Ir(2N)−Ir(N))/N` | **91.05** Ir/op | **71.81** Ir/op | **1.27×** |
| **FREE-only** `= Ir(alloc_free) − Ir(alloc_only)` | **108.77** Ir/free | **30.24** Ir/free | **3.60×** |
| full round (R23-2, alloc+free) | 203.86 | 101.81 | 2.00× |

**The single most important finding: the ~2× full-round gap is a BLEND of two
very different per-half gaps — a modest 1.27× on the alloc side and a
dominant 3.60× on the free side.** The full-round 2.0× headline masks how
lop-sided the gap really is: SeferAlloc's free path retires 3.60× mimalloc's
free-path instructions on the identical workload, while its alloc path is only
1.27×. R23-2's "2.002×" is the average of these two; it is NOT a uniform gap
across the round.

**Where the free-half cost actually goes** (R24-2 reconciliation, §5, holds
within 2.6%): of the measured 108.77 Ir/free, **61.5% is magazine overflow
(30 events × 571 Ir) and 35.9% is cheap non-overflow pushes (226 × 44.25 Ir)**.
This confirms — directly, at N=256, not by extrapolation alone — that the free
half of `cold_alloc_free_256x16b` is dominated by the SAME overflow mechanic
R24-2 isolated at N≤64 (`docs/perf/R24_2_FREE_BY_MAGAZINE_STATE_GATE.md`). The
overflow is the gap's locus.

**Three-outcome verdict (per the task's own framing): OUTCOME (a) holds.** Most
of the ~2× gap lives in the free half, and that half is overflow-dominated
(matching R24-2's mechanism). (b) is ruled out — the alloc half is only 1.27×,
not where the gap concentrates. (c) is ruled out — SeferAlloc's halves do NOT
match mimalloc's (the free half is 3.60×, decisively not "roughly matching").
**But outcome (a) carries the explicit caveat the task's framing warned about:
the gap's root cause is now understood (free-side magazine overflow), yet the
two approaches already tried in R24-3 (flush_magazine_class bitmap-clear merge)
and R24-4 (SegmentBitmap bulk-mask primitive) both measured NO-GO regressions
when applied to the bitmap-clear sub-cost.** The overflow's larger untried
lever is `flush_class` itself (mark_free + dec_live + decommit-check per block,
~487 Ir/event — the non-isolable remainder R24-2 §5.1 flagged), NOT the
bitmap-clear R24-3/R24-4 targeted.

---

## 1. Investigation performed first (per the task's instruction)

### 1.1 Why the alloc-only arm is a valid shared prefix for the free-only split

`cold_alloc_free_256x16b`'s body is `SeferAlloc::new()` → allocate `COLD_BATCH`
(256) distinct 16 B blocks into `ptrs` → `black_box(&ptrs)` → free them all.
The new `cold_alloc_only_256x16b` is byte-identical EXCEPT the free loop is
removed (pointers deliberately leaked — each `#[library_benchmark]` runs in its
own fresh process under callgrind, so leaking is harmless; this is the exact
rationale `dealloc_prealloc_only_16b`'s doc comment already established for
R22-17's free-side isolation). So:

```
free_cost(N) = Ir(cold_alloc_free_N) − Ir(cold_alloc_only_N)
```

cancels the shared alloc prefix exactly — the SAME shared-prefix technique
R24-2 used (`dealloc_free_only_16b − dealloc_prealloc_only_16b`), scaled from
`CHURN_OPS` (64) to `COLD_BATCH` (256). This is the task's number 2.

### 1.2 Why N/2N is valid for the alloc-only marginal (number 1)

The alloc-only arm's raw Ir = `B + N·c_alloc` where `B` is the one-time
process bootstrap (`SeferAlloc::new()` + array zero-init + primordial-segment
commit). Two op counts give `c_alloc = (Ir(2N) − Ir(N))/N` with `B` cancelled
algebraically — R23-2's technique. A 4N sibling cross-checks the linearity
assumption (§4.1). N/2N is used ONLY for the alloc-only marginal; the free
cost is obtained purely by shared-prefix subtraction (the overflow step is
non-linear in N, so N/2N is invalid for the free side — the same reason R24-2
used shared-prefix, not N/2N, for its sweep).

### 1.3 Call-chain investigation: `carve_batch_only_16b` does NOT answer "one virgin refill"

The task explicitly required investigating `HeapCore::alloc`'s magazine-miss
path (`src/registry/heap_core_alloc.rs`) before assuming `carve_batch_only_16b`
already isolates a refill. Read in full:

- `carve_batch_only_16b` calls `AllocCore::dbg_carve_batch` directly on a bare
  `AllocCore` (`benches/perf_gate_iai.rs:882`) — pure bump-cursor advance, no
  magazine, no `HeapCore`, no `BinTable`, no stamping. This measures the
  **primitive floor**, not a refill.
- A real virgin refill is `HeapCore::refill_magazine_slow`
  (`heap_core_alloc.rs:665`), reached on a magazine miss: it drains the
  deferred-large-free and heap-overflow rings, calls
  `AllocCore::refill_class_bump_checked` to carve `refill_n_for_class(16 B) =
  TCACHE_CAP = 16` blocks straight into the magazine slots, runs the P4
  stamp-dedupe over the 16 carved blocks, then `mark_magazine` (a
  `segment_base_of_ptr` + bitmap RMW) on the 15 retained, then pops one for
  the caller. **This is a strictly larger unit than pure carve** — 16 blocks'
  worth of bump-carve PLUS magazine-loading, stamp-dedupe, 15
  magazine-residency bitmap RMWs, and the two drains.

So `carve_batch_only_16b` (23.05 Ir/block, §4.4 — reproduced R23-3 exactly)
is the carve-primitive floor; "one virgin refill" is a different, larger unit
that this task isolates as a DERIVED figure (§4.3), not via `carve_batch_only`
directly. A refill-standalone hook (calling `refill_class_bump_checked`
outside the magazine-miss context) was NOT added: production never runs that
mechanism standalone, the exact Heisenberg risk R24-2 §5.1 invoked to decline
a `flush_class`-standalone hook.

### 1.4 The magazine state at the START of round 2 (governs the recycled-refill count)

`recycle_alloc_free_256x16b`'s round 1 frees 256 blocks into the magazine.
Because `FLUSH_N = 8`, the frees trigger 30 overflows (the frees are the
SAME 256-free overflow pattern §5 analyses), each flushing 8 and leaving 8;
after free #256 the magazine sits at `count == 16` (FULL). So round 2's allocs
do NOT start at `count == 0`: the first 16 allocs are magazine HITS (cheap
pops of blocks round 1's frees left behind), and only from alloc #17 onward
does the magazine miss and refill — by DRAINING the BinTable freelist round 1's
overflow flushes populated (`refill_class_bump_checked` drains free blocks
first, then bump-carves the remainder). This makes round 2 = 256 pops + 15
freelist-draining refills (not 16): the recycle-refill count and the
derivation in §4.4 both rest on this state.

---

## 2. Two isolation techniques used (no new hooks — both are pre-established)

- **Shared-prefix subtraction** for the free-only cost (number 2):
  `free_cost(N) = Ir(cold_alloc_free_N) − Ir(cold_alloc_only_N)`. Identical to
  R22-17/R23-1/R24-2. Also used to split round 2 (recycle) into alloc-only vs
  free-only (number 4): `Ir(recycle_alloc_only) − Ir(cold_alloc_free)` =
  round-2 alloc-only; `Ir(recycle_alloc_free) − Ir(recycle_alloc_only)` =
  round-2 free-only.
- **N/2N bootstrap cancellation** for the alloc-only marginal (number 1) and
  its mimalloc mirror: `c = (Ir(2N) − Ir(N))/N`. Identical to R23-2. A 4N
  sibling cross-checks linearity (§4.1).

**The refill-event costs (numbers 3 and 4) are DERIVED, not directly isolated**
(§4.3, §4.4): the refill is fused with the magazine pop in every 16th alloc,
and isolating it by bench-arm subtraction alone is underdetermined (one
equation, two unknowns — `c_pop` and `c_refill`). The derivation uses
`c_pop = 22.38 Ir/op` (R23-3's directly-isolated magazine-hit alloc, the same
`SeferAlloc::alloc` hit arm, whose code is identical hot or cold). This
rest-on-a-prior-figure is stated explicitly wherever a refill number is cited;
`carve_batch_only`'s directly-measured 23.05 Ir/block floor is given alongside
every refill figure so the reader can see the carve-primitive floor separately
from the full-path refill.

---

## 3. New bench arms (SEVEN — the minimum necessary; zero new hooks)

| arm | isolates | technique | feature gate |
|---|---|---|---|
| `cold_alloc_only_256x16b` | shared alloc prefix (256) + alloc marginal | shared-prefix + N/2N | `alloc-global` (linux) |
| `cold_alloc_only_256x16b_2n` | alloc marginal (2N) | N/2N | linux |
| `cold_alloc_only_256x16b_4n` | alloc linearity cross-check (4N) | N/2N | linux |
| `mimalloc_cold_alloc_only_256x16b` | mimalloc's alloc prefix + marginal | shared-prefix + N/2N | linux |
| `mimalloc_cold_alloc_only_256x16b_2n` | mimalloc alloc marginal (2N) | N/2N | linux |
| `mimalloc_cold_alloc_only_256x16b_4n` | mimalloc linearity cross-check (4N) | N/2N | linux |
| `recycle_alloc_only_256x16b` | round-2 (recycle) ALLOC-only prefix | shared-prefix | linux |

All arms follow this file's existing conventions: `#[cfg(target_os = "linux")]`,
`black_box` on observables, `// SAFETY:` on every `unsafe` block, doc comments
explaining what each isolates and how, registered in the `perf_gate`
`library_benchmark_group!` list (**50 benches total, up from 43** after R24-2).
The mimalloc alloc-only trio exists because a rigorous three-outcome verdict
requires mimalloc's OWN alloc/free split (alloc-vs-alloc, free-vs-free), not a
full-round-only comparison — the task explicitly asked which comparison is
needed, and §6 states the choice and why. No pre-existing bench fn body was
edited.

---

## 4. Results — real, deterministic `npm run iai` numbers (two independent runs, byte-identical `Ir`)

Raw evidence (both runs full stdout, all 50 benches):

- `docs/perf/_raw_r24_5_run1.log`
- `docs/perf/_raw_r24_5_run2.log`

Both runs: **50 benches, byte-identical `Ir` for every row including all 7 new
arms** (confirmed via an `awk`-extracted `name Ir` diff of the two runs —
`diff` exit 0, zero differences). The 12 pre-existing reference arms
reproduced their prior-report numbers EXACTLY — `small_churn_16b`=8051,
`small_churn_16b_2n`=12467, `cold_alloc_free_256x16b`=50164/102353/202867,
`mimalloc_cold_alloc_free_256x16b`=32325/58389/106653,
`dealloc_prealloc_only_16b`=7003, `dealloc_free_only_16b`=12923, and all six
R24-2 sweep points (`_n1`/`_n8`/`_n9`/`_n16`/`_n17`/`_n32` = 7058/7367/7410/
7711/8282/9451) — confirming the run is on the byte-identical toolchain/host
as R23-2/R24-2 and that the new arms' numbers are directly comparable to
R24-2's per-cheap-push (44.25) and per-overflow (571) figures.

### 4.1 Raw Ir table (new arms + the rows they derive against)

| bench | raw Ir | role |
|---|---:|---|
| `cold_alloc_only_256x16b` (new) | 22,318 | alloc prefix (N=256) |
| `cold_alloc_only_256x16b_2n` (new) | 45,627 | alloc prefix (2N=512) |
| `cold_alloc_only_256x16b_4n` (new) | 88,381 | alloc prefix (4N=1024) |
| `cold_alloc_free_256x16b` (existing) | 50,164 | alloc+free (N=256) |
| `cold_alloc_free_256x16b_2n` (existing) | 102,353 | alloc+free (2N) |
| `cold_alloc_free_256x16b_4n` (existing) | 202,867 | alloc+free (4N) |
| `recycle_alloc_only_256x16b` (new) | 70,310 | round1(alloc+free)+round2(alloc) |
| `recycle_alloc_free_256x16b` (existing) | 98,343 | round1 + round2 (both alloc+free) |
| `carve_batch_only_16b` (existing) | 68,284 | pure bump-carve primitive (N) |
| `carve_batch_only_16b_2n` (existing) | 74,185 | pure bump-carve primitive (2N) |
| `mimalloc_cold_alloc_only_256x16b` (new) | 24,584 | mimalloc alloc prefix (N) |
| `mimalloc_cold_alloc_only_256x16b_2n` (new) | 42,968 | mimalloc alloc prefix (2N) |
| `mimalloc_cold_alloc_only_256x16b_4n` (new) | 75,872 | mimalloc alloc prefix (4N) |
| `mimalloc_cold_alloc_free_256x16b` (existing) | 32,325 | mimalloc alloc+free (N) |
| `small_churn_16b` (existing, context) | 8,051 | interleaved hot ref |

### 4.2 Number 1 — cold ALLOC-only per-op (SeferAlloc)

```
c_alloc(N,2N)  = (45,627 − 22,318) / 256 = 23,309 / 256 = 91.05 Ir/op
c_alloc(2N,4N) = (88,381 − 45,627) / 512 = 42,754 / 512 = 83.50 Ir/op
non-linearity (2N,4N vs N,2N) = −8.3%
```

**Cold ALLOC-only ≈ 91.05 Ir/op** (N,2N; the 4N cross-check gives 83.50, same
"cheaper at larger N" direction R23-2 found for the full round at −3.7%, here
larger because the alloc-only arm lacks the free loop's O(N) work to dilute
the once-per-bench array-zero-init/bootstrap component that does not cancel
perfectly — reported honestly, not assumed away; the verdict in §6 is robust
to either figure). This is the per-op cost of the full magazine-aware cold
alloc path: each op is one magazine pop, with a virgin refill every 16 ops.

### 4.3 Number 3 — one virgin refill (DERIVED; floor measured directly)

Each marginal alloc op = 1 magazine pop + 1/16 of a refill (refill brings
`TCACHE_CAP = 16` for the 16 B class, so a refill fires every 16 allocs on a
cold start). So `c_alloc = c_pop + c_refill/16`:

```
c_pop    = 22.38 Ir/op   (R23-3's directly-isolated magazine-hit alloc; the
                          `alloc` hit arm, `heap_core_alloc.rs:155-254`, is
                          identical code hot or cold, so this generalizes)
c_refill = 16 × (91.05 − 22.38) = 16 × 68.67 = 1098.7 Ir/event ≈ 1099 Ir/event
```

**One virgin refill ≈ 1099 Ir/event (DERIVED).** Compared to the directly-
measured carve-primitive floor:

```
carve_batch_only_16b: (74,185 − 68,284) / 256 = 5,901 / 256 = 23.05 Ir/block
  → carve 16 blocks = 368.8 Ir   (reproduces R23-3's 23.05 exactly)
refill / primitive = 1099 / 368.8 = 2.98×
```

A refill is ~3× the pure bump-carve primitive (it adds magazine-loading, P4
stamp-dedupe, 15 `mark_magazine` bitmap RMWs, and the two opportunistic
drains). **`carve_batch_only_16b` does NOT answer "one virgin refill"** — the
task's anticipated finding (§1.3). The refill event itself is not directly
isolable (fused with the pop; §2), so 1099 is a derived figure resting on
`c_pop = 22.38`; the 23.05 Ir/block floor is the directly-measured companion.

### 4.4 Number 4 — one recycled refill (DERIVED; round-2 split sums to R23-3 exactly)

Round 2's alloc-only, isolated by shared-prefix subtraction against
`cold_alloc_free` (which IS byte-identical to round 1):

```
round2_alloc_total = Ir(recycle_alloc_only) − Ir(cold_alloc_free)
                   = 70,310 − 50,164 = 20,146   → 78.70 Ir/op
round2_free_total  = Ir(recycle_alloc_free) − Ir(recycle_alloc_only)
                   = 98,343 − 70,310 = 28,033   → 109.50 Ir/free
round2_total       = Ir(recycle_alloc_free) − Ir(cold_alloc_free)
                   = 98,343 − 50,164 = 48,179   → 188.20 Ir/op
```

**Cross-check: round2_total = 188.20 Ir/op matches R23-3's independently-
published round-2 figure EXACTLY** (`docs/perf/R23_3_HOT_PATH_ATTRIBUTION_GATE.md`
derived the same 188.20 from `recycle_alloc_free − cold_alloc_free` with no
alloc-only arm). My round-2 split (20,146 alloc + 28,033 free = 48,179) sums
to that figure to the instruction — strong internal-consistency proof that the
shared-prefix subtraction is sound. (round2_free 109.50 vs round1_free 108.77:
+0.7% — confirms the free cost is identical across rounds, exactly as expected
since both rounds free 256 blocks through the same 30-overflow pattern.)

Round 2 = 256 pops + 15 freelist-draining refills (§1.4: the magazine starts
round 2 FULL at count=16 from round 1's frees, so the first 16 allocs are
hits, then refills every 16 from alloc #17: 15 refills). Solving:

```
round2_alloc_total = 256·c_pop + 15·c_refill_recycle
20,146 = 256 × 22.38 + 15·c_refill_recycle = 5,729.3 + 15·c_refill_recycle
c_refill_recycle = (20,146 − 5,729.3) / 15 = 14,416.7 / 15 = 961.1 Ir/event
```

**One recycled refill ≈ 961 Ir/event (DERIVED)** — **12.5% CHEAPER than the
virgin refill (1099)**. Draining the BinTable freelist is cheaper than virgin
bump-carve (it skips the commit-frontier-grow and reuses an already-mapped
offset), consistent with R23-3's finding that "recycle is NOT the
costlier-than-carving mechanism" — now decomposed to the refill level, not
just the full round.

---

## 5. The R24-2 reconciliation: does the free half extrapolate from R24-2's model?

R24-2 (`docs/perf/R24_2_FREE_BY_MAGAZINE_STATE_GATE.md`) isolated, at N≤64
through the bare `HeapCore::dealloc` face (`dealloc_free_only_16b` family):

- cheap non-overflow push (SCOPE A, full free) ≈ **44.25 Ir** (amortized over 16)
- one overflow event (SCOPE A) = **571 Ir** (12.9× a cheap push)
- overflow fires at frees **#17, #25, #33, #41, #49, #57** for N=64 — period
  **8** (because `FLUSH_N = 8`: each overflow flushes 8 and leaves 8, so 8
  more pushes refill it to the next overflow).

The free half here goes through the `SeferAlloc::dealloc` face (a thin
`GlobalAlloc` dispatch on top of `HeapCore::dealloc`) at N=256. **Overflow
count at N=256, from R24-2's own confirmed period-8 pattern:**
`floor((256 − 17)/8) + 1 = floor(239/8) + 1 = 29 + 1 = 30` overflows;
cheap pushes = 256 − 30 = 226.

```
predicted free_cost(256) = 226 × 44.25 + 30 × 571
                         = 10,000.5 + 17,130 = 27,130.5
measured  free_cost(256) = Ir(cold_alloc_free) − Ir(cold_alloc_only)
                         = 50,164 − 22,318   = 27,846
reconciliation = 27,130.5 / 27,846 = 97.4%   (predicted 2.6% below measured)
per-free:       predicted 27,130.5/256 = 105.98   vs   measured 108.77   (2.6%)
```

**The reconciliation HOLDS within 2.6%.** R24-2's per-cheap-push (44.25) and
per-overflow-event (571) figures, extrapolated to the CORRECT 30-overflow
count at N=256, reconstruct the directly-measured free-only-256 cost to within
2.6%. The decomposition: **overflow is 30 × 571 = 17,130 of 27,846 = 61.5% of
the free cost; cheap pushes are 226 × 44.25 = 10,000.5 = 35.9%; the 2.6%
residual is non-linearity/subtraction noise** (the free-cost per-op is flat
across N at 108.77 → 110.79 → 111.80 for N/2N/4N, a ~3% spread, of which the
2.6% reconciliation residual is a part).

**This is the task's central deliverable, and it confirmed rather than
refuted:** the free half of `cold_alloc_free_256x16b` IS dominated by the same
overflow mechanic R24-2 isolated — extrapolation + a single new shared-prefix
arm sufficed, no all-new free-side machinery was needed.

### 5.1 The task brief's "~15 overflows at N=256" was an under-count

The task brief estimated "at N=256, overflow fires ~15 times." R24-2's own
confirmed data — which the brief instructed reusing — implies **30**, not 15
(overflow period 8, not 16; `FLUSH_N = 8` flushes half the magazine per event).
The "~15" model would predict `15 × 571 + 241 × 44.25 = 19,229`, only 69.1% of
the measured 27,846 — a 31% miss that would have falsely signalled "the model
breaks at N=256." Using the correct period-8 count (30) reconciles to 2.6%.
Reported here rather than silently corrected, per this project's "measured,
not spun" convention.

---

## 6. The mimalloc split and the three-outcome verdict

The task asked whether a per-half mimalloc split is needed for a fair
comparison, or whether a full-round comparison suffices. **A per-half split IS
needed** — without it, one cannot tell whether the 2× full-round gap
concentrates in SeferAlloc's alloc half or its free half, which is the whole
point of the three-outcome verdict. The three `mimalloc_cold_alloc_only` arms
provide it:

```
c_alloc_mi(N,2N) = (42,968 − 24,584) / 256 = 18,384 / 256 = 71.81 Ir/op
c_alloc_mi(2N,4N)= (75,872 − 42,968) / 512 = 32,904 / 512 = 64.27 Ir/op
free_mi(256)      = 32,325 − 24,584 = 7,741   → 30.24 Ir/free
free_mi(512)      = 58,389 − 42,968 = 15,421  → 30.12 Ir/free
free_mi(1024)     = 106,653 − 75,872 = 30,781 → 30.06 Ir/free
```

mimalloc's free cost is extraordinarily flat (30.06–30.24, <1% spread) — a
clean baseline. The per-half ratios (using N,2N for alloc, N=256 for free):

| half | SeferAlloc | mimalloc | Sefer / mimalloc |
|---|---:|---:|---:|
| alloc-only | 91.05 | 71.81 | **1.27×** |
| free-only | 108.77 | 30.24 | **3.60×** |
| full round (R23-2) | 203.86 | 101.81 | 2.00× |

**The verdict is OUTCOME (a):** the ~2× gap lives overwhelmingly in the FREE
half (3.60×), not the alloc half (1.27×), and the free half is 61.5% magazine
overflow (§5). (b) "stays on alloc-only" is ruled out — the alloc gap is a
modest 1.27×, and is itself mostly the magazine-aware path's overhead over
mimalloc's, not a single dominant mechanism. (c) "both halves roughly match
mimalloc's halves but the full round does not" is ruled out — SeferAlloc's
free half is decisively NOT matching mimalloc's (3.60×); no uncaptured
setup/segment-transition cost is needed to explain the gap, and R23-2's
warm-N/2N bootstrap-cancellation technique was already applied (it IS the
alloc-marginal derivation here), so outcome (c)'s specific failure mode
(an uncanceled bootstrap) does not apply.

**Outcome (a)'s explicit caveat — the task's framing was prescient:** the
root cause is now understood (free-side magazine overflow), but the two
approaches already tried against it both measured NO-GO regressions:

- **R24-3** (`flush_magazine_class` bitmap-clear merge, task #381): targeted
  the overflow's 84-Ir bitmap-clear sub-cost; measured **+37 Ir/overflow-event
  REGRESSION** (compiler fully unrolls the fixed-`FLUSH_N` pre-pass and CSE's
  it with `flush_class`'s run-grouping; the merged dynamic-length loop cannot
  unroll). NO-GO; reverted.
- **R24-4** (`SegmentBitmap::clear_many`/`set_many` bulk-mask primitive, task
  #382): targeted the deferred-clear loop in `alloc_batch`; measured **+14
  Ir/block REGRESSION** (the accumulator's per-offset bookkeeping costs more
  in-context than the hot-cache-line RMWs it coalesces). NO-GO; reverted.

Both NO-GOs were against **bitmap-clear** sub-costs (R24-2's one cleanly-
isolable overflow piece, 84 Ir). **Neither targeted the overflow's larger
piece — `flush_class` itself** (mark_free + dec_live + decommit-check per
block, ~487 Ir/event = the non-isolable remainder R24-2 §5.1 reported, =
571 − 84). The actionable untried lever the data points to is flush_class,
NOT further bitmap-clear work; a flush_class isolation measurement (the
prerequisite R24-2 §6 named for any flush_class optimization attempt) would
be the next measurement task, not an optimization.

---

## 7. What could NOT be cleanly isolated, and why

### 7.1 The refill event itself (virgin and recycled)

The refill is fused with the magazine pop in every 16th alloc, so
bench-arm subtraction gives `c_pop + c_refill/16` as one number — one
equation, two unknowns. Isolating `c_refill` standalone needs either a second
independent equation or a refill-standalone hook; the latter was NOT added
because production never runs `refill_class_bump_checked` outside the
magazine-miss context (same Heisenberg category as the
`flush_class`-standalone hook R24-2 §5.1 declined). The derived refill figures
(1099 virgin, 961 recycled) rest on `c_pop = 22.38` from R23-3, stated
wherever cited; the directly-measured carve-primitive floor (23.05 Ir/block)
is given alongside both.

### 7.2 The alloc-only N/2N non-linearity (−8.3%)

c_alloc(N,2N)=91.05 vs c_alloc(2N,4N)=83.50 is a larger non-linearity than the
full round's −3.7% (R23-2). Same direction (cheaper at larger N), same likely
cause (a fixed once-per-bench bootstrap/array-zero-init component amortizing
better at larger N — NOT a segment-boundary crossing, which would make the
SECOND half costlier, not cheaper). Reported honestly; the verdict (§6) is
robust to either figure (using 83.50, the alloc ratio is 83.50/64.27 = 1.30×,
still far below the free ratio 3.60×).

---

## 8. Verification performed

- **Read the mechanism FIRST** (§1): `carve_batch_only` vs the real
  `refill_magazine_slow` call chain; the round-2 magazine state (count=16
  full from round 1's frees); why N/2N is valid for the alloc marginal but
  NOT for the free cost.
- **Chose the isolation technique per-quantity** (§2): shared-prefix for the
  free-only cost and the round-2 split (valid — one-prefix-difference); N/2N
  for the alloc-only marginal (valid — linear bootstrap cancellation);
  derived-with-stated-prior for the refill event (the only honest option short
  of a Heisenberg-risk hook).
- **Two independent `npm run iai` runs** (50 benches each, `--features
  production`, the CI default) — byte-identical `Ir` for every bench including
  all 7 new arms, confirmed via an `awk`-extracted `name Ir` diff (`diff`
  exit 0).
- **Reference arms reproduced prior reports exactly** (§4) — 12 pre-existing
  arms byte-identical to R23-2/R24-2/R23-3's published numbers, confirming the
  byte-identical toolchain/host and that the new arms are comparable to
  R24-2's 44.25/571 model.
- **R24-2 reconciliation cross-check** (§5): the extrapolated 30-overflow
  model reconstructs the measured free-only-256 to 2.6%.
- **Round-2 internal-consistency cross-check** (§4.4): my round-2 split
  (alloc + free = 48,179) sums exactly to R23-3's independently-published
  48,179 round-2 total (188.20 Ir/op).
- **`cargo check --bench perf_gate_iai --features production`** (WSL2, the
  platform this bench compiles its real body under) — clean (the only warning
  is a pre-existing `proc-macro-error2` future-incompat, unrelated to this
  task).
- **`production`'s feature composition confirmed unchanged**: `grep -n
  "^production = " Cargo.toml` returns the same 7-feature list as R24-2;
  `Cargo.toml` is not in this task's diff.
- **No production behavior changed**: zero `src/` files touched; the only
  source-tree change is seven new `#[library_benchmark]` fns + group-list
  entries in `benches/perf_gate_iai.rs`. Zero new hooks.
- **clippy/fmt were NOT run** under WSL for this measurement-only change
  (rustfmt is not installed on this WSL nightly toolchain; `npm run check`
  is the reviewing session's pre-push gate, not part of this measurement
  task — same caveat as R24-2 §7). The new arms were written byte-identical
  to their mirrored siblings' bodies, so formatting matches by construction.

---

## 9. Files touched

- `benches/perf_gate_iai.rs` — added `cold_alloc_only_256x16b`/`_2n`/`_4n`,
  `mimalloc_cold_alloc_only_256x16b`/`_2n`/`_4n`, `recycle_alloc_only_256x16b`
  (7 new `#[library_benchmark]` fns); registered all seven in the `perf_gate`
  `library_benchmark_group!` list (50 benches total, up from 43). Zero changes
  to any pre-existing bench fn's body. **Zero new hooks.**
- `docs/perf/R24_5_COLD_ALLOC_FREE_SPLIT_GATE.md` — this report.
- `docs/perf/R24_5_COLD_ALLOC_FREE_SPLIT_GATE_summary.csv` — companion
  machine-readable summary.
- `docs/perf/_raw_r24_5_run1.log` / `_raw_r24_5_run2.log` — full raw
  `npm run iai` stdout for the two independent, byte-identical-`Ir` runs cited
  in §4. `git add -f` needed (`.gitignore` excludes `docs/perf/_raw_*.log` by
  default, R13-10/task #280).
- `docs/perf/OPEN_ITEMS.md` — item 1 gets a "DONE (task #383, R24-5)" note.
- `Cargo.toml` — **untouched** (confirmed in §8).

**Files needing `git add -f`** (gitignored by `.gitignore`, `/docs/perf/_raw_*.log`):

- `docs/perf/_raw_r24_5_run1.log`
- `docs/perf/_raw_r24_5_run2.log`

**Note on an out-of-scope tree artifact:** the working tree also carries
pre-existing `tests/*.rs` modifications that `git diff --ignore-all-space`
shows are pure CRLF/whitespace churn (empty content diff) — NOT produced by
this task (this task touched only `benches/perf_gate_iai.rs`), left untouched
per the measurement-only / leave-the-tree-unstaged instruction.
