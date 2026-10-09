//! The M6 decommit cluster — the `dbg_*_for`/`dbg_force_decommit_retain_for`
//! test hooks and the shared `decommit_empty_segment_for_release`/`_impl`
//! reset body (mechanical split of the former flat `alloc_core_small_pool.rs`;
//! pure code movement, no behavior changed).

use crate::alloc_core::node::Node;
use crate::alloc_core::os::{self, SEGMENT};
use crate::alloc_core::segment_header::{Layout as SegLayout, SegmentMeta, FREE_LIST_NULL};
// `SegmentHeader`/`SegmentKind` are consulted here only by the
// `internals`-gated `dbg_*_for`/`dbg_force_decommit_retain_for` test hooks —
// the production decommit body consults neither — so gate the imports
// identically.
#[cfg(feature = "internals")]
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind};

use crate::alloc_core::alloc_core::{AllocCore, DECOMMIT_CALLS};

impl AllocCore {
    /// TEST-ONLY (Phase 35): whether `ptr`'s segment is currently decommitted, or
    /// `None` if `ptr` is foreign / not small/primordial.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_is_decommitted_for(&self, ptr: *mut u8) -> Option<bool> {
        let candidate = os::segment_base_of_ptr(ptr);
        let base = self.table.canonical_base_of(candidate)?;
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return None;
        }
        Some(SegmentMeta::new(base).is_decommitted())
    }

    /// TEST-ONLY (R12-10, task #261, `virgin-zero-skip`): force the
    /// `release_follows == false` (decommit-and-RETAIN) leg of
    /// `decommit_empty_segment_impl` to run on `ptr`'s segment, bypassing the
    /// fact that this leg has ZERO production callers today (see that
    /// function's doc). Exists so `tests/alloc_zeroed_virgin_small_skip.rs`
    /// can prove the defensive `payload_virgin = false` clear at that site
    /// (§3 reset table in both design docs) actually fires — the regression
    /// guard the design docs flagged as needed "if that path is ever
    /// re-enabled". Returns `false` (no-op) if `ptr` is foreign / not a
    /// `Small` segment specifically; the caller is responsible for having
    /// emptied the segment first (this hook does NOT check `live_count` —
    /// it drives the shared decommit body directly, matching what a real
    /// caller would have already established).
    ///
    /// **Excludes `Primordial` deliberately** — same exclusion
    /// `dec_live_and_maybe_decommit` enforces ("NEVER decommit the
    /// PRIMORDIAL segment": its metadata extends to
    /// `primordial_meta_end()`, but `decommit_empty_segment_impl` computes
    /// `payload_start` from the (smaller) `small_meta_end()`; decommitting
    /// from there would unmap part of the self-hosted registry the
    /// primordial segment hosts, corrupting the substrate). This test hook
    /// bypasses the LIVE-COUNT check, not the segment-KIND safety
    /// invariant — calling it on the primordial segment would be a genuine
    /// use-after-free of the registry, not merely a test artefact.
    ///
    /// # Safety
    ///
    /// The caller MUST guarantee the segment at `ptr`'s base has
    /// `live_count == 0` — i.e. every block previously carved from that
    /// segment has already been freed — BEFORE calling this hook. It drives
    /// `decommit_empty_segment_impl` directly, which returns the segment's
    /// PAYLOAD pages to the OS (`os::decommit_pages`), resets the bump cursor,
    /// empties every class free list, and re-zeros the alloc bitmap. This hook
    /// deliberately does NOT verify `live_count` — the very reason a real
    /// production caller would have invoked it after observing the segment go
    /// empty. Calling it on a segment that still owns LIVE allocations
    /// decommits the backing pages of those live blocks; the next access
    /// through any of them is a use-after-free / access violation. (The
    /// `contains_base_ro` / `Small`-kind checks below reject foreign or
    /// non-`Small` pointers by returning `false`, but they say nothing about
    /// `live_count`.)
    ///
    /// `ptr` must be an allocator-issued raw pointer into this `AllocCore`'s
    /// still-live, mapped Small reservation, with provenance/access permission
    /// for the initialized header, BinTable and allocation bitmap read/reset
    /// through the derived base, and authority to decommit its payload extent.
    /// The segment must be exclusively owned for the entire call, without
    /// conflicting aliases, pending frees, or concurrent metadata/payload
    /// accesses. A payload-only reference/slice or `Box` reborrow tag is
    /// insufficient unless it still authorizes those metadata regions;
    /// `contains_base_ro` checks an address, not provenance or access rights.
    /// A saved raw block pointer after logical free, or a reservation handle's
    /// raw base, is permitted only while that same reservation remains owned
    /// and mapped; it need not designate a currently allocated payload block.
    /// Do not use it after reservation release, and do not access decommitted
    /// payload until the allocator has recommitted/reissued it.
    // R29-8 (task #439): `pub unsafe fn` + `bench-internals`-gated. This hook
    // resolves `ptr`'s segment base, checks `contains_base_ro` + `Small` kind,
    // then decommits the payload with NO `live_count` check — a direct instance
    // of the R25-1 (task #395) safe-`pub fn`-that-touches-allocator-state hole
    // CLAUDE.md's benchmark-hook rule targets. `AllocCore` is a crate-root
    // re-exported public type (`src/lib.rs`), so this was reachable from 100%
    // safe code under plain `--features production` (alloc-decommit) — worse
    // exposure than R29-7's `#[doc(hidden)]`-module item. NEW tier-2 site:
    // this file is NOT a tier-1 seam module (no `#![allow(unsafe_code)]`), so
    // the item-level `#[allow(unsafe_code)]` below is required; it mirrors
    // `dbg_dealloc_own_thread_with_base` / `dbg_flush_class_only` in
    // `heap_core_diag.rs`. The body forwards to the SAFE
    // `decommit_empty_segment_impl`; the `unsafe fn` signature exists solely
    // to enforce the `live_count == 0` precondition at the call site.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[allow(unsafe_code)] // R29-8: `unsafe fn` boundary (live_count==0 precondition).
    pub unsafe fn dbg_force_decommit_retain_for(&self, ptr: *mut u8) -> bool {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base_ro(base) {
            return false;
        }
        if !matches!(SegmentHeader::kind_at(base), SegmentKind::Small) {
            return false;
        }
        let mut meta = SegmentMeta::new(base);
        Self::decommit_empty_segment_impl(&mut meta, base, false);
        true
    }

    /// PERF-4 (task #14): release-leg metadata reset for an empty Small segment.
    /// Pool admission retains committed pages and intact free lists without
    /// this reset. When release follows, reset the bump cursor and flag,
    /// skipping payload decommit and full metadata initialization before
    /// the whole reservation is recycled. Historical investigation:
    /// `docs/checkpoints/2026-07-08-perf4-decommit-churn-investigation.md`.
    #[cfg(feature = "alloc-decommit")]
    pub(super) fn decommit_empty_segment_for_release(meta: &mut SegmentMeta, base: *mut u8) {
        Self::decommit_empty_segment_impl(meta, base, true);
    }

    /// `release_follows == true` selects the minimal reset before whole-
    /// reservation release. The retain-reset branch is exercised by the
    /// explicit decommit-retain test hook, not production pool admission.
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    fn decommit_empty_segment_impl(meta: &mut SegmentMeta, base: *mut u8, release_follows: bool) {
        // Test seam: count the invocation (diagnostic; relaxed). Counted on BOTH
        // variants so the soak / regression tests (`dbg_decommit_count`) observe
        // the same number of decommit events as before this optimization.
        DECOMMIT_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let payload_start = SegLayout::small_meta_end();
        if release_follows {
            // Reset the bump cursor and flag before whole-reservation release;
            // no payload decommit or full metadata reset is needed.
            meta.set_bump(payload_start);
            meta.set_decommitted(true);
            return;
        }
        // B3 (R7 Workstream B): lazy-commit-aware retain decommit.
        //
        // The explicit decommit-retain hook selects this Small-only branch.
        // Under `small-segment-lazy-commit` (R12-9, task #260), decommit only
        // the payload pages
        // ABOVE the initial lazy chunk: `[meta_end + LAZY_FIRST_CHUNK,
        // SEGMENT)`. The initial chunk `[meta_end, meta_end +
        // LAZY_FIRST_CHUNK)` stays committed so the reused segment is
        // immediately carveable without a recommit syscall (fault-free,
        // matching a freshly reserved lazy segment). The frontier is reset
        // to `small_decommit_start() + LAZY_FIRST_CHUNK` — arithmetically
        // IDENTICAL to the page-rounded value a fresh `reserve_small_segment`
        // commits under the lazy path (`Layout::lazy_initial_commit`, task
        // #1074: LAZY_FIRST_CHUNK is a multiple of every supported page
        // size, so the round-up distributes over the sum).
        //
        // On the eager path (feature-OFF, Unix, miri, numa-aware), the whole
        // payload `[meta_end, SEGMENT)` is decommitted as before, and the
        // frontier is not touched (it is SEGMENT throughout on the eager path).
        // This keeps the feature-OFF behaviour byte-identical.
        //
        // Metadata in `[0, meta_end)` is outside the decommit range.
        // Terminal ingress lives in independent sidecars, not this payload.
        #[cfg(feature = "small-segment-lazy-commit")]
        {
            // R8-6 (task #219): the decommit boundary must be REAL-OS-page-
            // aligned. `LAZY_FIRST_CHUNK` (256 KiB) is a multiple of every
            // realistic page size, but `payload_start + LAZY_FIRST_CHUNK`
            // inherits `payload_start`'s residue modulo the real page size —
            // so on a 16/64 KiB-page machine where `payload_start` (= the
            // TIGHT `small_meta_end()`) is only 4 KiB aligned, the naive sum
            // would land mid-real-page and the OS would silently round the
            // decommit boundary, reclaiming part of the initial chunk that
            // must stay committed for fault-free reuse. Compute the boundary
            // from the real-page-safe `small_decommit_start()` instead.
            let initial_frontier = SegLayout::small_decommit_start()
                + crate::alloc_core::alloc_core_small::LAZY_FIRST_CHUNK;
            // Decommit only above the initial chunk.
            os::decommit_pages(base, initial_frontier, SEGMENT);
            meta.set_committed_payload_end(initial_frontier);
        }
        #[cfg(not(feature = "small-segment-lazy-commit"))]
        {
            // R8-6 (task #219): decommit starting at the real-page-safe
            // boundary, not the tight `payload_start` — on a 16/64 KiB-page
            // machine the tight value lands mid-real-page and the OS silently
            // rounds it, reclaiming (or leaving committed) the wrong byte
            // range.
            os::decommit_pages(base, SegLayout::small_decommit_start(), SEGMENT);
        }
        // 2a. Reset bump to the payload start. Before reuse, `dealloc_small`
        // rejects prior payload offsets with `off >= bump`, then checks current
        // free state. `reclaim_sidecar_record` instead validates payload start,
        // class alignment and checked block end against bump before payload
        // dereference; invalid geometry aborts. Its current free/residency guards
        // reject already-free or magazine-resident records. These checks do not
        // guarantee detection of all late/double/stale frees after reissue;
        // such misuse remains outside the unsafe caller contract.
        meta.set_bump(payload_start);
        // 2b. Empty every class free list.
        let mut bt = meta.bin_table();
        for c in 0..crate::alloc_core::size_classes::SMALL_CLASS_COUNT {
            bt.set_head(c, FREE_LIST_NULL);
        }
        // 2c. Re-mark every payload page `Free` in the page map (metadata pages
        //     keep their `Meta` marking). Payload pages are `[meta_pages,
        //     PAGES_PER_SEGMENT)`.
        //
        // R12-11 (task #262): `PageMap` maintenance is diagnostic-only (see
        // its struct doc) — gated behind `page-map-diag` (additionally to
        // this whole module's `alloc-decommit` gate) and elided from the
        // default/production decommit-reset path.
        #[cfg(feature = "page-map-diag")]
        {
            let mut pm = meta.page_map();
            let meta_pages = SegLayout::small_meta_pages();
            for p in meta_pages..crate::alloc_core::segment_header::PAGES_PER_SEGMENT {
                pm.set_free(p);
            }
        }
        // 2d. Zero the alloc bitmap (every slot "allocated / not-a-block" — the
        //     init state; with no live blocks and an empty free list this is the
        //     correct clean state). Re-init in place over the bitmap bytes.
        crate::alloc_core::alloc_bitmap::AllocBitmap::init_in_place(Node::offset(
            base,
            SegLayout::alloc_bitmap_off(),
        ));
        // RAD-5 (E4) GO/NO-GO EXPERIMENT: the second (magazine-residency)
        // bitmap must also be reset on a full decommit — a stale "resident"
        // bit surviving decommit would misreport magazine membership for a
        // future carve at the same offset. This full-reset path is NOT the
        // virgin-skip elision (the segment is being reused, not freshly
        // reserved), so this call stays UNCONDITIONAL, mirroring the
        // `AllocBitmap` re-init immediately above.
        crate::alloc_core::magazine_bitmap::MagazineBitmap::init_in_place(Node::offset(
            base,
            SegLayout::magazine_bitmap_off(),
        ));
        // 3. Flag the segment decommitted so the next `carve_block` recommits.
        meta.set_decommitted(true);
        // R12-10 (task #261, `virgin-zero-skip`): defensively clear the
        // payload-virgin bit. This is the ONLY path that can decommit a
        // small segment's payload while leaving it registered for a future
        // recommit-on-reuse carve — the exact macOS `MADV_DONTNEED`-is-
        // advisory-and-lazy hazard the design docs
        // (`docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md` §4.3,
        // `docs/perf/R11_8_SMALL_VIRGIN_ZERO_SKIP_DESIGN.md` §4.4(b)) flag as
        // the load-bearing risk area. Today this branch has ZERO production
        // callers (`decommit_empty_segment_impl`'s only call site,
        // `decommit_empty_segment_for_release`, hard-codes
        // `release_follows = true`) — verified by grep this session, exactly
        // as both design docs verified independently. The clear is kept
        // here regardless, unconditionally (not gated further), so that IF a
        // future decommit policy ever re-enables this leg, the virgin skip
        // fails SAFE (degrades to "always zero the next carve on this
        // segment") rather than silently becoming unsound: a subsequent
        // recommit is not OS-zero-guaranteed on every backend (macOS/XNU/*BSD
        // `MADV_DONTNEED` is advisory + lazy, no zero-fill guarantee).
        #[cfg(feature = "virgin-zero-skip")]
        meta.set_payload_virgin(false);
    }
}
