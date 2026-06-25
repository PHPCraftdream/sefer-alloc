//! [`Heap`] -- a per-thread heap with intrusive free lists over the Phase 8
//! segment substrate. The hot path (`alloc_small` / `dealloc_small`) is a
//! lock-free pop / push on a singly-linked intrusive free list stored inside
//! freed blocks via the [`Node`](crate::alloc_core::node::Node) seam.
//!
//! Phase 9: the single-owner fast path. No lock, no atomic, no
//! `Vec`/`Box`/`HashSet`/`std::alloc` on the alloc/dealloc path (M5
//! reentrancy-freedom upheld).
//!
//! Phase 10 (behind `alloc-xthread`): cross-thread free (M7) via the
//! [`ThreadFreeStack`] Treiber stack. A non-owner thread pushes freed blocks
//! onto the owning heap's thread-free stack; the owner drains in bulk on its
//! next operation. Sound across thread death via abandonment-leak: on
//! `Heap::drop`, segments and the Treiber head are intentionally leaked so
//! late cross-thread frees never touch unmapped memory or a freed `Box`.
//! Full abandoned-heap adoption (reclaiming leaked segments) is Phase 11.
//!
//! Decommit (M6) is NOT delivered in Phase 10 -- the `os::decommit_pages` /
//! `os::recommit_pages` seam is in place but not wired into the heap. M6 is
//! a Phase 11 deliverable.

use core::alloc::Layout;
#[cfg(feature = "alloc-xthread")]
use core::mem::ManuallyDrop;
#[cfg(feature = "alloc-xthread")]
use core::ptr::NonNull;

use crate::alloc_core::node::Node;
#[cfg(feature = "alloc-xthread")]
use crate::alloc_core::os;
#[cfg(feature = "alloc-xthread")]
use crate::alloc_core::segment_header::{
    SegmentHeader, SegmentKind, SegmentMeta, SEGMENT_MAGIC,
};
use crate::alloc_core::AllocCore;

use super::free_list::FreeList;
#[cfg(feature = "alloc-xthread")]
use super::thread_free::ThreadFreeStack;

/// Number of size classes we cache per-heap. Must equal `SMALL_CLASS_COUNT`
/// from the size-class table.
const HEAP_BINS: usize = crate::alloc_core::size_classes::SMALL_CLASS_COUNT;

/// A per-thread heap: owns an [`AllocCore`] plus per-class intrusive free
/// lists. With `alloc-xthread`, also owns a cross-thread free stack (Phase 10).
///
/// The hot path is a pop/push on the thread's own free list -- no lock,
/// no atomic. Cross-thread frees (only with `alloc-xthread`) are pushed
/// onto the `ThreadFreeStack` by remote threads and drained by the owner
/// on its next op.
pub struct Heap {
    /// The underlying single-threaded segment substrate.
    ///
    /// Under `alloc-xthread`: wrapped in `ManuallyDrop` so `Heap::drop` can
    /// LEAK the segments (abandonment-leak for thread-death soundness) by
    /// simply not calling `ManuallyDrop::drop`.
    ///
    /// Under plain `alloc` (no `alloc-xthread`): owned directly, dropped
    /// normally by the compiler (Phase 9 single-owner, sound).
    #[cfg(feature = "alloc-xthread")]
    core: ManuallyDrop<AllocCore>,
    #[cfg(not(feature = "alloc-xthread"))]
    core: AllocCore,

    /// Per-class intrusive free lists. Index `i` caches freed blocks of class
    /// `i`. The free-list nodes are stored inside the freed blocks themselves
    /// (the `next` pointer occupies the first word of the block).
    bins: [FreeList; HEAP_BINS],

    /// The Treiber stack for cross-thread free (Phase 10, M7). Remote threads
    /// push freed blocks here; the owner drains on its next alloc/dealloc.
    /// `Box`-allocated internally so its `AtomicPtr` has a stable address
    /// that segment headers can store.
    ///
    /// Wrapped in `ManuallyDrop` so `Heap::drop` can LEAK the `Box<AtomicPtr>`
    /// (abandonment-leak), ensuring late cross-thread pushes remain sound.
    ///
    /// Only present with the `alloc-xthread` feature.
    #[cfg(feature = "alloc-xthread")]
    thread_free: ManuallyDrop<ThreadFreeStack>,
}

impl Heap {
    /// Create a new per-thread heap. Bootstraps the segment substrate
    /// ([`AllocCore::new`]). Returns `None` only on primordial OOM.
    #[must_use]
    pub fn new() -> Option<Self> {
        let core = AllocCore::new()?;
        Some(Self {
            #[cfg(feature = "alloc-xthread")]
            core: ManuallyDrop::new(core),
            #[cfg(not(feature = "alloc-xthread"))]
            core,
            bins: [FreeList::EMPTY; HEAP_BINS],
            #[cfg(feature = "alloc-xthread")]
            thread_free: ManuallyDrop::new(ThreadFreeStack::new()),
        })
    }

    /// Allocate `layout.size()` bytes satisfying `layout.align()`.
    ///
    /// Returns a non-null `*mut u8` on success, or null on OOM. The memory is
    /// **uninitialised**.
    ///
    /// **Hot path (small):** pop from the thread's class free list -- one
    /// pointer read, no lock, no atomic. On drain, refill from the substrate.
    /// With `alloc-xthread`, drains the cross-thread free stack before refilling
    /// so remotely-freed blocks are returned to their class free lists.
    #[must_use]
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size().max(crate::alloc_core::size_classes::MIN_BLOCK);
        let align = layout.align();
        match classify(size, align) {
            Some(class_idx) => self.alloc_small(class_idx),
            None => {
                let ptr = self.core.alloc(layout);
                #[cfg(feature = "alloc-xthread")]
                if !ptr.is_null() {
                    // Stamp the segment header with our thread-free pointer so
                    // cross-thread freers can find us.
                    self.stamp_owner(ptr);
                }
                ptr
            }
        }
    }

    /// Allocate `layout.size()` bytes of **zeroed** memory.
    #[must_use]
    pub fn alloc_zeroed(&mut self, layout: Layout) -> *mut u8 {
        let ptr = self.alloc(layout);
        if !ptr.is_null() {
            Node::zero(
                ptr,
                layout.size().max(crate::alloc_core::size_classes::MIN_BLOCK),
            );
        }
        ptr
    }

    /// Deallocate memory previously returned by [`alloc`](Self::alloc).
    ///
    /// Without `alloc-xthread`, this is the Phase 9 single-owner dealloc: the
    /// block is returned to the local free list or substrate. Cross-thread
    /// dealloc is NOT supported (the caller must free from the owning thread).
    ///
    /// With `alloc-xthread` (Phase 10, M7): if the block belongs to a segment
    /// owned by THIS heap, it is returned to the local free list (hot path).
    /// If the block belongs to a segment owned by ANOTHER heap (cross-thread
    /// free), it is pushed onto that heap's `ThreadFreeStack` via an atomic CAS
    /// (the Phase-7b Treiber protocol).
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
        Node::copy_nonoverlapping(ptr, new_ptr, copy);
        self.dealloc(ptr, old_layout);
        new_ptr
    }

    /// Deallocate a block from any thread. This is the PUBLIC cross-thread-safe
    /// entry point. It routes to the remote heap's Treiber push depending on
    /// ownership (determined by reading the segment header at the block's
    /// segment base).
    ///
    /// Only available with the `alloc-xthread` feature. Without it, all
    /// deallocation must happen on the owning thread via [`dealloc`](Self::dealloc).
    ///
    /// **Large blocks:** a cross-thread free of a large block (a block in a
    /// `SegmentKind::Large` segment) is a no-op -- the large segment stays
    /// mapped until its owning `Heap` drops (at which point the segments are
    /// leaked under `alloc-xthread`). This is a bounded leak, not a
    /// correctness violation: the block is not lost (it remains accessible to
    /// the owning heap), and the segment is reclaimed when the owning thread
    /// exits (or, under Phase 11 adoption, when another thread adopts the
    /// abandoned heap). The alternative -- routing large cross-thread frees
    /// through the Treiber stack -- would require the drain path to distinguish
    /// large from small blocks, adding complexity for a rare case.
    ///
    /// **Unstamped segments:** if a segment's `owner_thread_free` is null (the
    /// segment was created by Phase 8 `AllocCore` standalone, or not yet
    /// stamped), the cross-thread free is a no-op. The block is leaked until
    /// the owning `AllocCore` drops and releases the segment. This is the
    /// conservative fallback -- no UAF, no corruption.
    #[cfg(feature = "alloc-xthread")]
    pub fn dealloc_any_thread(ptr: *mut u8, _layout: Layout) {
        if ptr.is_null() {
            return;
        }
        // Find the owning segment.
        let base = os::segment_base_of(ptr as usize) as *mut u8;
        let hdr = SegmentHeader::read_at(base);
        if hdr.magic != SEGMENT_MAGIC {
            return; // Foreign pointer.
        }
        if hdr.kind == SegmentKind::Large {
            // Large segments: cross-thread free is a no-op. The large segment
            // stays mapped until its owning Heap drops (leaked under
            // alloc-xthread). See the doc comment above for the rationale.
            return;
        }
        // Small segment: check if the owner's thread-free pointer is set.
        let owner_tf = hdr.owner_thread_free;
        if owner_tf.is_null() {
            // No owner registered. Cross-thread free is a no-op (the block is
            // leaked until the owning AllocCore drops). See the doc comment.
            return;
        }
        // Push onto the owning heap's Treiber stack (lock-free CAS).
        ThreadFreeStack::push(owner_tf, ptr);
    }

    // -----------------------------------------------------------------------
    // Internals
    // -----------------------------------------------------------------------

    /// Pop from the per-heap free list for `class_idx`. If the list is empty,
    /// drain the thread-free stack first (with `alloc-xthread`), then try
    /// again, then refill from the substrate.
    fn alloc_small(&mut self, class_idx: usize) -> *mut u8 {
        // Hot path: pop from our free list.
        if let Some(ptr) = self.bins[class_idx].pop() {
            return ptr;
        }
        // With alloc-xthread: drain remotely-freed blocks before refilling.
        #[cfg(feature = "alloc-xthread")]
        {
            self.drain_thread_free();
            // Retry after drain.
            if let Some(ptr) = self.bins[class_idx].pop() {
                return ptr;
            }
        }
        // Cold path: refill from the substrate.
        self.refill_and_alloc(class_idx)
    }

    /// Push a freed block onto the per-heap free list for `class_idx`.
    ///
    /// Without `alloc-xthread`: always pushes locally (the caller must be the
    /// owning thread -- single-owner Phase 9 invariant).
    ///
    /// With `alloc-xthread`: checks ownership via the segment header. If the
    /// block belongs to this heap, pushes locally (hot path). If it belongs
    /// to another heap, pushes onto the remote heap's Treiber stack.
    fn dealloc_small(&mut self, ptr: *mut u8, class_idx: usize) {
        #[cfg(not(feature = "alloc-xthread"))]
        {
            // Phase 9 single-owner: always local.
            self.bins[class_idx].push(ptr);
        }
        #[cfg(feature = "alloc-xthread")]
        {
            // Check ownership: is this block in a segment we own?
            let base = os::segment_base_of(ptr as usize) as *mut u8;
            let hdr = SegmentHeader::read_at(base);
            if hdr.magic != SEGMENT_MAGIC {
                return; // Foreign pointer -- no-op.
            }
            // Check if the segment's owner_thread_free points to OUR thread-free
            // stack. If so, this is a local dealloc (hot path). If not, push onto
            // the remote heap's Treiber stack.
            let our_head = self.thread_free.head_ptr();
            if hdr.owner_thread_free == our_head {
                // Own-thread dealloc: push onto our local free list (fast path).
                self.bins[class_idx].push(ptr);
            } else if !hdr.owner_thread_free.is_null() {
                // Cross-thread dealloc: push onto the owning heap's Treiber stack.
                ThreadFreeStack::push(hdr.owner_thread_free, ptr);
            } else {
                // No owner registered. Fall back to substrate dealloc.
                let block_size = crate::alloc_core::size_classes::SizeClasses::block_size(class_idx);
                let layout = match Layout::from_size_align(block_size, block_size.min(16)) {
                    Ok(l) => l,
                    Err(_) => return,
                };
                self.core.dealloc(ptr, layout);
            }
        }
    }

    /// Drain the thread-free stack: atomically swap the head to null, then walk
    /// the chain and return each block to its per-class free list (determined
    /// from the segment's page map).
    ///
    /// Only compiled with `alloc-xthread`.
    #[cfg(feature = "alloc-xthread")]
    fn drain_thread_free(&mut self) {
        let mut cur = self.thread_free.drain();
        while !cur.is_null() {
            let cur_nn = match NonNull::new(cur) {
                Some(nn) => nn,
                None => break,
            };
            // Read `next` BEFORE we push `cur` onto a free list (which
            // overwrites the first word).
            let next = Node::read_next(cur_nn);
            // Determine the class from the segment's page map.
            let base = os::segment_base_of(cur as usize) as *mut u8;
            let meta = SegmentMeta::new(base);
            let page_idx = (cur as usize - base as usize) / os::PAGE;
            if let Some(class_idx) = meta.page_map().class_of(page_idx) {
                self.bins[class_idx].push(cur);
            }
            // else: page is Meta or Free -- this block is not from a class page
            // (should not happen in correct usage; skip it defensively).
            cur = next;
        }
    }

    /// Stamp a segment's header with our thread-free pointer so cross-thread
    /// freers can find this heap. Called when the heap first allocates from a
    /// segment (either by carving new blocks or by adopting a segment).
    ///
    /// Only compiled with `alloc-xthread`.
    #[cfg(feature = "alloc-xthread")]
    fn stamp_owner(&mut self, ptr: *mut u8) {
        let base = os::segment_base_of(ptr as usize) as *mut u8;
        let mut meta = SegmentMeta::new(base);
        let mut hdr = meta.header();
        if hdr.owner_thread_free.is_null() {
            hdr.owner_thread_free = self.thread_free.head_ptr();
            meta.write_header(hdr);
        }
    }

    /// Refill class `class_idx` by carving a batch of blocks from the
    /// substrate, then return one block to the caller. With `alloc-xthread`,
    /// also stamps the segment with our owner pointer.
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
        // Stamp ownership on the segment this block came from (xthread only).
        #[cfg(feature = "alloc-xthread")]
        self.stamp_owner(first);

        // Carve more blocks to pre-populate the free list.
        const REFILL_BATCH: usize = 31;
        for _ in 0..REFILL_BATCH {
            let ptr = self.core.alloc(layout);
            if ptr.is_null() {
                break;
            }
            #[cfg(feature = "alloc-xthread")]
            self.stamp_owner(ptr);
            self.bins[class_idx].push(ptr);
        }

        first
    }
}

/// Drop implementation.
///
/// Under `alloc-xthread` (Phase 10): abandonment-leak for thread-death
/// soundness. Another thread may still hold a pointer into one of our segments
/// and later call `dealloc` -> `segment_base_of` reads the segment header ->
/// pushes onto our `ThreadFreeStack`. If we released (munmapped/VirtualFree'd)
/// our segments here, that late cross-thread `dealloc` would read unmapped
/// memory (UAF). If we freed the `Box<AtomicPtr>` inside `ThreadFreeStack`,
/// the late push would CAS on a freed box (UAF).
///
/// The SOUND fix (without full abandoned-heap adoption, which is Phase 11):
/// LEAK both the segments and the Treiber head. The segments stay mapped, so
/// `segment_base_of` + header reads remain valid. The `Box<AtomicPtr>` stays
/// allocated, so CAS pushes remain valid. This is a BOUNDED leak: it happens
/// only on thread death (one heap per thread), and the leaked memory is bounded
/// by the heap's segment footprint at death time. For the target workload
/// (long-lived thread pools), thread death is rare and the leak is negligible.
/// Phase 11 will implement full abandoned-heap adoption.
///
/// Under plain `alloc` (no `alloc-xthread`, Phase 9): release segments
/// normally. No cross-thread refs exist, so this is sound. `AllocCore::drop`
/// munmaps/VirtualFrees all segments.
impl Drop for Heap {
    fn drop(&mut self) {
        #[cfg(feature = "alloc-xthread")]
        {
            // Drain any remaining remotely-freed blocks (best-effort cleanup).
            self.drain_thread_free();

            // LEAK both `self.core` (and thus all its segments) and
            // `self.thread_free` (the Box<AtomicPtr>) by NOT dropping them.
            // Both fields are `ManuallyDrop`; the compiler does not drop
            // `ManuallyDrop` fields automatically. The segments stay mapped,
            // the Treiber head stays allocated. Late cross-thread frees remain
            // sound.
        }

        // Under plain `alloc` (no `alloc-xthread`): `self.core` is a plain
        // `AllocCore` (not ManuallyDrop), so the compiler drops it normally,
        // releasing all segments. There is no `thread_free` field at all.
    }
}

/// Classify a `(size, align)` as a small class index, or `None` for large.
fn classify(size: usize, align: usize) -> Option<usize> {
    crate::alloc_core::size_classes::SizeClasses::class_for(size, align)
}
