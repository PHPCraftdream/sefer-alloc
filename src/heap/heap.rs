//! [`Heap`] -- a per-thread heap with intrusive free lists over the Phase 8
//! segment substrate. The hot path (`alloc_small` / `dealloc_small`) is a
//! lock-free pop / push on a singly-linked intrusive free list stored inside
//! freed blocks via the [`Node`](super::alloc_core::node::Node) seam.
//!
//! This is Phase 9 of `MALLOC_PLAN.md` -- the single-owner fast path. No lock,
//! no atomic, no `Vec`/`Box`/`HashSet`/`std::alloc` on the alloc/dealloc path
//! (M5 reentrancy-freedom upheld). Cross-thread free is Phase 10.

use core::alloc::Layout;

use crate::alloc_core::AllocCore;

use super::free_list::FreeList;

/// Number of size classes we cache per-heap. Must equal `SMALL_CLASS_COUNT`
/// from the size-class table.
const HEAP_BINS: usize = crate::alloc_core::size_classes::SMALL_CLASS_COUNT;

/// A per-thread heap: owns an [`AllocCore`] plus per-class intrusive free
/// lists. The hot path is a pop/push on the thread's own free list -- no lock,
/// no atomic. When a class drains, the heap refills by carving blocks from the
/// substrate via `AllocCore::alloc`.
///
/// `Heap` is neither `Send` nor `Sync`: it owns an [`AllocCore`], which is
/// single-owner and deliberately not `Send`. This is correct for the TLS
/// binding — `std::thread_local!` does not require `Send`; each thread
/// constructs and owns its own `Heap` in place. Cross-thread sharing/transfer
/// is out of scope until Phase 10 (cross-thread free).
pub struct Heap {
    /// The underlying single-threaded segment substrate.
    core: AllocCore,
    /// Per-class intrusive free lists. Index `i` caches freed blocks of class
    /// `i`. The free-list nodes are stored inside the freed blocks themselves
    /// (the `next` pointer occupies the first word of the block).
    bins: [FreeList; HEAP_BINS],
}

impl Heap {
    /// Create a new per-thread heap. Bootstraps the segment substrate
    /// ([`AllocCore::new`]). Returns `None` only on primordial OOM.
    #[must_use]
    pub fn new() -> Option<Self> {
        let core = AllocCore::new()?;
        Some(Self {
            core,
            bins: [FreeList::EMPTY; HEAP_BINS],
        })
    }

    /// Allocate `layout.size()` bytes satisfying `layout.align()`.
    ///
    /// Returns a non-null `*mut u8` on success, or null on OOM. The memory is
    /// **uninitialised**.
    ///
    /// **Hot path (small):** pop from the thread's class free list -- one
    /// pointer read, no lock, no atomic. On drain, refill from the substrate.
    #[must_use]
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size().max(crate::alloc_core::size_classes::MIN_BLOCK);
        let align = layout.align();
        match classify(size, align) {
            Some(class_idx) => self.alloc_small(class_idx),
            None => self.core.alloc(layout),
        }
    }

    /// Allocate `layout.size()` bytes of **zeroed** memory.
    #[must_use]
    pub fn alloc_zeroed(&mut self, layout: Layout) -> *mut u8 {
        let ptr = self.alloc(layout);
        if !ptr.is_null() {
            crate::alloc_core::node::Node::zero(
                ptr,
                layout.size().max(crate::alloc_core::size_classes::MIN_BLOCK),
            );
        }
        ptr
    }

    /// Deallocate memory previously returned by [`alloc`](Self::alloc).
    ///
    /// **Own-thread only** (Phase 9). Cross-thread free is Phase 10.
    ///
    /// **Hot path (small):** push onto the thread's class free list -- one
    /// pointer write, no lock, no atomic.
    pub fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        let size = layout.size().max(crate::alloc_core::size_classes::MIN_BLOCK);
        let align = layout.align();
        match classify(size, align) {
            Some(class_idx) => self.dealloc_small(ptr, class_idx),
            None => self.core.dealloc(ptr, layout),
        }
    }

    /// Shrink/grow an allocation in place or by alloc + copy + dealloc.
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
        crate::alloc_core::node::Node::copy_nonoverlapping(ptr, new_ptr, copy);
        self.dealloc(ptr, old_layout);
        new_ptr
    }

    // -----------------------------------------------------------------------
    // Internals
    // -----------------------------------------------------------------------

    /// Pop from the per-heap free list for `class_idx`. If the list is empty,
    /// refill from the substrate (carve a batch of blocks).
    fn alloc_small(&mut self, class_idx: usize) -> *mut u8 {
        // Hot path: pop from our free list.
        if let Some(ptr) = self.bins[class_idx].pop() {
            return ptr;
        }
        // Cold path: refill from the substrate.
        self.refill_and_alloc(class_idx)
    }

    /// Push a freed block onto the per-heap free list for `class_idx`.
    fn dealloc_small(&mut self, ptr: *mut u8, class_idx: usize) {
        self.bins[class_idx].push(ptr);
    }

    /// Refill class `class_idx` by carving a batch of blocks from the
    /// substrate, then return one block to the caller. The batch size is chosen
    /// to amortize the substrate call: we carve up to `REFILL_BATCH` blocks
    /// (or until the substrate returns null, meaning its current segment is
    /// full and it reserved a new one -- we let it handle that).
    fn refill_and_alloc(&mut self, class_idx: usize) -> *mut u8 {
        let block_size = crate::alloc_core::size_classes::SizeClasses::block_size(class_idx);
        let layout = match Layout::from_size_align(block_size, block_size.min(16)) {
            Ok(l) => l,
            Err(_) => return core::ptr::null_mut(),
        };

        // Carve one block for the caller.
        let first = self.core.alloc(layout);
        if first.is_null() {
            return core::ptr::null_mut();
        }

        // Carve more blocks to pre-populate the free list.
        const REFILL_BATCH: usize = 31;
        for _ in 0..REFILL_BATCH {
            let ptr = self.core.alloc(layout);
            if ptr.is_null() {
                break;
            }
            self.bins[class_idx].push(ptr);
        }

        first
    }
}

/// Classify a `(size, align)` as a small class index, or `None` for large.
fn classify(size: usize, align: usize) -> Option<usize> {
    crate::alloc_core::size_classes::SizeClasses::class_for(size, align)
}
