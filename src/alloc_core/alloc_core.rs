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

use core::alloc::Layout;

use super::bootstrap;
use super::node::Node;
#[cfg(feature = "numa-aware")]
use super::numa;
use super::os;
#[cfg(feature = "alloc-decommit")]
use super::os::SEGMENT;
#[cfg(feature = "numa-aware")]
use super::segment_header::SegmentMeta;
use super::segment_header::{SegmentHeader, SegmentKind};
use super::segment_table::SegmentTable;
use super::size_classes::{AllocKind, SizeClasses};

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
#[cfg(feature = "alloc-decommit")]
pub(super) const LARGE_CACHE_SLOTS: usize = 8;

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
use super::large_cache_mode::LargeCacheMode;

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

#[cfg(feature = "alloc-decommit")]
impl LargeCacheDecayConfig {
    /// Build the decay config from a resolved [`LargeCacheConfig`].
    ///
    /// [`LargeCacheConfig`]: super::large_cache_config::LargeCacheConfig
    fn from_config(cfg: &super::large_cache_config::LargeCacheConfig) -> Self {
        Self {
            decay_rate_bp: cfg.resolved_decay_rate_bp(),
            decay_interval: cfg.resolved_decay_interval(),
            headroom_bytes: cfg.resolved_headroom_bytes(),
        }
    }
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

/// TEST-ONLY (Phase 35): process-wide M6-decommit invocation counter. Bumped in
/// `decommit_empty_segment_impl` (the shared decommit body); read by the soak
/// test via [`AllocCore::dbg_decommit_count`]. Diagnostic only (relaxed).
#[cfg(feature = "alloc-decommit")]
pub(super) static DECOMMIT_CALLS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// DIAGNOSTIC (review finding 2.3): process-wide count of `dealloc` calls that
/// hit the foreign-or-unroutable no-op branch — a `ptr` whose segment base is
/// NOT one of this heap's registered segments, so `dealloc` silently drops it
/// (see [`AllocCore::dealloc`]).
///
/// **Why this counter exists — the `alloc-global`-without-`alloc-xthread`
/// footgun.** In a build WITHOUT `alloc-xthread` there is no cross-thread
/// routing path: a block allocated on thread A and freed on thread B resolves
/// to a base that is not in B's heap's segment table, falls into this no-op,
/// and is **leaked permanently** (see `SeferAlloc`'s "Multi-thread safety"
/// docs). That configuration is a legitimate single-threaded trade-off — so
/// there is no `compile_error!` — but a multi-threaded program built that way
/// by mistake would leak monotonically with NO observable metric. This counter
/// is that metric: a non-zero, growing value under `alloc-global` alone is the
/// signature of a misconfiguration (or a genuine foreign-pointer free).
///
/// Surfaced as [`AllocStats::foreign_or_unroutable_frees`](crate::AllocStats::foreign_or_unroutable_frees)
/// via [`AllocCore::dbg_foreign_or_unroutable_frees`]. Diagnostic only
/// (Relaxed, like `DECOMMIT_CALLS` / `DBG_RING_OVERFLOW`).
///
/// The per-event increment is gated behind `alloc-stats` (default OFF, not in
/// `production`), matching the other per-event stat counters (`tcache_hits`,
/// `large_cache_hits`): the free hot path carries no bookkeeping unless
/// `alloc-stats` is compiled in. The static itself is always present (gated on
/// `alloc-core` — the feature that first defines `AllocCore::dealloc` and its
/// foreign-pointer no-op) so the accessor has a stable definition regardless of
/// the rest of the feature set. `alloc-stats` depends on `alloc-core`, so
/// whenever the increment is compiled in the static is guaranteed to exist.
#[cfg(feature = "alloc-core")]
pub(crate) static FOREIGN_OR_UNROUTABLE_FREES: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

// TEST/DIAGNOSTIC-ONLY (task D1 → 0.4.x task #133): large-cache HIT
// counter. Originally a single process-wide `static AtomicU64`, bumped by
// EVERY heap's `alloc_large` cache-hit path — a contended `lock xadd` on a
// path that is architecturally per-heap (each `AllocCore` lives inside one
// `HeapCore`, which lives on one thread's registry slot). Under MT this
// counter's cache line ping-ponged across cores on every large-cache hit —
// directly on the hot path of the crate's flagship workload (large-object
// churn, e.g. shamir-db), perf regression #133.
//
// Fix: the counter is now a PER-HEAP field (`AllocCore::large_cache_hits`,
// see below), incremented only by its owning `AllocCore`'s (and therefore
// that heap's owning thread's) own calls — never shared with another
// heap's cache line. It stays an `AtomicU64` (Relaxed) rather than a plain
// `u64` because the process-global VIEW
// (`registry::heap_registry::large_cache_hits_total`, aggregated into
// `SeferAlloc::stats()`) reads every live heap's counter from whatever
// thread calls `stats()` — a plain `u64` written by the owner and read by
// a different thread without synchronisation would be a data race (UB);
// `Relaxed` on both sides is sound for a diagnostic counter with no
// ordering requirement (the same pattern as `DBG_LARGE_XTHREAD_RECLAIMED`
// and the new `HeapCore::tcache_hits`), and needs no `unsafe` — safe-Rust
// atomics all the way, consistent with `#![forbid(unsafe_code)]`.
//
// TASK W3 (0.3.0) — the counter STORAGE moved out of `AllocCore` and into the
// owning `HeapSlot` (`HeapSlot::large_cache_hits`), closing a formal aliasing
// gap: the process-wide aggregator (`large_cache_hits_total`) used to
// materialise a shared `&HeapCore`/`&AllocCore` (`(*heap_ptr).core
// .dbg_large_cache_hits()`) over a struct the OWNING thread concurrently holds
// a protected `&mut` into — a foreign-read of a protected `Unique`, UB under
// Stacked Borrows. The counter now lives in the `Sync` slot; the owner reaches
// it through a SAFE `Option<&'static AtomicU64>` handle (a raw pointer would
// be a hard error — this module is `#![forbid(unsafe_code)]`), planted by
// `HeapRegistry::claim` at bind time. See `HeapSlot::large_cache_hits`.
#[cfg(feature = "alloc-decommit")]
pub(super) type LargeCacheHitCounter = core::sync::atomic::AtomicU64;

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
    #[cfg(feature = "alloc-decommit")]
    pub(super) pooled_count: usize,

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
    #[cfg(feature = "alloc-decommit")]
    pub(super) pool_cap: usize,

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

    /// R8-2 (task #215): consecutive genuine directory misses (no candidate
    /// validated) since the last full-scan re-validation pass. Reset to 0 every
    /// time a periodic re-validation scan actually runs (whether or not it finds
    /// anything). See `DIRECTORY_MISS_FULL_SCAN_PERIOD`.
    #[cfg(feature = "alloc-segment-directory")]
    pub(super) directory_miss_streak: u32,

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
}

impl AllocCore {
    /// Bootstrap the allocator using default large-cache configuration.
    ///
    /// Equivalent to `AllocCore::new_with_config(LargeCacheConfig::DEFAULT)`.
    /// Returns `None` only if the OS refuses the primordial reservation (OOM at
    /// startup).
    ///
    /// `AllocCore` intentionally does **not** implement `Default` (R3-C). Both
    /// `new()` and `new_with_config()` return `Option<Self>` because the very
    /// first thing construction does is a real multi-MiB OS memory reservation
    /// for the primordial segment, which can fail under memory pressure /
    /// OOM / `rlimit`. A `Default` impl would have to hide that fallibility
    /// behind an `.expect(...)` panic, but generic code across the ecosystem
    /// treats `T::default()` / `T: Default` as a conventionally-cheap,
    /// infallible operation (e.g. `Option::<T>::unwrap_or_default()`,
    /// `#[derive(Default)]`, `mem::take`, `resize_with(Default::default)`) —
    /// none of those call sites expect a multi-MiB syscall plus a latent
    /// panic. Keeping the construction fallibility explicit (callers write
    /// `AllocCore::new().expect("...")` themselves) is the deliberate design,
    /// not an oversight; do not re-add a `Default` impl.
    #[must_use]
    #[inline]
    pub fn new() -> Option<Self> {
        #[cfg(feature = "alloc-decommit")]
        return Self::new_with_config(super::large_cache_config::LargeCacheConfig::DEFAULT);
        #[cfg(not(feature = "alloc-decommit"))]
        return Self::new_inner();
    }

    /// Bootstrap the allocator with a user-supplied large-cache configuration.
    ///
    /// All `LargeCacheConfig` fields use their documented defaults when `None`.
    /// Returns `None` only if the OS refuses the primordial reservation (OOM at
    /// startup).
    ///
    /// Use this when you want to set the cache knobs at compile time without
    /// environment variables (runnable form in `tests/large_cache_config_knobs.rs`):
    ///
    /// ```text
    /// use sefer_alloc::{AllocCore, LargeCacheConfig, LargeCacheMode};
    ///
    /// let cfg = LargeCacheConfig::new()
    ///     .budget_bytes(512 * 1024 * 1024)
    ///     .headroom_bytes(64 * 1024 * 1024)
    ///     .decay_interval_ms(200)
    ///     .decay_rate_percent(25)
    ///     .mode(LargeCacheMode::Lazy);
    ///
    /// let ac = AllocCore::new_with_config(cfg).expect("primordial");
    /// drop(ac);
    /// ```
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    #[inline]
    pub fn new_with_config(config: super::large_cache_config::LargeCacheConfig) -> Option<Self> {
        let mut core = Self::new_inner()?;
        core.large_cache_budget_bytes = config.resolved_budget_bytes();
        core.decay_config = LargeCacheDecayConfig::from_config(&config);
        // R3-B (round3, решение №2): `LargeCacheMode` now carries only the
        // `Lazy` variant — the unimplemented `Background`/`Both` variants were
        // removed from the enum entirely ("make invalid states
        // unrepresentable"). The old T5 panic-match that rejected them is gone
        // with the variants: that match was itself reachable lazily through
        // `GlobalAlloc::alloc` (first-bind materialises the heap) and so
        // conflicted with the never-panics entry-point guarantee. With only
        // `Lazy` representable there is nothing left to validate or branch on
        // here — just store the resolved mode. See
        // `docs/reviews/2026-07-12-round3-remediation-plan.md`.
        core.large_cache_mode = config.resolved_mode();
        // Mechanism 2 (task #51); RAD-3 (E2, task #56): resolve the
        // empty-small-segment pool cap. The effective cap is the tighter of
        // the two bounds the user actually controls:
        //   - the configured segment count,
        //   - the byte ceiling expressed in whole segments (each is `SEGMENT`).
        // A `0` in EITHER config knob disables the pool (min → 0), matching the
        // `SmallSegmentPoolConfig` contract. NO third `.min(POOL_MAX_SLOTS)`
        // term — the old compile-time array bound is gone (the pool's storage
        // is now the intrusive `SegmentHeader::pool_next`/`pool_prev` list,
        // which has no fixed capacity), so a caller who asks for
        // `.pool_segments(64)` genuinely GETS a cap of 64 (subject only to the
        // byte budget), not a silent clamp to 4.
        let pool_cfg = config.resolved_pool();
        let by_segments = pool_cfg.resolved_pool_segments();
        let by_bytes = pool_cfg.resolved_pool_byte_cap() / SEGMENT;
        core.pool_cap = by_segments.min(by_bytes);
        Some(core)
    }

    /// Compare this heap's live (resolved) cache/pool policy against a
    /// requested [`LargeCacheConfig`]. Returns `true` when the resolved
    /// values match what [`new_with_config`](Self::new_with_config) would
    /// apply from `requested`.
    ///
    /// Used by `HeapRegistry::claim_with_config` (task #95 / N2) to detect
    /// that a recycled slot's pre-existing policy silently overrides a
    /// different config passed by a later claimant. Comparing **resolved**
    /// values (not the raw `Option` fields of `LargeCacheConfig`) means two
    /// configs that resolve identically — e.g. `budget_bytes(None)` vs
    /// `LargeCacheConfig::DEFAULT` — are correctly treated as a match, so
    /// this only flags genuine policy differences, not stylistic builder
    /// variation.
    ///
    /// **Drift hazard:** the comparisons here MUST stay in sync with the
    /// fields [`new_with_config`](Self::new_with_config) sets. If a future
    /// change adds a new config-derived field to `AllocCore`, add the
    /// matching comparison here.
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn live_config_matches(
        &self,
        requested: &super::large_cache_config::LargeCacheConfig,
    ) -> bool {
        if self.large_cache_budget_bytes != requested.resolved_budget_bytes() {
            return false;
        }
        if self.decay_config != LargeCacheDecayConfig::from_config(requested) {
            return false;
        }
        if self.large_cache_mode != requested.resolved_mode() {
            return false;
        }
        let pool_cfg = requested.resolved_pool();
        let by_segments = pool_cfg.resolved_pool_segments();
        let by_bytes = pool_cfg.resolved_pool_byte_cap() / SEGMENT;
        self.pool_cap == by_segments.min(by_bytes)
    }

    /// Inner bootstrap: reserve the primordial segment and hand-carve its
    /// self-hosted metadata. All feature-gated fields are set to their
    /// defaults here; `new_with_config` then overwrites the decommit knobs.
    #[inline]
    fn new_inner() -> Option<Self> {
        let prim = bootstrap::primordial()?;
        let primordial_base = prim.segment.as_ptr();
        // The primordial segment hosts the registry AND serves as the first
        // small segment (its remaining payload is free for small allocs).
        let small_cur = primordial_base;
        // We take ownership of the registry; the primordial Segment handle is
        // forgotten — its memory is freed by walking the registry in `drop`
        // (the registry records the reservation pointers, so we do not need
        // the Rust `Segment` handle to free it).
        core::mem::forget(prim.segment);
        // Phase C (numa-aware): the primordial segment was reserved by
        // `bootstrap::primordial()` via the plain OS path (it predates NUMA
        // awareness). Stamp the current thread's NUMA node into its header NOW
        // so that `find_segment_with_free` can treat it as a local segment.
        // On platforms without NUMA `current_node()` returns `NO_NODE`; the
        // field already holds `NO_NODE_RAW` (same value), so this is a no-op
        // in terms of visible effect — but it makes the invariant explicit.
        #[cfg(feature = "numa-aware")]
        {
            let my_node = numa::current_node();
            SegmentMeta::new(primordial_base).set_node_id(my_node);
        }
        Some(Self {
            table: prim.table,
            small_cur,
            #[cfg(feature = "alloc-decommit")]
            large_cache: [const { None }; LARGE_CACHE_SLOTS],
            #[cfg(feature = "alloc-decommit")]
            large_cache_budget_bytes: None,
            #[cfg(feature = "alloc-decommit")]
            large_cache_used_bytes: 0,
            #[cfg(feature = "alloc-decommit")]
            large_cache_seq: 0,
            #[cfg(feature = "alloc-decommit")]
            decay_config: LargeCacheDecayConfig {
                decay_rate_bp: super::large_cache_config::DEFAULT_DECAY_RATE_PERCENT * 100,
                decay_interval: core::time::Duration::from_millis(
                    super::large_cache_config::DEFAULT_DECAY_INTERVAL_MS,
                ),
                headroom_bytes: super::large_cache_config::DEFAULT_HEADROOM_BYTES,
            },
            #[cfg(feature = "alloc-decommit")]
            last_decay_tick: None,
            #[cfg(feature = "alloc-decommit")]
            large_cache_mode: LargeCacheMode::Lazy,
            #[cfg(feature = "alloc-decommit")]
            large_cache_hits: LargeCacheHitCounter::new(0),
            // W3: unbound by default; `HeapRegistry::claim` redirects this to
            // the owning slot's counter for a registry-bound heap. Standalone
            // `AllocCore`s (tests) stay `None` and count into the owned field
            // above.
            #[cfg(feature = "alloc-decommit")]
            large_cache_hits_sink: None,
            // Mechanism 2 (task #51); RAD-3 (E2, task #56): empty pool.
            // `pool_cap` defaults to the production default here (as if
            // `SmallSegmentPoolConfig::DEFAULT`); `new_with_config` overwrites
            // it from the resolved config. The `new_inner`-only path (a
            // `not(alloc-decommit)`-style direct bootstrap) never reaches
            // this arm — every `alloc-decommit` build funnels construction
            // through `new_with_config`. `pool_head`/`pool_tail` start
            // `null` (empty list) — no fixed-size array to zero-init.
            #[cfg(feature = "alloc-decommit")]
            pool_head: core::ptr::null_mut(),
            #[cfg(feature = "alloc-decommit")]
            pool_tail: core::ptr::null_mut(),
            #[cfg(feature = "alloc-decommit")]
            pooled_count: 0,
            #[cfg(feature = "alloc-decommit")]
            pool_cap:
                super::small_segment_pool_config::SmallSegmentPoolConfig::DEFAULT_POOL_SEGMENTS.min(
                    super::small_segment_pool_config::SmallSegmentPoolConfig::DEFAULT_POOL_BYTE_CAP
                        / SEGMENT,
                ),
            #[cfg(feature = "alloc-decommit")]
            last_pool_decay_tick: None,
            #[cfg(feature = "alloc-segment-directory")]
            directory_sidecar: core::ptr::null_mut(),
            #[cfg(feature = "alloc-segment-directory")]
            directory_miss_streak: 0,
            #[cfg(all(feature = "alloc-xthread", feature = "alloc-segment-directory"))]
            dirty_segments: None,
        })
    }

    /// Allocate `layout.size()` bytes satisfying `layout.align()`.
    ///
    /// Returns a non-null `*mut u8` on success, or null on OOM. The memory is
    /// **uninitialised** (matching `GlobalAlloc::alloc`); see
    /// [`alloc_zeroed`](Self::alloc_zeroed) for zeroed memory.
    ///
    /// Zero-size layouts are not supported (they violate the `GlobalAlloc`
    /// contract; we round up to `MIN_BLOCK` and serve normally).
    #[must_use]
    #[inline(always)]
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size().max(super::size_classes::MIN_BLOCK);
        let align = layout.align();
        match Self::classify(size, align) {
            AllocKind::Small { class_idx } => self.alloc_small(class_idx),
            AllocKind::Large => self.alloc_large(size, align),
        }
    }

    /// Allocate `layout.size()` bytes of **zeroed** memory.
    #[must_use]
    pub fn alloc_zeroed(&mut self, layout: Layout) -> *mut u8 {
        let ptr = self.alloc(layout);
        if !ptr.is_null() {
            Node::zero(ptr, layout.size().max(super::size_classes::MIN_BLOCK));
        }
        ptr
    }

    /// Deallocate memory previously returned by [`alloc`](Self::alloc) (or
    /// `alloc_zeroed`/`realloc`).
    ///
    /// This is an **`unsafe fn`**: it trusts its caller to honour the
    /// [`GlobalAlloc::dealloc`](crate::global::SeferAlloc)-shaped contract (see
    /// `# Safety`). The crate's former posture was a *safe* `pub fn` with an
    /// M2 "defensive-free" guard (matching `free()` in mimalloc/glibc); that
    /// posture was **reversed in R6-MS-1/2** after the round5
    /// `memory_safety_review` produced concrete safe-Rust counterexamples
    /// proving the defensive checks insufficient — a same-class in-place
    /// realloc that *resurrects* a freed block (two live allocations at one
    /// address), a fully-overlapping `copy_nonoverlapping(p, p, n)` in the
    /// realloc move leg, and an interior `dealloc` of a Large segment
    /// releasing the whole reservation of a still-live neighbour. Marking the
    /// entry point `unsafe fn` makes a contract violation *documented caller
    /// UB* rather than an unsound *safe* API. The M2 defensive paths
    /// (foreign-pointer no-op, bitmap guard) are **retained as
    /// defence-in-depth** — they no longer carry the soundness argument, but
    /// they still make many accidental misuses benign at runtime.
    ///
    /// See `docs/agent_reviews_round5/memory_safety_review.md` (R5-MS-1/MS-2)
    /// for the full exploit catalogue and `CHANGELOG.md` (R6-MS-1/2) for the
    /// migration.
    ///
    /// **Phase 13.3 — arithmetic own-thread free.** The hot path is now pure
    /// arithmetic + (at most) one field-specific header byte read, NOT a
    /// full-struct `SegmentHeader::read_at`. Specifically:
    ///   - `segment_base_of(ptr)` — one mask (already the case).
    ///   - `self.table.contains_base(base)` — the foreign-pointer guard (this
    ///     is the load-bearing defence-in-depth check, NOT the `magic` word:
    ///     a foreign pointer's computed base is simply not in our registry,
    ///     so we never touch its bytes).
    ///   - `SegmentHeader::kind_at(base)` — ONE byte field read (via
    ///     `offset_of!`) to distinguish Large from Small/Primordial. This is
    ///     the minimum read necessary: Large blocks are freed by marking the
    ///     segment (no class free list), Small/Primordial go to the BinTable;
    ///     without distinguishing them we'd misroute. `kind` is written once
    ///     at segment init and immutable thereafter, so this byte read cannot
    ///     race an owner write on the disjoint `bump` field (the §11
    ///     root-cause analysis).
    ///   - the size class is derived from the caller-supplied `Layout` via
    ///     `Self::classify` — pure arithmetic, no `page_map` lookup (§13:
    ///     `page_map` is unreliable for mixed-class pages, and own-thread
    ///     free HAS the `Layout`, so deriving from it is both cheaper AND
    ///     correct).
    ///
    /// The `SEGMENT_MAGIC` full-struct sanity check is intentionally absent
    /// here: it lives ONLY on the defensive cross-thread routing path
    /// (`HeapCore::dealloc_routing` under `alloc-xthread`), where a foreign
    /// pointer could in principle resolve to a registered-but-not-ours base.
    /// On the trusted own-thread path, `contains_base` is the sole guard and
    /// the `Layout` is authoritative for the class — a full header load would
    /// be a dependent load on the free critical path with no correctness gain.
    ///
    /// # Safety
    ///
    /// The caller must uphold the [`GlobalAlloc::dealloc`] contract for `ptr`
    /// and `layout`. Concretely:
    ///
    /// - `ptr` is **null** OR the exact **start** pointer of a currently-LIVE
    ///   allocation owned by *this* `AllocCore`, returned by a prior
    ///   [`alloc`](Self::alloc)/[`alloc_zeroed`](Self::alloc_zeroed)/
    ///   [`realloc`](Self::realloc). It MUST NOT be an interior pointer
    ///   (`base + interior_offset`): the foreign/interior defences are
    ///   best-effort, not a soundness guarantee, and an interior Large free
    ///   would release the whole reservation of a still-live neighbour.
    /// - `layout` exactly matches the layout the allocation was made with.
    /// - The allocation is freed **at most once**: a double-free, and any
    ///   re-issue of `ptr` after this call (before a later `alloc` happens to
    ///   reuse its address for a new owner), is UB.
    /// - `ptr` is not a foreign / already-released-unmapped base (a pointer
    ///   from another allocator or a segment whose OS reservation has been
    ///   released).
    ///
    /// Null `ptr` is always safe (early return). The M2 defensive paths
    /// (foreign-pointer no-op, bitmap guard) make several of these accidental
    /// violations benign *at runtime*, but they are NOT a substitute for
    /// honouring the contract — a violation this method cannot detect is UB.
    #[inline]
    #[allow(unsafe_code)] // R6-MS-1/2: `unsafe fn` boundary (caller-pointer contract).
    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        let base = os::segment_base_of_ptr(ptr);
        // Foreign-pointer check: if the computed segment base is NOT one of our
        // registered segments, this pointer is not one of ours — no-op (do not
        // touch foreign memory, do not even read a header that may be unmapped).
        if !self.table.contains_base(base) {
            // Review finding 2.3: make the drop OBSERVABLE. Without
            // `alloc-xthread` this branch is the sole guard, and a cross-thread
            // free lands here as a PERMANENT leak — the misconfiguration
            // signature this counter exists to expose (see
            // `FOREIGN_OR_UNROUTABLE_FREES`). Gated behind `alloc-stats` so the
            // free hot path pays nothing by default, matching the crate's other
            // per-event stat counters. Relaxed: diagnostic only.
            #[cfg(feature = "alloc-stats")]
            FOREIGN_OR_UNROUTABLE_FREES.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return;
        }
        // Field-specific `kind` read (Phase 13.3): a single byte at its
        // `offset_of!` offset, NOT a full-struct `read_at`. Distinguishes
        // Large (free = mark segment) from Small/Primordial (free = push to
        // BinTable). `kind` is immutable after init, so this byte read is
        // race-free against the owner's disjoint `bump` writes.
        match SegmentHeader::kind_at(base) {
            SegmentKind::Large => {
                // Large/huge: the segment is being freed. The full header read
                // here is on the cold Large path (one allocation per segment,
                // rare), so the dependent-load cost does not matter.
                //
                // OPT-E (alloc-decommit): if the segment is small enough to
                // cache AND a free slot exists, decommit its payload pages and
                // deposit it into the large_cache so the next alloc_large of a
                // compatible size can reuse it without an OS round-trip.
                //
                // Without alloc-decommit (or when the cache admission is
                // declined): the OS reservation is released EAGERLY, right
                // here — NOT deferred to `Drop` (task #125; see the release
                // branches below for the full rationale). `unregister(base)`
                // runs first so `Drop`'s `table.bases()` walk never sees
                // `base` again — no double-free of the reservation.
                //
                // Phase 2: run a lazy decay tick on large free (same cheap
                // Instant check as on the alloc path).
                #[cfg(feature = "alloc-decommit")]
                self.maybe_decay_large_cache();

                let stale = SegmentHeader::read_at(base);

                #[cfg(feature = "alloc-decommit")]
                {
                    // The physical usable span is read from the header's
                    // stable `span_usable` field — NOT recomputed from
                    // `large_size`/`large_align`. Bug #134: on a cache-hit
                    // reuse the header's logical size/align can be smaller
                    // than the segment's actual physical footprint (the OS
                    // reservation is reused as-is for a smaller request), so
                    // recomputing "usable size" from size/align here
                    // under-reports the true span and corrupts the
                    // large-cache byte-budget accounting. `span_usable` is
                    // set once at the segment's original OS reservation and
                    // carried forward verbatim through every cache-hit reuse
                    // (see `SegmentHeader::span_usable` doc).
                    let usable_size = stale.span_usable;

                    // Phase 1 large-cache admission: byte-budget enforcement.
                    //
                    // Strategy:
                    //   1. Find a free slot (None). If none is free, try FIFO eviction.
                    //   2. Check that depositing this span would not exceed the budget
                    //      (if one is set). If the budget would overflow, evict the
                    //      oldest occupied slot to make room. If eviction still can't
                    //      satisfy the budget (budget < usable_size), skip caching.
                    //   3. Deposit into the (now-free) slot.
                    //
                    // FIFO definition (task D1): the "oldest" slot is the
                    // occupied one with the smallest insertion `seq`, found via
                    // `oldest_occupied_slot` (a seq-based min-by scan) — NOT slot
                    // index 0. Each deposit stamps `large_cache_seq` into
                    // `CachedLarge::seq`, so eviction picks the true FIFO-oldest
                    // entry regardless of slot order. This holds for any
                    // LARGE_CACHE_SLOTS (currently 8), not just the old 2-slot
                    // minimal implementation that happened to fill slots in order.
                    // Two independent admission constraints: (1) there must be a
                    // free slot, (2) the byte-budget (if set) must accommodate
                    // `usable_size`. Either failing means we evict the oldest and
                    // retry. Bug #94 history: an earlier version short-circuited
                    // `try_evict_to_fit` to `true` when budget=None and missed
                    // the "slots full" case entirely, silently releasing every
                    // span beyond the first two to the OS. The loop below treats
                    // both constraints uniformly.
                    let mut admitted: Option<usize> = None;
                    loop {
                        let free_slot = self.large_cache.iter().position(|s| s.is_none());
                        let budget_ok = self.large_cache_budget_bytes.is_none_or(|budget| {
                            self.large_cache_used_bytes + usable_size <= budget
                        });
                        if let Some(idx) = free_slot {
                            if budget_ok {
                                admitted = Some(idx);
                                break;
                            }
                        }
                        // Either no free slot, or budget would overflow → evict
                        // the oldest entry and retry. If the cache is already
                        // empty there is nothing more we can do.
                        if !self.evict_one_oldest() {
                            break;
                        }
                    }

                    if let Some(slot_idx) = admitted {
                        // We keep the pages COMMITTED in the cache (no decommit
                        // on deposit). On Windows, `VirtualAlloc(MEM_DECOMMIT)`
                        // followed immediately by `VirtualAlloc(MEM_COMMIT)` on
                        // the next cache hit costs more than just leaving the
                        // pages mapped — the entire purpose of the cache is to
                        // amortise the OS round-trip cost. Decommitting here
                        // would reduce RSS by the usable payload size, but at the
                        // cost of an expensive recommit on every hit, negating
                        // the speedup. We intentionally trade RSS for latency:
                        // a cached large segment keeps its pages warm between uses.
                        //
                        // NULL the table slot WITHOUT releasing the OS reservation.
                        // The cached entry owns the reservation; AllocCore::drop
                        // releases it explicitly from the large_cache array.
                        self.table.unregister(base);
                        // Zero the magic so that if something reads the header
                        // while it's in the cache, it won't be confused as a
                        // live registered segment.
                        //
                        // UBFIX-6 (M-2, docs/reviews/2026-07-10-ub-audit-final-
                        // synthesis.md): this used to be `hdr_zero = stale;
                        // hdr_zero.magic = 0; Node::write_struct(base, hdr_zero)`
                        // — a non-atomic FULL-STRUCT write that races with
                        // `SegmentHeader::magic_at`/`kind_at`/`large_size_at`/
                        // `span_usable_at` (remote defensive field reads that can
                        // observe a live header concurrently with this owner
                        // write under a stale/duplicate remote free — misuse of
                        // the `GlobalAlloc` contract the defensive reads exist to
                        // survive without UB). `stale` is a fresh `read_at(base)`
                        // taken just above, so every OTHER field of `hdr_zero`
                        // is byte-identical to what is already in memory — the
                        // full-struct write's only REAL effect was zeroing
                        // `magic`. Restoring this file's own §11 discipline
                        // ("remote-readable field ⇒ atomic single-word access",
                        // the same pattern `SegmentMeta::owner_state_atomic`
                        // already uses for cross-thread owner-state reads): write only the
                        // `magic` field, through an `&AtomicU32` view at its
                        // `offset_of!` offset, so a concurrent remote
                        // `magic_at`/`kind_at`/`large_size_at`/`span_usable_at`
                        // read never races a torn/non-atomic store — those other
                        // three fields are untouched here, so no write to them is
                        // needed at all.
                        let magic_off = core::mem::offset_of!(SegmentHeader, magic);
                        Node::atomic_u32_at(base, magic_off)
                            .store(0, core::sync::atomic::Ordering::Release);
                        // Deposit into cache and update the byte-budget counter.
                        let seq = self.large_cache_seq;
                        self.large_cache_seq = self.large_cache_seq.wrapping_add(1);
                        self.large_cache[slot_idx] = Some(CachedLarge {
                            reservation: stale.reservation,
                            reservation_len: stale.reservation_len,
                            base,
                            usable_size,
                            seq,
                        });
                        self.large_cache_used_bytes += usable_size;
                        return;
                    }
                    // Not admitted (no free slot after eviction, or budget too small):
                    // release the OS reservation EAGERLY right now rather than
                    // deferring to `AllocCore::drop` (task #125 / same leak
                    // class as A1/#114). In the Phase 12.5 shard model, the
                    // per-thread `AllocCore` living in a registry slot is
                    // effectively never dropped mid-process (the slot is
                    // recycled between threads, but the `AllocCore` itself
                    // persists) — "defer release to Drop" is therefore a
                    // PERMANENT leak of both the OS reservation and the
                    // `SegmentTable` slot on the own-thread admission-reject
                    // path, eventually exhausting `MAX_SEGMENTS` and forcing
                    // `alloc_large` to return null. `unregister` FIRST (frees
                    // the slot for reuse; mirrors `reclaim_large_segment`'s
                    // ordering), THEN release — Drop's `table.bases()` walk
                    // will no longer see `base`, so there is no double-free.
                    self.table.unregister(base);
                    os::release_segment(stale.reservation, stale.reservation_len);
                }
                #[cfg(not(feature = "alloc-decommit"))]
                {
                    // No large-cache at all: every own-thread large free must
                    // release eagerly for the same reason as the
                    // admission-reject branch above (task #125) — deferring
                    // to `Drop` leaks the reservation AND the `SegmentTable`
                    // slot for the remaining process lifetime.
                    self.table.unregister(base);
                    os::release_segment(stale.reservation, stale.reservation_len);
                }
            }
            SegmentKind::Small | SegmentKind::Primordial => {
                // Derive the class from the caller's `Layout` (pure
                // arithmetic via `SIZE2CLASS`) — NOT from `page_map`. §13 of
                // RACE_DRAIN_RECLAIM.md: `page_map` records only the FIRST
                // class to touch a page, so it returns the wrong class for
                // any later block of a different class in the same page. The
                // own-thread freer HAS the original `Layout`, so classifying
                // from it is both cheaper (no page_map load) AND correct.
                let size = layout.size().max(super::size_classes::MIN_BLOCK);
                let align = layout.align();
                let kind = Self::classify(size, align);
                let class_idx = match kind {
                    AllocKind::Small { class_idx } => class_idx,
                    // Layout mismatch: the original allocation was small but
                    // the dealloc layout classifies as large. This is a
                    // contract violation; no-op (do not corrupt).
                    AllocKind::Large => return,
                };
                self.dealloc_small(base, ptr, class_idx);
            }
            // L-5 (UBFIX-11): `contains_base` already proved `base` is one of
            // OUR registered segments, but the `kind` BYTE at that base has
            // been corrupted to something other than the three legitimate
            // discriminants (0/1/2) — `kind_at`'s strict decode maps that to
            // `Unknown` rather than guessing. Neither the Large branch (which
            // would release/cache the OS reservation) nor the Small/
            // Primordial branch (which would write a BinTable/free-list
            // header into the payload) is safe to run against a segment
            // whose real kind we cannot trust — no-op is the only sound
            // choice: do not touch this segment's payload or reservation at
            // all. Same reject-not-guess posture as the H-1 payload
            // lower-bound guard (UBFIX-3).
            SegmentKind::Unknown => {}
        }
    }

    /// Shrink/grow an allocation in place or by alloc + copy + dealloc.
    ///
    /// Two in-place fast paths are attempted first (shared with
    /// [`try_realloc_inplace_known_base`](Self::try_realloc_inplace_known_base), which [`HeapCore::realloc`](crate::registry::HeapCore)
    /// calls so its alloc leg can route through the magazine-aware
    /// `HeapCore::alloc`):
    ///
    /// **OPT-F — in-place small→small realloc:** when both the old and new
    /// sizes resolve to the SAME size class (`new_class_idx == old_class_idx`),
    /// the block physically fits the new size without any data movement, so we
    /// return the original pointer unchanged: no alloc, no copy, no dealloc.
    /// The block's live-count and alloc-bitmap stay intact. The `==` (not
    /// `<=`) rule is load-bearing — see `realloc_inplace_fast_path`'s comment
    /// and `tests/regression_realloc_cross_class_shrink.rs`.
    ///
    /// **OPT-G — in-place Large→Large realloc:** when the block lives in a
    /// Large segment and the grown size (clamped to `MIN_BLOCK`) still fits
    /// the segment's `span_usable`, we update the header's `large_size` and
    /// return the same pointer. Shrinks fall through to the slow path
    /// (reclaims RSS). The stored size is clamped to `MIN_BLOCK` to stay
    /// symmetric with the alloc path and the #138 cross-thread consistency
    /// check (`large_layout_consistent`).
    ///
    /// On growth the new tail is **uninitialised** (matching `GlobalAlloc`).
    /// Returns null on failure, leaving the old allocation intact. Null `ptr`
    /// returns null without touching state.
    ///
    /// A **foreign pointer** (its computed segment base is not one of ours)
    /// also returns null without touching state, symmetric with
    /// [`dealloc`](Self::dealloc)'s foreign-pointer no-op. This is a
    /// substrate-level (`AllocCore`) entry point with no cross-heap concept:
    /// unlike `HeapCore::realloc` (which has a design-load-bearing foreign-leg
    /// for `alloc-xthread` — a pointer from another live heap in the SAME
    /// process is legitimate there and `dealloc` routes it cross-thread), a
    /// pointer this `AllocCore` does not recognise is never legitimate.
    ///
    /// This is an **`unsafe fn`** (R6-MS-1/2): the move leg's
    /// [`Node::copy_nonoverlapping`](crate::alloc_core::node::Node) reads
    /// `old_layout.size()` bytes out of `ptr`, and trusts the caller's
    /// `old_layout`/`ptr` exactly as `GlobalAlloc::realloc` does. The crate's
    /// former posture was a safe `pub fn` bounded by
    /// [`safe_payload_read_span`](Self::safe_payload_read_span); that was
    /// reversed after the round5 review showed the same-class in-place branch
    /// resurrecting a freed block and the foreign/move legs being reachable
    /// from safe code. See `# Safety` and `CHANGELOG.md` (R6-MS-1/2).
    ///
    /// # Safety
    ///
    /// The caller must uphold the [`GlobalAlloc::realloc`] contract for `ptr`
    /// and `old_layout`:
    ///
    /// - `ptr` is **null** OR the exact **start** pointer of a currently-LIVE
    ///   allocation owned by *this* `AllocCore`, made with a `Layout` whose
    ///   size/align match `old_layout`. It MUST NOT be an interior pointer.
    /// - `old_layout` exactly matches the allocation's layout; in particular
    ///   `old_layout.size()` must not exceed the block's true size (the move
    ///   leg copies that many bytes out of `ptr`).
    /// - On success (`!null` return) the OLD `ptr` is freed by this call — it
    ///   MUST NOT be used or re-freed afterwards. On null return `ptr` is left
    ///   intact and still owned by the caller.
    /// - `ptr` is not a foreign / already-released-unmapped base.
    ///
    /// Null `ptr` is always safe (early return).
    #[allow(unsafe_code)] // R6-MS-1/2: `unsafe fn` boundary (caller-pointer contract).
    pub unsafe fn realloc(&mut self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return core::ptr::null_mut();
        }
        // OPT-F / OPT-G: try the in-place fast paths first (Large grow-in-span
        // and Small same-class). The detection logic lives in ONE place —
        // `realloc_inplace_fast_path` — shared with `try_realloc_inplace`
        // (which `HeapCore::realloc` calls so its alloc leg can route through
        // the magazine-aware `HeapCore::alloc`). Keeping a single source of
        // truth here closes the unmarked duplication/divergence hazard flagged
        // in the X-arc retrospective (C2): a bugfix applied to one copy but
        // not the other would silently disagree.
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base(base) {
            // Foreign/unregistered pointer: NOT a legitimate move-leg
            // candidate at the substrate level (see doc comment above and F1
            // in the UB/memory-safety audit). Symmetric with `dealloc`'s
            // foreign-pointer no-op — return null instead of falling through
            // to `self.alloc` + `Node::copy_nonoverlapping(ptr, ..)`, which
            // would read `old_layout.size()` bytes from an address we never
            // registered.
            return core::ptr::null_mut();
        }
        if let Some(p) = self.realloc_inplace_fast_path_known_base(base, ptr, old_layout, new_size)
        {
            return p;
        }
        // In-place fast paths did not apply: alloc a fresh block, copy the
        // preserved prefix, and free the old block.
        //
        // R2-1 (soundness): the move leg copies `old_layout.size().min(
        // new_size)` bytes OUT of `ptr`. `contains_base(base)` proved the
        // segment is ours & mapped, but NOT that the block is as large as
        // `old_layout` claims — and this is a SAFE `pub fn` (no `unsafe`
        // marker), so unlike `GlobalAlloc::realloc` (whose `unsafe` signature
        // makes the caller's `old_layout` a trusted precondition) a bogus
        // `old_layout` (e.g. 8 MiB claimed for a 16-byte block) must not drive
        // an out-of-bounds read. The write side is always safe (`copy <=
        // new_size <= the fresh allocation`); the unsound half is the READ.
        // Reject (return null, `ptr` untouched) when the claimed old size
        // exceeds the segment's actual committed span.
        if old_layout.size() > AllocCore::safe_payload_read_span(base, ptr) {
            return core::ptr::null_mut();
        }
        let new_layout = match Layout::from_size_align(new_size, old_layout.align()) {
            Ok(l) => l,
            Err(_) => return core::ptr::null_mut(),
        };
        let new_ptr = self.alloc(new_layout);
        if new_ptr.is_null() {
            return core::ptr::null_mut();
        }
        let copy = old_layout.size().min(new_size);
        Node::copy_nonoverlapping(ptr, new_ptr, copy);
        // SAFETY: `ptr` is a live own-segment allocation (proven by
        // `contains_base(base)` above) whose true size bounds the move-leg
        // read (`old_layout.size() <= safe_payload_read_span`), made with
        // `old_layout`; the fresh `new_ptr` holds the copied prefix, so
        // freeing the old block once here completes the contract-honouring
        // realloc move leg.
        unsafe { self.dealloc(ptr, old_layout) };
        new_ptr
    }

    /// R2-1 (soundness): the maximum number of bytes starting at `payload`
    /// that lie within the COMMITTED span of the segment at `base`, computed
    /// purely from segment-header metadata — WITHOUT trusting any
    /// caller-supplied `Layout`.
    ///
    /// [`realloc`](Self::realloc) and `HeapCore::realloc` are SAFE `pub fn`s
    /// (no `unsafe` marker), so they must not let a bogus `old_layout.size()`
    /// drive an out-of-bounds read in the move leg's
    /// [`Node::copy_nonoverlapping`]. `contains_base(base)` proves the segment
    /// is OURS and MAPPED, but says nothing about how large the block at
    /// `payload` actually is; this method supplies that missing upper bound.
    ///
    /// For a Large segment the committed span is the header's `span_usable`
    /// (the physical OS reservation, `>=` the logical `large_size`, so all real
    /// data is preserved). For a Small/Primordial segment `span_usable` is
    /// unused (0) — the segment is exactly one `SEGMENT` (4 MiB), fully
    /// committed on reserve — so `SEGMENT` is the bound. In both cases the
    /// result is an upper bound on the bytes that can be read from `payload`
    /// without faulting or escaping the segment's OS allocation; the move legs
    /// reject (`old_layout.size() >` this value) before any copy rather than
    /// reading past the segment.
    ///
    /// # Preconditions
    ///
    /// `base` MUST already be proven to be a live, mapped segment — via
    /// `contains_base(base)` (own-segment legs) or `magic_at(base) ==
    /// SEGMENT_MAGIC` (the cross-heap foreign leg under `alloc-xthread`).
    /// This method reads `kind`/`span_usable` header fields at `base`, which
    /// is only sound for a mapped segment.
    #[inline]
    pub(crate) fn safe_payload_read_span(base: *mut u8, payload: *mut u8) -> usize {
        let seg_span = if SegmentHeader::kind_at(base) == SegmentKind::Large {
            SegmentHeader::span_usable_at(base)
        } else {
            // Small/Primordial: `span_usable` is 0 (inert — see
            // `SegmentHeader::small`); the segment is exactly one SEGMENT,
            // fully committed on reserve.
            os::SEGMENT
        };
        let off = (payload as usize).wrapping_sub(base as usize);
        seg_span.saturating_sub(off)
    }

    /// Single source of truth for the OPT-F / OPT-G in-place realloc fast
    /// paths. Returns `Some(ptr)` (the SAME pointer, unchanged or with its
    /// Large header's `large_size` updated in place) when an in-place resize
    /// is possible, `None` otherwise. Does NOT fall through to `self.alloc` —
    /// callers own that decision (the substrate-level [`realloc`](Self::realloc)
    /// calls `self.alloc` + copy + `self.dealloc`; the registry-level
    /// [`try_realloc_inplace_known_base`](Self::try_realloc_inplace_known_base) is consumed by `HeapCore::realloc`, which routes
    /// its alloc leg through the magazine-aware `HeapCore::alloc`).
    ///
    /// Both callers share these detection predicates so a bugfix applied to
    /// one cannot silently fail to reach the other (the X-arc retrospective
    /// C2 hazard).
    ///
    /// # OPT-G — Large→Large in-place grow
    ///
    /// Preconditions (all must hold to take the fast path):
    ///   1. The pointer lives in one of OUR segments (registered in the
    ///      table).
    ///   2. The segment kind is `Large` (dedicated single-allocation
    ///      segment). Huge is excluded conservatively — only Large segments
    ///      have a verified committed-span guarantee via `span_usable`.
    ///   3. GROW or SAME size only (clamped: `new_eff >= old_eff`). A
    ///      shrink falls through to the slow path, which reclaims RSS by
    ///      moving the payload to a smaller segment/class.
    ///   4. The grown payload fits the committed span:
    ///      `payload_offset + new_eff <= span_usable` (checked add to
    ///      guard against usize wrap on pathological sizes).
    ///
    /// MIN_BLOCK clamping: the alloc path clamps every request to
    /// `MIN_BLOCK` before storing `large_size` in the header. The #138
    /// cross-thread consistency check (`large_layout_consistent`) compares
    /// the header value against `layout_size.max(MIN_BLOCK)`. We must
    /// clamp identically here so a later cross-thread free does not see
    /// `raw != clamped` and silently drop the free — permanently leaking
    /// the segment + its SegmentTable slot (#114/#130 class).
    ///
    /// Soundness:
    ///   (a) `dealloc` routes Large frees by `SegmentHeader::kind_at(base)`,
    ///       NOT by the passed layout. A grown-in-place block stays a Large
    ///       segment, so `dealloc(ptr, new_layout)` frees the whole segment
    ///       correctly regardless of `new_size`.
    ///   (b) `crates/vmem` reserves large segments with
    ///       `VirtualAlloc(MEM_RESERVE|MEM_COMMIT)` over the WHOLE span;
    ///       the large-cache keeps pages committed on deposit. The entire
    ///       `span_usable` region is committed and writable — growing into
    ///       it cannot fault.
    ///   (c) Large reservations round UP to whole SEGMENT (4 MiB) multiples,
    ///       so e.g. a 512 KiB large alloc owns a full 4 MiB committed span
    ///       and can grow to ~4 MiB in place.
    ///
    /// When all hold: update the header's `large_size` to the CLAMPED
    /// `new_eff` and return the SAME pointer. The grown tail is
    /// uninitialised (matching `GlobalAlloc`).
    ///
    /// # OPT-F — Small→Small same-class in-place
    ///
    /// Preconditions (all must hold to take the fast path):
    ///   1. The pointer lives in one of OUR segments (registered in the table).
    ///   2. The segment kind is Small or Primordial (has a BinTable / class).
    ///   3. Both the old layout and the new size classify as Small (not Large).
    ///   4. new_class_idx == old_class_idx → the block stays in EXACTLY the
    ///      same size class.
    ///
    /// Why `==` and NOT `<=` (the subtle correctness point): a caller that
    /// reallocs `ptr` then later frees it MUST, per the `GlobalAlloc`
    /// contract, pass the NEW layout (`new_size`, same align) to `dealloc`.
    /// Our `dealloc` (post-#114) derives the block's size class from that
    /// layout alone — NOT from where the block physically sits. A block is
    /// carved at an offset that is a multiple of ITS class's `block_size`;
    /// that offset is NOT necessarily a multiple of a *smaller* class's
    /// `block_size` (the class sizes are not divisors of one another —
    /// e.g. the 132464-byte class is not a multiple of the 4096-byte
    /// class). So if we returned `ptr` unchanged for a shrink that crosses
    /// into a smaller class (`new_class < old_class`), the eventual
    /// `dealloc` would push this block's offset onto the SMALLER class's
    /// free list, where the offset is misaligned — corrupting that free
    /// list so a later `alloc` from it returns a mis-placed pointer. This
    /// was latent until task B1 added page-aligned classes (512..16384):
    /// before B1 the shrink target for a page-aligned request classified
    /// to `None` (Large) and never hit this path, so the bug never
    /// manifested. `==` keeps the block in its own class, where the
    /// carved offset is valid for the free list `dealloc` will use.
    ///
    /// When the class matches we return `ptr` unchanged. No copy (the
    /// block has not moved), no dealloc (we reuse it); the alloc-bitmap
    /// and live-count are unaffected (the block stays live).
    ///
    /// A cross-class shrink (`new_class < old_class`) falls through to the
    /// slow path (alloc new block in the smaller class + copy + dealloc
    /// old block in its own class) — correct, just not zero-copy. Growth
    /// (`new_class > old_class`) and Large on either side also fall
    /// through.
    /// In-place realloc fast paths for a pointer whose segment base has already
    /// been proven live in this `AllocCore`'s table. This is the same logic as
    /// [`realloc_inplace_fast_path`](Self::realloc_inplace_fast_path), split so
    /// `HeapCore::realloc` can reuse its own `contains_base(base)` proof instead
    /// of probing the segment table again.
    #[inline]
    fn realloc_inplace_fast_path_known_base(
        &mut self,
        base: *mut u8,
        ptr: *mut u8,
        old_layout: Layout,
        new_size: usize,
    ) -> Option<*mut u8> {
        assert!(
            self.table.contains_base_ro(base),
            "known-base realloc called for a segment not owned by this core"
        );
        let kind = SegmentHeader::kind_at(base);
        // OPT-G: Large→Large in-place grow.
        if kind == SegmentKind::Large {
            let old_eff = old_layout.size().max(super::size_classes::MIN_BLOCK);
            let new_eff = new_size.max(super::size_classes::MIN_BLOCK);
            if new_eff >= old_eff {
                let payload_off = ptr as usize - base as usize;
                let span_usable = SegmentHeader::span_usable_at(base);
                if let Some(end) = payload_off.checked_add(new_eff) {
                    if end <= span_usable {
                        SegmentHeader::set_large_size_at(base, new_eff);
                        return Some(ptr);
                    }
                }
            }
            return None;
        }
        // OPT-F: Small→Small same-class in-place.
        if matches!(kind, SegmentKind::Small | SegmentKind::Primordial) {
            let old_size = old_layout.size().max(super::size_classes::MIN_BLOCK);
            let align = old_layout.align();
            let clamped_new = new_size.max(super::size_classes::MIN_BLOCK);
            if let (Some(old_class), Some(new_class)) = (
                super::size_classes::SizeClasses::class_for(old_size, align),
                super::size_classes::SizeClasses::class_for(clamped_new, align),
            ) {
                if new_class == old_class {
                    return Some(ptr);
                }
            }
        }
        None
    }

    /// Try the two in-place realloc fast paths (Large grow-in-span, Small same-class), but the
    /// caller has already proven `base` is live in this core's segment table.
    /// Used by `HeapCore::realloc` to avoid a duplicate `contains_base` probe
    /// after its own ownership check.
    #[cfg(feature = "alloc-global")]
    pub(crate) fn try_realloc_inplace_known_base(
        &mut self,
        base: *mut u8,
        ptr: *mut u8,
        old_layout: Layout,
        new_size: usize,
    ) -> Option<*mut u8> {
        if ptr.is_null() {
            return None;
        }
        self.realloc_inplace_fast_path_known_base(base, ptr, old_layout, new_size)
    }

    /// Iterate over all registered segment bases (read-only). A registry-walk
    /// primitive used by cross-thread-free routing and (historically) the
    /// Phase 12.4 abandonment walk (now removed — task #97 / R4-5).
    ///
    /// `#[doc(hidden)]` (task #136): `AllocCore` itself is re-exported as
    /// stable public API (unlike most of `alloc_core`), but this iterator is
    /// an internal registry-walk primitive, not something an external
    /// caller is expected to use directly — it leaked into the visible
    /// public surface only because `AllocCore` is public. Kept `pub` (not
    /// `pub(crate)`) because `registry::heap_core::HeapCore::segment_bases`
    /// delegates to it across the crate boundary between `alloc_core` and
    /// `registry`.
    #[cfg(any(feature = "alloc-global", feature = "alloc-xthread"))]
    #[doc(hidden)]
    pub fn segment_bases(&self) -> impl Iterator<Item = *mut u8> {
        self.table.bases()
    }

    /// O(1) membership test: is `base` one of THIS substrate's registered,
    /// LIVE (non-NULL) segment bases? Thin delegation to
    /// `SegmentTable::contains_base` (the OPT-B open-addressing hash table).
    ///
    /// Task #135 (Part 2/3): exposes the table's existing O(1) check at the
    /// `AllocCore` level so `HeapCore::realloc` (own-segment ownership test)
    /// and `HeapCore::dealloc_routing` (M2 hardening — see its doc comment)
    /// no longer need to fall back to the O(count) `segment_bases().any(...)`
    /// scan.
    ///
    /// Gated on `alloc-global` only (not also `alloc-xthread`): both call
    /// sites live in `registry::heap_core::HeapCore`, and the entire
    /// `registry` module is itself `#[cfg(feature = "alloc-global")]`-gated
    /// at the crate root (`src/lib.rs`) — `alloc-xthread` alone (without
    /// `alloc-global`) does not compile `HeapCore` at all, so a wider gate
    /// here would leave this method genuinely unused under that combination.
    #[cfg(feature = "alloc-global")]
    #[inline(always)]
    pub(crate) fn contains_base(&mut self, base: *mut u8) -> bool {
        self.table.contains_base(base)
    }

    /// RAD-4b (task #72): the current small segment's base, for callers
    /// outside this module that need to pass it into
    /// [`reclaim_offset`](Self::reclaim_offset) /
    /// [`reclaim_offset_checked`](Self::reclaim_offset_checked) (both of
    /// which take `small_cur` as a plain argument rather than reading `self`,
    /// since they are associate functions, not methods — see their doc
    /// comments). `small_cur` itself is `pub(super)` (module-private); this
    /// thin `pub(crate)` accessor is the sole reason `HeapCore::
    /// drain_heap_overflow` (`src/registry/heap_core.rs`, `registry` module,
    /// outside `alloc_core`) needs to exist.
    #[cfg(feature = "alloc-xthread")]
    #[must_use]
    pub(crate) fn small_cur(&self) -> *mut u8 {
        self.small_cur
    }

    // Э4 (task #145) "classify once" wrappers `alloc_small_class` /
    // `dealloc_small_class` were RETIRED in P3 (task #147): their only callers
    // were the P7 alloc-side and dealloc-side bulk bypasses, both removed here.
    // The classify-once win survives where it still has a live caller
    // (`HeapCore::dealloc_own_thread` already resolves the class once); these
    // one-line pass-throughs are trivially re-addable if a future path needs
    // a class-resolved single-block primitive again.

    // -----------------------------------------------------------------------
    // Internals — the safe Cartographer. All raw memory touches go through
    // `Node`; no `Vec`/`Box`/`HashSet`/`std::alloc`.
    // -----------------------------------------------------------------------

    /// Classify a `(size, align)` request as Small or Large.
    #[inline]
    pub(super) fn classify(size: usize, align: usize) -> AllocKind {
        match SizeClasses::class_for(size, align) {
            Some(class_idx) => AllocKind::Small { class_idx },
            None => AllocKind::Large,
        }
    }
}

/// # ⚠️ Quiescence pin (UBFIX-12 / L-8, 0.3.0) — read before adding any
/// `Sync`/cross-thread capability to `AllocCore`, or before making registry
/// heaps droppable
///
/// This `drop` walks every segment in `self.table` and releases its OS
/// reservation (`os::release_segment`) unconditionally — it does NOT perform
/// any handshake to prove no OTHER thread is concurrently pushing onto one of
/// these segments' cross-thread remote-free rings
/// ([`RemoteFreeRing`](super::remote_free_ring::RemoteFreeRing), the
/// `alloc-xthread` per-segment MPSC the segment header's `owner_thread_free`
/// stamp routes into) before unmapping. If such a push raced this `drop`, it
/// would write into memory that is either about to be, or has already been,
/// unmapped — a use-after-free / wild write on the remote thread's side.
///
/// **Today this is reachable-but-moot, not a live bug**, for two independent
/// reasons, EITHER of which is already sufficient on its own:
///
/// 1. **Registry heaps never reach this `drop`.** The `HeapRegistry`/
///    `HeapCore` substrate that `SeferAlloc`/TLS actually use lives for the
///    entire process (`HeapCore::new`'s `AllocCore` is never dropped by
///    `recycle` — `recycle` only flips the slot's state and pushes it onto
///    `free_slots` for reuse; see `HeapRegistry::recycle`). So the ONLY way
///    to reach `AllocCore::drop` today is constructing a STANDALONE
///    `AllocCore` directly (`AllocCore::new`/`::default`, bypassing the
///    registry entirely) and letting it go out of scope.
/// 2. **A standalone `AllocCore` cannot be shared across threads in the
///    first place.** `AllocCore` carries raw pointers (`table`, `small_cur`,
///    `large_cache` entries) and has no `unsafe impl Sync for AllocCore`
///    anywhere in this crate (verified by grep at the time of writing) — so
///    it is `!Sync` by the ordinary auto-trait rules, and a `&AllocCore`
///    cannot be handed to another thread to begin with. Without a live
///    `&AllocCore` on some OTHER thread, nothing can call the remote-free
///    routing that would push onto a segment's `RemoteFreeRing` while this
///    thread's `drop` is unmapping it — the race this note warns about has
///    no way to be constructed against a standalone `AllocCore` today.
///
/// Both conditions must be independently defeated before this becomes live:
/// (a) some future change makes registry heaps droppable (e.g. a
/// decommit-when-empty or heap-teardown policy that actually frees a
/// `HeapCore`'s `AllocCore`, not just recycles the slot), OR (b) some future
/// change adds `unsafe impl Sync for AllocCore` (or otherwise exposes a
/// standalone `AllocCore` for cross-thread sharing outside the registry).
/// If EITHER lands, this `drop` needs a quiescence handshake — e.g. draining
/// every segment's `RemoteFreeRing` under a happens-before edge that rules
/// out a concurrent remote push, or otherwise proving no other thread holds
/// a reference capable of routing a free into a segment this `drop` is about
/// to unmap — before it is safe to release segments unconditionally as it
/// does now. This note is the load-bearing reminder to add that handshake at
/// that time; do not remove or weaken it while working on (a) or (b) above.
impl Drop for AllocCore {
    fn drop(&mut self) {
        // OPT-E (alloc-decommit): release any large segments held in the
        // free-cache BEFORE walking the segment table. The cached entries are
        // NOT in the table (they were unregistered on deposit), so the normal
        // `table.bases()` walk below won't see them. We must release them
        // explicitly here or they would leak.
        #[cfg(feature = "alloc-decommit")]
        for slot in &mut self.large_cache {
            if let Some(cached) = slot.take() {
                os::release_segment(cached.reservation, cached.reservation_len);
            }
        }

        // Collect every live segment's `(reservation, reservation_len)` into a
        // fixed-size stack array FIRST, then free them all. We must NOT free
        // the primordial segment while still reading the registry — the
        // registry lives IN the primordial's payload, so freeing it would
        // unmap the array we're iterating over. Collecting up front (into a
        // stack array, no global-allocator involvement) breaks that aliasing.
        //
        // `self.table.bases()` already filters NULL (recycled) slots — those
        // segments were released by `recycle()` during their decommit cycle and
        // must NOT be freed again. Only non-NULL (live) segments are collected
        // and freed here.
        //
        // The array is bounded by MAX_SEGMENTS (1024 × 16 B = 16 KiB stack —
        // fine; a deeply-nested drop chain would be the only concern, and
        // AllocCore is a top-level owner).
        let mut to_free: [(*mut u8, usize); super::segment_table::MAX_SEGMENTS] =
            [(core::ptr::null_mut(), 0usize); super::segment_table::MAX_SEGMENTS];
        let mut n = 0usize;
        for base in self.table.bases() {
            if n >= super::segment_table::MAX_SEGMENTS {
                break;
            }
            let hdr = SegmentHeader::read_at(base);
            // Every registered segment has a valid reservation recorded (set
            // at register-time). We free them all — including large segments
            // whose magic was zeroed by `dealloc` (they are still mapped and
            // still carry the reservation info in their header).
            to_free[n] = (hdr.reservation, hdr.reservation_len);
            n += 1;
        }
        // Now free every collected reservation. The primordial (whose payload
        // hosts the registry) is freed here alongside the rest — safe, because
        // we no longer read the registry.
        for &(reservation, reservation_len) in &to_free[..n] {
            os::release_segment(reservation, reservation_len);
        }
    }
}

// NOTE: `AllocCore` is intentionally NOT `Send` (nor `Sync`) in Phase 8.
// Phase 8 is single-threaded; `Send` is not needed. Phase 9 (per-thread
// heaps) will add `Send` at the heap layer (the segment substrate is
// `Send`-capable, but the claim belongs to the layer that owns the threading
// discipline, not the substrate itself). Adding it here would require an
// `unsafe impl` that has no place outside the two named `unsafe` seams.

/// `base + off` as `*mut u8`, routed through the `node` seam. The Cartographer
/// only ever passes offsets derived from the fixed [`SegLayout`] or the bump
/// cursor (both bounded by `SEGMENT`).
pub(super) fn base_add(base: *mut u8, off: usize) -> *mut u8 {
    Node::offset(base, off)
}
