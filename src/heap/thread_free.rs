//! [`ThreadFreeStack`] -- a lock-free Treiber stack for cross-thread free (M7).
//!
//! When a thread frees a block it does NOT own, it pushes the block onto the
//! owning heap's `ThreadFreeStack` via an atomic `compare_exchange` loop (the
//! Phase-7b linearization protocol, re-based onto the Phase 8/9 segment
//! substrate). The owner drains the stack in bulk on its next operation and
//! returns each block to its owning segment's `BinTable` (Phase 12.1: free
//! state lives in segments, not a heap-local array).
//!
//! ## Design (simplest sound)
//!
//! The Treiber stack is an `AtomicPtr<u8>` where each "node" is a freed block
//! whose first word stores the `next` pointer (intrusive -- same layout as
//! `FreeList`, using the `Node` seam). No heap allocation, no ABA counter
//! needed (the blocks are never reused while on the stack -- only the owner
//! pops, and it drains the whole stack atomically via `swap(null)`).
//!
//! ## Ordering justification
//!
//! - **Push (`compare_exchange`):** `Release` on success so the owner's
//!   `swap(Acquire)` drain sees the `next` pointer the pusher wrote. `Relaxed`
//!   on failure (retry loop; no side-effects on failure).
//! - **Drain (`swap`):** `Acquire` so the draining owner sees all `next`
//!   pointers written by pushers' `Release` stores.
//! - **`is_empty` (`load`):** `Relaxed` -- a momentary observation; the owner
//!   checks emptiness heuristically (e.g. skip drain if likely empty).

use core::ptr::NonNull;
use core::sync::atomic::{AtomicPtr, Ordering};

use crate::alloc_core::node::Node;

/// A lock-free Treiber stack of freed blocks for cross-thread free.
///
/// `Box`-allocated so its address is stable for the lifetime of the owning
/// `Heap` -- segment headers store a `*const AtomicPtr<u8>` pointing here.
/// The `AtomicPtr` is the stack head; each block's first word is the `next`
/// pointer (intrusive, via the `Node` seam).
pub(crate) struct ThreadFreeStack {
    /// The stable-address head of the Treiber stack. `Box`-allocated so
    /// segment headers can store a raw pointer to it that remains valid even
    /// if the `Heap` struct moves (e.g. inside `RefCell<Option<Heap>>`).
    head: Box<AtomicPtr<u8>>,
}

/// A borrow-only view over a `ThreadFreeStack`'s head atomic. Used by
/// [`HeapCore`](crate::registry::HeapCore), which owns its head as an
/// `Option<Box<AtomicPtr<u8>>>` (so it can lazily install it on the TLS
/// bind-slow path, outside the registry bootstrap). The view exposes the
/// drain method without taking ownership of the box.
///
/// This is the borrow-equivalent of [`ThreadFreeStack`]: same head atomic,
/// no allocation. The lifetime ties the view to the borrow of the underlying
/// `AtomicPtr`. Only constructed under `alloc-xthread` (by `HeapCore`).
#[cfg(feature = "alloc-xthread")]
pub(crate) struct ThreadFreeBorrow<'a> {
    head: &'a AtomicPtr<u8>,
}

impl ThreadFreeStack {
    /// Create a new empty thread-free stack.
    pub(crate) fn new() -> Self {
        Self {
            head: Box::new(AtomicPtr::new(core::ptr::null_mut())),
        }
    }

    /// A stable pointer to the `AtomicPtr<u8>` head. Stored in segment headers
    /// so cross-thread freers can find this stack from any thread.
    pub(crate) fn head_ptr(&self) -> *const AtomicPtr<u8> {
        &*self.head as *const AtomicPtr<u8>
    }

    /// Push a freed block onto the stack (CAS loop). Called by a NON-OWNER
    /// thread. The block's first word is overwritten with the current head
    /// pointer (intrusive node), then the head is CAS'd to point to this block.
    ///
    /// This is the Phase-7b linearization point re-based: the `compare_exchange`
    /// is the single atomic step that makes the push visible. Exactly one
    /// pusher per CAS attempt succeeds; losers retry.
    pub(crate) fn push(head_atomic: *const AtomicPtr<u8>, block: *mut u8) {
        let Some(block_nn) = NonNull::new(block) else {
            return;
        };
        // Dereference the `*const AtomicPtr<u8>` through the node seam (the
        // unsafe dereference lives in `Node::deref_atomic_ptr`, not here).
        // Heap-lifetime reasoning: a segment's `owner_thread_free` pointer is
        // set when the heap creates or adopts the segment, and is cleared or
        // the segment is released before the heap is dropped. A cross-thread
        // freer can only reach this pointer while it holds a live pointer into
        // the segment (the block being freed), which means the segment is
        // mapped and the header is readable. The owning heap's
        // `ThreadFreeStack` (and its `Box<AtomicPtr>`) is dropped only after
        // all segments have been released (in `Heap::drop` / TLS teardown),
        // so the pointer is valid for the duration of the push.
        let head: &AtomicPtr<u8> = Node::deref_atomic_ptr(head_atomic);
        loop {
            // Load the current head.
            // Relaxed: we will CAS with Release on success, which is the
            // synchronization point. The load here is just to get the expected
            // value for the CAS; if it's stale, the CAS fails and we retry.
            let old_head = head.load(Ordering::Relaxed);
            // Write `old_head` as the `next` pointer of this block (intrusive).
            Node::write_next(block_nn, old_head);
            // CAS: try to swing the head to this block.
            // Release on success: the owner's Acquire swap on drain will see
            // the `next` pointer we just wrote (the Release-Acquire pair
            // establishes happens-before from pusher to drainer).
            // Relaxed on failure: no side-effects, just retry.
            match head.compare_exchange_weak(
                old_head,
                block,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(_) => continue, // Retry: another pusher (or the owner's drain) changed the head.
            }
        }
    }

    /// Drain the entire stack atomically: swap the head to null and return the
    /// old head (the start of the chain). The caller walks the chain via
    /// `Node::read_next` to collect all freed blocks.
    ///
    /// Called ONLY by the owning thread (single consumer). The `Acquire`
    /// ordering ensures we see all `next` pointers written by pushers'
    /// `Release` CAS successes.
    pub(crate) fn drain(&self) -> *mut u8 {
        // Acquire: see all writes (Node::write_next) from pushers whose
        // Release CAS succeeded before this swap. This is the happens-before
        // chain: pusher's Release CAS -> owner's Acquire swap.
        self.head.swap(core::ptr::null_mut(), Ordering::Acquire)
    }

    /// Whether the stack is likely empty. Momentary observation -- another
    /// thread may push immediately after this returns `true`.
    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        // Relaxed: a heuristic check; no ordering guarantee needed. The
        // owner uses this to skip the drain path when the stack is likely
        // empty (an optimization, not a correctness requirement).
        self.head.load(Ordering::Relaxed).is_null()
    }
}

#[cfg(feature = "alloc-xthread")]
impl<'a> ThreadFreeBorrow<'a> {
    /// Construct a borrow view over an externally-owned `AtomicPtr<u8>` head.
    /// Used by [`HeapCore`](crate::registry::HeapCore) to drain its
    /// `Option<Box<AtomicPtr<u8>>>` without moving out of it. The caller
    /// guarantees `head` outlives the borrow (it does: the box is leaked on
    // thread death by the abandon guard).
    #[allow(dead_code)] // Used only by `registry::HeapCore`, gated on `alloc-global`.
    pub(crate) fn from_head(head: &'a AtomicPtr<u8>) -> Self {
        Self { head }
    }

    /// Drain the entire stack atomically: swap the head to null and return
    /// the old head (the chain start). Same ordering/contract as
    /// [`ThreadFreeStack::drain`].
    #[allow(dead_code)] // Used only by `registry::HeapCore`, gated on `alloc-global`.
    pub(crate) fn drain(&self) -> *mut u8 {
        self.head.swap(core::ptr::null_mut(), Ordering::Acquire)
    }
}
