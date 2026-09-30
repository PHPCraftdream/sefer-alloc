//! [`HeapCore`] — the thin heap value that lives inside a registry slot.
//!
//! This is the type the Phase 12.3 raw-pointer TLS caches as
//! `*mut HeapCore`. Per §2.0 of `ALLOC_PLAN_PHASE12-13.md` the heap is now
//! thin (segment-centric free state lives in each segment's `BinTable`, not
//! in a heap-local array), so the per-slot heap needs to carry only:
//!
//! - its **id** (its slot index — or, for the fallback heap, the reserved
//!   `OWNER_ID_FALLBACK` sentinel; see `pack_owner`'s doc), used by the 12.3
//!   ownership stamping on segment headers (`owner = OWNER_STATE_LIVE | id`,
//!   with the stamp's generation sub-field hardcoded to 0 — the adoption
//!   substrate that once bumped it was removed, task #97 / R4-5), and
//! - the **segment substrate** ([`AllocCore`]) that owns this heap's segments
//!   and performs all per-segment `BinTable` arithmetic.
//!
//! ## Phase 12.3 — allocation routes through `HeapCore`
//!
//! 12.3 wires `HeapCore::alloc`/`dealloc`/`realloc`/`alloc_zeroed` as the
//! entry points the raw-pointer TLS binding hands to the alloc face. They
//! delegate to [`AllocCore`] on owner-local paths. Foreign frees publish once
//! into independently allocated terminal descriptors; they never borrow this
//! core or touch reservation headers/payload. The exclusive owner consumes
//! Small records on refill/miss paths, Large obligations on cold allocation
//! paths, and both kinds during trim or maintenance.
//!
//! ## M5-clean bootstrap invariant
//!
//! Construction reserves metadata through OS seams, never `std::alloc`.
//! Descriptor and Small sidecar capacity are established before issue, so
//! terminal publication never allocates metadata on the free path.
//!
//! ## `HeapCore` — the sole allocator face
//!
//! `HeapCore` is the slot-resident value the registry stores and the raw-
//! pointer TLS caches. The malloc face (`SeferAlloc`) routes through `HeapCore`
//! via the registry. (An earlier `Heap`/`with_heap` public face existed in
//! 0.3.0–0.3.x as a thin `AllocCore` wrapper without the magazine; it was
//! removed in 0.3.x — never used by the production fast path, ~9–12x slower
//! than mimalloc per `docs/HEAP_BENCH.md`. `HeapCore` is the magazine-backed
//! successor that supersedes it entirely.)

use crate::alloc_core::AllocCore;

// TEST-ONLY (0.3.0, task C1 → 0.4.x task #133): magazine (tcache) HIT
// counter. Originally a single process-wide `static AtomicU64`, bumped by
// EVERY thread's alloc fast path — a contended `lock xadd` on an otherwise
// fully per-thread hot path (the "churn hot path": pop from the magazine).
// Under MT this counter's cache line ping-pongs across cores on every
// magazine hit, adding cross-core traffic to a path that is architecturally
// per-thread (each `HeapCore` lives on one thread's registry slot — see the
// module doc). Perf regression #133.
//
// Fix: the counter is now a PER-HEAP field (`HeapCore::tcache_hits`, see
// below), incremented by its own owning thread only. Two threads' counters
// never share a cache line (each lives inside its own slot in the
// `'static` registry array), so the increment is a plain (uncontended)
// atomic RMW on ST and has NO cross-core traffic on MT — the contention is
// eliminated, not just made cheaper.
//
// It stays an `AtomicU64` (not a plain `u64`) because the process-global
// VIEW (`tcache_hits()` below, and `SeferAlloc::stats().tcache_hits`) reads
// EVERY live heap's counter from whatever thread calls `stats()` — a
// different thread than the owner in general. A plain `u64` written by one
// thread and read by another without synchronisation is a data race (UB,
// caught by TSan); `Relaxed` on both sides keeps this sound (no ordering
// requirement on a diagnostic counter — see the crate's existing
// `DBG_LARGE_XTHREAD_RECLAIMED` for the same relaxed-diagnostic pattern)
// while remaining `#![forbid(unsafe_code)]`-clean (no seam module needed —
// `AtomicU64` is safe-Rust top to bottom).
//
// TASK W3 (0.3.0) — the counter STORAGE moved out of `HeapCore` and into the
// owning `HeapSlot` (`HeapSlot::tcache_hits`), closing a formal aliasing gap.
// The old design put the `AtomicU64` INSIDE `HeapCore`; the process-wide
// aggregator (`tcache_hits_total`) then materialised a shared `&HeapCore`
// (`(*heap_ptr).tcache_hits()`) over a struct the OWNING thread concurrently
// holds a protected `&mut` into (the `alloc(&mut self, …)` protector) — a
// foreign-read of a protected `Unique`, UB under Stacked Borrows. Storing the
// counter in the `HeapSlot` (which is `Sync`, designed to be shared, and
// already read by the aggregator via `&HeapSlot` for `initialised`) lets the
// aggregator read it WITHOUT any `&HeapCore`. The owner reaches its slot's
// counter through the stable `*const AtomicU64` in the field below, planted by
// `HeapRegistry::claim` right after the slot is bound. See
// `HeapSlot::tcache_hits`.
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
#[doc(hidden)]
pub(crate) type TcacheHitCounter = ::core::sync::atomic::AtomicU64;

/// The thin, slot-resident heap value.
///
/// Lives inside a [`HeapSlot`](crate::registry::heap_slot::HeapSlot)'s `UnsafeCell` and
/// is handed out to a thread via
/// [`HeapRegistry::claim`](crate::registry::heap_registry::HeapRegistry::claim) as a
/// `*mut HeapCore`. Single-writer invariant (the owning thread is the only
/// mutator of its heap's bins) makes the `UnsafeCell` sound.
pub struct HeapCore {
    /// The owning slot's index in the registry. Used by
    /// [`recycle`](crate::registry::heap_registry::HeapRegistry::recycle) to find the
    /// slot back from a `*mut HeapCore` (12.3 stamps this into segment
    /// headers as the ownership key).
    /// `u32::MAX` is reserved as "not yet bound to a slot" (a freshly-init'd
    /// slot has `id = u32::MAX` until `claim` overwrites it). The
    /// process-global fallback heap instead uses `OWNER_ID_FALLBACK`
    /// (0x7FFF_FFFE, `alloc_core::segment_header`).
    ///
    /// Invariant: any id that will be stamped into a segment's `owner_state`
    /// must stay < 2^31 so `pack_owner`/`unpack_owner_id` round-trip it
    /// exactly (the OPT-C stamp-cache compare unpacks the stored word and
    /// compares against this id). Do not reintroduce a >= 2^31 sentinel here:
    /// the `u32::MAX` "unbound" value is safe only because it never reaches
    /// `pack_owner` — it is overwritten by `claim` before any alloc.
    pub(crate) id: u32,
    /// The segment substrate this heap owns. Owns the primordial + any
    /// additionally-reserved small/large segments. Phase 12.1: free-list
    /// state lives in each segment's `BinTable`, so this is the heap's entire
    /// small-allocation engine.
    pub(crate) core: AllocCore,

    /// Per-thread, per-class magazine cache (Phase P2 — fastbin).
    /// Gated on `alloc-global + fastbin`. Owner-private (single-writer):
    /// only the owning thread touches it. See `registry::heap_core::state::tcache`.
    ///
    /// ## D1 invariant (Phase 5/P5)
    ///
    /// A magazine-resident block COUNTS AS LIVE for the purposes of
    /// `live_count` / decommit. The invariant chain:
    ///   - refill_class pulls via alloc_small → inc_live per block.
    ///   - magazine push/pop do NOT touch live_count.
    ///   - magazine flush calls dealloc_small → dec_live → maybe_decommit.
    ///
    /// So `live_count` = blocks carved AND not on a BinTable free list
    /// = (blocks handed out to user) + (blocks in our magazine). Decommit
    /// fires only when a segment's blocks are ALL on the BinTable free
    /// list (none handed out, none in magazine).
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub(crate) tcache: crate::registry::heap_core::state::tcache::Tcache,

    /// TEST/DIAGNOSTIC-ONLY (task C1 → #133 → W3): stable handle to THIS
    /// heap's magazine HIT counter, which now lives in the owning
    /// [`HeapSlot::tcache_hits`](crate::registry::heap_slot::HeapSlot::tcache_hits) — a
    /// `Sync`, process-`'static` slot — rather than inline in this `HeapCore`.
    /// See the module-level comment above [`TcacheHitCounter`] for the full
    /// aliasing-gap rationale (task W3: an aggregator materialising a shared
    /// `&HeapCore` over a struct another thread holds a protected `&mut` into
    /// is UB under Stacked Borrows).
    ///
    /// Planted by [`HeapRegistry::claim`](crate::registry::heap_registry::HeapRegistry::claim)
    /// immediately after the slot is bound (`bind_counters`): it points at the
    /// slot's `AtomicU64`. Because the slot lives in the `'static` registry
    /// array, this pointer is sound for the slot's (process) lifetime and is
    /// never re-pointed. `null` only in the transient window before the first
    /// bind (never observed on any alloc path — `alloc` runs only after
    /// `claim` planted it); the increment/read helpers treat `null` as "no
    /// counter" defensively.
    ///
    /// The increment (owner-only, single writer) and the cross-thread
    /// aggregate read both go through the SAME slot `AtomicU64` (Relaxed),
    /// so `HeapCore::tcache_hits()` and `tcache_hits_total()` agree.
    ///
    /// Stored as a SAFE `Option<&'static _>` (not a raw pointer): this module
    /// is `#![deny(unsafe_code)]` with no local `allow`, so a raw-pointer
    /// deref would be a hard error. The `&'static` is minted by
    /// `HeapRegistry::claim` (which lives in the unsafe-permitted registry
    /// seam) from the slot's counter and planted here — a shared reference to
    /// a process-`'static` atomic, entirely sound to hold and read/write from
    /// the owning thread. `None` only in the transient pre-bind window (never
    /// observed on an alloc path).
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub(crate) tcache_hits: Option<&'static TcacheHitCounter>,

    /// OPT-C (task #66): lazy stamp cache.
    ///
    /// The base address of the last segment for which this heap successfully
    /// ran `stamp_segment_owner`. On the next alloc from the SAME segment the
    /// cache-hit fast path performs only a Relaxed load of `owner_state` (to
    /// confirm ownership is still ours) instead of a full Acquire-load +
    /// conditional Release-store. This eliminates the costly Release-store on
    /// the 99 % of allocations that stay in the hot segment.
    ///
    /// **Null** means "no segment cached yet" (initial state).
    ///
    /// **Cache invalidation safety:**
    /// - *Segment migration* — when the active segment changes (new small
    ///   segment carved, large-segment alloc) `base != last_stamped_segment`
    ///   → cache miss → slow path stamps and updates the cache.
    /// - *Segment recycle / decommit* — a recycled segment may reuse the same
    ///   base address. The Relaxed-load in the fast path re-reads `owner_state`
    ///   and compares against `self.id`; if the segment was recycled and its
    ///   `owner_state` reset to `OWNER_ID_NONE`, the comparison fails → slow
    ///   path re-stamps.
    /// - *Inter-heap segment transfer* — if a future phase introduces
    ///   transferring segments between heaps, the code doing the transfer MUST
    ///   null this cache field (`last_stamped_segment = null`) so the stale
    ///   entry is cleared before the next alloc. Currently no such path exists
    ///   (the shard model: each segment stays with its original heap forever).
    ///
    /// Only present under `alloc-global` (the feature that enables
    /// `stamp_segment_owner`).
    #[cfg(feature = "alloc-global")]
    pub(crate) last_stamped_segment: *mut u8,
}

// R34-18 (task #537, F-6 [low]) — compile-time stack-pressure budget pin.
//
// `HeapCore` is constructed BY VALUE on the stack of the frame that triggers a
// thread's FIRST allocation: `HeapRegistry::claim` does
// `HeapCore::new(idx) → heap_ptr.cast::<HeapCore>().write(hc)`
// (`heap_registry/claim.rs`, both `claim` and `claim_with_config`), and the
// process-global fallback constructs it the same way inside a
// `MaybeUninit<HeapCore>` (`global/fallback.rs`). Rust does NOT guarantee
// return-value/move elision: on a debug build, or any toolchain/backend that
// materialises the temporary, `HeapCore::new(..)` can place one multi-KiB copy
// on that first-allocation frame — frequently very early in a thread's life,
// or inside another thread-local's initialiser. Threads with small stacks
// (embedded-class 16–64 KiB, or a constrained thread pool) are a realistic
// deployment, so a single such temporary must not approach the stack limit.
//
// **Budget = 9216 (9 KiB), unconditional across EVERY feature composition.**
//
// Correction (task #572/H2's own follow-up wave — an independent readonly
// review, `docs/reviews/2026-08-05-wave3-h1h8-remediation-readonly-review.md`
// finding F1, caught this): the ORIGINAL R34-18/#537 pin used a fixed 8192 B
// budget plus a `#[cfg(not(any(experimental, pinning, bench-internals,
// batch-api)))]` exclusion (added by #571/H1) to dodge `--all-features`'s
// 8840 B size. That exclusion's premise — "8192 covers every SHIPPING
// composition" — was false: `medium-classes` is a genuine shipping opt-in
// (four dedicated CI rows, not experimental/test-only) and reaches 8408 B
// under plain `production medium-classes`, still over the 8192 budget and
// NOT covered by the exclusion list, so two live `ci.yml` commands were
// silently red. Enumerating "which features are experimental" by name is
// inherently fragile — the fix missed exactly one shipping feature and broke
// CI. Fixed structurally instead: raised the budget to cover the measured
// maximum of every composition this crate can currently build
// (`--all-features`, the union of every feature this crate has, 8840 B,
// confirmed the largest of every composition tried: plain `production`=7576
// B, `production medium-classes`=8408 B, `production medium-classes
// numa-aware`=8416 B, `production medium-classes-wide numa-aware`=8832 B,
// `--all-features`=8840 B) plus real headroom, and REMOVED the exclusion
// `#[cfg]` entirely — the assert below is now unconditional. Precision note
// (F6 of `docs/reviews/2026-08-05-hs-new-waves-release-readonly-review.md`):
// "`--all-features` is the largest possible `HeapCore`" holds for the
// CURRENT `HeapCore` field layout, whose per-feature fields are all added
// via purely ADDITIVE `#[cfg(feature = "...")]` (never `cfg(not(...))`,
// never a mutually-exclusive representation) — see the field list above —
// measured on the current target/toolchain; it is not a standing theorem
// about every possible future field layout, target, or toolchain. The
// UNCONDITIONAL assert below is what actually enforces the budget across
// ANY future change: if a future field addition (additive or not) or a
// different target/ABI ever pushes any composition past 9216 B, the build
// fails honestly rather than silently exceeding an un-checked assumption.
// The runtime test in `tests/r34_18_heap_core_stack_pressure_pin.rs` mirrors
// this: its own upper-bound assertion is likewise unconditional now.
//
// 9216 → 8840 (the measured `--all-features` maximum) leaves 376 B headroom
// (~4%): enough for minor field growth without immediately retripping the
// build, tight enough that material bloat (a new array/sub-struct, or
// `Tcache` growing another magazine class) still fails the build and forces
// a deliberate budget-bump decision recording the new stack-pressure
// implication. 9 KiB is still a bit over half of a 16 KiB embedded stack —
// the "one HeapCore temporary is tolerable, two back-to-back are not"
// boundary this pin exists to guard, now honestly measured against the
// crate's real maximum size rather than an enumerated subset of it.
//
// This is the ONLY unbounded-growth stack-pressure surface in the tree (F-6
// audit): there is no unbounded/data-dependent recursion, no recursive drop
// glue, and no other stack buffer larger than `emptied_bases: [*mut u8; 64]`
// (512 B, cold path) — so this single pin guards the entire category. (R1-04:
// this claim was briefly false — `AllocCore::drop`'s segment-release walk
// used a `[(*mut u8, usize); MAX_SEGMENTS]` stack array, 65 536 B, which
// overflowed a 64 KiB-stack thread on a plain `AllocCore::new()` + `drop`.
// Fixed by freeing each non-primordial segment inline during the walk instead
// of buffering reservations first — see `Drop for AllocCore` in
// `src/alloc_core/alloc_core/lifecycle.rs`.)
const _: () = assert!(size_of::<HeapCore>() <= 9216);

impl HeapCore {
    /// Construct a fresh heap value bound to slot `id`. Bootstraps the
    /// segment substrate via [`AllocCore::new`] (which goes through the OS
    /// aperture — `mmap`/`VirtualAlloc` — and never `std::alloc`, upholding
    /// M5). Returns `None` only on primordial OOM (the OS refused the
    /// reservation).
    ///
    /// Construction uses the OS aperture only. Terminal route and sidecar
    /// storage are established before any allocation is issued.
    ///
    /// Called lazily by [`HeapRegistry::claim`](crate::registry::heap_registry::HeapRegistry::claim)
    /// when it transitions a slot `FREE → LIVE` and needs to materialise the
    /// heap value in the slot's `UnsafeCell`.
    #[must_use]
    pub(crate) fn new(id: u32) -> Option<Self> {
        let core = AllocCore::new_with_owner(id)?;
        Some(Self {
            id,
            core,
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            tcache: crate::registry::heap_core::state::tcache::Tcache::new(),
            // W3: the counter now lives in the owning HeapSlot; this handle
            // is planted by `HeapRegistry::claim` (via `bind_tcache_hits`)
            // right after the slot binds. `None` until then (never observed on
            // any alloc path — alloc runs only after claim planted it).
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            tcache_hits: None,
            #[cfg(feature = "alloc-global")]
            last_stamped_segment: ::core::ptr::null_mut(),
        })
    }

    /// Construct a fresh heap value bound to slot `id`, using `config` to
    /// tune the large-segment free-cache. Only present under `alloc-decommit`.
    ///
    /// Identical to [`new`](Self::new) except it calls
    /// [`AllocCore::new_with_config`] so per-thread large-cache behaviour
    /// matches the compile-time `SeferAlloc::with_config(...)` choice.
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub(crate) fn new_with_config(
        id: u32,
        config: crate::alloc_core::LargeCacheConfig,
    ) -> Option<Self> {
        let core = AllocCore::new_with_config_for_owner(config, id)?;
        Some(Self {
            id,
            core,
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            tcache: crate::registry::heap_core::state::tcache::Tcache::new(),
            // W3: the counter now lives in the owning HeapSlot; this handle
            // is planted by `HeapRegistry::claim` (via `bind_tcache_hits`)
            // right after the slot binds. `None` until then (never observed on
            // any alloc path — alloc runs only after claim planted it).
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            tcache_hits: None,
            #[cfg(feature = "alloc-global")]
            last_stamped_segment: ::core::ptr::null_mut(),
        })
    }

    /// The slot index this heap is bound to. Read by `recycle` to locate the
    /// owning slot from a `*mut HeapCore`.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Iterate over the segment bases this heap owns (read-only). Delegates to
    /// the substrate's segment-table iterator.
    ///
    /// `#[doc(hidden)] pub` so integration tests can obtain a real segment
    /// base (the test-only pub surface of the registry, documented in
    /// `mod.rs`).
    ///
    /// R2-01 (task #2003): `+ '_` threads the same lifetime bind through
    /// this re-export — see `AllocCore::segment_bases`'s doc comment for the
    /// full UAF rationale this closes.
    #[doc(hidden)]
    pub fn segment_bases(&self) -> impl Iterator<Item = *mut u8> + '_ {
        self.core.segment_bases()
    }

    /// Compare this heap's live (resolved) cache/pool policy against a
    /// requested config. Forwards to `AllocCore::live_config_matches`.
    /// Used by `HeapRegistry::claim_with_config` (N2) to detect a config
    /// mismatch on a recycled, already-materialised slot.
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn live_config_matches(
        &self,
        requested: &crate::alloc_core::LargeCacheConfig,
    ) -> bool {
        self.core.live_config_matches(requested)
    }

    /// R11-5: invalidate the cached `current_node()` value on this heap's
    /// `AllocCore`. Called by `HeapRegistry::claim` /
    /// `claim_with_config` immediately before handing the freshly-claimed
    /// `*mut HeapCore` to the caller, so a recycled slot's next
    /// `current_node_cached()` call re-queries the OS instead of returning
    /// the previous owner's stale value. See `docs/PHASE_NUMA_DESIGN.md`
    /// §4.1 for the design note and the slot-recycle soundness argument.
    ///
    /// Gate: only present under `numa-aware` (no cache exists without it).
    #[cfg(feature = "numa-aware")]
    pub(crate) fn invalidate_numa_node_cache(&mut self) {
        self.core.invalidate_numa_node_cache();
    }

    /// R11-5 test-only: read this heap's cached NUMA-node value without
    /// populating it. Delegates to
    /// [`AllocCore::dbg_cached_numa_node`](crate::alloc_core::AllocCore::dbg_cached_numa_node).
    /// The `tests/numa_cache_invalidation.rs` slot-recycle regression uses
    /// this to assert the newly-claimed slot's cached value reflects the
    /// NEW mock node, not the stale one from before recycling — the exact
    /// bug `invalidate_numa_node_cache` exists to prevent.
    ///
    /// `#[doc(hidden)] pub` per the established test-only-export pattern
    /// (CLAUDE.md "File and module structure" sanctioned exception 1).
    ///
    /// H2 (task #572): additionally gated `internals` — the delegated
    /// [`AllocCore::dbg_cached_numa_node`] moved behind `internals`
    /// (`alloc_core.rs`'s module doc).
    #[cfg(all(feature = "numa-aware", feature = "internals"))]
    #[doc(hidden)]
    pub fn dbg_cached_numa_node(&self) -> Option<u32> {
        self.core.dbg_cached_numa_node()
    }

    /// R11-5 test-only: deterministically populate the cache by directly
    /// invoking [`AllocCore::current_node_cached`]. The
    /// `tests/numa_cache_invalidation.rs` slot-recycle regression uses
    /// this so its assertions do NOT depend on incidental free-list state
    /// left on a recycled slot by an earlier test (a leftover free block
    /// for the requested class makes `pop_free` succeed, skipping
    /// `find_segment_with_free` — and thus skipping the cache populate —
    /// making the test flaky across test-orderings). The hook lets the
    /// test exercise the cached accessor's populate path directly while
    /// ALSO doing real allocs (see the test) for end-to-end coverage.
    ///
    /// `#[doc(hidden)] pub` per the established test-only-export pattern.
    #[cfg(feature = "numa-aware")]
    #[doc(hidden)]
    pub fn dbg_populate_numa_cache_for_test(&mut self) {
        let _ = self.core.current_node_cached();
    }
}
