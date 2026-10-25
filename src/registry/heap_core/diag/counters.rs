//! Bench/diagnostic static counters of the `HeapCore` free path
//! (R1-01 magazine byte-budget investigation). Mirrors the B4_* bank in
//! `alloc_core::alloc_core::counters`: statics are defined here, incremented
//! at their call sites (feature-gated there), and re-exported via the parent
//! `diag/mod.rs` for cross-module consumers and `dbg_*` exposure.

use core::sync::atomic::AtomicU64;

/// R1-01 (P1-box / item 164) path-activation oracle: number of frees that
/// hit the `cnt == cap < TCACHE_CAP` branch of the small-free tcache route —
/// i.e. frees where the per-refill byte budget (`FREE_PARK_CAP[c]`) is
/// already exhausted and the O(1) swap-top policy evicts the magazine's top
/// block to the substrate instead of parking the current block.
///
/// Process-global (all heaps/threads), monotone within a run. Used by
/// `tests/support/r8_global_box_witness.rs::r1_01_cap_frame` as an oracle
/// that the witness scenario actually exercised this branch (the scenario's
/// earlier phase performs at most 8 pushes, so the branch cannot fire before
/// the 9th drop).
///
/// Diagnostic only: `AtomicU64` + `Relaxed` (no ordering guarantees needed —
/// the counter is never used for synchronization, only observed for
/// "did the path execute at all"). Gated behind `bench-internals` so the
/// release build pays nothing; the increment lives under the same gate at
/// the call site in `dealloc_own_base.rs`.
#[cfg(feature = "bench-internals")]
pub(crate) static R1_01_CAP_ROUTE_FREES: AtomicU64 = AtomicU64::new(0);

// ── B4 depot provenance (chunk 1, task b4-provenance) ────────────────────
//
// Flush/refill path-activation counters for the depot hypothesis (Bonwick
// 2001) on the own-free ping-pong working set: when the magazine overflows
// (`flush_class_internal` on the FLUSH_N half-flush), a depot would absorb
// the batch instead of writing `next` into block bodies and re-validating
// them at refill. These statics count, per flush CALL SITE (the provenance
// proxy — blocks carry no ownership marker once binned, so provenance is
// counted at the event level, which is exact for this purpose):
//
// R1 (review round) rework — BIN-INGRESS semantics: each counter counts
// blocks ENTERING a size-class bin, split by provenance at the exact call
// site (blocks carry no ownership marker once binned, so provenance is
// counted at the event level, which is exact for this purpose):
// - `DEPOT_BIN_INGRESS_OWN_OVERFLOW_*` — the own-thread magazine overflow
//   half-flush (`dealloc_own_base.rs`, `cnt == TCACHE_CAP` arm, FLUSH_N
//   blocks). Formerly `DEPOT_FLUSH_OVERFLOW_*`.
// - `DEPOT_FLUSH_SWAP_TOP_*`   — the R1-01 byte-budget swap-top eviction
//   (1 block; magazine bypasses the depot candidate set → also invalidates
//   the heap-private shadow, see `state/depot_shadow.rs`). Not a general
//   bin-ingress site (single-block eviction straight out of the magazine);
//   kept under the FLUSH name on purpose.
// - `DEPOT_BIN_INGRESS_OWN_BATCH_*` — `dealloc_batch`'s staged overflow
//   flushes (blocks routed straight to the bins, bypassing the depot
//   candidate set). Formerly `DEPOT_FLUSH_BATCHED_*`. OWN: these are the
//   batch's OWN thread's frees — this counter is NEVER a remote count.
// - `DEPOT_BIN_INGRESS_REMOTE_*` — remote free: a pending publication on a
//   foreign segment's sidecar is drained by the OWNING thread into its
//   freelist/bin. EVENTS: ONE increment per reclaiming pass at the SINGLE
//   point inside `AllocCore::consume_small_cuts_impl`
//   (`alloc_core/alloc_core/sidecar_drain.rs`) — R4: this covers ALL three
//   caller paths (strict trim `drain_sidecar_ingress`, bounded maintenance
//   `drain_sidecar_ingress_bounded`, and the discovery drain
//   `find_segment` → `drain_segment_sidecar`). BLOCKS: one per reclaimed
//   sidecar record (`consume_small_cuts_impl`). NOTE (R4): Large-retirement
//   inflates the strict/bounded `reclaimed` RETURN VALUE but is NOT counted
//   by EVENTS/BLOCKS — they count ONLY Small sidecar reclaim; Large has no
//   ingress counter (it never pushes a Small block into a bin).
// - `DEPOT_BIN_INGRESS_SUBSTRATE_PUSHES` — R4: the physical Small-block bin
//   ingress total (`AllocCore::dealloc_small`, after
//   `bt.set_head(class_idx, off)`); see the identity comment at the static.
// - `DEPOT_FLUSH_TEARDOWN_*`   — `flush_all_tcache` (teardown/trim).
//
// Refill accounting is unchanged: a drained remote publication re-enters
// through the refill drain (`DEPOT_REFILL_BLOCKS_DRAINED`).
//
// Refill side (`finish_magazine_refill` + `refill_class_bump_impl`):
// - `DEPOT_REFILL_EVENTS` / `DEPOT_REFILL_BLOCKS_TOTAL` — magazine refill
//   events and total blocks parked (issue_state == MAG only, like S1).
// - `DEPOT_REFILL_BLOCKS_DRAINED` — refill blocks served by a bin drain
//   (own-flushed OR remote-pending blocks — no marker distinguishes them,
//   which is exactly what the depot-count simulation does not need).
// - `DEPOT_REFILL_BLOCKS_CARVED` / `DEPOT_REFILL_CARVE_EVENTS` — virgin
//   carve blocks/events on refill (depot never participates in a carve;
//   the shadow flags these, it does not reset on them).
//
// All: process-global, monotone, Relaxed, measurement-only; same gating
// convention as `R1_01_CAP_ROUTE_FREES` above. Read via the
// `HeapCore::dbg_depot_*` associated fns in `diag_probes.rs`.
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_BIN_INGRESS_OWN_OVERFLOW_EVENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_BIN_INGRESS_OWN_OVERFLOW_BLOCKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_FLUSH_SWAP_TOP_EVENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_FLUSH_SWAP_TOP_BLOCKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_BIN_INGRESS_OWN_BATCH_EVENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_BIN_INGRESS_OWN_BATCH_BLOCKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_FLUSH_TEARDOWN_EVENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_FLUSH_TEARDOWN_BLOCKS: AtomicU64 = AtomicU64::new(0);
// R1: tripwire for depot-shadow hooks reached with a heap id that maps to
// NO shadow row (neither a registry slot nor the dedicated fallback row).
// Zero in every configuration this tree can produce — see
// `state/depot_shadow.rs` ("Row mapping").
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_SHADOW_UNMAPPED_EVENTS: AtomicU64 = AtomicU64::new(0);
// R1: remote bin-ingress counters — see the site map in the header comment
// above (`drain_sidecar_ingress`/`_bounded` for EVENTS,
// `consume_small_cuts_impl` for BLOCKS).
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_BIN_INGRESS_REMOTE_EVENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_BIN_INGRESS_REMOTE_BLOCKS: AtomicU64 = AtomicU64::new(0);
// R4 (b4-provenance): SUBSTRATE bin-ingress total — incremented at EVERY
// physical Small-block bin-entry point, on each successful push (after
// `bt.set_head`): (1) `AllocCore::dealloc_small`
// (alloc_core/small/alloc_core_small/dealloc.rs), (2) the `flush_run`
// splice (alloc_core/small/alloc_core_small_magazine.rs — byte-identical
// to N sequential `dealloc_small` pushes but done WITHOUT calling it;
// this is how own-overflow / own-batch / swap-top / teardown flush blocks
// physically enter the bin), (3) `reclaim_sidecar_record`
// (alloc_core/small/alloc_core_small_reclaim.rs — remote reclaim pushes
// straight onto the freelist). Together these cover every physical
// Small-block bin entry, so the identity holds exactly:
//
//   SUBSTRATE_PUSHES == own_overflow_blocks + own_batch_blocks
//                     + remote_blocks + swap_top_blocks
//                     + teardown_blocks + OTHER
//
// `OTHER = max(0, SUBSTRATE_PUSHES − Σ(attributed))` is a COMPUTED
// quantity (no separate static); examples compute and assert it
// (`OTHER == 0` is the provenance-completeness oracle — nonzero OTHER
// means an unattributed ingress path exists). The increment site is
// gated `bench-internals + alloc-global` (the
// `registry::heap_core::diag` path requires `alloc-global`).
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_BIN_INGRESS_SUBSTRATE_PUSHES: AtomicU64 = AtomicU64::new(0);
// R4 (b4-provenance) Large-retirement note: the strict
// (`drain_sidecar_ingress`) and bounded (`drain_sidecar_ingress_bounded`)
// return values (`reclaimed`) INCLUDE Large-retirement
// (`retire_large_route`) — that is production return semantics of the
// retirement path and is intentionally NOT changed. However
// `DEPOT_BIN_INGRESS_REMOTE_EVENTS` / `DEPOT_BIN_INGRESS_REMOTE_BLOCKS`
// count ONLY Small sidecar reclaim (EVENTS: `consume_small_cuts_impl`
// sees Small records only; BLOCKS: `reclaim_sidecar_record` Small records
// only). Large-retirement pushes NO Small block into a bin, so it has NO
// ingress counter by design and is NOT part of the SUBSTRATE_PUSHES
// identity above.
// ── B4 chunk 1b: process-wide STATIC MIRROR of the per-K depot-shadow
// aggregates ─────────────────────────────────────────────────────────────
//
// The heap-private `DepotShadow` (`state/depot_shadow.rs`) is unreachable
// when a workload runs through the installed `#[global_allocator] SeferAlloc`
// facade (the working heap cannot be claimed). These statics mirror the
// shadow's per-K aggregates with a parallel Relaxed `fetch_add` performed
// INSIDE the shadow's update methods (same events, same deltas — in addition
// to, never instead of, the local fields), so real facade-driven runs can
// read the aggregate absorption distribution per depot capacity
// K ∈ {1,2,4,8}.
//
// Consistency invariant (documented at the reader
// `HeapCore::dbg_depot_shadow_aggregates`): only the magazine OVERFLOW
// half-flush (`DEPOT_BIN_INGRESS_OWN_OVERFLOW_*`) feeds
// `depot_on_overflow_flush`; swap-top / batched / teardown flushes bypass
// the depot candidate set and only invalidate residency. Hence
// `sum_k(FLUSH_BLOCKS_ABSORBED[k] + FLUSH_BLOCKS_REAL[k]) ==
// DEPOT_BIN_INGRESS_OWN_OVERFLOW_BLOCKS` (process-global). Likewise the shadow refill
// hook fires at exactly the `DEPOT_REFILL_EVENTS` sites, so
// `sum_k(REFILL_FROM_DEPOT[k] + REFILL_DRAIN[k]) ==
// DEPOT_REFILL_BLOCKS_TOTAL`.
//
// Cross-heap note: the mirror is process-global while the local shadow is
// per-heap; multi-heap runs aggregate all heaps' events (which is exactly
// what a facade-driven measurement wants).
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_SHADOW_FLUSH_BLOCKS_ABSORBED: [AtomicU64; 4] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_SHADOW_FLUSH_BLOCKS_REAL: [AtomicU64; 4] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_SHADOW_FLUSH_EVENTS_FULLY_ABSORBED: [AtomicU64; 4] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_SHADOW_REFILL_BLOCKS_FROM_DEPOT: [AtomicU64; 4] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_SHADOW_REFILL_BLOCKS_DRAIN: [AtomicU64; 4] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_SHADOW_REFILL_FULLY_FROM_DEPOT: [AtomicU64; 4] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_SHADOW_INVALIDATION_EVENTS: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_REFILL_EVENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_REFILL_BLOCKS_TOTAL: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_REFILL_BLOCKS_DRAINED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_REFILL_BLOCKS_CARVED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
pub(crate) static DEPOT_REFILL_CARVE_EVENTS: AtomicU64 = AtomicU64::new(0);
