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
#[cfg(feature = "numa-aware")]
use super::numa;
#[cfg(not(feature = "numa-aware"))]
use super::os::Segment;
use super::os::{self, SEGMENT};
use super::segment_header::{
    align_up, BinTable, Layout as SegLayout, PageMap, SegmentHeader, SegmentKind, SegmentMeta,
    FREE_LIST_NULL,
};
use super::segment_table::{SegmentTable, MAX_SEGMENTS};
use super::size_classes::{AllocKind, SizeClasses};

// ---------------------------------------------------------------------------
// OPT-E — large-segment free-cache (feature = "alloc-decommit")
//
// The hot path for `alloc_large` / `dealloc` large is a full OS round-trip
// (mmap/VirtualAlloc + munmap/VirtualFree). mimalloc avoids this by keeping a
// per-allocator page-cache of recently-freed large spans so the next alloc of
// the same size hits the cache instead of the OS (~800 ns vs ~8–240 µs).
//
// We implement a MINIMAL version: a fixed array of LARGE_CACHE_SLOTS entries.
// The cache is ONLY active under `alloc-decommit` (it uses `table.recycle` for
// the slot-NULL step, which is only compiled with that feature; this keeps the
// logic consistent with the decommit-gate on the small-segment recycle path).
// ---------------------------------------------------------------------------

/// Maximum number of large segments held in the free-cache between uses.
/// 2 slots is enough to eliminate the OS round-trip for the common alloc→free→
/// alloc pattern, without holding significant unreachable virtual memory.
#[cfg(feature = "alloc-decommit")]
const LARGE_CACHE_SLOTS: usize = 2;

/// Maximum size of a single cached large reservation, in bytes. Segments larger
/// than this are released immediately to the OS rather than cached.
/// 64 MiB: covers the 4 MiB / 16 MiB typical workloads without holding an
/// enormous reservation when a one-off 100 MiB+ allocation is freed.
#[cfg(feature = "alloc-decommit")]
const MAX_CACHED_LARGE_BYTES: usize = 64 * 1024 * 1024;

/// Size-ratio bound: we only reuse a cached entry if its usable_size is at most
/// `needed * LARGE_CACHE_SIZE_FACTOR`. Without this a 64 MiB cached segment
/// would be permanently reused for every 4 MiB request — wasting 60 MiB of
/// RSS during the cache lifetime.
#[cfg(feature = "alloc-decommit")]
const LARGE_CACHE_SIZE_FACTOR: usize = 2;

/// One entry in the large-segment free-cache.
///
/// Invariant: `base` is SEGMENT-aligned, `reservation` was returned by the OS,
/// `usable_size` equals the `usable` computed in `alloc_large` at the time the
/// segment was first reserved (i.e. `n_segments * SEGMENT`). The segment's OS
/// reservation is still live (not yet released to the OS). Pages are kept
/// COMMITTED (no decommit on deposit) so that a cache hit requires no recommit.
///
/// When a cache hit occurs, the caller MUST:
///   1. Re-register `base` in the `SegmentTable`.
///   2. Write a fresh `SegmentHeader` over the old one (pages already committed).
///   3. Return `Node::deref(base, hdr_aligned)` to the caller.
#[cfg(feature = "alloc-decommit")]
struct CachedLarge {
    /// Start of the original OS reservation.
    reservation: *mut u8,
    /// Total size of the OS reservation.
    reservation_len: usize,
    /// SEGMENT-aligned base of the segment (the "usable" start).
    base: *mut u8,
    /// The `usable` bytes this reservation covers — `n_segments * SEGMENT` for
    /// the original allocation. Used to match incoming requests.
    usable_size: usize,
}

/// TEST-ONLY (Phase 35): process-wide M6-decommit invocation counter. Bumped in
/// [`AllocCore::decommit_empty_segment`]; read by the soak test via
/// [`AllocCore::dbg_decommit_count`]. Diagnostic only (relaxed).
#[cfg(feature = "alloc-decommit")]
static DECOMMIT_CALLS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

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
    /// OPT-E — large-segment free-cache. A small fixed array of recently-freed
    /// large/huge segments whose OS reservations are still live. `alloc_large`
    /// checks this array first; a size-matched entry is reused without a new
    /// OS reservation. `dealloc` on the large path deposits the segment here
    /// (if a slot is free and the segment is small enough) instead of releasing
    /// the OS reservation immediately. Pages are kept committed between uses so
    /// no recommit syscall is needed on a cache hit. The cache is gated on
    /// `alloc-decommit` for consistency with the small-segment recycle path
    /// (both operate in the regime where empty slots are recyclable).
    #[cfg(feature = "alloc-decommit")]
    large_cache: [Option<CachedLarge>; LARGE_CACHE_SLOTS],
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
        // Phase C (numa-aware): the primordial segment was reserved by
        // `bootstrap::primordial()` via the plain OS path (it predates NUMA
        // awareness). Stamp the current thread's NUMA node into its header NOW
        // so that `find_segment_with_free` can treat it as a local segment.
        // On platforms without NUMA `current_node()` returns `NO_NODE`; the
        // field already holds `NO_NODE_RAW` (same value), so this is a no-op
        // in terms of visible effect — but it makes the invariant explicit.
        #[cfg(feature = "numa-aware")]
        {
            let my_node = numa::current_node();
            SegmentMeta::new(primordial_base).set_node_id(my_node);
        }
        Some(Self {
            table: prim.table,
            small_cur,
            #[cfg(feature = "alloc-decommit")]
            large_cache: [const { None }; LARGE_CACHE_SLOTS],
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
        let base = os::segment_base_of_ptr(ptr);
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
                // Large/huge: the segment is being freed. The full header read
                // here is on the cold Large path (one allocation per segment,
                // rare), so the dependent-load cost does not matter.
                //
                // OPT-E (alloc-decommit): if the segment is small enough to
                // cache AND a free slot exists, decommit its payload pages and
                // deposit it into the large_cache so the next alloc_large of a
                // compatible size can reuse it without an OS round-trip.
                //
                // Without alloc-decommit: mark the segment as freed (zero the
                // magic) so `Drop` knows its reservation should be released.
                // We do NOT release eagerly here — that would unmap the header
                // before `Drop` can read it to discover the reservation info.
                let stale = SegmentHeader::read_at(base);

                #[cfg(feature = "alloc-decommit")]
                {
                    // Try to cache the segment. Conditions for caching:
                    //   1. usable_size <= MAX_CACHED_LARGE_BYTES.
                    //   2. There is a free (None) slot in large_cache.
                    let hdr_aligned = align_up(
                        core::mem::size_of::<SegmentHeader>(),
                        stale.large_align.max(super::os::PAGE),
                    );
                    let n_segments = (hdr_aligned + align_up(stale.large_size, stale.large_align))
                        .div_ceil(SEGMENT);
                    let usable_size = n_segments * SEGMENT;

                    let free_slot = if usable_size <= MAX_CACHED_LARGE_BYTES {
                        self.large_cache.iter().position(|s| s.is_none())
                    } else {
                        None
                    };

                    if let Some(slot_idx) = free_slot {
                        // We keep the pages COMMITTED in the cache (no decommit
                        // on deposit). On Windows, `VirtualAlloc(MEM_DECOMMIT)`
                        // followed immediately by `VirtualAlloc(MEM_COMMIT)` on
                        // the next cache hit costs more than just leaving the
                        // pages mapped — the entire purpose of the cache is to
                        // amortise the OS round-trip cost. Decommitting here
                        // would reduce RSS by the usable payload size, but at the
                        // cost of an expensive recommit on every hit, negating
                        // the speedup. We intentionally trade RSS for latency:
                        // a cached large segment keeps its pages warm between uses.
                        //
                        // NULL the table slot WITHOUT releasing the OS reservation.
                        // The cached entry owns the reservation; AllocCore::drop
                        // releases it explicitly from the large_cache array.
                        self.table.unregister(base);
                        // Zero the magic so that if something reads the header
                        // while it's in the cache, it won't be confused as a
                        // live registered segment.
                        let mut hdr_zero = stale;
                        hdr_zero.magic = 0;
                        Node::write_struct(base as *mut SegmentHeader, hdr_zero);
                        // Deposit into cache.
                        self.large_cache[slot_idx] = Some(CachedLarge {
                            reservation: stale.reservation,
                            reservation_len: stale.reservation_len,
                            base,
                            usable_size,
                        });
                        return;
                    }
                    // No free slot or too large: fall through to immediate release.
                    // NULL the magic so Drop frees the reservation via the header.
                    let mut stale2 = stale;
                    stale2.magic = 0;
                    Node::write_struct(base as *mut SegmentHeader, stale2);
                }
                #[cfg(not(feature = "alloc-decommit"))]
                {
                    let mut stale2 = stale;
                    stale2.magic = 0;
                    Node::write_struct(base as *mut SegmentHeader, stale2);
                }
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
    // `small_cur` is consumed only by the `alloc-decommit` dec-then-decommit
    // step; without that feature the reclaim path does no live-count bookkeeping.
    #[cfg_attr(not(feature = "alloc-decommit"), allow(unused_variables))]
    pub(crate) fn reclaim_offset(base: *mut u8, packed: u32, small_cur: *mut u8) -> bool {
        // Unpack the offset and the class the cross-thread freer stamped.
        let (off, class_idx) = super::remote_free_ring::unpack_entry(packed);
        let off = off as usize;
        let class_idx = class_idx as usize;
        let ptr = Node::deref(base, off);
        // Field-specific reads: this runs on the Owner's alloc path
        // (find_segment_with_free's lazy ring drain), concurrent with a
        // Remote's `dealloc_routing` field reads. A full-struct
        // `SegmentHeader::read_at` here would race them; reading individual
        // fields via their offsets touches bytes disjoint from any racing
        // writer, so there is no data race.
        if SegmentHeader::magic_at(base) != super::segment_header::SEGMENT_MAGIC {
            return false;
        }
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return false;
        }
        // Sanity: the offset must be a whole number of `block_size` units. carve
        // aligns the bump to `block_size`, so a real block offset is always a
        // multiple of its class's block_size. A mis-aligned offset would write
        // the free-list `next` into the middle of a block — the §13 corruption.
        // This never fires for a correctly-packed entry; it is defence-in-depth
        // against a garbled ring value (no abort — just skip, matching the
        // defensive `dealloc` contract).
        let bs = SizeClasses::block_size(class_idx) as u32;
        if !(off as u32).is_multiple_of(bs) {
            return false;
        }
        let meta = SegmentMeta::new(base);
        // Phase 35 (M6 decommit) — the STALE-RING-INTO-DECOMMITTED-SEGMENT guard.
        // When a segment empties it is decommitted AND reset: its `bump` returns
        // to `small_meta_end()` and its alloc bitmap is zeroed. A ring entry that
        // arrives (or lingers) for an offset in the now-decommitted payload would
        // pass the bitmap `is_free` check (the reset cleared every bit), and the
        // reclaim below would `write_next` into a DECOMMITTED page — a UAF / write
        // to unmapped memory. The bump guard closes this: a real, currently-carved
        // block always has `off < bump`; an offset `>= bump` is either uncarved or
        // (post-reset) in the decommitted region — no-op, never touch the page.
        // This is the concrete realization of design §1.3 ("reclaim does a no-op
        // BEFORE touching the block on a stale entry") for the reset bitmap. The
        // owner is the sole `bump` writer, and reclaim runs owner-side, so this
        // field read is consistent (no concurrent bump write). Owner-only, so
        // gated to the feature that resets the bump.
        #[cfg(feature = "alloc-decommit")]
        if off >= meta.bump_of() {
            return false;
        }
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
            return false; // Already on a free list (M2 double-free): no-op.
        }
        let block_nn = match NonNull::new(ptr) {
            Some(nn) => nn,
            None => return false,
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
        // Phase 35 (M6): a cross-thread-freed block is now back on the free list
        // → one fewer live block. The owner-side drain runs this, so the
        // owner-only counter is single-writer (the cross-thread freer NEVER
        // touched it — it only pushed the offset into the ring). If the segment
        // is now empty AND not the carve target, return its payload to the OS.
        // Returns true if decommit fired (caller should call recycle after drain).
        #[cfg(feature = "alloc-decommit")]
        {
            Self::dec_live_and_maybe_decommit(base, small_cur)
        }
        #[cfg(not(feature = "alloc-decommit"))]
        false
    }

    /// Phase 35 (M6 decommit) — the shared dec-then-maybe-decommit step, called
    /// after a block returns to a segment's free list (own-thread `dealloc_small`
    /// or owner-side `reclaim_offset`). It decrements the owner-only `live_count`
    /// and, if the segment just went empty (`live_count == 0`) AND is not the
    /// current carve target (`base != small_cur`), returns the segment's payload
    /// pages to the OS, resets the segment, releases the OS reservation, and
    /// recycles the table slot (task #60, variant B).
    ///
    /// **Self-less** (associated fn) so the self-less `reclaim_offset` can call
    /// it; the `small_cur` snapshot and `table` raw pointer are threaded in from
    /// the owner. The raw pointer is sound because `AllocCore` is single-owner
    /// (owner thread is the sole writer of its segments' metadata and table).
    ///
    /// ## Why M6 is decommit-safe WITHOUT an M11 epoch barrier (design §1)
    ///
    /// The original plan (§2.5) reached for `crossbeam-epoch` because the OLD
    /// intrusive cross-thread-free model wrote the free-list `next` pointer INSIDE
    /// the block — a late cross-thread freer could write into a page we had just
    /// decommitted (UAF / write-to-unmapped). Variant-2 (Phase 12.6) dissolved
    /// that: the cross-thread freer NEVER dereferences the block — it pushes
    /// `(offset|class)` into the `RemoteFreeRing`, which lives in the segment's
    /// METADATA (the metadata pages are NEVER decommitted — we decommit only
    /// `[small_meta_end, SEGMENT)`). The decommit is therefore safe without epoch:
    ///
    ///   1. We decommit the payload ONLY at `live_count == 0` → there is not one
    ///      live block in the decommitted range; nothing to UAF.
    ///   2. A late VALID cross-thread free at `live_count == 0` is impossible:
    ///      every block is already free, so a further free of one is a double-free
    ///      (the bitmap `is_free` guard below makes it a no-op before any write).
    ///   3. `reclaim_offset` on a stale ring entry computes the block address via
    ///      `Node::deref` (pure arithmetic — NO memory access) and then reads
    ///      `magic` / `kind` / **bitmap `is_free`** — ALL in the never-decommitted
    ///      metadata — and for a free block (and at `live==0` ALL are free) does a
    ///      no-op BEFORE touching the block. The decommitted page is never read or
    ///      written.
    ///   4. `reclaim` (drain) and `decommit` both run owner-side, so they are
    ///      serialized on the owning thread — there is no reclaim-vs-decommit race
    ///      on one segment.
    ///
    /// ⇒ No UAF, no write to decommitted memory. `crossbeam-epoch` is NOT needed;
    /// none is added. (Full argument: `docs/PHASE35_DECOMMIT_DESIGN.md` §1.)
    ///
    /// ## Slot recycle (task #60)
    ///
    /// After decommit + reset, [`decommit_empty_segment`] also releases the OS
    /// reservation for the segment and NULLs the table slot (via `table`). This
    /// lifts the 1024-segment hard cap: the freed slot can be reused immediately
    /// by the next `register` call, so long-running workloads never exhaust the
    /// table. Both the OS release and the slot NULL happen atomically inside
    /// `decommit_empty_segment`; there is no window where the OS segment is
    /// released but the slot is still non-NULL.
    /// Returns `true` if decommit fired (the segment became empty, was
    /// decommitted, and needs slot recycling). The caller is responsible for
    /// calling `self.table.recycle(base)` when `true` is returned — but ONLY
    /// after any in-progress ring drain for `base` has completed, so that
    /// stale ring entries can still read the (still-committed) metadata.
    #[cfg(feature = "alloc-decommit")]
    fn dec_live_and_maybe_decommit(base: *mut u8, small_cur: *mut u8) -> bool {
        let mut meta = SegmentMeta::new(base);
        let live = meta.dec_live();
        // Only an empty, non-current, not-already-decommitted segment is
        // returned to the OS. The current carve target stays committed (we are
        // about to bump-allocate into it); already-decommitted is idempotent.
        if live != 0 || base == small_cur || meta.is_decommitted() {
            return false;
        }
        // NEVER decommit the PRIMORDIAL segment: its metadata extends to
        // `primordial_meta_end()` (it hosts the self-hosted registry between
        // `small_meta_end()` and `primordial_meta_end()`), but the decommit reset
        // computes the payload start at `small_meta_end()`. Decommitting from
        // there would return the registry pages to the OS and reset page-map /
        // bump over the registry — corrupting the substrate. Only `Small`
        // segments (whose payload genuinely starts at `small_meta_end()`) are
        // eligible. A field-specific `kind` read (disjoint from the owner's
        // `bump`/`live_count` writes; race-free like the other `kind_at` reads).
        if !matches!(SegmentHeader::kind_at(base), SegmentKind::Small) {
            return false;
        }
        Self::decommit_empty_segment(&mut meta, base);
        true
    }

    /// TEST-ONLY (Phase 35): the process-wide count of M6 decommit invocations
    /// (`decommit_empty_segment` calls). The soak test reads this to assert the
    /// decommit hook actually fires when segments empty (the counterfactual: with
    /// the live-count proviso miswired it stays zero and the test goes red). A
    /// plain relaxed atomic — diagnostic only, no ordering obligation.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_decommit_count() -> u64 {
        DECOMMIT_CALLS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// TEST-ONLY (Phase 35): the owner-only `live_count` of `ptr`'s segment, or
    /// `None` if `ptr` is foreign / not small/primordial. Lets the soak test
    /// assert a segment reaches `live_count == 0` before decommit.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_live_count_for(&self, ptr: *mut u8) -> Option<u32> {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base(base) {
            return None;
        }
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return None;
        }
        Some(SegmentMeta::new(base).live_count_of())
    }

    /// TEST-ONLY (Phase 35): whether `ptr`'s segment is currently decommitted, or
    /// `None` if `ptr` is foreign / not small/primordial.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_is_decommitted_for(&self, ptr: *mut u8) -> Option<bool> {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base(base) {
            return None;
        }
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return None;
        }
        Some(SegmentMeta::new(base).is_decommitted())
    }

    /// Decommit an empty small segment's payload and reset it to a clean blank.
    /// Precondition (caller's invariant): `live_count == 0` for this segment, so
    /// the entire payload `[small_meta_end, SEGMENT)` holds no live block.
    ///
    /// Steps (design §3):
    ///   1. Return the payload pages `[small_meta_end, SEGMENT)` to the OS. The
    ///      metadata pages (header / page-map / bin-table / alloc-bitmap / ring)
    ///      stay committed — cross-thread readers touch them, and `recycle` will
    ///      read the header reservation info AFTER this function returns.
    ///   2. Reset the segment to clean-empty: `bump = small_meta_end`, every
    ///      `BinTable` head = `FREE_LIST_NULL`, every payload page-map entry =
    ///      `Free`, the alloc bitmap = all-zeros. Safe because `live_count == 0`:
    ///      no block is live, every free-list node we are dropping is itself free.
    ///   3. Set the `decommitted` flag so the next carve recommits first.
    ///
    /// **Slot recycle** (task #60) is NOT done here — it happens after the
    /// drain loop that called `reclaim_offset` finishes (so that subsequent
    /// stale ring entries for the same segment still find the metadata
    /// readable). The caller is responsible for calling `self.table.recycle(base)`
    /// once no further `reclaim_offset` calls will target `base`. See
    /// `dealloc_small` and `find_segment_with_free` for the two call sites.
    #[cfg(feature = "alloc-decommit")]
    fn decommit_empty_segment(meta: &mut SegmentMeta, base: *mut u8) {
        // Test seam: count the invocation (diagnostic; relaxed).
        DECOMMIT_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let payload_start = SegLayout::small_meta_end();
        // 1. Return the payload pages to the OS (no-op under miri).
        os::decommit_pages(base, payload_start, SEGMENT);
        // 2a. Reset the bump cursor to the payload start (segment is blank). This
        //     is the load-bearing reset for the post-decommit stale-free guard:
        //     after this, every prior block offset in the payload is `>= bump`, so
        //     a late free / double-free / stale reclaim targeting this segment is
        //     rejected by the `off >= bump` check in `dealloc_small` /
        //     `reclaim_offset` BEFORE it writes a `next` pointer into a (now
        //     decommitted / unmapped) payload page.
        meta.set_bump(payload_start);
        // 2b. Empty every class free list.
        let mut bt = meta.bin_table();
        for c in 0..super::size_classes::SMALL_CLASS_COUNT {
            bt.set_head(c, FREE_LIST_NULL);
        }
        // 2c. Re-mark every payload page `Free` in the page map (metadata pages
        //     keep their `Meta` marking). Payload pages are `[meta_pages,
        //     PAGES_PER_SEGMENT)`.
        let mut pm = meta.page_map();
        let meta_pages = SegLayout::small_meta_pages();
        for p in meta_pages..super::segment_header::PAGES_PER_SEGMENT {
            pm.set_free(p);
        }
        // 2d. Zero the alloc bitmap (every slot "allocated / not-a-block" — the
        //     init state; with no live blocks and an empty free list this is the
        //     correct clean state). Re-init in place over the bitmap bytes.
        super::alloc_bitmap::AllocBitmap::init_in_place(Node::offset(
            base,
            SegLayout::alloc_bitmap_off(),
        ));
        // 3. Flag the segment decommitted so the next `carve_block` recommits.
        meta.set_decommitted(true);
    }

    /// TEST-ONLY (Phase B/C): the NUMA `node_id` stored in `ptr`'s segment
    /// header, or `None` if `ptr` is foreign. Returns `u32::MAX` (`NO_NODE_RAW`)
    /// for a segment that was not bound to a specific NUMA node (e.g. on a
    /// non-NUMA platform, or when `numa-aware` is off). The field is present in
    /// EVERY build's layout (layout-stable across feature configs); this accessor
    /// is only compiled under `numa-aware` because the test that reads it is also
    /// gated on that feature.
    #[doc(hidden)]
    #[cfg(feature = "numa-aware")]
    pub fn dbg_node_id_for(&self, ptr: *mut u8) -> Option<u32> {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base(base) {
            return None;
        }
        Some(SegmentMeta::new(base).node_id_of())
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
        let base = os::segment_base_of_ptr(ptr);
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
        // Collect bases first so we can call `self.table.recycle` after each
        // drain without conflicting with a concurrent bases() iterator borrow.
        let mut bases_buf = [core::ptr::null_mut::<u8>(); MAX_SEGMENTS];
        let mut n = 0usize;
        for base in self.table.bases() {
            if n < MAX_SEGMENTS {
                bases_buf[n] = base;
                n += 1;
            }
        }
        for &base in &bases_buf[..n] {
            let hdr = SegmentHeader::read_at(base);
            if !matches!(hdr.kind, SegmentKind::Small | SegmentKind::Primordial) {
                continue;
            }
            let ring = SegmentMeta::new(base).remote_ring();
            let small_cur = self.small_cur;
            #[cfg(feature = "alloc-decommit")]
            let mut decommit_happened = false;
            ring.drain(|off| {
                #[cfg(feature = "alloc-decommit")]
                if Self::reclaim_offset(base, off, small_cur) {
                    decommit_happened = true;
                }
                #[cfg(not(feature = "alloc-decommit"))]
                {
                    let _ = Self::reclaim_offset(base, off, small_cur);
                }
            });
            #[cfg(feature = "alloc-decommit")]
            if decommit_happened {
                self.table.recycle(base);
            }
        }
    }

    /// TEST-ONLY (Phase 13.3): reveal the size class `page_map` would assign
    /// to `ptr`'s page, so the counterfactual test for "own-thread dealloc
    /// derives the class from `Layout`, not `page_map`" can prove it is
    /// non-vacuous. Returns `None` if `ptr` is foreign, the segment is not
    /// small/primordial, or the page is uncarved. This is the (now-removed)
    /// `page_map`-class derivation the old intrusive-TFS drain used — kept here
    /// as a pure read so the test can prove the Layout-class and page_map-class
    /// genuinely differ on a mixed-class page (the §13 counterfactual).
    /// `#[doc(hidden)] pub` per the established test-only surface.
    #[doc(hidden)]
    pub fn dbg_page_map_class_for(&self, ptr: *mut u8) -> Option<usize> {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base(base) {
            return None;
        }
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
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
    /// **OPT-F — in-place small→small realloc:** when both the old and new
    /// sizes resolve to the same size class (or the new class is smaller —
    /// i.e. `new_class_idx <= old_class_idx`), the block physically fits the
    /// new size without any data movement. In that case we return the original
    /// pointer unchanged: no alloc, no copy, no dealloc. The block's live-count
    /// and alloc-bitmap stay intact (the block is still "live" under the same
    /// segment, just now described by a smaller `Layout`).
    ///
    /// The short-circuit applies ONLY to small (non-large) segments. Large
    /// blocks occupy a dedicated segment and there is no class to compare
    /// against, so they always take the full alloc+copy+dealloc path.
    ///
    /// On growth the new tail is **uninitialised** (matching `GlobalAlloc`).
    /// Returns null on failure, leaving the old allocation intact. Safe: a
    /// null `ptr` returns null without touching state.
    pub fn realloc(&mut self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return core::ptr::null_mut();
        }
        // OPT-F: in-place short-circuit for small→small realloc.
        //
        // Preconditions (all must hold to take the fast path):
        //   1. The pointer lives in one of OUR segments (registered in the table).
        //   2. The segment kind is Small or Primordial (has a BinTable / class).
        //   3. Both the old layout and the new size classify as Small (not Large).
        //   4. new_class_idx <= old_class_idx → the block's physical storage
        //      already fits `new_size` bytes.
        //
        // When all four hold we return `ptr` unchanged. No copy is needed
        // because the block has not moved; no dealloc because we are reusing it.
        // The alloc-bitmap and live-count are unaffected (the block stays live).
        {
            let base = os::segment_base_of_ptr(ptr);
            if self.table.contains_base(base)
                && matches!(
                    SegmentHeader::kind_at(base),
                    SegmentKind::Small | SegmentKind::Primordial
                )
            {
                let old_size = old_layout.size().max(super::size_classes::MIN_BLOCK);
                let align = old_layout.align();
                let clamped_new = new_size.max(super::size_classes::MIN_BLOCK);
                if let (Some(old_class), Some(new_class)) = (
                    super::size_classes::SizeClasses::class_for(old_size, align),
                    super::size_classes::SizeClasses::class_for(clamped_new, align),
                ) {
                    if new_class <= old_class {
                        // The new size fits inside the existing block.
                        return ptr;
                    }
                }
                // Falls through: new_class > old_class (growth into a larger class),
                // OR one of them is Large (class_for returned None). Take slow path.
            }
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
    fn carve_block_with_refill(&mut self, class_idx: usize, block_size: usize) -> Option<*mut u8> {
        // Carve the caller's block first.
        let first = self.carve_block(class_idx, block_size)?;
        // Refill batch: carve extra blocks and push each into its OWN segment.
        // `carve_block` returns None when the current segment is full; we stop
        // the batch there (the next alloc will reserve a fresh segment).
        //
        // Size chosen by measurement (Phase 13.5, task #29). Swept
        // {31, 63, 127, 255, 511} over the MT macro-bench (larson + mstress,
        // T=1/2/4 ops/sec — the load where refill actually bites) and the
        // single-threaded fixed-size churn micro-bench. Result: 31 is the
        // throughput winner. Larger batches do NOT help — they monotonically
        // HURT larson (working-set churn): T1/T2 larson fell from ~21–25 M to
        // ~14–18 M at 127–511, because a free-list miss now does up to 8×–16×
        // more upfront carve work (page faults, page-map writes) that the
        // steady-state churn never amortises. mstress was within noise and the
        // single-threaded churn was flat (~23–24 µs at every value — it pops
        // from the free list and never re-enters the cold carve). The §3.5
        // "raise toward a page of blocks (256–512)" hypothesis did not hold
        // under measurement; 31 stays. (Bigger upfront carve = worse locality
        // for the churn pattern, not better.)
        const REFILL_BATCH: usize = 31;
        for _ in 0..REFILL_BATCH {
            let Some(extra) = self.carve_block(class_idx, block_size) else {
                break;
            };
            let base = os::segment_base_of_ptr(extra);
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
    /// class.
    ///
    /// ## Slot recycle integration (task #60, `alloc-decommit`)
    ///
    /// Under `alloc-xthread` + `alloc-decommit`, the ring drain inside this
    /// function may trigger `dec_live_and_maybe_decommit` (via `reclaim_offset`)
    /// which decommits an empty segment. Slot recycling — `self.table.recycle(base)`
    /// — is deferred until AFTER the drain for that `base` is complete. This is
    /// critical: a partially-drained ring still has ring entries that
    /// `reclaim_offset` processes by reading the segment's metadata (which stays
    /// committed). Recycling before the drain ends would release the OS
    /// reservation prematurely — the metadata read in `magic_at` / `kind_at`
    /// would UAF. By recycling after the drain, we ensure:
    ///   a. All ring entries for `base` are processed (or safely skipped via
    ///      the `off >= bump` guard — bump was reset by decommit).
    ///   b. The OS release + slot NULL happen atomically in `recycle`, with no
    ///      window where the slot is non-NULL but the OS segment is gone.
    pub(crate) fn find_segment_with_free(&mut self, class_idx: usize) -> Option<*mut u8> {
        // Collect all registered live segment bases FIRST (into a fixed-size
        // stack array) so we can iterate without holding a `&self.table` borrow —
        // which would block the `&mut self.table.recycle(...)` call needed for
        // slot recycle under `alloc-decommit`. MAX_SEGMENTS is the capacity bound;
        // only live (non-NULL) bases are stored, so `n <= live_count <= MAX_SEGMENTS`.
        let mut bases_buf = [core::ptr::null_mut::<u8>(); MAX_SEGMENTS];
        let mut n = 0usize;
        for base in self.table.bases() {
            if n < MAX_SEGMENTS {
                bases_buf[n] = base;
                n += 1;
            }
        }

        // Phase C (numa-aware): on the first pass we prefer segments whose
        // node_id matches the calling thread's NUMA node; we collect segments
        // from foreign nodes in `fallback` and return the first one only if
        // the first pass found nothing.
        //
        // Strategy (a) — "ignore migration": we call current_node() once per
        // find_segment_with_free invocation (not per allocation). If the thread
        // migrated between nodes mid-scan, we may prefer a now-wrong segment —
        // that is the accepted MVP trade-off (§4 of PHASE_NUMA_DESIGN.md).
        #[cfg(feature = "numa-aware")]
        let my_node = numa::current_node();
        // A single fallback slot: the first segment from a foreign node that has
        // a free block.  On a single-NUMA machine (or when numa-aware is off)
        // this path is never taken — all segments have node_id == my_node (or
        // NO_NODE_RAW, which is treated as "acceptable" / unknown).
        #[cfg(feature = "numa-aware")]
        let mut fallback: Option<*mut u8> = None;

        for &base in &bases_buf[..n] {
            // Skip large/huge segments: they have no BinTable. Field-specific
            // `kind` read (task #33): this is the Owner's alloc path,
            // concurrent with a Remote's `dealloc_routing` field reads — a
            // full-struct `read_at` here would race them. `kind_at` reads only
            // the `kind` byte, disjoint from any writer.
            if !matches!(
                SegmentHeader::kind_at(base),
                SegmentKind::Small | SegmentKind::Primordial
            ) {
                continue;
            }
            // Variant-2: lazily drain this segment's remote-free ring before
            // inspecting its BinTable. Cross-thread frees that targeted THIS
            // segment (a segment we own but are not currently allocating from)
            // are sitting in its ring; without this drain they would never
            // reach the BinTable and the scan would miss them.
            //
            // Under `alloc-decommit`, if draining empties the segment it is
            // decommitted inside `reclaim_offset`. We track whether a decommit
            // fired via the `decommit_happened` flag, then recycle the slot
            // AFTER the drain completes — not during — so that any remaining
            // ring entries for `base` can still safely read the (still-committed)
            // metadata via `magic_at`/`kind_at`/`bump_of`.
            #[cfg(feature = "alloc-xthread")]
            {
                let ring = SegmentMeta::new(base).remote_ring();
                let small_cur = self.small_cur;
                #[cfg(feature = "alloc-decommit")]
                let mut decommit_happened = false;
                ring.drain(|off| {
                    #[cfg(feature = "alloc-decommit")]
                    if Self::reclaim_offset(base, off, small_cur) {
                        decommit_happened = true;
                    }
                    #[cfg(not(feature = "alloc-decommit"))]
                    {
                        let _ = Self::reclaim_offset(base, off, small_cur);
                    }
                });
                // Slot recycle: now that the drain is complete, it is safe to
                // release the OS reservation and NULL the slot. Any stale ring
                // entries have already been processed (and guarded by `off >= bump`).
                #[cfg(feature = "alloc-decommit")]
                if decommit_happened {
                    self.table.recycle(base);
                    // This base is now recycled; skip the BinTable check.
                    continue;
                }
            }
            let meta = SegmentMeta::new(base);
            let bt = meta.bin_table();
            if bt.head(class_idx) != FREE_LIST_NULL {
                // Phase C (numa-aware): check whether this segment belongs to
                // our NUMA node.  Segments with node_id == NO_NODE_RAW are
                // "unknown" — treat them as local (no penalty, and on platforms
                // without NUMA they all carry NO_NODE_RAW so this degrades
                // gracefully to the pre-NUMA single-pass behaviour).
                #[cfg(feature = "numa-aware")]
                {
                    let seg_node = meta.node_id_of();
                    if seg_node != my_node && seg_node != super::segment_header::NO_NODE_RAW {
                        // Foreign-node segment with a free block.  Remember as
                        // fallback if we find nothing local, then keep scanning.
                        if fallback.is_none() {
                            fallback = Some(base);
                        }
                        continue;
                    }
                    // Local or unknown node — use it immediately.
                    return Some(base);
                }
                // Without numa-aware: same as before — return the first match.
                #[cfg(not(feature = "numa-aware"))]
                return Some(base);
            }
        }
        // First pass found no local segment with a free block; fall back to
        // the first foreign-node segment we recorded (or None if everything is
        // empty / all recycled).
        #[cfg(feature = "numa-aware")]
        return fallback;
        #[cfg(not(feature = "numa-aware"))]
        None
    }

    /// Pop a free block of `class_idx` from `segment`'s bin table. Returns
    /// null if the free list is empty. Writes the block's `next` word to null
    /// (it becomes the new head) via the node seam.
    #[inline]
    fn pop_free(&self, segment: *mut u8, class_idx: usize, block_size: usize) -> Option<*mut u8> {
        #[cfg(feature = "alloc-decommit")]
        let mut meta = SegmentMeta::new(segment);
        #[cfg(not(feature = "alloc-decommit"))]
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
        // Phase 35 (M6): a block left the free list and is handed to the caller
        // → one more live block in this segment. Owner-only counter. A popped
        // block always comes from a COMMITTED payload (a decommitted segment was
        // reset to an empty free list, so `pop_free` finds nothing there), so no
        // recommit is needed on this path — only `carve_block` writes fresh
        // payload and thus recommits.
        #[cfg(feature = "alloc-decommit")]
        meta.inc_live();
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
        // Phase 35 (M6 recommit): if this segment's payload was decommitted (it
        // emptied and we returned its pages to the OS), we are about to write
        // into the payload — recommit the whole payload range and clear the flag
        // BEFORE the bump cursor advances / the page-map / the block is touched.
        // The reset that accompanied decommit left `bump == small_meta_end`, so
        // a decommitted segment is always carved from its payload start; the
        // simplest correct recommit is the whole `[small_meta_end, SEGMENT)`
        // payload at once (per §4 of the design — pessimistic but correct, and
        // a recommit only happens on the first reuse after an empty→decommit).
        #[cfg(feature = "alloc-decommit")]
        if meta.is_decommitted() {
            os::recommit_pages(segment, SegLayout::small_meta_end(), SEGMENT);
            meta.set_decommitted(false);
        }
        // Update ONLY the bump cursor.
        meta.set_bump(aligned_bump + block_size);
        // Phase 35: this carved block is now live (handed to the caller, or — on
        // the refill path — immediately pushed to the free list, which calls
        // `dealloc_small` → `dec_live`, netting zero for refill blocks; the
        // caller's block keeps the +1). Owner-only counter, plain field bump.
        #[cfg(feature = "alloc-decommit")]
        meta.inc_live();
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
        // Phase 35 (M6 decommit) — the post-decommit stale-free guard. When a
        // segment empties it is decommitted AND reset: `bump` returns to
        // `small_meta_end()` and the alloc bitmap is zeroed. A late free / a
        // legitimate double-free of a block that lived in the now-decommitted
        // payload would (a) pass the zeroed bitmap `is_free` check and (b)
        // `write_next` into a DECOMMITTED / unmapped page — a UAF. Every block
        // that was ever carved has `off >= bump` ONLY after such a reset (a live
        // block in a committed segment always has `off < bump`); so rejecting
        // `off >= bump` closes the window with no false positive on a real free.
        // Owner-only `bump` read (single-writer), gated to the feature that
        // resets the bump.
        #[cfg(feature = "alloc-decommit")]
        if (off as usize) >= meta.bump_of() {
            return;
        }
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
        // Phase 35 (M6): one fewer live block in this segment; decommit if it
        // just emptied and is not the current carve target. Own-thread free runs
        // on the owner, so the counter stays single-writer.
        // Task #60 (slot recycle): if decommit fired, recycle the table slot
        // immediately — `dealloc_small` is NOT inside a ring drain (no stale
        // ring entries arrive here for `base` on the own-thread path), so the
        // metadata is readable, the slot can be NULLed, and the OS reservation
        // can be released right away.
        #[cfg(feature = "alloc-decommit")]
        if Self::dec_live_and_maybe_decommit(base, self.small_cur) {
            self.table.recycle(base);
        }
    }

    /// Allocate a large/huge block: reserve a dedicated segment sized to fit,
    /// place the allocation at the first page-aligned offset past the header,
    /// register the segment, and return the allocation pointer.
    ///
    /// **OPT-E (alloc-decommit):** before going to the OS, check the
    /// `large_cache` for a previously-freed segment that is large enough to
    /// satisfy the request. A cache hit avoids the full OS round-trip
    /// (mmap/VirtualAlloc + registration) at the cost of one recommit call
    /// (Windows only; unix is a no-op after MADV_DONTNEED).
    fn alloc_large(&mut self, size: usize, align: usize) -> *mut u8 {
        // The segment must hold: header + alignment padding + size, rounded up
        // to a whole number of segments. `Segment::reserve` does the rounding.
        let hdr_aligned = align_up(
            core::mem::size_of::<SegmentHeader>(),
            align.max(super::os::PAGE),
        );
        let needed = hdr_aligned + align_up(size, align);
        // Round up to a whole number of SEGMENT-sized spans — the same rounding
        // `Segment::reserve` does internally.  `reserve_aligned_on_node` (like
        // the OS `mmap`/`VirtualAlloc` path) requires the usable size to be an
        // exact multiple of SEGMENT so the over-reserve + trim arithmetic holds:
        //   base_addr + usable <= region_addr + over   (over = usable * 2)
        // With an un-rounded `needed` this can fail if `needed < SEGMENT` and
        // `align_up(region_addr, SEGMENT)` skips a large head region.
        let n_segments = needed.div_ceil(SEGMENT);
        let usable = n_segments * SEGMENT;

        // OPT-E: try the large-segment cache first.
        // Scan all slots for a compatible entry: usable_size >= usable (the
        // cached segment is big enough) AND usable_size <= usable *
        // LARGE_CACHE_SIZE_FACTOR (not so big we waste RSS). The size-ratio
        // bound prevents a 64 MiB cached segment from permanently absorbing
        // every 4 MiB request.
        #[cfg(feature = "alloc-decommit")]
        {
            let mut hit_idx: Option<usize> = None;
            for i in 0..LARGE_CACHE_SLOTS {
                if let Some(ref slot) = self.large_cache[i] {
                    if slot.usable_size >= usable
                        && slot.usable_size <= usable.saturating_mul(LARGE_CACHE_SIZE_FACTOR)
                    {
                        hit_idx = Some(i);
                        break;
                    }
                }
            }
            if let Some(idx) = hit_idx {
                let slot = self.large_cache[idx].take().unwrap();
                // Re-register the base in the segment table. Under
                // alloc-decommit, recycle() left a NULL slot that register()
                // will reuse — so this should not fail. If it does (table is
                // genuinely full) we cannot reuse this slot; release it and
                // fall through to the slow OS path.
                let id = match self.table.register(slot.base) {
                    Some(id) => id,
                    None => {
                        // Table still full: release the cached reservation and
                        // fall through to the slow path.
                        os::release_segment(slot.reservation, slot.reservation_len);
                        // Fall through to OS path below.
                        return self.alloc_large_slow(size, align, usable, hdr_aligned);
                    }
                };
                // Pages are kept committed in the cache (no decommit on deposit,
                // no recommit needed on hit — they are already mapped and
                // accessible). Just write a fresh header and return.
                // Write a fresh header over the old one. The allocation lives
                // at hdr_aligned (same computation as the slow path).
                let bump = hdr_aligned + align_up(size, align);
                let hdr = SegmentHeader::large(
                    id,
                    size,
                    align,
                    bump,
                    slot.reservation,
                    slot.reservation_len,
                );
                Node::write_struct(slot.base as *mut SegmentHeader, hdr);
                // Phase C (numa-aware): re-stamp with the CURRENT thread's NUMA
                // node. The thread may have migrated since the segment was cached;
                // updating the tag reflects the current physical binding.
                #[cfg(feature = "numa-aware")]
                {
                    let my_node = numa::current_node();
                    SegmentMeta::new(slot.base).set_node_id(my_node);
                }
                return Node::deref(slot.base, hdr_aligned);
            }
        }

        self.alloc_large_slow(size, align, usable, hdr_aligned)
    }

    /// The slow (OS round-trip) path for `alloc_large` — called when the
    /// `large_cache` has no matching entry. Factored out so the cache-hit path
    /// can call `return self.alloc_large_slow(...)` cleanly when the table is
    /// full (avoiding a goto / code-duplication).
    fn alloc_large_slow(
        &mut self,
        size: usize,
        align: usize,
        usable: usize,
        hdr_aligned: usize,
    ) -> *mut u8 {
        // Phase C (numa-aware): steer the large segment to the calling thread's
        // NUMA node, same as for small segments.
        #[cfg(feature = "numa-aware")]
        let my_node = numa::current_node();

        #[cfg(feature = "numa-aware")]
        let (base, reservation, reservation_len) = {
            match numa::reserve_aligned_on_node(usable, my_node) {
                Some((b, r, rl)) => (b.as_ptr(), r, rl),
                None => return core::ptr::null_mut(),
            }
        };
        #[cfg(not(feature = "numa-aware"))]
        let (base, reservation, reservation_len) = {
            let segment = match Segment::reserve(usable) {
                Some(s) => s,
                None => return core::ptr::null_mut(),
            };
            let b = segment.as_ptr();
            let r = segment.reservation();
            let rl = segment.reservation_len();
            core::mem::forget(segment);
            (b, r, rl)
        };

        // no-panic: register returns None if the segment table is full (too many
        // live large allocations). We release the reservation and return null
        // (graceful OOM) rather than panicking.
        let id = match self.table.register(base) {
            Some(id) => id,
            None => {
                // Release the reservation we own.
                os::release_segment(reservation.as_ptr(), reservation_len);
                return core::ptr::null_mut();
            }
        };
        // Lay down the large header. The allocation lives at `hdr_aligned`.
        let bump = hdr_aligned + align_up(size, align);
        let hdr =
            SegmentHeader::large(id, size, align, bump, reservation.as_ptr(), reservation_len);
        Node::write_struct(base as *mut SegmentHeader, hdr);
        // Phase C (numa-aware): stamp the NUMA node into the header after
        // writing it (the constructor sets node_id to NO_NODE_RAW).
        #[cfg(feature = "numa-aware")]
        SegmentMeta::new(base).set_node_id(my_node);

        Node::deref(base, hdr_aligned)
    }

    /// Reserve a fresh small segment, initialise its metadata, register it,
    /// and set it as the current small segment. Returns its base.
    fn reserve_small_segment(&mut self) -> Option<*mut u8> {
        // Phase C (numa-aware): determine the calling thread's NUMA node
        // BEFORE the reservation so we can pass it to `reserve_aligned_on_node`
        // (Windows requires the node at reserve-time via VirtualAllocExNuma;
        // Linux can bind post-mmap, but we unify the paths here).
        #[cfg(feature = "numa-aware")]
        let my_node = numa::current_node();

        // Reserve one SEGMENT's worth of virtual address space.
        // Under numa-aware we call the NUMA-steering path; otherwise the plain
        // OS path.  The returned triple always provides (base, reservation,
        // reservation_len) with the same semantics as Segment::reserve.
        #[cfg(feature = "numa-aware")]
        let (base, reservation, reservation_len) = {
            let (b, r, rl) = numa::reserve_aligned_on_node(SEGMENT, my_node)?;
            (b.as_ptr(), r, rl)
        };
        #[cfg(not(feature = "numa-aware"))]
        let (base, reservation, reservation_len) = {
            let segment = Segment::reserve(SEGMENT)?;
            let b = segment.as_ptr();
            let r = segment.reservation();
            let rl = segment.reservation_len();
            core::mem::forget(segment);
            (b, r, rl)
        };

        // no-panic: register returns None if the segment table is full. We
        // must release the reservation we just made before returning None.
        let id = match self.table.register(base) {
            Some(id) => id,
            None => {
                // Release the reservation we just made (we own it now).
                os::release_segment(reservation.as_ptr(), reservation_len);
                return None;
            }
        };
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
        // Phase C (numa-aware): stamp the NUMA node into the header NOW,
        // immediately after writing it. The header constructor set node_id to
        // NO_NODE_RAW; we overwrite it with the actual node. This must happen
        // BEFORE any carve/alloc so that find_segment_with_free sees the real
        // node on the very first scan that includes this segment.
        #[cfg(feature = "numa-aware")]
        meta.set_node_id(my_node);

        PageMap::init_in_place(base_add(base, SegLayout::page_map_off()), meta_pages);
        BinTable::init_in_place(base_add(base, SegLayout::bin_table_off()) as *mut u32);
        // Initialise the per-segment alloc-bitmap (Phase 13.4a double-free
        // guard) to all-zeros; bits flip to FREE as blocks are pushed.
        super::alloc_bitmap::AllocBitmap::init_in_place(base_add(
            base,
            SegLayout::alloc_bitmap_off(),
        ));
        // Initialise the per-segment remote-free ring (Variant-2 fix). Only
        // under `alloc-xthread`; the Layout always reserves the bytes.
        #[cfg(feature = "alloc-xthread")]
        {
            super::remote_free_ring::RemoteFreeRing::init_in_place(
                base,
                SegLayout::remote_ring_off(),
            );
        }
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
        // OPT-E (alloc-decommit): release any large segments held in the
        // free-cache BEFORE walking the segment table. The cached entries are
        // NOT in the table (they were unregistered on deposit), so the normal
        // `table.bases()` walk below won't see them. We must release them
        // explicitly here or they would leak.
        #[cfg(feature = "alloc-decommit")]
        for slot in &mut self.large_cache {
            if let Some(cached) = slot.take() {
                os::release_segment(cached.reservation, cached.reservation_len);
            }
        }

        // Collect every live segment's `(reservation, reservation_len)` into a
        // fixed-size stack array FIRST, then free them all. We must NOT free
        // the primordial segment while still reading the registry — the
        // registry lives IN the primordial's payload, so freeing it would
        // unmap the array we're iterating over. Collecting up front (into a
        // stack array, no global-allocator involvement) breaks that aliasing.
        //
        // `self.table.bases()` already filters NULL (recycled) slots — those
        // segments were released by `recycle()` during their decommit cycle and
        // must NOT be freed again. Only non-NULL (live) segments are collected
        // and freed here.
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
