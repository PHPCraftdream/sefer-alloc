//! [`HeapCore`] — the thin heap value that lives inside a registry slot.
//!
//! This is the type the Phase 12.3 raw-pointer TLS caches as
//! `*mut HeapCore`. Per §2.0 of `MALLOC_PLAN_PHASE12-13.md` the heap is now
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
//! entry points the raw-pointer TLS binding hands to the malloc face. They
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
//! face (`SeferMalloc`) is rewired to route through `HeapCore` via the
//! registry. The eventual collapse of `Heap` into `HeapCore` is later work.

use core::alloc::Layout;
use core::sync::atomic::Ordering;
#[cfg(feature = "alloc-xthread")]
use core::sync::atomic::AtomicPtr;

use crate::alloc_core::{node::Node, AllocCore};
#[cfg(feature = "alloc-global")]
use crate::alloc_core::os;
#[cfg(feature = "alloc-xthread")]
use crate::alloc_core::segment_header::{
    SegmentHeader, SegmentKind, SEGMENT_MAGIC,
};
#[cfg(any(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::segment_header::SegmentMeta;
#[cfg(feature = "alloc-global")]
use crate::alloc_core::segment_header::pack_owner;

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
    /// `Box::new` on the TLS bind path would recurse into `SeferMalloc::alloc`
    /// → `current_for_alloc` → `bind_slow` → `install_thread_free` → `Box::new`
    /// → infinite recursion → stack overflow. The inline field breaks that
    /// cycle: the head's address is `&self.thread_free`, stable for the
    /// slot's lifetime (the `HeapCore` lives in the `'static` registry slot
    /// array), so segment headers can store a raw `*const AtomicPtr<u8>` to
    /// it.
    ///
    /// Only present under `alloc-xthread` (the cross-thread feature).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) thread_free: AtomicPtr<u8>,
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

    /// Phase 12.4 adoption: drain an adopted segment's `ThreadFreeStack`
    /// (if its header stamps one) into its `BinTable`s. Cross-thread frees
    /// that arrived while the segment was abandoned are processed here.
    /// No-op if the segment has no TFS head stamped (own-thread-only heap).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn drain_segment_tfs(&mut self, base: *mut u8) {
        // Field-specific read (task #33 root-cause fix): read ONLY
        // `owner_thread_free`, not the whole mutable header. This path runs
        // in the adopter context after winning the ABANDONED→LIVE CAS, which
        // happens-before-synchronises with the prior owner's abandonment, but
        // a concurrent Remote `dealloc_routing` may still be mid-flight on
        // the same header; reading a single field avoids racing the Owner's
        // `bump` writes (the §11 root cause).
        let owner_tf = SegmentHeader::owner_thread_free_at(base);
        if owner_tf.is_null() {
            return;
        }
        // Drain via the segment's stamped TFS head. We borrow the TFS view
        // through the node seam (the head pointer is stable — the abandoning
        // heap leaked its `Box<AtomicPtr>` per the abandonment discipline).
        use crate::heap::thread_free::ThreadFreeBorrow;
        let head_atomic = Node::deref_atomic_ptr(owner_tf);
        let tfs = ThreadFreeBorrow::from_head(head_atomic);
        let mut cur = tfs.drain();
        while !cur.is_null() {
            let cur_nn = match core::ptr::NonNull::new(cur) {
                Some(nn) => nn,
                None => break,
            };
            // Read `next` BEFORE we free `cur` (dealloc overwrites the first
            // word as the free-list node pointer).
            let next = Node::read_next(cur_nn);
            self.core.dealloc_small_by_segment(cur);
            cur = next;
        }
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
    pub(crate) fn thread_free_head(&self) -> *const AtomicPtr<u8> {
        &self.thread_free as *const AtomicPtr<u8>
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
    /// drains the inline TFS first: late cross-thread frees that arrived while
    /// this slot was between owners (released by a dying thread, not yet
    /// reclaimed) are routed into this heap's segments via
    /// [`AllocCore::dealloc_small_by_segment`]. This is the shard-reuse
    /// discipline — the new owner drains the freed shard's remote-free queue
    /// on its first op, exactly as `ShardedRegion` 7b models it. The drain is
    /// the ONLY cross-thread touch on the alloc path; everything else is
    /// single-writer (this thread owns the slot, ergo its segments).
    #[must_use]
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
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
        let ptr = self.core.alloc(layout);
        if !ptr.is_null() {
            self.stamp_segment_owner(ptr);
        }
        ptr
    }

    /// Allocate `layout.size()` bytes of **zeroed** memory.
    #[must_use]
    pub fn alloc_zeroed(&mut self, layout: Layout) -> *mut u8 {
        let ptr = self.alloc(layout);
        if !ptr.is_null() {
            Node::zero(ptr, layout.size().max(crate::alloc_core::size_classes::MIN_BLOCK));
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
            self.core.dealloc(ptr, layout);
        }
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
    fn dealloc_routing(&mut self, ptr: *mut u8, layout: Layout) {
        let base = os::segment_base_of(ptr as usize) as *mut u8;
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
            self.core.dealloc(ptr, layout);
            return;
        }
        if SegmentHeader::kind_at(base) == SegmentKind::Large {
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
        let class_idx = match crate::alloc_core::size_classes::SizeClasses::class_for(
            size,
            layout.align(),
        ) {
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
    fn stamp_segment_owner(&mut self, ptr: *mut u8) {
        use crate::alloc_core::segment_header::{unpack_owner_id, OWNER_STATE_LIVE};
        let base = os::segment_base_of(ptr as usize) as *mut u8;
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
            let cur_head = crate::alloc_core::segment_header::SegmentHeader::owner_thread_free_at(base);
            if cur_head.is_null() {
                meta.stamp_owner_thread_free(
                    &self.thread_free as *const AtomicPtr<u8> as *const _,
                );
            }
        }
    }
}
