# R25-8 — Run-encoded free batch (arithmetic free list): DESIGN-ONLY (no code change)

**Task:** a design-only study of a "run-descriptor" alternative to the current
intrusive per-block free list. The current mechanism writes free-list linkage
into each freed block's own payload (the intrusive `next` word) PLUS does
per-block bitmap work (the M2 double-free guard) even for a homogeneous batch
of same-class, same-segment blocks whose offsets are arithmetically
predictable. This doc explores recording `(segment, first_offset, count,
stride)` for such a batch, allocating FROM the run arithmetically (no
free-list walk while the run is intact), and materializing ordinary
free-list nodes only when a run must split or escape.
**Outcome:** **DESIGN-ONLY.** No `src/`, `Cargo.toml`, `tests/`, or `benches/`
file is modified. The deliverable is this doc + an `OPEN_ITEMS.md` `[D]`-tier
entry. §5 states the verdict. **No implementation is recommended now** (§5's
conditional bar is deliberately high — three NO-GOs in the exact region this
targets, plus no downstream consumer for the one shape that satisfies the
precondition).
**Date:** 2026-07-28. **Base revision:** `main` @ `52fafe0` (R25-1/R25-2/R25-7
landed; R25-3 NO-GO @ `0465c97` is the trigger). Line numbers below are current
as of `52fafe0`.
**Platform:** Windows 10 Pro x86-64 (analysis host). This is a design study:
the cost analysis (§1, §4) is analytical from R24-2's measured Ir
decomposition and instruction-level reasoning, mirroring R10-4 §5 / R9-5 §8.
No measurement is performed (the trigger that would justify one is unmet —
§5).

---

## 0. TL;DR — CONDITIONAL-GO, single narrow lever, gated on two independent triggers (neither met)

The design is **sound in principle** — arithmetic runs are exactly how
mimalloc structures its per-page free lists (a "page" of same-class blocks is
a run; the free list is offset-predictable). The genuinely novel lever this
design offers that **none** of the three prior NO-GOs (R24-3 `flush_magazine_class`,
R24-4 bulk-mask, R25-3 `FLUSH_N`) touched is the **allocation side**: a
contiguous run lets `drain_freelist_batch` skip the per-block `read_next`
dependent load (the chain walk `drain_freelist_batch`'s own doc at
`alloc_core_small.rs:1306-1312` calls "irreducible, no way to hoist it" —
that claim is true for a SCATTERED free list and FALSE for an arithmetic run).

**But three findings collapse the scope to a narrow, conditional sliver:**

1. **The mechanism's natural target — the magazine-overflow free path — does
   NOT satisfy the run-descriptor's own contiguity precondition** (§3.1). A
   magazine is a LIFO stack in FREE-order, not offset-order; `slots[0..FLUSH_N]`
   (the flushed 8) are the 8 oldest-freed blocks at arbitrary offsets. Only
   `dealloc_batch` of a freshly-carved contiguous batch is contiguous by
   construction (`carve_batch`, `alloc_core_small.rs:1710`,
   `off = aligned_start + i * block_size`).
2. **The M2 double-free guard (`AllocBitmap::mark_free`) cannot be eliminated
   by a run-descriptor** (§3.3). A block inside an intact run has no
   materialized free-list node, so the bitmap MUST still record it free, or a
   double-free of a run-block is undetected. This forces `mark_free` to stay
   per-block — leaving only the `Node::write_next` payload store as the
   free-side cost a run could eliminate. That store is the same "cheap
   hot-cache-line RMW" class R24-4 already measured as a **+14 Ir/block net
   REGRESSION** to coalesce (Heisenberg: the coalescing bookkeeping cost more
   than the hot store).
3. **The free-side region (magazine overflow) is now 3× NO-GO** (R24-3/R24-4/
   R25-3); the alloc-side lever (the `read_next` chain) is untried but only
   reachable if runs survive from free-time to refill-time, which the
   fragmentation logic (§3.2) makes unlikely for the magazine path.

**Verdict (§5): CONDITIONAL-GO**, gated on BOTH (a) R23-7's `dealloc_batch`
promoted from P2/no-downstream-consumer (the only shape with guaranteed
contiguity), AND (b) a Stage-1 Ir measurement on THAT consumer confirming the
`drain_freelist_batch` `read_next` dependent-load chain is the dominant
remaining cost. The magazine-overflow free path — the region that motivated
this study — is **explicitly OUTSIDE** the conditional's scope: its flushes
are not offset-contiguous, so a run-descriptor cannot encode them without an
O(n) offset-sort per flush that would itself exceed the per-block savings.

---

## 1. Problem statement — the concrete cost this design targets

### 1.1 The magazine-overflow event, decomposed (R24-2)

R24-2 (`R24_2_FREE_BY_MAGAZINE_STATE_GATE.md` §4.3) isolated one magazine-
overflow event at **571 Ir = 12.9× a cheap non-overflow push** (43–44 Ir).
The overflow decomposes into exactly two sub-costs:

- **(i) The per-block bitmap-clear pass — 84 Ir** (R24-2 §4.4, measured via
  the `dbg_overflow_bitmap_clear_pass` hook against `heap_core_free.rs:762-768`'s
  `clear_magazine` loop; the `MagazineBitmap` clear, the RAD-5 second bitmap).
  R24-3 tried to merge this into `flush_run` → **NO-GO, +37 Ir** (a fixed-
  length `const` loop the compiler unrolled/CSE'd became a dynamic-length
  run-grouping loop). R24-4 tried `clear_many` bulk primitives → **NO-GO,
  +14 Ir/block** (the per-offset accumulator bookkeeping cost more than the
  hot RMWs it coalesced). **Two independent NO-GOs confirm this 84-Ir pass is
  already efficiently compiled and is NOT a fruitful target.**
- **(ii) The `flush_class` + 8-pointer compaction + final push remainder —
  ~470 Ir** (R24-2 §4.5, derived as `571 − 84`, "non-isolable" — fused in one
  straight-line block with no workload-level separation point). R25-3's
  `FLUSH_N` sweep targeted the compaction's shape → **NO-GO** (every value
  either regressed gate 1 or triggered the gate-3 refill-thrash kill
  condition). This is the "third NO-GO in the region."

The ~470-Ir remainder is the run-descriptor's *theoretical* target: within it,
`flush_run` (`alloc_core_small_magazine.rs:586-695`) does, **per accepted
block**:

| # | Operation | Site (current source) | Per-block? |
|---|---|---|---|
| F1 | `Node::write_next(block_nn, next_ptr)` — store the intrusive `next` pointer into the freed block's FIRST WORD (payload write) | `alloc_core_small_magazine.rs:652` (batched `flush_run`); `alloc_core_small.rs:1816` (per-block `dealloc_small`) | **yes** |
| F2 | `bm.mark_free(off)` — `AllocBitmap` RMW (byte load + OR + store); the M2 double-free guard | `alloc_core_small_magazine.rs:653`; `alloc_core_small.rs:1818` | **yes** |
| F3 | `bt.set_head(class_idx, off)` — update the `BinTable` head to the last accepted block | `alloc_core_small_magazine.rs:665` | **no — already ONCE per run** (hoisted) |

So on the **free side**, a run-descriptor could in principle eliminate **F1**
(the payload store) and **F2** (the bitmap RMW) for run-blocks — but §3.3
shows F2 cannot be eliminated, leaving only F1.

### 1.2 The allocation-side mirror — where the run's real win lives

`drain_freelist_batch` (`alloc_core_small.rs:1342-1405`, the magazine-refill
mechanism) walks the intrusive free list. **Per popped block**:

| # | Operation | Site | Per-block? |
|---|---|---|---|
| A1 | `Node::read_next(block_nn)` — the DEPENDENT LOAD that walks the chain (each `next` lives in the previous block's body) | `alloc_core_small.rs:1377` | **yes** |
| A2 | `bm.mark_alloc(head_off)` — `AllocBitmap` RMW (byte load + AND + store) | `alloc_core_small.rs:1391` | **yes** |
| A3 | `Node::deref(segment, head_off)` — `(base, off) → *mut u8`, pure arithmetic | `alloc_core_small.rs:1366` | yes (trivial) |
| A4 | `bt.head` / `bt.set_head` / `inc_live` | `:1359, :1403` + `add_live` | **no — already hoisted** |

`drain_freelist_batch`'s own doc (`:1306-1321`) states A1 is irreducible:
"the dependent load that walks the intrusive chain… there is no way to hoist
it (each `next` lives in the previous block's body)." **This claim is true for
a SCATTERED free list and FALSE for a contiguous run** — a run's next block is
`first_offset + stride`, computable without touching the block body. **A1 is
the one genuinely new lever a run-descriptor offers, and none of the three
prior NO-GOs touched the allocation side.** But A2 (the bitmap RMW) is the same
"already-cheap, already-proved-net-negative-to-coalesce" class as F2.

### 1.3 The wall-clock gap this would close (R24-5)

R24-5 (`R24_5_COLD_ALLOC_FREE_SPLIT_GATE.md`) localized the cold-path gap:
**free-only Sefer 108.77 vs mimalloc 30.24 = 3.60×**, of which **overflow is
61.5%** (30 overflow events × 571 Ir at N=256). The gap is overwhelmingly in
the free half, and within the free half overwhelmingly in the overflow event.
**This is exactly the region three NO-GOs have failed to improve** — so any
design here must clear a high evidentiary bar before implementation, not
assume a win exists.

---

## 2. Design sketch

### 2.1 The run-descriptor

```text
// SKETCH — NOT applied. Illustrative only.
struct FreeRun {
    segment: *mut u8,      // the owning segment base (run is per-segment)
    class_idx: u8,         // all blocks are this one small class
    first_off: u32,        // segment offset of the lowest block in the run
    count: u16,            // number of contiguous blocks currently in the run
    // stride is implicit: block_size(class_idx) — recoverable from class_idx,
    // so it is NOT stored (saves a field; every block in a class shares it).
}
```

`stride` is **not stored** — it is `SizeClasses::block_size(class_idx)`,
recoverable from `class_idx`. The run is **per-segment-per-class**: `flush_class`
already groups a flush slice into same-segment runs
(`alloc_core_small_magazine.rs:532-569`), so a run-descriptor slots into that
existing grouping naturally.

### 2.2 Where it would live

A run is **transient, built during a batch/overflow-flush call and consumed
(or discarded) by the next refill of the same class from the same segment.**
Two placement options:

- **(a) Per-segment-per-class sidecar** — a small fixed array of `FreeRun`
  records attached to the segment header (or in the already-reserved second
  `BinTable` slot, `segment_header.rs` — the same slot R10-4 §4.2 proposed for
  its run-origin array). Bounded: at most a handful of runs per class per
  segment (a 4 MiB segment hosts at most `SEGMENT/block_size` blocks of one
  class; for 16 B that is ~262k blocks, but realistically a few runs).
- **(b) A single "active run" cache** per `AllocCore` (or per tcache class) —
  the most-recently-flushed contiguous run, checked first on refill. Cheaper,
  but loses runs on class/segment churn.

Both add a new metadata structure with its own capacity bound (§3.5).

### 2.3 The allocation-from-run fast path (the win)

On refill (`drain_freelist_batch` shape), if an intact run of class `c` in
`segment` has `count >= requested`:

```text
// SKETCH — replaces the read_next chain walk for run-blocks.
// Pop `requested` blocks arithmetically, no block-body touch.
let base = run.segment;
let bs = block_size(run.class_idx);
for k in 0..requested {
    // The highest-offset block is popped first (LIFO within the run,
    // matching the existing free-list LIFO discipline).
    let off = run.first_off + (run.count - 1 - k) as u32 * bs;
    out[k] = Node::deref(base, off as usize);   // A3, kept (cheap arithmetic)
    bm.mark_alloc(off);                          // A2, kept (M2 guard)
    // A1 (read_next) is GONE — computed arithmetically, not loaded from body.
}
run.count -= requested as u16;
if run.count == 0 { /* discard the run descriptor */ }
```

**This eliminates A1 (the dependent load) for every block popped from a run.**
That is the design's one real saving. A2 (the bitmap RMW) stays per-block
(§3.3). The run's `count` decrement is O(1) regardless of `requested`.

### 2.4 The free-into-run fast path (the conditional win)

On a batch flush (`flush_run` shape), if the flushed blocks form a contiguous
stride-regular run (detected by checking `off[i+1] - off[i] == block_size`
while iterating the slice), record a `FreeRun` instead of writing `write_next`
per block:

```text
// SKETCH — only when contiguity holds (the precondition §3.1 gates).
// F1 (write_next) is GONE for run-blocks; F2 (mark_free) is KEPT (§3.3).
for &ptr in run_slice {
    let off = (ptr - base) as u32;
    bm.mark_free(off);          // F2 kept — M2 double-free guard, per-block
    // F1 (write_next) is SKIPPED — the block body is never touched.
}
// Record the run: (base, class_idx, min_off, count) — ONE descriptor.
```

**This eliminates F1 (the payload store) for run-blocks** — but ONLY when the
flushed slice is offset-contiguous, which §3.1 shows the magazine-overflow
path generally is NOT.

### 2.5 Split / escape triggers (what fragments a run)

A run must materialize ordinary free-list nodes (fall back to the existing
`write_next` + `set_head` path) when:

1. **A foreign (cross-thread) free lands inside the run's offset range.**
   `reclaim_offset` (`alloc_core_small_reclaim.rs:92-197`) writes a `next`
   node and pushes onto `BinTable[class_idx]`. The run now has a "hole" — a
   materialized node at an offset the run would otherwise hand out
   arithmetically. The run must split into `[first_off, hole)` and
   `(hole, first_off + count*stride)`, or the hole's block must be excluded
   from the arithmetic pop.
2. **An out-of-order alloc from the middle.** The current free list is LIFO;
   a run popped LIFO consumes the tail down, staying contiguous. But a
   `pop_free` (single-block alloc, `alloc_core_small.rs:1196`) that targets a
   specific offset not at the run's tail would break contiguity. In practice
   the magazine refill (§2.3) pops the tail, so this is rare — but the design
   must handle it (split the run at the requested offset).
3. **A decommit/reclaim event on the segment.** `dec_live_and_maybe_decommit`
   (`alloc_core_small_pool.rs:78-113`) fires at `live_count == 0`; if the
   segment is released, the run descriptor is invalidated. If pooled, the run
   survives (the pool keeps free-lists intact).
4. **Run-descriptor capacity exhaustion.** If the sidecar's run cap is hit
   (§3.5), the overflow degrades to ordinary per-block free-list nodes — a
   silent fallback, not an error.

---

## 3. Correctness complexity inventory

### 3.1 THE precondition failure: magazine-overflow flushes are NOT offset-contiguous

**This is the single most load-bearing finding of the study.** The
run-descriptor's `(first_off, count, stride)` encoding requires the batch's
blocks to occupy a contiguous, stride-regular offset range. Tracing the
magazine's actual contents:

- `carve_batch` (`alloc_core_small.rs:1613-1724`) carves contiguous blocks:
  `off = aligned_start + i * block_size` (`:1710`). ✓ contiguous — but this is
  the ALLOC side, already arithmetic.
- The carved blocks enter the magazine via refill (`drain_freelist_batch`),
  are handed to the caller (magazine pop, LIFO), and later freed back
  (magazine push, LIFO, `heap_core_free.rs:712-743`).
- The magazine (`PerClass::slots`, `tcache.rs`) is a **LIFO stack in
  FREE-order**. `slots[0..count]` hold blocks in the order the caller freed
  them — **arbitrary offsets, not offset-sorted.**
- On overflow (`cnt == TCACHE_CAP`), `slots[0..FLUSH_N]` (the **oldest 8**
  freed blocks) are flushed (`heap_core_free.rs:762-778`). These 8 offsets are
  whatever the caller freed 8-frees-ago through 1-flush-ago — **in general
  NOT contiguous and NOT stride-regular.**

**Exception (the only shape that satisfies the precondition):** a workload
that frees a freshly-carved contiguous batch in allocation order. The
R24-2/R24-5 measurement benches (`dealloc_prealloc_only_*`) are exactly this
shape — pre-allocate a contiguous run via carve, then free sequentially. So
the measured 571-Ir overflow cost is the **BEST CASE** for a run-descriptor,
and even there three NO-GOs found no win. A real churn workload (free in
arbitrary order) produces **scattered** flush slices that a run-descriptor
cannot encode without an O(n log n) offset-sort per flush — which would
itself exceed the per-block savings (§4).

**Implication:** the magazine-overflow free path — the exact region that
motivated this study and that R24-5's 3.60× gap localizes — is **outside the
run-descriptor's reach.** Only `dealloc_batch` (R23-7) of a freshly-carved
batch is contiguous by construction, and that API has **no downstream
consumer** (R23-7: `alloc_batch`/`dealloc_batch` have exactly one in-tree call
chain, under the `batch-api` feature, NOT in `production`).

### 3.2 Fragmentation / escape — runs rarely survive to refill

For the alloc-side win (§2.3) to materialize, a run created at flush-time
must survive **intact** until the next refill of the same class from the same
segment. Three forces work against this:

- A **foreign free** into the run's range (§2.5 trigger 1) fragments it. Under
  `alloc-xthread`, cross-thread frees are routine; a run spanning offsets
  `[o, o+7*bs]` is fragile against a single remote free of any block in that
  range.
- The **magazine itself** re-fills from the free list on every
  `refill_magazine_slow` miss; if the run was created by a flush and the very
  next operation is a refill, the run is consumed (good). But if intervening
  per-block frees/allocs occur, the run coexists with ordinary `BinTable`
  nodes and the refill's `drain_freelist_batch` must consult BOTH structures
  (§3.4), adding per-refill branching that may erase the A1 saving.
- **Decommit** at `live_count == 0` discards the run (§2.5 trigger 3).

The honest expectation: runs survive to refill mainly in the
contiguous-batch-dealloc-then-realloc shape — again, the `dealloc_batch`
consumer, not the magazine.

### 3.3 THE double-free boundary: `mark_free` cannot be eliminated (F2 stays per-block)

`AllocBitmap::is_free` (`alloc_bitmap.rs:106-108`) is the **M2 exact O(1)
double-free guard**: bit = 1 means the block is on a free list. A block inside
an intact run has **no materialized free-list node** — so the bitmap is the
ONLY per-block record that it is free. If the run-descriptor skipped
`mark_free(off)` for run-blocks (to save F2), then a subsequent free of the
SAME block would read `is_free == false` (bit still 0) → proceed to
`write_next` → **silent double-free corruption**, exactly the M2 failure the
bitmap exists to prevent.

**Therefore F2 (`mark_free`) MUST stay per-block even for run-blocks.** This
collapses the free-side win to F1 alone (`write_next`, the payload store). F1
is a single unaligned word store into a block whose cache line is HOT (the
block was just freed into the magazine — its line is resident). This is the
**same "cheap hot-cache-line RMW" class R24-4 measured as a +14 Ir/block net
REGRESSION to coalesce** (R24-4 §root cause: "the bulk primitive's per-offset
bookkeeping costs more in-context than the HOT-CACHE-LINE RMWs it
coalesces"). There is no evidence F1's elimination would fare better than
F2's did — both are single hot stores.

Symmetric on the alloc side: A2 (`mark_alloc`) must stay per-block for the
same M2 reason (a run-block handed to the caller must have its bit cleared, or
a free of it is not detected as a double-free). So the run-descriptor
eliminates **only A1 (the dependent load) on the alloc side and only F1 (the
payload store) on the free side** — both single hot operations.

### 3.4 Coexistence with the existing `BinTable` nodes

A segment may simultaneously hold (a) intact runs (in the run sidecar) and
(b) ordinary `BinTable[class_idx]` free-list nodes (for blocks not in any run,
or materialized by a split). The refill path (`drain_freelist_batch`) and
single-block `pop_free` must consult BOTH:

- `BinTable::head(class_idx)` first (existing path); if empty, check the run
  sidecar for an intact run of `class_idx` in this segment.
- Or: a run, when created, becomes the head of a *hybrid* structure — the run
  points at the `BinTable` head as its "tail," so popping past the run's last
  block falls through to the ordinary free list. This mirrors how `flush_run`
  links its first block to the captured `old_head` (`:609-610, :647-651`).

Either design adds per-refill branching (a run-present check) on a path that
is currently a single `head` read + null test. The check is cheap (one sidecar
lookup), but it runs on EVERY refill, and R24-8 measured the staging-array
zero-init cost of a similar structure (STAGE_CAP) as material — metadata
structures on the refill path are not free.

### 3.5 Run-descriptor capacity and overflow

A per-segment sidecar can hold a bounded number of runs. When the cap is hit
(a segment with many small fragmented runs), the overflow degrades to ordinary
per-block `BinTable` nodes — a silent correctness-preserving fallback, but one
that means the optimization is *intermittently inactive* under fragmentation,
making its measured benefit workload-dependent and hard to predict (the same
"benefit depends on workload shape" fragility R24-3/R24-4 hit).

### 3.6 Mixed segments / classes within one logical batch

`flush_class` already handles this: it groups the slice into same-segment runs
(`:532-569`). A run-descriptor is per-segment-per-class, so mixed segments
naturally produce multiple single-segment runs, and mixed classes are never
in one flush slice (`flush_class` takes a single `class_idx`). **No new
complexity here** — the existing grouping is reused.

### 3.7 `live_count` / decommit interaction

`live_count` (the owner-only live-block counter, `segment_header.rs:373`) and
the decommit-at-zero path (`dec_live_batch_and_maybe_decommit`,
`alloc_core_small_pool.rs:137-159`) are **orthogonal** to the free-LIST
representation. A run is just a different encoding of "these blocks are free";
the blocks must STILL decrement `live_count` when freed (the batched
`sub_live(k)` is unaffected) and the decommit reset (bump → payload_start,
bitmap zeroed) must STILL invalidate any run descriptor for the segment.
**The run-descriptor adds one more piece of state the decommit-reset must
clear** — a correctness obligation, not automatically handled.

---

## 4. Isolated victim judge (DESIGN, not implementation)

A future task could implement this judge directly from this spec. It isolates
the run-descriptor's win from ordinary free-list overhead by forcing the ONE
shape the design can help (contiguous batch) and measuring the alloc-side A1
elimination specifically.

### 4.1 Bench arms (Ir, `npm run iai`-shaped, `--features production batch-api`)

- **`run_encoded_dealloc_batch_then_refill_16b_n256`**: allocate 256
  contiguous 16 B blocks via `alloc_batch` (guaranteed contiguous by
  `carve_batch`), free them via `dealloc_batch` (the run is created here),
  THEN immediately refill-allocate 256 blocks of the same class (the run is
  consumed arithmetically here). Timed region = the dealloc_batch + the
  refill. **Shared-prefix subtraction** against
  `run_encoded_refill_only_16b_n256` (same 256-block refill from an ordinary
  scattered free list, no run) isolates the run's alloc-side saving.
- **`run_encoded_refill_only_16b_n256`**: the control — 256 blocks freed
  ONE-AT-A-TIME (scattered, no run forms), then refilled. Measures the
  ordinary `read_next` chain cost.
- **Reference arms**: the existing `dealloc_free_only_1088_16b_n{256,1024}`
  (R25-3, kept infra) measure the magazine-overflow free path UNCHANGED —
  they must be byte-identical before/after, confirming the run-descriptor
  does not touch the (non-contiguous) magazine path.

### 4.2 What the judge would establish

- **If `dealloc_batch_then_refill` − `refill_only` shows a material Ir drop**
  (e.g. > 5%, beyond this project's treated-as-noise band): the alloc-side A1
  elimination is a real win FOR THE CONTIGUOUS-BATCH SHAPE. This is necessary
  but NOT sufficient for implementation — it must be paired with a real
  consumer (§5 trigger a).
- **If the reference magazine-overflow arms are NOT byte-identical**: the
  implementation leaked into the non-contiguous path (a correctness/Scope
  bug), regardless of the measured win.
- **A counterfactual the judge must include**: a "scattered dealloc_batch"
  arm (free the 256 blocks in random order, so no run forms) must measure
  IDENTICALLY to `refill_only` — confirming the run-descriptor's benefit is
  contiguity-gated, not a free win.

### 4.3 Why the judge is NOT run now

The trigger (§5) is unmet: no contiguous-batch consumer exists, so the judge
would measure a workload no production caller exercises. Building it now would
repeat R24-3's "measure a mechanism production never runs" Heisenberg risk
without a consumer to make the result load-bearing.

---

## 5. Verdict — CONDITIONAL-GO, two independent triggers (neither met)

### 5.1 The verdict

**CONDITIONAL-GO — design is sound, but implement ONLY if BOTH triggers fire.**
Do NOT start implementation now. This matches the `[D]`-tier "implement only
if X materializes" convention (R17-10, R11-7, R14-7, R10-4).

### 5.2 The two triggers (BOTH required)

- **(a) A real contiguous-batch consumer emerges.** R23-7's `dealloc_batch`
  (currently P2, `batch-api` feature, NOT in `production`, no in-tree
  production caller) is promoted because a downstream project adopts batch-
  shaped allocation OR an internal consumer emerges (R23-7's three falsifi-
  ability triggers). This is the ONLY shape where the run-descriptor's
  contiguity precondition holds by construction (§3.1). **The magazine-
  overflow free path — the region that motivated this study — is explicitly
  OUTSIDE the conditional:** its flushes are not offset-contiguous, so no
  amount of implementation effort makes a run-descriptor encode them.
- **(b) A Stage-1 Ir measurement on THAT consumer confirms the alloc-side A1
  (`read_next` dependent-load chain) is the dominant remaining cost** in its
  refill path — not the free-side costs (F1/F2) three NO-GOs already proved
  un-optimizable, and not something else entirely. The judge in §4 is the
  instrument; run it ONLY after (a) fires.

### 5.3 Why CONDITIONAL-GO and not NO-GO-on-paper

The design is **not** blocked by a fundamental correctness flaw (contrast
R22-16's whole-segment-remap base-address blocker, which was a hard NO-GO on
paper). Arithmetic runs are correct (mimalloc ships them); the M2 double-free
guard is preservable (§3.3); the liveness/decommit accounting is orthogonal
(§3.7). The blocker is **economic and workload-shaped**, not logical: the
mechanism's benefit is confined to a contiguous-batch shape that has no
consumer today, and the free-side region it was motivated by does not satisfy
its own precondition. That is a "no victim exists yet" CONDITIONAL (matching
R11-7's framing: "NO-GO now; kept as a reusable CONDITIONAL-GO starting point
IF a real workload materializes"), not a "the design is wrong" NO-GO.

### 5.4 What implementation, if ever green-lit, must NOT do

- Must NOT touch the magazine-overflow free path (`heap_core_free.rs:744-816`,
  `flush_run`) — it is non-contiguous (§3.1) and 3× NO-GO.
- Must NOT eliminate `mark_free`/`mark_alloc` (F2/A2) — M2 guard, per-block
  (§3.3).
- Must be feature-gated (a new `run-encoded-free` implying `batch-api`), NOT
  in `production`, scoped to `dealloc_batch`-produced runs only — mirroring
  R10-4's `wide-class-align` gating discipline.
- Must include the §4.2 counterfactual (scattered dealloc_batch measures
  identically to the control) as a non-vacuous test.

---

## 6. Caveats

- **Single analysis host, no measurement performed.** The cost analysis (§1)
  cites R24-2's MEASURED 571/84/~470 Ir decomposition (not re-measured here);
  the run-descriptor's own win is analytical instruction-count reasoning
  (mirroring R10-4 §5 / R9-5 §8). §4's judge is the gate that would turn the
  A1-elimination claim into a measured number — it is NOT run (trigger unmet).
- **The contiguity finding (§3.1) is the load-bearing conclusion and was
  derived by reading the magazine mechanism, not assumed.** `PerClass::slots`
  is LIFO in free-order; the overflow flushes `slots[0..FLUSH_N]` (oldest 8);
  these are arbitrary offsets. Verified against `heap_core_free.rs:762-778`
  and `tcache.rs`'s `PerClass` this session.
- **The "mark_free cannot be eliminated" finding (§3.3) is the load-bearing
  correctness constraint.** A run-block with no materialized node has only the
  bitmap as its free-state record; eliding `mark_free` reopens the M2
  double-free hole the bitmap (Phase 13.4a) was built to close. This collapses
  the free-side win to F1 alone — the same hot-store class R24-4 proved
  net-negative.
- **No `src/`, `Cargo.toml`, `tests/`, or `benches/` file is modified.** All
  code blocks are illustrative `// SKETCH` fragments, not applied.
- **This is explicitly the LOWEST-priority task in the Round 25 queue** (P3,
  conditional), following three consecutive NO-GOs in the exact region it
  targets. A rigorous but appropriately time-boxed design doc is the complete,
  sufficient deliverable — no prototype is built.
