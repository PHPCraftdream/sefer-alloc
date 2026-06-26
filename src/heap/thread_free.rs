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
//! segment's `RemoteFreeRing`). The atomic's *value* is no longer used as a
//! stack — only its *address* matters, as a per-heap identity. The `Box`
//! allocation keeps that address stable even if the `Heap` struct moves (e.g.
//! inside `RefCell<Option<Heap>>`), and it is leaked on `Heap::drop` under
//! `alloc-xthread` so a late cross-thread freer reading the stamp never
//! dereferences freed memory (abandonment-leak soundness).

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
    /// struct moves. Its value stays null for the heap's lifetime (it is an
    /// identity token, not a stack head — cross-thread frees go to the
    /// per-segment `RemoteFreeRing`).
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
}
