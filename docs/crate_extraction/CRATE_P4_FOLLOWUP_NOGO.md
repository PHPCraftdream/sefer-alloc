# CRATE-P4-followup (#187) — ring-mpsc in-tree swap: verified NO-GO

**Date:** 2026-07-17. **Task:** swap the two shipping in-tree cross-thread-free
rings (`RemoteFreeRing`, `HeapOverflow`) onto the extracted `crates/ring-mpsc`,
retiring their in-tree loom models. **Verdict: NO-GO on BOTH tiers** — zero code
changed. This was re-investigated (not merely inherited from the #174 escape
hatch) and the incompatibilities were verified against source; forcing the swap
would degrade a cache-line-layout perf fix and risk a permanent-wedge hazard in
the most safety-critical path in the codebase for **no runtime benefit** (the
swap is pure dedup). The task, the #174 precedent, and the orchestrator brief all
sanctioned a reasoned NO-GO. **All 7 in-tree ring/dirty loom models are KEPT**
(the shipping code is unchanged, so its coverage must stay — the #174 lesson).

## Tier A — `src/alloc_core/remote_free_ring.rs` (raw tier): NO-GO

Structural layout incompatibility with `ring-mpsc`'s `over_raw` / `RawStore`
(verified against both sources):

| | `RemoteFreeRing` (shipping) | `ring-mpsc` `RawStore` |
|---|---|---|
| cursors | `head`/`tail` = **`AtomicU32`** | `head()`/`tail()` = **`AtomicUsize`** (fixed) |
| extra word | **`overflow: AtomicU32`** (discarded-push count) | none |
| cursor block | **`CURSOR_BLOCK = 128`** (two cache lines; PERF-PASS-4/G8/#52 producer/consumer ping-pong fix — `head`@0 consumer-only, `tail`@64 producer) | cursors adjacent, no padding |
| entry packing | base `u32`; **plus a hardened `[gen:8\|class:6\|off16:18]` scheme** threading X7's per-granule generation stamp | one `RingEntry` type, no third scheme |

`RemoteFreeRing::FOOTPRINT = CURSOR_BLOCK(128) + RING_CAP*4` and `remote_ring_off()`
are wired into `segment_header_layout.rs`'s `small_meta_end()` (which every other
metadata region and `primordial_registry_off()` chains off of), pinned by
compile-time `const _: () = assert!(...)` byte-offset checks and layout-sensitive
tests (`regression_ring_cursor_wrap`, `phase13_drain_reclaim_layout_class`,
`regression_gen_table_layout`), and reached from 20+ call sites via
`Node::atomic_u32_at`. Swapping would require either (a) `RemoteFreeRing` adopting
`usize` cursors + dropping the `overflow` word + collapsing the 128-byte
separation (breaking the PERF-PASS-4 cache-line fix and every offset assert), or
(b) generalizing `RawStore` to a caller-configurable cursor layout/width AND
adding a hardened-packing `RingEntry` — a far larger, riskier crate change than
an additive extension. Out of proportion to the (zero-runtime-benefit) dedup.

## Tier B — `src/registry/heap_overflow.rs` (two-tier inline+sidecar): NO-GO

Two blockers:
1. **Storage straddle.** One `head`/`tail` pair indexes across an inline
   `[_; INLINE_CAP]` array AND a lazily-mmap'd `HeapOverflowSidecar`
   (`index < INLINE_CAP → inline, else → sidecar`). `ring-mpsc`'s `Owned`/`Raw`
   are single-region; expressing this needs an additive `Storage` (index→slot)
   trait on the crate (mirroring tagged-index-stack's `Links`). Feasible in
   principle, but —
2. **Wedge-hazard ordering lives inside `push`'s loop.** The module doc's
   "wedge hazard" fix requires checking `t % HEAP_OVERFLOW_CAP >= INLINE_CAP` and
   materialising the sidecar (`bootstrap::ensure_overflow_sidecar`, a bespoke
   CAS(null→SENTINEL)→reserve→publish→rollback-on-OOM protocol) **before** the
   tail CAS, **inside** the reservation loop. `MpscRing::push` owns that loop
   opaquely; exposing a fallible per-index "can this reservation be honoured"
   hook from inside the crate's loop is a protocol inversion the crate doesn't
   have, and getting it subtly wrong reintroduces exactly the
   permanent-wedge / silently-disabled-ring hazard the doc warns is *worse* than
   the existing bounded-loss behaviour. `HeapOverflow` is also embedded as a
   plain `overflow: HeapOverflow` field inside the `'static` registry `HeapSlot`,
   materialised by that bootstrap protocol — not the crate's `over_raw`/`view_raw`
   init model.

## Consequence for loom coverage

Neither tier swapped → **all 7 in-tree models KEPT**: `loom_remote_ring`,
`loom_remote_ring_drain_guard`, `loom_heap_overflow`,
`loom_heap_overflow_drain_guard`, `loom_overflow_first_retry`,
`loom_dirty_publish`, `loom_dirty_multi_segment`. The crate's own
`loom_ring_mpsc` suite stays as additive real-type coverage of the *extracted*
protocol (as it has since #174). `scripts/loom.mjs` already documents this exact
"additive, not a replacement" rationale.

## Baseline (nothing changed; confirms the tree is green)

loom 38/38 (crate 11 + 7 in-tree models 27, all counterfactuals fire); miri
`reclaim_offset_unit` + `regression_ring_drain_guard_miri` PASS; tsan remote
fan-in 10/10 (0 races); `cargo test --features production` 356/0;
`--all-features` 499/0.

## If someone wants to revisit

The only path that clears the bar: FIRST add an additive, layout-configurable
`RawStore` (cursor width + offsets + optional side-counter) and a `Storage`
index→slot trait to `ring-mpsc`, THEN wrap the hardened generation-stamping and
the wedge-hazard sidecar-before-CAS ordering sefer-side. That is a dedicated
crate-design task, not a swap — and it buys dedup only, never runtime
performance. Deferred until there is external demand for the generalized ring.
