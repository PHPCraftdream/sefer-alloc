//! Large-path cluster of [`AllocCore`] (mechanical split of `alloc_core.rs`).
//!
//! This file holds an additional `impl AllocCore { .. }` block carrying the
//! large/huge alloc + reclaim methods. It is a pure code-movement sibling of
//! `alloc_core.rs`; no behavior changed.

use super::node::Node;
#[cfg(feature = "numa-aware")]
use super::numa;
#[cfg(not(feature = "numa-aware"))]
use super::os::Segment;
use super::os::{self, SEGMENT};
#[cfg(feature = "numa-aware")]
use super::segment_header::SegmentMeta;
use super::segment_header::{align_up, SegmentHeader};

use super::alloc_core::AllocCore;
#[cfg(feature = "alloc-decommit")]
use super::alloc_core::{CachedLarge, LARGE_CACHE_SIZE_FACTOR, LARGE_CACHE_SLOTS};

impl AllocCore {
    /// Allocate a large/huge block: reserve a dedicated segment sized to fit,
    /// place the allocation at the first page-aligned offset past the header,
    /// register the segment, and return the allocation pointer.
    ///
    /// **OPT-E (alloc-decommit):** before going to the OS, check the
    /// `large_cache` for a previously-freed segment that is large enough to
    /// satisfy the request. A cache hit avoids the full OS round-trip
    /// (mmap/VirtualAlloc + registration) at the cost of one recommit call
    /// (Windows only; unix is a no-op after MADV_DONTNEED).
    ///
    /// **Phase 2 (alloc-decommit):** runs one lazy decay tick before serving
    /// the request. Cost: one `Instant::now()` + one duration compare on the
    /// common path; actual eviction only when the interval has elapsed AND the
    /// cache is over the headroom target.
    pub(super) fn alloc_large(&mut self, size: usize, align: usize) -> *mut u8 {
        // align >= SEGMENT is not serviceable by the dedicated-segment large
        // path: the block would land at base + SEGMENT-multiple (mis-registered
        // → dealloc leak → eventual MAX_SEGMENTS abort) or, for align >
        // SEGMENT, at a pointer only SEGMENT-aligned (GlobalAlloc contract
        // violation → UB). Reject with null — a legal alloc-failure signal —
        // rather than leak/misalign. (Task #130.)
        if align >= SEGMENT {
            return core::ptr::null_mut();
        }

        // Phase 2: lazy decay tick on every large allocation.
        #[cfg(feature = "alloc-decommit")]
        self.maybe_decay_large_cache();

        // The segment must hold: header + alignment padding + size, rounded up
        // to a whole number of segments. `Segment::reserve` does the rounding.
        let hdr_aligned = align_up(
            core::mem::size_of::<SegmentHeader>(),
            align.max(super::os::PAGE),
        );
        // task #25 (security): `checked_add` for local overflow safety — a
        // wrap here is unreachable under the `Layout` size/align invariant
        // today, but this no longer RELIES on the caller's `Layout` being
        // well-formed (parity with the realloc path, which already uses
        // `checked_add`). A wrap → null (a legal alloc-failure signal).
        let needed = match hdr_aligned.checked_add(align_up(size, align)) {
            Some(n) => n,
            None => return core::ptr::null_mut(),
        };
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
            // G11 (task #51) — BEST-FIT: scan ALL slots and pick the compatible
            // entry with the SMALLEST `usable_size`, instead of taking the first
            // fit. A cached entry is compatible when it is big enough
            // (`usable_size >= usable`) yet not wastefully large
            // (`usable_size <= usable * LARGE_CACHE_SIZE_FACTOR`). Best-fit keeps
            // the tightest span for this request and leaves the larger cached
            // spans available for larger future requests — reducing internal
            // fragmentation / RSS waste versus first-fit, at O(LARGE_CACHE_SLOTS)
            // (8) cost on the cold large-alloc path (negligible).
            let mut hit_idx: Option<usize> = None;
            let mut best_usable: usize = usize::MAX;
            for i in 0..LARGE_CACHE_SLOTS {
                if let Some(ref slot) = self.large_cache[i] {
                    if slot.usable_size >= usable
                        && slot.usable_size <= usable.saturating_mul(LARGE_CACHE_SIZE_FACTOR)
                        && slot.usable_size < best_usable
                    {
                        best_usable = slot.usable_size;
                        hit_idx = Some(i);
                    }
                }
            }
            if let Some(idx) = hit_idx {
                let slot = self.large_cache[idx].take().unwrap();
                // Diagnostic (task D1): count this as a cache hit.
                // Э5 (task #145): load+store instead of `fetch_add` — no
                // `lock xadd`. SOUND for the same single-writer reason as
                // `HeapCore::tcache_hits`: the counter is per-heap and
                // `alloc_large` (its only incrementer) runs solely on the
                // owning thread (the slot's claim-CAS winner). No other thread
                // writes it, so splitting the atomic RMW into Relaxed load +
                // Relaxed store cannot drop a count. The cross-thread
                // `large_cache_hits_total` reader still does a Relaxed atomic
                // load — identical visibility to the old `fetch_add(Relaxed)`.
                //
                // W3: increment the SLOT's counter when this heap is bound
                // (`large_cache_hits_sink`), else the owned fallback (standalone
                // `AllocCore`). Same 2 mem-ops either way. Safe references
                // throughout (forbid-unsafe).
                //
                // W3 Part B: gated behind `alloc-stats` (default OFF, NOT in
                // `production`) — when off it compiles OUT of the large-cache
                // hit path and `stats().large_cache_hits` reads 0. See the
                // `alloc-stats` feature doc in Cargo.toml.
                #[cfg(feature = "alloc-stats")]
                {
                    let ctr = self.large_cache_hits_sink.unwrap_or(&self.large_cache_hits);
                    ctr.store(
                        ctr.load(core::sync::atomic::Ordering::Relaxed)
                            .wrapping_add(1),
                        core::sync::atomic::Ordering::Relaxed,
                    );
                }
                // Update the byte-budget counter: this slot is leaving the cache.
                self.large_cache_used_bytes =
                    self.large_cache_used_bytes.saturating_sub(slot.usable_size);
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
                // `span_usable` is carried forward from the CACHED slot's own
                // `usable_size` — the true physical span of the segment being
                // reused — NOT recomputed from the new (possibly smaller)
                // `size`/`align`. Bug #134.
                let hdr = SegmentHeader::large(
                    id,
                    size,
                    align,
                    slot.usable_size,
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
            let reserved = numa::reserve_aligned_on_node(usable, my_node);
            // Mechanism 2 (task #51): if the OS refused the reservation, the
            // small-segment hysteresis pool may be holding committed memory the
            // OS could hand back — drain it and retry ONCE before conceding OOM.
            // This makes the pool a reclaimable SOFT reserve, not a hard pin: a
            // large allocation that would otherwise succeed is never starved by
            // committed-but-idle pooled small segments.
            #[cfg(feature = "alloc-decommit")]
            let reserved = match reserved {
                Some(t) => Some(t),
                None if self.pooled_count > 0 => {
                    self.drain_small_pool();
                    numa::reserve_aligned_on_node(usable, my_node)
                }
                None => None,
            };
            match reserved {
                Some((b, r, rl)) => (b.as_ptr(), r, rl),
                None => return core::ptr::null_mut(),
            }
        };
        #[cfg(not(feature = "numa-aware"))]
        let (base, reservation, reservation_len) = {
            let mut seg = Segment::reserve(usable);
            // Mechanism 2 (task #51): pool-drain-and-retry on OS-reservation
            // failure — see the numa-aware arm above for the rationale (the pool
            // is a reclaimable soft reserve, not a hard pin).
            #[cfg(feature = "alloc-decommit")]
            if seg.is_none() && self.pooled_count > 0 {
                self.drain_small_pool();
                seg = Segment::reserve(usable);
            }
            let segment = match seg {
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
        // Fresh reservation: `span_usable` = the just-computed physical
        // usable span (`usable`) — this is the ORIGINAL stamping that every
        // later cache-hit reuse of this segment will carry forward verbatim.
        let hdr = SegmentHeader::large(
            id,
            size,
            align,
            usable,
            bump,
            reservation.as_ptr(),
            reservation_len,
        );
        Node::write_struct(base as *mut SegmentHeader, hdr);
        // Phase C (numa-aware): stamp the NUMA node into the header after
        // writing it (the constructor sets node_id to NO_NODE_RAW).
        #[cfg(feature = "numa-aware")]
        SegmentMeta::new(base).set_node_id(my_node);

        Node::deref(base, hdr_aligned)
    }

    /// Reclaim a Large/huge segment that was freed by a REMOTE thread (0.3.0,
    /// task A1). `base` MUST be a currently-registered `Large`-kind segment
    /// base owned by this `AllocCore` — its header's `magic`/`kind` are still
    /// intact (a cross-thread free never zeroes them; only the OWNER's
    /// own-thread `dealloc` does that, on the path this function replaces for
    /// the remote case).
    ///
    /// Removes `base` from the segment table (freeing its slot for reuse —
    /// this is the fix for the permanent `SegmentTable` slot pin described in
    /// the A1 bug) and either:
    /// - (`alloc-decommit`) deposits the reservation into `large_cache`, same
    ///   admission policy as the own-thread large-dealloc path, so a
    ///   same-size `alloc_large` can reuse it without an OS round-trip; or
    /// - (no `alloc-decommit`) releases the OS reservation immediately via
    ///   `os::release_segment`, matching the own-thread path's behaviour
    ///   without the cache (own-thread `dealloc` there only zeroes the magic
    ///   and defers the release to `Drop`; here there is no `Drop` moment to
    ///   defer to mid-lifetime, and deferring would re-introduce the leak —
    ///   the slot must be freed NOW so `SegmentTable` capacity is not
    ///   permanently consumed by a segment nobody can address any more, since
    ///   we already removed it from the table above).
    ///
    /// Called by [`drain_large_deferred_free`](super::super::registry::heap_core::HeapCore)
    /// (via the `HeapCore` cross-thread reclaim path) on the owner's
    /// `alloc_large` slow-path, once per queued base.
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn reclaim_large_segment(&mut self, base: *mut u8) {
        let hdr = SegmentHeader::read_at(base);
        // Remove from the table FIRST (frees the slot for reuse regardless of
        // which branch below runs) — mirrors the own-thread cache-deposit
        // ordering in `dealloc`'s Large branch.
        self.table.unregister(base);

        #[cfg(feature = "alloc-decommit")]
        {
            self.maybe_decay_large_cache();
            // See the own-thread `dealloc` Large branch above (bug #134): the
            // physical usable span is read from the header's stable
            // `span_usable` field, not recomputed from `large_size`/
            // `large_align` (which can be stale-small after a cache-hit
            // reuse).
            let usable_size = hdr.span_usable;

            let mut admitted: Option<usize> = None;
            loop {
                let free_slot = self.large_cache.iter().position(|s| s.is_none());
                let budget_ok = self
                    .large_cache_budget_bytes
                    .is_none_or(|budget| self.large_cache_used_bytes + usable_size <= budget);
                if let Some(idx) = free_slot {
                    if budget_ok {
                        admitted = Some(idx);
                        break;
                    }
                }
                if !self.evict_one_oldest() {
                    break;
                }
            }

            if let Some(slot_idx) = admitted {
                let mut hdr_zero = hdr;
                hdr_zero.magic = 0;
                Node::write_struct(base as *mut SegmentHeader, hdr_zero);
                let seq = self.large_cache_seq;
                self.large_cache_seq = self.large_cache_seq.wrapping_add(1);
                self.large_cache[slot_idx] = Some(CachedLarge {
                    reservation: hdr.reservation,
                    reservation_len: hdr.reservation_len,
                    base,
                    usable_size,
                    seq,
                });
                self.large_cache_used_bytes += usable_size;
                return;
            }
        }

        // No `alloc-decommit` cache (or cache admission declined): release
        // the OS reservation immediately. The slot is already unregistered
        // above, so there is no dangling table entry pointing at unmapped
        // memory.
        os::release_segment(hdr.reservation, hdr.reservation_len);
    }
}
