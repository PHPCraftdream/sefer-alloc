//! [`HeapCore`] — the thin heap value that lives inside a registry slot.
//!
//! This is the type the Phase 12.3 raw-pointer TLS caches as
//! `*mut HeapCore`. Per §2.0 of `ALLOC_PLAN_PHASE12-13.md` the heap is now
//! thin (segment-centric free state lives in each segment's `BinTable`, not
//! in a heap-local array), so the per-slot heap needs to carry only:
//!
//! - its **id** (its slot index + the slot's `generation`), used by the 12.3
//!   ownership stamping on segment headers (`owner = heap id + generation`)
//!   and the M8/M9 coherence checks, and
//! - the **segment substrate** ([`AllocCore`]) that owns this heap's segments
//!   and performs all per-segment `BinTable` arithmetic.
//!
//! ## Phase 12.3 — allocation routes through `HeapCore`
//!
//! 12.3 wires `HeapCore::alloc`/`dealloc`/`realloc`/`alloc_zeroed` as the
//! entry points the raw-pointer TLS binding hands to the alloc face. They
//! delegate to the [`AllocCore`] (own-thread path). Under `alloc-xthread`,
//! [`HeapCore`] also carries the cross-thread `ThreadFreeStack` handle and
//! stamps `owner_thread_free` on segment headers so remote threads can route
//! cross-thread frees here (the §2.2 "owner stamping — 12.3" rule).
//!
//! ## M5-clean bootstrap invariant
//!
//! `HeapCore::new` bootstraps via [`AllocCore::new`] (OS aperture only —
//! `mmap`/`VirtualAlloc`, **never** `std::alloc`). The cross-thread
//! `ThreadFreeStack` is `Box`-allocated (goes through `std::alloc`), so it
//! is **NOT** constructed in `HeapCore::new` — that would violate the
//! M5-clean bootstrap of `HeapRegistry::claim` (which lazily materialises
//! a `HeapCore` inside the slot and runs inside the registry's
//! `ensure`/bootstrap path). Instead the TFS handle is installed lazily by
//! `HeapCore::install_thread_free`, called from the TLS bind-slow path
//! (outside the registry bootstrap). Until then `thread_free` is `None` and
//! the heap serves only own-thread allocations (cross-thread frees to its
//! segments are a safe no-op, matching the existing unstamped-segment
//! behaviour in `Heap::dealloc_small`).
//!
//! ## `Heap` vs `HeapCore`
//!
//! The existing [`Heap`](crate::heap::Heap) is the Phase 9/10/12.1
//! thread-local heap (owns `AllocCore` + the cross-thread stack, served via
//! `RefCell<Option<Heap>>` TLS, with the abandon-on-drop leak discipline).
//! `HeapCore` is the *slot-resident* value the registry stores and the 12.3
//! raw-pointer TLS caches. They coexist in 12.3: the explicit-`Heap`-API
//! (`with_heap`) keeps serving the `alloc` feature and its tests; the malloc
//! face (`SeferAlloc`) is rewired to route through `HeapCore` via the
//! registry. The eventual collapse of `Heap` into `HeapCore` is later work.

use core::alloc::Layout;
#[cfg(feature = "alloc-xthread")]
use core::sync::atomic::AtomicPtr;
use core::sync::atomic::Ordering;

#[cfg(feature = "alloc-global")]
use crate::alloc_core::os;
#[cfg(feature = "alloc-global")]
use crate::alloc_core::segment_header::pack_owner;
#[cfg(any(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::segment_header::SegmentMeta;
#[cfg(feature = "alloc-xthread")]
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind, SEGMENT_MAGIC};
use crate::alloc_core::{node::Node, AllocCore};

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
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
#[doc(hidden)]
pub(crate) type TcacheHitCounter = core::sync::atomic::AtomicU64;

/// The thin, slot-resident heap value.
///
/// Lives inside a [`HeapSlot`](super::heap_slot::HeapSlot)'s `UnsafeCell` and
/// is handed out to a thread via
/// [`HeapRegistry::claim`](super::heap_registry::HeapRegistry::claim) as a
/// `*mut HeapCore`. Single-writer invariant (the owning thread is the only
/// mutator of its heap's bins) makes the `UnsafeCell` sound.
pub struct HeapCore {
    /// The owning slot's index in the registry. Used by `recycle`/`abandon`
    /// to find the slot back from a `*mut HeapCore` (12.3 stamps this into
    /// segment headers as the ownership key).
    /// `u32::MAX` is reserved as "not yet bound to a slot" (a freshly-init'd
    /// slot has `id = u32::MAX` until `claim` overwrites it).
    pub(crate) id: u32,
    /// The segment substrate this heap owns. Owns the primordial + any
    /// additionally-reserved small/large segments. Phase 12.1: free-list
    /// state lives in each segment's `BinTable`, so this is the heap's entire
    /// small-allocation engine.
    pub(crate) core: AllocCore,
    /// The cross-thread free-stack head. Phase 12.5: this is an INLINE
    /// `AtomicPtr<u8>` (not a `Box<AtomicPtr<u8>>`) so that
    /// [`install_thread_free`](Self::install_thread_free) performs NO
    /// `std::alloc` — it is a no-op (the field is initialised to null in
    /// [`new`](Self::new), which is M5-clean). This is load-bearing for the
    /// `alloc-global + alloc-xthread` combo: under `#[global_allocator]`, a
    /// `Box::new` on the TLS bind path would recurse into `SeferAlloc::alloc`
    /// → `current_for_alloc` → `bind_slow` → `install_thread_free` → `Box::new`
    /// → infinite recursion → stack overflow. The inline field breaks that
    /// cycle: the head's address is `&self.thread_free`, stable for the
    /// slot's lifetime (the `HeapCore` lives in the `'static` registry slot
    /// array), so segment headers can store a raw `*const AtomicPtr<u8>` to
    /// it.
    ///
    /// Only present under `alloc-xthread` (the cross-thread feature).
    /// 0.3.0 (task A1): ALSO doubles as the head of this heap's per-heap
    /// deferred-free Treiber stack for Large/huge segments freed by a
    /// REMOTE thread.
    ///
    /// **Background:** under the Phase 12.5 shard model the intrusive TFS
    /// this field was originally built for is gone — cross-thread frees of
    /// SMALL blocks route through each segment's `RemoteFreeRing`, not
    /// through this head. `thread_free` therefore serves ONLY as an identity
    /// stamp (`owner_thread_free_at(base) == &heap.thread_free` is how a
    /// remote freer recognises "this segment belongs to that heap" — see
    /// `dealloc_routing`); its `AtomicPtr<u8>` VALUE was otherwise always
    /// null and unused.
    ///
    /// **The A1 leak this reuse fixes:** `dealloc_routing`'s Large branch
    /// used to be a bare `return` (a permanent no-op) whenever a Large
    /// segment was freed by a thread other than its owner — the segment
    /// (whole 4+ MiB, or more for an oversized allocation) was never
    /// released and its `SegmentTable` slot was never recycled: a silent,
    /// permanent leak under any workload that allocates-here/frees-there for
    /// large blocks (e.g. async runtimes migrating tasks across worker
    /// threads).
    ///
    /// **The fix:** since `thread_free` is idle, we press it into service as
    /// a SECOND role — a Treiber-stack head over segment BASES (not small
    /// blocks). A remote free of a Large segment now pushes the segment's
    /// `base` onto this stack (via
    /// [`push_large_deferred_free`](Self::push_large_deferred_free)) instead
    /// of no-op'ing. The OWNER thread drains the stack lazily, on its own
    /// `alloc_large` slow path (via
    /// [`drain_large_deferred_free`](Self::drain_large_deferred_free)),
    /// before reserving a fresh segment — so a cross-thread-freed large
    /// segment is recycled (via `AllocCore::reclaim_large_segment`, which
    /// either deposits it in the `alloc-decommit` large-cache or releases it
    /// to the OS) the next time this heap does a large allocation.
    ///
    /// **No conflation with the identity-stamp role:** the identity check
    /// (`owner_tf == our_head`, comparing the `*const AtomicPtr<u8>`
    /// ADDRESS) never dereferences the pointed-to `AtomicPtr<u8>` VALUE, so
    /// stuffing a segment base into that value cell does not corrupt the
    /// stamp comparison — the two roles use disjoint parts of the same word
    /// (the address is the identity; the pointee is the stack head).
    ///
    /// **Structurally identical to** the Phase 12.4
    /// `Registry::abandoned_segs` intrusive Treiber stack
    /// (`push_abandoned_segment_into`/`pop_abandoned_segment` in
    /// `heap_registry.rs`) — same push/CAS/pop shape — but PER-HEAP (this
    /// field, not the global registry head) and reusing each segment's
    /// `next_abandoned` header field as the intrusive link.
    ///
    /// ⚠️ **This field-sharing is safe ONLY while `abandon_segments` stays
    /// unreachable from production** (Phase 12.5's "release the slot only"
    /// discipline — see its doc comment in `heap_registry.rs`). It is NOT a
    /// structural guarantee: both this local stack and the global
    /// abandoned-segs stack write the SAME `next_abandoned` field, and if the
    /// global walk is ever reactivated (e.g. a future decommit-when-empty
    /// policy — see the `⚠️ REACTIVATION HAZARD` note on
    /// `HeapRegistry::abandon_segments`) without excluding Large segments, a
    /// segment mid-flight on THIS stack can have its link silently
    /// overwritten by that walk — corrupting this stack's chain (leak of
    /// everything behind it, and a possible wild-pointer read on a later
    /// local pop). Read that note in full before wiring `abandon_segments`
    /// back onto any hot path.
    ///
    /// No ABA tag is needed here (unlike the global stack): only the OWNER
    /// thread ever pops from this stack (single consumer), so the classic
    /// multi-popper ABA race does not apply; multiple REMOTE threads may push
    /// concurrently (multi-producer), which a plain CAS-loop push handles
    /// without a tag.
    ///
    /// `null` = empty stack (steady state — most workloads never hit this
    /// path).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) thread_free: AtomicPtr<u8>,

    /// Per-thread, per-class magazine cache (Phase P2 — fastbin).
    /// Gated on `alloc-global + fastbin`. Owner-private (single-writer):
    /// only the owning thread touches it. See `registry::tcache`.
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
    pub(crate) tcache: super::tcache::Tcache,

    /// TEST/DIAGNOSTIC-ONLY (task C1 → #133): per-heap magazine HIT counter.
    /// See the module-level comment above [`TcacheHitCounter`] for why this
    /// moved from a single process-wide `static` to a per-heap field (perf
    /// regression #133 — the global counter's `lock xadd` contended and
    /// cache-line-ping-ponged across every thread's magazine-hit fast path).
    /// Bumped (Relaxed) by [`HeapCore::alloc`]'s magazine-hit branch — always
    /// by THIS heap's owning thread, so the increment itself is never
    /// contended by another thread. Read cross-thread only by the
    /// aggregating [`tcache_hits_total`] (diagnostics / `stats()`), which is
    /// why this stays an `AtomicU64` (Relaxed load) rather than a plain
    /// `u64` — a plain field read from a non-owning thread without
    /// synchronisation would be a data race (UB) despite `#![forbid(unsafe_code)]`
    /// never being violated by the *type itself*.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub(crate) tcache_hits: TcacheHitCounter,

    /// OPT-C (task #66): lazy stamp cache.
    ///
    /// The base address of the last segment for which this heap successfully
    /// ran `stamp_segment_owner`. On the next alloc from the SAME segment the
    /// cache-hit fast path performs only a Relaxed load of `owner_state` (to
    /// confirm ownership is still ours) instead of a full Acquire-load +
    /// conditional Release-store. This eliminates the costly Release-store on
    /// the 99 % of allocations that stay in the hot segment.
    ///
    /// **Null** means "no segment cached yet" (initial state, or after
    /// [`reset_stamp_cache`](Self::reset_stamp_cache) was called).
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
    /// - *Abandoned-heap adoption* — if a future phase introduces inter-heap
    ///   segment transfer, the code that adopts foreign segments MUST call
    ///   `reset_stamp_cache()` so the stale cache entry is cleared before the
    ///   next alloc. Currently no such path exists (the shard model: each
    ///   segment stays with its original heap forever). See the TODO in
    ///   `reset_stamp_cache`.
    ///
    /// Only present under `alloc-global` (the feature that enables
    /// `stamp_segment_owner`).
    #[cfg(feature = "alloc-global")]
    last_stamped_segment: *mut u8,
}

impl HeapCore {
    /// Construct a fresh heap value bound to slot `id`. Bootstraps the
    /// segment substrate via [`AllocCore::new`] (which goes through the OS
    /// aperture — `mmap`/`VirtualAlloc` — and never `std::alloc`, upholding
    /// M5). Returns `None` only on primordial OOM (the OS refused the
    /// reservation).
    ///
    /// **M5-clean:** this performs NO `std::alloc`. The cross-thread TFS
    /// handle is `None` here; it is installed separately by
    /// [`install_thread_free`](Self::install_thread_free) on the TLS bind
    /// path (which is allowed to touch `std::alloc` — it is NOT inside the
    /// registry bootstrap).
    ///
    /// Called lazily by [`HeapRegistry::claim`](super::heap_registry::HeapRegistry::claim)
    /// when it transitions a slot `FREE → LIVE` and needs to materialise the
    /// heap value in the slot's `UnsafeCell`.
    #[must_use]
    pub(crate) fn new(id: u32) -> Option<Self> {
        let core = AllocCore::new()?;
        Some(Self {
            id,
            core,
            #[cfg(feature = "alloc-xthread")]
            thread_free: AtomicPtr::new(core::ptr::null_mut()),
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            tcache: super::tcache::Tcache::new(),
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            tcache_hits: TcacheHitCounter::new(0),
            #[cfg(feature = "alloc-global")]
            last_stamped_segment: core::ptr::null_mut(),
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
        let core = AllocCore::new_with_config(config)?;
        Some(Self {
            id,
            core,
            #[cfg(feature = "alloc-xthread")]
            thread_free: AtomicPtr::new(core::ptr::null_mut()),
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            tcache: super::tcache::Tcache::new(),
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            tcache_hits: TcacheHitCounter::new(0),
            #[cfg(feature = "alloc-global")]
            last_stamped_segment: core::ptr::null_mut(),
        })
    }

    /// The slot index this heap is bound to. Read by `recycle`/`abandon` to
    /// locate the owning slot from a `*mut HeapCore`.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Iterate over the segment bases this heap owns (read-only). Used by the
    /// Phase 12.4 abandonment walk: `abandon_segments` stamps each segment's
    /// `owner_state = ABANDONED` and pushes its base onto the global
    /// abandoned-segments stack. Delegates to the substrate's segment-table
    /// iterator. Phase 12.4 addition.
    ///
    /// `#[doc(hidden)] pub` so integration tests can obtain a real segment
    /// base for the abandoned-stack round-trip test (the test-only pub
    /// surface of the registry, documented in `mod.rs`).
    #[doc(hidden)]
    pub fn segment_bases(&self) -> impl Iterator<Item = *mut u8> {
        self.core.segment_bases()
    }

    /// TEST/DIAGNOSTIC-ONLY (task #133): this heap's own magazine-hit count.
    /// Relaxed load of [`tcache_hits`](Self::tcache_hits) — sound for a
    /// cross-thread diagnostic read (see the field's doc comment). Used by
    /// [`super::heap_registry::tcache_hits_total`] to aggregate across every
    /// LIVE slot into the process-wide view `stats()` exposes.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[doc(hidden)]
    #[must_use]
    pub fn tcache_hits(&self) -> u64 {
        self.tcache_hits.load(Ordering::Relaxed)
    }

    /// Phase 12.4 adoption substrate: register an adopted segment base into
    /// this heap's substrate table. Thin wrapper over
    /// [`AllocCore::register_segment`]. Called by `try_adopt` after winning
    /// the ABANDONED→LIVE CAS. Retained for the loom-proven abandon/adopt
    /// substrate (a future decommit-when-empty policy); NOT on the hot path
    /// of the shard model.
    #[cfg(feature = "alloc-global")]
    #[allow(dead_code)]
    pub(crate) fn register_segment_internal(&mut self, base: *mut u8) -> Option<u32> {
        self.core.register_segment(base)
    }

    /// Phase 12.4 adoption substrate: make `base` the current small segment so
    /// new allocations carve from it. Thin wrapper over
    /// [`AllocCore::set_small_current`]. Retained for the substrate; NOT on
    /// the shard-model hot path.
    #[cfg(feature = "alloc-global")]
    #[allow(dead_code)]
    pub(crate) fn set_small_current_internal(&mut self, base: *mut u8) {
        self.core.set_small_current(base);
    }

    /// Lazily "install" the cross-thread free-stack handle on the TLS
    /// bind-slow path.
    ///
    /// Phase 12.5 redesign: this performs **NO allocation** — it is a no-op
    /// that simply hands out the address of the INLINE
    /// [`thread_free`](Self::thread_free) field (an `AtomicPtr<u8>` already
    /// initialised to null in [`new`](Self::new)). It does NOT allocate a
    /// `Box<AtomicPtr<u8>>` — an earlier design did, but that was removed
    /// because a `Box::new` here would recurse through `SeferAlloc::alloc` →
    /// `bind_slow` → `install_thread_free` → `Box::new` under a
    /// `#[global_allocator]` build and self-deadlock/overflow (see the field
    /// doc on `thread_free`). Because it allocates nothing, it is trivially
    /// M5-clean and idempotent (every call returns the same address; there is
    /// no per-call state to guard).
    ///
    /// Returns the stable `*const AtomicPtr<u8>` head pointer (the inline
    /// field's address, stable for the slot's lifetime) that segment headers
    /// store in `owner_thread_free` so cross-thread freers can route to this
    /// heap.
    ///
    /// Only compiled under `alloc-xthread` (the cross-thread feature).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn install_thread_free(&mut self) -> *const AtomicPtr<u8> {
        // The field is already initialised (null) in `new`; we only hand out
        // its address. The address is stable for the slot's lifetime (the
        // `HeapCore` lives in the `'static` registry slot array).
        &self.thread_free as *const AtomicPtr<u8>
    }

    /// The stable `*const AtomicPtr<u8>` head pointer of this heap's TFS, or
    /// null if [`install_thread_free`](Self::install_thread_free) was never
    /// called (no cross-thread stamping has happened → cross-thread frees to
    /// this heap's segments are a safe no-op). Used by the drain / routing
    /// paths on the owning thread.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn thread_free_head(&self) -> *const AtomicPtr<u8> {
        &self.thread_free as *const AtomicPtr<u8>
    }

    /// 0.3.0 (task A1); extracted for #132: push a Large/huge segment `base`
    /// onto the OWNING heap's deferred-free stack, given `head` — the
    /// owner's `thread_free_head()` (a `*const AtomicPtr<u8>`, obtained by a
    /// REMOTE freer from `owner_thread_free_at(segment_base)`). Called from
    /// [`dealloc_routing`](Self::dealloc_routing) in place of the old
    /// permanent-leak no-op.
    ///
    /// Thin delegation to the shared
    /// [`alloc_core::deferred_large::push_large_deferred_free`] primitive
    /// (byte-for-byte the same push/CAS/double-push-guard logic this method
    /// used to inline — see that function's doc comment for the full
    /// mechanism and the double-push-guard hardening rationale). The
    /// primitive takes `&AtomicPtr<u8>` directly, so the pointer-to-reference
    /// deref of `head` stays HERE (via the `alloc_core::node` seam, same
    /// discipline as `next_abandoned_atomic`/`owner_state_atomic`) rather
    /// than inside the shared (seam-free) module — `Heap`'s call site
    /// derives its own `&AtomicPtr<u8>` without any seam at all (it owns the
    /// field directly).
    #[cfg(feature = "alloc-xthread")]
    fn push_large_deferred_free(head: *const AtomicPtr<u8>, base: *mut u8) {
        // `heap_core.rs` is NOT an allowed `unsafe` seam (see `src/lib.rs`'s
        // seam whitelist), so the pointer-to-reference deref is delegated to
        // `Node::atomic_ptr_ref` (the `alloc_core::node` seam), same
        // discipline as `next_abandoned_atomic`/`owner_state_atomic`.
        let head_ref: &AtomicPtr<u8> = Node::atomic_ptr_ref(head);
        crate::alloc_core::deferred_large::push_large_deferred_free(head_ref, base);
    }

    /// 0.3.0 (task A1); extracted for #132: drain this heap's deferred-free
    /// stack, reclaiming every queued Large/huge segment base via
    /// [`AllocCore::reclaim_large_segment`]. Called by the OWNER on its own
    /// `alloc_large` slow path, before reserving a fresh segment, so a
    /// cross-thread-freed large segment becomes available for reuse (via the
    /// `alloc-decommit` large-cache) or is released to the OS immediately
    /// (without `alloc-decommit`) — either way its `SegmentTable` slot is
    /// freed for reuse (the fix for the A1 permanent-leak bug).
    ///
    /// Thin delegation to the shared
    /// [`alloc_core::deferred_large::drain_large_deferred_free`] primitive
    /// (byte-for-byte the same pop-loop/reclaim logic this method used to
    /// inline).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn drain_large_deferred_free(&mut self) {
        crate::alloc_core::deferred_large::drain_large_deferred_free(
            &self.thread_free,
            &mut self.core,
        );
    }

    // -----------------------------------------------------------------------
    // Allocation entry points (12.3). Delegate to the substrate; under
    // `alloc-xthread` also drain the TFS and stamp segment ownership.
    // -----------------------------------------------------------------------

    /// Allocate `layout.size()` bytes satisfying `layout.align()`. Returns a
    /// non-null `*mut u8` on success, or null on OOM. Memory is
    /// **uninitialised** (matching `GlobalAlloc::alloc`).
    ///
    /// Own-thread path: delegates to [`AllocCore::alloc`] (the single-thread
    /// substrate, no adoption hook — a heap owns its segments exclusively and
    /// never pulls in segments from other heaps). Under `alloc-xthread`,
    /// cross-thread frees that targeted this heap's segments sit in each
    /// segment's [`RemoteFreeRing`](crate::alloc_core::remote_free_ring) and are
    /// reclaimed LAZILY by [`AllocCore::find_segment_with_free`] on a free-list
    /// miss (it drains every owned segment's ring via `reclaim_offset`, which
    /// trusts the class carried in the ring entry — never the owner's `page_map`,
    /// unreliable for mixed-class pages, §13). This is the `ShardedRegion` 7b
    /// shard-reuse discipline; everything else is single-writer (this thread
    /// owns the slot, ergo its segments).
    #[must_use]
    #[inline(always)]
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        // 0.3.0 (task A1): drain this heap's cross-thread Large-segment
        // deferred-free stack before a Large-classified request reaches
        // `AllocCore::alloc_large`'s slow path. Mirrors the RemoteFreeRing's
        // lazy-drain discipline (see the comment block below) but is scoped
        // to ONLY the Large-request case (checked here, not inside
        // `AllocCore`, because `AllocCore` has no `HeapCore` back-reference
        // to drain from) — small-classified requests pay zero cost for this
        // check beyond the one `class_for` call already needed below.
        // Э9 (P7.1, task #160): classify ONCE. `size`, `align` and
        // `class_for(size, align)` are pure functions of `layout`; they were
        // previously computed TWICE per alloc under production (once in the
        // xthread Large-drain check, once in the fastbin magazine-routing
        // block). We compute them a single time here and thread the result
        // through both consumers. The binding is gated on `any(...)` so it
        // exists whenever EITHER consumer is compiled in, and each consuming
        // block stays behind its own cfg. Behaviour is byte-identical
        // (`class_for` is pure → same index; the A1 Large-drain fires for
        // exactly the same Large-classified layouts).
        #[cfg(any(
            feature = "alloc-xthread",
            all(feature = "alloc-global", feature = "fastbin")
        ))]
        let size = layout
            .size()
            .max(crate::alloc_core::size_classes::MIN_BLOCK);
        #[cfg(any(
            feature = "alloc-xthread",
            all(feature = "alloc-global", feature = "fastbin")
        ))]
        let align = layout.align();
        #[cfg(any(
            feature = "alloc-xthread",
            all(feature = "alloc-global", feature = "fastbin")
        ))]
        let class = crate::alloc_core::size_classes::SizeClasses::class_for(size, align);

        // 0.3.0 (task A1): drain this heap's cross-thread Large-segment
        // deferred-free stack before a Large-classified request reaches
        // `AllocCore::alloc_large`'s slow path. Uses the single `class`
        // computed above (Large ⇔ `class.is_none()`).
        #[cfg(feature = "alloc-xthread")]
        {
            if class.is_none() {
                self.drain_large_deferred_free();
            }
        }

        // Cross-thread-freed blocks are reclaimed LAZILY, inside
        // `AllocCore::find_segment_with_free` (the alloc-slow-path drains each
        // owned segment's `RemoteFreeRing` → `reclaim_offset`). We do NOT drain
        // eagerly on every alloc: that was a redundant deviation from the
        // `ShardedRegion` lazy discipline, and draining-before-alloc under a
        // real allocation workload (the installed `#[global_allocator]` serving
        // libtest's own cross-thread frees) corrupted the free list, while the
        // lazy slow-path drain handles the identical workload correctly
        // (verified: `global_alloc_installed` + `race_repro` ×5). Reclaim
        // completeness is preserved — the owner drains a segment's ring the
        // moment it needs a free block from it; until then cross-thread frees
        // sit in the bounded ring (overflow → bounded leak, the original 7b
        // discipline).

        // ── Magazine fast path (P2+P4, fastbin) ─────────────────────────
        // Small-class allocations are served from the per-thread magazine.
        // On a hit: array pop, return — NO per-alloc stamp (P4 hoist).
        // On a miss: batch-refill via `refill_class_stamped` (stamps each
        // distinct source segment exactly once inside the refill), then pop
        // one. The large path still stamps per-alloc (it does not go
        // through the magazine/refill).
        #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
        {
            use crate::alloc_core::size_classes::SizeClasses;
            // Э9 (P7.1): `size`, `align`, `class` come from the single
            // classification hoisted above — no recompute here.
            // C1 (0.3.0): the magazine fast path used to be gated on
            // `align <= SMALL_ALIGN_MAX` (16), so every align>16 request
            // (tokio `Cell` at align=128, page-aligned buffers, etc.) fell
            // through to the substrate on EVERY alloc/dealloc, bypassing the
            // magazine entirely. This is unnecessary: `class_for(size, align)`
            // already guarantees (for any `Some(c)` it returns) that
            // `block_size(c) % align == 0` — see its divisibility-walk slow
            // path in `size_classes.rs`. Every block carved for class `c` sits
            // at an offset that is a multiple of `block_size(c)` (see
            // `carve_block`'s `align_up(bump, block_size)`), and the segment
            // itself is 4 MiB (SEGMENT)-aligned, so any block of class `c` is
            // automatically `align`-aligned regardless of what `align` was —
            // the SAME guarantee the substrate's own `alloc_small` relies on.
            // Keying the magazine purely by `class_idx` (derived from the
            // caller-supplied `Layout` on both alloc and dealloc, per the
            // `GlobalAlloc` contract) is therefore sound for any align that
            // `class_for` accepted. Cross-thread routing is unaffected: this
            // whole block is the OWN-THREAD path (`dealloc_routing` decides
            // ownership BEFORE reaching `dealloc_own_thread`/the magazine).
            {
                if let Some(c) = class {
                    let cnt = self.tcache.count[c] as usize;
                    if cnt > 0 {
                        // Magazine hit: pop from the top of the stack.
                        // P4: NO stamp here — the block's source segment was
                        // already stamped during the refill that originally
                        // pulled it. The OPT-C cache guarantees the segment
                        // header still carries our ownership.
                        let new_cnt = cnt - 1;
                        self.tcache.count[c] = new_cnt as u16;
                        // Э5 (task #145): load+store instead of `fetch_add` — no
                        // `lock xadd` on the churn hot path. SOUND because this
                        // thread is the SOLE WRITER of ITS OWN `tcache_hits`: the
                        // counter is a per-heap field and this magazine-hit path
                        // runs only on the owning thread (the single-writer
                        // invariant `tls_heap.rs` establishes — `current_for_alloc`
                        // yields `Own(&mut HeapCore)` only to the thread that won
                        // the slot's claim CAS). No other thread ever increments
                        // this field, so a non-atomic RMW split into a Relaxed
                        // load + Relaxed store cannot lose an update. The remote
                        // `stats()` reader (`tcache_hits_total`) still does a
                        // Relaxed atomic load and observes a monotonically
                        // non-decreasing value — identical visibility to the old
                        // `fetch_add(Relaxed)` (Relaxed gives no ordering either
                        // way; only atomicity of the single word, which `store`
                        // preserves). Only the lock prefix is dropped.
                        self.tcache_hits.store(
                            self.tcache_hits.load(Ordering::Relaxed).wrapping_add(1),
                            Ordering::Relaxed,
                        );
                        return self.tcache.slots[c][new_cnt];
                    }

                    // Magazine miss: batch-refill + stamp hoist (P4).
                    // We inline the refill+stamp here instead of calling
                    // `refill_class_stamped` because borrowing `self.core`
                    // and `self.tcache.slots[c]` separately avoids a
                    // double-mutable-borrow conflict on `self`.
                    //
                    // P3 (Э1, task #147): the miss refills via
                    // `refill_class_bump` — bump-direct batched carve. On a
                    // cold miss it drains existing free blocks first
                    // (pop_free / find_segment_with_free, which reclaims
                    // cross-thread frees — source order preserved), then
                    // bump-carves the remaining slots DIRECTLY into the
                    // magazine, skipping the old carve→BinTable→pop_free
                    // round-trip (a tautology on freshly-carved virgin
                    // blocks — bit 0 is already "allocated", so setting it
                    // free and immediately clearing it was pure overhead).
                    // D1/M2 end-state is byte-identical to the former
                    // `refill_class` (see `refill_class_bump`'s proofs).
                    //
                    // P3 (task #147): the P7 alloc-side bulk-bypass and the
                    // `alloc_streak` counter are RETIRED. bump-direct IS the
                    // ideal bulk path — a magazine miss now carves straight
                    // into the magazine at near-`memcpy` cost, so the
                    // "skip the magazine on an alloc-without-free streak"
                    // heuristic no longer buys anything. Retiring the alloc
                    // side also retires the dealloc-side companion flush
                    // (see `dealloc_own_thread`): without a streak counter it
                    // could never fire, so keeping it would be dead code.
                    //
                    // D3: the refill amount is a per-class BYTE budget, not
                    // the fixed `TCACHE_CAP` for every class — see
                    // `refill_n_for_class`. Small classes still get the full
                    // `TCACHE_CAP` (unchanged behaviour); large small-classes
                    // (block_size approaching SMALL_MAX) get fewer blocks per
                    // refill, so one magazine miss cannot park megabytes in a
                    // single idle thread's cache.
                    let want = super::tcache::refill_n_for_class(SizeClasses::block_size(c));
                    let n = self
                        .core
                        .refill_class_bump(c, &mut self.tcache.slots[c][0..want]);
                    if n == 0 {
                        return core::ptr::null_mut(); // true OOM
                    }
                    // P4 stamp hoist + Э11 (task #161) stamp-dedupe: stamp each
                    // pulled block's source segment, but call
                    // `stamp_segment_owner` only when the block's segment base
                    // CHANGES from the previous block's. `stamp_segment_owner`
                    // is idempotent per segment (it stamps the segment header,
                    // not the block), so stamping once per DISTINCT source
                    // segment is sufficient — every source segment is still
                    // stamped before any of its blocks is handed out (the same
                    // guarantee as the P4 hoist). The OPT-C cache already
                    // fast-pathed repeated same-segment stamps to a
                    // mask+compare+Relaxed-load; tracking `prev_base` here skips
                    // even that per-block cost. A batch drain (Э7) yields long
                    // same-segment runs, so this collapses to ~one stamp per
                    // refill in the common single-segment case.
                    //
                    // `usize::MAX` is not a valid segment base (bases are
                    // SEGMENT-aligned pointers ≪ usize::MAX), so it is a safe
                    // "no previous segment" sentinel that forces the first
                    // non-null block to stamp.
                    let mut prev_base = usize::MAX;
                    for i in 0..n {
                        let p = self.tcache.slots[c][i];
                        if !p.is_null() {
                            let base = os::segment_base_of_ptr(p) as usize;
                            if base != prev_base {
                                self.stamp_segment_owner(p);
                                prev_base = base;
                            }
                        }
                    }

                    // Pop the top, leave n-1 in the magazine.
                    let new_cnt = n - 1;
                    self.tcache.count[c] = new_cnt as u16;
                    return self.tcache.slots[c][new_cnt];
                }
                // not a small class -> fall through to large path
            }
        }

        // Existing path: reclaim+alloc through AllocCore (large, or non-fastbin).
        let ptr = self.core.alloc(layout);
        if !ptr.is_null() {
            self.stamp_segment_owner(ptr);
        }
        ptr
    }

    /// Allocate `layout.size()` bytes of **zeroed** memory.
    #[must_use]
    #[inline]
    pub fn alloc_zeroed(&mut self, layout: Layout) -> *mut u8 {
        let ptr = self.alloc(layout);
        if !ptr.is_null() {
            Node::zero(
                ptr,
                layout
                    .size()
                    .max(crate::alloc_core::size_classes::MIN_BLOCK),
            );
        }
        ptr
    }

    /// Deallocate `ptr` (previously returned by [`alloc`](Self::alloc)).
    ///
    /// Own-thread path: routes to the owning segment's `BinTable` via
    /// [`AllocCore::dealloc`] (which applies the M2 double-free guard).
    /// Under `alloc-xthread`: if the segment is stamped with another heap's
    /// head, route cross-thread via the TFS (the §2.2 protocol re-based on
    /// the registry). Foreign pointers (not a sefer segment) are a safe
    /// no-op.
    #[inline(always)]
    pub fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        #[cfg(feature = "alloc-xthread")]
        {
            self.dealloc_routing(ptr, layout);
        }
        #[cfg(not(feature = "alloc-xthread"))]
        {
            self.dealloc_own_thread(ptr, layout);
        }
    }

    /// Own-thread dealloc: small frees go to the magazine (under fastbin),
    /// everything else to `core.dealloc`. Called from the `!alloc-xthread`
    /// path (no routing needed) and from `dealloc_routing` after confirming
    /// the block is ours.
    ///
    /// Э9 (P7.1, task #160): under fastbin this delegates to
    /// [`dealloc_own_thread_with_base`](Self::dealloc_own_thread_with_base),
    /// computing `base = os::segment_base_of_ptr(ptr)` itself. The
    /// Э9 (P7.1): under fastbin the magazine body lives in
    /// [`dealloc_own_thread_with_base`](Self::dealloc_own_thread_with_base)
    /// (which takes the pre-computed `base`), and BOTH callers of the
    /// own-thread path under fastbin already hold `base` (the cross-thread
    /// `dealloc_routing` from its `contains_base` check; there is no
    /// `!alloc-xthread` caller under fastbin since `fastbin ⟹ alloc-xthread`).
    /// So this own-arg wrapper is compiled ONLY when fastbin is OFF — where the
    /// own-thread path has no magazine and simply delegates to `core.dealloc`.
    /// Callers: the `!alloc-xthread` branch of [`dealloc`](Self::dealloc) and
    /// the non-fastbin arm of `dealloc_routing`.
    #[cfg(not(all(feature = "alloc-global", feature = "fastbin")))]
    #[inline(always)]
    fn dealloc_own_thread(&mut self, ptr: *mut u8, layout: Layout) {
        // Non-fastbin own-thread free: no magazine — delegate to core.
        self.core.dealloc(ptr, layout);
    }

    /// Э9 (P7.1, task #160): own-thread dealloc body, taking a pre-computed
    /// `base = os::segment_base_of_ptr(ptr)` so the cross-thread
    /// [`dealloc_routing`](Self::dealloc_routing) path — which already
    /// computed `base` for its `contains_base` ownership check — does not
    /// recompute it. Behaviour is byte-identical to the former inline body:
    /// the R1 `off >= bump` stale-free guard and the Э6 magazine/bitmap M2
    /// oracles all operate on this passed-in `base` (which equals what they
    /// used to compute locally, `segment_base_of_ptr` being pure).
    ///
    /// Only compiled under fastbin (the only build with a magazine + the only
    /// consumer of `base` on this path).
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[inline(always)]
    fn dealloc_own_thread_with_base(&mut self, ptr: *mut u8, layout: Layout, base: *mut u8) {
        {
            use super::tcache::{FLUSH_N, TCACHE_CAP};
            use crate::alloc_core::size_classes::{SizeClasses, MIN_BLOCK};
            let size = layout.size().max(MIN_BLOCK);
            let align = layout.align();
            // C1 (0.3.0): gate removed — see the matching comment in `alloc`'s
            // magazine fast path above for the full soundness argument
            // (`class_for` guarantees `block_size % align == 0` for any
            // `Some(c)` it returns, so keying the magazine by class alone is
            // sound for any align it accepted, not just align<=16).
            {
                if let Some(c) = SizeClasses::class_for(size, align) {
                    let cnt = self.tcache.count[c] as usize;

                    // ── H1 (task #167): interior-pointer guard (HARDENED) ──
                    // A block start of class `c` always sits at a segment
                    // offset that is a whole multiple of `block_size(c)`
                    // (carve aligns the bump to `block_size`). An INTERIOR
                    // pointer (offset into a live block, not its start) has
                    // `off % block_size(c) != 0`. The M2 oracles below are
                    // BLIND to this: the alloc bitmap is indexed at
                    // `off >> MIN_BLOCK_SHIFT` (16 B granularity), so an
                    // interior offset that is still 16 B-aligned maps to a
                    // DIFFERENT bit that reads "allocated" → the bogus pointer
                    // falls through and is pushed into the magazine → a later
                    // alloc hands out a mid-block address → silent aliasing /
                    // corruption. This guard rejects it as a no-op.
                    //
                    // Cost: a `%` by a non-power-of-two `block_size` (a real
                    // division, ~tens of cycles) on EVERY small free — NOT
                    // free, so gated behind `hardened` (default OFF), never on
                    // the production hot path. `block_size(c)` is a table load.
                    #[cfg(feature = "hardened")]
                    {
                        let off_h = (ptr as usize).wrapping_sub(base as usize);
                        let bs = SizeClasses::block_size(c);
                        if !off_h.is_multiple_of(bs) {
                            return; // interior-pointer free — no-op
                        }
                    }

                    // ── M2 double-free guard (Э6, P6.1) ──────────────────
                    // The two exact oracles are consulted on every free (no
                    // block-body filter gates them), and the block body is never
                    // read or written on the free path. They are EXACT for the
                    // two own-thread resting places (this class's magazine + the
                    // BinTable free list); see the RESIDUAL M2 LIMIT note below
                    // for the cross-thread-double-free case (undrained
                    // RemoteFreeRing entry) they do NOT cover — task #164.
                    //
                    // The pre-Э6 design used a per-heap key stamped into the
                    // block's word1 (bytes 8..16) as a fast-path FILTER:
                    // `word1 != key` skipped the oracles and pushed directly.
                    // That filter cost a read+write of the BLOCK BODY on every
                    // push (a cold/conflict cache line at block stride — the
                    // 256 B churn regression), and — worse — it was UNSOUND
                    // under user writes: once the user wrote to bytes 8..16 of
                    // a block (legitimate use of allocated memory), a later
                    // double-free saw `word1 != key`, SKIPPED the oracles, and
                    // fell through to push → the block landed BOTH in the
                    // magazine AND on a BinTable free list → the same pointer
                    // issued twice.
                    //
                    // Э6 removes the filter and always runs the two exact
                    // oracles, in this exact order:
                    //
                    //   (1) in-magazine scan  — catches a block freed but not
                    //       yet flushed (still queued in `slots`). Bounded by
                    //       `cnt <= TCACHE_CAP` (16); in churn cnt is 1–3 and
                    //       the array is hot/L1.
                    //   (2) BinTable bitmap   — catches a block that was
                    //       flushed to a free list (`is_free(off)` set). The
                    //       bitmap line is shared by hundreds of blocks → hot.
                    //
                    // A genuinely live block is in neither → push. Order is
                    // load-bearing: scan FIRST (unflushed), bitmap SECOND
                    // (flushed); do NOT reorder.
                    //
                    // This STRENGTHENS M2 for the OWN-THREAD double-free: the
                    // pre-Э6 flushed-double-free hole (user overwrote word1 →
                    // stale/garbage key → oracles skipped → double-issue) is
                    // now closed unconditionally — the bitmap oracle no longer
                    // depends on the block body being pristine. That is a strict
                    // correctness improvement, not a trade, and it is EXACT for
                    // the two own-thread resting places a freed block can be in:
                    // (1) this class's magazine (the scan), and (2) the segment's
                    // BinTable free list (the bitmap). The magazine free path now
                    // touches no block body at all (mimalloc, by contrast, must
                    // write `next` into the body on every free — we are
                    // structurally cheaper per free on cold working sets).
                    //
                    // ── RESIDUAL M2 LIMIT (cross-thread double-free) — task #164
                    // ─────────────────────────────────────────────────────────
                    // The two oracles are exact ONLY for those two resting
                    // places. They are BLIND to a third, transient one: a block
                    // whose CROSS-THREAD free is still in-flight — packed into
                    // its segment's `RemoteFreeRing` (see
                    // `crate::alloc_core::remote_free_ring::RemoteFreeRing` and
                    // `dealloc_routing`'s Variant-2 push) but NOT YET DRAINED by
                    // the owner. A ring entry sets NEITHER oracle: the in-magazine
                    // scan cannot see it (it is not in `slots`), and the bitmap
                    // still reads "allocated" (only the owner-side drain's
                    // `reclaim_offset` → `mark_free` sets the bit; the ring push
                    // deliberately leaves the bitmap untouched). So if a block P
                    // is freed cross-thread (queued in the ring) AND ALSO freed
                    // own-thread here before the owner drains that ring entry —
                    // a genuine USER cross-thread double-free — both oracles pass
                    // and P is pushed into the magazine. P is then BOTH
                    // magazine-resident AND pending in the ring; a later drain's
                    // `reclaim_offset` (which also passes its own is_free/bump
                    // guards, P being still-carved) links P onto the BinTable and
                    // `dec_live`s it, clobbering P's now-live user bytes if the
                    // magazine already re-issued it → double-issue + freelist
                    // corruption (and, under `alloc-decommit`, a possible
                    // decommit+unmap of a magazine-resident segment).
                    //
                    // This is a PRE-EXISTING residual limit of the ring↔magazine
                    // composition (present since fastbin; Э6 neither opened nor
                    // closed it — it closed only the word1-overwrite hole above).
                    // It is fundamentally UB as with any allocator for a
                    // double-free (M2 promises an exact no-op only for the
                    // live/mapped, single-legged case), mirroring the released-
                    // Large-segment residual note in `dealloc_routing`. The real
                    // fix (a drain-with-magazine-visibility / per-heap bloom /
                    // conflict-list hybrid — ring-peek is rejected as 256 loads
                    // per free) is tracked as task #164; pinned by
                    // `tests/regression_xthread_double_free_residual.rs` (RED,
                    // #[ignore]d) and modelled by
                    // `tests/loom_magazine_ring_compose.rs`.
                    //
                    // (1) in-magazine DF oracle — ALWAYS. Э10 (P7.4): the
                    // sequential early-exit scan above (`for i in 0..cnt`) does
                    // not vectorize; in a free-storm `cnt` sits at TCACHE_CAP=16
                    // right before every overflow, so this runs hot. Rewritten
                    // branchless: process `floor(cnt/4)*4` entries in chunks of
                    // 4 with an OR-combined equality (one branch per chunk),
                    // then a scalar tail of the remaining `cnt%4` (0..3).
                    //
                    // CRITICAL — never compare an index >= cnt. `slots[c]` is a
                    // fixed `[*mut u8; TCACHE_CAP]`; entries at `i >= cnt` are
                    // STALE (the magazine is a stack — slots above `cnt` hold
                    // old pointers of blocks since re-issued). A stale MATCH
                    // would turn a legitimate free into a false double-free
                    // no-op → a LEAK. So we scan EXACTLY `cnt` entries: the
                    // chunk loop covers `0..(cnt/4)*4` and the tail covers
                    // `(cnt/4)*4..cnt`. We do NOT round `cnt` up to a multiple
                    // of 4 (that would read stale slots). Semantics are
                    // byte-identical to the old loop: the tested set
                    // `{slots[c][i] : i < cnt}` is unchanged; only the
                    // evaluation order (chunked OR vs. sequential early-exit)
                    // differs.
                    let slots = &self.tcache.slots[c];
                    let chunks = cnt & !3; // (cnt / 4) * 4
                    let mut i = 0;
                    while i < chunks {
                        let hit = (slots[i] == ptr)
                            | (slots[i + 1] == ptr)
                            | (slots[i + 2] == ptr)
                            | (slots[i + 3] == ptr);
                        if hit {
                            return; // in-magazine double-free — no-op
                        }
                        i += 4;
                    }
                    while i < cnt {
                        if slots[i] == ptr {
                            return; // in-magazine double-free — no-op
                        }
                        i += 1;
                    }
                    // (2) flushed DF oracle — ALWAYS. `base`/`off`/bitmap are
                    // read on a segment already PROVEN ours and mapped by
                    // `dealloc_routing`'s `contains_base` ownership check
                    // (fastbin ⇒ alloc-xthread structurally), exactly as
                    // before. Э9 (P7.1): `base` is the pre-computed argument
                    // (same value `segment_base_of_ptr` would return — pure),
                    // threaded in from `dealloc_routing` so it is computed
                    // once on the own-thread free path.
                    let off = (ptr as usize - base as usize) as u32;
                    let meta = SegmentMeta::new(base);
                    // Stale-free guard, parity with `dealloc_small`
                    // (alloc_core.rs). A block that was carved into a segment
                    // later decommitted+reset has `off >= bump` (bump was reset
                    // to small_meta_end and the bitmap zeroed = "allocated", so
                    // the bitmap oracle below would NOT catch it); likewise a
                    // never-carved in-segment address. A real, currently-carved
                    // live block always has `off < bump`, so no false positive
                    // on a legitimate free. Owner-only `bump` read
                    // (single-writer), gated to the feature that resets the
                    // bump — exactly as `dealloc_small`.
                    #[cfg(feature = "alloc-decommit")]
                    if (off as usize) >= meta.bump_of() {
                        return;
                    }
                    if meta.alloc_bitmap().is_free(off) {
                        return; // flushed-then-double-freed — no-op
                    }

                    if cnt < TCACHE_CAP {
                        // Legit free → push. NO key stamp, NO block-body write.
                        self.tcache.slots[c][cnt] = ptr;
                        self.tcache.count[c] = (cnt + 1) as u16;
                        return;
                    }
                    // ── Magazine overflow (cnt == TCACHE_CAP) ──────────
                    // P3 (task #147): the P7 dealloc-side bulk-mode bypass is
                    // RETIRED together with the alloc-side bypass and the
                    // `alloc_streak` counter. That branch fired only when the
                    // alloc side had advanced the streak past BULK_THRESHOLD;
                    // with the counter gone it could never fire, so keeping it
                    // would be dead code guarded by a stuck-at-0 condition.
                    // The always-taken half-flush + compact + push below is the
                    // sole overflow policy now. D1/M2 unchanged: `flush_class`
                    // returns blocks to the substrate via `dealloc_small`
                    // (mark_free + dec_live) exactly as before.
                    //
                    // Normal overflow: half-flush, then push.
                    self.core.flush_class(c, &self.tcache.slots[c][0..FLUSH_N]);
                    // Compact: shift entries [FLUSH_N..CAP] down to [0..CAP-FLUSH_N].
                    let remaining = TCACHE_CAP - FLUSH_N;
                    for i in 0..remaining {
                        self.tcache.slots[c][i] = self.tcache.slots[c][i + FLUSH_N];
                    }
                    // Push (Э6: NO key stamp, NO block-body write). The oracles
                    // above already ran before this overflow branch, so a
                    // double-free is caught even when the magazine is full.
                    self.tcache.slots[c][remaining] = ptr;
                    self.tcache.count[c] = (remaining + 1) as u16;
                    return;
                }
            }
        }
        // Large / non-small / non-fastbin: delegate to core.
        self.core.dealloc(ptr, layout);
    }

    /// Shrink/grow an allocation. Returns null on OOM (leaving the old
    /// allocation intact). Null `ptr` returns null.
    ///
    /// ## C2 (0.3.0) — delegate own-thread reallocs to `AllocCore::realloc`
    ///
    /// This used to ALWAYS do alloc+copy+dealloc, even when `ptr` belongs to
    /// one of our own segments — which meant the OPT-F in-place short-circuit
    /// in [`AllocCore::realloc`] (same-class shrink/no-op — see its doc
    /// comment, especially the `==` vs `<=` correctness note fixed alongside
    /// #114) was dead code on the `HeapCore`/global-allocator face: nothing
    /// ever called it. Every `realloc` through `SeferAlloc` paid a full
    /// alloc+copy+dealloc even for a same-class resize that could have
    /// returned the original pointer untouched.
    ///
    /// The fix: if `ptr` lives in one of OUR segments (`segment_bases()`
    /// contains its base — the same ownership test `dbg_owner_id_for` already
    /// uses), delegate to `self.core.realloc`, which performs the in-place
    /// short-circuit for a same-class small→small resize and otherwise falls
    /// back to alloc+copy+dealloc INTERNALLY (so behaviour for the
    /// non-in-place cases is unchanged, just now funnelled through one
    /// correct implementation instead of a second, redundant one here).
    ///
    /// We must NOT call `AllocCore::realloc` for a pointer we do not own
    /// (e.g. under `alloc-xthread`, a block that lives in ANOTHER heap's
    /// segment): `AllocCore::realloc`'s `self.table.contains_base` check
    /// would correctly reject it and fall through to its own alloc+copy+
    /// dealloc, but that alloc would happen on OUR heap while the dealloc of
    /// the OLD block would go through OUR `core.dealloc` — which does not
    /// know how to route a foreign pointer cross-thread (only `HeapCore`'s
    /// `dealloc`/`dealloc_routing` does). So a foreign `ptr` takes the
    /// original path here: alloc a new block on OUR heap, copy
    /// `min(old, new)`, then free the OLD pointer via `self.dealloc` (which
    /// DOES route cross-thread correctly under `alloc-xthread`).
    pub fn realloc(&mut self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return core::ptr::null_mut();
        }
        #[cfg(feature = "alloc-global")]
        {
            let base = os::segment_base_of_ptr(ptr);
            // Task #135 (Part 2): O(1) membership test (`AllocCore::contains_base`
            // → the OPT-B hash table) replaces the O(segment count) linear scan
            // `segment_bases().any(|b| b == base)`. Same semantics: `true` iff
            // `base` is one of THIS heap's registered, live segments.
            if self.core.contains_base(base) {
                // Own-segment pointer: delegate to the substrate, which
                // performs the in-place short-circuit (OPT-F) when possible
                // and alloc+copy+dealloc otherwise -- entirely within
                // `self.core`, so no cross-thread routing concern here (we
                // own this segment).
                //
                // MUST-1 (0.3.0, C2 regression fix): `AllocCore::realloc` may
                // internally `alloc` a FRESH segment for the non-in-place
                // cases (a large→large grow carves a new dedicated Large
                // segment; a small→new resize carves a new small segment).
                // That substrate alloc does NOT run the two ownership hooks
                // `HeapCore::alloc` applies — segment-ownership stamping
                // (`stamp_segment_owner`, which under `alloc-xthread` also
                // writes `owner_thread_free`, the field that makes a remote
                // free route back here instead of leaking) and the A1
                // deferred-large drain (`drain_large_deferred_free`). Without
                // them, a Vec grown via realloc on thread A lives in an
                // UNSTAMPED (`owner_thread_free == null`) Large segment; when
                // A hands it to thread B and B drops it, `dealloc_routing`
                // sees not-ours + magic OK + `owner_tf == null` → silent
                // no-op → the whole segment (4+ MiB) and its `SegmentTable`
                // slot leak forever (the resurrected A1/#114 leak-to-abort).
                //
                // Fix: mirror `HeapCore::alloc`'s two hooks around the
                // substrate realloc.
                //
                //   (1) A1 Large drain — BEFORE delegating, if the NEW request
                //       classifies as Large (`class_for(...).is_none()`, the
                //       exact predicate `alloc` uses), drain this heap's
                //       deferred-free stack so a realloc-growth-only thread
                //       still reclaims cross-thread-freed large segments
                //       (otherwise its stack accumulates unboundedly — the
                //       A1 drain-bypass leg of the bug).
                #[cfg(feature = "alloc-xthread")]
                {
                    let class = crate::alloc_core::size_classes::SizeClasses::class_for(
                        new_size.max(crate::alloc_core::size_classes::MIN_BLOCK),
                        old_layout.align(),
                    );
                    if class.is_none() {
                        self.drain_large_deferred_free();
                    }
                }
                //   (2) Ownership stamp — stamp the RESULT so any fresh
                //       segment the substrate carved gets `owner_state` +
                //       `owner_thread_free` written (the latter is what makes
                //       a later cross-thread free route back here). An
                //       in-place realloc returns the SAME `ptr`, whose segment
                //       was already stamped when it was first allocated, so we
                //       skip re-stamping then (`stamp_segment_owner` is
                //       idempotent via the OPT-C cache, so this is only an
                //       optimisation). A null result is an OOM — nothing to
                //       stamp.
                let p = self.core.realloc(ptr, old_layout, new_size);
                #[cfg(feature = "alloc-global")]
                if !p.is_null() && p != ptr {
                    self.stamp_segment_owner(p);
                }
                return p;
            }
        }
        // Foreign pointer (not one of our segments) or `alloc-global` absent:
        // alloc a fresh block on OUR heap, copy, then free the OLD pointer
        // through `self.dealloc` (which routes cross-thread correctly under
        // `alloc-xthread`; under plain own-thread builds this is `core.dealloc`,
        // a safe no-op for a truly foreign pointer per its own contract).
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

    // -----------------------------------------------------------------------
    // Cross-thread free routing (only under `alloc-xthread`).
    //
    // This re-bases the Phase 10 `Heap::dealloc_small` /
    // `Heap::dealloc_any_thread` discipline on the registry-resident
    // `HeapCore`. The block at `ptr` may belong to:
    //   - a segment THIS heap owns (stamped with our head, or unstamped) →
    //     own-thread path via `AllocCore::dealloc`;
    //   - a segment owned by ANOTHER heap (stamped with its head) → push
    //     onto that heap's TFS via `ThreadFreeStack::push`;
    //   - a foreign (non-sefer) pointer → safe no-op.
    // -----------------------------------------------------------------------

    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    fn dealloc_routing(&mut self, ptr: *mut u8, layout: Layout) {
        let base = os::segment_base_of_ptr(ptr);

        // Task #135 (Part 3, M2 hardening): check `self.core.contains_base(base)`
        // FIRST, before touching any segment memory. `contains_base` is an O(1)
        // lookup in OUR OWN `SegmentTable`'s open-addressing hash — it reads
        // only our own primordial-segment-resident table, never `base`'s
        // memory, so it is safe to call even if `base` is unmapped (a
        // released/decommitted segment).
        //
        // `contains_base(base) == true` if and only if `base` is currently
        // registered in OUR table — which happens exactly when we own a live
        // (mapped) segment there (`register_segment`/`alloc_large*` register
        // on creation; `unregister`/`recycle` remove on release — see
        // `segment_table.rs`). So TRUE implies "our segment, definitely
        // mapped" — equivalent to the old `owner_tf.is_null() || owner_tf ==
        // our_head` condition for every segment WE registered (an unstamped
        // own-segment has `owner_tf == null`; a stamped own-segment has
        // `owner_tf == our_head` — both cases are covered by "it's in our
        // table"), without reading `base`'s memory at all. Route it own-thread
        // immediately — no magic/kind read needed.
        if self.core.contains_base(base) {
            // Э9 (P7.1): `base` is already in hand from the `contains_base`
            // ownership check above; under fastbin, hand it to the own-thread
            // body directly so `segment_base_of_ptr` is not recomputed. Under
            // non-fastbin `dealloc_own_thread` just delegates to `core.dealloc`
            // (base unused there).
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            self.dealloc_own_thread_with_base(ptr, layout, base);
            #[cfg(not(all(feature = "alloc-global", feature = "fastbin")))]
            self.dealloc_own_thread(ptr, layout);
            return;
        }

        // `contains_base` is FALSE: `base` is not one of OUR segments. Two
        // possibilities:
        //   (a) a LIVE segment owned by ANOTHER heap — mapped, its owner's
        //       table contains it (just not ours) — reading its header is
        //       safe, and this cross-thread free must be routed to its owner.
        //   (b) a segment WE (or someone) already released — decommitted +
        //       unmapped, its table slot recycled — reading its header would
        //       fault.
        // We cannot O(1)-distinguish (a) from (b) without a global registry
        // (out of scope here); this is the same limitation every allocator
        // has for a double-free-after-full-release. A double-free of a
        // released, unmapped segment is fundamentally UB (as with any
        // allocator) and is NOT fixed by this change — only guarded for the
        // live/mapped case, which is what M2 promises. See the module-level
        // note referenced from task #135's report for the full argument.
        //
        // 0.3.0 (task #138): for the Large branch below, a further
        // POST-reuse mitigation (layout-vs-header size consistency check,
        // `large_layout_consistent`) narrows — but does not close — the
        // remaining window where `base` WAS released and has since been
        // reused for a new allocation before this stale free arrives. See
        // that function's doc comment for the residual limit.
        //
        // Field-specific reads (task #33 root-cause fix): read ONLY `magic`,
        // `kind`, `owner_thread_free` — the cross-thread-read fields written
        // once at init/stamp time and only read thereafter. A full-struct
        // `SegmentHeader::read_at` here raced with the Owner's `bump`-touching
        // `write_header` on `carve_block` (the §11 data race); reading each
        // field individually via its `offset_of!` offset touches bytes
        // disjoint from the owner-mutated `bump`, so there is no race.
        if SegmentHeader::magic_at(base) != SEGMENT_MAGIC {
            return;
        }
        let our_head = self.thread_free_head();
        let owner_tf = SegmentHeader::owner_thread_free_at(base);
        if owner_tf.is_null() || owner_tf == our_head {
            // `contains_base` was false, yet the header claims this segment is
            // unstamped or stamped as ours — this can only happen for a
            // segment that used to be ours and was released (case (b) above,
            // reading now-decommitted-but-still-committed metadata pages of a
            // NOT-YET-actually-unmapped segment is impossible in this
            // process — metadata pages are only unmapped by `os::release_segment`,
            // at which point this read would fault, not return a stale value).
            // Defensive no-op: do NOT route to ourselves via a table state we
            // just proved does not list this segment.
            return;
        }
        if SegmentHeader::kind_at(base) == SegmentKind::Large {
            // 0.3.0 (task A1): used to be a bare `return` here — a PERMANENT
            // leak. The whole segment (4+ MiB, or more for an oversized
            // allocation) was never released and its `SegmentTable` slot was
            // never recycled, because no code path ever revisited a
            // cross-thread-freed Large segment. Fix: push `base` onto the
            // OWNING heap's deferred-free stack (`owner_tf`, already read
            // above — the owner's `thread_free_head()`); the owner reclaims
            // it lazily on its next `alloc_large` slow path (see
            // `drain_large_deferred_free`, called from `alloc`).
            //
            // 0.3.0 (task #138, A1 post-reuse mitigation): before queuing,
            // check that `layout`'s size matches the CURRENT occupant's
            // `large_size` in the header. A stale double-free whose segment
            // was ALREADY reclaimed+reused between the original free and
            // this call will, in the overwhelming majority of cases,
            // observe a header describing a DIFFERENT allocation — this is
            // NOT a full fix (a reuse that happens to request the
            // bit-identical size is not caught; double-free is UB by
            // contract) but narrows the post-reuse corruption window. See
            // `alloc_core::deferred_large::large_layout_consistent`'s doc
            // comment for the full rationale and residual limit.
            if crate::alloc_core::deferred_large::large_layout_consistent(base, layout.size()) {
                Self::push_large_deferred_free(owner_tf, base);
            }
            return;
        }
        // Variant-2: push (offset, class) to the per-segment ring (block bytes
        // untouched). The freer HAS the `Layout`, so it derives the size class
        // here and carries it in the ring entry — the owner's `page_map` is
        // unreliable for the mixed-class pages a shared bump cursor produces, so
        // `reclaim_offset` must NOT derive the class itself (RACE_DRAIN_RECLAIM
        // §13). `kind != Large` is already established above, so a small block's
        // class is always `Some`.
        let off = (ptr as usize - base as usize) as u32;
        let size = layout
            .size()
            .max(crate::alloc_core::size_classes::MIN_BLOCK);
        let class_idx =
            match crate::alloc_core::size_classes::SizeClasses::class_for(size, layout.align()) {
                Some(c) => c as u32,
                None => return, // Large layout on a small segment: contract violation; drop.
            };
        let packed = crate::alloc_core::remote_free_ring::pack_entry(off, class_idx);
        let ring = SegmentMeta::new(base).remote_ring();
        let _ = ring.push(packed);
    }

    /// Stamp a segment's header with this heap's ownership (Phase 12.4). Two
    /// parts:
    ///
    /// 1. **`owner_state = LIVE(self.id, 0)`** — the adoption-coherence field.
    ///    Set on every alloc so the abandonment walk can identify our segments
    ///    and an adopter's CAS has a well-defined expected value. Idempotent:
    ///    a segment already stamped with our id is left alone.
    /// 2. **(alloc-xthread only) `owner_thread_free` head pointer** — the
    ///    cross-thread free routing target, so a remote freer can find this
    ///    heap's TFS. Idempotent: only stamps if currently null.
    ///
    /// Called on the alloc path after a successful allocation. The segment is
    /// exclusively ours (single-writer invariant from the claim CAS), so the
    /// `owner_state` store is race-free.
    ///
    /// ## OPT-C fast path (task #66)
    ///
    /// `last_stamped_segment` caches the base of the most recently stamped
    /// segment. On a cache hit the function performs only a **Relaxed** load
    /// of `owner_state` and compares it with `self.id`. If they match, we
    /// know the segment is already stamped → return immediately with NO
    /// Release-store (the expensive part on x86 — an `MFENCE`-equivalent).
    /// On a miss or on a ownership mismatch the original slow path runs.
    ///
    /// The Relaxed load is safe because:
    /// - This is the **owning thread** — the single writer of `owner_state`
    ///   on this segment. A Relaxed load cannot race with our own prior
    ///   Release-store (same thread → SC-in-program-order).
    /// - A cache miss (base changed or Relaxed-load mismatch) falls through
    ///   to the slow path which restores the Acquire/Release protocol.
    #[inline(always)]
    fn stamp_segment_owner(&mut self, ptr: *mut u8) {
        use crate::alloc_core::segment_header::{unpack_owner_id, OWNER_STATE_LIVE};
        let base = os::segment_base_of_ptr(ptr);

        // -----------------------------------------------------------------------
        // OPT-C fast path: cache-hit check.
        //
        // If the cached segment base matches the current allocation's segment
        // base, do a cheap Relaxed load of `owner_state` to confirm ownership.
        // If ownership is confirmed → early return (no Release-store, no memory
        // fence). If ownership is not confirmed (e.g., segment was recycled and
        // reset to OWNER_ID_NONE) → fall through to the slow path below.
        // -----------------------------------------------------------------------
        if base == self.last_stamped_segment && !self.last_stamped_segment.is_null() {
            // Cache hit: re-check ownership with a cheaper Relaxed load.
            // Owner-only read (we are the sole writer of owner_state on OUR
            // segments), so Relaxed ordering is race-free here.
            let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
            let cur = owner_atomic.load(Ordering::Relaxed);
            if unpack_owner_id(cur) == self.id {
                // Still our segment, already stamped. Skip the Release-store.
                // The alloc-xthread TFS stamp is also idempotent (once
                // stamped it stays); if `last_stamped_segment` is set then
                // the TFS was already written on the slow path below.
                return;
            }
            // Ownership mismatch (e.g., recycled segment): clear the cache
            // and run the slow path.
            self.last_stamped_segment = core::ptr::null_mut();
        }

        // -----------------------------------------------------------------------
        // Slow path: full Acquire-load + conditional Release-store.
        // -----------------------------------------------------------------------
        // `mut` is needed under `alloc-xthread` (the stamp branch below calls
        // `meta.stamp_owner_thread_free(&mut self)`). Silence the unused-mut
        // warning under plain `alloc-global` where the branch is absent.
        #[allow(unused_mut)]
        let mut meta = SegmentMeta::new(base);
        // 1. Stamp owner_state (adoption coherence).
        let owner_atomic = meta.owner_state_atomic();
        let cur = owner_atomic.load(Ordering::Acquire);
        if unpack_owner_id(cur) != self.id {
            let me = pack_owner(OWNER_STATE_LIVE, self.id, 0);
            // Release: a later abandon's Acquire CAS must observe our stamp.
            owner_atomic.store(me, Ordering::Release);
        }
        // 2. (alloc-xthread) Stamp the TFS head for cross-thread routing.
        // Phase 12.5 (shard model): stamped ONCE, when the segment is first
        // allocated from, and NEVER cleared or re-stamped. The inline TFS
        // head's address is stable for the slot's lifetime (it does not
        // change across release→claim), so the stamp remains valid for as
        // long as the slot owns this segment — which is forever in the shard
        // model (segments do not leave their heap).
        //
        // Field-specific write (task #33 root-cause fix): we stamp ONLY the
        // `owner_thread_free` field via `stamp_owner_thread_free`, NOT a
        // full-struct `write_header`. A full-struct RMW here rewrote `bump`
        // and every other field, and — although the stamp itself runs on the
        // owning thread — the struct read it performed (`meta.header()`)
        // raced the Owner's own later `bump` writes is not the issue; the
        // issue is that `write_header` writes `magic`/`kind`/`bump` bytes
        // that a concurrent Remote `dealloc_routing` field-read may observe
        // mid-update. Writing only the `owner_thread_free` word touches bytes
        // disjoint from every field a Remote reads, so there is no race.
        // The single-writer invariant (the slot's owner is the sole writer
        // of its segments' headers) makes the plain field write race-free.
        #[cfg(feature = "alloc-xthread")]
        {
            let cur_head =
                crate::alloc_core::segment_header::SegmentHeader::owner_thread_free_at(base);
            if cur_head.is_null() {
                // Task #142: expose this atomic's provenance so a REMOTE
                // freer can reconstruct a wildcard pointer to it (via
                // `Node::atomic_ptr_ref` → `with_exposed_provenance_mut`)
                // rather than inheriting this owner's `&mut self`-rooted
                // reference provenance — which a concurrent remote write
                // would disable, corrupting other remotes' access (see
                // `Node::atomic_ptr_ref`). `addr_of!` takes the field address
                // WITHOUT an intermediate `&` retag; `expose_provenance`
                // registers it for the paired `with_exposed_provenance_mut`.
                let tf_ptr = core::ptr::addr_of!(self.thread_free);
                let _ = tf_ptr.expose_provenance();
                meta.stamp_owner_thread_free(tf_ptr as *const _);
            }
        }

        // Slow path succeeded: cache the segment base so the next alloc from
        // the same segment takes the fast path.
        self.last_stamped_segment = base;
    }

    /// OPT-C (task #66): reset the stamp cache.
    ///
    /// Sets `last_stamped_segment` to null, forcing the next call to
    /// `stamp_segment_owner` to take the slow path (Acquire-load +
    /// conditional Release-store).
    ///
    /// Call this whenever segment ownership may have changed out of band —
    /// specifically if a future phase introduces inter-heap segment adoption
    /// (e.g., `try_adopt` transferring a segment from an abandoned heap into
    /// this heap). Without a reset, the cache might hit on a segment whose
    /// `owner_state` has already been updated by the adopter's CAS, and the
    /// Relaxed-load fast-path check would detect the mismatch and fall
    /// through correctly — but defensive reset is cleaner.
    ///
    /// **Current status:** In the shard model (Phase 12.5+) segments never
    /// leave their original heap, so this method is not called from any
    /// production path. It is provided as a safety hook for future phases.
    /// TODO: call from `try_adopt` / `reclaim_abandoned` if those paths are
    /// ever wired to transfer segments across heaps.
    #[cfg(feature = "alloc-global")]
    #[allow(dead_code)]
    pub(crate) fn reset_stamp_cache(&mut self) {
        self.last_stamped_segment = core::ptr::null_mut();
    }

    /// TEST-ONLY (P4): read the `owner_id` stamped in the segment header of
    /// the segment that contains `ptr`. Returns `None` if `ptr` is not in a
    /// segment owned by this heap's substrate. Used by
    /// `tests/heap_core_tcache_stamp.rs` to verify the stamp-hoist wrote
    /// the correct ownership.
    #[doc(hidden)]
    #[cfg(feature = "alloc-global")]
    pub fn dbg_owner_id_for(&self, ptr: *mut u8) -> Option<u32> {
        use crate::alloc_core::segment_header::{unpack_owner_id, SegmentMeta};
        let base = os::segment_base_of_ptr(ptr);
        if !self.core.segment_bases().any(|b| b == base) {
            return None;
        }
        let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
        let word = owner_atomic.load(Ordering::Relaxed);
        Some(unpack_owner_id(word))
    }

    /// TEST-ONLY (P4): the cached `last_stamped_segment` base, or null if
    /// no segment has been stamped yet. Allows tests to observe whether the
    /// stamp-cache was updated without re-stamping.
    #[doc(hidden)]
    #[cfg(feature = "alloc-global")]
    pub fn dbg_last_stamped_segment(&self) -> *mut u8 {
        self.last_stamped_segment
    }

    /// TEST-ONLY (P7): read the magazine count for class `c`.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub fn dbg_tcache_count(&self, c: usize) -> u16 {
        self.tcache.count[c]
    }

    /// TEST-ONLY (task D3): resolve the size class index for `layout`, the
    /// same classification `alloc` uses to index `tcache.slots`/`count`.
    /// Delegates to [`AllocCore::dbg_layout_class_for`]; exposed at the
    /// `HeapCore` level because `core` is `pub(crate)` and external
    /// integration tests only see `HeapCore`/`HeapRegistry`.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub fn dbg_class_for(&self, layout: Layout) -> Option<usize> {
        self.core.dbg_layout_class_for(layout)
    }

    /// TEST-ONLY (task D3): the per-class refill amount `alloc`'s
    /// magazine-miss path actually uses for class `c` — i.e.
    /// `super::tcache::refill_n_for_class(SizeClasses::block_size(c))`, the
    /// exact expression `alloc` evaluates. Lets a test assert the byte-budget
    /// clamp fired for a given class without duplicating (and risking
    /// drifting from) the formula.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub fn dbg_refill_n_for_class(&self, c: usize) -> usize {
        super::tcache::refill_n_for_class(crate::alloc_core::size_classes::SizeClasses::block_size(
            c,
        ))
    }

    /// TEST-ONLY (task R2/#154): push `ptr`'s segment-relative offset — packed
    /// with `class_idx` — into its segment's `RemoteFreeRing`, exactly as a
    /// cross-thread freer's `dealloc_routing` Variant-2 push would. Thin
    /// delegation to [`AllocCore::dbg_push_to_ring`]; exposed at the `HeapCore`
    /// level so the ring↔magazine residual-limit pinning test
    /// (`tests/regression_xthread_double_free_residual.rs`) can simulate a
    /// remote free while driving the magazine through `HeapCore`. Returns
    /// `false` if the ring was full or `ptr` is not one of this heap's segments.
    /// Zero production impact: `#[doc(hidden)]`, test-only, delegates to an
    /// existing hook.
    #[doc(hidden)]
    #[cfg(feature = "alloc-xthread")]
    pub fn dbg_push_to_ring(&self, ptr: *mut u8, class_idx: usize) -> bool {
        self.core.dbg_push_to_ring(ptr, class_idx)
    }

    /// TEST-ONLY (task R2/#154): drain every owned segment's `RemoteFreeRing`
    /// into its `BinTable`, exactly as the alloc slow path's lazy drain does,
    /// but unconditionally. Thin delegation to
    /// [`AllocCore::dbg_drain_all_rings`]. Zero production impact.
    #[doc(hidden)]
    #[cfg(feature = "alloc-xthread")]
    pub fn dbg_drain_all_rings(&mut self) {
        self.core.dbg_drain_all_rings();
    }

    /// TEST-ONLY (P5): force-flush every class's magazine back to the
    /// substrate. Used by decommit-soak tests to drain magazine-buffered
    /// blocks before asserting decommit invariants.
    ///
    /// After this call, every magazine slot is empty (`count[c] == 0` for
    /// all classes) and the blocks have been returned to their owning
    /// segments via `flush_class` → `dealloc_small` → `dec_live` →
    /// `maybe_decommit`. If any segment reaches `live_count == 0` during
    /// the flush, decommit fires.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub fn dbg_flush_all(&mut self) {
        use crate::alloc_core::size_classes::SMALL_CLASS_COUNT;
        for c in 0..SMALL_CLASS_COUNT {
            let n = self.tcache.count[c] as usize;
            if n == 0 {
                continue;
            }
            self.core.flush_class(c, &self.tcache.slots[c][0..n]);
            self.tcache.count[c] = 0;
        }
    }
}
