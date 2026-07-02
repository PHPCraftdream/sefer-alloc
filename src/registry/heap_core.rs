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
//! [`HeapCore`] also carries the cross-thread [`ThreadFreeStack`] handle and
//! stamps `owner_thread_free` on segment headers so remote threads can route
//! cross-thread frees here (the §2.2 "owner stamping — 12.3" rule).
//!
//! ## M5-clean bootstrap invariant
//!
//! [`HeapCore::new`] bootstraps via [`AllocCore::new`] (OS aperture only —
//! `mmap`/`VirtualAlloc`, **never** `std::alloc`). The cross-thread
//! [`ThreadFreeStack`] is `Box`-allocated (goes through `std::alloc`), so it
//! is **NOT** constructed in [`HeapCore::new`] — that would violate the
//! M5-clean bootstrap of [`HeapRegistry::claim`] (which lazily materialises
//! a `HeapCore` inside the slot and runs inside the registry's
//! `ensure`/bootstrap path). Instead the TFS handle is installed lazily by
//! [`HeapCore::install_thread_free`], called from the TLS bind-slow path
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

/// TEST-ONLY (0.3.0, task A1): process-wide count of Large/huge segments
/// reclaimed via the cross-thread deferred-free path
/// ([`HeapCore::drain_large_deferred_free`]). Bumped once per segment
/// successfully drained and handed to
/// [`AllocCore::reclaim_large_segment`](crate::alloc_core::AllocCore::reclaim_large_segment).
///
/// Diagnostic only (relaxed, like `DECOMMIT_CALLS` in `alloc_core.rs`), `pub`
/// so `tests/regression_xthread_large_free_no_leak.rs` can assert reclaim
/// actually happened (counterfactual: with the A1 fix reverted — `return` in
/// `dealloc_routing`'s Large branch — this counter stays zero and the
/// regression test goes red).
#[cfg(feature = "alloc-xthread")]
#[doc(hidden)]
pub static DBG_LARGE_XTHREAD_RECLAIMED: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

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
    /// `next_abandoned` header field as the intrusive link. This is safe to
    /// reuse: `next_abandoned` is otherwise only written by the global
    /// abandon/adopt substrate, and Large segments are NEVER abandoned onto
    /// that global stack (the shard model keeps large segments with their
    /// allocating heap; `abandon_segments` only walks a heap's segments on
    /// the now-dormant Phase 12.4 thread-exit path, and even there a Large
    /// segment abandoned mid-flight would simply have its `next_abandoned`
    /// overwritten again — no aliasing hazard, since a segment cannot be on
    /// both stacks at once in the shard model's production path). No ABA tag
    /// is needed here (unlike the global stack): only the OWNER thread ever
    /// pops from this stack (single consumer), so the classic multi-popper
    /// ABA race does not apply; multiple REMOTE threads may push
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
    /// So `live_count` = blocks carved AND not on a BinTable free list
    /// = (blocks handed out to user) + (blocks in our magazine). Decommit
    /// fires only when a segment's blocks are ALL on the BinTable free
    /// list (none handed out, none in magazine).
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub(crate) tcache: super::tcache::Tcache,

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

    /// Lazily install the cross-thread free-stack handle on the TLS bind-slow
    /// path. Allocates a single `Box<AtomicPtr<u8>>` (via `std::alloc`); this
    /// is the ONLY `std::alloc` touch on the `HeapCore` path, and it is
    /// explicitly outside the registry bootstrap (called from the TLS
    /// binding, not from `HeapRegistry::claim`/`ensure`).
    ///
    /// Idempotent: a second call is a no-op (returns without re-allocating).
    /// Returns the stable `*const AtomicPtr<u8>` head pointer that segment
    /// headers store in `owner_thread_free` so cross-thread freers can route
    /// to this heap.
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

    /// 0.3.0 (task A1): push a Large/huge segment `base` onto the OWNING
    /// heap's deferred-free stack, given `head` — the owner's
    /// `thread_free_head()` (a `*const AtomicPtr<u8>`, obtained by a REMOTE
    /// freer from `owner_thread_free_at(segment_base)`). Called from
    /// [`dealloc_routing`](Self::dealloc_routing) in place of the old
    /// permanent-leak no-op.
    ///
    /// Classic Treiber push: read `base`'s `next_abandoned` link (repurposed
    /// here as this stack's intrusive link — see the field doc on
    /// `thread_free`), point it at the current head, CAS the head to `base`.
    /// Multi-producer (any number of remote threads may race this push);
    /// single-consumer (only the owner ever pops, in
    /// [`drain_large_deferred_free`](Self::drain_large_deferred_free)), so no
    /// ABA tag is needed — a pop can only be concurrent with OTHER pushes,
    /// never with another pop.
    ///
    /// # Safety (informal — no `unsafe` used; documented for the concurrency
    /// reasoning, not a soundness escape hatch)
    ///
    /// `head` must be a live owner's `thread_free_head()` (guaranteed by the
    /// caller: `dealloc_routing` reads it fresh from the segment header on
    /// every call). `base` must be a currently-registered `Large`-kind
    /// segment base (guaranteed by the caller: only reached after
    /// `kind_at(base) == SegmentKind::Large`).
    #[cfg(feature = "alloc-xthread")]
    fn push_large_deferred_free(head: *const AtomicPtr<u8>, base: *mut u8) {
        let next_atomic = SegmentMeta::new(base).next_abandoned_atomic();
        // `heap_core.rs` is NOT an allowed `unsafe` seam (see `src/lib.rs`'s
        // seam whitelist), so the pointer-to-reference deref is delegated to
        // `Node::atomic_ptr_ref` (the `alloc_core::node` seam), same
        // discipline as `next_abandoned_atomic`/`owner_state_atomic`.
        let head_ref: &AtomicPtr<u8> = Node::atomic_ptr_ref(head);
        let mut cur = head_ref.load(Ordering::Acquire);
        loop {
            // Link `base` to the current head (or the "empty" sentinel,
            // `core::ptr::null_mut()`, encoded as `ABANDONED_TAIL` in the
            // link word so a later pop can distinguish "no next" from a real
            // base — a real base is never null).
            let next_link = if cur.is_null() {
                crate::alloc_core::segment_header::ABANDONED_TAIL
            } else {
                cur as u64
            };
            next_atomic.store(next_link, Ordering::Release);
            match head_ref.compare_exchange(cur, base, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => return,
                Err(actual) => cur = actual,
            }
        }
    }

    /// 0.3.0 (task A1): drain this heap's deferred-free stack, reclaiming
    /// every queued Large/huge segment base via
    /// [`AllocCore::reclaim_large_segment`]. Called by the OWNER on its own
    /// `alloc_large` slow path, before reserving a fresh segment, so a
    /// cross-thread-freed large segment becomes available for reuse (via the
    /// `alloc-decommit` large-cache) or is released to the OS immediately
    /// (without `alloc-decommit`) — either way its `SegmentTable` slot is
    /// freed for reuse (the fix for the A1 permanent-leak bug).
    ///
    /// Pop loop: single-consumer (only the owner calls this), so a plain pop
    /// — no ABA tag, no CAS-retry-on-pop needed beyond racing concurrent
    /// PUSHERS (remote frees can still be arriving concurrently; the CAS
    /// handles that).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn drain_large_deferred_free(&mut self) {
        loop {
            let cur = self.thread_free.load(Ordering::Acquire);
            if cur.is_null() {
                return;
            }
            let meta = SegmentMeta::new(cur);
            let next_link = meta.next_abandoned_atomic().load(Ordering::Acquire);
            let next = if next_link == crate::alloc_core::segment_header::ABANDONED_TAIL {
                core::ptr::null_mut()
            } else {
                next_link as *mut u8
            };
            match self
                .thread_free
                .compare_exchange(cur, next, Ordering::Acquire, Ordering::Relaxed)
            {
                Ok(_) => {
                    self.core.reclaim_large_segment(cur);
                    DBG_LARGE_XTHREAD_RECLAIMED.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => continue, // a concurrent push raced us — retry with fresh head
            }
        }
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
        #[cfg(feature = "alloc-xthread")]
        {
            let size = layout
                .size()
                .max(crate::alloc_core::size_classes::MIN_BLOCK);
            if crate::alloc_core::size_classes::SizeClasses::class_for(size, layout.align())
                .is_none()
            {
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
            use super::tcache::BULK_THRESHOLD;
            use crate::alloc_core::size_classes::{SizeClasses, SMALL_ALIGN_MAX};
            let size = layout
                .size()
                .max(crate::alloc_core::size_classes::MIN_BLOCK);
            let align = layout.align();
            if align <= SMALL_ALIGN_MAX {
                if let Some(c) = SizeClasses::class_for(size, align) {
                    let cnt = self.tcache.count[c] as usize;
                    if cnt > 0 {
                        // Magazine hit: pop from the top of the stack.
                        // P4: NO stamp here — the block's source segment was
                        // already stamped during the refill that originally
                        // pulled it. The OPT-C cache guarantees the segment
                        // header still carries our ownership.
                        //
                        // P7: NO streak touch here. The streak counts refill
                        // misses, not individual allocs. This keeps the
                        // magazine-hit path (the churn hot path) completely
                        // streak-free — no read, no write of alloc_streak.
                        let new_cnt = cnt - 1;
                        self.tcache.count[c] = new_cnt as u16;
                        return self.tcache.slots[c][new_cnt];
                    }

                    // Magazine miss — check P7 bulk-mode bypass before
                    // attempting a refill. If this class has hit
                    // BULK_THRESHOLD consecutive refills without intervening
                    // frees, skip the magazine and go straight to the
                    // substrate. This avoids the per-free overflow flush
                    // cost on alloc-without-free streaks (the bulk pattern).
                    //
                    // The check is AFTER the magazine-hit test so the
                    // churn hot path (magazine hit) never reads
                    // alloc_streak — zero overhead.
                    let streak = self.tcache.alloc_streak[c];
                    if streak >= BULK_THRESHOLD {
                        let ptr = self.core.alloc(layout);
                        if !ptr.is_null() {
                            self.stamp_segment_owner(ptr);
                        }
                        return ptr;
                    }

                    // Magazine miss: batch-refill + stamp hoist (P4).
                    // We inline the refill+stamp here instead of calling
                    // `refill_class_stamped` because borrowing `self.core`
                    // and `self.tcache.slots[c]` separately avoids a
                    // double-mutable-borrow conflict on `self`.
                    let n = self.core.refill_class(
                        c,
                        super::tcache::REFILL_N,
                        &mut self.tcache.slots[c],
                    );
                    if n == 0 {
                        return core::ptr::null_mut(); // true OOM
                    }
                    // P4 stamp hoist: stamp each pulled block's source
                    // segment. The OPT-C cache (`last_stamped_segment`)
                    // short-circuits repeated stamps of the same segment to
                    // a Relaxed load + compare, so the typical single-segment
                    // refill stamps once and the rest are near-zero cache hits.
                    for i in 0..n {
                        let p = self.tcache.slots[c][i];
                        if !p.is_null() {
                            self.stamp_segment_owner(p);
                        }
                    }
                    // P7: bump refill streak. Each refill = REFILL_N allocs
                    // without a free for this class (magazine was empty).
                    self.tcache.alloc_streak[c] = self.tcache.alloc_streak[c].saturating_add(1);

                    // P7: if streak just reached BULK_THRESHOLD, flush the
                    // magazine. The refill pulled n blocks into the magazine;
                    // we're about to enter bulk mode on the NEXT alloc, so
                    // drain everything now to keep M2 invariants clean (no
                    // blocks stranded in the magazine during bulk mode, so
                    // live_count accurately reflects handed-out blocks only).
                    if self.tcache.alloc_streak[c] == BULK_THRESHOLD {
                        let new_cnt = n - 1;
                        if new_cnt > 0 {
                            self.core.flush_class(c, &self.tcache.slots[c][0..new_cnt]);
                        }
                        self.tcache.count[c] = 0;
                        return self.tcache.slots[c][new_cnt];
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
    #[inline(always)]
    fn dealloc_own_thread(&mut self, ptr: *mut u8, layout: Layout) {
        #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
        {
            use super::tcache::{FLUSH_N, TCACHE_CAP, TCACHE_KEY};
            use crate::alloc_core::size_classes::{SizeClasses, MIN_BLOCK, SMALL_ALIGN_MAX};
            let size = layout.size().max(MIN_BLOCK);
            let align = layout.align();
            if align <= SMALL_ALIGN_MAX {
                if let Some(c) = SizeClasses::class_for(size, align) {
                    let cnt = self.tcache.count[c] as usize;

                    // ── M2 double-free guard (P3) ────────────────────────
                    // Two-layer guard. word1 carries a per-heap "tcache key"
                    // marker while a block is in the magazine; flush leaves
                    // the key in place (we do not clear word1 on flush).
                    //
                    //   word1 != key  →  fast path, push.
                    //   word1 == key  →  SLOW path:
                    //     1. Bounded scan of the magazine. If `ptr` is
                    //        found here, this is an in-magazine double-free
                    //        (the block is still queued in our magazine
                    //        from a prior free) → no-op (M2 upheld).
                    //     2. Scan miss → either a `~2^-64` random user-data
                    //        false positive, OR a "flushed-then-double-freed"
                    //        case where `ptr` is on a BinTable free list AND
                    //        word1 still carries our stale key. Check the
                    //        BinTable bitmap (the authoritative M2 oracle
                    //        for flushed blocks): `bm.is_free(off)` true →
                    //        block is on a free list → no-op (M2 upheld).
                    //        bitmap-allocated → genuine false positive →
                    //        fall through to normal push.
                    //
                    // Without step 2 there was a real hole: a block that
                    // had been in the magazine and then half-flushed
                    // retained `word1 == key`; a subsequent double-free
                    // would hit the slow path, miss the magazine scan
                    // (block no longer there), and fall through to push —
                    // resulting in `ptr` being present BOTH in the
                    // magazine AND on the BinTable free list. The next
                    // refill would pull it off the BinTable while a
                    // separate alloc popped it from the magazine, issuing
                    // the same pointer twice. The bitmap check closes
                    // that window.
                    let key = TCACHE_KEY ^ (self.id as usize);
                    let word1_ptr = Node::offset(ptr, core::mem::size_of::<usize>()) as *mut usize;
                    let word1 = Node::read_usize(word1_ptr as *const usize);
                    if word1 == key {
                        // (1) bounded magazine scan
                        let n = self.tcache.count[c] as usize;
                        for i in 0..n {
                            if self.tcache.slots[c][i] == ptr {
                                return; // in-magazine DF — no streak change
                            }
                        }
                        // (2) bitmap check — authoritative for flushed blocks
                        let base = os::segment_base_of_ptr(ptr);
                        let off = (ptr as usize - base as usize) as u32;
                        let bm = SegmentMeta::new(base).alloc_bitmap();
                        if bm.is_free(off) {
                            return; // flushed-then-double-freed — no streak change
                        }
                        // Bitmap says alloc → block was carved or refilled
                        // for legitimate use and still carries an old key
                        // in word1 the user did not overwrite. Fall through
                        // to a normal push (re-stamps the key).
                    }

                    if cnt < TCACHE_CAP {
                        // Room in the magazine: stamp the key and push.
                        Node::write_usize(word1_ptr, key);
                        self.tcache.slots[c][cnt] = ptr;
                        self.tcache.count[c] = (cnt + 1) as u16;
                        return;
                    }
                    // ── P7 bulk-mode bypass on dealloc overflow ────────
                    // Magazine is full (cnt == TCACHE_CAP). If the alloc
                    // side is in bulk mode (streak >= BULK_THRESHOLD),
                    // skip the expensive half-flush + compact + push
                    // cycle and free directly via core.dealloc. Also
                    // flush the entire magazine: the blocks in it were
                    // pushed during the current free batch and should be
                    // returned to the substrate for clean live_count
                    // accounting (D1). Streak is NOT modified — it stays
                    // high so subsequent overflows also bypass, and the
                    // alloc side stays in bypass mode. Under churn,
                    // overflow is rare (alloc pops keep cnt < CAP), so
                    // this check adds zero overhead.
                    if self.tcache.alloc_streak[c] >= super::tcache::BULK_THRESHOLD {
                        // Flush the full magazine to the substrate.
                        self.core
                            .flush_class(c, &self.tcache.slots[c][0..TCACHE_CAP]);
                        self.tcache.count[c] = 0;
                        // Free this block directly.
                        self.core.dealloc(ptr, layout);
                        return;
                    }
                    // Normal overflow: half-flush, then push.
                    self.core.flush_class(c, &self.tcache.slots[c][0..FLUSH_N]);
                    // Compact: shift entries [FLUSH_N..CAP] down to [0..CAP-FLUSH_N].
                    let remaining = TCACHE_CAP - FLUSH_N;
                    for i in 0..remaining {
                        self.tcache.slots[c][i] = self.tcache.slots[c][i + FLUSH_N];
                    }
                    // Stamp the key and push.
                    Node::write_usize(word1_ptr, key);
                    self.tcache.slots[c][remaining] = ptr;
                    self.tcache.count[c] = (remaining + 1) as u16;
                    return;
                }
            }
        }
        // Large / non-small / non-fastbin: delegate to core.
        self.core.dealloc(ptr, layout);
    }

    /// Shrink/grow an allocation via alloc + copy + dealloc. Returns null on
    /// OOM (leaving the old allocation intact). Null `ptr` returns null.
    pub fn realloc(&mut self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
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
            // Own-thread free: route through magazine (under fastbin) or
            // directly to core.dealloc (without fastbin).
            self.dealloc_own_thread(ptr, layout);
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
            Self::push_large_deferred_free(owner_tf, base);
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
                meta.stamp_owner_thread_free(&self.thread_free as *const AtomicPtr<u8> as *const _);
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

    /// TEST-ONLY (P7): read the alloc streak counter for class `c`.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub fn dbg_alloc_streak(&self, c: usize) -> u8 {
        self.tcache.alloc_streak[c]
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
