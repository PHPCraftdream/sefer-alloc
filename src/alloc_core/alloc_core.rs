//! [`AllocCore`] — the single-threaded allocator over the self-hosted segment
//! substrate (Phase 8, `alloc-core` feature).
//!
//! This is the **Cartographer** of the segment substrate: all placement logic
//! (which size class, which page, free-list pop/push, large/huge routing) is
//! **pure safe integer arithmetic** over segment-relative offsets and
//! size-class indices. Every raw memory touch is delegated to the [`node`](node)
//! seam; every OS reservation to the [`os`](os) seam. `AllocCore` itself
//! contains NO `unsafe` and NO `Vec`/`Box`/`HashSet`/`std::alloc` — the alloc
//! path is therefore **reentrancy-free (M5)**: it cannot recurse into the
//! global allocator because it allocates no metadata through it.
//!
//! ## API
//!
//! - [`AllocCore::new`] — bootstrap the primordial segment (the ONLY place
//!   that hand-carves self-hosted metadata; see [`bootstrap`]).
//! - [`alloc`](AllocCore::alloc) / [`dealloc`](AllocCore::dealloc) /
//!   [`realloc`](AllocCore::realloc) / [`alloc_zeroed`](AllocCore::alloc_zeroed)
//!   — the single-threaded allocator entry points. `dealloc`/`realloc` are
//!   `unsafe` per the `GlobalAlloc` contract (the caller must pass a valid
//!   prior pointer/layout); they never panic and never recurse.
//!
//! ## Single-threaded
//!
//! Phase 8 is single-threaded (correctness before concurrency — §5 P8).
//! Per-thread heaps + lock-free cross-thread free are Phase 9/10. `AllocCore`
//! is `Send` (it owns its segments, which are `Send`) but NOT `Sync`.

use core::alloc::Layout;
use core::ptr::NonNull;

use super::bootstrap;
use super::node::{Node, NODE_SIZE};
use super::os::{self, Segment, SEGMENT};
use super::segment_header::{
    align_up, BinTable, FREE_LIST_NULL, Layout as SegLayout, PageMap, SegmentHeader, SegmentKind,
    SegmentMeta,
};
use super::segment_table::SegmentTable;
use super::size_classes::{AllocKind, SizeClasses};

/// A single-threaded allocator over the self-hosted segment substrate.
///
/// Owns its segments (the primordial + any additionally-reserved small or
/// large/huge segments). The registry of live segments lives in the
/// primordial segment's payload (self-hosted) — there is NO `Vec<Segment>`:
/// `AllocCore::drop` walks the registry and frees every reservation through
/// the [`os`] seam.
pub struct AllocCore {
    /// The primordial segment registry (self-hosted in segment 0's payload).
    table: SegmentTable,
    /// Metadata view of the "current" small segment — the one whose bump
    /// cursor and free lists new small allocations draw from. When it fills,
    /// [`alloc_small`] reserves a fresh small segment and switches to it.
    ///
    /// [`alloc_small`]: Self::alloc_small
    small_cur: *mut u8,
}

impl AllocCore {
    /// Bootstrap the allocator: reserve the primordial segment and hand-carve
    /// its self-hosted metadata. See [`bootstrap`].
    ///
    /// Returns `None` only if the OS refuses the primordial reservation
    /// (OOM at startup).
    #[must_use]
    pub fn new() -> Option<Self> {
        let prim = bootstrap::primordial()?;
        let primordial_base = prim.segment.as_ptr();
        // The primordial segment hosts the registry AND serves as the first
        // small segment (its remaining payload is free for small allocs).
        let small_cur = primordial_base;
        // We take ownership of the registry; the primordial Segment handle is
        // forgotten — its memory is freed by walking the registry in `drop`
        // (the registry records the reservation pointers, so we do not need
        // the Rust `Segment` handle to free it).
        core::mem::forget(prim.segment);
        Some(Self {
            table: prim.table,
            small_cur,
        })
    }

    /// Allocate `layout.size()` bytes satisfying `layout.align()`.
    ///
    /// Returns a non-null `*mut u8` on success, or null on OOM. The memory is
    /// **uninitialised** (matching `GlobalAlloc::alloc`); see
    /// [`alloc_zeroed`](Self::alloc_zeroed) for zeroed memory.
    ///
    /// Zero-size layouts are not supported (they violate the `GlobalAlloc`
    /// contract; we round up to `MIN_BLOCK` and serve normally).
    #[must_use]
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size().max(super::size_classes::MIN_BLOCK);
        let align = layout.align();
        match Self::classify(size, align) {
            AllocKind::Small { class_idx } => self.alloc_small(class_idx),
            AllocKind::Large => self.alloc_large(size, align),
        }
    }

    /// Allocate `layout.size()` bytes of **zeroed** memory.
    #[must_use]
    pub fn alloc_zeroed(&mut self, layout: Layout) -> *mut u8 {
        let ptr = self.alloc(layout);
        if !ptr.is_null() {
            Node::zero(ptr, layout.size().max(super::size_classes::MIN_BLOCK));
        }
        ptr
    }

    /// Deallocate memory previously returned by [`alloc`](Self::alloc) (or
    /// `alloc_zeroed`/`realloc`).
    ///
    /// This entry point is **safe**: a foreign pointer (not one of ours) or a
    /// double-free is a **no-op** (M2 — never UB, never corrupts the
    /// allocator), matching the defensive contract the Phase 11 `GlobalAlloc`
    /// face will require. A well-behaved caller passes a valid prior
    /// allocation of `layout`; the safety here is defence-in-depth, not a
    /// licence to free garbage.
    ///
    /// **Phase 13.3 — arithmetic own-thread free.** The hot path is now pure
    /// arithmetic + (at most) one field-specific header byte read, NOT a
    /// full-struct `SegmentHeader::read_at`. Specifically:
    ///   - `segment_base_of(ptr)` — one mask (already the case).
    ///   - `self.table.contains_base(base)` — the foreign-pointer guard (this
    ///     is the load-bearing defence-in-depth check, NOT the `magic` word:
    ///     a foreign pointer's computed base is simply not in our registry,
    ///     so we never touch its bytes).
    ///   - `SegmentHeader::kind_at(base)` — ONE byte field read (via
    ///     `offset_of!`) to distinguish Large from Small/Primordial. This is
    ///     the minimum read necessary: Large blocks are freed by marking the
    ///     segment (no class free list), Small/Primordial go to the BinTable;
    ///     without distinguishing them we'd misroute. `kind` is written once
    ///     at segment init and immutable thereafter, so this byte read cannot
    ///     race an owner write on the disjoint `bump` field (the §11
    ///     root-cause analysis).
    ///   - the size class is derived from the caller-supplied `Layout` via
    ///     [`Self::classify`] — pure arithmetic, no `page_map` lookup (§13:
    ///     `page_map` is unreliable for mixed-class pages, and own-thread
    ///     free HAS the `Layout`, so deriving from it is both cheaper AND
    ///     correct).
    ///
    /// The `SEGMENT_MAGIC` full-struct sanity check is intentionally absent
    /// here: it lives ONLY on the defensive cross-thread routing path
    /// ([`HeapCore::dealloc_routing`] under `alloc-xthread`), where a foreign
    /// pointer could in principle resolve to a registered-but-not-ours base.
    /// On the trusted own-thread path, `contains_base` is the sole guard and
    /// the `Layout` is authoritative for the class — a full header load would
    /// be a dependent load on the free critical path with no correctness gain.
    #[inline]
    pub fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        let base = os::segment_base_of(ptr as usize) as *mut u8;
        // Foreign-pointer check: if the computed segment base is NOT one of our
        // registered segments, this pointer is not one of ours — no-op (do not
        // touch foreign memory, do not even read a header that may be unmapped).
        if !self.table.contains_base(base) {
            return;
        }
        // Field-specific `kind` read (Phase 13.3): a single byte at its
        // `offset_of!` offset, NOT a full-struct `read_at`. Distinguishes
        // Large (free = mark segment) from Small/Primordial (free = push to
        // BinTable). `kind` is immutable after init, so this byte read is
        // race-free against the owner's disjoint `bump` writes.
        match SegmentHeader::kind_at(base) {
            SegmentKind::Large => {
                // Large/huge: mark the segment as freed (zero the magic) so
                // `Drop` knows its reservation should be released. We do NOT
                // release eagerly here — that would unmap the header before
                // `Drop` can read it to discover the reservation info. (M6 —
                // eager decommit / OS return — is a Phase 10 deliverable; for
                // Phase 8 correctness, all segment freeing happens at `Drop`.)
                //
                // The full header read here is on the cold Large path (one
                // allocation per segment, rare), so the dependent-load cost
                // does not matter; correctness requires reading + rewriting
                // the header to flag the segment for `Drop`.
                let mut stale = SegmentHeader::read_at(base);
                stale.magic = 0;
                Node::write_struct(base as *mut SegmentHeader, stale);
            }
            SegmentKind::Small | SegmentKind::Primordial => {
                // Derive the class from the caller's `Layout` (pure
                // arithmetic via `SIZE2CLASS`) — NOT from `page_map`. §13 of
                // RACE_DRAIN_RECLAIM.md: `page_map` records only the FIRST
                // class to touch a page, so it returns the wrong class for
                // any later block of a different class in the same page. The
                // own-thread freer HAS the original `Layout`, so classifying
                // from it is both cheaper (no page_map load) AND correct.
                let size = layout.size().max(super::size_classes::MIN_BLOCK);
                let align = layout.align();
                let kind = Self::classify(size, align);
                let class_idx = match kind {
                    AllocKind::Small { class_idx } => class_idx,
                    // Layout mismatch: the original allocation was small but
                    // the dealloc layout classifies as large. This is a
                    // contract violation; no-op (do not corrupt).
                    AllocKind::Large => return,
                };
                self.dealloc_small(base, ptr, class_idx);
            }
        }
    }

    /// Deallocate a small block whose size class is NOT known from a `Layout`
    /// (e.g. a block drained from the cross-thread free stack — the drainer
    /// only has the pointer, not the original layout). The class is derived
    /// from the owning segment's page map (the Phase 8 page-dedication rule:
    /// the page holding the block knows its class).
    ///
    /// **Phase 12.1:** this routes the block to its own segment's `BinTable`
    /// (via `segment_base_of(ptr)`), preserving the segment-centric free state.
    /// Used by the heap layer's cross-thread drain path.
    ///
    /// Safe: a foreign pointer or a block not in a class page is a no-op
    /// (matches the defensive `dealloc` contract). Applies the M2 double-free
    /// guard.
    #[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
    pub(crate) fn dealloc_small_by_segment(&mut self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }
        let base = os::segment_base_of(ptr as usize) as *mut u8;
        if !self.table.contains_base(base) {
            return; // Foreign pointer.
        }
        let hdr = SegmentHeader::read_at(base);
        if hdr.magic != super::segment_header::SEGMENT_MAGIC {
            return;
        }
        if !matches!(hdr.kind, SegmentKind::Small | SegmentKind::Primordial) {
            return; // Large/other: not a small-block free.
        }
        // Derive the class from the page map (the page-dedication rule).
        let meta = SegmentMeta::new(base);
        let page_idx = (ptr as usize - base as usize) / super::os::PAGE;
        let class_idx = match meta.page_map().class_of(page_idx) {
            Some(c) => c,
            None => return, // Page is Meta/Free: not a class block; skip.
        };
        self.dealloc_small(base, ptr, class_idx);
    }

    /// Reclaim a cross-thread-freed block identified by its **segment-relative
    /// offset** back into its owning segment's `BinTable`. This is the
    /// non-intrusive reclaim path (Variant-2): the block's offset arrived via
    /// the segment's `RemoteFreeRing` (the block's own bytes were never touched
    /// by the cross-thread freer), so we turn the offset back into a pointer
    /// and route it through the same `dealloc_small` path as an own-thread free.
    ///
    /// **Self-less** (an associated function, not `&mut self`): it touches ONLY
    /// segment metadata reachable from `base` via `SegmentMeta` (header, page
    /// map, bin table) — never the `AllocCore` registry. This lets the
    /// `find_segment_with_free` drain call it while iterating `&self.table`
    /// without an aliasing conflict, and keeps the single-consumer reclaim
    /// uniform with the own-thread path. The caller MUST be the segment's sole
    /// `BinTable` writer (the slot's owner) — the same invariant `dealloc_small`
    /// relies on.
    ///
    /// **Class is carried in the ring entry, NOT derived from `page_map`.** The
    /// segment has ONE bump cursor shared by all size classes, so a page can
    /// host blocks of several classes (the page-dedication rule records only
    /// the FIRST class to touch a page). Deriving the class from `page_map`
    /// therefore returns the wrong class for any later block of a different
    /// class in the same page, and reclaim would link the free-list `next` at a
    /// mis-aligned address, corrupting a neighbour (the §13 root cause). The
    /// cross-thread freer has the original `Layout`, so it packs
    /// `class_idx = classify(layout)` into the high bits of the ring entry;
    /// here we unpack it and use it directly.
    ///
    /// `packed` layout: `off = packed & OFF_MASK` (low 22 bits, since
    /// `SEGMENT = 1 << 22` so every offset is `< 2^22`), `class_idx = packed >>
    /// OFF_BITS` (high bits; `SMALL_CLASS_COUNT = 40 ≪ 2^10`, so it fits).
    ///
    /// Safe: a foreign segment (magic mismatch), a large segment, or an offset
    /// that is not `block_size`-aligned is a no-op (defence-in-depth). Applies
    /// the M2 double-free guard.
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn reclaim_offset(base: *mut u8, packed: u32) {
        // Unpack the offset and the class the cross-thread freer stamped.
        let (off, class_idx) = super::remote_free_ring::unpack_entry(packed);
        let off = off as usize;
        let class_idx = class_idx as usize;
        let ptr = Node::deref(base, off);
        // Field-specific reads: this runs on the Owner's alloc path
        // (drain_thread_free / find_segment_with_free), concurrent with a
        // Remote's `dealloc_routing` field reads. A full-struct
        // `SegmentHeader::read_at` here would race them; reading individual
        // fields via their offsets touches bytes disjoint from any racing
        // writer, so there is no data race.
        if SegmentHeader::magic_at(base) != super::segment_header::SEGMENT_MAGIC {
            return;
        }
        if !matches!(SegmentHeader::kind_at(base), SegmentKind::Small | SegmentKind::Primordial) {
            return;
        }
        // Sanity: the offset must be a whole number of `block_size` units. carve
        // aligns the bump to `block_size`, so a real block offset is always a
        // multiple of its class's block_size. A mis-aligned offset would write
        // the free-list `next` into the middle of a block — the §13 corruption.
        // This never fires for a correctly-packed entry; it is defence-in-depth
        // against a garbled ring value (no abort — just skip, matching the
        // defensive `dealloc_small_by_segment` contract).
        let bs = SizeClasses::block_size(class_idx) as u32;
        if !(off as u32).is_multiple_of(bs) {
            return;
        }
        let meta = SegmentMeta::new(base);
        // Inline of `dealloc_small` (self-less): double-free guard + push to
        // BinTable. We cannot call the `&mut self` method from here (this fn is
        // an associated function), so we replicate the body. The replication is
        // small and the invariant is identical.
        let mut bt = meta.bin_table();
        // O(1) exact double-free guard (Phase 13.4a): test the segment's alloc
        // bitmap instead of walking the free list. The owner is the bitmap's
        // sole writer (reclaim runs on the owner — see this fn's docs), so the
        // read/modify/write needs no atomics. Replaces the former inline O(N)
        // `free_list_contains` walk that gave reclaim the same O(N²) regression
        // as own-thread free.
        let mut bm = meta.alloc_bitmap();
        if bm.is_free(off as u32) {
            return; // Already on a free list (M2 double-free): no-op.
        }
        let block_nn = match NonNull::new(ptr) {
            Some(nn) => nn,
            None => return,
        };
        let old_head = bt.head(class_idx);
        let old_head_ptr = if old_head == FREE_LIST_NULL {
            core::ptr::null_mut()
        } else {
            Node::deref(base, old_head as usize)
        };
        Node::write_next(block_nn, old_head_ptr);
        bt.set_head(class_idx, off as u32);
        bm.mark_free(off as u32);
    }

    /// TEST-ONLY: push `ptr`'s segment-relative offset — packed with its
    /// `class_idx` in the high bits — into its segment's `RemoteFreeRing`,
    /// exactly as a cross-thread freer would. Lets a single-threaded test
    /// exercise the ring→reclaim path (which the public own-thread `dealloc`
    /// bypasses) and isolate `reclaim_offset` logic from concurrency. The caller
    /// supplies `class_idx` (the class it allocated the block under) because the
    /// reclaim contract carries the class in the ring entry — the owner must
    /// never re-derive it from `page_map` (the §13 root cause).
    #[doc(hidden)]
    #[cfg(feature = "alloc-xthread")]
    pub fn dbg_push_to_ring(&self, ptr: *mut u8, class_idx: usize) -> bool {
        let base = os::segment_base_of(ptr as usize) as *mut u8;
        if !self.table.contains_base(base) {
            return false;
        }
        let off = (ptr as usize - base as usize) as u32;
        let packed = super::remote_free_ring::pack_entry(off, class_idx as u32);
        let ring = SegmentMeta::new(base).remote_ring();
        ring.push(packed).is_ok()
    }

    /// TEST-ONLY (task #37): drain every owned segment's ring into its
    /// `BinTable`, exactly as `find_segment_with_free` does, but unconditionally.
    #[doc(hidden)]
    #[cfg(feature = "alloc-xthread")]
    pub fn dbg_drain_all_rings(&mut self) {
        for base in self.table.bases() {
            let hdr = SegmentHeader::read_at(base);
            if !matches!(hdr.kind, SegmentKind::Small | SegmentKind::Primordial) {
                continue;
            }
            let ring = SegmentMeta::new(base).remote_ring();
            ring.drain(|off| Self::reclaim_offset(base, off));
        }
    }

    /// TEST-ONLY (Phase 13.3): reveal the size class `page_map` would assign
    /// to `ptr`'s page, so the counterfactual test for "own-thread dealloc
    /// derives the class from `Layout`, not `page_map`" can prove it is
    /// non-vacuous. Returns `None` if `ptr` is foreign, the segment is not
    /// small/primordial, or the page is uncarved. This is the SAME lookup
    /// `dealloc_small_by_segment` performs on the cross-thread drain path —
    /// kept here as a pure read for the test to compare against the Layout's
    /// class. `#[doc(hidden)] pub` per the established test-only surface.
    #[doc(hidden)]
    pub fn dbg_page_map_class_for(&self, ptr: *mut u8) -> Option<usize> {
        let base = os::segment_base_of(ptr as usize) as *mut u8;
        if !self.table.contains_base(base) {
            return None;
        }
        if !matches!(SegmentHeader::kind_at(base), SegmentKind::Small | SegmentKind::Primordial) {
            return None;
        }
        let meta = SegmentMeta::new(base);
        let page_idx = (ptr as usize - base as usize) / super::os::PAGE;
        meta.page_map().class_of(page_idx)
    }

    /// TEST-ONLY (Phase 13.3): the size class the own-thread `dealloc` SHOULD
    /// derive from `layout` (i.e. what `Self::classify` resolves to). Returns
    /// `None` for a Large layout. Exposed so the counterfactual test can
    /// compare the Layout-derived class against the `page_map`-derived class
    /// on a mixed-class page and prove the two genuinely differ (otherwise
    /// the test would be vacuous).
    #[doc(hidden)]
    pub fn dbg_layout_class_for(&self, layout: Layout) -> Option<usize> {
        let size = layout.size().max(super::size_classes::MIN_BLOCK);
        match Self::classify(size, layout.align()) {
            AllocKind::Small { class_idx } => Some(class_idx),
            AllocKind::Large => None,
        }
    }

    /// Shrink/grow an allocation in place or by alloc + copy + dealloc.
    ///
    /// On growth the new tail is **uninitialised** (matching `GlobalAlloc`).
    /// Returns null on failure, leaving the old allocation intact. Safe: a
    /// null `ptr` returns null without touching state.
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

    /// Iterate over all registered segment bases (read-only). Exposed for the
    /// Phase 12.4 abandonment walk (`HeapCore::segment_bases` →
    /// `abandon_segments`) and, under `alloc-xthread`, the per-segment
    /// `RemoteFreeRing` drain in `HeapCore::drain_thread_free`.
    #[cfg(any(feature = "alloc-global", feature = "alloc-xthread"))]
    pub fn segment_bases(&self) -> impl Iterator<Item = *mut u8> {
        self.table.bases()
    }

    /// Register an already-reserved segment base into this substrate's table
    /// (Phase 12.4 adoption). Returns the assigned `segment_id`, or `None` if
    /// the table is full. Used by `HeapRegistry::try_adopt` to register an
    /// adopted segment into the adopter's `AllocCore` so subsequent
    /// `alloc`/`dealloc` routing finds it. The caller MUST have laid down a
    /// valid header at `base` (the abandon path left it intact).
    #[cfg(feature = "alloc-global")]
    pub(crate) fn register_segment(&mut self, base: *mut u8) -> Option<u32> {
        self.table.register(base)
    }

    /// Mark `base` as the current small segment (Phase 12.4 adoption primitive).
    /// An adopted segment with free space becomes the bump target so the
    /// adopter carves new allocations from it. Retained for the loom-proven
    /// abandon/adopt substrate (a future decommit-when-empty policy); NOT on
    /// the hot path of the shard model (a heap owns its segments exclusively
    /// and never transfers them).
    #[cfg(feature = "alloc-global")]
    #[allow(dead_code)]
    pub(crate) fn set_small_current(&mut self, base: *mut u8) {
        self.small_cur = base;
    }

    // -----------------------------------------------------------------------
    // Internals — the safe Cartographer. All raw memory touches go through
    // `Node`; no `Vec`/`Box`/`HashSet`/`std::alloc`.
    // -----------------------------------------------------------------------

    /// Classify a `(size, align)` request as Small or Large.
    #[inline]
    fn classify(size: usize, align: usize) -> AllocKind {
        match SizeClasses::class_for(size, align) {
            Some(class_idx) => AllocKind::Small { class_idx },
            None => AllocKind::Large,
        }
    }

    /// Allocate a small block of the given class. Routes through the current
    /// small segment's free list (pop); on a miss, scans ALL owned segments for
    /// one with a non-empty class free list (Phase 12.1: free state lives in
    /// per-segment `BinTable`s, so a freed block in a non-current segment must
    /// be reusable — otherwise non-current segments leak unboundedly); only
    /// then carves a fresh block / reserves a fresh segment. When carving, also
    /// carves a refill batch (Phase 9 amortisation), pushing each extra block
    /// into its OWN segment's `BinTable` via `segment_base_of` (defect A fix:
    /// never a captured "current" pointer).
    ///
    /// Phase 12.5 (shard model): a heap owns its segments exclusively — there
    /// is no adoption hook. On a free-list miss it carves/reserves from its
    /// OWN segments only. Cross-thread frees arrive via the inline TFS and are
    /// drained by `HeapCore::alloc` BEFORE this runs, so they are already on
    /// the per-segment BinTables by the time we scan.
    fn alloc_small(&mut self, class_idx: usize) -> *mut u8 {
        let block_size = SizeClasses::block_size(class_idx);
        debug_assert!(block_size >= NODE_SIZE);
        // 1. Try the free list of the current small segment.
        if let Some(ptr) = self.pop_free(self.small_cur, class_idx, block_size) {
            return ptr;
        }
        // 2. Current segment's class free list is empty: scan the OTHER owned
        //    segments for one with a non-empty class free list. A freed block
        //    may live in any segment we own (Phase 12.1 segment-centric free
        //    state); without this scan those blocks would leak. O(segments)
        //    only on a free-list miss — acceptable for 12.1 (per-class
        //    segment queues are a Phase 13 speed optimisation, not a 12.1
        //    deliverable). M5-safe: pure arithmetic + head reads via `Node`,
        //    no allocation.
        if let Some(seg) = self.find_segment_with_free(class_idx) {
            if let Some(ptr) = self.pop_free(seg, class_idx, block_size) {
                return ptr;
            }
        }
        // 3. No free block anywhere: carve a FRESH block. On the cold carve
        //    path we also carve a refill batch (Phase 9 amortisation) so the
        //    next allocs pop from the free list instead of carving one-by-one.
        //    Each refilled block is pushed into its OWN segment's BinTable
        //    (via `segment_base_of(ptr)`), never a captured "current" pointer
        //    — defect A fix: `small_cur` may shift mid-batch when a segment
        //    fills, and a captured pointer would then target the wrong
        //    segment, corrupting its BinTable head.
        if let Some(ptr) = self.carve_block_with_refill(class_idx, block_size) {
            return ptr;
        }
        // 4. Current segment is full: reserve a new small segment and retry.
        match self.reserve_small_segment() {
            Some(_) => {
                // Retry once on the fresh segment. Recurse-free: a single
                // direct retry (not a loop that could grow unboundedly).
                if let Some(ptr) = self.pop_free(self.small_cur, class_idx, block_size) {
                    return ptr;
                }
                // no-panic: a fresh small segment is guaranteed by construction
                // to have room for at least one block of every small class
                // (compile-time sanity: `small_meta_end() + PAGE <= SEGMENT`,
                // and every class block fits in a page). If carve_block returns
                // None here it indicates metadata corruption; we return null
                // (graceful OOM) rather than panicking — the GlobalAlloc face
                // (Phase 11) must never abort.
                self.carve_block_with_refill(class_idx, block_size)
                    .unwrap_or(core::ptr::null_mut())
            }
            None => core::ptr::null_mut(),
        }
    }

    /// Carve one fresh block of `class_idx` for the caller, plus a refill
    /// batch of extra blocks that are pushed onto their OWN segments'
    /// `BinTable[class_idx]` (Phase 9 amortisation, Phase 12.1 segment-centric
    /// free state). Each extra block's owning segment is derived per-block via
    /// `segment_base_of(ptr)` — `small_cur` may shift mid-batch when the
    /// current segment fills, so a captured pointer would corrupt the wrong
    /// segment's BinTable head (defect A).
    ///
    /// Returns the first carved block (for the caller), or `None` if the
    /// current segment cannot fit even one block (caller reserves a fresh
    /// segment and retries).
    fn carve_block_with_refill(
        &mut self,
        class_idx: usize,
        block_size: usize,
    ) -> Option<*mut u8> {
        // Carve the caller's block first.
        let first = self.carve_block(class_idx, block_size)?;
        // Refill batch: carve extra blocks and push each into its OWN segment.
        // `carve_block` returns None when the current segment is full; we stop
        // the batch there (the next alloc will reserve a fresh segment).
        const REFILL_BATCH: usize = 31;
        for _ in 0..REFILL_BATCH {
            let Some(extra) = self.carve_block(class_idx, block_size) else {
                break;
            };
            let base = os::segment_base_of(extra as usize) as *mut u8;
            self.dealloc_small(base, extra, class_idx);
        }
        Some(first)
    }

    /// Scan all owned SMALL/PRIMORDIAL segments and return the base of the
    /// first one whose `BinTable[class_idx]` is non-empty. Used by
    /// [`alloc_small`] on a current-segment miss to reuse freed blocks in
    /// non-current segments (Phase 12.1: free state lives in per-segment
    /// `BinTable`s).
    ///
    /// **Large segments are excluded:** a large segment has no `BinTable`
    /// (only a header), so reading its `bin_table()` would dereference
    /// garbage and could return a bogus non-null head — leading `pop_free`
    /// to read a junk block and compute an out-of-segment `next` pointer
    /// (overflow/UAF). We read each candidate's header `kind` and skip
    /// non-small/primordial segments.
    ///
    /// Returns `None` if no owned small segment has a free block of this
    /// class. Pure safe composition: iterates `self.table.bases()` (read-only)
    /// and reads each segment's header kind + `BinTable` head through the
    /// `node` seam. No allocation, no mutation — M5 (reentrancy-freedom)
    /// upheld.
    pub(crate) fn find_segment_with_free(&self, class_idx: usize) -> Option<*mut u8> {
        for base in self.table.bases() {
            // Skip large/huge segments: they have no BinTable. Field-specific
            // `kind` read (task #33): this is the Owner's alloc path,
            // concurrent with a Remote's `dealloc_routing` field reads — a
            // full-struct `read_at` here would race them. `kind_at` reads only
            // the `kind` byte, disjoint from any writer.
            if !matches!(SegmentHeader::kind_at(base), SegmentKind::Small | SegmentKind::Primordial) {
                continue;
            }
            // Variant-2: lazily drain this segment's remote-free ring before
            // inspecting its BinTable. Cross-thread frees that targeted THIS
            // segment (a segment we own but are not currently allocating from)
            // are sitting in its ring; without this drain they would never
            // reach the BinTable and the scan would miss them. The drain uses
            // the self-less `reclaim_offset` (it touches only segment metadata
            // via `SegmentMeta`, never `self`), so the immutable `bases()`
            // iterator borrow is not aliased. M5-clean.
            #[cfg(feature = "alloc-xthread")]
            {
                let ring = SegmentMeta::new(base).remote_ring();
                ring.drain(|off| Self::reclaim_offset(base, off));
            }
            let meta = SegmentMeta::new(base);
            let bt = meta.bin_table();
            if bt.head(class_idx) != FREE_LIST_NULL {
                return Some(base);
            }
        }
        None
    }

    /// Pop a free block of `class_idx` from `segment`'s bin table. Returns
    /// null if the free list is empty. Writes the block's `next` word to null
    /// (it becomes the new head) via the node seam.
    #[inline]
    fn pop_free(&self, segment: *mut u8, class_idx: usize, block_size: usize) -> Option<*mut u8> {
        let meta = SegmentMeta::new(segment);
        let mut bt = meta.bin_table();
        let head_off = bt.head(class_idx);
        if head_off == FREE_LIST_NULL {
            return None;
        }
        let block_ptr = Node::deref(segment, head_off as usize);
        let block_nn = NonNull::new(block_ptr)?;
        let next = Node::read_next(block_nn);
        let new_head = if next.is_null() {
            FREE_LIST_NULL
        } else {
            // Compute the offset of `next` relative to this segment. `next`
            // is an absolute pointer into the same segment (free lists are
            // per-segment), so offset = next - segment.
            (next as usize - segment as usize) as u32
        };
        bt.set_head(class_idx, new_head);
        // Phase 13.4a: clear the block's bitmap bit — it leaves the free list
        // and is handed to the caller, so a subsequent free must NOT see it as
        // already-free (and the next legitimate free must be able to re-mark it).
        meta.alloc_bitmap().mark_alloc(head_off);
        let _ = block_size; // block_size is the caller's invariant; not needed here.
        Some(block_ptr)
    }

    /// Carve a fresh `block_size`-aligned block from the current small
    /// segment's bump cursor. Returns None if the segment is full.
    ///
    /// On a page boundary crossing, marks the freshly entered page as owned by
    /// `class_idx` in the page map (the page-dedication rule).
    fn carve_block(&mut self, class_idx: usize, block_size: usize) -> Option<*mut u8> {
        let segment = self.small_cur;
        let mut meta = SegmentMeta::new(segment);
        // Field-specific bump read/write (task #33 root-cause fix): the Owner
        // touches ONLY the `bump` field, never the cross-thread-read header
        // fields. A full-struct `write_header` here rewrote `magic`/`kind`/
        // `owner_thread_free` too, racing a Remote's full-struct `read_at` in
        // `dealloc_routing` (the §11 data race). `bump` is owner-only (no
        // Remote reads it), so a plain field write is race-free.
        let bump = meta.bump_of();
        let aligned_bump = align_up(bump, block_size);
        if aligned_bump + block_size > SEGMENT {
            return None;
        }
        // Update ONLY the bump cursor.
        meta.set_bump(aligned_bump + block_size);
        // Mark the page containing `aligned_bump` as owned by `class_idx`.
        let mut pm = meta.page_map();
        let page = aligned_bump / super::os::PAGE;
        if pm.class_of(page).is_none() {
            // Page was Free or Meta; dedicate it to this class.
            pm.set_class(page, class_idx);
        }
        let ptr = Node::deref(segment, aligned_bump);
        Some(ptr)
    }

    /// Deallocate a small block: push it onto its owning segment's class free
    /// list. `ptr` is the block address; `base` is its segment base (computed
    /// by the caller via `segment_of`).
    ///
    /// **Double-free guard (M2 — Phase 13.4a):** before pushing, we test the
    /// segment's [`AllocBitmap`](super::alloc_bitmap::AllocBitmap) bit for this
    /// block. If it is already FREE (`is_free` true → the block is on some free
    /// list of this segment), this is a double-free: we no-op (never corrupt the
    /// free list — no self-loop, no duplicate). Otherwise we set the bit
    /// (`mark_free`) and push. This replaces the Phase 8 O(free-list-length)
    /// `free_list_contains` walk — which made own-thread free O(N²) under churn
    /// (#41) — with an O(1) exact bit test. The bitmap is single-writer (owner
    /// only), so the read/modify/write needs no atomics.
    #[inline]
    fn dealloc_small(&mut self, base: *mut u8, ptr: *mut u8, class_idx: usize) {
        let meta = SegmentMeta::new(base);
        let mut bt = meta.bin_table();
        let off = (ptr as usize - base as usize) as u32;
        // O(1) exact double-free guard via the alloc bitmap.
        let mut bm = meta.alloc_bitmap();
        if bm.is_free(off) {
            return; // Already on a free list (M2 double-free): no-op.
        }
        let block_nn = match NonNull::new(ptr) {
            Some(nn) => nn,
            None => return,
        };
        let old_head = bt.head(class_idx);
        let old_head_ptr = if old_head == FREE_LIST_NULL {
            core::ptr::null_mut()
        } else {
            Node::deref(base, old_head as usize)
        };
        Node::write_next(block_nn, old_head_ptr);
        bt.set_head(class_idx, off);
        bm.mark_free(off);
    }

    /// Allocate a large/huge block: reserve a dedicated segment sized to fit,
    /// place the allocation at the first page-aligned offset past the header,
    /// register the segment, and return the allocation pointer.
    fn alloc_large(&mut self, size: usize, align: usize) -> *mut u8 {
        // The segment must hold: header + alignment padding + size, rounded up
        // to a whole number of segments. `Segment::reserve` does the rounding.
        let hdr_aligned = align_up(core::mem::size_of::<SegmentHeader>(), align.max(super::os::PAGE));
        let needed = hdr_aligned + align_up(size, align);
        let segment = match Segment::reserve(needed) {
            Some(s) => s,
            None => return core::ptr::null_mut(),
        };
        let base = segment.as_ptr();
        let reservation = segment.reservation();
        let reservation_len = segment.reservation_len();
        // no-panic: register returns None if the segment table is full (too many
        // live large allocations). We release the segment and return null
        // (graceful OOM) rather than panicking.
        let id = match self.table.register(base) {
            Some(id) => id,
            None => {
                // Release the segment we just reserved (drop releases it).
                drop(segment);
                return core::ptr::null_mut();
            }
        };
        // Lay down the large header. The allocation lives at `hdr_aligned`.
        let bump = hdr_aligned + align_up(size, align);
        let hdr = SegmentHeader::large(
            id,
            size,
            align,
            bump,
            reservation.as_ptr(),
            reservation_len,
        );
        Node::write_struct(base as *mut SegmentHeader, hdr);
        // Forget the owning handle: drop walks the registry to free.
        core::mem::forget(segment);
        Node::deref(base, hdr_aligned)
    }

    /// Reserve a fresh small segment, initialise its metadata, register it,
    /// and set it as the current small segment. Returns its base.
    fn reserve_small_segment(&mut self) -> Option<*mut u8> {
        let segment = Segment::reserve(SEGMENT)?;
        let base = segment.as_ptr();
        let reservation = segment.reservation();
        let reservation_len = segment.reservation_len();
        // no-panic: register returns None if the segment table is full. We
        // release the segment and return None (graceful OOM).
        let id = self.table.register(base)?;
        // Lay down the small header + page map + bin table at the fixed
        // offsets. `bump` starts at the small-meta end (past the metadata).
        let meta_end = SegLayout::small_meta_end();
        let meta_pages = SegLayout::small_meta_pages();
        let mut meta = SegmentMeta::new(base);
        meta.write_header(SegmentHeader::small(
            id,
            meta_end,
            reservation.as_ptr(),
            reservation_len,
        ));
        PageMap::init_in_place(base_add(base, SegLayout::page_map_off()), meta_pages);
        BinTable::init_in_place(base_add(base, SegLayout::bin_table_off()) as *mut u32);
        // Initialise the per-segment alloc-bitmap (Phase 13.4a double-free
        // guard) to all-zeros; bits flip to FREE as blocks are pushed.
        super::alloc_bitmap::AllocBitmap::init_in_place(
            base_add(base, SegLayout::alloc_bitmap_off()),
        );
        // Initialise the per-segment remote-free ring (Variant-2 fix). Only
        // under `alloc-xthread`; the Layout always reserves the bytes.
        #[cfg(feature = "alloc-xthread")]
        {
            super::remote_free_ring::RemoteFreeRing::init_in_place(
                base,
                SegLayout::remote_ring_off(),
            );
        }
        core::mem::forget(segment);
        self.small_cur = base;
        Some(base)
    }
}

impl Default for AllocCore {
    fn default() -> Self {
        Self::new().expect("AllocCore::new: primordial segment reservation failed (OOM)")
    }
}

impl Drop for AllocCore {
    fn drop(&mut self) {
        // Collect every segment's `(reservation, reservation_len)` into a
        // fixed-size stack array FIRST, then free them all. We must NOT free
        // the primordial segment while still reading the registry — the
        // registry lives IN the primordial's payload, so freeing it would
        // unmap the array we're iterating over. Collecting up front (into a
        // stack array, no global-allocator involvement) breaks that aliasing.
        //
        // All registered segments are still mapped at drop time (Phase 8 does
        // not eagerly release in `dealloc` — see M6 / the Large branch). So
        // reading every header is safe. We free every registered reservation
        // exactly once.
        //
        // The array is bounded by MAX_SEGMENTS (1024 × 16 B = 16 KiB stack —
        // fine; a deeply-nested drop chain would be the only concern, and
        // AllocCore is a top-level owner).
        let mut to_free: [(*mut u8, usize); super::segment_table::MAX_SEGMENTS] =
            [(core::ptr::null_mut(), 0usize); super::segment_table::MAX_SEGMENTS];
        let mut n = 0usize;
        for base in self.table.bases() {
            if n >= super::segment_table::MAX_SEGMENTS {
                break;
            }
            let hdr = SegmentHeader::read_at(base);
            // Every registered segment has a valid reservation recorded (set
            // at register-time). We free them all — including large segments
            // whose magic was zeroed by `dealloc` (they are still mapped and
            // still carry the reservation info in their header).
            to_free[n] = (hdr.reservation, hdr.reservation_len);
            n += 1;
        }
        // Now free every collected reservation. The primordial (whose payload
        // hosts the registry) is freed here alongside the rest — safe, because
        // we no longer read the registry.
        for &(reservation, reservation_len) in &to_free[..n] {
            os::release_segment(reservation, reservation_len);
        }
    }
}

// NOTE: `AllocCore` is intentionally NOT `Send` (nor `Sync`) in Phase 8.
// Phase 8 is single-threaded; `Send` is not needed. Phase 9 (per-thread
// heaps) will add `Send` at the heap layer (the segment substrate is
// `Send`-capable, but the claim belongs to the layer that owns the threading
// discipline, not the substrate itself). Adding it here would require an
// `unsafe impl` that has no place outside the two named `unsafe` seams.

/// `base + off` as `*mut u8`, routed through the `node` seam. The Cartographer
/// only ever passes offsets derived from the fixed [`SegLayout`] or the bump
/// cursor (both bounded by `SEGMENT`).
fn base_add(base: *mut u8, off: usize) -> *mut u8 {
    Node::offset(base, off)
}
