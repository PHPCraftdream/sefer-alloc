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
        // Now safe to read the header (the base is one of ours).
        let hdr = SegmentHeader::read_at(base);
        if hdr.magic != super::segment_header::SEGMENT_MAGIC {
            return;
        }
        match hdr.kind {
            SegmentKind::Large => {
                // Large/huge: mark the segment as freed (zero the magic) so
                // `Drop` knows its reservation should be released. We do NOT
                // release eagerly here — that would unmap the header before
                // `Drop` can read it to discover the reservation info. (M6 —
                // eager decommit / OS return — is a Phase 10 deliverable; for
                // Phase 8 correctness, all segment freeing happens at `Drop`.)
                let mut stale = hdr;
                stale.magic = 0;
                Node::write_struct(base as *mut SegmentHeader, stale);
            }
            SegmentKind::Small | SegmentKind::Primordial => {
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
    /// `abandon_segments`).
    #[cfg(feature = "alloc-global")]
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
            // Skip large/huge segments: they have no BinTable. Reading the
            // header kind is safe (every registered segment has a valid header
            // at offset 0, including large ones).
            let hdr = SegmentHeader::read_at(base);
            if !matches!(hdr.kind, SegmentKind::Small | SegmentKind::Primordial) {
                continue;
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
        let mut hdr = meta.header();
        let bump = hdr.bump;
        let aligned_bump = align_up(bump, block_size);
        if aligned_bump + block_size > SEGMENT {
            return None;
        }
        // Update the bump cursor.
        hdr.bump = aligned_bump + block_size;
        meta.write_header(hdr);
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
    /// **Double-free guard (M2):** before pushing, we scan the class free list
    /// for `ptr`. If it is already on the list (a double-free), this is a
    /// no-op — we never corrupt the free list (no self-loop, no duplicate).
    /// The scan is O(free-list length); Phase 8 free lists stay short for a
    /// typical working set, and Phase 9's per-thread heaps will replace this
    /// with a cheaper cookie-based guard.
    fn dealloc_small(&mut self, base: *mut u8, ptr: *mut u8, class_idx: usize) {
        let meta = SegmentMeta::new(base);
        let mut bt = meta.bin_table();
        // Double-free guard: walk the free list; if `ptr` is already there,
        // no-op (M2 — never corrupt).
        if self.free_list_contains(&bt, base, class_idx, ptr) {
            return;
        }
        let off = (ptr as usize - base as usize) as u32;
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
    }

    /// Whether `ptr` is currently on segment `base`'s class-`class_idx` free
    /// list. O(free-list length). Used by the double-free guard.
    fn free_list_contains(
        &self,
        bt: &BinTable,
        base: *mut u8,
        class_idx: usize,
        ptr: *mut u8,
    ) -> bool {
        let mut cur_off = bt.head(class_idx);
        while cur_off != FREE_LIST_NULL {
            let cur_ptr = Node::deref(base, cur_off as usize);
            if cur_ptr == ptr {
                return true;
            }
            let cur_nn = match NonNull::new(cur_ptr) {
                Some(n) => n,
                None => return false,
            };
            let next = Node::read_next(cur_nn);
            if next.is_null() {
                return false;
            }
            // Guard against a (bug-introduced) self-loop terminating the scan.
            if next == cur_ptr {
                return false;
            }
            cur_off = (next as usize - base as usize) as u32;
        }
        false
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
