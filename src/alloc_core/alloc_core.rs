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
//!   — the single-threaded allocator entry points. `dealloc`/`realloc` are
//!   `unsafe` per the `GlobalAlloc` contract (the caller must pass a valid
//!   prior pointer/layout); they never panic and never recurse.
//!
//! ## Single-threaded
//!
//! Phase 8 is single-threaded (correctness before concurrency — §5 P8).
//! Per-thread heaps + lock-free cross-thread free are Phase 9/10. `AllocCore`
//! is `Send` (it owns its segments, which are `Send`) but NOT `Sync`.

use core::alloc::Layout;
use core::ptr::NonNull;

use super::bootstrap;
use super::node::{Node, NODE_SIZE};
#[cfg(feature = "numa-aware")]
use super::numa;
#[cfg(not(feature = "numa-aware"))]
use super::os::Segment;
use super::os::{self, SEGMENT};
use super::segment_header::{
    align_up, BinTable, Layout as SegLayout, PageMap, SegmentHeader, SegmentKind, SegmentMeta,
    FREE_LIST_NULL,
};
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
const LARGE_CACHE_SLOTS: usize = 8;

/// Size-ratio bound: we only reuse a cached entry if its usable_size is at most
/// `needed * LARGE_CACHE_SIZE_FACTOR`. Without this a very large cached segment
/// would be permanently reused for every small large-request — wasting RSS
/// during the cache lifetime. Kept at 2 (as before): a 2× size tolerance is
/// tight enough to avoid gross RSS waste while still allowing minor rounding
/// differences between consecutive large allocations of "the same" size.
#[cfg(feature = "alloc-decommit")]
const LARGE_CACHE_SIZE_FACTOR: usize = 2;

/// The three large-cache operating modes.
///
/// `Lazy` is the default; the others are reserved for a future background
/// scavenger thread (not yet implemented — they currently behave identically
/// to `Lazy`). Set via [`LargeCacheConfig::mode`].
///
/// [`LargeCacheConfig::mode`]: super::large_cache_config::LargeCacheConfig::mode
#[cfg(feature = "alloc-decommit")]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum LargeCacheMode {
    /// Default: Phase 2 lazy decay only. No background thread. Identical to
    /// pre-Phase-3 behaviour; all existing tests continue to pass unchanged.
    Lazy,
    /// Reserved for a future background scavenger thread that visits idle
    /// shards and calls `run_decay_step()` on their large-caches. Currently
    /// behaves identically to `Lazy`.
    Background,
    /// Alias for `Background`. Reserved for the future distinction "lazy hooks
    /// AND background thread active" vs "background thread only".
    Both,
}

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
struct LargeCacheDecayConfig {
    /// Fraction of the excess to release per tick, in basis points.
    /// 1000 = 10%, 5000 = 50%, 10000 = 100%.
    decay_rate_bp: u32,
    /// Minimum wall-clock interval between consecutive decay ticks.
    decay_interval: core::time::Duration,
    /// Target cache size in bytes. The "excess" above this level is subject
    /// to decay. On Phase 2 we treat `live_bytes = 0`; the target is just
    /// `headroom_bytes`. A future phase can add explicit live-count tracking.
    headroom_bytes: usize,
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
struct CachedLarge {
    /// Start of the original OS reservation.
    reservation: *mut u8,
    /// Total size of the OS reservation.
    reservation_len: usize,
    /// SEGMENT-aligned base of the segment (the "usable" start).
    base: *mut u8,
    /// The `usable` bytes this reservation covers — `n_segments * SEGMENT` for
    /// the original allocation. Used to match incoming requests.
    usable_size: usize,
    /// Insertion sequence number (task D1). Monotonically increasing per
    /// deposit, taken from `AllocCore::large_cache_seq`. The true FIFO-oldest
    /// occupied slot is the one with the SMALLEST `seq` — NOT necessarily the
    /// lowest array index once `LARGE_CACHE_SLOTS > 2` (with more than two
    /// slots, hits and re-deposits no longer fill/empty strictly in index
    /// order, so "lowest index = oldest" stops holding; see D1 in
    /// `docs/checkpoints` history). This field restores a correct FIFO
    /// ordering independent of slot count.
    seq: u64,
}

/// TEST-ONLY (Phase 35): process-wide M6-decommit invocation counter. Bumped in
/// [`AllocCore::decommit_empty_segment`]; read by the soak test via
/// [`AllocCore::dbg_decommit_count`]. Diagnostic only (relaxed).
#[cfg(feature = "alloc-decommit")]
static DECOMMIT_CALLS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

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
type LargeCacheHitCounter = core::sync::atomic::AtomicU64;

/// A single-threaded allocator over the self-hosted segment substrate.
///
/// Owns its segments (the primordial + any additionally-reserved small or
/// large/huge segments). The registry of live segments lives in the
/// primordial segment's payload (self-hosted) — there is NO `Vec<Segment>`:
/// `AllocCore::drop` walks the registry and frees every reservation through
/// the `os` seam.
pub struct AllocCore {
    /// The primordial segment registry (self-hosted in segment 0's payload).
    table: SegmentTable,
    /// Metadata view of the "current" small segment — the one whose bump
    /// cursor and free lists new small allocations draw from. When it fills,
    /// [`alloc_small`] reserves a fresh small segment and switches to it.
    ///
    /// [`alloc_small`]: Self::alloc_small
    small_cur: *mut u8,
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
    large_cache: [Option<CachedLarge>; LARGE_CACHE_SLOTS],

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
    large_cache_budget_bytes: Option<usize>,

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
    large_cache_used_bytes: usize,

    /// Monotonic insertion-sequence counter for `large_cache` deposits (task
    /// D1). Each deposit stamps the current value into `CachedLarge::seq` and
    /// then increments this counter. FIFO eviction picks the occupied slot
    /// with the smallest `seq` — the true "oldest" entry — rather than
    /// assuming index order, which only happened to hold for the old
    /// `LARGE_CACHE_SLOTS == 2` minimal implementation.
    #[cfg(feature = "alloc-decommit")]
    large_cache_seq: u64,

    // ── Phase 2 — lazy decay ─────────────────────────────────────────────────
    /// Immutable decay parameters: rate, interval, headroom. Set once at
    /// `AllocCore::new_with_config` from a `LargeCacheConfig`; overridable in
    /// tests via `dbg_set_decay_config`.
    #[cfg(feature = "alloc-decommit")]
    decay_config: LargeCacheDecayConfig,

    /// Wall-clock time of the last decay tick. `None` = never ticked yet (the
    /// first call to `maybe_decay_large_cache` primes the timer without
    /// releasing anything). Stored as `Option<std::time::Instant>` so the very
    /// first call does not accidentally release half the cache at process start.
    #[cfg(feature = "alloc-decommit")]
    last_decay_tick: Option<std::time::Instant>,

    // ── Phase 3 — cache operating mode ───────────────────────────────────────
    /// The large-cache operating mode, set once at `AllocCore::new_with_config`
    /// from a `LargeCacheConfig`. Stored for diagnostic/test access and as the
    /// anchor for future scavenger-thread wiring.
    ///
    /// `Lazy` (default): Phase 2 lazy decay, no background thread.
    /// `Background` / `Both`: reserved for the future background scavenger.
    #[cfg(feature = "alloc-decommit")]
    large_cache_mode: LargeCacheMode,

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
    large_cache_hits: LargeCacheHitCounter,

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
    large_cache_hits_sink: Option<&'static LargeCacheHitCounter>,
}

impl AllocCore {
    /// Bootstrap the allocator using default large-cache configuration.
    ///
    /// Equivalent to `AllocCore::new_with_config(LargeCacheConfig::DEFAULT)`.
    /// Returns `None` only if the OS refuses the primordial reservation (OOM at
    /// startup).
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
    /// environment variables:
    ///
    /// ```rust
    /// # #[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
    /// # {
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
    /// # }
    /// ```
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    #[inline]
    pub fn new_with_config(config: super::large_cache_config::LargeCacheConfig) -> Option<Self> {
        let mut core = Self::new_inner()?;
        core.large_cache_budget_bytes = config.resolved_budget_bytes();
        core.decay_config = LargeCacheDecayConfig::from_config(&config);
        core.large_cache_mode = config.resolved_mode();
        Some(core)
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
    /// This entry point is **safe**: a foreign pointer (not one of ours) or a
    /// double-free is a **no-op** (M2 — never UB, never corrupts the
    /// allocator), matching the defensive contract the Phase 11 `GlobalAlloc`
    /// face will require. A well-behaved caller passes a valid prior
    /// allocation of `layout`; the safety here is defence-in-depth, not a
    /// licence to free garbage.
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
    #[inline]
    pub fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        let base = os::segment_base_of_ptr(ptr);
        // Foreign-pointer check: if the computed segment base is NOT one of our
        // registered segments, this pointer is not one of ours — no-op (do not
        // touch foreign memory, do not even read a header that may be unmapped).
        if !self.table.contains_base(base) {
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
                        let mut hdr_zero = stale;
                        hdr_zero.magic = 0;
                        Node::write_struct(base as *mut SegmentHeader, hdr_zero);
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
        }
    }

    /// Reclaim a cross-thread-freed block identified by its **segment-relative
    /// offset** back into its owning segment's `BinTable`. This is the
    /// non-intrusive reclaim path (Variant-2): the block's offset arrived via
    /// the segment's `RemoteFreeRing` (the block's own bytes were never touched
    /// by the cross-thread freer), so we turn the offset back into a pointer
    /// and route it through the same `dealloc_small` path as an own-thread free.
    ///
    /// **Self-less** (an associated function, not `&mut self`): it touches ONLY
    /// segment metadata reachable from `base` via `SegmentMeta` (header, page
    /// map, bin table) — never the `AllocCore` registry. This lets the
    /// `find_segment_with_free` drain call it while iterating `&self.table`
    /// without an aliasing conflict, and keeps the single-consumer reclaim
    /// uniform with the own-thread path. The caller MUST be the segment's sole
    /// `BinTable` writer (the slot's owner) — the same invariant `dealloc_small`
    /// relies on.
    ///
    /// **Class is carried in the ring entry, NOT derived from `page_map`.** The
    /// segment has ONE bump cursor shared by all size classes, so a page can
    /// host blocks of several classes (the page-dedication rule records only
    /// the FIRST class to touch a page). Deriving the class from `page_map`
    /// therefore returns the wrong class for any later block of a different
    /// class in the same page, and reclaim would link the free-list `next` at a
    /// mis-aligned address, corrupting a neighbour (the §13 root cause). The
    /// cross-thread freer has the original `Layout`, so it packs
    /// `class_idx = classify(layout)` into the high bits of the ring entry;
    /// here we unpack it and use it directly.
    ///
    /// `packed` layout: `off = packed & OFF_MASK` (low 22 bits, since
    /// `SEGMENT = 1 << 22` so every offset is `< 2^22`), `class_idx = packed >>
    /// OFF_BITS` (high bits; `SMALL_CLASS_COUNT = 49 ≪ 2^10`, so it fits).
    ///
    /// Safe: a foreign segment (magic mismatch), a large segment, or an offset
    /// that is not `block_size`-aligned is a no-op (defence-in-depth). Applies
    /// the M2 double-free guard.
    /// Task #164: variant of `reclaim_offset` that consults an `is_in_magazine`
    /// predicate AFTER all existing guards and BEFORE `write_next`. If the
    /// predicate returns `true` (block is magazine-resident), the ring entry is
    /// a duplicate free → return `false` without linking (no `write_next`, no
    /// `mark_free`, no `dec_live`). The magazine copy remains the sole canonical
    /// reference. Closes the in-magazine leg of the ring↔magazine cross-thread
    /// double-free residual (task #164, §5 fallback (a)-closure).
    ///
    /// `F` receives `(ptr: *mut u8, class_idx: usize)` and must return `true`
    /// if the block is currently resident in the owner's magazine for that class.
    #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))]
    #[cfg_attr(not(feature = "alloc-decommit"), allow(unused_variables))]
    pub(crate) fn reclaim_offset_checked<F: Fn(*mut u8, usize) -> bool>(
        base: *mut u8,
        packed: u32,
        small_cur: *mut u8,
        is_in_magazine: &F,
    ) -> bool {
        // X7 Ф3 (task #191): under `hardened` the ring entry was packed with
        // `pack_entry_hardened` (touch (b) in `dealloc_routing`), so it must be
        // unpacked with the matching `unpack_entry_hardened` — which also
        // recovers the stamped generation byte. Under non-hardened the entry
        // was packed with the untouched `pack_entry`, so the unpack is the
        // untouched `unpack_entry` (byte-identical to the pre-X7 code,
        // verified by construction — the `cfg(not)` branch IS the pre-existing
        // code). Sibling-block discipline mirrors `Layout::small_meta_end()`.
        //
        // `stamped_gen` is consulted AFTER all existing guards (the load-bearing
        // ordering: magic/kind/align/bump/is_free/X2-magazine, THEN gen) and
        // BEFORE `write_next` — see the comment below.
        #[cfg(feature = "hardened")]
        let (stamped_gen, class_idx_raw, off_raw) =
            super::remote_free_ring::unpack_entry_hardened(packed);
        #[cfg(not(feature = "hardened"))]
        let (off_raw, class_idx_raw) = super::remote_free_ring::unpack_entry(packed);
        let off = off_raw as usize;
        let class_idx = class_idx_raw as usize;
        if class_idx >= super::size_classes::SMALL_CLASS_COUNT {
            return false;
        }
        let ptr = Node::deref(base, off);
        if SegmentHeader::magic_at(base) != super::segment_header::SEGMENT_MAGIC {
            return false;
        }
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return false;
        }
        let bs = SizeClasses::block_size(class_idx) as u32;
        if !(off as u32).is_multiple_of(bs) {
            return false;
        }
        let meta = SegmentMeta::new(base);
        #[cfg(feature = "alloc-decommit")]
        if off >= meta.bump_of() {
            return false;
        }
        let mut bt = meta.bin_table();
        let mut bm = meta.alloc_bitmap();
        if bm.is_free(off as u32) {
            return false;
        }
        // Task #164 (§5 fallback (a)-closure): the block's bitmap reads
        // "allocated". Before linking it onto the freelist (which would
        // clobber its word0 via `write_next`), consult the magazine. If the
        // block IS magazine-resident, this ring entry is the duplicate leg of
        // a cross-thread double-free — DROP it (keep the magazine copy, no
        // link, no mark_free, no dec_live).
        if is_in_magazine(ptr, class_idx) {
            return false;
        }
        // X7 Ф3 (task #191) touch (c): the GENERATIONAL guard. Under
        // `hardened`, AFTER all existing guards (magic/kind/align/bump/
        // is_free/X2-magazine — that ordering is load-bearing, do NOT reorder)
        // and BEFORE `write_next`/`mark_free`: compare the generation stamped
        // in the ring note (touch (b)) against the block's CURRENT generation.
        // A mismatch means the block has been RE-ISSUED since the note was
        // stamped (its life counter advanced via `bump_gen` at the issue pop),
        // so honouring this note would double-free / corrupt the CURRENT
        // occupant — DROP it (return false: no link, no mark_free, no
        // dec_live), exactly like the `is_in_magazine` drop above. This closes
        // the re-issue-before-drain leg (residual leg 3): a note that
        // "survived" a re-issue in the ring is identified by its stale
        // generation and discarded. Compiled ONLY under `hardened`; under
        // non-hardened this block is absent (byte-identical to pre-X7).
        //
        // Wrap 1/256 (X7 §2.5): after 256 re-issues-without-drain a stale note
        // coincides with the current generation and is wrongly honoured — a
        // probabilistic residual accepted by design, pinned by a Ф5 boundary
        // test, not fixable without doubling the ring footprint.
        #[cfg(feature = "hardened")]
        {
            let current_gen =
                super::segment_header::gen_at(base, off);
            if stamped_gen != current_gen {
                return false;
            }
        }
        let block_nn = match NonNull::new(ptr) {
            Some(nn) => nn,
            None => return false,
        };
        let old_head = bt.head(class_idx);
        let old_head_ptr = if old_head == FREE_LIST_NULL {
            core::ptr::null_mut()
        } else {
            Node::deref(base, old_head as usize)
        };
        Node::write_next(block_nn, old_head_ptr);
        bt.set_head(class_idx, off as u32);
        bm.mark_free(off as u32);
        #[cfg(feature = "alloc-decommit")]
        {
            Self::dec_live_and_maybe_decommit(base, small_cur)
        }
        #[cfg(not(feature = "alloc-decommit"))]
        false
    }

    #[cfg(feature = "alloc-xthread")]
    // `small_cur` is consumed only by the `alloc-decommit` dec-then-decommit
    // step; without that feature the reclaim path does no live-count bookkeeping.
    // Under fastbin, `reclaim_offset_checked` is used instead (dead_code expected).
    #[cfg_attr(feature = "fastbin", allow(dead_code))]
    #[cfg_attr(not(feature = "alloc-decommit"), allow(unused_variables))]
    pub(crate) fn reclaim_offset(base: *mut u8, packed: u32, small_cur: *mut u8) -> bool {
        // Unpack the offset and the class the cross-thread freer stamped.
        let (off, class_idx) = super::remote_free_ring::unpack_entry(packed);
        let off = off as usize;
        let class_idx = class_idx as usize;
        // Contract (see this fn's docs: "defence-in-depth against a garbled ring
        // value — no abort, just skip"): the ring entry's class field physically
        // carries 10 bits (0..1023), but only `SMALL_CLASS_COUNT` classes exist.
        // A garbled entry (e.g. a user heap-overflow writing into this segment's
        // metadata region) can present `class_idx >= SMALL_CLASS_COUNT`, which
        // would index `SIZE_CLASS_TABLE` out of bounds in `block_size` below and
        // panic inside the global allocator → process abort. Bounds-check FIRST
        // and no-op (return the skip signal) instead, honouring the no-panic
        // alloc-path discipline.
        if class_idx >= super::size_classes::SMALL_CLASS_COUNT {
            return false;
        }
        let ptr = Node::deref(base, off);
        // Field-specific reads: this runs on the Owner's alloc path
        // (find_segment_with_free's lazy ring drain), concurrent with a
        // Remote's `dealloc_routing` field reads. A full-struct
        // `SegmentHeader::read_at` here would race them; reading individual
        // fields via their offsets touches bytes disjoint from any racing
        // writer, so there is no data race.
        if SegmentHeader::magic_at(base) != super::segment_header::SEGMENT_MAGIC {
            return false;
        }
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return false;
        }
        // Sanity: the offset must be a whole number of `block_size` units. carve
        // aligns the bump to `block_size`, so a real block offset is always a
        // multiple of its class's block_size. A mis-aligned offset would write
        // the free-list `next` into the middle of a block — the §13 corruption.
        // This never fires for a correctly-packed entry; it is defence-in-depth
        // against a garbled ring value (no abort — just skip, matching the
        // defensive `dealloc` contract).
        let bs = SizeClasses::block_size(class_idx) as u32;
        if !(off as u32).is_multiple_of(bs) {
            return false;
        }
        let meta = SegmentMeta::new(base);
        // Phase 35 (M6 decommit) — the STALE-RING-INTO-DECOMMITTED-SEGMENT guard.
        // When a segment empties it is decommitted AND reset: its `bump` returns
        // to `small_meta_end()` and its alloc bitmap is zeroed. A ring entry that
        // arrives (or lingers) for an offset in the now-decommitted payload would
        // pass the bitmap `is_free` check (the reset cleared every bit), and the
        // reclaim below would `write_next` into a DECOMMITTED page — a UAF / write
        // to unmapped memory. The bump guard closes this: a real, currently-carved
        // block always has `off < bump`; an offset `>= bump` is either uncarved or
        // (post-reset) in the decommitted region — no-op, never touch the page.
        // This is the concrete realization of design §1.3 ("reclaim does a no-op
        // BEFORE touching the block on a stale entry") for the reset bitmap. The
        // owner is the sole `bump` writer, and reclaim runs owner-side, so this
        // field read is consistent (no concurrent bump write). Owner-only, so
        // gated to the feature that resets the bump.
        #[cfg(feature = "alloc-decommit")]
        if off >= meta.bump_of() {
            return false;
        }
        // Inline of `dealloc_small` (self-less): double-free guard + push to
        // BinTable. We cannot call the `&mut self` method from here (this fn is
        // an associated function), so we replicate the body. The replication is
        // small and the invariant is identical.
        let mut bt = meta.bin_table();
        // O(1) exact double-free guard (Phase 13.4a): test the segment's alloc
        // bitmap instead of walking the free list. The owner is the bitmap's
        // sole writer (reclaim runs on the owner — see this fn's docs), so the
        // read/modify/write needs no atomics. Replaces the former inline O(N)
        // `free_list_contains` walk that gave reclaim the same O(N²) regression
        // as own-thread free.
        let mut bm = meta.alloc_bitmap();
        if bm.is_free(off as u32) {
            return false; // Already on a free list (M2 double-free): no-op.
        }
        let block_nn = match NonNull::new(ptr) {
            Some(nn) => nn,
            None => return false,
        };
        let old_head = bt.head(class_idx);
        let old_head_ptr = if old_head == FREE_LIST_NULL {
            core::ptr::null_mut()
        } else {
            Node::deref(base, old_head as usize)
        };
        Node::write_next(block_nn, old_head_ptr);
        bt.set_head(class_idx, off as u32);
        bm.mark_free(off as u32);
        // Phase 35 (M6): a cross-thread-freed block is now back on the free list
        // → one fewer live block. The owner-side drain runs this, so the
        // owner-only counter is single-writer (the cross-thread freer NEVER
        // touched it — it only pushed the offset into the ring). If the segment
        // is now empty AND not the carve target, return its payload to the OS.
        // Returns true if decommit fired (caller should call recycle after drain).
        #[cfg(feature = "alloc-decommit")]
        {
            Self::dec_live_and_maybe_decommit(base, small_cur)
        }
        #[cfg(not(feature = "alloc-decommit"))]
        false
    }

    /// Phase 35 (M6 decommit) — the shared dec-then-maybe-decommit step, called
    /// after a block returns to a segment's free list (own-thread `dealloc_small`
    /// or owner-side `reclaim_offset`). It decrements the owner-only `live_count`
    /// and, if the segment just went empty (`live_count == 0`) AND is not the
    /// current carve target (`base != small_cur`), returns the segment's payload
    /// pages to the OS, resets the segment, releases the OS reservation, and
    /// recycles the table slot (task #60, variant B).
    ///
    /// **Self-less** (associated fn) so the self-less `reclaim_offset` can call
    /// it; the `small_cur` snapshot and `table` raw pointer are threaded in from
    /// the owner. The raw pointer is sound because `AllocCore` is single-owner
    /// (owner thread is the sole writer of its segments' metadata and table).
    ///
    /// ## Why M6 is decommit-safe WITHOUT an M11 epoch barrier (design §1)
    ///
    /// The original plan (§2.5) reached for `crossbeam-epoch` because the OLD
    /// intrusive cross-thread-free model wrote the free-list `next` pointer INSIDE
    /// the block — a late cross-thread freer could write into a page we had just
    /// decommitted (UAF / write-to-unmapped). Variant-2 (Phase 12.6) dissolved
    /// that: the cross-thread freer NEVER dereferences the block — it pushes
    /// `(offset|class)` into the `RemoteFreeRing`, which lives in the segment's
    /// METADATA (the metadata pages are NEVER decommitted — we decommit only
    /// `[small_meta_end, SEGMENT)`). The decommit is therefore safe without epoch:
    ///
    ///   1. We decommit the payload ONLY at `live_count == 0` → there is not one
    ///      live block in the decommitted range; nothing to UAF.
    ///   2. A late VALID cross-thread free at `live_count == 0` is impossible:
    ///      every block is already free, so a further free of one is a double-free
    ///      (the bitmap `is_free` guard below makes it a no-op before any write).
    ///   3. `reclaim_offset` on a stale ring entry computes the block address via
    ///      `Node::deref` (pure arithmetic — NO memory access) and then reads
    ///      `magic` / `kind` / **bitmap `is_free`** — ALL in the never-decommitted
    ///      metadata — and for a free block (and at `live==0` ALL are free) does a
    ///      no-op BEFORE touching the block. The decommitted page is never read or
    ///      written.
    ///   4. `reclaim` (drain) and `decommit` both run owner-side, so they are
    ///      serialized on the owning thread — there is no reclaim-vs-decommit race
    ///      on one segment.
    ///
    /// ⇒ No UAF, no write to decommitted memory. `crossbeam-epoch` is NOT needed;
    /// none is added. (Full argument: `docs/PHASE35_DECOMMIT_DESIGN.md` §1.)
    ///
    /// ## Slot recycle (task #60)
    ///
    /// After decommit + reset, [`decommit_empty_segment`] also releases the OS
    /// reservation for the segment and NULLs the table slot (via `table`). This
    /// lifts the 1024-segment hard cap: the freed slot can be reused immediately
    /// by the next `register` call, so long-running workloads never exhaust the
    /// table. Both the OS release and the slot NULL happen atomically inside
    /// `decommit_empty_segment`; there is no window where the OS segment is
    /// released but the slot is still non-NULL.
    /// Returns `true` if decommit fired (the segment became empty, was
    /// decommitted, and needs slot recycling). The caller is responsible for
    /// calling `self.table.recycle(base)` when `true` is returned — but ONLY
    /// after any in-progress ring drain for `base` has completed, so that
    /// stale ring entries can still read the (still-committed) metadata.
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    fn dec_live_and_maybe_decommit(base: *mut u8, small_cur: *mut u8) -> bool {
        let mut meta = SegmentMeta::new(base);
        let live = meta.dec_live();
        // Only an empty, non-current, not-already-decommitted segment is
        // returned to the OS. The current carve target stays committed (we are
        // about to bump-allocate into it); already-decommitted is idempotent.
        if live != 0 || base == small_cur || meta.is_decommitted() {
            return false;
        }
        // NEVER decommit the PRIMORDIAL segment: its metadata extends to
        // `primordial_meta_end()` (it hosts the self-hosted registry between
        // `small_meta_end()` and `primordial_meta_end()`), but the decommit reset
        // computes the payload start at `small_meta_end()`. Decommitting from
        // there would return the registry pages to the OS and reset page-map /
        // bump over the registry — corrupting the substrate. Only `Small`
        // segments (whose payload genuinely starts at `small_meta_end()`) are
        // eligible. A field-specific `kind` read (disjoint from the owner's
        // `bump`/`live_count` writes; race-free like the other `kind_at` reads).
        if !matches!(SegmentHeader::kind_at(base), SegmentKind::Small) {
            return false;
        }
        Self::decommit_empty_segment(&mut meta, base);
        true
    }

    /// E3 (task W4) — batched dec-then-maybe-decommit for a same-segment flush
    /// run. Subtracts `k` (the number of accepted blocks in the run) from
    /// `live_count` in ONE `sub_live` and makes the SAME decommit decision the
    /// per-block loop would make.
    ///
    /// ## Byte-identical to `k` sequential `dec_live_and_maybe_decommit` calls
    ///
    /// `flush_run`'s doc already proves that within a same-segment run `live`
    /// can only reach 0 at the LAST accepted block (every still-un-flushed
    /// same-segment block counts as live, so the segment empties iff the run
    /// flushes ALL its remaining live blocks — and then only at block `k`). So:
    ///   - The final `live_count` is identical: `sub_live(k)` == `k` `dec_live`s.
    ///   - Decommit fires at most once, on the SAME transition (the k-th block
    ///     that brings `live` to 0), under the SAME proviso
    ///     (`live == 0 && base != small_cur && !is_decommitted && kind == Small`)
    ///     — the per-block loop's earlier iterations all had `live > 0` and so
    ///     never entered the decommit branch. Checking the proviso ONCE on the
    ///     post-`sub_live` value therefore reproduces the loop exactly.
    ///
    /// Returns `true` iff decommit fired (caller runs `table.recycle`).
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    fn dec_live_batch_and_maybe_decommit(base: *mut u8, k: u32, small_cur: *mut u8) -> bool {
        if k == 0 {
            return false;
        }
        let mut meta = SegmentMeta::new(base);
        let live = meta.sub_live(k);
        if live != 0 || base == small_cur || meta.is_decommitted() {
            return false;
        }
        // Same PRIMORDIAL exclusion as `dec_live_and_maybe_decommit`: only a
        // `Small` segment's payload genuinely starts at `small_meta_end()`.
        if !matches!(SegmentHeader::kind_at(base), SegmentKind::Small) {
            return false;
        }
        Self::decommit_empty_segment(&mut meta, base);
        true
    }

    /// TEST-ONLY (Phase 35): the process-wide count of M6 decommit invocations
    /// (`decommit_empty_segment` calls). The soak test reads this to assert the
    /// decommit hook actually fires when segments empty (the counterfactual: with
    /// the live-count proviso miswired it stays zero and the test goes red). A
    /// plain relaxed atomic — diagnostic only, no ordering obligation.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_decommit_count() -> u64 {
        DECOMMIT_CALLS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// DIAGNOSTIC (task E1): process-wide count of successful OS segment
    /// reservations since process start (every `os::Segment::reserve`
    /// success plus NUMA-pinned reservations). Monotonic, relaxed — pairs
    /// with [`AllocCore::dbg_segments_released_total`]; the difference is
    /// the current process-wide live segment count. Always compiled (not
    /// feature-gated) — every build reserves segments via `os::Segment::reserve`.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_segments_reserved_total() -> u64 {
        super::os::segments_reserved_total()
    }

    /// DIAGNOSTIC (task E1): process-wide count of successful OS segment
    /// releases since process start. Monotonic, relaxed. See
    /// [`AllocCore::dbg_segments_reserved_total`].
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_segments_released_total() -> u64 {
        super::os::segments_released_total()
    }

    /// TEST-ONLY (Phase 35): the owner-only `live_count` of `ptr`'s segment, or
    /// `None` if `ptr` is foreign / not small/primordial. Lets the soak test
    /// assert a segment reaches `live_count == 0` before decommit.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_live_count_for(&self, ptr: *mut u8) -> Option<u32> {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base_ro(base) {
            return None;
        }
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return None;
        }
        Some(SegmentMeta::new(base).live_count_of())
    }

    /// TEST-ONLY (Phase 35): whether `ptr`'s segment is currently decommitted, or
    /// `None` if `ptr` is foreign / not small/primordial.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_is_decommitted_for(&self, ptr: *mut u8) -> Option<bool> {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base_ro(base) {
            return None;
        }
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return None;
        }
        Some(SegmentMeta::new(base).is_decommitted())
    }

    /// Decommit an empty small segment's payload and reset it to a clean blank.
    /// Precondition (caller's invariant): `live_count == 0` for this segment, so
    /// the entire payload `[small_meta_end, SEGMENT)` holds no live block.
    ///
    /// Steps (design §3):
    ///   1. Return the payload pages `[small_meta_end, SEGMENT)` to the OS. The
    ///      metadata pages (header / page-map / bin-table / alloc-bitmap / ring)
    ///      stay committed — cross-thread readers touch them, and `recycle` will
    ///      read the header reservation info AFTER this function returns.
    ///   2. Reset the segment to clean-empty: `bump = small_meta_end`, every
    ///      `BinTable` head = `FREE_LIST_NULL`, every payload page-map entry =
    ///      `Free`, the alloc bitmap = all-zeros. Safe because `live_count == 0`:
    ///      no block is live, every free-list node we are dropping is itself free.
    ///   3. Set the `decommitted` flag so the next carve recommits first.
    ///
    /// **Slot recycle** (task #60) is NOT done here — it happens after the
    /// drain loop that called `reclaim_offset` finishes (so that subsequent
    /// stale ring entries for the same segment still find the metadata
    /// readable). The caller is responsible for calling `self.table.recycle(base)`
    /// once no further `reclaim_offset` calls will target `base`. See
    /// `dealloc_small` and `find_segment_with_free` for the two call sites.
    #[cfg(feature = "alloc-decommit")]
    fn decommit_empty_segment(meta: &mut SegmentMeta, base: *mut u8) {
        // Test seam: count the invocation (diagnostic; relaxed).
        DECOMMIT_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let payload_start = SegLayout::small_meta_end();
        // 1. Return the payload pages to the OS (no-op under miri).
        os::decommit_pages(base, payload_start, SEGMENT);
        // 2a. Reset the bump cursor to the payload start (segment is blank). This
        //     is the load-bearing reset for the post-decommit stale-free guard:
        //     after this, every prior block offset in the payload is `>= bump`, so
        //     a late free / double-free / stale reclaim targeting this segment is
        //     rejected by the `off >= bump` check in `dealloc_small` /
        //     `reclaim_offset` BEFORE it writes a `next` pointer into a (now
        //     decommitted / unmapped) payload page.
        meta.set_bump(payload_start);
        // 2b. Empty every class free list.
        let mut bt = meta.bin_table();
        for c in 0..super::size_classes::SMALL_CLASS_COUNT {
            bt.set_head(c, FREE_LIST_NULL);
        }
        // 2c. Re-mark every payload page `Free` in the page map (metadata pages
        //     keep their `Meta` marking). Payload pages are `[meta_pages,
        //     PAGES_PER_SEGMENT)`.
        let mut pm = meta.page_map();
        let meta_pages = SegLayout::small_meta_pages();
        for p in meta_pages..super::segment_header::PAGES_PER_SEGMENT {
            pm.set_free(p);
        }
        // 2d. Zero the alloc bitmap (every slot "allocated / not-a-block" — the
        //     init state; with no live blocks and an empty free list this is the
        //     correct clean state). Re-init in place over the bitmap bytes.
        super::alloc_bitmap::AllocBitmap::init_in_place(Node::offset(
            base,
            SegLayout::alloc_bitmap_off(),
        ));
        // 2e. PERF-3 Ф4 (task #211, plan §2.5): clear the per-segment `RunStack`
        //     for EVERY class. After the payload pages are returned to the OS
        //     (step 1) and `bump` is reset to the payload start (step 2a), any
        //     stale run descriptor would point into the decommitted/unmapped
        //     payload region; a later `drain_freelist_batch` on this segment
        //     (before it is slot-recycled) would reconstruct `start_off +
        //     i*block_size` into that dead region. Clearing the `RunStack` here
        //     makes the post-decommit drain see an empty stack and return 0,
        //     exactly as the head-zeroing in step 2b makes the linked-list drain
        //     see `FREE_LIST_NULL` and return 0 — end-state byte-identical to
        //     the pre-PERF-3 decommit for the drain path (both representations
        //     empty). This mirrors the structural role of the `bt.set_head(c,
        //     FREE_LIST_NULL)` loop above (NULL the per-class fast-path state)
        //     and is the direct analogue of X7's decommit-lifecycle seam — with
        //     the OPPOSITE policy: X7's gen table is deliberately NOT re-zeroed
        //     (numbering is continuous across decommit, plan X7 §2.2), whereas
        //     the `RunStack` MUST be re-zeroed (its descriptors are address
        //     hints into the payload, which is now unmapped; stale hints are
        //     unsafe, not merely stale). Compiled ONLY under
        //     `alloc-runfreelist`; the non-feature decommit path is byte-
        //     identical to pre-Ф4 (the production-judge neutrality gate).
        #[cfg(feature = "alloc-runfreelist")]
        {
            super::run_stack::RunStack::clear_all(base);
        }
        // 3. Flag the segment decommitted so the next `carve_block` recommits.
        meta.set_decommitted(true);
    }

    /// TEST-ONLY (Phase B/C): the NUMA `node_id` stored in `ptr`'s segment
    /// header, or `None` if `ptr` is foreign. Returns `u32::MAX` (`NO_NODE_RAW`)
    /// for a segment that was not bound to a specific NUMA node (e.g. on a
    /// non-NUMA platform, or when `numa-aware` is off). The field is present in
    /// EVERY build's layout (layout-stable across feature configs); this accessor
    /// is only compiled under `numa-aware` because the test that reads it is also
    /// gated on that feature.
    #[doc(hidden)]
    #[cfg(feature = "numa-aware")]
    pub fn dbg_node_id_for(&self, ptr: *mut u8) -> Option<u32> {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base_ro(base) {
            return None;
        }
        Some(SegmentMeta::new(base).node_id_of())
    }

    /// TEST-ONLY: push `ptr`'s segment-relative offset — packed with its
    /// `class_idx` in the high bits — into its segment's `RemoteFreeRing`,
    /// exactly as a cross-thread freer would. Lets a single-threaded test
    /// exercise the ring→reclaim path (which the public own-thread `dealloc`
    /// bypasses) and isolate `reclaim_offset` logic from concurrency. The caller
    /// supplies `class_idx` (the class it allocated the block under) because the
    /// reclaim contract carries the class in the ring entry — the owner must
    /// never re-derive it from `page_map` (the §13 root cause).
    #[doc(hidden)]
    #[cfg(feature = "alloc-xthread")]
    pub fn dbg_push_to_ring(&self, ptr: *mut u8, class_idx: usize) -> bool {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base_ro(base) {
            return false;
        }
        let off = (ptr as usize - base as usize) as u32;
        // X7 Ф3 (task #191): mirror `dealloc_routing`'s touch (b). Under
        // `hardened` the drain unpacks with `unpack_entry_hardened`, so the
        // push MUST pack with `pack_entry_hardened` and stamp the current
        // generation — otherwise the gen-check at drain would compare against
        // an unstamped (zero) gen and false-mismatch on every entry. Under
        // non-hardened the untouched `pack_entry` is used (byte-identical).
        // Sibling-block discipline mirrors `dealloc_routing`'s Variant-2 block.
        #[cfg(feature = "hardened")]
        {
            let gen = super::segment_header::gen_at(base, off as usize);
            let packed =
                super::remote_free_ring::pack_entry_hardened(gen, class_idx as u32, off);
            let ring = SegmentMeta::new(base).remote_ring();
            ring.push(packed).is_ok()
        }
        #[cfg(not(feature = "hardened"))]
        {
            let packed = super::remote_free_ring::pack_entry(off, class_idx as u32);
            let ring = SegmentMeta::new(base).remote_ring();
            ring.push(packed).is_ok()
        }
    }

    /// TEST-ONLY (task #37): drain every owned segment's ring into its
    /// `BinTable`, exactly as `find_segment_with_free` does, but unconditionally.
    #[doc(hidden)]
    #[cfg(feature = "alloc-xthread")]
    pub fn dbg_drain_all_rings(&mut self) {
        // Default: no magazine predicate (non-fastbin callers, or
        // AllocCore-level tests that have no magazine).
        self.dbg_drain_all_rings_impl(&|_, _| false);
    }

    /// Task #164: variant with an explicit magazine predicate, called from
    /// `HeapCore::dbg_drain_all_rings` to exercise the production decision path.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))]
    pub fn dbg_drain_all_rings_checked<F: Fn(*mut u8, usize) -> bool>(
        &mut self,
        is_in_magazine: &F,
    ) {
        self.dbg_drain_all_rings_impl(is_in_magazine);
    }

    #[cfg(feature = "alloc-xthread")]
    #[cfg_attr(not(feature = "fastbin"), allow(unused_variables))]
    #[inline]
    fn dbg_drain_all_rings_impl<F: Fn(*mut u8, usize) -> bool>(&mut self, is_in_magazine: &F) {
        // Index-driven scan (task #126), mirroring `find_segment_with_free`:
        // `base_at(i)` is a self-contained read with no borrow tied to the
        // loop, so it can be freely interleaved with `self.table.recycle`
        // below without a pre-collect buffer.
        let n = self.table.count() as usize;
        for i in 0..n {
            let base = self.table.base_at(i);
            if base.is_null() {
                continue;
            }
            let hdr = SegmentHeader::read_at(base);
            if !matches!(hdr.kind, SegmentKind::Small | SegmentKind::Primordial) {
                continue;
            }
            let ring = SegmentMeta::new(base).remote_ring();
            let small_cur = self.small_cur;
            #[cfg(feature = "alloc-decommit")]
            let mut decommit_happened = false;
            ring.drain(|off| {
                #[cfg(feature = "fastbin")]
                let reclaimed = Self::reclaim_offset_checked(base, off, small_cur, is_in_magazine);
                #[cfg(not(feature = "fastbin"))]
                let reclaimed = Self::reclaim_offset(base, off, small_cur);
                #[cfg(feature = "alloc-decommit")]
                if reclaimed {
                    decommit_happened = true;
                }
                #[cfg(not(feature = "alloc-decommit"))]
                {
                    let _ = reclaimed;
                }
            });
            #[cfg(feature = "alloc-decommit")]
            if decommit_happened {
                self.table.recycle(base);
            }
        }
    }

    /// TEST-ONLY (Phase 13.3): reveal the size class `page_map` would assign
    /// to `ptr`'s page, so the counterfactual test for "own-thread dealloc
    /// derives the class from `Layout`, not `page_map`" can prove it is
    /// non-vacuous. Returns `None` if `ptr` is foreign, the segment is not
    /// small/primordial, or the page is uncarved. This is the (now-removed)
    /// `page_map`-class derivation the old intrusive-TFS drain used — kept here
    /// as a pure read so the test can prove the Layout-class and page_map-class
    /// genuinely differ on a mixed-class page (the §13 counterfactual).
    /// `#[doc(hidden)] pub` per the established test-only surface.
    #[doc(hidden)]
    pub fn dbg_page_map_class_for(&self, ptr: *mut u8) -> Option<usize> {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base_ro(base) {
            return None;
        }
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return None;
        }
        let meta = SegmentMeta::new(base);
        let page_idx = (ptr as usize - base as usize) / super::os::PAGE;
        meta.page_map().class_of(page_idx)
    }

    /// TEST-ONLY (Phase 13.3): the size class the own-thread `dealloc` SHOULD
    /// derive from `layout` (i.e. what `Self::classify` resolves to). Returns
    /// `None` for a Large layout. Exposed so the counterfactual test can
    /// compare the Layout-derived class against the `page_map`-derived class
    /// on a mixed-class page and prove the two genuinely differ (otherwise
    /// the test would be vacuous).
    /// TEST-ONLY (task #135): the segment table's high-water slot count (see
    /// `SegmentTable::count`). Used by `tests/segment_table_o1.rs` to verify
    /// the O(1) free-list actually recycles vacated indices instead of
    /// letting the high-water mark grow unbounded.
    #[doc(hidden)]
    pub fn dbg_table_count(&self) -> u32 {
        self.table.count()
    }

    /// TEST-ONLY (W2 tombstone-rebuild): the segment table's current exact
    /// TOMBSTONE count in the open-addressing hash (see
    /// `SegmentTable::dbg_hash_tombstones`). Used by
    /// `tests/regression_segment_table_tombstone_rebuild.rs` to verify the
    /// tombstone-threshold rebuild actually fires and keeps the count bounded
    /// (the counterfactual: without the rebuild trigger this grows without
    /// bound). Thin forwarder over the `pub(crate)` table seam, mirroring
    /// `dbg_table_count`. Zero production impact.
    #[doc(hidden)]
    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    pub fn dbg_hash_tombstones(&self) -> u32 {
        self.table.dbg_hash_tombstones()
    }

    /// TEST-ONLY (task #135): public wrapper over `AllocCore::contains_base`
    /// for integration tests (which cannot see the `pub(crate)` version, nor
    /// the `pub(crate)` `os::segment_base_of_ptr` needed to derive a segment
    /// base from an arbitrary in-segment pointer). Takes any pointer
    /// previously returned by `alloc`/`alloc_large` (not necessarily the
    /// segment base itself) and derives the base internally, matching the
    /// convention of the other `dbg_*_for` accessors in this file.
    #[doc(hidden)]
    pub fn dbg_contains_base(&self, ptr: *mut u8) -> bool {
        self.table.contains_base_ro(os::segment_base_of_ptr(ptr))
    }

    /// TEST-ONLY (task #135): read the stamped `segment_id` field of `ptr`'s
    /// segment (field-specific read, mirrors what
    /// `SegmentTable::unregister`/`recycle` now use internally for their O(1)
    /// slot lookup).
    #[doc(hidden)]
    pub fn dbg_segment_id_of(&self, ptr: *mut u8) -> u32 {
        SegmentHeader::segment_id_at(os::segment_base_of_ptr(ptr))
    }

    /// TEST-ONLY (task #135): overwrite the stamped `segment_id` field of
    /// `ptr`'s segment (field-specific write). Used to construct the
    /// corrupted-id scenario exercised by
    /// `unregister_defends_against_mismatched_segment_id`.
    #[doc(hidden)]
    pub fn dbg_stamp_segment_id(&self, ptr: *mut u8, id: u32) {
        SegmentHeader::set_segment_id_at(os::segment_base_of_ptr(ptr), id);
    }

    /// TEST-ONLY (OPT-G regression): read the `large_size` field from the
    /// header of `ptr`'s segment. Uses a direct field read (same pattern as
    /// `large_size_at` but without the `alloc-xthread` feature gate) so
    /// integration tests can verify the stored value after an in-place realloc.
    #[doc(hidden)]
    pub fn dbg_large_size_of(&self, ptr: *mut u8) -> usize {
        let base = os::segment_base_of_ptr(ptr);
        let off = core::mem::offset_of!(SegmentHeader, large_size);
        Node::read_usize(Node::offset(base, off) as *const usize)
    }

    /// TEST-ONLY (task #135): directly invoke `SegmentTable::unregister` for
    /// `ptr`'s segment, for a public integration test (which cannot call the
    /// `pub(crate)` version). Exercises the O(1) `segment_id`-indexed lookup
    /// and its defensive `slots[id] == base` guard in isolation from any
    /// surrounding dealloc bookkeeping (the caller is responsible for
    /// whatever cleanup the test scenario needs afterwards).
    #[doc(hidden)]
    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    pub fn dbg_unregister(&mut self, ptr: *mut u8) {
        self.table.unregister(os::segment_base_of_ptr(ptr));
    }

    /// TEST-ONLY (E2, task W4): the `block_size` of a small class, so the
    /// `refill_n` LUT-vs-formula equivalence test can feed the same input to
    /// both without needing `pub(crate)` access to `SIZE_CLASS_TABLE`.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_block_size(class_idx: usize) -> usize {
        SizeClasses::block_size(class_idx)
    }

    /// TEST-ONLY (E2, task W4): number of small size classes.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_small_class_count() -> usize {
        super::size_classes::SMALL_CLASS_COUNT
    }

    /// TEST-ONLY (E1, task W4): drive [`carve_batch`](Self::carve_batch)
    /// directly (it is a private internal), so the equivalence regression test
    /// can carve a run and inspect the exact block set without going through the
    /// magazine. Returns the number of blocks carved into `out`.
    #[doc(hidden)]
    pub fn dbg_carve_batch(&mut self, class_idx: usize, out: &mut [*mut u8]) -> usize {
        let block_size = SizeClasses::block_size(class_idx);
        self.carve_batch(class_idx, block_size, out)
    }

    #[doc(hidden)]
    pub fn dbg_layout_class_for(&self, layout: Layout) -> Option<usize> {
        let size = layout.size().max(super::size_classes::MIN_BLOCK);
        match Self::classify(size, layout.align()) {
            AllocKind::Small { class_idx } => Some(class_idx),
            AllocKind::Large => None,
        }
    }

    /// TEST-ONLY (Э7, task #161): the segment-relative offset of the head of
    /// `ptr`'s segment's `BinTable[class_idx]` free list, or `FREE_LIST_NULL`
    /// (`u32::MAX`) if the list is empty. Lets the batch-drain regression test
    /// observe `set_head`'s exact post-drain value directly (partial drain →
    /// remaining head; full drain → NULL).
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_freelist_head_for(&self, ptr: *mut u8, class_idx: usize) -> u32 {
        let base = os::segment_base_of_ptr(ptr);
        SegmentMeta::new(base).bin_table().head(class_idx)
    }

    /// TEST-ONLY (Э7, task #161): whether `ptr`'s block is currently marked FREE
    /// (on a free list) in its segment's alloc bitmap — the M2 double-free bit.
    /// `false` ⟺ the block is ALLOCATED (handed out). Lets the batch-drain test
    /// assert every drained block ends bitmap-allocated, exactly as `pop_free`
    /// leaves it.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_is_free_for(&self, ptr: *mut u8) -> bool {
        let base = os::segment_base_of_ptr(ptr);
        let off = (ptr as usize - base as usize) as u32;
        SegmentMeta::new(base).alloc_bitmap().is_free(off)
    }

    /// TEST-ONLY (Э7, task #161): drive `drain_freelist_batch` directly on
    /// `ptr`'s segment so a regression test can observe partial/full-drain
    /// behaviour (return count, resulting `set_head`, per-block bitmap state) in
    /// isolation from the surrounding `refill_class_bump` carve logic.
    #[doc(hidden)]
    pub fn dbg_drain_freelist_batch(
        &self,
        ptr: *mut u8,
        class_idx: usize,
        out: &mut [*mut u8],
    ) -> usize {
        let base = os::segment_base_of_ptr(ptr);
        self.drain_freelist_batch(base, class_idx, out)
    }

    /// Shrink/grow an allocation in place or by alloc + copy + dealloc.
    ///
    /// Two in-place fast paths are attempted first (shared with
    /// [`try_realloc_inplace`], which [`HeapCore::realloc`](crate::registry::HeapCore)
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
    /// Returns null on failure, leaving the old allocation intact. Safe: a
    /// null `ptr` returns null without touching state.
    pub fn realloc(&mut self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
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
        if let Some(p) = self.realloc_inplace_fast_path(ptr, old_layout, new_size) {
            return p;
        }
        // In-place fast paths did not apply: alloc a fresh block, copy the
        // preserved prefix, and free the old block.
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
        self.dealloc(ptr, old_layout);
        new_ptr
    }

    /// Single source of truth for the OPT-F / OPT-G in-place realloc fast
    /// paths. Returns `Some(ptr)` (the SAME pointer, unchanged or with its
    /// Large header's `large_size` updated in place) when an in-place resize
    /// is possible, `None` otherwise. Does NOT fall through to `self.alloc` —
    /// callers own that decision (the substrate-level [`realloc`](Self::realloc)
    /// calls `self.alloc` + copy + `self.dealloc`; the registry-level
    /// [`try_realloc_inplace`] is consumed by `HeapCore::realloc`, which routes
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
    #[inline]
    fn realloc_inplace_fast_path(
        &mut self,
        ptr: *mut u8,
        old_layout: Layout,
        new_size: usize,
    ) -> Option<*mut u8> {
        // OPT-G: Large→Large in-place grow.
        {
            let base = os::segment_base_of_ptr(ptr);
            if self.table.contains_base(base) && SegmentHeader::kind_at(base) == SegmentKind::Large
            {
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
            }
        }
        // OPT-F: Small→Small same-class in-place.
        {
            let base = os::segment_base_of_ptr(ptr);
            if self.table.contains_base(base)
                && matches!(
                    SegmentHeader::kind_at(base),
                    SegmentKind::Small | SegmentKind::Primordial
                )
            {
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
        }
        None
    }

    /// Task #164: try the two in-place realloc fast paths (Large grow-in-span,
    /// Small same-class) WITHOUT falling through to `self.alloc` on miss. Returns
    /// `Some(ptr)` on in-place success, `None` on fallthrough. `HeapCore::realloc`
    /// uses this so the alloc leg routes through the magazine-aware
    /// `HeapCore::alloc` (which drains via the checked predicate), closing the
    /// unchecked drain path through `AllocCore::alloc` → `alloc_small`.
    ///
    /// Thin wrapper over [`realloc_inplace_fast_path`] — the OPT-F/OPT-G
    /// detection logic lives in exactly one place, shared with the
    /// substrate-level [`realloc`](Self::realloc).
    #[cfg(feature = "alloc-global")]
    pub(crate) fn try_realloc_inplace(
        &mut self,
        ptr: *mut u8,
        old_layout: Layout,
        new_size: usize,
    ) -> Option<*mut u8> {
        if ptr.is_null() {
            return None;
        }
        self.realloc_inplace_fast_path(ptr, old_layout, new_size)
    }

    /// Iterate over all registered segment bases (read-only). Exposed for the
    /// Phase 12.4 abandonment walk (`HeapCore::segment_bases` →
    /// `abandon_segments`).
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

    /// Register an already-reserved segment base into this substrate's table
    /// (Phase 12.4 adoption). Returns the assigned `segment_id`, or `None` if
    /// the table is full. Used by `HeapRegistry::try_adopt` to register an
    /// adopted segment into the adopter's `AllocCore` so subsequent
    /// `alloc`/`dealloc` routing finds it. The caller MUST have laid down a
    /// valid header at `base` (the abandon path left it intact).
    #[cfg(feature = "alloc-global")]
    pub(crate) fn register_segment(&mut self, base: *mut u8) -> Option<u32> {
        self.table.register(base)
    }

    /// Mark `base` as the current small segment (Phase 12.4 adoption primitive).
    /// An adopted segment with free space becomes the bump target so the
    /// adopter carves new allocations from it. Retained for the loom-proven
    /// abandon/adopt substrate (a future decommit-when-empty policy); NOT on
    /// the hot path of the shard model (a heap owns its segments exclusively
    /// and never transfers them).
    #[cfg(feature = "alloc-global")]
    #[allow(dead_code)]
    pub(crate) fn set_small_current(&mut self, base: *mut u8) {
        self.small_cur = base;
    }

    // -----------------------------------------------------------------------
    // Batch APIs (Phase 103 / P1 — fastbin / tcache substrate)
    //
    // Thin wrappers around the existing `alloc_small` / `dealloc_small`
    // primitives, called in a loop. NO new placement logic, NO new
    // invariants — the audited M2 / decommit / cross-thread paths run
    // UNCHANGED, just grouped into batches for the magazine layer (P2+).
    // -----------------------------------------------------------------------

    /// Pull up to `want` free blocks of class `class_idx` out of the segment
    /// substrate into `out`. Returns how many were written (0 on true OOM,
    /// else `> 0` and `<= want`).
    ///
    /// Each pulled block undergoes EXACTLY the same transition as a single
    /// `alloc_small`: bitmap `mark_alloc` + `inc_live` (under alloc-decommit).
    /// So a magazine-resident block will be "live + bitmap-allocated",
    /// identical to a handed-out block.
    #[doc(hidden)]
    #[inline]
    pub fn refill_class(&mut self, class_idx: usize, want: usize, out: &mut [*mut u8]) -> usize {
        debug_assert!(
            out.len() >= want,
            "refill_class: out.len() ({}) < want ({})",
            out.len(),
            want,
        );
        for (i, slot) in out.iter_mut().take(want).enumerate() {
            let ptr = self.alloc_small(class_idx);
            if ptr.is_null() {
                return i; // OOM or no more capacity
            }
            *slot = ptr;
        }
        want
    }

    /// Э1 (task #147) — **bump-direct batched carve**. Fill `out` with up to
    /// `out.len()` live, bitmap-allocated blocks of class `class_idx`, producing
    /// the IDENTICAL end-state as `refill_class` (each block: `live_count += 1`,
    /// bitmap "allocated", handed to the magazine) but SKIPPING the BinTable
    /// round-trip for freshly-carved blocks. Returns the number of slots filled
    /// (0 on true OOM, else `> 0` and `<= out.len()`).
    ///
    /// ## Source order — NON-NEGOTIABLE (free-drain BEFORE bump)
    ///
    /// For each wanted slot we prefer an EXISTING free block and bump-carve ONLY
    /// when no free block remains:
    ///   1. Drain free blocks first — `pop_free(small_cur)`, and on a miss
    ///      `find_segment_with_free` (which lazily drains each owned segment's
    ///      remote-free ring, reclaiming cross-thread frees). This MUST run
    ///      before any bump-carve: if we carved first, freed blocks sitting in
    ///      the per-segment rings/BinTables would go stale, the rings would back
    ///      up (RSS drift), and the xthread ring-reclaim expectations (A1) would
    ///      break — a freed remote block must be reused, not stranded while we
    ///      grow the bump cursor.
    ///   2. For the remaining slots, bump-carve DIRECTLY into `out` via
    ///      `carve_block` — no `dealloc_small`, no BinTable push, no subsequent
    ///      `pop_free`. `carve_block` already does `inc_live` + bump + page-map +
    ///      recommit (under `alloc-decommit`) and leaves the alloc bitmap UNSET
    ///      (= "allocated", the M2 convention), so a carved block is already in
    ///      the exact "live, allocated" state a handed-out block must be in
    ///      (see `carve_block` ~1783: it never touches `alloc_bitmap()`).
    ///      On `carve_block` → `None` (current segment full) we
    ///      `reserve_small_segment` and continue; if reserve fails we stop and
    ///      return the count filled so far (graceful — the caller treats `0` as
    ///      OOM and a partial fill as a normal short refill).
    ///
    /// ## D1 (live_count) — exact, per block +1, never double
    ///
    /// Each `out` block receives EXACTLY one `inc_live`: either from `pop_free`
    /// (drain branch) OR from `carve_block` (bump branch), never both — a slot
    /// is filled by exactly one of the two. This equals what `refill_class`
    /// produced (its `alloc_small` did one `inc_live` per block). The removed
    /// BinTable round-trip in the OLD path was net-zero on `live_count` anyway
    /// (`carve_block` +1 then the immediate `dealloc_small` −1 for each refill
    /// extra, then `pop_free` +1 when later re-popped); collapsing it changes
    /// nothing about the final count, only the intermediate churn.
    ///
    /// ## M2 (double-free bitmap) — byte-identical
    ///
    /// Carved blocks keep their bitmap bit UNSET (allocated). They are returned
    /// to the substrate later via `flush_class` → `dealloc_small`, which
    /// `mark_free`s them THEN — the identical lifecycle as `refill_class`, minus
    /// the redundant intermediate set-free-then-clear. A double-free of such a
    /// block still hits `dealloc_small`'s `is_free` guard exactly as before.
    #[doc(hidden)]
    #[inline]
    pub fn refill_class_bump(&mut self, class_idx: usize, out: &mut [*mut u8]) -> usize {
        self.refill_class_bump_impl(
            class_idx,
            out,
            #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))]
            &|_, _| false,
        )
    }

    /// Task #164: variant with magazine predicate.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))]
    pub fn refill_class_bump_checked<F: Fn(*mut u8, usize) -> bool>(
        &mut self,
        class_idx: usize,
        out: &mut [*mut u8],
        is_in_magazine: &F,
    ) -> usize {
        self.refill_class_bump_impl(class_idx, out, is_in_magazine)
    }

    #[inline]
    fn refill_class_bump_impl<
        #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))] F: Fn(*mut u8, usize) -> bool,
    >(
        &mut self,
        class_idx: usize,
        out: &mut [*mut u8],
        #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))] is_in_magazine: &F,
    ) -> usize {
        let block_size = SizeClasses::block_size(class_idx);
        debug_assert!(block_size >= NODE_SIZE);
        let want = out.len();
        let mut filled = 0usize;
        // Once the whole-heap free scan (`find_segment_with_free`) reports NO
        // free block of this class anywhere AND has drained every owned
        // segment's remote-free ring, there is nothing more to reclaim for the
        // rest of THIS refill: our own frees cannot happen mid-refill, and
        // remote frees that arrive now land in the (already-scanned) rings and
        // are deferred to the NEXT refill's drain — exactly the amortisation
        // the retired `carve_block_with_refill` used (it also drained/scanned
        // once, then carved its whole batch). Latching this avoids re-running
        // the O(segments) scan + ring drain on every carved block of a cold
        // storm; correctness is unchanged because the drain still runs at
        // least once BEFORE any carve (source order preserved).
        let mut free_exhausted = false;
        while filled < want {
            // 1. FREE-DRAIN FIRST (order is non-negotiable — see doc). Prefer
            //    free blocks from the current segment, then from any owned
            //    segment (which also drains remote rings → xthread reclaim).
            //
            //    Э7 (task #161): drain the segment's freelist in ONE walk via
            //    `drain_freelist_batch` instead of one `pop_free` per block —
            //    `set_head`/`head`-read/`inc_live` are hoisted out of the
            //    per-block loop. The end-state (bitmap bits, live_count,
            //    freelist head) is byte-identical to the per-block path. Source
            //    order is UNCHANGED: current segment's freelist, then the
            //    ring-draining whole-heap scan, then bump-carve.
            //
            //    E1 (task W4): once `free_exhausted` is latched there is nothing
            //    left to reclaim for the rest of this refill (proof below), so we
            //    SKIP the per-iteration `drain_freelist_batch` re-read + subslice
            //    construction — a pure tautology after the latch — and go
            //    straight to the batched bump-carve. The head cannot become
            //    non-null mid-refill: no dealloc / reclaim / flush runs inside
            //    `refill_class_bump` after the latch, and a remote free that
            //    arrives now lands in the (already-scanned) ring, deferred to the
            //    NEXT refill's drain. So re-draining the current segment's
            //    freelist would only ever pop 0 — safe to skip.
            if !free_exhausted {
                let n = self.drain_freelist_batch(self.small_cur, class_idx, &mut out[filled..]);
                if n != 0 {
                    filled += n;
                    continue;
                }
                // `find_segment_with_free` runs the A1 ring-drain (reclaiming
                // cross-thread frees into the per-segment BinTables) BEFORE it
                // returns a base — that ordering is preserved: we call the batch
                // drain only on the base it hands back.
                // Task R1 (retro C1): wrap the caller's magazine predicate
                // with an out-membership guard. The predicate passed in from
                // `refill_magazine_slow` opens with `if k == c { return false; }`
                // (justified ONLY by the borrow-safety invariant count[c]==0),
                // which means blocks already pulled into `out[0..filled]` during
                // THIS refill call — magazine-destined but not yet stamped into
                // the magazine — are INVISIBLE to it. A stale cross-thread
                // double-free note for such a block still sitting in a ring
                // would then be reclaimed (write_next + mark_free), relinking
                // the block onto the freelist, and the SAME refill loop would
                // pull it into `out` AGAIN → P issued twice out of one refill.
                //
                // The guard closes the window for free: when the ring is empty
                // (the common case) `issued_so_far.contains` is never consulted,
                // so the Ir cost on the hot refill path is exactly zero — the
                // out-buffer is non-empty only when we have already drained at
                // least one block from the freelist AND the ring has work, and
                // even then the scan is over a CAP-bounded magazine refill batch.
                #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))]
                let found_seg = {
                    let issued_so_far: &[*mut u8] = &out[..filled];
                    self.find_segment_with_free_checked(class_idx, &|ptr, k| {
                        is_in_magazine(ptr, k)
                            || (k == class_idx && issued_so_far.contains(&ptr))
                    })
                };
                #[cfg(not(all(feature = "alloc-xthread", feature = "fastbin")))]
                let found_seg = self.find_segment_with_free(class_idx);
                if let Some(seg) = found_seg {
                    let n = self.drain_freelist_batch(seg, class_idx, &mut out[filled..]);
                    if n != 0 {
                        filled += n;
                        continue;
                    }
                }
                // Scan found nothing (and drained all rings): stop re-scanning
                // AND re-draining for the remainder of this refill; carve only.
                free_exhausted = true;
            }
            // 2. No free block anywhere: batched bump-carve DIRECTLY into `out`
            //    (E1, task W4). One `carve_batch` fills the whole remaining run
            //    from the current segment's bump in one shot — no BinTable
            //    round-trip, block live + bitmap-allocated, exactly the
            //    handed-out state (byte-identical to the per-block `carve_block`
            //    loop it replaces; see `carve_batch`).
            let n = self.carve_batch(class_idx, block_size, &mut out[filled..]);
            if n != 0 {
                filled += n;
                continue;
            }
            // 3. Current segment is full: reserve a fresh one and retry the
            //    carve. If reserve fails, stop and return what we have.
            match self.reserve_small_segment() {
                Some(_) => {
                    let n = self.carve_batch(class_idx, block_size, &mut out[filled..]);
                    if n != 0 {
                        filled += n;
                        continue;
                    }
                    // A fresh segment that cannot fit even one block indicates
                    // metadata corruption; stop gracefully rather than loop.
                    break;
                }
                None => break,
            }
        }
        filled
    }

    /// Push a batch of blocks of class `class_idx` back onto their owning
    /// segments' `BinTable`s.
    ///
    /// Each block undergoes EXACTLY the same transition as a single
    /// `dealloc_small`: off>=bump guard + `is_free` (M2 double-free) +
    /// `write_next`/`set_head` + `mark_free` + `dec_live_and_maybe_decommit`
    /// (+ `table.recycle` on decommit if fired).
    ///
    /// Per-block base is derived per-block via `os::segment_base_of_ptr`
    /// (the magazine CAN hold blocks from multiple segments).
    ///
    /// ## Э8 (task #162) — same-segment run batching, BYTE-IDENTICAL to the
    /// per-block path
    ///
    /// The magazine holds blocks from possibly several segments, but a
    /// cold-storm flush of consecutively-freed blocks is ~100% same-segment, so
    /// scanning for RUNS of consecutive blocks with the same
    /// `segment_base_of_ptr` (ONE mask-compare per block, NO sorting) yields
    /// long runs; a scattered magazine degrades to runs of length 1 — still
    /// correct. For each run (all sharing one `base`) we hoist the metadata
    /// views (`SegmentMeta::new`, `bin_table`, `alloc_bitmap`, and — under
    /// decommit — the `bump_of` LOAD) ONCE and write the freelist head ONCE,
    /// instead of once per block.
    ///
    /// ### The TWO guards STAY per-block (they are NOT tautologies)
    ///
    /// 1. `is_free(off)` — a REAL guard: under the documented #164 residual, a
    ///    cross-thread free of a magazine-resident block routes via the ring →
    ///    `reclaim_offset` marks it FREE on the BinTable while it still sits in
    ///    the magazine; this flush must then SKIP it (`is_free == true`) or the
    ///    freelist gets a duplicate. So the run-local chain links ONLY blocks
    ///    that PASS `is_free`.
    /// 2. `off >= bump` (decommit stale-free) — the COMPARE stays per-block;
    ///    only the `bump_of()` LOAD is hoisted. A flush never carves, so `bump`
    ///    cannot advance during a flush; and a decommit-reset of `bump` can only
    ///    happen at the LAST accepted block of a run (see the decommit proof
    ///    below), after which there is no further block in the run to mis-judge.
    ///
    /// ### Splice — provably byte-identical to N sequential `dealloc_small`s
    ///
    /// A sequential run `dealloc_small(b0); …; dealloc_small(bk)` (accepted
    /// blocks only; a rejected block never calls `set_head`, so it is simply
    /// absent from the chain) builds a LIFO push: each accepted block becomes
    /// the new head pointing at the prior head. Final state:
    /// `head = off(b_last)`, `b_last.next = off(prev accepted)`, …,
    /// `b_first.next = old_head` (the segment's head captured at run start).
    /// The batch reproduces this EXACTLY: capture `old_head` once, then for each
    /// ACCEPTED block in source order `write_next(b, prev_accepted_or_old_head)`
    /// + `mark_free(off)`, remembering `b` as the new `prev_accepted`; after the
    /// run, `set_head(off(last accepted))` ONCE (only if ≥1 accepted). Every
    /// `write_next` writes the identical `next`, every `mark_free` sets the
    /// identical bit, `set_head` lands on the identical value ⇒ byte-identical.
    ///
    /// ### Decommit — deferred `dec_live`/decommit is EQUIVALENT
    ///
    /// Within a same-segment run, `live_count` starts at the segment's current
    /// count `L` and drops by one per accepted block. Every un-flushed
    /// same-segment block (still handed out to the user, still in the magazine,
    /// or later in this/another run) counts as live, so `live` reaches 0 iff the
    /// run flushes ALL `L` remaining live blocks — and then ONLY at the LAST
    /// accepted block. The per-block path likewise only decommits at the block
    /// that brings `live` to 0. So running `dec_live_and_maybe_decommit`
    /// per-accepted-block here (AFTER the run's `set_head`, matching the
    /// sequential order where each block's dec-then-decommit follows its own
    /// `set_head`) fires decommit on exactly the same block, exactly once, and
    /// `table.recycle` exactly when it fired. If decommit DOES fire at the last
    /// accepted block, `decommit_empty_segment` re-NULLs every class head
    /// (including this one) and zeroes the bitmap — wiping the chain we just
    /// spliced. That wipe is CORRECT and identical to the sequential path (whose
    /// last block's decommit does the same after its own `set_head`); there is
    /// no subsequent block in the run to be affected, since `live` can only reach
    /// 0 at the last.
    #[doc(hidden)]
    #[inline]
    pub fn flush_class(&mut self, class_idx: usize, blocks: &[*mut u8]) {
        let mut i = 0;
        while i < blocks.len() {
            let ptr = blocks[i];
            if ptr.is_null() {
                i += 1;
                continue; // defensive: skip nulls (matches per-block path)
            }
            let base = os::segment_base_of_ptr(ptr);
            // Detect the run of consecutive same-segment blocks starting at `i`.
            // Nulls terminate a run (they are handled by the outer loop as
            // no-ops, exactly as the per-block path skips them).
            let mut run_end = i + 1;
            while run_end < blocks.len() {
                let q = blocks[run_end];
                if q.is_null() || os::segment_base_of_ptr(q) != base {
                    break;
                }
                run_end += 1;
            }
            self.flush_run(class_idx, base, &blocks[i..run_end]);
            i = run_end;
        }
    }

    /// Flush ONE run of blocks that all share segment `base` (Э8). See
    /// `flush_class` for the byte-identical / decommit-equivalence proofs. Every
    /// block in `run` is non-null and has `segment_base_of_ptr(block) == base`.
    #[inline]
    fn flush_run(&mut self, class_idx: usize, base: *mut u8, run: &[*mut u8]) {
        // PERF-3 Ф2: under `alloc-runfreelist`, detect contiguous-accepted
        // sub-runs (offset-adjacent blocks) and encode them as compact
        // `(start_off, count)` descriptors on the per-segment `RunStack` instead
        // of writing per-block `next` pointers — later drains reconstruct
        // addresses by stride arithmetic, eliminating the dependent-load
        // pointer chase that is this arc's target (plan §1). The detection
        // strategy is SORT-then-detect: the magazine's LIFO refill returns
        // blocks in DESCENDING address order within a refill batch, so an
        // in-place scan of the flush batch finds ~0% offset-adjacent neighbours
        // (empirically measured on the `bench_direct_alloc` pattern — see the Ф2
        // design report); sorting the accepted offsets ASCENDING first turns
        // that same batch into a ~100%-contiguous ascending run. Singletons
        // (runs of 1) and runs whose `RunStack::push` overflows (the per-class
        // `RUNSTACK_CAPACITY = 8` full) fall back to the EXACT classic LIFO-
        // chain path — the linked-list representation and the run-stack coexist;
        // Ф3's drain reads both. The bitmap (`mark_free`) fires for EVERY
        // accepted block regardless of representation (plan §2.3). Under
        // `not(feature = "alloc-runfreelist")` the body is byte-identical to the
        // pre-Ф2 `flush_run` (the neutrality gate).
        let meta = SegmentMeta::new(base);
        let mut bt = meta.bin_table();
        let mut bm = meta.alloc_bitmap();
        // Hoist the `bump` LOAD once (the COMPARE stays per-block). A flush
        // never carves, so `bump` cannot advance during this run.
        #[cfg(feature = "alloc-decommit")]
        let bump = meta.bump_of();

        // PERF-3 Ф2: collect the offsets of ACCEPTED blocks for the run-
        // detection pass below (under `alloc-runfreelist` only). The bound is
        // `FLUSH_RUN_DETECT_CAP`: the production magazine's physical cap is 16
        // (`TCACHE_CAP` in `registry::tcache`, not imported here to respect the
        // `alloc_core` ← `registry` layering), and the overflow-flush batch is
        // `FLUSH_N = TCACHE_CAP/2 = 8`. A same-segment run longer than 16 is a
        // structural impossibility from the magazine; tests may call
        // `flush_class` with larger slices, and those extra blocks simply stay
        // on the classic linked list (the `accepted_n < CAP` guard drops them
        // from the detection buffer — they remain correctly linked and
        // `mark_free`'d by the classic path). M5: `AllocCore` allocates NO
        // `Vec`/`Box` (the reentrancy-free invariant), so a fixed stack array
        // is the only sound choice here.
        #[cfg(feature = "alloc-runfreelist")]
        const FLUSH_RUN_DETECT_CAP: usize = 16;
        #[cfg(feature = "alloc-runfreelist")]
        let mut accepted_offs: [u32; FLUSH_RUN_DETECT_CAP] = [0; FLUSH_RUN_DETECT_CAP];
        #[cfg(feature = "alloc-runfreelist")]
        let mut accepted_n: usize = 0;

        // Capture the segment's CURRENT freelist head ONCE — the first accepted
        // block links to this (matching the first sequential `dealloc_small`,
        // whose `old_head` is exactly this value).
        let old_head = bt.head(class_idx);
        let mut prev_off = old_head; // next-target for the next accepted block
        let mut last_accepted: Option<u32> = None;
        // Track how many blocks were accepted, in source order, so the decommit
        // step can run per accepted block AFTER the run's single `set_head`.
        #[cfg(feature = "alloc-decommit")]
        let mut accepted_count: usize = 0;

        for &ptr in run {
            let off = (ptr as usize - base as usize) as u32;
            // Guard 1 (per-block): decommit stale-free `off >= bump`.
            #[cfg(feature = "alloc-decommit")]
            if (off as usize) >= bump {
                continue;
            }
            // Guard 2 (per-block): M2 double-free — skip a block already free
            // (e.g. a ring-DF'd magazine resident marked free by reclaim).
            if bm.is_free(off) {
                continue;
            }
            let block_nn = match NonNull::new(ptr) {
                Some(nn) => nn,
                None => continue,
            };
            // PERF-3 Ф2: record the accepted offset for the run-detection pass
            // (under `alloc-runfreelist`). The guard `accepted_n < CAP` keeps
            // the fixed array in bounds; an over-long run simply skips detection
            // for the tail (those blocks stay correctly on the linked list).
            #[cfg(feature = "alloc-runfreelist")]
            if accepted_n < FLUSH_RUN_DETECT_CAP {
                accepted_offs[accepted_n] = off;
                accepted_n += 1;
            }
            // Link this accepted block at the head of the run-local chain: its
            // `next` is the PRIOR accepted block's off (or the captured
            // `old_head` for the first accepted). Byte-identical to the LIFO
            // push each sequential `dealloc_small` performs.
            let next_ptr = if prev_off == FREE_LIST_NULL {
                core::ptr::null_mut()
            } else {
                Node::deref(base, prev_off as usize)
            };
            Node::write_next(block_nn, next_ptr);
            bm.mark_free(off);
            prev_off = off;
            last_accepted = Some(off);
            #[cfg(feature = "alloc-decommit")]
            {
                accepted_count += 1;
            }
        }

        // PERF-3 Ф2 (under `alloc-runfreelist` only): DIVERT contiguous-accepted
        // sub-runs away from the linked list we just built, into `RunStack`
        // descriptors. This runs AFTER the classic chain is fully built, so the
        // non-feature path above is byte-identical. A run-encoded block's `next`
        // word is never read on the drain path (Ф3 reconstructs by stride
        // arithmetic), and the linked-list head is repaired below to reference
        // ONLY the blocks that remain on the linked list. The bitmap stays
        // `mark_free` for every accepted block either way (sole ground truth).
        #[cfg(feature = "alloc-runfreelist")]
        {
            // `run_member[i]` is true iff `accepted_offs[i]` was successfully
            // diverted to a `RunStack` descriptor. `linked_count` counts the
            // blocks that STAY on the linked list (the complement).
            let mut run_member = [false; FLUSH_RUN_DETECT_CAP];
            let mut linked_count = accepted_n;
            if accepted_n >= 2 {
                // Step 1 — sort: build an index permutation `idx[..accepted_n]`
                // that sorts `accepted_offs` ascending. We permute INDICES (not
                // the array itself) so `run_member` lines up with the original
                // source-order slots (which is what the rebuild walk scans).
                // Insertion sort: n ≤ 16, branch-friendly, allocation-free.
                let mut idx: [usize; FLUSH_RUN_DETECT_CAP] =
                    [0; FLUSH_RUN_DETECT_CAP];
                let mut k = 0;
                while k < accepted_n {
                    idx[k] = k;
                    k += 1;
                }
                let mut a = 1;
                while a < accepted_n {
                    let mut b = a;
                    while b > 0 && accepted_offs[idx[b - 1]] > accepted_offs[idx[b]] {
                        idx.swap(b - 1, b);
                        b -= 1;
                    }
                    a += 1;
                }
                // Step 2 — detect: scan the SORTED order for contiguous sub-runs
                // of length ≥ 2 (offset-adjacent: `cur == prev + block_size`).
                // For each, attempt `RunStack::push`; on success mark every
                // member diverted. Overflow (push returns false) or a sub-run of
                // length 1 → those offsets stay on the linked list.
                let block_size = SizeClasses::block_size(class_idx);
                let mut i = 0;
                while i < accepted_n {
                    let mut j = i + 1;
                    while j < accepted_n {
                        let prev = accepted_offs[idx[j - 1]] as usize;
                        let cur = accepted_offs[idx[j]] as usize;
                        if cur != prev + block_size {
                            break;
                        }
                        j += 1;
                    }
                    let run_len = j - i;
                    if run_len >= 2 {
                        let start_off = accepted_offs[idx[i]];
                        if super::run_stack::RunStack::push(base, class_idx, start_off, run_len as u16) {
                            let mut m = i;
                            while m < j {
                                run_member[idx[m]] = true;
                                m += 1;
                            }
                            linked_count -= run_len;
                        }
                        // Overflow: the whole sub-run stays linked (run_member
                        // remains false for every member) — the classic chain
                        // built in the guard pass stands unchanged for them.
                    }
                    i = j;
                }
            }

            // Step 3 — rebuild: if ANY offsets were diverted, re-link the
            // COMPLEMENT (non-diverted blocks) into a fresh LIFO chain tipped by
            // `old_head`, so the linked list references ONLY non-diverted
            // blocks. We walk `accepted_offs` in SOURCE order (index 0..n),
            // skipping diverted members; the resulting chain is a valid LIFO
            // push of the complement onto `old_head` (the order among complement
            // blocks does not matter for correctness — each becomes head in
            // turn, pointing at the prior — and Ф3's drain walks the chain via
            // `read_next`, not by offset order). If NOTHING was diverted,
            // `linked_count == accepted_n` and the already-built chain stands.
            if linked_count != accepted_n {
                prev_off = old_head;
                last_accepted = None;
                let mut m = 0;
                while m < accepted_n {
                    if !run_member[m] {
                        let off = accepted_offs[m];
                        let block_ptr = Node::deref(base, off as usize);
                        // `block_ptr` is a non-null in-segment address (it came
                        // from a real accepted pointer); `NonNull::new` always
                        // succeeds. The `None` arm is dead but handled for
                        // robustness (skip on a paradoxical null).
                        if let Some(nn) = NonNull::new(block_ptr) {
                            let next_ptr = if prev_off == FREE_LIST_NULL {
                                core::ptr::null_mut()
                            } else {
                                Node::deref(base, prev_off as usize)
                            };
                            // `mark_free` already fired in the guard pass — NOT
                            // repeated (the bitmap is already correct; sole
                            // ground truth, plan §2.3).
                            Node::write_next(nn, next_ptr);
                            prev_off = off;
                            last_accepted = Some(off);
                        }
                    }
                    m += 1;
                }
            }
            // `accepted_count` (used by the decommit pass below) counts EVERY
            // accepted block — including diverted ones — because every accepted
            // block decrements `live_count` exactly once, regardless of
            // representation. Do NOT substitute `linked_count` here.
        }

        // Write the new head ONCE (only if ≥1 block was accepted). Mirrors the
        // final `set_head` of the last sequential `dealloc_small` in the run.
        if let Some(off) = last_accepted {
            bt.set_head(class_idx, off);
        }

        // E3 (task W4): batched `dec_live` (AFTER `set_head`, matching the
        // sequential ordering). `live` can only reach 0 at the LAST accepted
        // block (see `flush_run`'s doc), so one `sub_live(accepted_count)` + a
        // single decommit check is byte-identical to the former per-accepted-block
        // `dec_live_and_maybe_decommit` loop — at most one decommit fires, on the
        // same transition, under the same proviso. Recycle the slot if it fired.
        #[cfg(feature = "alloc-decommit")]
        {
            let small_cur = self.small_cur;
            if Self::dec_live_batch_and_maybe_decommit(base, accepted_count as u32, small_cur) {
                self.table.recycle(base);
            }
        }
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
    fn classify(size: usize, align: usize) -> AllocKind {
        match SizeClasses::class_for(size, align) {
            Some(class_idx) => AllocKind::Small { class_idx },
            None => AllocKind::Large,
        }
    }

    /// Allocate a small block of the given class. Routes through the current
    /// small segment's free list (pop); on a miss, scans ALL owned segments for
    /// one with a non-empty class free list (Phase 12.1: free state lives in
    /// per-segment `BinTable`s, so a freed block in a non-current segment must
    /// be reusable — otherwise non-current segments leak unboundedly); only
    /// then carves a fresh block / reserves a fresh segment. When carving, also
    /// carves a refill batch (Phase 9 amortisation), pushing each extra block
    /// into its OWN segment's `BinTable` via `segment_base_of` (defect A fix:
    /// never a captured "current" pointer).
    ///
    /// Phase 12.5 (shard model): a heap owns its segments exclusively — there
    /// is no adoption hook. On a free-list miss it carves/reserves from its
    /// OWN segments only. Cross-thread frees arrive via the inline TFS and are
    /// drained by `HeapCore::alloc` BEFORE this runs, so they are already on
    /// the per-segment BinTables by the time we scan.
    #[inline(always)]
    fn alloc_small(&mut self, class_idx: usize) -> *mut u8 {
        let block_size = SizeClasses::block_size(class_idx);
        debug_assert!(block_size >= NODE_SIZE);
        // 1. Try the free list of the current small segment.
        if let Some(ptr) = self.pop_free(self.small_cur, class_idx, block_size) {
            return ptr;
        }
        // 2. Current segment's class free list is empty: scan the OTHER owned
        //    segments for one with a non-empty class free list. A freed block
        //    may live in any segment we own (Phase 12.1 segment-centric free
        //    state); without this scan those blocks would leak. O(segments)
        //    only on a free-list miss — acceptable for 12.1 (per-class
        //    segment queues are a Phase 13 speed optimisation, not a 12.1
        //    deliverable). M5-safe: pure arithmetic + head reads via `Node`,
        //    no allocation.
        if let Some(seg) = self.find_segment_with_free(class_idx) {
            if let Some(ptr) = self.pop_free(seg, class_idx, block_size) {
                return ptr;
            }
        }
        // 3. No free block anywhere: carve a FRESH block. On the cold carve
        //    path we also carve a refill batch (Phase 9 amortisation) so the
        //    next allocs pop from the free list instead of carving one-by-one.
        //    Each refilled block is pushed into its OWN segment's BinTable
        //    (via `segment_base_of(ptr)`), never a captured "current" pointer
        //    — defect A fix: `small_cur` may shift mid-batch when a segment
        //    fills, and a captured pointer would then target the wrong
        //    segment, corrupting its BinTable head.
        if let Some(ptr) = self.carve_block_with_refill(class_idx, block_size) {
            return ptr;
        }
        // 4. Current segment is full: reserve a new small segment and retry.
        match self.reserve_small_segment() {
            Some(_) => {
                // Retry once on the fresh segment. Recurse-free: a single
                // direct retry (not a loop that could grow unboundedly).
                if let Some(ptr) = self.pop_free(self.small_cur, class_idx, block_size) {
                    return ptr;
                }
                // no-panic: a fresh small segment is guaranteed by construction
                // to have room for at least one block of every small class
                // (compile-time sanity: `small_meta_end() + PAGE <= SEGMENT`,
                // and every class block fits in a page). If carve_block returns
                // None here it indicates metadata corruption; we return null
                // (graceful OOM) rather than panicking — the GlobalAlloc face
                // (Phase 11) must never abort.
                self.carve_block_with_refill(class_idx, block_size)
                    .unwrap_or(core::ptr::null_mut())
            }
            None => core::ptr::null_mut(),
        }
    }

    /// Carve one fresh block of `class_idx` for the caller, plus a refill
    /// batch of extra blocks that are pushed onto their OWN segments'
    /// `BinTable[class_idx]` (Phase 9 amortisation, Phase 12.1 segment-centric
    /// free state). Each extra block's owning segment is derived per-block via
    /// `segment_base_of(ptr)` — `small_cur` may shift mid-batch when the
    /// current segment fills, so a captured pointer would corrupt the wrong
    /// segment's BinTable head (defect A).
    ///
    /// Returns the first carved block (for the caller), or `None` if the
    /// current segment cannot fit even one block (caller reserves a fresh
    /// segment and retries).
    fn carve_block_with_refill(&mut self, class_idx: usize, block_size: usize) -> Option<*mut u8> {
        // Carve the caller's block first.
        let first = self.carve_block(class_idx, block_size)?;
        // Refill batch: carve extra blocks and push each into its OWN segment.
        // `carve_block` returns None when the current segment is full; we stop
        // the batch there (the next alloc will reserve a fresh segment).
        //
        // Size chosen by measurement (Phase 13.5, task #29). Swept
        // {31, 63, 127, 255, 511} over the MT macro-bench (larson + mstress,
        // T=1/2/4 ops/sec — the load where refill actually bites) and the
        // single-threaded fixed-size churn micro-bench. Result: 31 is the
        // throughput winner. Larger batches do NOT help — they monotonically
        // HURT larson (working-set churn): T1/T2 larson fell from ~21–25 M to
        // ~14–18 M at 127–511, because a free-list miss now does up to 8×–16×
        // more upfront carve work (page faults, page-map writes) that the
        // steady-state churn never amortises. mstress was within noise and the
        // single-threaded churn was flat (~23–24 µs at every value — it pops
        // from the free list and never re-enters the cold carve). The §3.5
        // "raise toward a page of blocks (256–512)" hypothesis did not hold
        // under measurement; 31 stays. (Bigger upfront carve = worse locality
        // for the churn pattern, not better.)
        const REFILL_BATCH: usize = 31;
        for _ in 0..REFILL_BATCH {
            let Some(extra) = self.carve_block(class_idx, block_size) else {
                break;
            };
            let base = os::segment_base_of_ptr(extra);
            self.dealloc_small(base, extra, class_idx);
        }
        Some(first)
    }

    /// Scan all owned SMALL/PRIMORDIAL segments and return the base of the
    /// first one whose `BinTable[class_idx]` is non-empty. Used by
    /// [`alloc_small`] on a current-segment miss to reuse freed blocks in
    /// non-current segments (Phase 12.1: free state lives in per-segment
    /// `BinTable`s).
    ///
    /// **Large segments are excluded:** a large segment has no `BinTable`
    /// (only a header), so reading its `bin_table()` would dereference
    /// garbage and could return a bogus non-null head — leading `pop_free`
    /// to read a junk block and compute an out-of-segment `next` pointer
    /// (overflow/UAF). We read each candidate's header `kind` and skip
    /// non-small/primordial segments.
    ///
    /// Returns `None` if no owned small segment has a free block of this
    /// class.
    ///
    /// ## Slot recycle integration (task #60, `alloc-decommit`)
    ///
    /// Under `alloc-xthread` + `alloc-decommit`, the ring drain inside this
    /// function may trigger `dec_live_and_maybe_decommit` (via `reclaim_offset`)
    /// which decommits an empty segment. Slot recycling — `self.table.recycle(base)`
    /// — is deferred until AFTER the drain for that `base` is complete. This is
    /// critical: a partially-drained ring still has ring entries that
    /// `reclaim_offset` processes by reading the segment's metadata (which stays
    /// committed). Recycling before the drain ends would release the OS
    /// reservation prematurely — the metadata read in `magic_at` / `kind_at`
    /// would UAF. By recycling after the drain, we ensure:
    ///   a. All ring entries for `base` are processed (or safely skipped via
    ///      the `off >= bump` guard — bump was reset by decommit).
    ///   b. The OS release + slot NULL happen atomically in `recycle`, with no
    ///      window where the slot is non-NULL but the OS segment is gone.
    pub(crate) fn find_segment_with_free(&mut self, class_idx: usize) -> Option<*mut u8> {
        self.find_segment_with_free_impl(
            class_idx,
            #[cfg(feature = "alloc-xthread")]
            &|_, _| false,
        )
    }

    /// Task #164: variant with magazine predicate, called from
    /// `refill_class_bump` when the magazine is accessible.
    #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))]
    pub(crate) fn find_segment_with_free_checked<F: Fn(*mut u8, usize) -> bool>(
        &mut self,
        class_idx: usize,
        is_in_magazine: &F,
    ) -> Option<*mut u8> {
        self.find_segment_with_free_impl(class_idx, is_in_magazine)
    }

    #[cfg_attr(
        all(feature = "alloc-xthread", not(feature = "fastbin")),
        allow(unused_variables)
    )]
    #[inline]
    fn find_segment_with_free_impl<
        #[cfg(feature = "alloc-xthread")] F: Fn(*mut u8, usize) -> bool,
    >(
        &mut self,
        class_idx: usize,
        #[cfg(feature = "alloc-xthread")] is_in_magazine: &F,
    ) -> Option<*mut u8> {
        // Index-driven scan (task #126): walk slots `[0, count)` by index via
        // `SegmentTable::base_at`, instead of pre-collecting every live base
        // into an 8 KiB `[*mut u8; MAX_SEGMENTS]` stack buffer on every
        // free-list miss. `base_at` performs a single self-contained pointer
        // read (no borrow of `self.table` outlives the call), so it can be
        // freely interleaved with `self.table.recycle(base)` below — unlike
        // `self.table.bases()`, whose returned `impl Iterator` captures the
        // elided `&self` lifetime and would keep `self.table` borrowed for the
        // life of the loop, conflicting with the `&mut self.table.recycle`
        // call needed when a segment empties out mid-scan.
        //
        // This makes recycle UNBOUNDED within a single scan: however many
        // segments empty out (drained ring → decommit) during this call, each
        // is recycled the moment it is discovered — there is no fixed-size
        // buffer to overflow and no deferred/lost recycle (task #126 redo of
        // the Phase C attempt, which used a CAP=32 deferred-recycle ring that
        // silently dropped recycles for the 33rd+ emptied segment in one scan).
        let n = self.table.count() as usize;

        // Phase C (numa-aware): on the first pass we prefer segments whose
        // node_id matches the calling thread's NUMA node; we collect segments
        // from foreign nodes in `fallback` and return the first one only if
        // the first pass found nothing.
        //
        // Strategy (a) — "ignore migration": we call current_node() once per
        // find_segment_with_free invocation (not per allocation). If the thread
        // migrated between nodes mid-scan, we may prefer a now-wrong segment —
        // that is the accepted MVP trade-off (§4 of PHASE_NUMA_DESIGN.md).
        #[cfg(feature = "numa-aware")]
        let my_node = numa::current_node();
        // A single fallback slot: the first segment from a foreign node that has
        // a free block.  On a single-NUMA machine (or when numa-aware is off)
        // this path is never taken — all segments have node_id == my_node (or
        // NO_NODE_RAW, which is treated as "acceptable" / unknown).
        #[cfg(feature = "numa-aware")]
        let mut fallback: Option<*mut u8> = None;

        for i in 0..n {
            let base = self.table.base_at(i);
            if base.is_null() {
                // Recycled (NULL) slot — skip. `base_at` also returns NULL for
                // an out-of-range index, but `i < n == self.table.count()`
                // here, so a NULL here always means "recycled slot", never
                // "out of range".
                continue;
            }
            // Skip large/huge segments: they have no BinTable. Field-specific
            // `kind` read (task #33): this is the Owner's alloc path,
            // concurrent with a Remote's `dealloc_routing` field reads — a
            // full-struct `read_at` here would race them. `kind_at` reads only
            // the `kind` byte, disjoint from any writer.
            if !matches!(
                SegmentHeader::kind_at(base),
                SegmentKind::Small | SegmentKind::Primordial
            ) {
                continue;
            }
            // Variant-2: lazily drain this segment's remote-free ring before
            // inspecting its BinTable. Cross-thread frees that targeted THIS
            // segment (a segment we own but are not currently allocating from)
            // are sitting in its ring; without this drain they would never
            // reach the BinTable and the scan would miss them.
            //
            // Under `alloc-decommit`, if draining empties the segment it is
            // decommitted inside `reclaim_offset`. We track whether a decommit
            // fired via the `decommit_happened` flag, then recycle the slot
            // AFTER the drain completes — not during — so that any remaining
            // ring entries for `base` can still safely read the (still-committed)
            // metadata via `magic_at`/`kind_at`/`bump_of`.
            #[cfg(feature = "alloc-xthread")]
            {
                let ring = SegmentMeta::new(base).remote_ring();
                let small_cur = self.small_cur;
                #[cfg(feature = "alloc-decommit")]
                let mut decommit_happened = false;
                ring.drain(|off| {
                    // Task #164: when a magazine exists (fastbin), use the
                    // checked variant that consults the magazine predicate
                    // before `write_next`, closing the in-magazine leg of the
                    // ring↔magazine cross-thread double-free residual.
                    #[cfg(feature = "fastbin")]
                    let reclaimed =
                        Self::reclaim_offset_checked(base, off, small_cur, &is_in_magazine);
                    #[cfg(not(feature = "fastbin"))]
                    let reclaimed = Self::reclaim_offset(base, off, small_cur);
                    #[cfg(feature = "alloc-decommit")]
                    if reclaimed {
                        decommit_happened = true;
                    }
                    #[cfg(not(feature = "alloc-decommit"))]
                    {
                        let _ = reclaimed;
                    }
                });
                // Slot recycle: now that the drain is complete, it is safe to
                // release the OS reservation and NULL the slot. Any stale ring
                // entries have already been processed (and guarded by `off >= bump`).
                #[cfg(feature = "alloc-decommit")]
                if decommit_happened {
                    self.table.recycle(base);
                    // This base is now recycled; skip the BinTable check.
                    continue;
                }
            }
            let meta = SegmentMeta::new(base);
            let bt = meta.bin_table();
            if bt.head(class_idx) != FREE_LIST_NULL {
                // Phase C (numa-aware): check whether this segment belongs to
                // our NUMA node.  Segments with node_id == NO_NODE_RAW are
                // "unknown" — treat them as local (no penalty, and on platforms
                // without NUMA they all carry NO_NODE_RAW so this degrades
                // gracefully to the pre-NUMA single-pass behaviour).
                #[cfg(feature = "numa-aware")]
                {
                    let seg_node = meta.node_id_of();
                    if seg_node != my_node && seg_node != super::segment_header::NO_NODE_RAW {
                        // Foreign-node segment with a free block.  Remember as
                        // fallback if we find nothing local, then keep scanning.
                        if fallback.is_none() {
                            fallback = Some(base);
                        }
                        continue;
                    }
                    // Local or unknown node — use it immediately.
                    return Some(base);
                }
                // Without numa-aware: same as before — return the first match.
                #[cfg(not(feature = "numa-aware"))]
                return Some(base);
            }
        }
        // First pass found no local segment with a free block; fall back to
        // the first foreign-node segment we recorded (or None if everything is
        // empty / all recycled).
        #[cfg(feature = "numa-aware")]
        return fallback;
        #[cfg(not(feature = "numa-aware"))]
        None
    }

    /// Pop a free block of `class_idx` from `segment`'s bin table. Returns
    /// null if the free list is empty. Writes the block's `next` word to null
    /// (it becomes the new head) via the node seam.
    #[inline(always)]
    fn pop_free(&self, segment: *mut u8, class_idx: usize, block_size: usize) -> Option<*mut u8> {
        #[cfg(feature = "alloc-decommit")]
        let mut meta = SegmentMeta::new(segment);
        #[cfg(not(feature = "alloc-decommit"))]
        let meta = SegmentMeta::new(segment);
        let mut bt = meta.bin_table();
        let head_off = bt.head(class_idx);
        if head_off == FREE_LIST_NULL {
            return None;
        }
        let block_ptr = Node::deref(segment, head_off as usize);
        let block_nn = NonNull::new(block_ptr)?;
        let next = Node::read_next(block_nn);
        let new_head = if next.is_null() {
            FREE_LIST_NULL
        } else {
            // Compute the offset of `next` relative to this segment. `next`
            // is an absolute pointer into the same segment (free lists are
            // per-segment), so offset = next - segment.
            (next as usize - segment as usize) as u32
        };
        bt.set_head(class_idx, new_head);
        // Phase 13.4a: clear the block's bitmap bit — it leaves the free list
        // and is handed to the caller, so a subsequent free must NOT see it as
        // already-free (and the next legitimate free must be able to re-mark it).
        meta.alloc_bitmap().mark_alloc(head_off);
        // Phase 35 (M6): a block left the free list and is handed to the caller
        // → one more live block in this segment. Owner-only counter. A popped
        // block always comes from a COMMITTED payload (a decommitted segment was
        // reset to an empty free list, so `pop_free` finds nothing there), so no
        // recommit is needed on this path — only `carve_block` writes fresh
        // payload and thus recommits.
        #[cfg(feature = "alloc-decommit")]
        meta.inc_live();
        // X7 Ф3 (task #191) touch (a): bump the generation at ISSUE. `pop_free`
        // hands a block directly to the caller (it is the non-magazine substrate
        // pop, reachable from `alloc_small`). Under `hardened` (which implies
        // `fastbin`), `HeapCore::alloc` routes small blocks through the magazine
        // and never reaches here — but a direct `AllocCore` consumer (or a future
        // config change) could, so the bump is placed at this issue point for
        // correctness and defense-in-depth. The magazine refill path uses
        // `drain_freelist_batch` (which fills `out`, NOT issuing to a caller), so
        // blocks pulled into the magazine are NOT bumped here — they are bumped
        // on their later magazine pop. Compiled ONLY under `hardened`.
        #[cfg(feature = "hardened")]
        {
            super::segment_header::bump_gen(segment, head_off as usize);
        }
        let _ = block_size; // block_size is the caller's invariant; not needed here.
        Some(block_ptr)
    }

    /// Э7 (task #161) — **batch freelist drain**. Pop up to `out.len()` free
    /// blocks of class `class_idx` from `segment`'s `BinTable[class_idx]` in ONE
    /// walk, writing them into `out[..k]` and returning `k` (the number popped,
    /// `0` if the free list was empty). Byte-identical end-state to calling
    /// [`pop_free`] `k` times, but with the per-block round-trip HOISTED:
    ///
    ///   - `head` is read ONCE (not re-read from the `BinTable` per block).
    ///   - `set_head` is written ONCE at the end, to the first UN-popped node
    ///     (or `FREE_LIST_NULL` if the chain was exhausted before `out` filled).
    ///   - `inc_live` is applied ONCE by `k` (under `alloc-decommit`), exactly
    ///     equalling `k` individual `inc_live`s.
    ///
    /// The two per-block costs that MUST stay per-block are kept per-block:
    ///
    ///   - `read_next(block)` — the dependent load that walks the intrusive
    ///     chain. mimalloc pays this too; there is no way to hoist it (each
    ///     `next` lives in the previous block's body). We never WRITE the block
    ///     body on this path (pop doesn't), so reading `next` before advancing
    ///     is hazard-free: nothing overwrites a block between our read of its
    ///     `next` and our recording it.
    ///   - `mark_alloc(off)` — cleared per-block. **Decision: per-block, NOT
    ///     merged.** A freelist is a LIFO push chain, so consecutive popped
    ///     offsets are in general SCATTERED across the bitmap (they do not share
    ///     a byte the way a flush batch of consecutive carves would). Merging
    ///     the RMWs across blocks would only be byte-identical for offsets that
    ///     share a bitmap byte, which is not guaranteed here — so we keep the
    ///     per-block `mark_alloc`, which is trivially identical to `pop_free`'s.
    ///     The batch win is the hoisted `set_head` / `head`-read / `inc_live`,
    ///     NOT the bitmap RMW (which was never the expensive part).
    ///
    /// ## D1 / M2 / set_head correctness
    ///
    ///   - **D1:** exactly `k` blocks leave the free list and are handed out, so
    ///     `inc_live` by `k` == `k` per-block `inc_live`s. No double, no
    ///     under-count.
    ///   - **M2:** every recorded block ends bitmap-ALLOCATED (bit cleared) via
    ///     its own `mark_alloc`, exactly as `pop_free` leaves it. A later
    ///     double-free still hits `is_free` correctly.
    ///   - **set_head:** after the walk, `head` holds either the offset of the
    ///     first un-popped node (chain longer than `out`) or `FREE_LIST_NULL`
    ///     (chain exhausted). We `set_head` to that once. A subsequent
    ///     `pop_free`/drain therefore yields exactly the remaining blocks in the
    ///     same order.
    ///
    /// `&self` (not `&mut self`): identical borrow profile to `pop_free` — it
    /// touches only `segment` metadata via `SegmentMeta`, never `self.table`,
    /// so `refill_class_bump` can call it on a `find_segment_with_free`-returned
    /// base without an aliasing conflict.
    #[inline]
    fn drain_freelist_batch(
        &self,
        segment: *mut u8,
        class_idx: usize,
        out: &mut [*mut u8],
    ) -> usize {
        if out.is_empty() {
            return 0;
        }
        #[cfg(feature = "alloc-decommit")]
        let mut meta = SegmentMeta::new(segment);
        #[cfg(not(feature = "alloc-decommit"))]
        let meta = SegmentMeta::new(segment);
        let mut bt = meta.bin_table();

        // PERF-3 Ф3 (task #210): under `alloc-runfreelist`, FIRST drain the
        // per-segment `RunStack` for this class — reconstructing each
        // descriptor's member blocks by stride arithmetic
        // (`start_off + i * block_size`) instead of the dependent-load pointer
        // chase the classic linked-list walk pays (plan §1 — the attacked
        // mechanism). The `RunStack` holds compact `(start_off, count)`
        // descriptors that Ф2's `flush_run` pushed for contiguous-accepted
        // sub-runs; singletons and overflow stayed on the linked list and are
        // drained second by the unchanged loop below.
        //
        // The two bodies below (`cfg(feature)` and `cfg(not)`) are a SPLIT, not
        // an additive `#[cfg]` block, because the feature-on path must NOT take
        // the classic early-return on `head == NULL` (the RunStack may hold
        // blocks for a class whose linked-list head is NULL — that is the
        // all-run-no-singleton case Ф2 produces) and must gate `set_head` on
        // whether the linked-list walk actually ran (only-touch-changed-state).
        // The `cfg(not)` body is byte-identical to the pre-Ф3 body: the classic
        // early-return, unconditional `set_head`, and `inc_live(k)` stand
        // unchanged (the production-judge neutrality gate).
        #[cfg(feature = "alloc-runfreelist")]
        {
            let mut bm = meta.alloc_bitmap();
            let mut k = 0usize;
            let block_size = SizeClasses::block_size(class_idx);
            // Pop descriptors one at a time; for each, reconstruct every member
            // offset, guard it, and hand it out. Stop as soon as `out` is full.
            while k < out.len() {
                let desc = match super::run_stack::RunStack::pop(segment, class_idx) {
                    Some(d) => d,
                    None => break, // RunStack exhausted for this class → fall through.
                };
                let start = desc.start_off as usize;
                let mut i = 0usize;
                while i < desc.count as usize && k < out.len() {
                    let off = (start + i * block_size) as u32;
                    // **Defense-in-depth M2 guard (plan §2.3 decision 1,
                    // load-bearing).** The `AllocBitmap` is the SOLE ground
                    // truth; a `RunDesc` is only a reconstruction HINT. Every
                    // reconstructed offset is re-checked against the bitmap
                    // BEFORE being handed out, exactly as the linked-list drain
                    // does. The fallthrough (a reconstructed offset that is NOT
                    // free) is UNREACHABLE in correct operation: per plan §2.4's
                    // structural proof, a run-member block is FREE in the bitmap
                    // by construction (Ф2's flush guard accepted it then
                    // `mark_free`'d it), and no path transitions it to ALLOCATED
                    // except this very drain. BUT the guard must still exist and
                    // FAIL SAFE (skip that one slot, no panic, no batch abort):
                    // it is what prevents a double-issue if a future bug ever
                    // lets a non-free block appear in a descriptor, and it is
                    // what makes the M2-double-free-through-run counterfactual
                    // test (`regression_run_stack_drain.rs`) have teeth.
                    if bm.is_free(off) {
                        // FREE → ALLOC: hand the block out. This RMW is identical
                        // to the linked-list drain's `mark_alloc` (same
                        // invariant, same per-block cost — plan §2.3); the run
                        // descriptor only changes HOW the address is obtained.
                        bm.mark_alloc(off);
                        out[k] = Node::deref(segment, off as usize);
                        k += 1;
                    }
                    // else: UNREACHABLE in correct operation. Fail safe: skip
                    // this one reconstructed slot and continue to the next
                    // member. Do NOT add it to `out`.
                    i += 1;
                }
                // PERF-3 Ф4 (task #211): if `out` filled MID-descriptor (`i <
                // desc.count` when the inner loop exited because `k ==
                // out.len()`), the remaining `desc.count - i` members are still
                // FREE in the bitmap but the descriptor was just popped+cleared
                // — without this pushback those members would be LOST (a leak:
                // FREE-in-bitmap, on no linked list, referenced by no
                // descriptor, until a decommit/recommit resets the segment).
                //
                // This gap (found by @o46m's Ф3 review) is REAL and reachable
                // for small classes with `block_size > 8192 B`: there
                // `refill_n_for_class(block_size) = clamp(64 KiB / block_size,
                // 1, 16) < FLUSH_N (= 8)`, so a refill's `out` capacity is
                // SMALLER than a full-batch contiguous run's descriptor count
                // (up to `FLUSH_N = 8`). The inner loop then fills `out`
                // partway through the descriptor and the tail leaks.
                //
                // Fix: push a TRUNCATED REMAINDER descriptor covering exactly
                // the un-drained members `[start + i*block_size, count - i)`
                // back onto the `RunStack`. It is safe because (a) those
                // members are still FREE (the inner loop only `mark_alloc`'d
                // members `[0, i)`), and (b) the push ALWAYS succeeds: the slot
                // we just popped is now empty, so among the `RUNSTACK_CAPACITY`
                // (= 8) slots at most 7 are occupied → `push` finds the empty
                // popped slot. (If a concurrent push had raced it could in
                // principle fill the slot, but the `RunStack` is single-writer
                // — plan §"No atomics" — owned by this thread's drain, so no
                // race exists.) The next `drain_freelist_batch` call pops the
                // remainder and continues draining from `i`.
                //
                // This is the same phase's algorithm (Ф3's drain), not scope
                // creep: Ф3's pop-then-iterate design assumed `count <=
                // out.len()` (true only for `block_size <= 8192`), and this
                // closes the assumption for the large-small-class tail.
                if i < desc.count as usize {
                    let rem_start = (start + i * block_size) as u32;
                    let rem_count = desc.count - i as u16;
                    let pushed = super::run_stack::RunStack::push(
                        segment,
                        class_idx,
                        rem_start,
                        rem_count,
                    );
                    // The pushback MUST succeed (we just freed one slot; single-
                    // writer). A `false` here would indicate a capacity
                    // invariant violation (more than `RUNSTACK_CAPACITY`
                    // simultaneous live descriptors for one class from a single
                    // drain) — unreachable, but assert in debug builds to catch
                    // a future regression in the push/pop slot discipline.
                    debug_assert!(
                        pushed,
                        "RunStack pushback of a drained-descriptor remainder \
                         failed: capacity invariant violated (class {class_idx}, \
                         rem_count {rem_count})"
                    );
                    // `out` is full → the outer `while k < out.len()` exits
                    // next iteration. The remainder is safely on the stack.
                }
            }

            // THEN drain the classic linked list for any remaining capacity.
            // Read the head ONCE.
            let mut head_off = bt.head(class_idx);
            // `linked_walked` gates `set_head`: only state that actually changed
            // is touched. If the RunStack alone filled `out` and the linked list
            // was never walked, `set_head` is skipped (mirrors Ф2's rebuild-step
            // discipline for the mixed-representation case).
            let mut linked_walked = false;
            while k < out.len() && head_off != FREE_LIST_NULL {
                let block_ptr = Node::deref(segment, head_off as usize);
                let block_nn = match NonNull::new(block_ptr) {
                    Some(nn) => nn,
                    None => break,
                };
                let next = Node::read_next(block_nn);
                bm.mark_alloc(head_off);
                out[k] = block_ptr;
                k += 1;
                linked_walked = true;
                head_off = if next.is_null() {
                    FREE_LIST_NULL
                } else {
                    (next as usize - segment as usize) as u32
                };
            }
            if linked_walked {
                bt.set_head(class_idx, head_off);
            }
            // `inc_live` ONCE by the TOTAL `k` across both representations (D1).
            #[cfg(feature = "alloc-decommit")]
            for _ in 0..k {
                meta.inc_live();
            }
            return k;
        }

        // --- `#[cfg(not(feature = "alloc-runfreelist"))]`: byte-identical to
        //     the pre-Ф3 body (the production path — judge neutrality gate).
        #[cfg(not(feature = "alloc-runfreelist"))]
        {
            // Read the head ONCE.
            let mut head_off = bt.head(class_idx);
            if head_off == FREE_LIST_NULL {
                return 0;
            }
            let mut bm = meta.alloc_bitmap();
            let mut k = 0usize;
            while k < out.len() && head_off != FREE_LIST_NULL {
                let block_ptr = Node::deref(segment, head_off as usize);
                let block_nn = match NonNull::new(block_ptr) {
                    Some(nn) => nn,
                    // A null-deref would only arise from a corrupt offset; stop the
                    // walk here and commit what we have (defence-in-depth). `head`
                    // is left pointing at this node so nothing is lost.
                    None => break,
                };
                // Dependent load: read this block's `next` BEFORE recording it. The
                // block body is never written on the pop path, so this is race-free
                // against ourselves.
                let next = Node::read_next(block_nn);
                // Clear this block's bitmap bit — it leaves the free list and is
                // handed out (per-block, byte-identical to `pop_free`).
                bm.mark_alloc(head_off);
                out[k] = block_ptr;
                k += 1;
                head_off = if next.is_null() {
                    FREE_LIST_NULL
                } else {
                    // `next` is an absolute pointer into the SAME segment (free
                    // lists are per-segment), so offset = next - segment.
                    (next as usize - segment as usize) as u32
                };
            }
            // Write the new head ONCE: the first un-popped node, or NULL.
            bt.set_head(class_idx, head_off);
            // `inc_live` ONCE by `k` (D1): exactly `k` blocks were handed out. A
            // popped block always comes from a COMMITTED payload (a decommitted
            // segment was reset to an empty free list, so the drain finds nothing
            // there), so no recommit is needed on this path.
            #[cfg(feature = "alloc-decommit")]
            for _ in 0..k {
                meta.inc_live();
            }
            k
        }
    }

    /// Carve a fresh `block_size`-aligned block from the current small
    /// segment's bump cursor. Returns None if the segment is full.
    ///
    /// On a page boundary crossing, marks the freshly entered page as owned by
    /// `class_idx` in the page map (the page-dedication rule).
    fn carve_block(&mut self, class_idx: usize, block_size: usize) -> Option<*mut u8> {
        let segment = self.small_cur;
        let mut meta = SegmentMeta::new(segment);
        // Field-specific bump read/write (task #33 root-cause fix): the Owner
        // touches ONLY the `bump` field, never the cross-thread-read header
        // fields. A full-struct `write_header` here rewrote `magic`/`kind`/
        // `owner_thread_free` too, racing a Remote's full-struct `read_at` in
        // `dealloc_routing` (the §11 data race). `bump` is owner-only (no
        // Remote reads it), so a plain field write is race-free.
        let bump = meta.bump_of();
        let aligned_bump = align_up(bump, block_size);
        if aligned_bump + block_size > SEGMENT {
            return None;
        }
        // Phase 35 (M6 recommit): if this segment's payload was decommitted (it
        // emptied and we returned its pages to the OS), we are about to write
        // into the payload — recommit the whole payload range and clear the flag
        // BEFORE the bump cursor advances / the page-map / the block is touched.
        // The reset that accompanied decommit left `bump == small_meta_end`, so
        // a decommitted segment is always carved from its payload start; the
        // simplest correct recommit is the whole `[small_meta_end, SEGMENT)`
        // payload at once (per §4 of the design — pessimistic but correct, and
        // a recommit only happens on the first reuse after an empty→decommit).
        #[cfg(feature = "alloc-decommit")]
        if meta.is_decommitted() {
            os::recommit_pages(segment, SegLayout::small_meta_end(), SEGMENT);
            meta.set_decommitted(false);
        }
        // Update ONLY the bump cursor.
        meta.set_bump(aligned_bump + block_size);
        // Phase 35: this carved block is now live (handed to the caller, or — on
        // the refill path — immediately pushed to the free list, which calls
        // `dealloc_small` → `dec_live`, netting zero for refill blocks; the
        // caller's block keeps the +1). Owner-only counter, plain field bump.
        #[cfg(feature = "alloc-decommit")]
        meta.inc_live();
        // Mark the page containing `aligned_bump` as owned by `class_idx`.
        let mut pm = meta.page_map();
        let page = aligned_bump / super::os::PAGE;
        if pm.class_of(page).is_none() {
            // Page was Free or Meta; dedicate it to this class.
            pm.set_class(page, class_idx);
        }
        let ptr = Node::deref(segment, aligned_bump);
        Some(ptr)
    }

    /// E1 (task W4) — **batched bump-carve**. Carve a RUN of up to `out.len()`
    /// `block_size`-strided blocks from the current small segment's bump cursor
    /// in ONE shot, writing them into `out[..n]` and returning `n` (0 if the
    /// segment cannot fit even one block — the caller reserves a fresh segment,
    /// exactly as it does on `carve_block` → `None`).
    ///
    /// ## Byte-identical to `n` sequential `carve_block`s — what is HOISTED
    ///
    /// A run of `carve_block(class_idx, block_size)` calls, after the FIRST,
    /// always finds `bump` already `block_size`-aligned (the previous carve left
    /// `bump = aligned_prev + block_size`, and every class `block_size` is a
    /// multiple of `MIN_BLOCK`), so `align_up(bump, block_size)` is a TAUTOLOGY
    /// from the second block on. We therefore align ONCE (`aligned_start`), then
    /// stride by `block_size`. The following are hoisted across the run because
    /// none of them can change mid-run (a carve run touches only owner-only
    /// bump/live/page-map state, and no free/decommit runs between carves):
    ///   - `SegmentMeta::new` + `bump_of()` LOAD — once (bump only advances by
    ///     our own writes; we track it locally).
    ///   - `align_up` div — once (tautological after block 0).
    ///   - `set_bump` STORE — once, to `aligned_start + n*block_size` (identical
    ///     to the last sequential carve's final bump).
    ///   - `live += n` — one batched saturating add (D1: exactly `n` handed out,
    ///     byte-identical to `n` `inc_live`s; owner-only counter, intermediate
    ///     states unobservable — same argument as `drain_freelist_batch`).
    ///   - `is_decommitted()` check + recommit — once at run start (the flag is
    ///     set only in the decommit path, which cannot run mid-carve).
    ///
    /// ## What STAYS per-block (NOT tautologies)
    ///   - The page-map `class_of`/`set_class` "first class wins" marking is
    ///     applied per DISTINCT payload page: we compute the page of each block
    ///     and call `set_class` only when the page index CHANGES from the prior
    ///     block (byte-identical to `carve_block`'s per-block "mark only if
    ///     `class_of(page).is_none()`", since within a run the first block to
    ///     enter a page is the one that dedicates it, and later same-page blocks
    ///     find it already `Some` → no-op). For `block_size > PAGE` every block
    ///     lands on a fresh page, so this degrades to per-block correctly.
    ///
    /// ## M2 / D1 / boundary — preserved EXACTLY
    ///   - M2: carve NEVER touches the alloc bitmap (a bump-carved block is
    ///     already bit0=allocated, the M2 convention) — identical to `carve_block`.
    ///   - D1: `+n` for the `n` blocks handed out.
    ///   - Boundary: `n = min(out.len(), room)` where
    ///     `room = (SEGMENT - aligned_start) / block_size`, so
    ///     `aligned_start + n*block_size <= SEGMENT` — the same
    ///     `aligned + block_size > SEGMENT` per-block check, batched.
    fn carve_batch(&mut self, class_idx: usize, block_size: usize, out: &mut [*mut u8]) -> usize {
        if out.is_empty() {
            return 0;
        }
        let segment = self.small_cur;
        let mut meta = SegmentMeta::new(segment);
        let bump = meta.bump_of();
        let aligned_start = align_up(bump, block_size);
        if aligned_start + block_size > SEGMENT {
            return 0; // not room for even one block
        }
        // Recommit ONCE at run start if the segment's payload was decommitted
        // (identical to `carve_block`'s per-block check — the flag cannot change
        // mid-run, so one check covers the whole run).
        #[cfg(feature = "alloc-decommit")]
        if meta.is_decommitted() {
            os::recommit_pages(segment, SegLayout::small_meta_end(), SEGMENT);
            meta.set_decommitted(false);
        }
        // How many blocks fit from `aligned_start` to the segment end, capped by
        // the caller's slice.
        let room = (SEGMENT - aligned_start) / block_size;
        let n = out.len().min(room);
        // Advance the bump cursor ONCE to just past the last carved block —
        // byte-identical to the final `set_bump` of the n-th sequential carve.
        meta.set_bump(aligned_start + n * block_size);
        // Batched live increment (D1): exactly `n` blocks handed out.
        #[cfg(feature = "alloc-decommit")]
        meta.add_live(n as u32);
        // Page-map "first class wins", applied once per DISTINCT page entered by
        // this run. `carve_block` marks a page iff it was not already owned; the
        // first block to land on a page is the one that dedicates it, so calling
        // `set_class` on each page-index CHANGE reproduces that exactly.
        let mut pm = meta.page_map();
        let mut prev_page = usize::MAX;
        for (i, slot) in out[..n].iter_mut().enumerate() {
            let off = aligned_start + i * block_size;
            let page = off / super::os::PAGE;
            if page != prev_page {
                if pm.class_of(page).is_none() {
                    pm.set_class(page, class_idx);
                }
                prev_page = page;
            }
            *slot = Node::deref(segment, off);
        }
        n
    }

    /// Deallocate a small block: push it onto its owning segment's class free
    /// list. `ptr` is the block address; `base` is its segment base (computed
    /// by the caller via `segment_of`).
    ///
    /// **Double-free guard (M2 — Phase 13.4a):** before pushing, we test the
    /// segment's [`AllocBitmap`](super::alloc_bitmap::AllocBitmap) bit for this
    /// block. If it is already FREE (`is_free` true → the block is on some free
    /// list of this segment), this is a double-free: we no-op (never corrupt the
    /// free list — no self-loop, no duplicate). Otherwise we set the bit
    /// (`mark_free`) and push. This replaces the Phase 8 O(free-list-length)
    /// `free_list_contains` walk — which made own-thread free O(N²) under churn
    /// (#41) — with an O(1) exact bit test. The bitmap is single-writer (owner
    /// only), so the read/modify/write needs no atomics.
    #[inline(always)]
    fn dealloc_small(&mut self, base: *mut u8, ptr: *mut u8, class_idx: usize) {
        let meta = SegmentMeta::new(base);
        let mut bt = meta.bin_table();
        let off = (ptr as usize - base as usize) as u32;
        // ── H1 (task #167): interior-pointer guard (HARDENED) ───────────────
        // The SAME guard as `HeapCore::dealloc_own_thread_with_base`'s magazine
        // free path, here on the SUBSTRATE own-thread free — the path the
        // explicit `Heap` face (`with_heap` → `Heap::dealloc_small` →
        // `self.core.dealloc`) and any direct `AllocCore` user reach (the
        // magazine guard only covers the `SeferAlloc` face). A real block start
        // of class `class_idx` sits at an `off` that is a whole multiple of
        // `block_size(class_idx)` (carve aligns the bump to `block_size`); an
        // INTERIOR pointer has `off % block_size != 0` and would otherwise slip
        // past the 16 B-granular `is_free` bitmap oracle below (it maps to a
        // DIFFERENT bit that reads "allocated") → `write_next` into mid-block →
        // free-list corruption. Rejected here as a no-op. A `%` by a
        // non-power-of-two `block_size` per small free — a paid check, so
        // `hardened`-gated (default OFF), never on the production hot path. The
        // CROSS-THREAD leg is already covered UNCONDITIONALLY by
        // `reclaim_offset`'s identical `off % block_size` defence-in-depth.
        #[cfg(feature = "hardened")]
        if !(off as usize).is_multiple_of(SizeClasses::block_size(class_idx)) {
            return;
        }
        // Phase 35 (M6 decommit) — the post-decommit stale-free guard. When a
        // segment empties it is decommitted AND reset: `bump` returns to
        // `small_meta_end()` and the alloc bitmap is zeroed. A late free / a
        // legitimate double-free of a block that lived in the now-decommitted
        // payload would (a) pass the zeroed bitmap `is_free` check and (b)
        // `write_next` into a DECOMMITTED / unmapped page — a UAF. Every block
        // that was ever carved has `off >= bump` ONLY after such a reset (a live
        // block in a committed segment always has `off < bump`); so rejecting
        // `off >= bump` closes the window with no false positive on a real free.
        // Owner-only `bump` read (single-writer), gated to the feature that
        // resets the bump.
        #[cfg(feature = "alloc-decommit")]
        if (off as usize) >= meta.bump_of() {
            return;
        }
        // O(1) exact double-free guard via the alloc bitmap.
        let mut bm = meta.alloc_bitmap();
        if bm.is_free(off) {
            return; // Already on a free list (M2 double-free): no-op.
        }
        let block_nn = match NonNull::new(ptr) {
            Some(nn) => nn,
            None => return,
        };
        let old_head = bt.head(class_idx);
        let old_head_ptr = if old_head == FREE_LIST_NULL {
            core::ptr::null_mut()
        } else {
            Node::deref(base, old_head as usize)
        };
        Node::write_next(block_nn, old_head_ptr);
        bt.set_head(class_idx, off);
        bm.mark_free(off);
        // Phase 35 (M6): one fewer live block in this segment; decommit if it
        // just emptied and is not the current carve target. Own-thread free runs
        // on the owner, so the counter stays single-writer.
        // Task #60 (slot recycle): if decommit fired, recycle the table slot
        // immediately — `dealloc_small` is NOT inside a ring drain (no stale
        // ring entries arrive here for `base` on the own-thread path), so the
        // metadata is readable, the slot can be NULLed, and the OS reservation
        // can be released right away.
        #[cfg(feature = "alloc-decommit")]
        if Self::dec_live_and_maybe_decommit(base, self.small_cur) {
            self.table.recycle(base);
        }
    }

    /// Allocate a large/huge block: reserve a dedicated segment sized to fit,
    /// place the allocation at the first page-aligned offset past the header,
    /// register the segment, and return the allocation pointer.
    ///
    /// **OPT-E (alloc-decommit):** before going to the OS, check the
    /// `large_cache` for a previously-freed segment that is large enough to
    /// satisfy the request. A cache hit avoids the full OS round-trip
    /// (mmap/VirtualAlloc + registration) at the cost of one recommit call
    /// (Windows only; unix is a no-op after MADV_DONTNEED).
    ///
    /// **Phase 2 (alloc-decommit):** runs one lazy decay tick before serving
    /// the request. Cost: one `Instant::now()` + one duration compare on the
    /// common path; actual eviction only when the interval has elapsed AND the
    /// cache is over the headroom target.
    fn alloc_large(&mut self, size: usize, align: usize) -> *mut u8 {
        // align >= SEGMENT is not serviceable by the dedicated-segment large
        // path: the block would land at base + SEGMENT-multiple (mis-registered
        // → dealloc leak → eventual MAX_SEGMENTS abort) or, for align >
        // SEGMENT, at a pointer only SEGMENT-aligned (GlobalAlloc contract
        // violation → UB). Reject with null — a legal alloc-failure signal —
        // rather than leak/misalign. (Task #130.)
        if align >= SEGMENT {
            return core::ptr::null_mut();
        }

        // Phase 2: lazy decay tick on every large allocation.
        #[cfg(feature = "alloc-decommit")]
        self.maybe_decay_large_cache();

        // The segment must hold: header + alignment padding + size, rounded up
        // to a whole number of segments. `Segment::reserve` does the rounding.
        let hdr_aligned = align_up(
            core::mem::size_of::<SegmentHeader>(),
            align.max(super::os::PAGE),
        );
        let needed = hdr_aligned + align_up(size, align);
        // Round up to a whole number of SEGMENT-sized spans — the same rounding
        // `Segment::reserve` does internally.  `reserve_aligned_on_node` (like
        // the OS `mmap`/`VirtualAlloc` path) requires the usable size to be an
        // exact multiple of SEGMENT so the over-reserve + trim arithmetic holds:
        //   base_addr + usable <= region_addr + over   (over = usable * 2)
        // With an un-rounded `needed` this can fail if `needed < SEGMENT` and
        // `align_up(region_addr, SEGMENT)` skips a large head region.
        let n_segments = needed.div_ceil(SEGMENT);
        let usable = n_segments * SEGMENT;

        // OPT-E: try the large-segment cache first.
        // Scan all slots for a compatible entry: usable_size >= usable (the
        // cached segment is big enough) AND usable_size <= usable *
        // LARGE_CACHE_SIZE_FACTOR (not so big we waste RSS). The size-ratio
        // bound prevents a 64 MiB cached segment from permanently absorbing
        // every 4 MiB request.
        #[cfg(feature = "alloc-decommit")]
        {
            let mut hit_idx: Option<usize> = None;
            for i in 0..LARGE_CACHE_SLOTS {
                if let Some(ref slot) = self.large_cache[i] {
                    if slot.usable_size >= usable
                        && slot.usable_size <= usable.saturating_mul(LARGE_CACHE_SIZE_FACTOR)
                    {
                        hit_idx = Some(i);
                        break;
                    }
                }
            }
            if let Some(idx) = hit_idx {
                let slot = self.large_cache[idx].take().unwrap();
                // Diagnostic (task D1): count this as a cache hit.
                // Э5 (task #145): load+store instead of `fetch_add` — no
                // `lock xadd`. SOUND for the same single-writer reason as
                // `HeapCore::tcache_hits`: the counter is per-heap and
                // `alloc_large` (its only incrementer) runs solely on the
                // owning thread (the slot's claim-CAS winner). No other thread
                // writes it, so splitting the atomic RMW into Relaxed load +
                // Relaxed store cannot drop a count. The cross-thread
                // `large_cache_hits_total` reader still does a Relaxed atomic
                // load — identical visibility to the old `fetch_add(Relaxed)`.
                //
                // W3: increment the SLOT's counter when this heap is bound
                // (`large_cache_hits_sink`), else the owned fallback (standalone
                // `AllocCore`). Same 2 mem-ops either way. Safe references
                // throughout (forbid-unsafe).
                //
                // W3 Part B: gated behind `alloc-stats` (default OFF, NOT in
                // `production`) — when off it compiles OUT of the large-cache
                // hit path and `stats().large_cache_hits` reads 0. See the
                // `alloc-stats` feature doc in Cargo.toml.
                #[cfg(feature = "alloc-stats")]
                {
                    let ctr = self.large_cache_hits_sink.unwrap_or(&self.large_cache_hits);
                    ctr.store(
                        ctr.load(core::sync::atomic::Ordering::Relaxed)
                            .wrapping_add(1),
                        core::sync::atomic::Ordering::Relaxed,
                    );
                }
                // Update the byte-budget counter: this slot is leaving the cache.
                self.large_cache_used_bytes =
                    self.large_cache_used_bytes.saturating_sub(slot.usable_size);
                // Re-register the base in the segment table. Under
                // alloc-decommit, recycle() left a NULL slot that register()
                // will reuse — so this should not fail. If it does (table is
                // genuinely full) we cannot reuse this slot; release it and
                // fall through to the slow OS path.
                let id = match self.table.register(slot.base) {
                    Some(id) => id,
                    None => {
                        // Table still full: release the cached reservation and
                        // fall through to the slow path.
                        os::release_segment(slot.reservation, slot.reservation_len);
                        // Fall through to OS path below.
                        return self.alloc_large_slow(size, align, usable, hdr_aligned);
                    }
                };
                // Pages are kept committed in the cache (no decommit on deposit,
                // no recommit needed on hit — they are already mapped and
                // accessible). Just write a fresh header and return.
                // Write a fresh header over the old one. The allocation lives
                // at hdr_aligned (same computation as the slow path).
                let bump = hdr_aligned + align_up(size, align);
                // `span_usable` is carried forward from the CACHED slot's own
                // `usable_size` — the true physical span of the segment being
                // reused — NOT recomputed from the new (possibly smaller)
                // `size`/`align`. Bug #134.
                let hdr = SegmentHeader::large(
                    id,
                    size,
                    align,
                    slot.usable_size,
                    bump,
                    slot.reservation,
                    slot.reservation_len,
                );
                Node::write_struct(slot.base as *mut SegmentHeader, hdr);
                // Phase C (numa-aware): re-stamp with the CURRENT thread's NUMA
                // node. The thread may have migrated since the segment was cached;
                // updating the tag reflects the current physical binding.
                #[cfg(feature = "numa-aware")]
                {
                    let my_node = numa::current_node();
                    SegmentMeta::new(slot.base).set_node_id(my_node);
                }
                return Node::deref(slot.base, hdr_aligned);
            }
        }

        self.alloc_large_slow(size, align, usable, hdr_aligned)
    }

    /// The slow (OS round-trip) path for `alloc_large` — called when the
    /// `large_cache` has no matching entry. Factored out so the cache-hit path
    /// can call `return self.alloc_large_slow(...)` cleanly when the table is
    /// full (avoiding a goto / code-duplication).
    fn alloc_large_slow(
        &mut self,
        size: usize,
        align: usize,
        usable: usize,
        hdr_aligned: usize,
    ) -> *mut u8 {
        // Phase C (numa-aware): steer the large segment to the calling thread's
        // NUMA node, same as for small segments.
        #[cfg(feature = "numa-aware")]
        let my_node = numa::current_node();

        #[cfg(feature = "numa-aware")]
        let (base, reservation, reservation_len) = {
            match numa::reserve_aligned_on_node(usable, my_node) {
                Some((b, r, rl)) => (b.as_ptr(), r, rl),
                None => return core::ptr::null_mut(),
            }
        };
        #[cfg(not(feature = "numa-aware"))]
        let (base, reservation, reservation_len) = {
            let segment = match Segment::reserve(usable) {
                Some(s) => s,
                None => return core::ptr::null_mut(),
            };
            let b = segment.as_ptr();
            let r = segment.reservation();
            let rl = segment.reservation_len();
            core::mem::forget(segment);
            (b, r, rl)
        };

        // no-panic: register returns None if the segment table is full (too many
        // live large allocations). We release the reservation and return null
        // (graceful OOM) rather than panicking.
        let id = match self.table.register(base) {
            Some(id) => id,
            None => {
                // Release the reservation we own.
                os::release_segment(reservation.as_ptr(), reservation_len);
                return core::ptr::null_mut();
            }
        };
        // Lay down the large header. The allocation lives at `hdr_aligned`.
        let bump = hdr_aligned + align_up(size, align);
        // Fresh reservation: `span_usable` = the just-computed physical
        // usable span (`usable`) — this is the ORIGINAL stamping that every
        // later cache-hit reuse of this segment will carry forward verbatim.
        let hdr = SegmentHeader::large(
            id,
            size,
            align,
            usable,
            bump,
            reservation.as_ptr(),
            reservation_len,
        );
        Node::write_struct(base as *mut SegmentHeader, hdr);
        // Phase C (numa-aware): stamp the NUMA node into the header after
        // writing it (the constructor sets node_id to NO_NODE_RAW).
        #[cfg(feature = "numa-aware")]
        SegmentMeta::new(base).set_node_id(my_node);

        Node::deref(base, hdr_aligned)
    }

    /// Reclaim a Large/huge segment that was freed by a REMOTE thread (0.3.0,
    /// task A1). `base` MUST be a currently-registered `Large`-kind segment
    /// base owned by this `AllocCore` — its header's `magic`/`kind` are still
    /// intact (a cross-thread free never zeroes them; only the OWNER's
    /// own-thread `dealloc` does that, on the path this function replaces for
    /// the remote case).
    ///
    /// Removes `base` from the segment table (freeing its slot for reuse —
    /// this is the fix for the permanent `SegmentTable` slot pin described in
    /// the A1 bug) and either:
    /// - (`alloc-decommit`) deposits the reservation into `large_cache`, same
    ///   admission policy as the own-thread large-dealloc path, so a
    ///   same-size `alloc_large` can reuse it without an OS round-trip; or
    /// - (no `alloc-decommit`) releases the OS reservation immediately via
    ///   `os::release_segment`, matching the own-thread path's behaviour
    ///   without the cache (own-thread `dealloc` there only zeroes the magic
    ///   and defers the release to `Drop`; here there is no `Drop` moment to
    ///   defer to mid-lifetime, and deferring would re-introduce the leak —
    ///   the slot must be freed NOW so `SegmentTable` capacity is not
    ///   permanently consumed by a segment nobody can address any more, since
    ///   we already removed it from the table above).
    ///
    /// Called by [`drain_large_deferred_free`](super::super::registry::heap_core::HeapCore)
    /// (via the `HeapCore` cross-thread reclaim path) on the owner's
    /// `alloc_large` slow-path, once per queued base.
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn reclaim_large_segment(&mut self, base: *mut u8) {
        let hdr = SegmentHeader::read_at(base);
        // Remove from the table FIRST (frees the slot for reuse regardless of
        // which branch below runs) — mirrors the own-thread cache-deposit
        // ordering in `dealloc`'s Large branch.
        self.table.unregister(base);

        #[cfg(feature = "alloc-decommit")]
        {
            self.maybe_decay_large_cache();
            // See the own-thread `dealloc` Large branch above (bug #134): the
            // physical usable span is read from the header's stable
            // `span_usable` field, not recomputed from `large_size`/
            // `large_align` (which can be stale-small after a cache-hit
            // reuse).
            let usable_size = hdr.span_usable;

            let mut admitted: Option<usize> = None;
            loop {
                let free_slot = self.large_cache.iter().position(|s| s.is_none());
                let budget_ok = self
                    .large_cache_budget_bytes
                    .is_none_or(|budget| self.large_cache_used_bytes + usable_size <= budget);
                if let Some(idx) = free_slot {
                    if budget_ok {
                        admitted = Some(idx);
                        break;
                    }
                }
                if !self.evict_one_oldest() {
                    break;
                }
            }

            if let Some(slot_idx) = admitted {
                let mut hdr_zero = hdr;
                hdr_zero.magic = 0;
                Node::write_struct(base as *mut SegmentHeader, hdr_zero);
                let seq = self.large_cache_seq;
                self.large_cache_seq = self.large_cache_seq.wrapping_add(1);
                self.large_cache[slot_idx] = Some(CachedLarge {
                    reservation: hdr.reservation,
                    reservation_len: hdr.reservation_len,
                    base,
                    usable_size,
                    seq,
                });
                self.large_cache_used_bytes += usable_size;
                return;
            }
        }

        // No `alloc-decommit` cache (or cache admission declined): release
        // the OS reservation immediately. The slot is already unregistered
        // above, so there is no dangling table entry pointing at unmapped
        // memory.
        os::release_segment(hdr.reservation, hdr.reservation_len);
    }

    // ── Phase 2 — lazy decay helpers ─────────────────────────────────────────

    /// Check whether enough wall-clock time has elapsed since the last decay
    /// tick; if so, run one decay step. Called at the top of both
    /// `alloc_large` and the large-dealloc branch so the "tax" on each large
    /// operation is a single `Instant::now()` comparison — nanosecond-range
    /// overhead, negligible against OS reservation costs.
    #[cfg(feature = "alloc-decommit")]
    fn maybe_decay_large_cache(&mut self) {
        // FAST-PATH EARLY EXIT — avoid `Instant::now()` (a `QueryPerformanceCounter`
        // syscall on Windows, ~50-100 ns) when there is provably no work to do.
        // The decay can only ever release bytes when `cached > headroom`. If the
        // cache is at or below the headroom, `run_decay_step` would compute
        // `excess = 0` and bail anyway, so we skip the wall-clock read entirely.
        //
        // This covers the dominant benchmark workload (alloc+free cycle with one
        // cached span at ~4-16 MiB, far below the 256 MiB default headroom) and
        // restores the ~45 ns cache-hit timing that the unconditional clock read
        // had regressed to ~150 ns. See task #95.
        //
        // Correctness: a true decay opportunity (cached > headroom) only arises
        // *after* a `dealloc` deposit grows `large_cache_used_bytes` past
        // `headroom_bytes`; we then hit this path on the next op and do the
        // proper time-based decision.
        if self.large_cache_used_bytes <= self.decay_config.headroom_bytes {
            return;
        }
        let now = std::time::Instant::now();
        let elapsed = match self.last_decay_tick {
            Some(t) => now.duration_since(t),
            None => {
                // First call ever: prime the timer but do not decay yet.
                // Without this guard the first alloc_large after a cold start
                // would decay with an arbitrarily large "elapsed" (since the
                // epoch), potentially flushing the cache unnecessarily.
                self.last_decay_tick = Some(now);
                return;
            }
        };
        if elapsed < self.decay_config.decay_interval {
            return;
        }
        self.last_decay_tick = Some(now);
        self.run_decay_step();
    }

    /// Compute the excess over `headroom_bytes` and release `decay_rate_bp /
    /// 10 000` of it back to the OS via FIFO eviction.
    ///
    /// Phase 2 simplification: `live_bytes = 0` (we do not track outstanding
    /// large allocations explicitly). The target is therefore simply
    /// `headroom_bytes`. A future phase can add live-count tracking to tighten
    /// the target when many large blocks are outstanding.
    #[cfg(feature = "alloc-decommit")]
    fn run_decay_step(&mut self) {
        let target = self.decay_config.headroom_bytes; // live = 0 in Phase 2
        let excess = self.large_cache_used_bytes.saturating_sub(target);
        if excess == 0 {
            return; // Cache is at or below target — nothing to release.
        }
        // release = excess * rate_bp / 10_000.  We use saturating_mul to
        // guard against an absurdly large excess (> usize::MAX / 10_000 on
        // 32-bit — pathological but safe).
        let release = excess.saturating_mul(self.decay_config.decay_rate_bp as usize) / 10_000;
        if release == 0 {
            return;
        }
        self.evict_at_least(release);
    }

    /// FIFO-evict cached spans until at least `min_bytes` of cache have been
    /// released to the OS, or the cache is empty. Each iteration evicts the
    /// occupied slot with the smallest `seq` (task D1: true insertion-order
    /// FIFO, not array-index order — see the `CachedLarge::seq` doc comment
    /// for why index order stopped being a valid proxy once
    /// `LARGE_CACHE_SLOTS > 2`). The OS reservation of each evicted span is
    /// released immediately.
    #[cfg(feature = "alloc-decommit")]
    fn evict_at_least(&mut self, min_bytes: usize) {
        let mut released = 0usize;
        while released < min_bytes {
            // Find the occupied slot with the smallest seq (true FIFO-oldest).
            let Some(victim_idx) = self.oldest_occupied_slot() else {
                break; // Cache is empty.
            };
            let victim = self.large_cache[victim_idx].take().unwrap();
            self.large_cache_used_bytes = self
                .large_cache_used_bytes
                .saturating_sub(victim.usable_size);
            // Release the OS reservation. The slot was unregistered from the
            // table on deposit (same as `try_evict_to_fit`), so we release
            // directly without touching the table.
            os::release_segment(victim.reservation, victim.reservation_len);
            released += victim.usable_size;
        }
    }

    // ── Phase 2 test seams ────────────────────────────────────────────────────

    /// TEST-ONLY (Phase 2): force a decay tick by rewinding `last_decay_tick`
    /// to be exactly `decay_interval` in the past, then calling
    /// `maybe_decay_large_cache`. This causes the interval check to pass
    /// unconditionally on the very next call, without sleeping. Safe to call
    /// multiple times — each call produces exactly one decay step.
    ///
    /// Concretely: for a test with `decay_interval = 10s` this makes it
    /// appear as if 10 s have elapsed since the last tick, so the subsequent
    /// `maybe_decay_large_cache` fires immediately.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_force_decay_tick(&mut self) {
        // Rewind last_decay_tick by the full interval so the elapsed check
        // passes.  `checked_sub` returns None if the duration is longer than
        // the time since the epoch (impossible in practice); in that edge case
        // we fall back to `now` which will prime the timer without decaying.
        let interval = self.decay_config.decay_interval;
        self.last_decay_tick = Some(
            std::time::Instant::now()
                .checked_sub(interval)
                .unwrap_or_else(std::time::Instant::now),
        );
        self.maybe_decay_large_cache();
    }

    /// TEST-ONLY (Phase 2): override the decay configuration at runtime.
    /// Lets tests specify exact parameters without relying on env vars
    /// (which are process-global and therefore flaky in parallel runs).
    ///
    /// - `rate_bp`: decay rate in basis points (100 = 1%, 1000 = 10%).
    /// - `interval_ms`: minimum ms between ticks (0 = fire on every call).
    /// - `headroom`: target cache size in bytes.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_set_decay_config(&mut self, rate_bp: u32, interval_ms: u64, headroom: usize) {
        self.decay_config = LargeCacheDecayConfig {
            decay_rate_bp: rate_bp,
            decay_interval: core::time::Duration::from_millis(interval_ms),
            headroom_bytes: headroom,
        };
        // Reset the tick timer so the new interval is observed from this
        // moment forward (avoids a stale timer confusing the first post-config
        // call).
        self.last_decay_tick = None;
    }

    /// TEST-ONLY (Phase 2): return the current decay configuration as
    /// `(decay_rate_bp, decay_interval_ms, headroom_bytes)`.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_decay_config(&self) -> (u32, u64, usize) {
        (
            self.decay_config.decay_rate_bp,
            self.decay_config.decay_interval.as_millis() as u64,
            self.decay_config.headroom_bytes,
        )
    }

    // ── end Phase 2 ──────────────────────────────────────────────────────────

    /// Find the occupied slot with the smallest `seq` — the true FIFO-oldest
    /// entry (task D1). Returns `None` if the cache is empty. `O(LARGE_CACHE_SLOTS)`;
    /// only called on the large-alloc/dealloc slow paths (never the small hot
    /// path), so the linear scan is not performance-sensitive even with 8
    /// slots.
    #[cfg(feature = "alloc-decommit")]
    fn oldest_occupied_slot(&self) -> Option<usize> {
        self.large_cache
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|c| (i, c.seq)))
            .min_by_key(|&(_, seq)| seq)
            .map(|(i, _)| i)
    }

    /// Evict the FIFO-oldest cached entry (smallest `seq`, task D1 — see
    /// [`oldest_occupied_slot`](Self::oldest_occupied_slot)) and release its
    /// OS reservation. Returns `true` if an entry was evicted, `false` if the
    /// cache was already empty.
    ///
    /// Used by the admission policy when either the byte-budget would
    /// overflow or all slots are occupied (the loop in the large-`dealloc`
    /// branch evicts-and-retries until both constraints hold or the cache is
    /// empty). The victim was unregistered from the segment table on
    /// deposit, so this function only releases the OS reservation and
    /// updates the byte-budget counter.
    #[cfg(feature = "alloc-decommit")]
    fn evict_one_oldest(&mut self) -> bool {
        let Some(victim_idx) = self.oldest_occupied_slot() else {
            return false;
        };
        let victim = self.large_cache[victim_idx].take().unwrap();
        self.large_cache_used_bytes = self
            .large_cache_used_bytes
            .saturating_sub(victim.usable_size);
        os::release_segment(victim.reservation, victim.reservation_len);
        true
    }

    /// TEST-ONLY (Phase 1 large-cache budget): return the current running sum
    /// of `usable_size` across all occupied large-cache slots. The test
    /// `large_cache_used_bytes_invariant` compares this against the manual sum
    /// to verify the invariant is maintained.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_large_cache_used(&self) -> usize {
        self.large_cache_used_bytes
    }

    /// TEST/DIAGNOSTIC-ONLY (task D1 → #133): count of `alloc_large` calls
    /// served from `large_cache` (cache hits) for THIS `AllocCore` since it
    /// was constructed. Relaxed load of `large_cache_hits` — diagnostic
    /// only. Task #133 moved this from a process-wide `static` to a
    /// per-heap instance field (see its doc comment); callers that need the
    /// process-wide total should use
    /// `registry::heap_registry::large_cache_hits_total`, which sums this
    /// method's result across every live registry slot.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub fn dbg_large_cache_hits(&self) -> u64 {
        // W3: read the SLOT's counter when bound (the SAME `AtomicU64` the
        // aggregator reads, so per-heap and process-wide agree), else the owned
        // fallback (standalone `AllocCore`). Safe references throughout.
        self.large_cache_hits_sink
            .unwrap_or(&self.large_cache_hits)
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// W3: plant the stable `&'static` handle to THIS heap's SLOT-resident
    /// large-cache hit counter. Called (via `HeapCore::bind_large_cache_hits`)
    /// by `HeapRegistry::claim` right after the slot binds, before any alloc on
    /// this heap. Redirects all subsequent increments and diagnostic reads to
    /// the slot's `AtomicU64`, closing the aliasing gap (see
    /// [`LargeCacheHitCounter`]). Idempotent — the slot counter is `'static`,
    /// so re-planting on a re-claim is a harmless no-op.
    ///
    /// Only reachable via the registry (`HeapRegistry::claim`, `alloc-global`);
    /// unused in an `alloc-decommit`-without-`alloc-global` build.
    #[cfg(feature = "alloc-decommit")]
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    pub(crate) fn bind_large_cache_hits(&mut self, counter: &'static LargeCacheHitCounter) {
        self.large_cache_hits_sink = Some(counter);
    }

    /// TEST-ONLY (Phase 1 large-cache budget): return the `usable_size` of
    /// each large-cache slot as an array of `Option<usize>` (None = empty slot,
    /// Some(sz) = occupied with that many bytes). Lets tests verify the
    /// invariant `sum(Some values) == dbg_large_cache_used()` without exposing
    /// the private `CachedLarge` type.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_large_cache_slot_sizes(&self) -> [Option<usize>; LARGE_CACHE_SLOTS] {
        let mut out = [None; LARGE_CACHE_SLOTS];
        for (i, slot) in self.large_cache.iter().enumerate() {
            out[i] = slot.as_ref().map(|c| c.usable_size);
        }
        out
    }

    /// TEST-ONLY (Phase 1 large-cache budget): override the byte-budget at
    /// runtime. Allows a test to set a different budget after calling
    /// `AllocCore::new_with_config`, without constructing a new instance.
    /// Pass `None` for unbounded.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_set_large_cache_budget(&mut self, budget: Option<usize>) {
        self.large_cache_budget_bytes = budget;
    }

    // ── Phase 3 test seams ────────────────────────────────────────────────────

    /// TEST-ONLY: return the `LargeCacheMode` set at construction time via
    /// [`LargeCacheConfig::mode`]. Lets tests verify the mode stored in the
    /// shard without relying on implementation internals.
    ///
    /// Returns `LargeCacheMode::Lazy` when `LargeCacheConfig::DEFAULT` was
    /// used (or no `.mode()` call was made on the config).
    ///
    /// [`LargeCacheConfig::mode`]: super::large_cache_config::LargeCacheConfig::mode
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_large_cache_mode(&self) -> LargeCacheMode {
        self.large_cache_mode
    }

    // ── end Phase 3 ──────────────────────────────────────────────────────────

    /// Reserve a fresh small segment, initialise its metadata, register it,
    /// and set it as the current small segment. Returns its base.
    fn reserve_small_segment(&mut self) -> Option<*mut u8> {
        // Phase C (numa-aware): determine the calling thread's NUMA node
        // BEFORE the reservation so we can pass it to `reserve_aligned_on_node`
        // (Windows requires the node at reserve-time via VirtualAllocExNuma;
        // Linux can bind post-mmap, but we unify the paths here).
        #[cfg(feature = "numa-aware")]
        let my_node = numa::current_node();

        // Reserve one SEGMENT's worth of virtual address space.
        // Under numa-aware we call the NUMA-steering path; otherwise the plain
        // OS path.  The returned triple always provides (base, reservation,
        // reservation_len) with the same semantics as Segment::reserve.
        #[cfg(feature = "numa-aware")]
        let (base, reservation, reservation_len) = {
            let (b, r, rl) = numa::reserve_aligned_on_node(SEGMENT, my_node)?;
            (b.as_ptr(), r, rl)
        };
        #[cfg(not(feature = "numa-aware"))]
        let (base, reservation, reservation_len) = {
            let segment = Segment::reserve(SEGMENT)?;
            let b = segment.as_ptr();
            let r = segment.reservation();
            let rl = segment.reservation_len();
            core::mem::forget(segment);
            (b, r, rl)
        };

        // no-panic: register returns None if the segment table is full. We
        // must release the reservation we just made before returning None.
        let id = match self.table.register(base) {
            Some(id) => id,
            None => {
                // Release the reservation we just made (we own it now).
                os::release_segment(reservation.as_ptr(), reservation_len);
                return None;
            }
        };
        // Lay down the small header + page map + bin table at the fixed
        // offsets. `bump` starts at the small-meta end (past the metadata).
        let meta_end = SegLayout::small_meta_end();
        let meta_pages = SegLayout::small_meta_pages();
        let mut meta = SegmentMeta::new(base);
        meta.write_header(SegmentHeader::small(
            id,
            meta_end,
            reservation.as_ptr(),
            reservation_len,
        ));
        // Phase C (numa-aware): stamp the NUMA node into the header NOW,
        // immediately after writing it. The header constructor set node_id to
        // NO_NODE_RAW; we overwrite it with the actual node. This must happen
        // BEFORE any carve/alloc so that find_segment_with_free sees the real
        // node on the very first scan that includes this segment.
        #[cfg(feature = "numa-aware")]
        meta.set_node_id(my_node);

        PageMap::init_in_place(base_add(base, SegLayout::page_map_off()), meta_pages);
        BinTable::init_in_place(base_add(base, SegLayout::bin_table_off()) as *mut u32);
        // Initialise the per-segment alloc-bitmap (Phase 13.4a double-free
        // guard) to all-zeros; bits flip to FREE as blocks are pushed.
        super::alloc_bitmap::AllocBitmap::init_in_place(base_add(
            base,
            SegLayout::alloc_bitmap_off(),
        ));
        // Initialise the per-segment remote-free ring (Variant-2 fix). Only
        // under `alloc-xthread`; the Layout always reserves the bytes.
        #[cfg(feature = "alloc-xthread")]
        {
            super::remote_free_ring::RemoteFreeRing::init_in_place(
                base,
                SegLayout::remote_ring_off(),
            );
        }
        // X7 Ф3 (task #191): zero the per-segment generation table under
        // `hardened`. Compiled ONLY under `hardened`; under any other feature
        // the table does not exist and this call is absent (byte-identical to
        // the pre-X7 build). Closes the carried-over Ф1 gap: without this
        // zeroing, a `gen_at`/`bump_gen` Relaxed load on a never-written cell
        // is UB. NOT re-zeroed on decommit-reset (plan §2.2: generation
        // numbering is continuous across decommit-reset by design).
        #[cfg(feature = "hardened")]
        {
            super::segment_header::init_gen_table_in_place(base);
        }
        // PERF-3 Ф1 (task #208): zero the per-segment run-encoded freelist
        // stack under `alloc-runfreelist`. Compiled ONLY under
        // `alloc-runfreelist`; under any other feature the RunStack does not
        // exist and this call is absent (byte-identical to the pre-PERF-3
        // build). Every descriptor starts at `count == 0` (empty/sentinel —
        // plan §2.1). Mirrors the bootstrap's identical call site (plan §4.1
        // — the SAME two sites X7-Ф3 wired `init_gen_table_in_place` into).
        #[cfg(feature = "alloc-runfreelist")]
        {
            super::run_stack::RunStack::init_in_place(base);
        }
        self.small_cur = base;
        Some(base)
    }
}

/// `Default` is a construction-time convenience only — it is NOT on the
/// alloc/dealloc/realloc hot path and no code in this crate currently calls
/// it (verified: no `AllocCore::default()` / `Default::default()` callers
/// under `src/` or `tests/` at the time of writing). It exists for callers
/// who want the ergonomics of `AllocCore::default()` / a `Default` trait
/// bound at construction time and are prepared to accept that construction
/// can fail.
///
/// This panics ONLY on true primordial OOM (the OS refuses the very first
/// segment reservation) — the same failure `AllocCore::new` already surfaces
/// as `None`. It never panics after construction: `alloc`/`dealloc`/
/// `realloc` are infallible with respect to panicking (OOM there returns a
/// null pointer / `None`, per the crate's no-panic-on-the-alloc-path
/// discipline). Callers who cannot tolerate a panic at construction should
/// call `AllocCore::new()` directly and handle `None`.
impl Default for AllocCore {
    fn default() -> Self {
        Self::new().expect("AllocCore::new: primordial segment reservation failed (OOM)")
    }
}

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
fn base_add(base: *mut u8, off: usize) -> *mut u8 {
    Node::offset(base, off)
}
