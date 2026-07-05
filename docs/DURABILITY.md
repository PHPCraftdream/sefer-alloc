# DURABILITY — ultra-long-run counter inventory

The two failure modes that only appear on ultra-long (weeks/months, thread-per-
request, hot-segment) runs are:

- **(a) Unbounded growth / leaks** — caught by the soak tests
  (`examples/soak_xthread.rs`, `examples/rss_probe.rs`) and by `AllocStats`:
  watch `segments_reserved_total − segments_released_total` (net live segments)
  and `ring_overflows` (bounded cross-thread-free leak). Not the subject of this
  doc.
- **(b) Counter wrap** — a monotonic/wrapping/saturating counter or cursor
  reaching its width boundary and either corrupting FIFO order, re-colliding a
  generation/ABA tag, or silently truncating. **This doc is the authoritative
  inventory of every such counter.**

Honest framing: after **W7a** (widen `HeapSlot::generation` → `AtomicU64`;
repack `TaggedPtr` to `index:16 | tag:48`) and **W7b** (ring cursor wrap made
explicit + pinned), **none of these is a live bug today**. The purpose of this
doc is to make long-run robustness *auditable* and *future-proof* — and to
enforce the rule in the last section.

## Master table

| counter | file:line | width | class | boundary reachable? (arithmetic) | verdict | covered by |
|---|---|---|---|---|---|---|
| `RemoteFreeRing::head` / `tail` | `src/alloc_core/remote_free_ring.rs:345`,`350` | `u32` (per segment) | monotonic wrapping | **YES** — 2^32 cross-thread frees on one hot long-lived segment | wrap-safe by design: occupancy `tail.wrapping_sub(head)`, index `i % RING_CAP`, `RING_CAP` power-of-two so `2^32 % RING_CAP == 0` (continuous across wrap) | `tests/regression_ring_cursor_wrap.rs` (W7b) + const-assert `remote_free_ring.rs:167` |
| `RemoteFreeRing::overflow` | `src/alloc_core/remote_free_ring.rs:356` | `u32` (per segment) | wrapping (diagnostic) | 2^32 overflow events on one segment | not correctness-load-bearing (diagnostic only; a wrap loses an overflow *count*, never a block) | `tests/regression_ring_overflow_counter.rs` |
| `DBG_RING_OVERFLOW` | `src/alloc_core/remote_free_ring.rs:138` | `AtomicU64` (process-wide) | wrapping (diagnostic) | 2^64 — unreachable | not correctness-load-bearing | `tests/regression_ring_overflow_counter.rs` |
| `HeapSlot::generation` | `src/registry/heap_slot.rs:102` | `AtomicU64` (was `u32`) | monotonic | 2^64 recycles (thread-deaths) — unreachable (was 2^32, reachable over weeks) | **widened (W7a)**. NOTE: consumed only by the `new_gen == 1` first-materialise gate — there is **no cached-generation compare**; the stale-TLS hazard is guarded by the `TORN` sentinel (`global::tls_heap`), so the old `u32` wrap was defence-in-depth, not a live bug | `tests/regression_counter_wrap.rs::generation_crosses_u32_boundary_as_u64` |
| `TaggedPtr` tag (`free_slots` ABA) | `src/registry/tagged_ptr.rs:77` (`INDEX_BITS=16`) | 48-bit tag in a `u64` (was 32) | monotonic wrapping | 2^48 ≈ 2.8×10¹⁴ pops ≈ **89 years @ 100k pops/s** — effectively unreachable | **repacked (W7a)** `index:16 | tag:48`; `MAX_HEAPS=4096` fits 16 bits with the `0xFFFF` empty sentinel above it | `tests/regression_counter_wrap.rs` (tag-wrap counterfactual) + const-assert `tagged_ptr.rs:90` (`MAX_HEAPS <= INDEX_MASK`) |
| `abandoned_segs` tag | `src/registry/bootstrap.rs:181` (`ABANDON_TAG_BITS = ABANDON_SEG_SHIFT = 22`) | 22-bit tag in the low `SEGMENT`-alignment bits of a `u64` base | monotonic wrapping | 2^22 ≈ 4.2M pushes — reachable in principle | **dead-path**: the abandoned-segments stack is unused on production paths since Phase 12.5 (cross-thread free routes through `RemoteFreeRing`, not abandon/adopt). Documented residual: any future reactivation must widen this tag or accept the 4.2M-push ABA window. Reactivation guidance already carries a ⚠️ (`heap_registry.rs` ~245–275) | none (dead path) — reactivation MUST add a boundary test |
| `large_cache_seq` / `CachedLarge::seq` | `src/alloc_core/alloc_core.rs:292`,`185` | `u64` | monotonic wrapping (`wrapping_add`) | 2^64 large-cache deposits — unreachable | bounded-by-width (FIFO-oldest picked by `min_by_key(seq)`; a 2^64 wrap is not reachable in any process) | `tests/regression_large_cache_multi_size_cycle.rs` (FIFO order) |
| `SegmentHeader::live_count` | `src/alloc_core/segment_header.rs:285` | `u32`, **saturating** (`add_live`/`sub_live` sat) | saturating | blocks-per-segment = `SEGMENT/MIN_BLOCK` = 4 MiB/16 = 262144 ≪ 2^32 — cannot overflow | bounded (saturating is pure defence-in-depth) | `tests/regression_carve_batch.rs`, `regression_batch_flush.rs` |
| `SegmentHeader` `owner_gen` (packed in `owner_state`) | `src/alloc_core/segment_header.rs:86` (`OWNER_GEN_SHIFT=32`, mask `u32::MAX`) | 32-bit generation in bits [32..63] of the `owner_state` `u64` | monotonic wrapping | 2^32 abandon→adopt cycles on ONE segment — reachable only via the abandon/adopt path (dead since Phase 12.5, same as `abandoned_segs`) | dead-path (M9 adoption CAS is unreachable on production paths; `tests/loom_registry.rs` models it as an explicitly-unreachable protocol). Residual documented for any reactivation | `tests/loom_registry.rs` (models the CAS; honesty note in-file) |
| `SegmentTable::count` | `src/alloc_core/segment_table.rs:154` | `u32` | monotonic (high-water) | capped at `MAX_SEGMENTS = 1024` — cannot wrap | bounded | `tests/segment_table_o1.rs` |
| `SegmentTable::tombstones` | `src/alloc_core/segment_table.rs:196` | `u32` | bounded/reset | reset to 0 by the W2 rebuild when `> HASH_CAPACITY/4` (= 512); population never exceeds `HASH_CAPACITY` = 2048 | bounded | `tests/regression_segment_table_tombstone_rebuild.rs` |
| `SegmentHeader::bump` | `src/alloc_core/segment_header.rs:197` | `usize` | monotonic | bounded by `SEGMENT` (4 MiB) — never wraps | bounded | carve/refill tests (`regression_bump_direct_refill.rs`) |
| `Registry::count` | `src/registry/bootstrap.rs:267` | `AtomicU32` | monotonic (high-water) | capped at `MAX_HEAPS = 4096` | bounded | `tests/regression_counter_wrap.rs` (claim/recycle) |
| `os::SEGMENTS_RESERVED_TOTAL` / `RELEASED_TOTAL` | `src/alloc_core/os.rs:52`,`57` | `AtomicU64` | monotonic (`fetch_add`) | 2^64 — unreachable | bounded-by-width (diagnostic; net = reserved − released) | soak tests, `regression_*_no_leak.rs` |
| `AllocStats` fields (`tcache_hits`, `ring_overflows`, `large_cache_hits`, `decommit_calls`, `large_xthread_reclaimed`, `segments_reserved/released_total`, `heaps_claimed_high_water`) | `src/global/alloc_stats.rs:50`–128 | `u64` | monotonic/saturating (diagnostic) | 2^64 — unreachable | not correctness-load-bearing | `tests/regression_percounter_perheap_aggregation.rs`, `regression_w3_stats_aliasing_miri.rs` |

## Reachability arithmetic (the two genuinely-reachable ones)

- **Ring cursors (`head`/`tail`, `u32`, per segment).** Reachable = `2^32`
  cross-thread frees against a *single* hot, long-lived segment. At, say, 10M
  cross-thread frees/sec into one segment that would take ~430 s of sustained
  single-segment churn — plausible on a long run. This is why the wrap path is
  *tested*, not just argued: `regression_ring_cursor_wrap.rs` presets the
  cursors to the `u32::MAX` boundary (via the `dbg_set_cursors` seam) and drives
  the real ring across it. Safety rests on `tail.wrapping_sub(head)` occupancy +
  power-of-two `RING_CAP` (`2^32 % RING_CAP == 0`, so the slot index sequence is
  continuous across the wrap).
- **`HeapSlot::generation` (was `u32`).** Reachable = `2^32` thread deaths
  (`FREE → LIVE → FREE` recycles) of one slot — reachable on a thread-per-request
  server over weeks/months. **Now moot at `u64`** (2^64 ≈ 10^19 recycles). The
  wrap boundary is *removed*, not merely tested: `regression_counter_wrap.rs`
  presets generation to `u32::MAX − 1`, forces two recycle→reclaims, and asserts
  the value crosses `> u32::MAX` as a `u64` with no truncation.

## THE RULE — adding a new monotonic/wrapping counter

A new monotonic / wrapping / saturating counter or cursor is added **only** with
BOTH:

1. **A row in the master table above** — width, class, the reachability
   arithmetic, the verdict.
2. **A boundary-crossing test OR a compile-time bound assert:**
   - *Boundary test* (for genuinely-reachable or widened counters): preset the
     counter near its limit and drive it across. Templates:
     - `tests/regression_ring_cursor_wrap.rs` (W7b) — preset cursors to
       `u32::MAX`, drive the real ring across, with a non-vacuity counterfactual
       (`t - h` instead of `t.wrapping_sub(h)` fails; non-power-of-two `RING_CAP`
       fails to compile).
     - `tests/regression_counter_wrap.rs` (W7a) — preset generation near `2^32`
       and the tag at `2^48 − 1`, cross the boundary, assert no truncation. Each
       assertion fails if the widening is reverted (non-vacuous).
   - *Compile-time bound assert* (for structurally-bounded counters): pin the
     bound so a future config bump fails to compile rather than silently wrapping.
     Templates: `const _: () = assert!(RING_CAP.is_power_of_two(), …)`
     (`remote_free_ring.rs:167`); `const _: () = assert!(MAX_HEAPS <= INDEX_MASK, …)`
     (`tagged_ptr.rs:90`).

## Widening must be Ir-neutral (W1 judge)

Before widening any counter, **prove zero hot-path cost**. Precedent: the W7a
widenings (`generation` → `AtomicU64`, `TaggedPtr` repack) were judged
Ir-neutral by the W1 instruction-retired judge — **−4 Ir on the cold path,
byte-identical hot path** (both fields live on the cold registry-protocol path,
off every hot alloc/dealloc path; `pack`/`unpack` are the same two shifts/masks
on different constants). A widening that moved the hot-path Ir is not accepted;
narrow the change or move the counter off the hot path first.

---

*Cross-refs: [ARCHITECTURE.md](ARCHITECTURE.md) §10 (docs index),
`tests/regression_ring_cursor_wrap.rs`, `tests/regression_counter_wrap.rs`.*
