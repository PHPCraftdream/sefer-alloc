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
#[cfg(feature = "alloc-xthread")]
use core::sync::atomic::AtomicPtr;

use crate::alloc_core::{node::Node, AllocCore};
#[cfg(feature = "alloc-xthread")]
use crate::alloc_core::os;
#[cfg(feature = "alloc-xthread")]
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind, SegmentMeta, SEGMENT_MAGIC};
#[cfg(feature = "alloc-xthread")]
use crate::heap::thread_free::ThreadFreeStack;

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
    /// The cross-thread free-stack handle (`Box<AtomicPtr<u8>>`), installed
    /// lazily by [`install_thread_free`](Self::install_thread_free) on the
    /// TLS bind-slow path. `None` until then — a heap that has never served
    /// a cross-thread stamping has no TFS, and cross-thread frees to its
    /// segments are a safe no-op (matching the existing `owner_thread_free`
    /// null behaviour).
    ///
    /// Only present under `alloc-xthread` (the cross-thread feature).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) thread_free: Option<Box<AtomicPtr<u8>>>,
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
            thread_free: None,
        })
    }

    /// The slot index this heap is bound to. Read by `recycle`/`abandon` to
    /// locate the owning slot from a `*mut HeapCore`.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
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
        if self.thread_free.is_none() {
            self.thread_free = Some(Box::new(AtomicPtr::new(core::ptr::null_mut())));
        }
        // SAFETY: `thread_free` is `Some` now (just installed or was already).
        // The `Box`'s address is stable for the heap's lifetime; on thread
        // exit the abandon guard leaks the box (the abandonment-leak
        // discipline — late cross-thread pushes to a recycled heap must stay
        // sound), bounded by one Box per thread death.
        let head: &AtomicPtr<u8> = self.thread_free.as_ref().expect("installed above");
        head as *const AtomicPtr<u8>
    }

    /// The stable `*const AtomicPtr<u8>` head pointer of this heap's TFS, or
    /// null if [`install_thread_free`](Self::install_thread_free) was never
    /// called (no cross-thread stamping has happened → cross-thread frees to
    /// this heap's segments are a safe no-op). Used by the drain / routing
    /// paths on the owning thread.
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn thread_free_head(&self) -> *const AtomicPtr<u8> {
        match self.thread_free.as_ref() {
            Some(head) => head as *const AtomicPtr<u8>,
            None => core::ptr::null(),
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
    /// Own-thread path: delegates to [`AllocCore::alloc`]. Under
    /// `alloc-xthread`, drains the TFS first and stamps the segment header
    /// with this heap's TFS head so cross-thread freers can route to us.
    #[must_use]
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        #[cfg(feature = "alloc-xthread")]
        {
            self.drain_thread_free();
        }
        let ptr = self.core.alloc(layout);
        #[cfg(feature = "alloc-xthread")]
        if !ptr.is_null() {
            self.stamp_owner(ptr);
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
        let hdr = SegmentHeader::read_at(base);
        if hdr.magic != SEGMENT_MAGIC {
            // Foreign pointer: not a sefer segment. The substrate's `dealloc`
            // also no-ops on foreign pointers, but we short-circuit here to
            // avoid even the table-contains scan (a hot-path micro-opt that
            // also avoids reading our table for a pointer that cannot be in
            // it).
            return;
        }
        let our_head = self.thread_free_head();
        if hdr.owner_thread_free.is_null() || hdr.owner_thread_free == our_head {
            // Own-thread free (or unstamped — treat as own; matches the
            // existing `Heap::dealloc_small` fallback). The substrate
            // applies the M2 double-free guard and the foreign-pointer
            // table-contains check.
            self.core.dealloc(ptr, layout);
            return;
        }
        // Cross-thread free: the segment is stamped with another heap's TFS
        // head. Large segments are skipped cross-thread (bounded leak,
        // matching `Heap::dealloc_any_thread` — the large segment stays
        // mapped until the owning heap recycles).
        if hdr.kind == SegmentKind::Large {
            return;
        }
        ThreadFreeStack::push(hdr.owner_thread_free, ptr);
    }

    /// Drain this heap's TFS: swap the head to null, walk the chain, and
    /// route each drained block to its owning segment's `BinTable` via
    /// [`AllocCore::dealloc_small_by_segment`] (the class is derived from
    /// the page map; the drainer has only the pointer). No-op if the TFS
    /// handle was never installed.
    #[cfg(feature = "alloc-xthread")]
    fn drain_thread_free(&mut self) {
        use core::ptr::NonNull;
        use crate::heap::thread_free::ThreadFreeBorrow;
        let Some(head_box) = self.thread_free.as_ref() else {
            return;
        };
        // Borrow the box's AtomicPtr as a ThreadFreeBorrow view (the view
        // adds only the drain method; it holds no extra state and aliases
        // the box's atomic).
        let tfs = ThreadFreeBorrow::from_head(head_box.as_ref());
        let mut cur = tfs.drain();
        while !cur.is_null() {
            let cur_nn = match NonNull::new(cur) {
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

    /// Stamp a segment's header with our TFS head pointer so cross-thread
    /// freers can find this heap. Called on the alloc path after a successful
    /// allocation. Idempotent: only stamps if the header's
    /// `owner_thread_free` is currently null.
    #[cfg(feature = "alloc-xthread")]
    fn stamp_owner(&mut self, ptr: *mut u8) {
        let Some(head_box) = self.thread_free.as_ref() else {
            // No TFS installed yet (own-thread-only path). Nothing to stamp;
            // cross-thread frees to this segment will be a safe no-op until
            // the TFS is installed on a later bind/alloc.
            return;
        };
        let base = os::segment_base_of(ptr as usize) as *mut u8;
        let mut meta = SegmentMeta::new(base);
        let mut hdr = meta.header();
        if hdr.owner_thread_free.is_null() {
            hdr.owner_thread_free = head_box as *const AtomicPtr<u8> as *const _;
            meta.write_header(hdr);
        }
    }
}
