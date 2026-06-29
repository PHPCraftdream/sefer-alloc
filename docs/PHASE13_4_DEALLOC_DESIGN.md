# Phase 13.4 — dealloc clean rewrite: O(1) double-free guard + two-list

Design spec (written before implementation; implementation is driven by a
sub-agent following this document). Closes the O(N²) regression (formerly #41)
and delivers two-list (13.4).

## 0. Problem (confirmed by counterfactual)

`AllocCore::dealloc_small` (src/alloc_core/alloc_core.rs) on every own-thread
free calls `free_list_contains` — **O(free-list length)** walk (M2 double-free
guard). During the bench deallocation phase (1024 blocks of one class into a
single segment) the free-list grows 0→1024 → **O(N²)** ≈ 524k dereferences with
cache-misses = ~1.9 ms (vs mimalloc ~11 µs). Proven: `free_list_contains →
return false` ⇒ 16B churn **1.9 ms → 16.5 µs** (~115×). The same inline-walk
sits in `AllocCore::reclaim_offset` (cross-thread reclaim).

`free_list_contains` is a Phase-8 placeholder (its comment: "Phase 9 will
replace with a cheap cookie-guard"). Phase 12.1 wired the malloc face to
`dealloc_small` with this guard → reintroduction of O(N²).

## 1. Solution: O(1) exact double-free guard via per-segment alloc-bitmap

**Why bitmap, not canary/encoding:** M2 requires EXACT "double-free = no-op,
never corruption". An in-block canary gives false positives (user data ==
canary) → not exact. Unprotected double-free creates a self-loop in free-list →
double-issuance of a block → corruption. Bitmap — exact, O(1), standard (TLSF
et al.).

### 1.1 Structure `AllocBitmap`

New metadata area in EVERY small/primordial segment: 1 bit per MIN_BLOCK-slot
of the segment.

- `FOOTPRINT = SEGMENT / MIN_BLOCK / 8` bytes. For 4 MiB/16 = 32768 bytes =
  32 KiB = 8 pages. **Compute from constants**, do not hardcode.
- Bit semantics: `1` = block is FREE (resides in some free-list of this
  segment: `free` OR `local_free`); `0` = allocated / not-a-block-start.
- Index: `bit_index = (ptr - base) >> MIN_BLOCK_SHIFT`. Block starts are always
  MIN_BLOCK-aligned (carve aligns bump to block_size ≥ MIN_BLOCK, and
  block_size is a multiple of MIN_BLOCK) → the bit is unique per block. We
  cover the ENTIRE segment (including metadata) — metadata bits are simply never
  touched (no block starts there); this eliminates payload-start subtraction
  arithmetic.
- Initialization: all zeros (everything "allocated/not-a-block"). `init_in_place`
  via `Node::write_u8` (like PageMap/BinTable).
- API (all O(1), via `node` seam, WITHOUT atomics — single-writer: the segment
  is written only by its owner; cross-thread free goes through the ring, drained
  by the owner):
  - `is_free(off: u32) -> bool` — test the bit.
  - `mark_free(off: u32)` — set the bit (called on push to free-list).
  - `mark_alloc(off: u32)` — clear the bit (called on block issuance).
- File: `src/alloc_core/alloc_bitmap.rs` (single export `AllocBitmap`), like
  PageMap/BinTable. `mod.rs` — reexport only.

### 1.2 Layout (`segment_header::Layout`)

Insert bitmap into the metadata chain. CAREFUL with ordering (ring and
registry-offset depend on predecessors). Proposed order: header → page_map →
bin_table → **alloc_bitmap** → remote_ring → (primordial: registry). Update:
- `Layout::alloc_bitmap_off()` (new) = `align_up_const(bin_table_off() +
  BinTable::FOOTPRINT, 8)` (or 2× if two-list expands BinTable — see §2;
  account for this IMMEDIATELY to avoid shifting layout twice).
- `Layout::remote_ring_off()` = after bitmap.
- `SegmentMeta::alloc_bitmap()` view.
- Compile-time asserts (`small_meta_end + PAGE <= SEGMENT`,
  `primordial_meta_end + PAGE <= SEGMENT`) — must hold (+8 pages out of
  1024). Bootstrap (`bootstrap.rs`) — carve+init bitmap, update `meta_pages`.

### 1.3 Integration into alloc/dealloc

- `dealloc_small(base, ptr, class)`: replace `free_list_contains` with
  `bitmap.is_free(off)` → if true, no-op return (double-free, M2). Otherwise
  `bitmap.mark_free(off)` + push to free-list.
- `pop_free` / block issuance: `bitmap.mark_alloc(off)` before return.
- `carve_block` (fresh block): bit is already 0 (init) — issued as allocated;
  as a precaution, do NOT touch (it is 0). Refill-blocks are pushed via
  `dealloc_small` → `mark_free` works correctly.
- `reclaim_offset` (cross-thread reclaim): same guard — replace inline-walk with
  `is_free`/`mark_free`. The owner is the sole bitmap writer (reclaim runs on
  the owner), atomics are not needed.
- **Remove** `free_list_contains` (and the inline-copy walk in `reclaim_offset`).

## 2. two-list (`free` + `local_free`) — locality layer (13.4)

mimalloc: own-thread free pushes to `local_free`; alloc pops from `free`; when
`free` is exhausted, transplant `local_free`→`free` (collect). Reduces branching
and separates own/remote queues. cross-thread (ring) — a third queue, already
exists.

- BinTable: second array of u32 heads `local_free` (FOOTPRINT × 2 = 320 B).
  Account for in layout §1.2 IMMEDIATELY.
- own-thread `dealloc_small`: `mark_free` + push to `local_free` (NOT to `free`).
- `pop_free`: if `free` is empty — `free_head = local_free_head; local_free_head =
  NULL` (O(1) transplant, order does not matter), then pop from `free`.
- double-free guard (bitmap) covers both lists equally — `is_free` is true
  if the block is in either list. Therefore two-list does not complicate the
  guard.

**Honesty (plan §3.4):** accept two-list ONLY if the bench shows improvement.
Therefore implement in TWO commits:
- **13.4a (bitmap guard)** — by itself kills the regression (expected ~16 µs),
  M2-exact. Benchmark.
- **13.4b (two-list)** — on top; measure the delta; keep if it helps.

Compute bitmap layout immediately accounting for doubled BinTable (so that 13.4b
does not shift metadata a second time).

## 3. Regression gate (mandatory)

Test `tests/dealloc_sublinear.rs` (or in an existing one): free N and 2N blocks
of one class, measure work (node-reads counter OR rough time) — growth
must be ~linear, not quadratic. Counterfactual: FAILS on the old O(N) walk.
Without the gate, O(N²) will silently return.

Additionally: ensure there is a unit test for M2 double-free (a block freed
twice is not issued to two callers / free-list does not cycle). If missing —
add; it must pass with both old and new guard (invariant preserved).

## 4. Verification (zero-trust, by hand)

- Full suite green: `alloc-core`, `alloc-global`, `alloc-global alloc-xthread`.
- race_repro / race_norecycle / global_alloc_mt ×5 — no flakes.
- `clippy --all-targets` (same features) — 0 new warnings.
- Bench `global_alloc` 16/64/256/1024B: 16B returns to ~16 µs (from 1.9 ms).
- miri on bitmap invariant (small bounded), if cheap.
- Commit at phase boundary (13.4a separately, 13.4b separately).

## 5. Out of scope (separate tasks)

- **Per-class bump cursors** (true page-dedication, like mimalloc): would
  eliminate §13 at its root (page_map would become reliable → carry-class in
  ring not needed, #40 dissolves). Larger and riskier — a separate future task,
  NOT here.
- #40 (latent §13 on drain) — after/in light of 13.4 (same dealloc/drain code).
