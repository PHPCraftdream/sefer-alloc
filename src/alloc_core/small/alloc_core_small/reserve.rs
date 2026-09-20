//! Small-segment reservation — `reserve_small_segment` (cursor-publishing
//! wrapper) and `reserve_small_segment_impl` (cursor-free body; mechanical
//! split of the former flat `alloc_core_small.rs`; pure code movement, no
//! behavior changed).

#[cfg(feature = "numa-aware")]
use crate::alloc_core::numa;
#[cfg(not(any(feature = "numa-aware", feature = "small-segment-lazy-commit")))]
use crate::alloc_core::os::Segment;
use crate::alloc_core::os::{self, SEGMENT};
use crate::alloc_core::segment_header::{
    BinTable, Layout as SegLayout, SegmentHeader, SegmentMeta,
};
// R12-11 (task #262): `PageMap::init_in_place` is diagnostic-only (see its
// doc) and its sole call site in this file is gated behind `page-map-diag`.
#[cfg(feature = "page-map-diag")]
use crate::alloc_core::segment_header::PageMap;

use crate::alloc_core::alloc_core::{base_add, AllocCore};

// ── end Phase 3 ──────────────────────────────────────────────────────────
impl AllocCore {
    /// Reserve a fresh small segment, initialise its metadata, register it,
    /// and set it as the current small segment. Returns its base.
    ///
    /// Thin wrapper around [`reserve_small_segment_impl`](Self::reserve_small_segment_impl)
    /// that additionally publishes the freshly reserved segment as the live
    /// bump-carve cursor (`self.small_cur`). Every PRODUCTION caller
    /// (`alloc_small`, `alloc_small_with_virgin`, `refill_class_bump_impl`)
    /// needs exactly that: the newly reserved segment becomes the carve
    /// target for the retry that follows. See
    /// `reserve_small_segment_impl`'s own doc for why a measurement-only
    /// caller must NOT go through this wrapper.
    #[inline]
    pub(in crate::alloc_core) fn reserve_small_segment(&mut self) -> Option<*mut u8> {
        let base = self.reserve_small_segment_impl()?;
        self.small_cur = base;
        Some(base)
    }

    /// R30-1 (task #450): cursor-free half of [`reserve_small_segment`]'s
    /// body — reserves+registers+initialises a fresh small segment exactly
    /// as `reserve_small_segment` does, but returns WITHOUT publishing it as
    /// `self.small_cur`.
    ///
    /// ## Why this split exists (soundness fix, R30-1)
    ///
    /// `self.small_cur` is the live bump-carve cursor read by every small
    /// alloc dispatch (`alloc_small`/`alloc_small_with_virgin`'s step 1,
    /// `self.pop_free(self.small_cur, ...)`). `reserve_small_segment`'s
    /// FORMER single body set it unconditionally as its last statement —
    /// correct for the three production callers, which all immediately
    /// retry an allocation against the segment they just reserved. But the
    /// R29-3 measurement hooks (`dbg_decomp_full_cycle`,
    /// `dbg_decomp_reserve_and_keep`) called that same
    /// `reserve_small_segment` purely to measure OS/table/metadata cost,
    /// then immediately released the segment via
    /// `release_or_pool_empty_segment` — which, when the hysteresis pool is
    /// full, genuinely releases the OS reservation and recycles the table
    /// slot. Neither hook restored `small_cur` afterward, so it was left
    /// dangling at an unmapped segment; the next ordinary small alloc on
    /// that heap would read through it (`pop_free(self.small_cur, ...)`) —
    /// a use-after-free. See `docs/CORRECTNESS_OPEN_ITEMS.md` item 5 for the
    /// full confirmed trace.
    ///
    /// This function is the fix: a measurement-only caller that has no
    /// intention of allocating from the segment it just reserved (it only
    /// wants to measure reserve/release cost, or first-touch/decommit cost
    /// on an isolated segment) calls THIS instead of `reserve_small_segment`,
    /// so `self.small_cur` — and therefore every other in-flight allocation
    /// on this heap — is never disturbed. `pub(in crate::alloc_core)` (not further gated
    /// itself): the `bench-internals` gate lives on the `dbg_*` callers in
    /// `alloc_core_small_pool.rs`, matching how `reserve_small_segment`
    /// itself is visibility-scoped.
    ///
    /// Production callers MUST keep using `reserve_small_segment` (the
    /// cursor-publishing wrapper above) — this function is not a drop-in
    /// replacement for them; skipping the cursor publish would leave a
    /// freshly reserved segment unreachable as a carve target.
    pub(in crate::alloc_core) fn reserve_small_segment_impl(&mut self) -> Option<*mut u8> {
        // Mechanism 2 (task #51): this path is reached only when NO registered
        // segment — including any POOLED empty segment — has a free block of the
        // requested class (`find_segment_with_free` already scanned them all,
        // pooled included, and REMOVED any it reused from the pool). A pooled
        // segment is fully-carved (bump near `SEGMENT` end), so it cannot serve
        // as a FRESH carve target for a class its free list lacks — that is why
        // the pool is drawn from via `find_segment_with_free`'s free-list reuse
        // (the hysteresis win: the emptied segment's blocks are re-served with
        // no OS work), NOT popped as `small_cur` here.
        //
        // R8-10 (task #223): this holds identically under `alloc-lazy-commit`.
        // A prior design (B3, R7 Workstream B) popped a pooled segment here as
        // a "clean carve target", relying on pool admission having decommit-
        // reset it (bump at payload_start, free lists cleared,
        // `is_decommitted=true`). That reset was itself the 50-75× regression
        // (see `release_or_pool_empty_segment`'s doc comment) — with admission
        // fixed to never reset, a pooled segment under lazy-commit is a
        // partially-carved segment with a live free list, indistinguishable
        // from the eager leg's pooled segment. It is therefore reused the same
        // way: via `find_segment_with_free`'s free-list path, never popped here.
        //
        // This is the cold small-path clock edge, so trim any stale pooled
        // segment here (cheap: fast early-exit when the pool is empty; one
        // `Instant::now()` at most, only when something is pooled).
        #[cfg(feature = "alloc-decommit")]
        self.maybe_decay_small_pool();

        // Phase C (numa-aware): determine the calling thread's NUMA node
        // BEFORE the reservation so we can pass it to `reserve_aligned_on_node`
        // (Windows requires the node at reserve-time via VirtualAllocExNuma;
        // Linux can bind post-mmap, but we unify the paths here). R11-5: this
        // is the cached accessor — typically the same value the most recent
        // `find_segment_with_free` already populated.
        #[cfg(feature = "numa-aware")]
        let my_node = self.current_node_cached();

        // Reserve one SEGMENT's worth of virtual address space.
        // Under numa-aware we call the NUMA-steering path; otherwise the plain
        // OS path.  The returned triple always provides (base, reservation,
        // reservation_len) with the same semantics as Segment::reserve.
        #[cfg(feature = "numa-aware")]
        let (base, reservation, reservation_len) = {
            let reserved = numa::reserve_aligned_on_node(SEGMENT, my_node);
            // Mechanism 2 (task #51 / follow-up): pool-drain-and-retry on OS
            // reservation failure, mirroring `alloc_large`'s identical guard
            // — the pool is a reclaimable soft reserve, never a hard pin
            // under memory pressure, even for a plain small-segment reserve.
            #[cfg(feature = "alloc-decommit")]
            let reserved = match reserved {
                Some(t) => Some(t),
                None if self.pooled_count > 0 => {
                    self.drain_small_pool();
                    numa::reserve_aligned_on_node(SEGMENT, my_node)
                }
                None => None,
            };
            let (b, r, rl) = reserved?;
            (b.as_ptr(), r, rl)
        };
        #[cfg(not(feature = "numa-aware"))]
        let (base, reservation, reservation_len) = {
            // B1 (R7 Workstream B): under `small-segment-lazy-commit`
            // (Windows-only lazy path; Unix/miri eager fallback), reserve the
            // 4 MiB segment WITHOUT committing the whole payload. Only the
            // metadata region and the first payload chunk are committed; the
            // rest stays reserved-but-uncommitted. On the eager path
            // (feature-OFF) or under miri/Unix, `reserve_aligned_lazy` falls
            // back to the eager `reserve_aligned` internally, so the
            // observable behavior is identical.
            //
            // The NUMA path (above) always uses the eager
            // `reserve_aligned_on_node` — NUMA reservations go through
            // VirtualAllocExNuma and must not be disturbed (P2 gate).
            //
            // R12-9 (task #260): gated on `small-segment-lazy-commit`
            // specifically — this is the ONE call site that decides whether
            // an ORDINARY (non-primordial) small segment is reserved lazily;
            // enabling ONLY `primordial-lazy-commit` (without
            // `small-segment-lazy-commit`) leaves this arm on the eager
            // `Segment::reserve` path.
            #[cfg(feature = "small-segment-lazy-commit")]
            let mut seg = {
                let meta_end = SegLayout::small_meta_end();
                // initial_commit = metadata pages + first payload chunk,
                // rounded UP to the RUNTIME OS page size via
                // `SegLayout::lazy_initial_commit` (task #1074). The tight sum
                // is aligned only to the COMPILE-TIME `PAGE` (4 KiB) — `meta_end`
                // is a `const fn` and cannot query the real page size — so on a
                // 16/64 KiB-page host it is NOT a `page_size()` multiple and
                // `aligned_vmem::validate_initial_commit` (commit dd6d027,
                // task #1037) rejects the reservation, failing
                // `AllocCore::new()` wholesale (the macOS ARM64 release
                // blocker, CI run 32083383999).
                let initial_commit =
                    SegLayout::lazy_initial_commit(meta_end, aligned_vmem::page_size());
                // `initial_commit <= SEGMENT` by construction: the tight sum is
                // 335872 B (328 KiB, non-hardened) or 598016 B (584 KiB,
                // hardened), and the round-up adds less than one
                // MAX_REALISTIC_PAGE_SIZE (64 KiB) — the const-assert at the
                // top of this file pins the sum PLUS that slack within SEGMENT.
                debug_assert!(initial_commit <= SEGMENT);
                aligned_vmem::reserve_aligned_lazy(SEGMENT, SEGMENT, initial_commit).inspect(|_| {
                    os::SEGMENTS_RESERVED_TOTAL.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                })
            };
            // `mut` is needed under `alloc-decommit` (the pool-drain-and-retry
            // arm below reassigns `seg`). Silence the unused-mut warning when
            // `alloc-decommit` is off and this binding is never reassigned.
            #[cfg(not(feature = "small-segment-lazy-commit"))]
            #[allow(unused_mut)]
            let mut seg = Segment::reserve(SEGMENT);
            // Mechanism 2 (task #51 / follow-up): same pool-drain-and-retry
            // guard as the numa-aware arm above.
            #[cfg(feature = "alloc-decommit")]
            if seg.is_none() && self.pooled_count > 0 {
                self.drain_small_pool();
                #[cfg(feature = "small-segment-lazy-commit")]
                {
                    let meta_end = SegLayout::small_meta_end();
                    let initial_commit =
                        SegLayout::lazy_initial_commit(meta_end, aligned_vmem::page_size());
                    seg = aligned_vmem::reserve_aligned_lazy(SEGMENT, SEGMENT, initial_commit)
                        .inspect(|_| {
                            os::SEGMENTS_RESERVED_TOTAL
                                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        });
                }
                #[cfg(not(feature = "small-segment-lazy-commit"))]
                {
                    seg = Segment::reserve(SEGMENT);
                }
            }
            #[cfg(feature = "small-segment-lazy-commit")]
            {
                // `into_reservation()`: aligned-vmem's lazy constructors now hand
                // back a `LazyReservation`, which tracks the commit watermark for
                // callers that want the crate to do that bookkeeping. THIS caller
                // does not: the allocator keeps its own frontier
                // (`committed_payload_end`) INSIDE the mapped segment header,
                // because the hot allocation path reaches it by masking a bare
                // pointer and has no handle in scope. So take the explicit door
                // out and own the commit state from here on.
                let reservation = seg?.into_reservation();
                let b = reservation.as_ptr();
                // `reservation_ptr()` is always non-null per the
                // aligned_vmem::Reservation contract (checked at construction).
                // Use `new` + `?` to propagate as OOM rather than panic.
                let r = core::ptr::NonNull::new(reservation.reservation_ptr())?;
                let rl = reservation.reservation_len();
                core::mem::forget(reservation);
                (b, r, rl)
            }
            #[cfg(not(feature = "small-segment-lazy-commit"))]
            {
                let segment = seg?;
                let b = segment.as_ptr();
                let r = segment.reservation();
                let rl = segment.reservation_len();
                core::mem::forget(segment);
                (b, r, rl)
            }
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
        // R12-11 (task #262): only feeds the diagnostic-only `PageMap::init_in_place`
        // below; unused without `page-map-diag`.
        #[cfg(feature = "page-map-diag")]
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

        // R12-10 (task #261, `virgin-zero-skip`): stamp the payload-virgin
        // bit. This is a genuinely fresh OS reservation (the segment was just
        // registered above; it is not a decommit-reused segment — that path
        // is `decommit_empty_segment_impl`'s `release_follows=false` leg,
        // which stamps its OWN `false` on that different code path). Every
        // real OS backend zero-fills a fresh reservation (Windows
        // `VirtualAlloc` MEM_COMMIT demand-zero; Unix anonymous `mmap`
        // zero-fill), so `true` is correct there. Under `cfg!(miri)`,
        // `crates/aligned-vmem`'s aperture falls back to bare `std::alloc::alloc`,
        // which does NOT zero — so the bit is withheld (`false`) under miri,
        // mirroring `alloc_large_slow`'s identical `cfg!(not(miri))` freshness
        // gate (task #221/R9-1). This must run BEFORE any carve on this
        // segment; the very first thing that reads this bit is
        // `carve_block`/`carve_batch`.
        #[cfg(feature = "virgin-zero-skip")]
        meta.set_payload_virgin(cfg!(not(miri)));

        // B1 (R7 Workstream B): stamp the committed-payload frontier.
        //
        // Three-way split, mirroring the THREE platform-specific
        // `reserve_aligned_lazy_raw` implementations in
        // `crates/aligned-vmem/src/lib.rs` (the OS-level reservation this function
        // just completed via `Segment::reserve_lazy`):
        //
        //   1. `numa-aware` (any platform): `SEGMENT`. NUMA reservations go
        //      through `numa::reserve_aligned_on_node`, which is ALWAYS eager
        //      (P2 gate — `VirtualAllocExNuma` reservations must not be
        //      disturbed by a later partial commit).
        //
        //   2. `small-segment-lazy-commit` AND NOT `numa-aware` AND real
        //      Windows (not miri): the page-rounded `meta_end +
        //      LAZY_FIRST_CHUNK` (task #1074 — rounded UP to the runtime OS
        //      page size, exactly what the reservation committed; on the 4
        //      KiB pages every real Windows host has, identical to the tight
        //      sum). This is the
        //      ONLY platform where `reserve_aligned_lazy_raw` is GENUINELY
        //      lazy — a real 2-phase `VirtualAlloc(MEM_RESERVE)` then
        //      `VirtualAlloc(MEM_COMMIT)` on the metadata + first chunk
        //      prefix. The frontier accurately reflects that partial commit.
        //
        //   3. `small-segment-lazy-commit` AND NOT `numa-aware` AND
        //      Unix/miri: `SEGMENT`. Here `reserve_aligned_lazy_raw` ignores
        //      `_initial_commit` and `mmap`s / `alloc`s the WHOLE segment up
        //      front (Unix has no separate reserve/commit distinction the way
        //      Windows does; miri models no RSS). Understating the frontier
        //      at `meta_end + LAZY_FIRST_CHUNK` here used to be SOUND but
        //      POINTLESS (R8-5, task #218): every carve past the artificial
        //      frontier still ran through B2's grow-on-carve path (bounds
        //      check + a `commit_pages` call that is a correctness no-op on
        //      these platforms per `crates/aligned-vmem`'s own `commit_range_impl`
        //      for unix/miri + an atomic `GROW_COMMIT_COUNT` bump) for zero
        //      behavioral benefit. Stamping `SEGMENT` immediately restores
        //      the feature's promised zero-cost-when-unneeded property on
        //      Unix/miri.
        //
        // B2 wired: grow-on-carve is still live on the genuine Windows-lazy
        // path (case 2) — when a carve would exceed `committed_payload_end`,
        // the carve path commits the next chunk(s) and advances the frontier
        // before advancing bump. The first chunk (LAZY_FIRST_CHUNK) covers
        // initial carving; subsequent chunks are grown incrementally
        // (GROW_CHUNK) by carve_block/carve_batch. B3 will reset the frontier
        // after a decommit; B5 sweeps the chunk sizes.
        //
        // R12-9 (task #260): the OUTER gate here is the SHARED `any(...)`
        // condition, not `small-segment-lazy-commit` alone. Reasoning: the
        // `committed_payload_end` field defaults to `0` at construction
        // (`SegmentHeader::small`'s doc: "the caller (reserve_small_segment)
        // stamps the real value immediately... on the eager path: SEGMENT").
        // Whenever EITHER split sub-feature is on, `carve_block`/
        // `carve_batch`'s shared B2 grow-on-carve check compiles in and reads
        // this field on EVERY carve — including carves into THIS (ordinary
        // small) segment even when `small-segment-lazy-commit` itself is OFF
        // (e.g. `primordial-lazy-commit`-only builds). An unstamped `0`
        // frontier would then look like "nothing committed yet" and trigger
        // a spurious grow-on-carve commit on the very first carve. So this
        // segment MUST always be stamped when grow-on-carve can run at all;
        // the VALUE (lazy vs eager) still depends on `small-segment-lazy-commit`
        // specifically, matching step 1's reservation gate above.
        #[cfg(any(
            feature = "primordial-lazy-commit",
            feature = "small-segment-lazy-commit"
        ))]
        {
            #[cfg(all(feature = "small-segment-lazy-commit", feature = "numa-aware"))]
            meta.set_committed_payload_end(SEGMENT);
            #[cfg(all(
                feature = "small-segment-lazy-commit",
                not(feature = "numa-aware"),
                windows,
                not(miri)
            ))]
            meta.set_committed_payload_end(SegLayout::lazy_initial_commit(
                meta_end,
                aligned_vmem::page_size(),
            ));
            #[cfg(all(
                feature = "small-segment-lazy-commit",
                not(feature = "numa-aware"),
                any(not(windows), miri)
            ))]
            meta.set_committed_payload_end(SEGMENT);
            // `small-segment-lazy-commit` OFF (only `primordial-lazy-commit`
            // is on): this segment was reserved EAGERLY (step 1's `else` arm
            // — the plain `Segment::reserve`), so the frontier must be
            // stamped `SEGMENT` unconditionally, on every platform.
            #[cfg(not(feature = "small-segment-lazy-commit"))]
            meta.set_committed_payload_end(SEGMENT);
        }

        // R12-11 (task #262): diagnostic-only bookkeeping — see `PageMap`'s
        // struct doc. Gated behind `page-map-diag` and elided from the
        // default/production segment-reservation path.
        #[cfg(feature = "page-map-diag")]
        PageMap::init_in_place(base_add(base, SegLayout::page_map_off()), meta_pages);
        BinTable::init_in_place(base_add(base, SegLayout::bin_table_off()) as *mut u32);
        // Initialise the per-segment alloc-bitmap (Phase 13.4a double-free
        // guard) to all-zeros; bits flip to FREE as blocks are pushed.
        //
        // PERF-PASS-2 (G5/C1, task #50): under `cfg(not(miri))` this init is
        // SKIPPED — `base` is a segment JUST reserved fresh from the OS via
        // `Segment::reserve`/`numa::reserve_aligned_on_node` a few lines above
        // (never carved, never decommit-reset), and the OS guarantees fresh
        // pages read as zero (Windows `MEM_COMMIT` demand-zero; POSIX
        // anonymous `mmap` zero-fill — see `crates/aligned-vmem/src/lib.rs`'s reserve
        // paths). `AllocBitmap::init_in_place`'s target state is ALL ZEROS
        // (see its doc comment), so writing zero over memory the OS already
        // handed back as zero is a tautology. Skipping it avoids dirtying
        // `AllocBitmap::FOOTPRINT` (32 KiB / 8 pages for the default
        // SEGMENT/MIN_BLOCK pair) of metadata pages that would otherwise fault
        // in eagerly instead of lazily.
        //
        // Under `miri` this is NOT skipped: `crates/aligned-vmem/src/lib.rs`'s miri
        // fallback aperture is `std::alloc::alloc`, which is NOT guaranteed
        // zeroed — so miri keeps the explicit zero-init, exactly as before.
        //
        // This is NOT the rejected P4(b) `alloc_zeroed` virgin-skip (that NO-GO
        // was about *user-visible payload* virginity, where macOS
        // `MADV_DONTNEED` laziness on a RECYCLED (not freshly-reserved) mapping
        // makes "recycled == zero" an unsound assumption). Here the virgin
        // signal is exact — this function only ever runs immediately after a
        // fresh OS reservation, never on a decommit-reused segment (that path
        // is `decommit_empty_segment_impl`'s `release_follows=false` full
        // reset, which keeps its own explicit `AllocBitmap::init_in_place`
        // call unconditionally — see PERF-PASS-2 report / task #50) — and it
        // is metadata, not payload the user could have observed/mutated.
        #[cfg(miri)]
        crate::alloc_core::alloc_bitmap::AllocBitmap::init_in_place(base_add(
            base,
            SegLayout::alloc_bitmap_off(),
        ));
        // RAD-5 (E4) GO/NO-GO EXPERIMENT: same virgin-skip discipline extended
        // to the second (magazine-residency) bitmap — see
        // `magazine_bitmap.rs`'s module doc. Skipped under `cfg(not(miri))`
        // for the identical reason as the line above.
        #[cfg(miri)]
        crate::alloc_core::magazine_bitmap::MagazineBitmap::init_in_place(base_add(
            base,
            SegLayout::magazine_bitmap_off(),
        ));
        // Initialise the per-segment remote-free ring (Variant-2 fix). Only
        // under `alloc-xthread`; the Layout always reserves the bytes.
        #[cfg(feature = "alloc-xthread")]
        {
            crate::alloc_core::remote_free_ring::RemoteFreeRing::init_in_place(
                base,
                SegLayout::remote_ring_off(),
            );
        }
        // X7 Ф3 (task #191): zero the per-segment generation table under
        // `hardened`. Compiled ONLY under `hardened`; under any other feature
        // the table does not exist and this call is absent (byte-identical to
        // the pre-X7 build). Closes the carried-over Ф1 gap: without this
        // zeroing, a `gen_at`/`bump_gen` Relaxed load on a never-written cell
        // is UB. NOT re-zeroed on decommit-reset (plan §2.2: generation
        // numbering is continuous across decommit-reset by design).
        #[cfg(feature = "hardened")]
        {
            // SAFETY: `base` is a live, exclusively-owned segment whose
            // generation table is carved and writable.
            #[allow(unsafe_code)]
            unsafe {
                crate::alloc_core::segment_header::init_gen_table_in_place(base)
            };
        }
        // R7-A1: check whether the segment count has crossed the directory
        // materialisation threshold. If so, materialize the sidecar and do
        // the one-time rebuild. This is a lazy, one-shot operation: once the
        // pointer is non-null, subsequent calls are a single null-check.
        #[cfg(feature = "alloc-segment-directory")]
        self.maybe_materialize_directory();

        Some(base)
    }
}
