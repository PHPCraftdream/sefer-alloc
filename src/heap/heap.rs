//! [`Heap`] -- a per-thread heap over the Phase 8 segment substrate.
//!
//! ## Phase 12.1 — segment-centric free state (refactor)
//!
//! The small free-list state lives in each segment's [`BinTable`] (the "each
//! page owns its free list" rule, mimalloc-style), NOT in a heap-local `bins`
//! array. A segment is self-describing for free state: a freed block returns
//! to its owning segment's `BinTable[class]`, found by pure address arithmetic
//! (`segment_base_of(ptr)`). This is the foundation for Phase 12.2+ (heap
//! registry, adoption, decommit): adoption becomes O(1) because the free state
//! travels *with* the segments, not in a thread-local array that would need
//! merging.
//!
//! The heap is now thin: it holds the segment substrate ([`AllocCore`], which
//! owns the segments and does all per-segment free-list arithmetic) plus the
//! cross-thread [`ThreadFreeStack`] (Phase 10, `alloc-xthread`). The own-thread
//! small alloc/dealloc paths delegate to `core.alloc` / `core.dealloc`, which
//! route every freed block to its own segment's `BinTable` and, on a miss, scan
//! all owned segments for a non-empty class free list before carving fresh
//! blocks — preserving cross-segment reuse (the Phase 9 behaviour).
//!
//! Phase 10 (behind `alloc-xthread`): cross-thread free (M7). A non-owner
//! thread recognises the owning heap by the `owner_thread_free` stamp (a stable
//! [`ThreadFreeStack`] identity address) and pushes the freed block's
//! `(offset, class)` into the owning SEGMENT's
//! [`RemoteFreeRing`](crate::alloc_core::remote_free_ring) — a non-intrusive
//! MPSC queue that never touches the block's bytes and carries the size CLASS
//! the freer holds. The owner reclaims those entries LAZILY on a free-list miss
//! (`AllocCore::find_segment_with_free` → `reclaim_offset`), which trusts the
//! carried class and never the owner's `page_map` (unreliable for mixed-class
//! pages — the §13 hazard the old intrusive-Treiber drain carried). Sound
//! across thread death via abandonment-leak: on `Heap::drop`, segments and the
//! identity `Box` are intentionally leaked so late cross-thread frees never
//! touch unmapped memory or a freed `Box`. Full abandoned-heap adoption is
//! Phase 12.2+.
//!
//! Decommit (M6) is NOT delivered here -- the `os::decommit_pages` /
//! `os::recommit_pages` seam is in place but not wired into the heap. M6 is a
//! Phase 12.5 deliverable.

use core::alloc::Layout;
#[cfg(feature = "alloc-xthread")]
use core::mem::ManuallyDrop;

#[cfg(feature = "alloc-xthread")]
use crate::alloc_core::os;
#[cfg(feature = "alloc-xthread")]
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind, SegmentMeta, SEGMENT_MAGIC};
use crate::alloc_core::AllocCore;

#[cfg(feature = "alloc-xthread")]
use super::thread_free::ThreadFreeStack;

/// A per-thread heap: owns an [`AllocCore`] (the segment substrate). With
/// `alloc-xthread`, also owns a cross-thread free stack (Phase 10).
///
/// **Phase 12.1:** the heap holds NO free-list state of its own. Free-list
/// state lives in each segment's [`BinTable`]; own-thread alloc/dealloc
/// delegate to `AllocCore`, which routes every block to its owning segment via
/// `segment_base_of(ptr)` and scans owned segments for reusable free blocks on
/// a miss. The heap layer's only extra state is the cross-thread
/// [`ThreadFreeStack`] (Phase 10).
pub struct Heap {
    /// The underlying single-threaded segment substrate. It owns the segments
    /// and performs all per-segment `BinTable` arithmetic; the heap layer is a
    /// thin wrapper that adds cross-thread routing on top.
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

    /// The stable per-heap identity token for cross-thread free (Phase 10, M7).
    /// Its `Box`-allocated `AtomicPtr` address is stamped into segment headers
    /// (`owner_thread_free`); a remote freer compares it to recognise this heap
    /// as the owner and route into the right segment's `RemoteFreeRing`. The
    /// atomic's value is unused (it is an identity, not a stack — cross-thread
    /// frees go to the per-segment ring, NOT here).
    ///
    /// Wrapped in `ManuallyDrop` so `Heap::drop` can LEAK the `Box<AtomicPtr>`
    /// (abandonment-leak), ensuring a late cross-thread ownership check on a
    /// leaked segment never reads a freed box.
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
            #[cfg(feature = "alloc-xthread")]
            thread_free: ManuallyDrop::new(ThreadFreeStack::new()),
        })
    }

    /// Allocate `layout.size()` bytes satisfying `layout.align()`.
    ///
    /// Returns a non-null `*mut u8` on success, or null on OOM. The memory is
    /// **uninitialised**.
    ///
    /// **Hot path (small):** pops from the current segment's `BinTable` via
    /// `core.alloc`; on a miss, scans owned segments for a non-empty class
    /// free list, then refills from the substrate (carving a batch whose
    /// blocks each return to their own segment's `BinTable`). With
    /// `alloc-xthread`, drains the cross-thread free stack before the scan so
    /// remotely-freed blocks are reused first.
    #[must_use]
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let size = layout
            .size()
            .max(crate::alloc_core::size_classes::MIN_BLOCK);
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
            crate::alloc_core::node::Node::zero(
                ptr,
                layout
                    .size()
                    .max(crate::alloc_core::size_classes::MIN_BLOCK),
            );
        }
        ptr
    }

    /// Deallocate memory previously returned by [`alloc`](Self::alloc).
    ///
    /// Without `alloc-xthread`, this is the Phase 9 single-owner dealloc: the
    /// block is returned to its owning segment's `BinTable` (Phase 12.1) via
    /// `core.dealloc` (which uses `segment_base_of(ptr)` + the M2 double-free
    /// guard). Cross-thread dealloc is NOT supported (the caller must free
    /// from the owning thread).
    ///
    /// With `alloc-xthread` (Phase 10, M7): if the block belongs to a segment
    /// owned by THIS heap, it is returned to that segment's `BinTable` via
    /// `core.dealloc` (hot path). If the block belongs to a segment owned by
    /// ANOTHER heap (cross-thread free), its `(offset, class)` is pushed into
    /// that segment's `RemoteFreeRing` (the block's bytes are NOT touched), and
    /// the owner reclaims it via `reclaim_offset` on its next free-list miss.
    pub fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        let size = layout
            .size()
            .max(crate::alloc_core::size_classes::MIN_BLOCK);
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

    /// Deallocate a block from any thread. This is the PUBLIC cross-thread-safe
    /// entry point. It routes the block's `(offset, class)` into the owning
    /// segment's `RemoteFreeRing` (ownership determined by reading the segment
    /// header at the block's segment base).
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
    /// exits (or, under Phase 12.4 adoption, when another thread adopts the
    /// abandoned heap). The alternative -- routing large cross-thread frees
    /// through the ring -- would require the reclaim path to distinguish
    /// large from small blocks, adding complexity for a rare case.
    ///
    /// **Unstamped segments:** if a segment's `owner_thread_free` is null (the
    /// segment was created by Phase 8 `AllocCore` standalone, or not yet
    /// stamped), the cross-thread free is a no-op. The block is leaked until
    /// the owning `AllocCore` drops and releases the segment. This is the
    /// conservative fallback -- no UAF, no corruption.
    #[cfg(feature = "alloc-xthread")]
    pub fn dealloc_any_thread(ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        // Find the owning segment. Field-specific reads (task #33 / §11): read
        // ONLY `magic` / `kind` / `owner_thread_free`, never a full-struct
        // `read_at` that races the owner's `bump` writes.
        let base = os::segment_base_of_ptr(ptr);
        if SegmentHeader::magic_at(base) != SEGMENT_MAGIC {
            return; // Foreign pointer.
        }
        if SegmentHeader::kind_at(base) == SegmentKind::Large {
            // Large segments: cross-thread free is a no-op. The large segment
            // stays mapped until its owning Heap drops (leaked under
            // alloc-xthread). See the doc comment above for the rationale.
            return;
        }
        // Small segment: only route if the owner registered a thread-free head
        // (an own-thread free would arrive via `Heap::dealloc`, not here).
        if SegmentHeader::owner_thread_free_at(base).is_null() {
            // No owner registered. Cross-thread free is a no-op (the block is
            // leaked until the owning AllocCore drops). See the doc comment.
            return;
        }
        // Push (offset, class) to the owning segment's RemoteFreeRing — the
        // block's bytes are NOT touched. The freer HAS the `Layout`, so it
        // carries the size CLASS in the ring entry; the owner reclaims via
        // `reclaim_offset`, which trusts the carried class rather than the
        // unreliable `page_map` of a mixed-class page (§13). A Large `layout`
        // on a small segment is a contract violation — drop (no-op).
        let size = layout
            .size()
            .max(crate::alloc_core::size_classes::MIN_BLOCK);
        let class_idx =
            match crate::alloc_core::size_classes::SizeClasses::class_for(size, layout.align()) {
                Some(c) => c as u32,
                None => return,
            };
        let off = (ptr as usize - base as usize) as u32;
        let packed = crate::alloc_core::remote_free_ring::pack_entry(off, class_idx);
        let ring = SegmentMeta::new(base).remote_ring();
        let _ = ring.push(packed);
    }

    // -----------------------------------------------------------------------
    // Internals
    // -----------------------------------------------------------------------

    /// Allocate a small block of `class_idx`. Drains the cross-thread free
    /// stack first (with `alloc-xthread`) so remotely-freed blocks are reused,
    /// then delegates to `core.alloc` — which pops from the current segment,
    /// scans owned segments for a reusable free block, and only then carves
    /// (carving a refill batch into each block's own segment via
    /// `segment_base_of`, fixing defect A). Phase 12.1: free state lives in
    /// per-segment `BinTable`s, reachable across all owned segments by the
    /// substrate's cross-segment scan (fixing defect B).
    fn alloc_small(&mut self, class_idx: usize) -> *mut u8 {
        // With alloc-xthread: cross-thread-freed blocks are reclaimed LAZILY by
        // the substrate's `find_segment_with_free`, which drains each owned
        // segment's `RemoteFreeRing` into its `BinTable` (via `reclaim_offset`)
        // on a free-list miss. No eager drain here — this matches the proven
        // `HeapCore::alloc` discipline (the eager drain was the §13-prone
        // intrusive-TFS path; the ring carries the class, page_map does not).
        // Delegate to the substrate. The substrate does the pop + cross-segment
        // scan + carve-with-refill; it routes every block to its own segment's
        // BinTable via `segment_base_of(ptr)` (defect A fix). No heap-layer
        // free-list state — defect B fix (no captured `bins` array that would
        // bypass non-current segments).
        let block_size = crate::alloc_core::size_classes::SizeClasses::block_size(class_idx);
        let layout = match Layout::from_size_align(block_size, block_size.min(16)) {
            Ok(l) => l,
            Err(_) => return core::ptr::null_mut(),
        };
        let ptr = self.core.alloc(layout);
        #[cfg(feature = "alloc-xthread")]
        if !ptr.is_null() {
            // Stamp ownership on the segment this block came from so
            // cross-thread freers can find us.
            self.stamp_owner(ptr);
        }
        ptr
    }

    /// Push a freed small block onto its owning segment's class free list, or
    /// — under `alloc-xthread` — into the owning segment's `RemoteFreeRing` if
    /// the block belongs to another heap.
    ///
    /// Without `alloc-xthread`: delegates to `core.dealloc`, which routes via
    /// `segment_base_of(ptr)` to the owning segment's `BinTable` (the M2
    /// double-free guard is applied). Single-owner Phase 9 invariant: the
    /// caller IS the owning thread.
    ///
    /// With `alloc-xthread`: checks ownership via the segment header's
    /// `owner_thread_free` stamp. If the block belongs to this heap (or the
    /// segment is unstamped), delegates to `core.dealloc` (hot path, which
    /// derives the class from the `Layout`). If it belongs to another heap, the
    /// freer — who HAS the class — pushes `(offset, class)` into that segment's
    /// `RemoteFreeRing` (block bytes untouched); the owner later reclaims it via
    /// `reclaim_offset`, which trusts the carried class, never `page_map` (§13).
    fn dealloc_small(&mut self, ptr: *mut u8, class_idx: usize) {
        #[cfg(not(feature = "alloc-xthread"))]
        {
            // Phase 9 single-owner: route to the owning segment's BinTable via
            // the substrate (which uses segment_base_of + double-free guard).
            let block_size = crate::alloc_core::size_classes::SizeClasses::block_size(class_idx);
            let layout = match Layout::from_size_align(block_size, block_size.min(16)) {
                Ok(l) => l,
                Err(_) => return,
            };
            self.core.dealloc(ptr, layout);
        }
        #[cfg(feature = "alloc-xthread")]
        {
            // Check ownership: is this block in a segment we own?
            //
            // Field-specific reads (task #33 / §11): read ONLY `magic` and
            // `owner_thread_free` via their `offset_of!` offsets, never a
            // full-struct `read_at` that would race the owner's `bump` writes.
            let base = os::segment_base_of_ptr(ptr);
            if SegmentHeader::magic_at(base) != SEGMENT_MAGIC {
                return; // Foreign pointer -- no-op.
            }
            // Check if the segment's owner_thread_free points to OUR head. If so
            // (or if unstamped), this is an own-thread dealloc (hot path) — route
            // through the substrate (segment_base_of + Layout class + double-free
            // guard). Otherwise the block belongs to ANOTHER heap: route it
            // cross-thread via that segment's RemoteFreeRing, carrying the size
            // CLASS in the ring entry (§13: the owner's `page_map` is unreliable
            // for the mixed-class pages a shared bump cursor produces, so the
            // freer — who HAS the class — must carry it; the owner reclaims via
            // `reclaim_offset`, which trusts the carried class, never page_map).
            let our_head = self.thread_free.head_ptr();
            let owner_tf = SegmentHeader::owner_thread_free_at(base);
            if owner_tf.is_null() || owner_tf == our_head {
                let block_size =
                    crate::alloc_core::size_classes::SizeClasses::block_size(class_idx);
                let layout = match Layout::from_size_align(block_size, block_size.min(16)) {
                    Ok(l) => l,
                    Err(_) => return,
                };
                self.core.dealloc(ptr, layout);
            } else {
                // Cross-thread dealloc: push (offset, class) to the owning
                // segment's RemoteFreeRing (the block's bytes are NOT touched).
                let off = (ptr as usize - base as usize) as u32;
                let packed = crate::alloc_core::remote_free_ring::pack_entry(off, class_idx as u32);
                let ring = SegmentMeta::new(base).remote_ring();
                let _ = ring.push(packed);
            }
        }
    }

    /// Stamp a segment's header with our thread-free pointer so cross-thread
    /// freers can find this heap. Called when the heap first allocates from a
    /// segment (either by carving new blocks or by adopting a segment).
    ///
    /// Only compiled with `alloc-xthread`.
    #[cfg(feature = "alloc-xthread")]
    fn stamp_owner(&mut self, ptr: *mut u8) {
        let base = os::segment_base_of_ptr(ptr);
        let mut meta = SegmentMeta::new(base);
        // Field-specific read + write (task #33 / §11): a full-struct
        // `header()` + `write_header(hdr)` here rewrites EVERY header field
        // (magic/kind/bump/...) and races a concurrent cross-thread freer's
        // field reads of `magic`/`kind`/`owner_thread_free` (in `dealloc_small`
        // / `dealloc_any_thread`) — the §11 data race. Stamp ONLY the
        // `owner_thread_free` word via its `offset_of!` offset, disjoint from
        // every field a Remote reads/the owner's `bump` writes. Idempotent:
        // only when currently null (mirrors `HeapCore::stamp_segment_owner`).
        if SegmentHeader::owner_thread_free_at(base).is_null() {
            meta.stamp_owner_thread_free(self.thread_free.head_ptr());
        }
    }
}

/// Drop implementation.
///
/// Under `alloc-xthread` (Phase 10): abandonment-leak for thread-death
/// soundness. Another thread may still hold a pointer into one of our segments
/// and later call `dealloc` -> `segment_base_of` reads the segment header ->
/// reads our `owner_thread_free` identity stamp -> pushes into that segment's
/// `RemoteFreeRing`. If we released (munmapped/VirtualFree'd) our segments
/// here, that late cross-thread `dealloc` would read unmapped memory (UAF). If
/// we freed the `Box<AtomicPtr>` identity token, the late ownership check would
/// read a freed box (UAF).
///
/// The SOUND fix (without full abandoned-heap adoption, which is Phase 12.2+):
/// LEAK both the segments and the identity `Box`. The segments stay mapped, so
/// `segment_base_of` + header reads (and the segment's ring) remain valid. The
/// `Box<AtomicPtr>` stays allocated, so the identity comparison remains valid.
/// This is a BOUNDED leak: it happens
/// only on thread death (one heap per thread), and the leaked memory is bounded
/// by the heap's segment footprint at death time. For the target workload
/// (long-lived thread pools), thread death is rare and the leak is negligible.
/// Phase 12.2+ will implement full abandoned-heap adoption.
///
/// Under plain `alloc` (no `alloc-xthread`, Phase 9): release segments
/// normally. No cross-thread refs exist, so this is sound. `AllocCore::drop`
/// munmaps/VirtualFrees all segments.
impl Drop for Heap {
    fn drop(&mut self) {
        #[cfg(feature = "alloc-xthread")]
        {
            // Cross-thread-freed blocks now live in each segment's
            // `RemoteFreeRing` (not an intrusive stack); any still-in-flight at
            // drop are leaked alongside the segments below (bounded, sound — a
            // late freer pushing into a leaked segment's ring is harmless: the
            // ring stays mapped, nobody drains it, the block is leaked).
            //
            // LEAK both `self.core` (and thus all its segments) and
            // `self.thread_free` (the Box<AtomicPtr>) by NOT dropping them.
            // Both fields are `ManuallyDrop`; the compiler does not drop
            // `ManuallyDrop` fields automatically. The segments stay mapped,
            // the identity `Box` stays allocated. Late cross-thread frees
            // remain sound.
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
