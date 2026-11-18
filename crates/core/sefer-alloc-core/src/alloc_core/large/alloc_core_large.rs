//! Large-path cluster of [`AllocCore`] (mechanical split of `alloc_core.rs`):
//! an `impl AllocCore { .. }` block with the large/huge alloc + reclaim
//! methods; a pure code-movement sibling of `alloc_core.rs`, no behavior changed.

#[cfg(any(feature = "alloc-xthread", feature = "alloc-decommit"))]
use crate::alloc_core::fatal::fatal;
#[cfg(any(feature = "alloc-decommit", feature = "alloc-xthread"))]
use crate::alloc_core::large::reservation_state::LargeReservationState;
use crate::alloc_core::node::Node;
#[cfg(feature = "numa-aware")]
use crate::alloc_core::numa;
#[cfg(not(feature = "numa-aware"))]
use crate::alloc_core::os;
#[cfg(not(feature = "numa-aware"))]
use crate::alloc_core::os::Segment;
use crate::alloc_core::os::SEGMENT;
#[cfg(feature = "alloc-decommit")]
use crate::alloc_core::segment_header::SEGMENT_MAGIC;
use crate::alloc_core::segment_header::{align_up, SegmentHeader, SegmentKind, SegmentMeta};

#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
use crate::alloc_core::alloc_core::advise_fresh_large;
use crate::alloc_core::alloc_core::release_large_mapping;
use crate::alloc_core::alloc_core::AllocCore;
#[cfg(feature = "alloc-decommit")]
use crate::alloc_core::alloc_core::LARGE_CACHE_SIZE_FACTOR;
#[cfg(all(
    feature = "alloc-decommit",
    feature = "alloc-xthread",
    not(feature = "numa-aware")
))]
use crate::alloc_core::config::policy::large_thp_runtime::ThpRuntime;
#[cfg(feature = "internals")]
use core::sync::atomic::Ordering;
// R1-08: `CachedLarge` is constructed only inside `reclaim_large_segment`,
// which is itself `#[cfg(feature = "alloc-xthread")]` — narrower than the
// `alloc-decommit` gate `LARGE_CACHE_SIZE_FACTOR` above needs, so it must
// carry its own, tighter `cfg` or a build with `alloc-decommit` but no
// `alloc-xthread` warns `unused_imports`.
#[cfg(all(feature = "alloc-decommit", feature = "alloc-xthread"))]
use crate::alloc_core::alloc_core::CachedLarge;

/// R12-4 (EXPERIMENTAL, feature `large-reserved-capacity`): cap on how large a
/// Large segment's `reserved_capacity` may grow relative to `SEGMENT` (4 MiB),
/// regardless of request size. Without a cap, a pathologically large request
/// (e.g. 512 MiB) would reserve a 1 GiB VA span "for future growth" — cheap on
/// 64-bit but not free (page-table/TLB footprint, and a bound the OS can
/// refuse on a constrained VA budget). 64 MiB (16 x SEGMENT) comfortably
/// covers the realloc-growth-chain scenario this feature targets while keeping
/// worst-case reservation bounded and predictable.
///
/// `not(numa-aware)`: consumed only by `alloc_large_slow`'s `not(numa-aware)`
/// arm — the `numa-aware` arm always uses eager `numa::reserve_aligned_on_node`
/// with `reserved_capacity == usable` (same exclusion
/// `Segment::reserve_capacity_exact` documents). Gating the constant keeps
/// `--all-features` free of a dead-code warning.
#[cfg(all(feature = "large-reserved-capacity", not(feature = "numa-aware")))]
const LARGE_RESERVED_CAP_BYTES: usize = 16 * SEGMENT;

/// R12-4/**R14-6** (task #291): geometric growth factor applied to the
/// page-rounded request size to compute the INITIAL `reserved_capacity`.
///
/// R12-4 originally shipped 2x (classic amortised doubling). R13-6's A/B gate
/// (`docs/perf/R13_6_EXACT_SPAN_RESERVED_CAPACITY_PRODUCTION_GATE.md` §3.3)
/// measured that a 2x ceiling re-trips on almost every OTHER step of a
/// doubling-cadence realloc chain (the most common growth pattern; what
/// `Vec`/`String` do): a segment reserved at `usable * 2` fits exactly one
/// doubling in place, then the next doubling forces a relocation — a
/// deterministic iai-measured +102.3% instruction / +52.7% cycles regression
/// on `benches/perf_gate_iai.rs`'s `realloc_grow` (64 B -> 4 MiB).
///
/// R14-6 raises the factor to 4x, chosen by total bytes relocation-copied
/// across the same 16-step chain (the dominant cost — `Ir` scales with
/// `memcpy` bytes, since later doublings move MiB-scale prefixes):
///
/// | Factor | Relocations / 16 steps | Total bytes copied |
/// |---|---:|---:|
/// | 2x (R12-4) | 8 | ~2.67 MiB |
/// | **4x (R14-6)** | **5** | **~1.14 MiB (-57%)** |
/// | 8x | 4 | ~2.13 MiB (worse: dominant LAST copy barely shrinks) |
///
/// 4x is the sweet spot; see `docs/perf/R14_6_ADAPTIVE_RESERVED_CAPACITY_GATE.md`
/// for the full before/after iai numbers. A per-chain COMPOUNDING multiplier
/// models even better in isolation but needs per-segment chain-identity state
/// (no way to recognise "this fresh segment was born from relocating THIS
/// SAME logical realloc chain" vs an unrelated fresh `alloc`) — deferred as a
/// follow-up; a flat constant needs no `SegmentHeader` field and no threading
/// of a relocation-vs-fresh-alloc hint through the shared `self.alloc(..)`
/// path OPT-G's slow leg reuses.
///
/// `not(numa-aware)`: same gating rationale as [`LARGE_RESERVED_CAP_BYTES`].
#[cfg(all(feature = "large-reserved-capacity", not(feature = "numa-aware")))]
const LARGE_RESERVED_CAP_GROWTH_FACTOR: usize = 4;

fn large_payload_offset(align: usize) -> usize {
    if align >= SEGMENT {
        align_up(
            core::mem::size_of::<SegmentHeader>(),
            aligned_vmem::page_size(),
        )
    } else {
        align_up(
            core::mem::size_of::<SegmentHeader>(),
            align.max(aligned_vmem::page_size()),
        )
    }
}

pub use sefer_segment::large_payload_address_matches;

impl AllocCore {
    /// Pure address equality shared by owner metadata and foreign descriptors.
    /// No pointer is dereferenced; overflowing geometry cannot match.
    // Sole callers are the misuse/diagnostic mismatch arms; dead in plain builds.
    #[cfg_attr(
        not(any(feature = "misuse-checks", debug_assertions)),
        allow(dead_code)
    )]
    #[allow(dead_code)]
    #[inline(always)]
    pub(crate) fn large_payload_address_matches(
        address: usize,
        base: usize,
        payload_offset: usize,
    ) -> bool {
        sefer_segment::large_payload_address_matches(address, base, payload_offset)
    }

    /// `base` must be this owner's canonical live metadata root, including
    /// biased NUMA roots. The caller has already resolved table membership.
    #[cfg_attr(
        not(any(feature = "misuse-checks", debug_assertions)),
        allow(dead_code)
    )]
    #[allow(unsafe_code)] // P2.1 tier-2: header reads migrated to unsafe contract.
    #[doc(hidden)] // P2.2: widened from `pub(crate)` so root heap_core callers compile (E0624).
    #[inline]
    pub unsafe fn large_payload_mismatch(base: *mut u8, ptr: *mut u8) -> bool {
        // SAFETY: caller contract: `base` is this owner's canonical live
        // metadata root with table membership already resolved; header reads
        // stay within the live mapped header.
        unsafe {
            SegmentHeader::kind_at(base) == SegmentKind::Large
                && !large_payload_address_matches(
                    ptr.addr(),
                    base.addr(),
                    SegmentHeader::read_at(base).payload_offset,
                )
        }
    }

    /// Reject before any lifecycle credit or hardened small-layout guard.
    /// Pure predicate since R6-T55: the callers count the rejection (into
    /// `LOCAL_REJECT_FREES`, not the public foreign/unroutable stat) at their
    /// own single rejection site.
    ///
    /// # Safety
    ///
    /// Identical to [`AllocCore::large_payload_mismatch`]'s contract:
    /// `base` must point to this owner's canonical, live, table-resolved
    /// Large/segment metadata root; `SegmentHeader::kind_at(base)` and
    /// `SegmentHeader::read_at(base)` dereference it and therefore require
    /// the header bytes to be valid for reads for the whole
    /// `size_of::<SegmentHeader>()` region and not concurrently mutated for
    /// the duration of this call. `ptr` is never dereferenced — any value
    /// (dangling, interior, foreign) is fine. Violating the `base` contract
    /// is undefined behavior.
    #[cfg_attr(
        not(any(feature = "misuse-checks", debug_assertions)),
        allow(dead_code)
    )]
    #[allow(unsafe_code)] // P2.1 tier-2: forwards the header-read unsafe contract.
    #[doc(hidden)] // P2.2: widened from `pub(crate)` so root heap_core callers compile (E0624).
    #[inline]
    pub unsafe fn reject_large_interior_free(base: *mut u8, ptr: *mut u8) -> bool {
        // SAFETY: this call forwards the caller's contract verbatim — `base`
        // is the canonical live, table-resolved metadata root valid for the
        // full header read, exactly as required by `large_payload_mismatch`.
        unsafe { Self::large_payload_mismatch(base, ptr) }
    }

    /// Allocate a large/huge block: reserve a dedicated segment sized to fit,
    /// place the allocation at the first page-aligned offset past the header,
    /// register the segment, and return the allocation pointer.
    ///
    /// **OPT-E (alloc-decommit):** search the Large cache before reserving new
    /// memory. A hit avoids OS reservation and recommit, but still refreshes
    /// allocation metadata and registers the reused segment.
    ///
    /// **Phase 2 (alloc-decommit):** check lazy decay before serving the request.
    /// At or below headroom, skip the clock read; otherwise checks are stride-
    /// throttled, except for a zero interval. An elapsed check may run multiple
    /// bounded catch-up steps.
    ///
    /// # Freshness signal (task #221 / R8-8; miri fix R9-1)
    ///
    /// Returns `(*mut u8, bool)`; the bool — meaningful only for non-null — is
    /// `true` iff the allocation lives in a GENUINELY FRESH OS reservation
    /// whose bytes the OS zero-fills by construction (MEM_COMMIT, anonymous
    /// `mmap`). Under **miri** it is ALWAYS `false`: aligned-vmem's miri
    /// aperture falls back to bare `std::alloc::alloc`, which does NOT zero
    /// (see vmem's `leak_zeroed_pages`), so the caller must zero explicitly.
    /// `alloc_large_slow` (the only fresh-span producer) yields
    /// `cfg!(not(miri))`; a `large_cache` HIT (reused segment that may still
    /// hold the prior occupant's bytes) yields `false` everywhere. Null
    /// returns carry `true` purely by convention — the bool is unobservable
    /// for null. This is a conservative, SEGMENT-RESERVATION-level signal with
    /// no per-block bitmap and no interaction with decommit/MADV_FREE reuse.
    #[allow(unsafe_code)] // P2.1: direct owner-root access.
    ///
    /// # Safety
    ///
    /// P2.2-narrowed to the ACTUAL soundness proof (no body change):
    ///
    /// - `align` MUST be a non-zero power of two. This clause is the
    ///   caller-side contract, NOT a claim about the arithmetic below:
    ///   `align_up` itself handles ANY positive alignment (ceiling
    ///   division, debug-asserting only non-zero), and
    ///   `large_payload_offset` applies `max(page_size())` to a smaller
    ///   `align`. The requirement exists because `align` describes the
    ///   alignment of a valid `Large` allocation `Layout` — the geometry
    ///   stored in the segment header and relied on by callers. For a raw
    ///   `align` that is zero or not a power of two, no behaviour is
    ///   promised and there is no runtime guard on this hot path.
    /// - `size` may be ANY `usize` — including values for which
    ///   `Layout::from_size_align` fails (above `isize::MAX`, or too large
    ///   for the alignment padding). Every arithmetic step is checked: the
    ///   header-offset `checked_add` below, the SEGMENT rounding
    ///   (`checked_mul`, feature OFF) and the page rounding (`checked_mul`,
    ///   `exact-span-large` ON), so such sizes fail safely (returning
    ///   `(null, true)`) instead of wrapping — whether via an overflow
    ///   check or via the OS rejecting an oversized reservation. This is
    ///   what the `dbg_alloc_large_for_test` R6-CR-4 forwarder (and the
    ///   r6_t30 `usize::MAX - 4096` case) relies on.
    /// - Callers routing through a canonical `Layout` (heap_core
    ///   `alloc`/`realloc`/`alloc_zeroed`, `mem_impl`) trivially satisfy
    ///   both clauses.
    ///
    /// The former contract — "`align` a power of two AND the (`size`,
    /// `align`) pair must be one for which `Layout::from_size_align`
    /// succeeds" — over-stated the size obligation: representability is NOT
    /// required for soundness here, only for a non-null result.
    #[doc(hidden)] // P2.2: widened from `pub(crate)` so root heap_core callers compile (E0624).
    pub unsafe fn alloc_large(&mut self, size: usize, align: usize) -> (*mut u8, bool) {
        // R6-CR-3: injected OOM — concede null without touching metadata so a
        // test can drive the realloc move-leg failure path.
        #[cfg(feature = "internals")]
        if crate::alloc_core::large::reservation_state::DBG_INJECT_LARGE_ALLOC_OOM
            .load(Ordering::Relaxed)
        {
            return (core::ptr::null_mut(), true);
        }
        // Phase 2: lazy decay tick on every large allocation.
        #[cfg(feature = "alloc-decommit")]
        {
            self.maybe_decay_large_cache();
            // R6-T31: pool decay tick on the same cold cadence as the large cache.
            self.maybe_decay_small_pool();
        }

        // The segment must hold: header + alignment padding + size, rounded up
        // to a whole number of segments. `Segment::reserve` does the rounding.
        let hdr_aligned = large_payload_offset(align);
        // task #25 (security): `checked_add` for local overflow safety — a
        // wrap here is unreachable under the `Layout` size/align invariant
        // today, but this no longer RELIES on the caller's `Layout` being
        // well-formed (parity with the realloc path, which already uses
        // `checked_add`). A wrap → null (a legal alloc-failure signal).
        //
        // R6-CR-4: the rounding steps DOWNSTREAM of this `checked_add` (the
        // SEGMENT rounding in the feature-OFF arm and the page rounding in
        // the `exact-span-large` arm below) are checked too — an overflow at
        // ANY step returns null rather than wrapping.
        let needed = match hdr_aligned.checked_add(size) {
            Some(n) => n,
            None => return (core::ptr::null_mut(), true),
        };
        // R12-3 (EXPERIMENTAL, `exact-span-large`): size the physical
        // reservation to the exact page-rounded request instead of rounding
        // up to whole SEGMENT-sized (4 MiB) spans — a 260 KiB request costs
        // ~264 KiB instead of the 4 MiB minimum every Large request pays
        // today. (The OLD comment claimed `reserve_aligned_on_node` REQUIRES
        // a SEGMENT multiple for the over-reserve+trim arithmetic; that is
        // NOT a real backend constraint — both vmem backends trim to an
        // `align`-aligned `size`-byte span for ARBITRARY size; the SEGMENT
        // rounding is `os::Segment::reserve`'s own choice. See the feature
        // doc in `Cargo.toml`.)
        //
        // Useful-window sizing and identity are separate: ordinary Large
        // roots retain SEGMENT alignment; over-segment requests use a
        // page-aligned biased metadata root, and lookup uses the numeric
        // payload key, never a masked-pointer metadata root.
        //
        // With the feature OFF: round `needed` up to whole SEGMENT spans.
        #[cfg(not(feature = "exact-span-large"))]
        let usable = {
            let n_segments = needed.div_ceil(SEGMENT);
            // R6-CR-4: checked — a malformed Layout (usize::MAX-scale size from a
            // GlobalAlloc::realloc caller) overflows the SEGMENT rounding; return
            // null instead of wrapping (debug would panic).
            match n_segments.checked_mul(SEGMENT) {
                Some(u) => u,
                None => return (core::ptr::null_mut(), true),
            }
        };
        // Task #1074: round to the RUNTIME OS page size, not compile-time
        // `PAGE` — this value flows into `reserve_capacity_exact` as span
        // size AND `initial_commit`, and `aligned_vmem::validate_initial_commit`
        // (task #1037) requires runtime-`page_size()` multiples; a 4 KiB-only
        // multiple is rejected on 16/64 KiB-page hosts. A whole-`SEGMENT`
        // multiple is already a page multiple for every supported page size
        // (≤ 64 KiB ≪ 4 MiB).
        #[cfg(feature = "exact-span-large")]
        let usable = {
            // R6-CR-4: checked page rounding (align_up can overflow `q * page`).
            let page = aligned_vmem::page_size();
            match needed.div_ceil(page).checked_mul(page) {
                Some(u) => u,
                None => return (core::ptr::null_mut(), true),
            }
        };

        // OPT-E: try the large-segment cache first.
        // Scan all slots for a compatible entry: usable_size >= usable (the
        // cached segment is big enough) AND usable_size <= usable *
        // LARGE_CACHE_SIZE_FACTOR (not so big we waste RSS). The size-ratio
        // bound prevents a 64 MiB cached segment from permanently absorbing
        // every 4 MiB request.
        #[cfg(feature = "alloc-decommit")]
        if align < SEGMENT {
            // G11 (task #51) — BEST-FIT: scan every cached entry (compatibility:
            // big enough AND not wastefully large, per the size-ratio bound
            // above) and take the SMALLEST `usable_size` instead of the first
            // fit. Keeps the tightest span for this request and leaves larger
            // spans for larger future requests — less RSS waste than first-fit.
            let mut hit_idx: Option<usize> = None;
            let mut best_usable: usize = usize::MAX;
            // P1-3 (#1985): iterate only OCCUPIED slots via the R32-12
            // occupancy bitmask (same invariant `large_cache_find_free_slot`
            // relies on) instead of an `Option` discriminant load per slot
            // across `large_cache_scan_bound()`.
            #[cfg(not(feature = "large-cache-extended"))]
            {
                // The pre-J14 loop, unchanged: the production build keeps its code.
                let mut occupied = self.large_cache_occupied_within_bound();
                while occupied != 0 {
                    let i = occupied.trailing_zeros() as usize;
                    occupied &= occupied - 1; // clear the lowest set bit
                    if let Some(slot) = self.large_cache_slot_get(i) {
                        if slot.usable_size >= usable
                            && slot.usable_size <= usable.saturating_mul(LARGE_CACHE_SIZE_FACTOR)
                            && slot.usable_size < best_usable
                        {
                            best_usable = slot.usable_size;
                            hit_idx = Some(i);
                        }
                    }
                }
            }
            #[cfg(feature = "large-cache-extended")]
            for i in self.large_cache_occupied_slots() {
                if let Some(slot) = self.large_cache_slot_get(i) {
                    if slot.usable_size >= usable
                        && slot.usable_size <= usable.saturating_mul(LARGE_CACHE_SIZE_FACTOR)
                        && slot.usable_size < best_usable
                    {
                        best_usable = slot.usable_size;
                        hit_idx = Some(i);
                        // J14: an exact fit cannot be beaten and the scan is
                        // lowest-index first, so stopping keeps the result.
                        #[cfg(feature = "large-cache-adaptive")]
                        if best_usable == usable {
                            break;
                        }
                    }
                }
            }
            if let Some(idx) = hit_idx {
                let slot = self.large_cache_slot_take(idx);
                // SAFETY: `slot.base` is the owner's live Large header root.
                let terminal_meta = unsafe { SegmentMeta::new(slot.base) };
                let generation = match terminal_meta.begin_large_reuse() {
                    Some(generation) => generation,
                    None => {
                        self.large_cache_used_bytes =
                            self.large_cache_used_bytes.saturating_sub(slot.usable_size);
                        let state = LargeReservationState::new(terminal_meta.large_state_atomic());
                        let word = terminal_meta.terminal_snapshot().large_state;
                        if !state.release_cached(
                            crate::alloc_core::segment_header::large_generation(word),
                        ) {
                            fatal(b"large: cached release failed\n");
                        }
                        release_large_mapping(
                            // SAFETY: the owner reads the charge from the live cached header.
                            unsafe { SegmentHeader::thp_charge_at(slot.base) },
                            slot.reservation,
                            slot.reservation_len,
                        );
                        return self.alloc_large_slow(size, align, usable, hdr_aligned);
                    }
                };
                // Update the byte-budget counter: this slot is leaving the cache.
                self.large_cache_used_bytes =
                    self.large_cache_used_bytes.saturating_sub(slot.usable_size);
                self.large_cache_note_hit(slot.usable_size);
                // The cache retains the committed reservation. Before
                // registration, update the owner-side atomic legacy magic,
                // size, alignment, and bump fields; the payload offset is
                // also set before publication. Foreign frees use descriptors
                // and sidecars, not header readers.
                let bump = hdr_aligned + size;
                // `span_usable` is carried forward from the CACHED slot's own
                // `usable_size` — the true physical span of the segment being
                // reused — NOT recomputed from the new (possibly smaller)
                // `size`/`align`. Bug #134. R12-4: `reserved_capacity` is
                // carried forward the same way, from `slot.reserved_capacity`
                // (the segment's true reserved VA span) — same rationale,
                // never recomputed.
                //
                // Cached geometry is reused verbatim; allocation fields are rewritten.
                // Debug assertions verify geometry before targeted header writes.
                #[cfg(debug_assertions)]
                {
                    // SAFETY: `slot.base` is a live, table-registered large
                    // segment owned by this AllocCore; debug read only.
                    #[allow(unsafe_code)]
                    let pre = unsafe { SegmentHeader::read_at(slot.base) };
                    debug_assert_eq!(
                        pre.span_usable, slot.usable_size,
                        "F12: large-cache slot's in-memory span_usable diverged from slot.usable_size"
                    );
                    debug_assert_eq!(
                        pre.reserved_capacity, slot.reserved_capacity,
                        "F12: large-cache slot's in-memory reserved_capacity diverged from slot.reserved_capacity"
                    );
                    debug_assert_eq!(
                        pre.reservation, slot.reservation,
                        "F12: large-cache slot's in-memory reservation diverged from slot.reservation"
                    );
                    debug_assert_eq!(
                        pre.reservation_len, slot.reservation_len,
                        "F12: large-cache slot's in-memory reservation_len diverged from slot.reservation_len"
                    );
                }
                #[cfg(debug_assertions)]
                // SAFETY: the owner reads the charge from the live cached header.
                let charge_before = unsafe { SegmentHeader::thp_charge_at(slot.base) };
                // SAFETY: the owner exclusively updates the live cached header before registration.
                unsafe {
                    SegmentHeader::set_magic_at(slot.base, SEGMENT_MAGIC);
                    SegmentHeader::set_large_size_at(slot.base, size);
                    SegmentHeader::set_large_align_at(slot.base, align);
                    SegmentHeader::set_bump_at(slot.base, bump);
                }
                Node::write_usize(
                    Node::offset(
                        slot.base,
                        core::mem::offset_of!(SegmentHeader, payload_offset),
                    ) as *mut usize,
                    hdr_aligned,
                );
                #[cfg(debug_assertions)]
                debug_assert_eq!(
                    // SAFETY: the owner reads the charge from the live cached header.
                    unsafe { SegmentHeader::thp_charge_at(slot.base) },
                    charge_before,
                    "F12: large-cache hit's targeted header writes must not change thp_charge"
                );
                // Publish the prepared header; if the table is full, release and retry with a fresh mapping.
                // SAFETY: `slot.base` is the owner's live cached root, not yet registered.
                #[allow(unsafe_code)]
                let id = match unsafe {
                    self.table.register_payload(
                        slot.base,
                        slot.reserved_capacity,
                        SegmentKind::Large,
                        Node::offset(slot.base, hdr_aligned),
                    )
                } {
                    Some(id) => id,
                    None => {
                        if !LargeReservationState::new(terminal_meta.large_state_atomic())
                            .release_initializing(generation)
                        {
                            fatal(b"large: initializing release failed\n");
                        }
                        release_large_mapping(
                            // SAFETY: the owner reads the charge from the live cached header.
                            unsafe { SegmentHeader::thp_charge_at(slot.base) },
                            slot.reservation,
                            slot.reservation_len,
                        );
                        return self.alloc_large_slow(size, align, usable, hdr_aligned);
                    }
                };
                // Store the registry-derived segment id after registration;
                // the header and legacy magic were established beforehand.
                let segment_id_off = core::mem::offset_of!(SegmentHeader, segment_id);
                Node::write_u32(Node::offset(slot.base, segment_id_off) as *mut u32, id);
                // Phase C (numa-aware): re-stamp with the CURRENT thread's NUMA
                // node. The thread may have migrated since the segment was cached;
                // updating the tag reflects the current physical binding. R11-5:
                // reads the cached value (re-queried at most once per slot claim).
                #[cfg(feature = "numa-aware")]
                {
                    let my_node = self.current_node_cached();
                    // SAFETY: `slot.base` is the owner's live, initialized Large header root.
                    (unsafe { crate::alloc_core::segment_header::SegmentMeta::new(slot.base) })
                        .set_node_id(my_node);
                }
                terminal_meta.finish_large_reuse(generation);
                // Э5 (task #145): load+store instead of `fetch_add` — no
                // `lock xadd`. SOUND for the same single-writer reason as
                // `HeapCore::tcache_hits`: the counter is per-heap and
                // `alloc_large` (its only incrementer) runs solely on the
                // owning thread, so splitting the RMW cannot drop a count;
                // the cross-thread reader still does a Relaxed atomic load.
                //
                // W3: increment the SLOT's counter when this heap is bound
                // (`large_cache_hits_sink`), else the owned fallback. W3 Part
                // B: gated behind `alloc-stats` (default OFF) — when off it
                // compiles out and `stats().large_cache_hits` reads 0. See the
                // feature doc in Cargo.toml.
                #[cfg(feature = "alloc-stats")]
                {
                    let ctr = self.large_cache_hits_sink.unwrap_or(&self.large_cache_hits);
                    ctr.store(
                        ctr.load(core::sync::atomic::Ordering::Relaxed)
                            .wrapping_add(1),
                        core::sync::atomic::Ordering::Relaxed,
                    );
                }
                return (Node::deref(slot.base, hdr_aligned), false);
            }
            // J14: a cache miss — a mistake if a recently gone span would have served it.
            self.large_cache_note_miss(usable);
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
    #[allow(unsafe_code)] // R25-1: item-scoped; owner-constructed fresh-segment unsafe blocks below.
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
            let reserved = if align >= SEGMENT {
                numa::reserve_biased_on_node(usable, align, hdr_aligned, my_node)
            } else {
                numa::reserve_aligned_on_node(usable, my_node)
            };
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
                    if align >= SEGMENT {
                        numa::reserve_biased_on_node(usable, align, hdr_aligned, my_node)
                    } else {
                        numa::reserve_aligned_on_node(usable, my_node)
                    }
                }
                None => None,
            };
            match reserved {
                Some((b, r, rl)) => (b.as_ptr(), r, rl),
                None => return (core::ptr::null_mut(), true),
            }
        };
        #[cfg(not(feature = "numa-aware"))]
        let (base, reservation, reservation_len, reserved_capacity, thp_charge, _thp_observation) = {
            // R12-4: the RESERVED VA span. Feature ON (`usable` already
            // page-exact from `exact-span-large`): reserve a geometric multiple
            // of `usable`, capped at `LARGE_RESERVED_CAP_BYTES`, but COMMIT
            // only `usable` — the rest stays reserved-uncommitted until a
            // growing `realloc` needs it (`commit_pages`, OPT-G grow path in
            // `alloc_core.rs`). Feature OFF: reserved == committed.
            #[cfg(feature = "large-reserved-capacity")]
            let reserved_capacity_target = usable
                .saturating_mul(LARGE_RESERVED_CAP_GROWTH_FACTOR)
                .min(LARGE_RESERVED_CAP_BYTES)
                .max(usable);
            #[cfg(not(feature = "large-reserved-capacity"))]
            let reserved_capacity_target = usable;

            // R12-3: `Segment::reserve` ROUNDS `usable` up to a whole SEGMENT
            // multiple internally (`os.rs`) — exactly what `exact-span-large`
            // exists to skip, so this path calls the non-rounding sibling
            // `reserve_exact` when the feature is on (`usable` is already
            // runtime-page-exact, task #1074). Feature OFF: `usable` is a
            // SEGMENT multiple, the rounding is a no-op, nothing observable
            // changes.
            //
            // R12-4: with `large-reserved-capacity` also on, reserve
            // `reserved_capacity_target` bytes of VA (committing only
            // `usable`) via `reserve_capacity_exact` — a strict superset of
            // `reserve_exact` (identical reservation when target == usable,
            // e.g. at the `LARGE_RESERVED_CAP_BYTES` cap).
            #[cfg(feature = "large-reserved-capacity")]
            let mut seg = if align >= SEGMENT {
                None
            } else {
                Segment::reserve_capacity_exact(reserved_capacity_target, usable)
            };
            // `mut` is needed under `alloc-decommit` (the pool-drain-and-retry
            // arm below reassigns `seg`). Silence the unused-mut warning when
            // `alloc-decommit` is off and this binding is never reassigned.
            #[cfg(all(
                not(feature = "large-reserved-capacity"),
                not(feature = "exact-span-large")
            ))]
            #[allow(unused_mut)]
            let mut seg = if align >= SEGMENT {
                None
            } else {
                Segment::reserve(usable)
            };
            #[cfg(all(not(feature = "large-reserved-capacity"), feature = "exact-span-large"))]
            let mut seg = if align >= SEGMENT {
                None
            } else {
                Segment::reserve_exact(usable)
            };
            // Mechanism 2 (task #51): pool-drain-and-retry on OS-reservation
            // failure — see the numa-aware arm above for the rationale (the pool
            // is a reclaimable soft reserve, not a hard pin).
            #[cfg(feature = "alloc-decommit")]
            if align < SEGMENT && seg.is_none() && self.pooled_count > 0 {
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
            if align >= SEGMENT {
                #[cfg(feature = "internals")]
                #[cfg_attr(not(feature = "alloc-decommit"), allow(unused_mut))]
                let mut reserved =
                    if crate::alloc_core::large::reservation_state::DBG_INJECT_BIASED_RESERVE_FAIL
                        .load(Ordering::Relaxed)
                    {
                        None
                    } else {
                        os::Segment::reserve_biased(
                            reserved_capacity_target,
                            usable,
                            align,
                            hdr_aligned,
                        )
                    };
                #[cfg(not(feature = "internals"))]
                #[cfg_attr(not(feature = "alloc-decommit"), allow(unused_mut))]
                let mut reserved = os::Segment::reserve_biased(
                    reserved_capacity_target,
                    usable,
                    align,
                    hdr_aligned,
                );
                // Mechanism 2 (task #51 / R6-CR-9): the biased path must drain the
                // pooled small segments and retry ONCE too, like the NUMA branch and
                // the aligned path above — the pool is a reclaimable soft reserve.
                #[cfg(feature = "alloc-decommit")]
                if reserved.is_none() && self.pooled_count > 0 {
                    self.drain_small_pool();
                    // Real retry: the injection hook deliberately gates only the FIRST attempt.
                    reserved = os::Segment::reserve_biased(
                        reserved_capacity_target,
                        usable,
                        align,
                        hdr_aligned,
                    );
                }
                let Some((b, r, rl)) = reserved else {
                    return (core::ptr::null_mut(), true);
                };
                (b, r, rl, reserved_capacity_target, 0usize, 0u64)
            } else {
                let segment = match seg {
                    Some(s) => s,
                    None => return (core::ptr::null_mut(), true),
                };
                let b = segment.as_ptr();
                let r = segment.reservation();
                let rl = segment.reservation_len();
                // Advise before the live `Segment` is forgotten; the charge stays a local
                // until the header is written (or the registration failure returns it).
                #[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
                let (thp_charge, thp_observation) =
                    advise_fresh_large(&segment, size, usable, hdr_aligned + size);
                #[cfg(not(all(feature = "alloc-decommit", not(feature = "numa-aware"))))]
                let (thp_charge, thp_observation) = (0usize, 0u64);
                core::mem::forget(segment);
                (
                    b,
                    r,
                    rl,
                    reserved_capacity_target,
                    thp_charge,
                    thp_observation,
                )
            }
        };
        #[cfg(feature = "numa-aware")]
        let (reserved_capacity, thp_charge, _thp_observation) = (usable, 0usize, 0u64);

        // no-panic: register returns None if the segment table is full (too many
        // live large allocations). We release the reservation and return null
        // (graceful OOM) rather than panicking.
        #[cfg(feature = "internals")]
        let inject_register_fail =
            crate::alloc_core::large::reservation_state::DBG_INJECT_LARGE_REGISTER_FAIL
                .load(Ordering::Relaxed);
        #[cfg(not(feature = "internals"))]
        let inject_register_fail = false;
        let registered = if inject_register_fail {
            None
        } else {
            // SAFETY: `base` is the freshly reserved large segment being
            // registered into the live owner table (exclusive registration).
            #[allow(unsafe_code)]
            unsafe {
                self.table.register_payload(
                    base,
                    reserved_capacity,
                    SegmentKind::Large,
                    Node::offset(base, hdr_aligned),
                )
            }
        };
        let id = match registered {
            Some(id) => id,
            None => {
                // Release the reservation we own, then return any advice charge.
                release_large_mapping(thp_charge, reservation.as_ptr(), reservation_len);
                return (core::ptr::null_mut(), true);
            }
        };
        // Lay down the large header. The allocation lives at `hdr_aligned`.
        let bump = hdr_aligned + size;
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
        // SAFETY: this fresh Large mapping has an initialized header and aligned,
        // exclusively writable terminal storage. Registration exposes routing data,
        // but no payload has been issued and neither word has readers or loans.
        unsafe { SegmentMeta::new(base).init_large_terminal() };
        if thp_charge != 0 {
            // SAFETY: the aligned live Large header is initialized above; only this owner accesses the charge.
            unsafe { SegmentHeader::set_thp_charge_at(base, thp_charge) };
        }
        #[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
        // SAFETY: freshly constructed live segment under this owner's exclusive access; plain field.
        let () = unsafe { SegmentHeader::set_thp_observation_at(base, _thp_observation) };
        // Phase C (numa-aware): stamp NUMA node post-write (constructor set NO_NODE_RAW).
        #[cfg(feature = "numa-aware")]
        // SAFETY: `base` is the owner's live, initialized Large header root.
        unsafe { SegmentMeta::new(base) }.set_node_id(my_node);

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
    /// Called by the `HeapCore` terminal large-sidecar-ingress drain
    /// (`drain_large_sidecar_ingress`) on the owner's
    /// `alloc_large` slow-path, once per queued base.
    #[cfg(feature = "alloc-xthread")]
    // Sole caller (`retire_large_route`, alloc_core/sidecar_drain.rs), live only
    // under all(feature = "alloc-global", feature = "alloc-xthread").
    #[cfg_attr(
        not(all(feature = "alloc-global", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    #[allow(unsafe_code)] // R25-1: item-scoped; owner-reclaim unsafe blocks below.
    pub(crate) fn reclaim_large_segment(&mut self, base: *mut u8) {
        // SAFETY: forwarded owner-exclusive live-segment/table contract (P2.1 facade).
        #[allow(unsafe_code)]
        let terminal_meta = unsafe { SegmentMeta::new(base) };
        let state = LargeReservationState::new(terminal_meta.large_state_atomic());
        // The sidecar has transferred one consume obligation. This physical
        // owner-only lifecycle credit is claimed once, never producer-mutated.
        let Some(generation) = state.claim_live() else {
            fatal(b"large: live claim credit missing\n");
        };
        // SAFETY: `base` is the live registered large segment being reclaimed
        // under this owner's exclusive lifecycle step; header read only.
        let hdr = unsafe { SegmentHeader::read_at(base) };
        // A foreign realloc may have copied this mapping's payload without
        // updating its owner-only token. Consume remote controls conservatively;
        // only the owner-free path may sample a live token.
        #[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
        {
            // SAFETY (both blocks): live Large segment reclaimed under exclusive owner lifecycle.
            let token = unsafe { SegmentHeader::thp_observation_at(base) };
            if token != 0 {
                let () = unsafe { SegmentHeader::set_thp_observation_at(base, 0) };
                if matches!(token, 1..=6) {
                    ThpRuntime::note_invalidated_observation(hdr.large_size);
                } else if matches!(token, 9..=14) {
                    ThpRuntime::note_premise_failure(hdr.large_size);
                } else {
                    ThpRuntime::note_observation_error(hdr.large_size);
                }
            }
        }
        // Remove from the table FIRST (frees the slot for reuse regardless of
        // which branch below runs) — mirrors the own-thread cache-deposit
        // ordering in `dealloc`'s Large branch.
        // SAFETY: forwarded owner-exclusive live-segment/table contract (P2.1 facade).
        #[allow(unsafe_code)]
        unsafe {
            self.table.unregister(base)
        };

        #[cfg(feature = "alloc-decommit")]
        {
            self.maybe_decay_large_cache();
            // R6-T31: pool decay tick on the same cold cadence as the large cache.
            self.maybe_decay_small_pool();
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

            // R13-7 (task #277): free-slot search spans the combined
            // base+extension index space; R14-5 (task #290, review @fm P3):
            // budget-before-materialisation pre-check — both mirror
            // `AllocCore::dealloc`'s Large branch (`alloc_core.rs`); see
            // `large_cache_deposit_budget_infeasible`'s doc for rationale.
            self.large_cache_note_free(usable_size);
            let mut admitted: Option<usize> = None;
            if hdr.large_align < SEGMENT && !self.large_cache_deposit_budget_infeasible(usable_size)
            {
                loop {
                    let free_slot = self.large_cache_find_free_slot();
                    let budget_ok = self
                        .large_cache_effective_budget()
                        .is_none_or(|budget| self.large_cache_used_bytes + usable_size <= budget);
                    if let Some(idx) = free_slot {
                        if budget_ok {
                            admitted = Some(idx);
                            break;
                        }
                    } else if budget_ok
                        && self.large_cache_try_grow_slots(self.large_cache_used_bytes, usable_size)
                    {
                        continue; // J14: the learned target opened more slots
                    }
                    if !self.evict_oldest_noting_gone() {
                        break;
                    }
                }
            }

            if let Some(slot_idx) = admitted {
                // The segment is removed from the active table before cache
                // publication. Its legacy magic field is cleared atomically
                // as owner-side lifecycle state; foreign frees use route
                // descriptors and sidecars rather than reading this header.
                let magic_off = core::mem::offset_of!(SegmentHeader, magic);
                Node::atomic_u32_at(base, magic_off)
                    .store(0, core::sync::atomic::Ordering::Release);
                if !state.cache_consumed(generation) {
                    fatal(b"large: consumed cache transition failed\n");
                }
                let seq = self.large_cache_seq;
                self.large_cache_seq = self.large_cache_seq.wrapping_add(1);
                self.large_cache_slot_set(
                    slot_idx,
                    CachedLarge {
                        reservation: hdr.reservation,
                        reservation_len: hdr.reservation_len,
                        base,
                        usable_size,
                        reserved_capacity,
                        seq,
                    },
                );
                self.large_cache_used_bytes += usable_size;
                return;
            }
            if hdr.large_align < SEGMENT {
                self.large_cache_note_gone(usable_size); // J14: refused under pressure
            }
        }

        // No `alloc-decommit` cache (or cache admission declined): release
        // the OS reservation immediately. The slot is already unregistered
        // above, so there is no dangling table entry pointing at unmapped
        // memory.
        if !state.release_consumed(generation) {
            fatal(b"large: consumed release failed\n");
        }
        release_large_mapping(hdr.thp_charge, hdr.reservation, hdr.reservation_len);
    }
}
