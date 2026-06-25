//! [`FreeList`] -- a per-class intrusive singly-linked free list. The `next`
//! pointer of each free block is stored inside the block itself (the first
//! word), via the [`Node`](crate::alloc_core::node::Node) seam. Pure safe
//! composition over `Node::write_next` / `Node::read_next`.
//!
//! This is the hot-path data structure of the Phase 9 per-thread heap: `pop`
//! is one pointer read, `push` is one pointer write -- no lock, no atomic, no
//! allocation.

use core::ptr::NonNull;

use crate::alloc_core::node::Node;

/// An intrusive singly-linked free list of freed blocks.
///
/// Each block is `>= NODE_SIZE` bytes (enforced by the size-class table).
/// The first word of a free block stores the `next` pointer (or null for the
/// tail). The list is LIFO (stack): `push` prepends, `pop` removes the head.
#[derive(Clone, Copy)]
pub(crate) struct FreeList {
    head: *mut u8,
}

impl FreeList {
    /// An empty free list.
    pub(crate) const EMPTY: Self = Self {
        head: core::ptr::null_mut(),
    };

    /// Pop the head block from the list. Returns `None` if empty.
    ///
    /// **The hot path:** one pointer read via the node seam. No lock, no
    /// atomic.
    #[inline]
    pub(crate) fn pop(&mut self) -> Option<*mut u8> {
        let head = NonNull::new(self.head)?;
        let next = Node::read_next(head);
        self.head = next;
        Some(head.as_ptr())
    }

    /// Push a block onto the head of the list.
    ///
    /// **The hot dealloc path:** one pointer write. No lock, no atomic.
    #[inline]
    pub(crate) fn push(&mut self, ptr: *mut u8) {
        if let Some(nn) = NonNull::new(ptr) {
            Node::write_next(nn, self.head);
            self.head = ptr;
        }
    }

    /// Whether the list is empty.
    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.head.is_null()
    }
}
