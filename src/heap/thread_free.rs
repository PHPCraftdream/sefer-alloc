//! [`ThreadFreeStack`] -- a stable per-heap identity token for cross-thread
//! free routing (M7).
//!
//! ## History — from Treiber stack to identity token
//!
//! Originally this was an intrusive lock-free Treiber stack: a cross-thread
//! freer pushed the freed block onto the owning heap's stack (overwriting the
//! block's first word with the `next` pointer), and the owner drained it on its
//! next op. That intrusive design carried the §13 hazard (the drainer had only
//! a pointer, not the original `Layout`, so it re-derived the size class from
//! the segment's `page_map` — unreliable for the mixed-class pages a shared
//! bump cursor produces — and mis-linked the free list). It was superseded by
//! the per-segment [`RemoteFreeRing`](crate::alloc_core::remote_free_ring): a
//! non-intrusive MPSC queue of `(offset, class)` entries that never touches the
//! block's bytes and carries the class the freer holds.
//!
//! What remains here is the **stable, heap-unique address** the Treiber head
//! used to be: a `Box<AtomicPtr<u8>>` whose pointer a segment header stamps in
//! `owner_thread_free`. The owning [`Heap`](crate::heap::Heap) compares this
//! address to a segment's stamp to tell own-thread frees (route to the
//! `BinTable`) from cross-thread frees (push `(offset, class)` into the
//! segment's `RemoteFreeRing`). The `Box` allocation keeps that address
//! stable even if the `Heap` struct moves (e.g. inside
//! `RefCell<Option<Heap>>`), and it is leaked on `Heap::drop` under
//! `alloc-xthread` so a late cross-thread freer reading the stamp never
//! dereferences freed memory (abandonment-leak soundness).
//!
//! ## 0.3.x task #132 — the atomic's value is ALSO an A1 deferred-free stack
//!
//! Unified with [`registry::heap_core::HeapCore`](crate::registry::heap_core::HeapCore)'s
//! `thread_free` field (see its doc comment for the full mechanism): this
//! atomic's VALUE now doubles as the head of a per-heap Treiber stack of
//! Large/huge segment bases deferred for cross-thread reclaim (task A1). A
//! remote free of a `SegmentKind::Large` segment owned by this `Heap` pushes
//! the segment's base here (via
//! [`alloc_core::deferred_large::push_large_deferred_free`](crate::alloc_core::deferred_large::push_large_deferred_free))
//! instead of leaking it; the owner drains this stack lazily on its own
//! large-alloc slow path (via
//! [`alloc_core::deferred_large::drain_large_deferred_free`](crate::alloc_core::deferred_large::drain_large_deferred_free)).
//! No conflation with the identity role: the identity check compares the
//! `*const AtomicPtr<u8>` ADDRESS, never the pointee VALUE, so reusing the
//! value cell as a stack head does not corrupt the identity comparison.

use core::sync::atomic::AtomicPtr;

/// A stable, heap-unique identity token for cross-thread free routing.
///
/// `Box`-allocated so its address is stable for the lifetime of the owning
/// `Heap`; segment headers store a `*const AtomicPtr<u8>` pointing here in
/// `owner_thread_free`, and the owner uses pointer-identity to distinguish its
/// own segments from another heap's. See the module docs for why this is no
/// longer an intrusive stack.
pub(crate) struct ThreadFreeStack {
    /// The stable-address identity atomic. `Box`-allocated so segment headers
    /// can store a raw pointer to it that remains valid even if the `Heap`
    /// struct moves. `null` = "no A1 deferred-free entry queued" (the steady
    /// state — most workloads never hit this path); a non-null value is the
    /// head of this heap's A1 deferred-free Treiber stack (task #132 — see
    /// the module doc). Its ADDRESS is separately the identity token
    /// segment headers stamp in `owner_thread_free`.
    head: Box<AtomicPtr<u8>>,
}

impl ThreadFreeStack {
    /// Create a new identity token (a fresh stable address).
    pub(crate) fn new() -> Self {
        Self {
            head: Box::new(AtomicPtr::new(core::ptr::null_mut())),
        }
    }

    /// A stable pointer to the identity `AtomicPtr<u8>`. Stamped into segment
    /// headers (`owner_thread_free`) so a cross-thread freer can recognise this
    /// heap as the owner and route into the right segment's `RemoteFreeRing`.
    pub(crate) fn head_ptr(&self) -> *const AtomicPtr<u8> {
        &*self.head as *const AtomicPtr<u8>
    }

    /// A safe `&AtomicPtr<u8>` reference to THIS heap's own atomic — used by
    /// [`Heap::alloc`](crate::heap::Heap::alloc)'s large-request slow path to
    /// drain its own A1 deferred-free stack (task #132). No pointer seam
    /// needed here: the caller owns `self` directly, unlike a REMOTE freer,
    /// which only has the raw `owner_thread_free_at(base)` stamp and must go
    /// through the `node` seam's `atomic_ptr_ref`.
    pub(crate) fn head_atomic(&self) -> &AtomicPtr<u8> {
        &self.head
    }
}
