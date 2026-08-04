# R34-25 (task #544) — Small-magazine provenance design: feasibility analysis (DESIGN-ONLY, no code change)

**Task:** a design-only feasibility study of an architectural candidate for
reducing the production magazine-**hit** bookkeeping cost (`segment_base_of_ptr`
+ `MagazineBitmap::clear_magazine`) on the alloc fast path — the component an
isolating gate (R29-10) sized at **~12.2 Ir/hit ≈ 54.5% of a magazine hit** —
in the hope of closing part of the **repeated bulk burst 16/64 B gap**
(measured at **2.37× / 2.28× slower than mimalloc** in the R32-R33 global
bench review, P1 item 2).

**Outcome: DESIGN-ONLY.** No `src/`, `Cargo.toml`, `tests/`, or `benches/` file
is modified. The deliverable is this doc. The verdict (§7) is
**NEED-MORE-RESEARCH, leaning NO-GO** for the headline gap on first-principles
grounds that are conclusive enough to make a full prototype not clearly
warranted at this time. §8 names the one cheap, code-free check a future round
should run *first* before committing to any prototype.

**Date:** 2026-08-05. **Base revision read:** `main` @
`9b06b566ff114ef301d6b753dd9670036c96fd86`. **Scope:** pure reasoning from
already-measured numbers (R3, R22-17, R23-1, R23-3, R24-2/3/4, R25-3, R25-8,
R29-10, R32-6) and a line-by-line read of the current hot path. **No
measurement is performed in this task** — the trigger that would justify one (a
first-principles case that the design is net-positive) is NOT met (§6), so
building a prototype now would repeat the Heisenberg pattern three prior
attempts in this exact region fell into.

**Why this is design-only and not a prototype, stated up front (the scope
decision CLAUDE.md and the task brief both ask to be justified):** three
independent prior attempts to reduce bookkeeping in this exact region were all
rejected (delayed clear, dual bitmap, run-encoded freelist — §3), each *after*
an implementation+measurement investment. The first-principles analysis in §6
shows the candidate's headline lever (cache the segment base to avoid
`segment_base_of_ptr`) is very likely **net-negative**, because the function it
targets is a single inlined AND whose measured 9.03 Ir cost is a *probe
artifact*, not its real inlined cost (§2.3). Spending a prototype's worth of
effort to re-discover "the AND was already cheap" — the same Heisenberg class
R24-3/R24-4/R25-3 hit — is not justified when the cheaper code-free check (§8,
a disassembly-level count of the inlined AND) can settle the load-bearing
premise first. This mirrors R25-8's own scope discipline ("the trigger that
would justify a measurement is unmet — §5").

---

## 0. TL;DR

| question | answer |
|---|---|
| Does the candidate address the **steady-state** 16/64 B bulk-burst gap? | **No.** The gap is steady-state (recycled-block hits after the first burst); the candidate's only sound lever (skip-clear for fresh-carve blocks, §6.2) helps only the **cold** first refill. Recycled hits must pay the full correctness-required clear (R3) and are irreducible under the current architecture (§6.4). |
| Does caching the segment base (the "palette") help? | **Almost certainly not — net-negative on first principles.** `segment_base_of_ptr` is a single inlined AND (~1 Ir). The 9.03 Ir attributed to it (R23-1) is a *probe artifact*: it was measured through a **non-inlined** `dbg_segment_base_of_ptr` hook + `black_box` + 2 call boundaries (§2.3) — overhead production inlines away. A palette replaces ~1 AND with a load + tag-extract + branch ≈ break-even-to-worse. |
| What CAN the design soundly address? | The **cold** magazine-bitmap work on the first refill of a freshly-carved run (skip-clear, §6.2): eliminate ~N mark-at-refill + N clear-at-pop bitmap RMWs for never-issued blocks, replacing them with cheap per-slot bit ops mirroring the existing `virgin_mask`. Low-complexity, but **transient**, not steady-state. |
| Honest ceiling on the addressed component? | Cold-path clear RMW only: ~3.16 Ir/hit × fresh-blocks on the first burst; **zero** on steady-state recycled hits. A projection, not a measurement (§6.3). |
| What is residual (the design CANNOT close)? | The steady-state recycled-hit clear (12.19 Ir/hit, R3-correctness-required); TLS/routing (`dealloc_routing`'s `contains_base`, R22-17); genuine allocator-policy differences vs mimalloc (whose page-thread design has no magazine↔substrate bitmap bridge). Closing the steady-state gap needs an **architectural** change to how cross-thread frees learn magazine residency — not bookkeeping (§6.4). |
| Verdict | **NEED-MORE-RESEARCH**, lean **NO-GO** for the headline gap. Do §8's code-free check first; build a gated prototype only if it refutes the net-negative premise. |

---

## 1. The cost being targeted — read the mechanism first

### 1.1 The production magazine-hit clear block

On every small-class magazine **hit** under plain `production`
(`alloc-global + fastbin`), `HeapCore::alloc`
(`src/registry/heap_core_alloc.rs`, the RAD-5 E4 block, ~lines 222–236) runs:

```text
let issued = self.tcache.classes[c].slots[new_cnt];
{
    let base = os::segment_base_of_ptr(issued);                 // (a)
    let off = (issued as usize - base as usize) as u32;         // (b)
    SegmentMeta::new(base).magazine_bitmap().clear_magazine(off); // (c)
}
return issued;
```

`(a)` re-derives the SEGMENT-aligned base from the just-popped pointer
(`ptr.map_addr(|a| a & !(SEGMENT - 1))`, a single AND — `src/alloc_core/os.rs:121`).
`(b)` computes the segment-relative offset. `(c)` constructs a `MagazineBitmap`
view over the segment header's residency bitmap (`SegmentMeta::new(base)` just
stores `base`; `.magazine_bitmap()` is `MagazineBitmap::new(base +
Layout::magazine_bitmap_off())` — `segment_header.rs:1183`; the offset is a
`const fn`, so this is one `base + CONST` add) and clears `issued`'s bit
(`SegmentBitmap::clear`: `locate(off)` → byte-index + mask arithmetic, then a
byte load + AND-with-`!mask` + store — `segment_bitmap.rs:102`).

### 1.2 Why the clear exists and cannot be deferred (R3)

`MagazineBitmap` (RAD-5, GO) is the **only window `AllocCore` has into magazine
state**. `AllocCore` (the single-threaded substrate) has no magazine concept;
its cross-thread free-drain path `reclaim_offset_checked`
(`src/alloc_core/alloc_core_small_reclaim.rs:68`) consults `is_in_magazine(off)`
(line 145) to decide whether a remote-free note is the duplicate leg of a
cross-thread double-free. The own-thread free path
(`dealloc_own_thread_with_base`, `src/registry/heap_core_free.rs`) consults the
same oracle. R3 (`docs/perf/IAI_BASELINE.md` §"R3 honest-reject") proved the
clear **cannot be deferred** off the issue path: a stale `1` after issue (a)
makes a legitimately-freed block read as an in-magazine double-free → **leaked**
at the own-thread oracle, and (b) makes a genuine remote-free note look like a
duplicate cross-thread leg → **dropped/leaked** at `reclaim_offset_checked`.
**The bit must be exact at the ISSUE moment.** This is the load-bearing
correctness invariant any candidate in this region inherits.

### 1.3 The asymmetry that motivates the candidate

The **free** path threads the base through: `dealloc_routing`
(`src/registry/heap_core_xthread.rs`) computes `base = segment_base_of_ptr(ptr)`
**once**, then passes it into `dealloc_own_thread_with_base(ptr, layout, base)`
(`src/registry/heap_core_free.rs:298`), which reuses it for the `is_free` check
**and** `mark_magazine` — **no re-derivation**. The **alloc** (hit) path cannot
do this, because the magazine stores only **bare pointers**
(`PerClass::slots: [*mut u8; TCACHE_CAP]`, `tcache.rs:239`); on a hit it must
re-derive `base` from the popped pointer. This asymmetry — "the free path
already has the base; the alloc hit re-derives it" — is the intuition behind
both R29-10's point 4 (§2.4) and the candidate under study.

---

## 2. The cost decomposition — and why its headline attribution is misleading

### 2.1 The measured number (R29-10)

R29-10 (`docs/perf/R29_10_ALLOC_HIT_CLEAR_MAGAZINE_ISOLATION_GATE.md`) isolated
the whole `(a)+(b)+(c)` block at **12.19 Ir/hit** (195 Ir / 16 hits, two
byte-identical `npm run iai` runs) via a `bench-internals`-gated
`dbg_clear_magazine_on_hit` hook that **inlines the production block verbatim**
(`src/registry/heap_core_diag.rs:975`). That 12.19 Ir is **54.5% of a magazine
hit** (the hit itself reproduces R23-3's 22.4 Ir/op exactly).

### 2.2 The decomposition R29-10 published

R29-10 §3.3 decomposed the 12.19 Ir as **~9.03 Ir `segment_base_of_ptr`**
(R23-1's isolated figure for the same function) **+ ~3.16 Ir** residual
(`SegmentMeta::new` + `magazine_bitmap()` + the `clear_magazine` RMW), and
concluded the dominant sub-cost is `segment_base_of_ptr`. **That conclusion is
the seed of the candidate under study** — if 9 of the 12 Ir is base-derivation,
caching the base should recover most of it.

### 2.3 Why the 9.03 Ir is a PROBE ARTIFACT, not the real inlined cost — the load-bearing finding of this design study

R23-1's 9.03 Ir/call was measured by `dealloc_segment_base_of_ptr_probe_only_16b`,
whose timed loop is:

```text
let base = unsafe { (*heap).dbg_segment_base_of_ptr(ptr) };
black_box(base);
```

`dbg_segment_base_of_ptr` (`src/registry/heap_core_diag.rs:555`) is a **plain
`pub fn` — NOT `#[inline]`** — that delegates to `os::segment_base_of_ptr`
(which *is* `#[inline(always)]`, so it inlines *into `dbg_*`*). So each measured
call pays: **a call into `dbg_segment_base_of_ptr`** + the inlined `map_addr`
AND + **`black_box`** + **a return** — two of which production **never pays**,
because in production `os::segment_base_of_ptr` is `#[inline(always)]` and is
inlined *directly into the magazine-hit block* with no call boundary and no
`black_box`.

R29-10 §3.3 **acknowledges this itself**: *"R23-1's probe carries a per-call
`black_box` this arm does not, so the residual is an upper bound on the clear
proper, not a separate anomaly."* The same caveat applies in the other
direction: the 9.03 attributed to `segment_base_of_ptr` is an **upper bound**
inflated by the non-inlined call boundary + `black_box`; the real inlined
contribution of the `map_addr` AND to the production block is ~1 Ir.

**Implication for the candidate:** the "9 of 12 Ir is base-derivation" framing
that motivates caching the base rests on a probe-isolated figure, not the
inlined production cost. You cannot recover 9 Ir by caching the base, because
the inlined AND was never 9 Ir — most of the block's 12.19 Ir is the
**irreducible rest of the address-derivation + RMW chain** (`off` subtract,
`base + CONST` add, `locate`'s two shifts + mask, the byte load + AND + store),
all of which must run *regardless of how `base` was obtained*. This is
quantified in §6.1.

> This is not a rebuttal of R23-1/R29-10 — both reports are honest and R29-10
> flags the caveat explicitly. It is a re-reading of their *attribution*
> through the lens of "what is the candidate's actual addressable ceiling",
> which neither report needed to do (R29-10's scope was to *measure*, then
> *close permanently*).

### 2.4 R29-10 already named this exact candidate — and dismissed the naive form

R29-10 §5 point 4: *"A theoretical lever exists but is speculative and out of
scope here: caching the segment base ALONGSIDE the slot pointer in the tcache
(`(ptr, base)` pairs instead of bare `ptr`) would eliminate the per-hit
`segment_base_of_ptr` re-derivation — but it doubles the magazine's per-slot
footprint (cache-density risk) and, per this project's own R26-7 '~10×
Heisenberg gap' lesson, any such attempt MUST be an in-context A/B on
`small_churn_16b`, never a standalone-hook extrapolation."* The candidate under
study is the **refined form** of this — a per-class *palette* of bases (not a
per-slot `(ptr, base)` pair) to avoid the footprint doubling. §4 designs it
concretely; §6 shows the refinement does not rescue the lever, because the lever
itself (eliminate the AND) targets ~1 Ir, not 9.

---

## 3. Deny-list — prior attempts in this exact region and why each failed

A candidate here MUST not be a re-skin of a rejected form. Three are on the
deny-list (`docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md`
lines 255, 349; `docs/agent_reviews_us/radical_optimization_review.md` line 700):

### 3.1 Delayed clear / batching `MagazineBitmap::clear_magazine` off the issue path (R3)

**Form proposed (round-4 review finding R3):** defer the per-hit clear to
refill/flush time, on the theory that an existing in-magazine scan could absorb
the precision loss.

**Rejected: CORRECTNESS NO-GO.** The bitmap's exactness at the ISSUE moment is
load-bearing at two sites (§1.2): a stale `1` after issue (a) leaks a block at
the own-thread free oracle (`legit_free_after_pop_is_not_swallowed` would go
RED) and (b) drops/leaks it at `reclaim_offset_checked` (AllocCore has no
magazine concept; the bitmap is its only window). The review's suggested
fallback (re-scan `slots[0..cnt]`) does not exist post-RAD-5 — RAD-5 *deleted*
that scan and replaced it *with* this bitmap; re-adding it would only help site
(a), and site (b) is unreachable from a scan by construction, so closing it
would require re-adding the exact O(count) scan RAD-5 proved more expensive than
the bitmap. **The clear is correctness-required at issue time; the 12.19 Ir is a
fixed per-hit cost, not a tunable one.** (IAI_BASELINE §"R3 honest-reject".)

### 3.2 Bloom / dual bitmap — merge `AllocBitmap` + `MagazineBitmap` into one 2-bit-per-granule region (R32-6 / F1b)

**Form proposed:** one combined 2-bit-per-granule state word, so the free path's
`is_in_magazine` + `is_free` read pair costs one `locate` + one load instead of
two, from two cache lines 32 KiB apart.

**Rejected: COST NO-GO (implemented + measured).** Every bitmap-touching bench
**regressed** +3.5…+8.2 Ir/op, 20–25× past the ±10 churn kill threshold
(`R32_6_DUAL_BITMAP_GATE.md`). Root cause (§3.4 of that report): the 2-bit
packing (4-granules-per-byte) requires a more expensive `locate` (an extra
`pair_shift` computation) paid by **every single-plane call site**
(`pop_free`, `carve_batch`, `drain_freelist_batch`, `flush_run`,
`reclaim_offset*`), which *vastly outnumber* the two dual-oracle call sites that
would benefit. The storage merge itself is the dominant cost, not the
combined-read. **The two bitmaps stay separate in storage/addressing.**

### 3.3 Run-encoded freelist (Ф0 / PERF3 / R25-8)

**Form proposed:** record `(segment, first_off, count, stride)` for a
contiguous batch and allocate FROM the run arithmetically (no free-list chain
walk while the run is intact), eliminating the `read_next` dependent load on
the alloc side.

**Rejected: CONDITIONAL-GO, but the region that motivated it is OUTSIDE its
reach** (`R25_8_RUN_ENCODED_FREE_BATCH_DESIGN.md`). Three findings collapse the
scope: **(1)** the magazine-overflow flushes `slots[0..FLUSH_N]` — the 8
**oldest freed blocks** — are at **arbitrary offsets** (the magazine is LIFO in
*free-order*, not offset-order), so a run-descriptor cannot encode them without
an O(n log n) offset-sort per flush that would itself exceed the savings; only
`dealloc_batch` of a freshly-carved batch is contiguous by construction, and
that API has **no downstream consumer** (not in `production`). **(2)** the M2
double-free guard (`AllocBitmap::mark_free`/`mark_alloc`) **cannot be
eliminated** by a run-descriptor (a run-block has no materialized free-list
node, so the bitmap is the only record it is free). **(3)** the free-side
region is 3× NO-GO (R24-3/R24-4/R25-3); the alloc-side `read_next` lever is
untried but only reachable if runs survive free→refill, which fragmentation
makes unlikely. The earlier Ф0/PERF3 honest-reject measured +23…31% Ir
regression. **Run-descriptors are sound only for the contiguous-batch shape,
which has no production consumer.**

### 3.4 What the deny-list establishes for THIS candidate

Any new design in this region must: keep the clear at issue time (§3.1); keep
the two bitmaps separate in storage (§3.2); not depend on offset-contiguity for
the magazine path (§3.3); and not assume a contiguous-batch consumer exists. The
candidate under study (§4) respects all four — which is why §6 then asks whether
what remains is *worth* anything, and concludes it is not for the headline gap.

---

## 4. The new candidate, described concretely

The review (`docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md`
§"P1: убрать значимую часть Small magazine-hit bookkeeping", lines 257–261)
proposed three coupled ideas. Designed concretely against the current code:

### 4.1 Per-class segment-base palette + compact slot index

```text
// SKETCH — NOT applied. Illustrative only.
const PROVENANCE_PALETTE_SIZE: usize = 4; // 2 bits/slot tag

// Added to PerClass (tcache.rs), repr(C) to preserve the F4 cache-line layout:
//   prov_tags:  u32,                                 // 16 slots × 2 bits = 32 bits
//   prov_bases: [*mut u8; PROVENANCE_PALETTE_SIZE],  // 4 segment bases (or null = "loose")
```

On a magazine **hit**, instead of `os::segment_base_of_ptr(issued)`, look up
`base = prov_bases[(prov_tags >> (2*new_cnt)) & 3]`; if that entry is the
"loose" sentinel (null), fall back to the AND. On **push** (free path, which
already has `base`), insert `base` into the palette and store the tag. On
**refill** (fresh carve from one segment), all `want` slots get tag 0 and
`prov_bases[0]` = the carve segment — one palette entry serves the whole run.

### 4.2 Refill-run descriptor for a homogeneous freshly-carved run

A single `active_run: Option<RunProvenance { base, first_off, count }>` per
class (or per magazine), set at refill when `refill_class_bump` carves a
contiguous batch (`off = aligned_start + i*block_size`,
`alloc_core_small.rs` carve). While intact, pops from the run's tail recover
`base` and `off` arithmetically (`off = first_off + (count-1)*stride`) with no
per-slot palette/tag read. **Fallback to loose pointers** for recycled blocks
(blocks freed back into the magazine, not part of the intact run): they keep
bare `slots[i]` pointers and a "loose" tag, re-deriving `base` via the AND on
pop — exactly the current path.

### 4.3 Refill-local sidecar for batched bitmap clear

When several retained slots belong to one segment (the fresh-carve run), their
magazine-bitmap bits occupy adjacent/identical bytes (8 blocks per byte for the
default 4 MiB / 16 B geometry — `FOOTPRINT = SEGMENT/MIN_BLOCK/8`). Instead of
`mark_magazine` per block at refill + `clear_magazine` per block at pop, clear
the whole run's bits in one batched byte-pass at refill (or skip them entirely
— §6.2), so individual pops do not touch the bitmap.

### 4.4 What the candidate respects (vs the deny-list)

- Clear stays at issue time conceptually — but for fresh-carve blocks the bit is
  *already clear* (carve's leave-unset + skip-mark), so the "clear" is a no-op,
  not a deferral (§6.2 analyses the correctness of this distinction, which is
  NOT R3's stale-`1` deferral).
- The two bitmaps stay separate (no 2-bit packing).
- No offset-sort required: the fresh-carve run IS contiguous by construction;
  recycled blocks fall back to the current loose path, not a run encoding.
- No new consumer required: it targets the existing magazine hit/miss path.

---

## 5. Why a prototype is NOT built in this task (scope decision)

The task brief explicitly allows a design-only deliverable when a prototype is
not clearly warranted. Three reasons it is not warranted here:

1. **The first-principles case is net-negative for the headline lever (§6.1).**
   Spending a prototype to re-confirm "the AND was already cheap" repeats the
   Heisenberg pattern of §3.1–§3.3.
2. **The only sound lever (§6.2) helps the cold path, not the steady-state
   gap (§6.3).** The headline 2.37×/2.28× is steady-state; the lever's benefit
   is transient. A prototype would measure a benefit that does not map to the
   gap it was proposed to close.
3. **The full gate cycle the task names (16/64 bulk burst, steady churn, dealloc
   correctness, cross-thread free, cache footprint, miri/loom) touches the
   hottest path in the allocator and the F4-tuned `PerClass` layout.** That is a
   large implementation investment for a lever whose premise (§6.1) a cheaper,
   code-free check (§8) can settle first.

CLAUDE.md's "no new hot-path metadata without counterfactual AND footprint gate"
rule would require that full cycle *before any claim stronger than "promising
direction"* — and §6 says the direction is not promising enough to start it.

---

## 6. First-principles cost analysis of each lever (a PROJECTION, not a measurement)

### 6.1 Lever A — palette base-cache (avoid `segment_base_of_ptr`): NET-NEGATIVE

The production block is, in instructions:

```text
base = issued & !(SEGMENT-1)   // (a) 1 AND            ← what the palette would replace
off  = issued - base           // (b) 1 sub            ← stays (need off for locate)
bits = base + magazine_bitmap_off()  // 1 add (CONST)  ← stays (need the bitmap byte addr)
bit  = off >> MIN_BLOCK_SHIFT  // 1 shift              ← stays (locate)
bidx = bit >> 3                // 1 shift              ← stays (locate)
mask = 1 << (bit & 7)          // 1 and + 1 shift      ← stays (locate)
p    = bits + bidx             // 1 add                ← stays
byte = *p                      // 1 load               ← stays (the RMW)
*p   = byte & !mask            // 1 store              ← stays (the RMW)
```

The palette replaces `(a)` — **one AND** — with: a `prov_tags` shift+mask
extract (2 ops) + a `prov_bases[tag]` load (1 op, resident in the tcache line)
+ a "loose"-sentinel compare + conditional branch (2–3 ops). **Net: the palette
adds ~2–4 ops to save 1.** The other ~9 ops of the block — `off`, `bits`,
`locate`, the byte load + AND + store — are **irreducible** given the bitmap
must be touched, and they do not depend on how `base` was obtained.

This is the direct consequence of §2.3: the 9.03 Ir attribution was a probe
artifact; the real inlined AND is ~1 Ir, and caching it cannot recover 9 Ir
because 9 Ir was never the AND's inlined cost. **Lever A is net-negative on
first principles.** (Calibration: this is a projection from the instruction
shapes above + §2.3's probe-artifact finding, not a measurement; a disassembly
count per §8 could confirm, but the shapes are unambiguous — an AND is one
instruction.)

### 6.2 Lever B — skip-clear for fresh-carve blocks (the one SOUND lever): CORRECT but COLD-ONLY

**The idea:** a freshly-carved block entering the magazine via
`refill_class_bump` has its magazine bit **already 0** (carve's leave-unset
optimization touches only `AllocBitmap`, never `MagazineBitmap`; the bitmap is
zero at bootstrap). The current refill then runs `mark_magazine` to set it to 1,
and the pop later runs `clear_magazine` to set it back to 0 — **two RMWs that
net to the original 0**. If the magazine tracked "this slot is a never-issued
fresh-carve block" (a per-slot bit, exactly mirroring the existing
`virgin_mask`), refill could **skip the mark** and pop could **skip the clear**
for those slots: the bit stays 0 throughout, which is the correct value for a
never-issued block.

**Correctness (why this is NOT R3's stale-`1` deferral):** R3 rejected a *stale
`1` after issue* (a block reads as in-magazine when it is not). Lever B keeps
the bit at its **correct** value (0) for blocks that are genuinely not-yet-issued.
The two consumers of `is_in_magazine` — the own-thread free oracle and
`reclaim_offset_checked` — only ever consult the bit for a block being **freed**,
i.e. a block that **was issued** (it is in the caller's hands). A never-issued
fresh-carve block sitting in the magazine is never the subject of either free
path (no caller has its pointer; no cross-thread note can target it). So a `0`
bit on a resident fresh-carve block is never *read* as a wrong answer. The
moment such a block is popped and re-pushed as a recycled block, the free path's
`mark_magazine` sets the bit to 1 as usual, and the "fresh" bit is cleared —
restoring exact bookkeeping for the block's new (recycled) state. **This is
sound**, and it is the same per-slot-bit pattern `virgin_mask` already uses
(indeed, under `virgin-zero-skip` the two concepts coincide: a magazine-resident
virgin block is by definition a never-issued fresh-carve block).

**The killer — it is COLD-ONLY:** a "repeated bulk burst" workload (alloc N,
free N, repeat) is **steady-state recycled** after the very first burst:

```text
Burst 1: miss → refill (fresh-carve N). Pop N (FRESH → skip-clear helps here).
Free N:  push N recycled blocks (mark_magazine each → bits now 1).
Burst 2: hit N recycled blocks. Pop N (RECYCLED → must clear, bit is 1).
Free N:  push N recycled. ...
```

From burst 2 onward, **every hit is on a recycled block** whose bit is 1 and
**must** be cleared at issue (R3). Lever B helps **only burst 1** (the cold
refill). The headline 2.37×/2.28× gap is a **repeated** (steady-state) burst
gap; lever B does not touch it. (This is the same "benefit confined to the
contiguous/initial shape, not steady churn" finding R25-8 §3.1/§3.2 reached for
the run-descriptor's free side.)

### 6.3 Lever C — batched bitmap clear at refill (the sidecar): R25-8 territory, Heisenberg risk

Batching the clear across a contiguous run's shared bitmap bytes (§4.3) is sound
arithmetic (8 fresh 16 B blocks share one bitmap byte → one load + one
multi-bit-AND + one store instead of eight triplets), but: (a) it helps the same
**cold refill** as lever B (the run is intact only on first carve; steady-state
churn fragments it — R25-8 §3.2), so it shares lever B's cold-only ceiling; (b)
it adds a refill-local sidecar whose maintenance (run creation, split on foreign
free into the range, invalidation on decommit) is exactly the complexity R25-8
§3.2/§3.5 showed erodes the saving; (c) three prior attempts to coalesce
per-offset bitmap RMWs into bulk primitives (R24-3, R24-4, R25-3) were all
measured NO-GO for the same "the bulk primitive's own bookkeeping costs more
than the hot RMW it coalesces" Heisenberg reason. **Lever C is not free of the
pattern that killed its three predecessors.**

### 6.4 The steady-state gap is residual — what the design CANNOT close

The **steady-state** recycled-block hit must pay: `(a)` the AND + `(b)` off +
`(c)` the full `clear_magazine` RMW = the **entire 12.19 Ir/hit block**, because
for a recycled block the bit is genuinely 1 at issue and **must** be cleared
(R3). No bookkeeping rearrangement on the magazine side changes this: the bit is
the correctness-required bridge between the magazine (in `HeapCore`/tcache) and
`AllocCore`'s cross-thread drain, and it must be exact at the moment the block
leaves the magazine. The only way to remove the RMW for recycled blocks is to
**change how cross-thread frees learn magazine residency** — i.e. replace the
per-offset segment-header bitmap with a different bridge (e.g. a magazine-side
structure `AllocCore` queries). That is an **architectural** change, not a
bookkeeping optimization, and it is exactly the kind of change R3 proved is
not a free lunch (the pre-RAD-5 O(count) scan it would re-introduce was measured
more expensive than the bitmap). **It is out of scope for a "small-magazine
provenance" task and is named here only to bound the residual honestly.**

The rest of the 2.37×/2.28× gap beyond the clear block lives in TLS/routing
(`dealloc_routing`'s `contains_base`, R22-17, itself soundness-blocked from a
header-first shortcut) and in genuine allocator-policy differences vs mimalloc
(whose page-thread design carries no magazine↔substrate bitmap bridge at all).

---

## 7. Verdict — NEED-MORE-RESEARCH, lean NO-GO for the headline gap

### 7.1 The verdict

**NEED-MORE-RESEARCH**, with a strong lean toward **NO-GO** for the *headline*
steady-state 16/64 B bulk-burst gap. The candidate's three levers reduce to:
- **A (palette base-cache):** net-negative on first principles (§6.1) — the AND
  it targets is ~1 inlined Ir, and the 9 Ir attribution was a probe artifact.
- **B (skip-clear for fresh-carve):** sound and low-complexity (§6.2), but
  **cold-only** — it does not touch the steady-state gap.
- **C (batched clear sidecar):** R25-8 territory, cold-only, Heisenberg-risk
  (§6.3).

No lever materially addresses the steady-state recycled-hit clear, which is the
correctness-required core of the gap (§6.4).

### 7.2 Why not a clean NO-GO

Two honest caveats keep this from being a flat NO-GO:

1. **The §6.1 net-negative claim is a projection, not a measurement.** It rests
   on "an AND is one instruction" + §2.3's probe-artifact argument, both
   well-supported but neither directly measured for the *inlined production*
   block. If a disassembly-level check (§8) showed the inlined
   `segment_base_of_ptr` + its surrounding address derivation is genuinely
   >3 Ir in a way the palette could fold (e.g. `map_addr` emits more than an
   AND on this target, or `magazine_bitmap_off()` is not constant-folded), lever
   A would deserve a gated prototype.
2. **Lever B (cold-path skip-clear) is a real, sound, low-complexity
   micro-win** even though it does not close the headline gap. If a future round
   has a workload where cold refills dominate (e.g. many short-lived threads each
   doing one burst), it could be worth shipping behind an experimental flag —
   but that is a *different* motivation than the bulk-burst gap, and must pass
   the full gate cycle (footprint + churn + correctness + cross-thread) first.

### 7.3 What this task does NOT recommend

- Do NOT build a palette prototype to chase the steady-state gap — §6.1/§6.4 say
  it cannot close it.
- Do NOT re-attempt lever C without a contiguous-batch consumer (R25-8's
  trigger (a), still unmet).
- Do NOT promote anything into `production` without the full gate cycle
  (CLAUDE.md's "no new hot-path metadata without counterfactual AND footprint
  gate" rule; the F4 `PerClass` layout is deliberately tuned and any new field
  must re-pass the cache-line asserts in `tcache.rs:248–260`).

---

## 8. The one cheap, code-free check a future round should run FIRST

Before any prototype, settle the load-bearing premise of lever A — is the
inlined `segment_base_of_ptr` + address-derivation chain genuinely expensive, or
is the AND ~1 Ir as §6.1 projects?

```text
# Produce the release binary for the production magazine-hit path, then
# disassemble HeapCore::alloc's magazine-hit arm and COUNT the instructions
# from `issued = slots[new_cnt]` through the `clear_magazine` store.
cargo rustc --release --features production --lib -- -C debuginfo=1
# locate the magazine-hit block in the asm and count instructions in (a)+(b)+(c).
```

If the inlined AND + address derivation is ≤3 instructions (as §6.1 projects),
lever A is confirmed net-negative and the candidate is NO-GO for the headline
gap — no prototype needed. If it is materially larger (e.g. `map_addr` is not a
single AND on this target, or `magazine_bitmap_off()` is not folded), a gated
1-entry-palette prototype (the lowest-risk form of §4.1) becomes worth a full
gate-cycle run via the R34-7 causal harness
(`scripts/r34_7_causal_harness.mjs`, `examples/r34_7_causal_worker_*.rs`).

This check is code-free (no `src/` change), cheap (a disassembly read), and
directly falsifies/confirmes the premise the whole candidate rests on. It is the
honest next step, not a prototype.

---

## 9. Addressed-component vs residual (the split the task brief requires)

| component | Ir/hit (16 B hit) | addressable by this candidate? | notes |
|---|---:|---|---|
| `segment_base_of_ptr` inlined AND | ~1 (projection; §2.3/§6.1) | **no — net-negative** (palette adds ≥2 ops to save 1) | the 9.03 Ir attribution is a probe artifact |
| `off` subtract + `bits` add + `locate` arithmetic | ~5–6 (projection) | **no — irreducible** (needed for the RMW regardless of base source) | |
| `clear_magazine` byte RMW (load+AND+store) on **recycled** hits | ~3–4 (the 3.16 residual, steady-state) | **no — R3 correctness** (bit must be exact at issue) | the core of the steady-state gap (§6.4) |
| `clear_magazine` RMW on **fresh-carve** hits (cold refill only) | ~3–4 (transient) | **yes, via lever B** (skip-clear, §6.2) — but cold-only | does not touch the steady-state 2.37×/2.28× |
| `mark_magazine` at refill for fresh-carve blocks | ~3–4 × N (cold) | **yes, via lever B** (skip-mark) — but cold-only | same cold-only ceiling |
| rest of the 22.4 Ir hit (count dec, slot read, virgin_mask AND, return) | ~10 | **no** | not magazine-bitmap-related |
| TLS/routing (`contains_base`, R22-17) + allocator-policy gap to mimalloc | (separate axes) | **no** | residual; mimalloc has no magazine↔substrate bitmap bridge |

**Net: the candidate soundly addresses only the cold-path magazine-bitmap work
(levers B, ~3–4 Ir/hit × fresh-blocks on the first burst), which is transient and
does not map to the steady-state 2.37×/2.28× bulk-burst gap. The steady-state
gap's core (the recycled-hit clear, R3-required) and the base-derivation
arithmetic are residual under the current bitmap-bridge architecture.**

---

## 10. Verification performed (design-only deliverable)

- **Read the full hot path** before reasoning: `heap_core_alloc.rs` magazine-hit
  arm (RAD-5 E4 block) and its `alloc_zeroed` sibling;
  `heap_core_free.rs::dealloc_own_thread_with_base` (push/overflow-flush/mark);
  `tcache.rs::PerClass` (F4 `#[repr(C)]` layout + cache-line asserts);
  `segment_bitmap.rs` / `magazine_bitmap.rs` (the `locate`/`clear` mechanism);
  `os.rs::segment_base_of_ptr` (the single AND);
  `segment_header.rs::SegmentMeta::magazine_bitmap` (the `base + CONST` view);
  `alloc_core_small_reclaim.rs::reclaim_offset_checked` (the cross-thread
  `is_in_magazine` consumer).
- **Read every deny-list gate report** (R3 in IAI_BASELINE; R32-6 dual-bitmap;
  R25-8 run-encoded) and the cost-attribution gates (R22-17, R23-1, R23-3,
  R29-10) end-to-end, including R29-10 §3.3's own probe-artifact caveat and
  §5 point 4's prior identification of this exact candidate.
- **No `src/`/`Cargo.toml`/`tests/`/`benches/` file is modified.** This is a
  documentation-only deliverable; the sanity checks below confirm nothing was
  broken by the doc addition.
- `cargo fmt --check`, `cargo clippy`, `cargo test` run as sanity checks per the
  task's "design-only still verify" instruction — results in the commit message.

**Immutable source identity of this analysis:** `main` @
`9b06b566ff114ef301d6b753dd9670036c96fd86` (read-only; no working-tree code
change). All cited Ir figures are from already-committed gate reports at the
SHAs those reports record; this doc introduces no new measurement.

**Commit prefix:** `docs(process)` — design-only feasibility study, no runtime
or opt-in code changed, no speedup claimed (per CLAUDE.md's R30-12 taxonomy, a
gate/verdict that rests on no new measurement is not `bench`/`perf(*)`). The
candidate is explicitly NOT promoted toward `production` (§7.3).
