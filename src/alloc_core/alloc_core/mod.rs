//! [`AllocCore`] — the single-threaded allocator over the self-hosted segment
//! substrate (Phase 8, `alloc-core` feature).
//!
//! This is the **Cartographer** of the segment substrate: all placement logic
//! (which size class, which page, free-list pop/push, large/huge routing) is
//! **pure safe integer arithmetic** over segment-relative offsets and
//! size-class indices. Every raw memory touch is delegated to the [`node`](node)
//! seam; every OS reservation to the [`os`](os) seam. `AllocCore` itself
//! contains NO `unsafe` and NO `Vec`/`Box`/`HashSet`/`std::alloc` — the alloc
//! path is therefore **reentrancy-free (M5)**: it cannot recurse into the
//! global allocator because it allocates no metadata through it.
//!
//! ## API
//!
//! - [`AllocCore::new`] — bootstrap the primordial segment (the ONLY place
//!   that hand-carves self-hosted metadata; see [`bootstrap`]).
//! - [`alloc`](AllocCore::alloc) / [`dealloc`](AllocCore::dealloc) /
//!   [`realloc`](AllocCore::realloc) / [`alloc_zeroed`](AllocCore::alloc_zeroed)
//!   — the single-threaded allocator entry points. `alloc`/`alloc_zeroed`
//!   are **safe** `pub fn`s; `dealloc`/`realloc` are **`unsafe fn`s** (R6-MS-1/2)
//!   carrying a `# Safety` contract that mirrors `GlobalAlloc`'s — a
//!   well-behaved caller passes a valid prior pointer/layout (verify with
//!   `grep -n "pub unsafe fn dealloc\|pub unsafe fn realloc" src/alloc_core/alloc_core.rs`).
//!   The crate's former posture was a safe `pub fn` with a defensive **M2
//!   contract at runtime** (a foreign or already-freed pointer degraded to a
//!   no-op); that was reversed after the round5 `memory_safety_review` produced
//!   concrete safe-Rust counterexamples proving the defensive checks
//!   insufficient — see [`dealloc`](AllocCore::dealloc)'s `# Safety` section
//!   for the full rationale and exploit catalogue, and `CHANGELOG.md`
//!   (R6-MS-1/2) for the migration. The M2 defensive paths are RETAINED as
//!   defence-in-depth. None of the entry points panic or recurse.
//!
//! ## Single-threaded
//!
//! Phase 8 is single-threaded (correctness before concurrency — §5 P8).
//! Per-thread heaps + lock-free cross-thread free are Phase 9/10. `AllocCore`
//! is `Send` (it owns its segments, which are `Send`) but NOT `Sync`.

use crate::alloc_core::segment_table::SegmentTable;
#[cfg(feature = "alloc-segment-directory")]
use crate::alloc_core::size_classes::SMALL_CLASS_COUNT;

mod alloc_core_core_diag;
mod bootstrap;
/// Group module: the always-compiled bench/diagnostic statics
/// (`DECOMMIT_CALLS`, the decay/Tier-1 routing oracles, the zero-pass and
/// realloc/promotion counters, `FOREIGN_OR_UNROUTABLE_FREES`),
/// `promotion_byte_bucket`, and the `LargeCacheHitCounter` alias.
mod counters;
/// Group module: object lifecycle — construction config resolution, bootstrap
/// invocation, and teardown.
mod lifecycle;
/// Group module: the GlobalAlloc-face entry points (`alloc`, `alloc_zeroed`,
/// `dealloc`, `realloc`) and the in-place realloc fast-path family.
mod mem;
/// Group module: NUMA-node caching, the registry-walk/membership accessors
/// (`segment_bases`/`contains_base`/`small_cur`), and `(size, align)`
/// classification.
mod state;

// Path-parity re-exports: the diagnostic statics moved to `counters` (and
// `base_add` to `lifecycle`) in the mechanical split of the former flat
// `alloc_core.rs`, but their pre-split `alloc_core::<NAME>` module paths are
// still consumed across the crate (the root re-exports in
// `alloc_core/mod.rs`, `alloc_core_large_cache`, `alloc_core_small_pool`,
// `segment_table`). Each moved name is mirrored here at its original
// visibility, under the consumer's own feature gate, so every feature config
// keeps exactly the pre-split warning parity (a gate here without the
// consumer would be an unused-import warning; a consumer without the
// re-export would not resolve).
#[cfg(feature = "alloc-stats")]
pub(crate) use counters::LARGE_ZERO_PASS_CALLS;
#[cfg(all(feature = "alloc-stats", feature = "virgin-zero-skip"))]
pub(crate) use counters::SMALL_ZERO_PASS_CALLS;
#[cfg(all(
    feature = "bench-internals",
    feature = "medium-classes",
    any(
        not(feature = "exact-span-large"),
        all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
    )
))]
pub(crate) use counters::{
    promotion_byte_bucket, PROMOTION_BYTES_HIST, PROMOTION_BYTES_MAX, PROMOTION_BYTES_MIN,
    PROMOTION_BYTES_SUM, PROMOTION_COUNT,
};
#[cfg(feature = "alloc-decommit")]
pub(in crate::alloc_core) use counters::{LargeCacheHitCounter, DECOMMIT_CALLS};
#[cfg(feature = "bench-internals")]
pub(in crate::alloc_core) use counters::{CONTAINS_BASE_TIER1_HITS, CONTAINS_BASE_TIER1_MISSES};
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
pub(in crate::alloc_core) use counters::{FORCE_DECAY_CLOCK_READ, MAYBE_DECAY_GUARD_PASSED};
pub(in crate::alloc_core) use lifecycle::base_add;

// ---------------------------------------------------------------------------
// OPT-E — large-segment free-cache (feature = "alloc-decommit")
//
// The hot path for `alloc_large` / `dealloc` large is a full OS round-trip
// (mmap/VirtualAlloc + munmap/VirtualFree). mimalloc avoids this by keeping a
// per-allocator page-cache of recently-freed large spans so the next alloc of
// the same size hits the cache instead of the OS (~800 ns vs ~8–240 µs).
//
// We implement a MINIMAL version: a fixed array of LARGE_CACHE_SLOTS entries.
// The cache is ONLY active under `alloc-decommit` (it uses `table.recycle` for
// the slot-NULL step, which is only compiled with that feature; this keeps the
// logic consistent with the decommit-gate on the small-segment recycle path).
// ---------------------------------------------------------------------------

/// Maximum number of large segments held in the free-cache between uses.
///
/// Was 2 (Phase-1 minimal). Task D1: a workload that cycles through more than
/// two distinct large sizes (e.g. a DBMS with several large-object classes)
/// permanently evicted-and-recreated past 2 slots, forcing an OS round-trip
/// on every alloc despite the cache existing. 8 slots gives real headroom for
/// multi-size workloads while keeping the array (and the eviction scan, which
/// is O(LARGE_CACHE_SLOTS) and only runs on the large-alloc/dealloc slow path,
/// not the hot small-object path) cheap. The byte-budget
/// (`large_cache_budget_bytes`) remains the primary control on total cached
/// RSS; slot count only bounds how many *distinct* spans can be resident at
/// once.
///
/// R13-7 (task #277, EXPERIMENTAL `large-cache-extended`): this base count
/// stays 8 unconditionally (always-resident, zero-materialisation-cost
/// array) — a wider working set (16+ distinct sizes, e.g. under
/// `exact-span-large`'s per-size `usable` — see
/// `docs/perf/R13_6_EXACT_SPAN_RESERVED_CAPACITY_PRODUCTION_GATE.md` §4) is
/// served by the OPT-IN lazily-materialised `large_cache_extension` sidecar
/// (`large_cache_extended.rs`) layered on top, not by raising this constant.
/// See that module's doc for why "extend via an additive lazy sidecar"
/// was chosen over "raise this base array's size".
#[cfg(feature = "alloc-decommit")]
pub(super) const LARGE_CACHE_SLOTS: usize = 8;

// R32-12 (task #503, F8 sub-change (2)): compile-time pin that the combined
// base+extension slot count never exceeds the 64 bits `large_cache_occupied`
// (below) can address. Base is 8 unconditionally; the extension adds
// `LARGE_CACHE_EXTENDED_SLOTS` (32, `large_cache_extended.rs`) only under
// `large-cache-extended`, giving 40 total — comfortably under 64. If a future
// round raises either constant past this bound, this assertion is the
// intended trip-wire (widen `large_cache_occupied` to `u128` or a two-word
// bitmask rather than silently truncating `trailing_ones()`'s addressable
// range).
#[cfg(feature = "large-cache-extended")]
const _: () = assert!(
    LARGE_CACHE_SLOTS + super::large_cache_extended::LARGE_CACHE_EXTENDED_SLOTS
        <= u64::BITS as usize
);
#[cfg(all(feature = "alloc-decommit", not(feature = "large-cache-extended")))]
const _: () = assert!(LARGE_CACHE_SLOTS <= u64::BITS as usize);

/// Size-ratio bound: we only reuse a cached entry if its usable_size is at most
/// `needed * LARGE_CACHE_SIZE_FACTOR`. Without this a very large cached segment
/// would be permanently reused for every small large-request — wasting RSS
/// during the cache lifetime. Kept at 2 (as before): a 2× size tolerance is
/// tight enough to avoid gross RSS waste while still allowing minor rounding
/// differences between consecutive large allocations of "the same" size.
#[cfg(feature = "alloc-decommit")]
pub(super) const LARGE_CACHE_SIZE_FACTOR: usize = 2;

// RAD-3 (E2, task #56): the old `POOL_MAX_SLOTS = 4` compile-time hard cap —
// a fixed `[*mut u8; POOL_MAX_SLOTS]` storage array that silently clamped any
// `SmallSegmentPoolConfig::pool_segments` request above 4 — is REMOVED. The
// pool's storage is now an intrusive doubly-linked list threaded through
// `SegmentHeader::pool_next`/`pool_prev` (see `alloc_core_small_pool.rs`),
// so `AllocCore` holds only a head/tail pointer pair + `pooled_count` +
// `pool_cap`, independent of how large a cap the user configures — no
// compile-time array bound, and no per-registry-slot storage cost that scales
// with `MAX_HEAPS` regardless of whether a given heap ever pools a segment
// (the same class of structural cost RAD-1 removed from the registry's
// `next_free` bootstrap). `pool_cap` is now HONESTLY the resolved
// `min(pool_segments, pool_byte_cap / SEGMENT)` — no third `.min(_)` term.

// `LargeCacheMode` now lives in its own file (`large_cache_mode.rs`) per the
// one-export-per-file rule (task #27); it is re-exported unchanged by
// `alloc_core::mod.rs`. Imported here so the shard field, the config type, and
// the `dbg_large_cache_mode` test seam keep referring to it by bare name.
#[cfg(feature = "alloc-decommit")]
use crate::alloc_core::large_cache_mode::LargeCacheMode;

// ---------------------------------------------------------------------------
// Phase 2 — lazy exponential decay of the large-cache excess
// (feature = "alloc-decommit")
//
// Strategy: on each large alloc or free, check if enough wall-clock time has
// elapsed since the last decay tick. If so, compute the excess over the
// configurable headroom target and FIFO-evict a fraction (decay_rate_bp /
// 10 000) of that excess back to the OS. This keeps the cache from
// accumulating unbounded RSS between allocations while remaining "lazy" —
// no background thread is needed.
//
// Parameters are supplied via `LargeCacheConfig` (set once at
// `AllocCore::new_with_config`; defaults match the old env-var defaults when
// no variable was set).
// ---------------------------------------------------------------------------

/// Immutable decay configuration, computed once at `AllocCore::new_with_config`
/// from a [`LargeCacheConfig`](super::large_cache_config::LargeCacheConfig).
/// Kept in its own struct to make the intent clear and to allow
/// `dbg_set_decay_config` to swap it in tests.
#[cfg(feature = "alloc-decommit")]
#[derive(PartialEq)]
pub(super) struct LargeCacheDecayConfig {
    /// Fraction of the excess to release per tick, in basis points.
    /// 1000 = 10%, 5000 = 50%, 10000 = 100%.
    pub(super) decay_rate_bp: u32,
    /// Minimum wall-clock interval between consecutive decay ticks.
    pub(super) decay_interval: core::time::Duration,
    /// Target cache size in bytes. The "excess" above this level is subject
    /// to decay. On Phase 2 we treat `live_bytes = 0`; the target is just
    /// `headroom_bytes`. A future phase can add explicit live-count tracking.
    pub(super) headroom_bytes: usize,
}

/// One entry in the large-segment free-cache.
///
/// Invariant: `base` is SEGMENT-aligned, `reservation` was returned by the OS,
/// `usable_size` equals the `usable` computed in `alloc_large` at the time the
/// segment was first reserved (i.e. `n_segments * SEGMENT`). The segment's OS
/// reservation is still live (not yet released to the OS). Pages are kept
/// COMMITTED (no decommit on deposit) so that a cache hit requires no recommit.
///
/// When a cache hit occurs, the caller MUST:
///   1. Re-register `base` in the `SegmentTable`.
///   2. Write a fresh `SegmentHeader` over the old one (pages already committed).
///   3. Return `Node::deref(base, hdr_aligned)` to the caller.
#[cfg(feature = "alloc-decommit")]
pub(super) struct CachedLarge {
    /// Start of the original OS reservation.
    pub(super) reservation: *mut u8,
    /// Total size of the OS reservation.
    pub(super) reservation_len: usize,
    /// SEGMENT-aligned base of the segment (the "usable" start).
    pub(super) base: *mut u8,
    /// The `usable` bytes this reservation covers — `n_segments * SEGMENT` for
    /// the original allocation. Used to match incoming requests.
    pub(super) usable_size: usize,
    /// R12-4 (feature `large-reserved-capacity`): the segment's total
    /// RESERVED VA span at the time it was deposited (`>= usable_size`) —
    /// mirrors `SegmentHeader::reserved_capacity`'s bug-#134-shaped carry-
    /// forward discipline: read from the header's `reserved_capacity` field
    /// at deposit time and restamped verbatim into the reused header on the
    /// next cache hit, never recomputed. Without the feature this always
    /// equals `usable_size` ("reserved == committed", the inert value).
    #[cfg_attr(not(feature = "large-reserved-capacity"), allow(dead_code))]
    pub(super) reserved_capacity: usize,
    /// Insertion sequence number (task D1). Monotonically increasing per
    /// deposit, taken from `AllocCore::large_cache_seq`. The true FIFO-oldest
    /// occupied slot is the one with the SMALLEST `seq` — NOT necessarily the
    /// lowest array index once `LARGE_CACHE_SLOTS > 2` (with more than two
    /// slots, hits and re-deposits no longer fill/empty strictly in index
    /// order, so "lowest index = oldest" stops holding; see D1 in
    /// `docs/checkpoints` history). This field restores a correct FIFO
    /// ordering independent of slot count.
    pub(super) seq: u64,
}

/// A single-threaded allocator over the self-hosted segment substrate.
///
/// Owns its segments (the primordial + any additionally-reserved small or
/// large/huge segments). The registry of live segments lives in the
/// primordial segment's payload (self-hosted) — there is NO `Vec<Segment>`:
/// `AllocCore::drop` walks the registry and frees every reservation through
/// the `os` seam.
///
/// ## PERF-PASS-5 (G7/ML6, task #53) — field DECLARATION order is a no-op here
///
/// `AllocCore` is `repr(Rust)` (no explicit `#[repr(..)]`): field
/// declaration order is a HINT to the compiler, not a layout guarantee. The
/// 2026-07-10 memory-layout review (finding 6) measured rustc placing the
/// cold 384-byte `large_cache` array ahead of the dealloc-hot `table` field
/// under the OLD source order (`large_cache` declared before `table`). This
/// task moved `table` and `small_cur` to be the FIRST two fields declared
/// below — and then re-measured with `-Zprint-type-sizes` on this project's
/// CURRENT profile (task #49 already added `lto = "thin"` /
/// `codegen-units = 1`): the compiled layout is BYTE-IDENTICAL to before —
/// rustc still places `large_cache` first, `table`/`small_cur` in the
/// middle. A minimal reproduction (two structs with `table`/`small_cur`
/// declared first vs. `large_cache` declared first, otherwise identical
/// fields) confirmed this is not an artifact of this struct's specific
/// `#[cfg(feature = "alloc-decommit")]` gating: rustc's `repr(Rust)`
/// layout algorithm reorders fields by its own heuristic (chiefly
/// size/alignment) REGARDLESS of declaration order for this field set (all
/// fields here are `align <= 8`, so there is no alignment-driven reason to
/// prefer one order over the other, and the compiler's choice is not
/// influenced by which one happens to appear first in the source).
///
/// **Verdict: NO-OP, reported honestly per the task spec rather than forcing
/// a cosmetic reorder that measurement shows does nothing.** The
/// declaration order below (`table`/`small_cur` first) is KEPT anyway
/// because it is the more readable grouping (hot fields first in the
/// source, matching the doc narrative) and costs nothing — but it has no
/// effect on the compiled cache-line layout `AllocCore::dealloc`'s
/// `table.own_cache` touch actually observes. If a future toolchain/profile
/// change makes `repr(Rust)` field order load-bearing again, an explicit
/// `#[repr(C)]` (with the accompanying hand-packing discipline
/// `SegmentHeader` uses) would be the correct fix, not a bare declaration
/// reorder.
pub struct AllocCore {
    /// The primordial segment registry (self-hosted in segment 0's payload).
    pub(super) table: SegmentTable,
    /// Metadata view of the "current" small segment — the one whose bump
    /// cursor and free lists new small allocations draw from. When it fills,
    /// [`alloc_small`] reserves a fresh small segment and switches to it.
    ///
    /// [`alloc_small`]: Self::alloc_small
    pub(super) small_cur: *mut u8,
    /// R11-5: cached return value of `numa::current_node()` for the calling
    /// thread, populated lazily by [`current_node_cached`](Self::current_node_cached)
    /// and reset to `None` at registry-slot `claim()` time by
    /// [`invalidate_numa_node_cache`](Self::invalidate_numa_node_cache).
    ///
    /// `None` = "not yet queried on this claim"; `Some(n)` = "the last query
    /// returned `n`". Owner-private single-writer (this `AllocCore`'s owning
    /// thread is its sole mutator), so a plain `Option<u32>` — no `Cell`, no
    /// atomic — is sound and the cheapest possible cache. See
    /// `docs/PHASE_NUMA_DESIGN.md` §4.1 for the full design note, including
    /// the slot-recycle invalidation argument and the resulting staleness
    /// bound (a migrated thread's reads may lag the OS's real answer for
    /// the duration of the current slot claim — `claim()` to `recycle()`).
    ///
    /// R12-5: this claim-lifetime bound is unlimited in wall-clock time for a
    /// long-lived claim (a `HeapCore` held by a non-pinned thread for
    /// millions of allocations never re-queries once populated, pre-R12-5).
    /// [`current_node_cached`](Self::current_node_cached) now additionally
    /// forces a re-query every [`NUMA_NODE_REFRESH_PERIOD`] calls, bounding
    /// the staleness to that many refill-misses even within a single claim.
    /// See `docs/PHASE_NUMA_DESIGN.md` §4.1 "Bounded mid-claim refresh
    /// (R12-5)" for the full rationale.
    #[cfg(feature = "numa-aware")]
    pub(super) cached_numa_node: Option<u32>,
    /// R12-5: number of [`current_node_cached`](Self::current_node_cached)
    /// calls served from cache since the value was last populated by an
    /// actual `numa::current_node()` query (by either a cache miss or a
    /// periodic forced refresh). Reset to `0` every time the cache is
    /// (re-)populated; reset implicitly whenever `cached_numa_node` is set to
    /// `None` (the next call is a miss regardless of this counter's value).
    /// Compared against [`NUMA_NODE_REFRESH_PERIOD`] to trigger the periodic
    /// refresh. Owner-private single-writer, same discipline as
    /// `cached_numa_node`.
    #[cfg(feature = "numa-aware")]
    pub(super) numa_node_hits_since_refresh: u32,
    /// OPT-E — large-segment free-cache. A small fixed array of recently-freed
    /// large/huge segments whose OS reservations are still live. `alloc_large`
    /// checks this array first; a size-matched entry is reused without a new
    /// OS reservation. `dealloc` on the large path deposits the segment here
    /// (if a slot is free and the budget permits) instead of releasing
    /// the OS reservation immediately. Pages are kept committed between uses so
    /// no recommit syscall is needed on a cache hit. The cache is gated on
    /// `alloc-decommit` for consistency with the small-segment recycle path
    /// (both operate in the regime where empty slots are recyclable).
    #[cfg(feature = "alloc-decommit")]
    pub(super) large_cache: [Option<CachedLarge>; LARGE_CACHE_SLOTS],

    /// R32-12 (task #503, F8 sub-change (2)): occupancy bitmask over the
    /// COMBINED base+extension index space (see `CombinedSlot` in
    /// `alloc_core_large_cache.rs`) — bit `i` set ⟺ combined slot `i`
    /// currently holds `Some(CachedLarge)`. Replaces the free-slot-search
    /// scan (`self.large_cache.iter().position(|s| s.is_none())`,
    /// `large_cache_find_free_slot`) with a `trailing_ones()` lookup: the
    /// index of the lowest CLEAR bit is the lowest free slot, found without
    /// walking the 56-byte-per-slot `[Option<CachedLarge>; N]` array at all.
    ///
    /// The discriminant this bit mirrors is NOT duplicated data (a bit
    /// derived from `Option::is_some()` carries no independent value that
    /// could drift out of sync in the sense a copied field could) — but it
    /// IS state that must be maintained in lockstep with every occupation
    /// change, so every site that currently sets/clears an
    /// `Option<CachedLarge>` slot to `Some`/`None` must also set/clear the
    /// corresponding bit here. The complete enumeration of those sites (per
    /// CLAUDE.md's site-enumeration discipline):
    ///   1. [`large_cache_slot_set`](Self::large_cache_slot_set) — sets the
    ///      bit for `idx` (the slot transitions None → Some).
    ///   2. [`large_cache_slot_take`](Self::large_cache_slot_take) — clears
    ///      the bit for `idx` (the slot transitions Some → None).
    ///
    /// These are the ONLY two functions in this crate that ever write
    /// `self.large_cache[i]` or `ext.slots[i]` (verified by grep — see
    /// `docs/perf/R32_12_LARGE_CACHE_OCCUPANCY_BITMASK_GATE.md` §2 for the
    /// exhaustive site list); every other mutation (deposit, cache-hit take,
    /// eviction) already funnels through one of these two, so maintaining
    /// the bitmask in exactly these two places keeps it correct by
    /// construction, not by convention.
    ///
    /// Sized `u64`: `LARGE_CACHE_SLOTS + LARGE_CACHE_EXTENDED_SLOTS` (8 + 32
    /// = 40) is the current combined maximum, pinned `<= u64::BITS` by the
    /// compile-time assertions above this struct — see those assertions'
    /// own doc for what to do if a future round raises either constant past
    /// 64.
    #[cfg(feature = "alloc-decommit")]
    pub(super) large_cache_occupied: u64,

    /// R13-7 (task #277, EXPERIMENTAL `large-cache-extended`): lazily-
    /// materialised sidecar pointer widening the large-cache from
    /// `LARGE_CACHE_SLOTS` (8) to `8 + LARGE_CACHE_EXTENDED_SLOTS` (40) total
    /// slots. `null` = not yet materialised (either the base 8 slots have
    /// never overflowed, or the feature is off, or sidecar OOM — all three
    /// are indistinguishable and all three mean "cache is capped at the base
    /// 8 slots", exactly the pre-existing behaviour). Owner-only (plain
    /// `*mut`, not `AtomicPtr` — mirrors `directory_sidecar`, not
    /// `dirty_by_class`; see `large_cache_extended`'s module doc for why no
    /// `OncePtrCell` is needed here). Dereferenced via
    /// `large_cache_extended::deref_large_cache_extension[_mut]`.
    #[cfg(feature = "large-cache-extended")]
    pub(super) large_cache_extension: *mut super::large_cache_extended::LargeCacheExtension,

    /// Per-shard byte budget for the large-cache. `None` = unbounded (any span
    /// may be admitted as long as a free slot exists). When set, the sum of
    /// `usable_size` across all occupied slots is kept `<= large_cache_budget_bytes`;
    /// an incoming span that would exceed the budget triggers FIFO eviction of
    /// the oldest slot before admission.
    ///
    /// Set via [`LargeCacheConfig::budget_bytes`] passed to
    /// [`AllocCore::new_with_config`].
    ///
    /// [`LargeCacheConfig::budget_bytes`]: super::large_cache_config::LargeCacheConfig::budget_bytes
    #[cfg(feature = "alloc-decommit")]
    pub(super) large_cache_budget_bytes: Option<usize>,

    /// Running sum of `usable_size` across all currently occupied slots in
    /// `large_cache`.
    ///
    /// Invariant:
    /// ```text
    /// large_cache_used_bytes ==
    ///     large_cache.iter().filter_map(|s| s.as_ref().map(|c| c.usable_size)).sum()
    /// ```
    /// Maintained on every deposit (`+= usable_size`) and every eviction /
    /// cache-hit (`-= slot.usable_size`). NOT decremented on `AllocCore::drop`
    /// (the field is dead at that point).
    #[cfg(feature = "alloc-decommit")]
    pub(super) large_cache_used_bytes: usize,

    /// Monotonic insertion-sequence counter for `large_cache` deposits (task
    /// D1). Each deposit stamps the current value into `CachedLarge::seq` and
    /// then increments this counter. FIFO eviction picks the occupied slot
    /// with the smallest `seq` — the true "oldest" entry — rather than
    /// assuming index order, which only happened to hold for the old
    /// `LARGE_CACHE_SLOTS == 2` minimal implementation.
    #[cfg(feature = "alloc-decommit")]
    pub(super) large_cache_seq: u64,

    // ── Phase 2 — lazy decay ─────────────────────────────────────────────────
    /// Immutable decay parameters: rate, interval, headroom. Set once at
    /// `AllocCore::new_with_config` from a `LargeCacheConfig`; overridable in
    /// tests via `dbg_set_decay_config`.
    #[cfg(feature = "alloc-decommit")]
    pub(super) decay_config: LargeCacheDecayConfig,

    /// Wall-clock time of the last decay tick. `None` = never ticked yet (the
    /// first call to `maybe_decay_large_cache` primes the timer without
    /// releasing anything). Stored as `Option<std::time::Instant>` so the very
    /// first call does not accidentally release half the cache at process start.
    #[cfg(feature = "alloc-decommit")]
    pub(super) last_decay_tick: Option<std::time::Instant>,

    /// R32-8 (task #499, F9): monotonic count of `maybe_decay_large_cache`
    /// calls that passed the headroom fast-exit (i.e.
    /// `large_cache_used_bytes` greater than `headroom_bytes`) since the
    /// last clock read. A cheap counter increment/compare is used to decide
    /// WHETHER to consult the clock at all, ahead of the `Instant::now()`
    /// read itself — see
    /// [`maybe_decay_large_cache`](Self::maybe_decay_large_cache)'s own doc
    /// for the exact trade (decay-tick GRANULARITY vs clock-read frequency)
    /// this buys. Reset to 0 every time the clock IS actually consulted
    /// (`maybe_decay_large_cache`) so the stride restarts from the moment of
    /// the last real check. Also primed by
    /// [`dbg_force_decay_tick`](Self::dbg_force_decay_tick), which bypasses
    /// the stride entirely — see its own doc.
    #[cfg(feature = "alloc-decommit")]
    pub(super) large_cache_decay_op_count: u32,

    // ── Phase 3 — cache operating mode ───────────────────────────────────────
    /// The large-cache operating mode, set once at `AllocCore::new_with_config`
    /// from a `LargeCacheConfig`. Stored for diagnostic/test access and as the
    /// anchor for future scavenger-thread wiring.
    ///
    /// `Lazy` — the only variant, currently — is Phase 2 lazy decay, no
    /// background thread. `LargeCacheMode` is `#[non_exhaustive]`: a future
    /// background-scavenger mode can be added as a non-breaking addition
    /// (R3-B removed the earlier unimplemented `Background`/`Both`
    /// placeholders — see `docs/reviews/2026-07-12-round3-remediation-plan.md`,
    /// решение №2).
    #[cfg(feature = "alloc-decommit")]
    pub(super) large_cache_mode: LargeCacheMode,

    /// TEST/DIAGNOSTIC-ONLY (task D1 → #133): this `AllocCore`'s OWNED
    /// large-cache hit counter — the fallback target used when this heap is
    /// NOT bound to a registry slot (a STANDALONE `AllocCore` built directly by
    /// tests via `AllocCore::new`). For a slot-bound heap this field is left
    /// untouched after bind: the increment and the diagnostic read are both
    /// redirected to the slot's counter via [`large_cache_hits_sink`](Self::large_cache_hits_sink).
    ///
    /// Kept as an owned `AtomicU64` (not removed) precisely so the standalone
    /// path — which has no registry slot and no cross-thread aggregator reading
    /// it, hence no aliasing gap — still counts hits for the `AllocCore`-level
    /// large-cache regression tests.
    #[cfg(feature = "alloc-decommit")]
    pub(super) large_cache_hits: LargeCacheHitCounter,

    /// TEST/DIAGNOSTIC-ONLY (task W3): stable `&'static` handle to THIS heap's
    /// SLOT-resident large-cache hit counter
    /// ([`HeapSlot::large_cache_hits`](crate::registry::heap_slot::HeapSlot::large_cache_hits)),
    /// planted by `HeapRegistry::claim` via
    /// [`bind_large_cache_hits`](Self::bind_large_cache_hits) at bind time.
    /// See [`LargeCacheHitCounter`] above for the aliasing-gap rationale.
    ///
    /// `Some` for a slot-bound heap → the increment and `dbg_large_cache_hits`
    /// go to the slot's `AtomicU64` (the SAME one the cross-thread aggregator
    /// reads, so the views agree — and NO `&AllocCore` is ever materialised by
    /// the aggregator). `None` for a standalone `AllocCore` → both fall back to
    /// the owned [`large_cache_hits`](Self::large_cache_hits) field above.
    ///
    /// Stored as a SAFE `Option<&'static _>` (this module is
    /// `#![forbid(unsafe_code)]` — a raw pointer would be unusable).
    #[cfg(feature = "alloc-decommit")]
    pub(super) large_cache_hits_sink: Option<&'static LargeCacheHitCounter>,

    // ── Mechanism 2 (task #51; RAD-3/E2 task #56 restructure) — empty-small-
    // segment hysteresis pool ─────────────────────────────────────────────────
    /// The pool's HEAD: the base of the most-recently-pooled ("warmest")
    /// empty small segment, or `null` if the pool is empty. The pool's
    /// storage is an intrusive DOUBLY-linked list threaded through each
    /// pooled segment's own [`SegmentHeader::pool_next`]/`pool_prev` fields
    /// (see [`SmallSegmentPoolConfig`] for the pool's design) — `AllocCore`
    /// itself holds only this head pointer, [`pool_tail`](Self::pool_tail),
    /// [`pooled_count`](Self::pooled_count), and
    /// [`pool_cap`](Self::pool_cap).
    ///
    /// **Intrusive list, not a fixed array — why (RAD-3/E2).** The prior
    /// design used a fixed `[*mut u8; POOL_MAX_SLOTS]` array (`POOL_MAX_SLOTS
    /// = 4`), which silently clamped any `pool_segments` request above 4 and,
    /// more importantly, is a compile-time-sized field INSIDE `AllocCore` —
    /// which lives inline in every registry `HeapSlot` (`MAX_HEAPS = 4096`
    /// slots). Raising the cap by widening the array multiplies that fixed
    /// cost by 4096 regardless of whether a given heap ever pools a segment —
    /// exactly the structural RSS/binary-size cost class RAD-1 eliminated
    /// from the registry's `next_free` bootstrap. An intrusive list instead
    /// stores its "next"/"prev" links INSIDE the segments themselves (which
    /// already exist, already have header bytes to spare — see
    /// `SegmentHeader`'s RAD-3 doc note) — `AllocCore`'s own per-heap cost
    /// stays two pointers + two `usize`s, INDEPENDENT of how large a cap the
    /// user configures.
    ///
    /// List order: HEAD = warmest (most recently emptied) — the analogue of
    /// the old array's "push at `pooled_count`, pop from `pooled_count - 1`"
    /// LIFO order, now realised as O(1) push-front / pop-front. TAIL =
    /// coldest (least recently emptied) — evicted by the decay tick, the
    /// analogue of the old min-seq scan, now O(1) pop-back.
    ///
    /// No per-segment "is this pooled?" flag is needed (same invariant as
    /// before): a pooled segment stays a normal registered, committed,
    /// `live_count == 0` small segment; "pooled" means only "this segment is
    /// currently linked into the pool list" — `pool_next`/`pool_prev` are
    /// both `null` for the pool's sole entry (head==tail) or for a
    /// not-currently-pooled segment (see `release_or_pool_empty_segment` for
    /// the stale-ring-while-pooled soundness argument, unchanged by this
    /// restructure).
    ///
    /// [`SmallSegmentPoolConfig`]: super::small_segment_pool_config::SmallSegmentPoolConfig
    #[cfg(feature = "alloc-decommit")]
    pub(super) pool_head: *mut u8,

    /// The pool's TAIL: the base of the least-recently-pooled ("coldest")
    /// empty small segment, or `null` if the pool is empty. See
    /// [`pool_head`](Self::pool_head) for the full design note.
    #[cfg(feature = "alloc-decommit")]
    pub(super) pool_tail: *mut u8,

    /// Number of segments currently linked into the pool list
    /// (`pool_head`/`pool_tail` + each entry's `pool_next`/`pool_prev`).
    ///
    /// `u32`, not `usize` (task #1998): `AllocCore` lives inline in every
    /// registry `HeapSlot` (`MAX_HEAPS = 4096`), so each field's width here is
    /// a 4096x multiplier on registry footprint — the same cost discipline
    /// [`pool_head`](Self::pool_head)'s and
    /// [`dbg_reservation_owner_id`](Self::dbg_reservation_owner_id)'s doc
    /// comments already apply. Bounded by [`pool_cap`](Self::pool_cap), which
    /// is itself clamped to `u32::MAX` at resolution time, so this field can
    /// never legitimately need more than 32 bits.
    #[cfg(feature = "alloc-decommit")]
    pub(super) pooled_count: u32,

    /// Resolved runtime cap on pooled segments: `min(config.pool_segments,
    /// config.pool_byte_cap / SEGMENT)`. `0` = pool disabled (every empty
    /// small segment released immediately — the pre-Mechanism-2 behaviour).
    /// Set once at [`AllocCore::new_with_config`].
    ///
    /// **RAD-3 (E2, task #56): no third `.min(POOL_MAX_SLOTS)` term.** The
    /// prior compile-time array cap silently clamped any request above 4;
    /// the intrusive-list storage has no such compile-time bound, so this
    /// cap now HONESTLY reflects exactly what the caller configured (bounded
    /// only by the byte budget) — the value returned by
    /// [`dbg_pool_cap`](Self::dbg_pool_cap) is always the true operative cap,
    /// observable and un-clamped.
    ///
    /// `u32`, not `usize` (task #1998, same registry-footprint discipline as
    /// [`pooled_count`](Self::pooled_count)'s doc comment). Resolution
    /// (`AllocCore::new_with_config`) clamps `min(pool_segments,
    /// pool_byte_cap / SEGMENT)` to `u32::MAX` before storing — a resolved
    /// cap above `u32::MAX` segments would mean the caller configured
    /// `>= 16 EiB` of pooled segments (`u32::MAX * SEGMENT` at `SEGMENT` = 4
    /// MiB), not a real deployment; the clamp only prevents a silent
    /// truncation wraparound for that unreachable-in-practice input, it does
    /// not change behavior for any value a real config can produce.
    #[cfg(feature = "alloc-decommit")]
    pub(super) pool_cap: u32,

    /// Wall-clock time of the last small-pool decay tick. `None` = never ticked.
    /// Mirrors [`last_decay_tick`](Self::last_decay_tick) for the large cache.
    /// The decay evicts the FIFO-oldest pooled segment once the configured
    /// interval elapses, so a burst-then-quiet small workload does not pin the
    /// pooled segments indefinitely (the hard bound is still the `pool_cap` /
    /// byte-cap; the decay is the "eventual drain to zero when truly idle" that
    /// makes retention TEMPORARY, not merely bounded).
    #[cfg(feature = "alloc-decommit")]
    pub(super) last_pool_decay_tick: Option<std::time::Instant>,

    // ── R7-A1: per-class segment directory sidecar ──────────────────────────
    /// Lazily-materialised `SegmentDirectory` sidecar — null until
    /// `table.count() >= DIRECTORY_MATERIALIZE_THRESHOLD` (32). Owner-only
    /// (plain `*mut`, not `AtomicPtr`): only the owning thread reads/writes
    /// this pointer and the sidecar it points to.
    ///
    /// `null` = directory not yet materialised (either below threshold, or
    /// sidecar OOM). A non-null value is a valid, OS-zeroed-or-rebuilt
    /// `*mut SegmentDirectory` leaked for the process lifetime. Dereferenced
    /// via `os::deref_directory_sidecar[_mut]`.
    ///
    /// Nothing queries this directory for lookups yet (A3 scope). A1 adds
    /// only the storage, lazy materialisation, one-time rebuild, and the dbg
    /// accessor.
    #[cfg(feature = "alloc-segment-directory")]
    pub(super) directory_sidecar: *mut super::segment_directory::SegmentDirectory,

    /// R8-2 (task #215) / R9-8 (task #230): consecutive genuine directory
    /// misses (no candidate validated) since the last full-scan re-validation
    /// pass, tracked PER-CLASS so a drift-affected class trips its OWN rescan
    /// independent of how often other (healthy) classes miss. Reset to 0 for
    /// a class every time a periodic re-validation scan actually runs for THAT
    /// class (whether or not it finds anything). See
    /// `DIRECTORY_MISS_FULL_SCAN_PERIOD` (the per-class threshold, 64) for the
    /// rationale on the value. `u8` storage: the period (64) fits comfortably
    /// and keeps this at `SMALL_CLASS_COUNT` bytes (49 B default); the
    /// const-assert below pins that the period never exceeds `u8::MAX`.
    #[cfg(feature = "alloc-segment-directory")]
    pub(super) directory_miss_streak: [u8; SMALL_CLASS_COUNT],

    /// R7-A4: reference to the owning HeapSlot's `dirty_segments` bitmap
    /// (planted by `HeapCore` at bind time). `None` until bound (the
    /// pre-bind AllocCore is standalone and has no registry slot). The
    /// reference is `&'static` because the HeapSlot lives in the process-
    /// global registry array, leaked for the process lifetime.
    ///
    /// Used by `find_segment_with_free_impl` to drain ONLY dirty segments'
    /// rings instead of polling every ring. Feature-gated: the dirty routing
    /// only matters when both `alloc-xthread` (cross-thread frees exist) and
    /// `alloc-segment-directory` (the directory drives the drain) are active.
    #[cfg(all(feature = "alloc-xthread", feature = "alloc-segment-directory"))]
    pub(crate) dirty_segments:
        Option<&'static [core::sync::atomic::AtomicU64; super::segment_directory::WORDS_PER_CLASS]>,

    /// R12-7 stage 2 (`class-aware-dirty`, EXPERIMENTAL): reference to the
    /// owning HeapSlot's lazily-materialised per-(segment, class) dirty-bit
    /// sidecar cell (planted by `HeapCore` at bind time, same discipline as
    /// [`dirty_segments`](Self::dirty_segments)). `None` until bound.
    ///
    /// Note this is a handle to the CELL, not the sidecar itself — the
    /// sidecar behind the cell may still be UNINIT (no class-routed
    /// cross-thread free has landed on this heap yet); `drain_dirty_segments`
    /// resolves it read-only via `dirty_by_class::get_per_class_dirty` (never
    /// materialising it from the drain side — see that function's doc
    /// comment).
    #[cfg(feature = "class-aware-dirty")]
    pub(crate) dirty_by_class:
        Option<&'static once_ptr_cell::OncePtrCell<super::dirty_by_class::PerClassDirty>>,

    /// R13-1 (task #271, P0 fix): reference to the owning HeapSlot's
    /// coarse-only latch (planted by `HeapCore` at bind time, same
    /// discipline as [`dirty_by_class`](Self::dirty_by_class)). `None` until
    /// bound. See `registry::heap_slot::HeapSlotRemote::sidecar_oom_latch`'s
    /// doc comment for the full design; read by `drain_dirty_segments` to
    /// decide whether the per-class scan path may be trusted at all for this
    /// heap.
    #[cfg(feature = "class-aware-dirty")]
    pub(crate) sidecar_oom_latch: Option<&'static core::sync::atomic::AtomicBool>,

    /// R31-15 (task #486): a stable, process-wide-unique identity for THIS
    /// `AllocCore`, stamped once at construction ([`new_inner`](Self::new_inner))
    /// from [`DBG_RESERVATION_OWNER_ID_COUNTER`]'s `fetch_add`. Exists solely
    /// to bind [`ReservedSmallSegment`](super::reserved_small_segment::ReservedSmallSegment)
    /// handles to the exact `AllocCore` that minted them — see
    /// [`dbg_decomp_release`](super::alloc_core_small_pool::AllocCore::dbg_decomp_release)'s
    /// doc comment for the soundness hole this closes (a handle minted by one
    /// `AllocCore` could otherwise be handed to a DIFFERENT `AllocCore`'s
    /// `dbg_decomp_release`, corrupting the wrong heap's pool/table state).
    ///
    /// **Deliberately NOT the `&self` address.** An `AllocCore` is `Send` and
    /// lives inline inside a registry `HeapSlot` / can be moved by ordinary
    /// Rust value semantics (e.g. returned by value from `AllocCore::new()`),
    /// so two DIFFERENT logical `AllocCore`s can transiently or permanently
    /// occupy the same address over a process's lifetime (a moved-from slot's
    /// old address, or a recycled/reused stack slot in a test loop) — an
    /// address-based check would falsely accept a stale handle against a new
    /// owner that happens to reuse the old owner's address. A monotonic
    /// counter has no such collision: `fetch_add` on a process-global atomic
    /// never repeats a value for the process lifetime (barring a `u64`
    /// wraparound, astronomically unreachable for a counter incremented once
    /// per `AllocCore` construction).
    ///
    /// `bench-internals`-gated: this field exists purely to support the
    /// `dbg_decomp_reserve_and_keep`/`dbg_decomp_release` measurement-only
    /// hook pair and has no role in any production code path. Gating it
    /// behind `bench-internals` (rather than the narrower `alloc-decommit`
    /// that hook pair is also gated on) keeps the field's cost at exactly
    /// zero in every `production`/default build — `AllocCore` lives inline in
    /// every `HeapSlot` (`MAX_HEAPS = 4096`), so an always-present field here
    /// multiplies its size by 4096 regardless of whether any caller ever
    /// touches the decomposition hooks (the same cost-discipline argument
    /// [`pool_head`](Self::pool_head)'s doc comment makes for the intrusive
    /// pool-list redesign).
    #[cfg(feature = "bench-internals")]
    pub(super) dbg_reservation_owner_id: u64,
}
