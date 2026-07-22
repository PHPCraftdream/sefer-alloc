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

/// R12-4 (EXPERIMENTAL, feature `large-reserved-capacity`): the upper bound
/// on how large a Large segment's `reserved_capacity` may grow relative to
/// `SEGMENT` (4 MiB), regardless of the request size. Without a cap, a
/// pathologically large single request (e.g. 512 MiB) would reserve a 1 GiB
/// VA span "for future growth" that may never materialise — cheap in
/// address space on a 64-bit target, but not free (page-table / TLB
/// footprint for the reservation bookkeeping itself, and a bound the OS can
/// refuse on a constrained VA budget). 64 MiB (16 x SEGMENT) comfortably
/// covers the realloc-growth-chain scenario this feature targets (e.g. 256
/// KiB -> 4 MiB in successive doublings) while keeping the worst-case
/// reservation for any single request bounded and predictable.
///
/// `not(numa-aware)`: this constant is consumed only by `alloc_large_slow`'s
/// `not(numa-aware)` reservation arm — the `numa-aware` arm always uses the
/// eager `numa::reserve_aligned_on_node` path with `reserved_capacity ==
/// usable` (NUMA reservations are not disturbed by the lazy-capacity path,
/// same exclusion `Segment::reserve_capacity_exact` itself documents).
/// Gating the constant too (rather than leaving it defined-but-unused) keeps
/// `--all-features` (which enables `numa-aware` alongside
/// `large-reserved-capacity`) free of a dead-code warning.
#[cfg(all(feature = "large-reserved-capacity", not(feature = "numa-aware")))]
const LARGE_RESERVED_CAP_BYTES: usize = 16 * SEGMENT;

/// R12-4: geometric growth factor applied to the page-rounded request size
/// to compute the INITIAL `reserved_capacity` at first reservation. 2x
/// mirrors the classic amortised-doubling growth strategy (`Vec`, mimalloc's
/// own segment growth, etc.) — cheap to reason about, and empirically covers
/// one "grow to roughly double" realloc step without hitting the slow path.
///
/// `not(numa-aware)`: same gating rationale as
/// [`LARGE_RESERVED_CAP_BYTES`] above.
#[cfg(all(feature = "large-reserved-capacity", not(feature = "numa-aware")))]
const LARGE_RESERVED_CAP_GROWTH_FACTOR: usize = 2;

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
    ///
    /// # Freshness signal (task #221 / R8-8; miri fix R9-1)
    ///
    /// Returns `(*mut u8, bool)` where the bool — meaningful only when the
    /// pointer is non-null — is `true` iff the returned allocation lives in a
    /// GENUINELY FRESH OS reservation whose every byte the OS zero-fills by
    /// construction (Windows `VirtualAlloc` MEM_COMMIT, Unix zero-filled
    /// anonymous `mmap`). Under **miri** the bool is ALWAYS `false`, even for
    /// a fresh reservation: `crates/vmem`'s miri aperture falls back to bare
    /// `std::alloc::alloc`, which does NOT zero (vmem's own
    /// `leak_zeroed_pages` documents and works around exactly this), so a
    /// fresh miri reservation carries NO zero guarantee and the caller must
    /// zero explicitly. `alloc_large_slow` (the only producer of a fresh span)
    /// yields `cfg!(not(miri))`; a `large_cache` HIT (a reused,
    /// previously-freed segment that may still hold the prior occupant's
    /// bytes) yields `false` everywhere. Null returns carry `true` purely by
    /// convention — the caller trusts the bool ONLY for a non-null pointer, so
    /// the value for null is unobservable; `true` is chosen for uniformity.
    /// This is a conservative, SEGMENT-RESERVATION-level signal with no
    /// per-block bitmap and no interaction with any decommit/MADV_FREE reuse
    /// path.
    pub(crate) fn alloc_large(&mut self, size: usize, align: usize) -> (*mut u8, bool) {
        // align >= SEGMENT is not serviceable by the dedicated-segment large
        // path: the block would land at base + SEGMENT-multiple (mis-registered
        // → dealloc leak → eventual MAX_SEGMENTS abort) or, for align >
        // SEGMENT, at a pointer only SEGMENT-aligned (GlobalAlloc contract
        // violation → UB). Reject with null — a legal alloc-failure signal —
        // rather than leak/misalign. (Task #130.)
        if align >= SEGMENT {
            return (core::ptr::null_mut(), true);
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
            None => return (core::ptr::null_mut(), true),
        };
        // R12-3 (EXPERIMENTAL, feature `exact-span-large`): size the physical
        // reservation to the exact page-rounded request instead of rounding
        // up to a whole number of SEGMENT-sized (4 MiB) spans. Every Large
        // request today pays a MINIMUM of 4 MiB reserved+committed address
        // space (a 260 KiB request costs the same as a 4 MiB one); with the
        // page-exact span a 260 KiB request costs ~264 KiB instead.
        //
        // The OLD comment here claimed `reserve_aligned_on_node` (like the OS
        // `mmap`/`VirtualAlloc` path) REQUIRES the usable size to be an exact
        // multiple of SEGMENT for the over-reserve + trim arithmetic to hold
        // (`base_addr + usable <= region_addr + over`, `over = usable * 2`).
        // That is NOT a real backend constraint: both `crates/vmem` backends
        // (`win_reserve_commit` / `unix_reserve`) over-reserve `size + align`
        // (not `size * 2`) and trim to an `align`-aligned `size`-byte span
        // for ARBITRARY `size` — `try_reserve_aligned`'s only size contract
        // is "non-zero multiple of PAGE", never "multiple of align". The
        // SEGMENT-multiple rounding is `os::Segment::reserve`'s OWN choice
        // (`os.rs`), not something `vmem`/`reserve_aligned_on_node` demand;
        // `reserve_aligned_on_node` in particular already forwards `usable`
        // unrounded. See the `exact-span-large` feature doc in `Cargo.toml`.
        //
        // Alignment stays `SEGMENT` unconditionally (`align_up`/`div_ceil`
        // above already only ever go through `align.max(PAGE)`, never
        // `SEGMENT`, for the header offset) — the segment's BASE remains
        // SEGMENT-aligned either way, so `segment_base_of_ptr` (which masks
        // on SEGMENT) is unaffected; only the physical `usable` byte count
        // computed here differs.
        //
        // With the feature OFF this computation is byte-for-byte identical
        // to before: round `needed` up to a whole number of SEGMENT-sized
        // spans.
        #[cfg(not(feature = "exact-span-large"))]
        let usable = {
            let n_segments = needed.div_ceil(SEGMENT);
            n_segments * SEGMENT
        };
        #[cfg(feature = "exact-span-large")]
        let usable = align_up(needed, super::os::PAGE);

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
                // Pages are kept committed in the cache (no decommit on deposit,
                // no recommit needed on hit — they are already mapped and
                // accessible). Write a fresh header over the old one. The
                // allocation lives at `hdr_aligned` (same computation as the
                // slow path).
                //
                // UBFIX-6 (M-2, docs/reviews/2026-07-10-ub-audit-final-synthesis.md):
                // this used to call `self.table.register(slot.base)` FIRST (to
                // obtain `id` for the header) and only THEN
                // `Node::write_struct(slot.base, hdr)` — but `register` already
                // makes `slot.base` visible to `contains_base`/remote routing
                // lookups, so the plain full-struct write that followed could
                // race a concurrent remote defensive read
                // (`SegmentHeader::magic_at`/`kind_at`/`large_size_at`/
                // `span_usable_at`) on a stale/duplicate remote free — the same
                // data-race class as the two `dealloc`/`reclaim_large_segment`
                // "zero magic" sites, just in the opposite (publish) direction.
                //
                // Fix: reorder so the FULL header write happens while `slot.base`
                // is still UNREGISTERED (not yet in `contains_base`'s table, so
                // no remote reader can address it at all — the race is closed
                // by construction, not by per-field atomics). `segment_id` is
                // the only field that genuinely needs the registry-assigned
                // `id`, which is only available AFTER `register()` runs; it is
                // NOT one of the four remote-readable fields this finding
                // targets (`magic`/`kind`/`large_size`/`span_usable` — see
                // `SegmentHeader::segment_id_at`'s own doc: written once at
                // registration and otherwise only read by same-thread-ish
                // unregister/recycle bookkeeping), so it is written with a
                // placeholder here and patched with a single-word
                // `Node::write_u32` at its `offset_of!` offset, immediately
                // after `register()` returns the real slot index — no OTHER
                // code path can reach this not-yet-returned-to-its-caller
                // segment's `segment_id` between `register()` and the patch (it
                // is not yet handed to the allocation caller, so `unregister`/
                // `recycle` cannot race it here). This mirrors
                // `SegmentMeta::owner_state_atomic`'s general shape (write the
                // register-derived field last, as its own single-word store)
                // while requiring no changes to `segment_header.rs`/`node.rs`;
                // `SegmentHeader::set_segment_id_at` is intentionally NOT reused
                // here — it is documented TEST-ONLY (`dbg_stamp_segment_id`,
                // "never called on any production path"), so this production
                // patch performs the identical single `Node::write_u32` inline
                // instead of repurposing that test seam.
                let bump = hdr_aligned + align_up(size, align);
                // `span_usable` is carried forward from the CACHED slot's own
                // `usable_size` — the true physical span of the segment being
                // reused — NOT recomputed from the new (possibly smaller)
                // `size`/`align`. Bug #134. R12-4: `reserved_capacity` is
                // carried forward the same way, from `slot.reserved_capacity`
                // (the segment's true reserved VA span) — same rationale,
                // never recomputed.
                let hdr = SegmentHeader::large(
                    u32::MAX, // placeholder; patched below once `register()` assigns the real id
                    size,
                    align,
                    slot.usable_size,
                    slot.reserved_capacity,
                    bump,
                    slot.reservation,
                    slot.reservation_len,
                );
                Node::write_struct(slot.base as *mut SegmentHeader, hdr);
                // NOW publish `slot.base` to `contains_base`/remote routing.
                // Under alloc-decommit, `recycle()` left a NULL slot that
                // `register()` will reuse — so this should not fail. If it does
                // (table is genuinely full) we cannot reuse this slot; release
                // it and fall through to the slow OS path. `hdr` is already
                // written above, but the slot never becomes visible in that
                // failure branch, so there is nothing to unwind.
                let id = match self.table.register(slot.base) {
                    Some(id) => id,
                    None => {
                        os::release_segment(slot.reservation, slot.reservation_len);
                        return self.alloc_large_slow(size, align, usable, hdr_aligned);
                    }
                };
                // Patch the real registry slot index in now that the segment is
                // visible — a single-word field write (disjoint from
                // `bump`/`owner_state`, same discipline as `large_size_at`'s
                // sibling setter `set_large_size_at`).
                let segment_id_off = core::mem::offset_of!(SegmentHeader, segment_id);
                Node::write_u32(Node::offset(slot.base, segment_id_off) as *mut u32, id);
                // Phase C (numa-aware): re-stamp with the CURRENT thread's NUMA
                // node. The thread may have migrated since the segment was cached;
                // updating the tag reflects the current physical binding. R11-5:
                // reads the cached value (re-queried at most once per slot claim).
                #[cfg(feature = "numa-aware")]
                {
                    let my_node = self.current_node_cached();
                    SegmentMeta::new(slot.base).set_node_id(my_node);
                }
                return (Node::deref(slot.base, hdr_aligned), false);
            }
        }

        self.alloc_large_slow(size, align, usable, hdr_aligned)
    }

    /// The slow (OS round-trip) path for `alloc_large` — called when the
    /// `large_cache` has no matching entry. Factored out so the cache-hit path
    /// can call `return self.alloc_large_slow(...)` cleanly when the table is
    /// full (avoiding a goto / code-duplication).
    ///
    /// Every successful path here does a genuinely fresh
    /// `Segment::reserve`/`numa::reserve_aligned_on_node` reservation, so the
    /// freshness bool is `cfg!(not(miri))` (see `alloc_large`'s freshness doc:
    /// every real OS backend zero-fills a fresh reservation, but miri's
    /// `std::alloc` fallback does NOT — R9-1). Null/OOM returns carry `true`
    /// by the null-convention noted there (unobservable: the caller only
    /// consults the bool for non-null).
    fn alloc_large_slow(
        &mut self,
        size: usize,
        align: usize,
        usable: usize,
        hdr_aligned: usize,
    ) -> (*mut u8, bool) {
        // Phase C (numa-aware): steer the large segment to the calling thread's
        // NUMA node, same as for small segments. R11-5: cached accessor.
        #[cfg(feature = "numa-aware")]
        let my_node = self.current_node_cached();

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
                None => return (core::ptr::null_mut(), true),
            }
        };
        #[cfg(not(feature = "numa-aware"))]
        let (base, reservation, reservation_len, reserved_capacity) = {
            // R12-4: the RESERVED VA span for this segment. With the feature
            // ON (`usable` already page-exact from `exact-span-large`
            // above), reserve a geometric multiple of `usable` — capped at
            // `LARGE_RESERVED_CAP_BYTES` — but COMMIT only `usable` bytes;
            // the rest stays reserved-but-uncommitted until a growing
            // `realloc` needs it (`commit_pages`, see the OPT-G grow path in
            // `alloc_core.rs`). With the feature OFF this is simply
            // `usable` — "reserved == committed", byte-for-byte the
            // pre-R12-4 reservation.
            #[cfg(feature = "large-reserved-capacity")]
            let reserved_capacity_target = usable
                .saturating_mul(LARGE_RESERVED_CAP_GROWTH_FACTOR)
                .min(LARGE_RESERVED_CAP_BYTES)
                .max(usable);
            #[cfg(not(feature = "large-reserved-capacity"))]
            let reserved_capacity_target = usable;

            // R12-3: `Segment::reserve` ROUNDS `usable` up to a whole SEGMENT
            // multiple internally (`os.rs`) — that rounding is exactly what
            // `exact-span-large` exists to skip, so this path must call the
            // non-rounding sibling `reserve_exact` instead when the feature
            // is on (`usable` here is already page-exact, computed above).
            // With the feature OFF, `usable` is already a SEGMENT multiple,
            // so `Segment::reserve`'s internal rounding is a no-op — this
            // `#[cfg]` split changes nothing observable for the default path.
            //
            // R12-4: when `large-reserved-capacity` is also on, reserve
            // `reserved_capacity_target` bytes of VA (committing only
            // `usable`) via `reserve_capacity_exact` instead — a strict
            // superset of what `reserve_exact` does (it degrades to an
            // identical reservation when `reserved_capacity_target ==
            // usable`, e.g. right at the `LARGE_RESERVED_CAP_BYTES` cap).
            #[cfg(feature = "large-reserved-capacity")]
            let mut seg = Segment::reserve_capacity_exact(reserved_capacity_target, usable);
            #[cfg(all(
                not(feature = "large-reserved-capacity"),
                not(feature = "exact-span-large")
            ))]
            let mut seg = Segment::reserve(usable);
            #[cfg(all(not(feature = "large-reserved-capacity"), feature = "exact-span-large"))]
            let mut seg = Segment::reserve_exact(usable);
            // Mechanism 2 (task #51): pool-drain-and-retry on OS-reservation
            // failure — see the numa-aware arm above for the rationale (the pool
            // is a reclaimable soft reserve, not a hard pin).
            #[cfg(feature = "alloc-decommit")]
            if seg.is_none() && self.pooled_count > 0 {
                self.drain_small_pool();
                #[cfg(feature = "large-reserved-capacity")]
                {
                    seg = Segment::reserve_capacity_exact(reserved_capacity_target, usable);
                }
                #[cfg(all(
                    not(feature = "large-reserved-capacity"),
                    not(feature = "exact-span-large")
                ))]
                {
                    seg = Segment::reserve(usable);
                }
                #[cfg(all(not(feature = "large-reserved-capacity"), feature = "exact-span-large"))]
                {
                    seg = Segment::reserve_exact(usable);
                }
            }
            let segment = match seg {
                Some(s) => s,
                None => return (core::ptr::null_mut(), true),
            };
            let b = segment.as_ptr();
            let r = segment.reservation();
            let rl = segment.reservation_len();
            core::mem::forget(segment);
            (b, r, rl, reserved_capacity_target)
        };
        #[cfg(feature = "numa-aware")]
        let reserved_capacity = usable;

        // no-panic: register returns None if the segment table is full (too many
        // live large allocations). We release the reservation and return null
        // (graceful OOM) rather than panicking.
        let id = match self.table.register(base) {
            Some(id) => id,
            None => {
                // Release the reservation we own.
                os::release_segment(reservation.as_ptr(), reservation_len);
                return (core::ptr::null_mut(), true);
            }
        };
        // Lay down the large header. The allocation lives at `hdr_aligned`.
        let bump = hdr_aligned + align_up(size, align);
        // Fresh reservation: `span_usable` = the just-computed physical
        // usable span (`usable`) — this is the ORIGINAL stamping that every
        // later cache-hit reuse of this segment will carry forward verbatim.
        // R12-4: `reserved_capacity` is likewise the ORIGINAL reserved VA
        // span, carried forward the same way.
        let hdr = SegmentHeader::large(
            id,
            size,
            align,
            usable,
            reserved_capacity,
            bump,
            reservation.as_ptr(),
            reservation_len,
        );
        Node::write_struct(base as *mut SegmentHeader, hdr);
        // Phase C (numa-aware): stamp the NUMA node into the header after
        // writing it (the constructor sets node_id to NO_NODE_RAW).
        #[cfg(feature = "numa-aware")]
        SegmentMeta::new(base).set_node_id(my_node);

        // R9-1: fresh means OS-zeroed only on real OS backends; miri's
        // std::alloc fallback does not zero, so the freshness signal must be
        // withheld there (the caller then zeroes explicitly, restoring the
        // alloc_zeroed contract under miri).
        (Node::deref(base, hdr_aligned), cfg!(not(miri)))
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
            // reuse). R12-4: `reserved_capacity` is carried forward the same
            // way (never recomputed) — see `CachedLarge::reserved_capacity`'s
            // doc. The field is present in every build's layout (inert,
            // equal to `span_usable`, when the feature is off), so this read
            // needs no `#[cfg]` split.
            let usable_size = hdr.span_usable;
            let reserved_capacity = hdr.reserved_capacity;

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
                // UBFIX-6 (M-2, docs/reviews/2026-07-10-ub-audit-final-synthesis.md):
                // was `hdr_zero = hdr; hdr_zero.magic = 0; Node::write_struct(base,
                // hdr_zero)` — a non-atomic FULL-STRUCT write racing the remote
                // defensive field reads (`SegmentHeader::magic_at`/`kind_at`/
                // `large_size_at`/`span_usable_at`) that can observe a live header
                // concurrently with this owner write under a stale/duplicate
                // remote free. `hdr` is a fresh `read_at(base)` taken at the top
                // of this fn, so every OTHER field is already byte-identical to
                // what's in memory — the only real effect is zeroing `magic`.
                // Same fix as the mirror site in `AllocCore::dealloc`'s Large
                // branch: a single atomic `&AtomicU32` store at `magic`'s
                // `offset_of!` offset (the same field-wise-atomic-write pattern
                // `SegmentMeta::owner_state_atomic` already uses for the
                // cross-thread owner-state read — this crate's §11 discipline).
                let magic_off = core::mem::offset_of!(SegmentHeader, magic);
                Node::atomic_u32_at(base, magic_off)
                    .store(0, core::sync::atomic::Ordering::Release);
                let seq = self.large_cache_seq;
                self.large_cache_seq = self.large_cache_seq.wrapping_add(1);
                self.large_cache[slot_idx] = Some(CachedLarge {
                    reservation: hdr.reservation,
                    reservation_len: hdr.reservation_len,
                    base,
                    usable_size,
                    reserved_capacity,
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
