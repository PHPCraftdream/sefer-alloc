//! In-place realloc fast-path family for [`AllocCore`] (split out of `mem.rs`).
//!
//! Holds `safe_payload_read_span`, `realloc_inplace_fast_path_known_base`,
//! `try_grow_large_reserved_capacity`, and `try_realloc_inplace_known_base`.
//! Pure code movement; no behavior changed.

use core::alloc::Layout;

#[cfg(all(feature = "alloc-stats", feature = "virgin-zero-skip"))]
use super::super::counters::SMALL_ZERO_PASS_CALLS;
#[cfg(feature = "alloc-stats")]
use super::super::counters::{
    FOREIGN_OR_UNROUTABLE_FREES, LARGE_ZERO_PASS_CALLS, OPT_H_ATTEMPTS, OPT_H_HITS,
    RELOC_FASTPATH_DECLINE_CALLS, RELOC_INPLACE_LARGE_CALLS, RELOC_INPLACE_SMALL_CALLS,
};
use crate::alloc_core::alloc_core::AllocCore;
use crate::alloc_core::os;
#[cfg(feature = "large-reserved-capacity")]
use crate::alloc_core::segment_header::align_up;
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind};

impl AllocCore {
    /// R2-1 (soundness): the maximum number of bytes starting at `payload`
    /// that lie within the COMMITTED span of the segment at `base`, computed
    /// purely from segment-header metadata — WITHOUT trusting any
    /// caller-supplied `Layout`.
    ///
    /// [`realloc`](Self::realloc) and `HeapCore::realloc` are SAFE `pub fn`s
    /// (no `unsafe` marker), so they must not let a bogus `old_layout.size()`
    /// drive an out-of-bounds read in the move leg's
    /// [`Node::copy_nonoverlapping`]. `contains_base(base)` proves the segment
    /// is OURS and MAPPED, but says nothing about how large the block at
    /// `payload` actually is; this method supplies that missing upper bound.
    ///
    /// For a Large segment the committed span is the header's `span_usable`
    /// (the physical OS reservation, `>=` the logical `large_size`, so all real
    /// data is preserved). For a Small/Primordial segment `span_usable` is
    /// unused (0) — the segment is exactly one `SEGMENT` (4 MiB), fully
    /// committed on reserve — so `SEGMENT` is the bound. In both cases the
    /// result is an upper bound on the bytes that can be read from `payload`
    /// without faulting or escaping the segment's OS allocation; the move legs
    /// reject (`old_layout.size() >` this value) before any copy rather than
    /// reading past the segment.
    ///
    /// # Preconditions
    ///
    /// `base` MUST already be proven to be a live, mapped segment — via
    /// `contains_base(base)` (own-segment legs) or `magic_at(base) ==
    /// SEGMENT_MAGIC` (the cross-heap foreign leg under `alloc-xthread`).
    /// This method reads `kind`/`span_usable` header fields at `base`, which
    /// is only sound for a mapped segment.
    #[inline]
    pub(crate) fn safe_payload_read_span(base: *mut u8, payload: *mut u8) -> usize {
        let seg_span = if SegmentHeader::kind_at(base) == SegmentKind::Large {
            SegmentHeader::span_usable_at(base)
        } else {
            // Small/Primordial: `span_usable` is 0 (inert — see
            // `SegmentHeader::small`); the segment is exactly one SEGMENT,
            // fully committed on reserve.
            os::SEGMENT
        };
        let off = (payload as usize).wrapping_sub(base as usize);
        seg_span.saturating_sub(off)
    }

    /// Single source of truth for the OPT-F / OPT-G in-place realloc fast
    /// paths. Returns `Some(ptr)` (the SAME pointer, unchanged or with its
    /// Large header's `large_size` updated in place) when an in-place resize
    /// is possible, `None` otherwise. Does NOT fall through to `self.alloc` —
    /// callers own that decision (the substrate-level [`realloc`](Self::realloc)
    /// calls `self.alloc` + copy + `self.dealloc`; the registry-level
    /// [`try_realloc_inplace_known_base`](Self::try_realloc_inplace_known_base) is consumed by `HeapCore::realloc`, which routes
    /// its alloc leg through the magazine-aware `HeapCore::alloc`).
    ///
    /// Both callers share these detection predicates so a bugfix applied to
    /// one cannot silently fail to reach the other (the X-arc retrospective
    /// C2 hazard).
    ///
    /// # OPT-G — Large→Large in-place grow
    ///
    /// Preconditions (all must hold to take the fast path):
    ///   1. The pointer lives in one of OUR segments (registered in the
    ///      table).
    ///   2. The segment kind is `Large` (dedicated single-allocation
    ///      segment). Huge is excluded conservatively — only Large segments
    ///      have a verified committed-span guarantee via `span_usable`.
    ///   3. GROW or SAME size only (clamped: `new_eff >= old_eff`). A
    ///      shrink falls through to the slow path, which reclaims RSS by
    ///      moving the payload to a smaller segment/class.
    ///   4. The grown payload fits the committed span:
    ///      `payload_offset + new_eff <= span_usable` (checked add to
    ///      guard against usize wrap on pathological sizes).
    ///
    /// MIN_BLOCK clamping: the alloc path clamps every request to
    /// `MIN_BLOCK` before storing `large_size` in the header. The #138
    /// cross-thread consistency check (`large_layout_consistent`) compares
    /// the header value against `layout_size.max(MIN_BLOCK)`. We must
    /// clamp identically here so a later cross-thread free does not see
    /// `raw != clamped` and silently drop the free — permanently leaking
    /// the segment + its SegmentTable slot (#114/#130 class).
    ///
    /// Soundness:
    ///   (a) `dealloc` routes Large frees by `SegmentHeader::kind_at(base)`,
    ///       NOT by the passed layout. A grown-in-place block stays a Large
    ///       segment, so `dealloc(ptr, new_layout)` frees the whole segment
    ///       correctly regardless of `new_size`.
    ///   (b) `crates/aligned-vmem` reserves large segments with
    ///       `VirtualAlloc(MEM_RESERVE|MEM_COMMIT)` over the WHOLE span;
    ///       the large-cache keeps pages committed on deposit. The entire
    ///       `span_usable` region is committed and writable — growing into
    ///       it cannot fault.
    ///   (c) Large reservations round UP to whole SEGMENT (4 MiB) multiples,
    ///       so e.g. a 512 KiB large alloc owns a full 4 MiB committed span
    ///       and can grow to ~4 MiB in place.
    ///
    /// When all hold: update the header's `large_size` to the CLAMPED
    /// `new_eff` and return the SAME pointer. The grown tail is
    /// uninitialised (matching `GlobalAlloc`).
    ///
    /// # OPT-F — Small→Small same-class in-place
    ///
    /// Preconditions (all must hold to take the fast path):
    ///   1. The pointer lives in one of OUR segments (registered in the table).
    ///   2. The segment kind is Small or Primordial (has a BinTable / class).
    ///   3. Both the old layout and the new size classify as Small (not Large).
    ///   4. new_class_idx == old_class_idx → the block stays in EXACTLY the
    ///      same size class.
    ///
    /// Why `==` and NOT `<=` (the subtle correctness point): a caller that
    /// reallocs `ptr` then later frees it MUST, per the `GlobalAlloc`
    /// contract, pass the NEW layout (`new_size`, same align) to `dealloc`.
    /// Our `dealloc` (post-#114) derives the block's size class from that
    /// layout alone — NOT from where the block physically sits. A block is
    /// carved at an offset that is a multiple of ITS class's `block_size`;
    /// that offset is NOT necessarily a multiple of a *smaller* class's
    /// `block_size` (the class sizes are not divisors of one another —
    /// e.g. the 132464-byte class is not a multiple of the 4096-byte
    /// class). So if we returned `ptr` unchanged for a shrink that crosses
    /// into a smaller class (`new_class < old_class`), the eventual
    /// `dealloc` would push this block's offset onto the SMALLER class's
    /// free list, where the offset is misaligned — corrupting that free
    /// list so a later `alloc` from it returns a mis-placed pointer. This
    /// was latent until task B1 added page-aligned classes (512..16384):
    /// before B1 the shrink target for a page-aligned request classified
    /// to `None` (Large) and never hit this path, so the bug never
    /// manifested. `==` keeps the block in its own class, where the
    /// carved offset is valid for the free list `dealloc` will use.
    ///
    /// When the class matches we return `ptr` unchanged. No copy (the
    /// block has not moved), no dealloc (we reuse it); the alloc-bitmap
    /// and live-count are unaffected (the block stays live).
    ///
    /// A cross-class shrink (`new_class < old_class`) falls through to the
    /// slow path (alloc new block in the smaller class + copy + dealloc
    /// old block in its own class) — correct, just not zero-copy. Growth
    /// (`new_class > old_class`) and Large on either side also fall
    /// through.
    /// In-place realloc fast paths for a pointer whose segment base has already
    /// been proven live in this `AllocCore`'s table. This is the same logic as
    /// [`realloc_inplace_fast_path`](Self::realloc_inplace_fast_path), split so
    /// `HeapCore::realloc` can reuse its own `contains_base(base)` proof instead
    /// of probing the segment table again.
    #[inline]
    pub(super) fn realloc_inplace_fast_path_known_base(
        &mut self,
        base: *mut u8,
        ptr: *mut u8,
        old_layout: Layout,
        new_size: usize,
    ) -> Option<*mut u8> {
        assert!(
            self.table.contains_base_ro(base),
            "known-base realloc called for a segment not owned by this core"
        );
        let kind = SegmentHeader::kind_at(base);
        // OPT-G: Large→Large in-place grow.
        if kind == SegmentKind::Large {
            let old_eff = old_layout
                .size()
                .max(crate::alloc_core::size_classes::MIN_BLOCK);
            let new_eff = new_size.max(crate::alloc_core::size_classes::MIN_BLOCK);
            if new_eff >= old_eff {
                let payload_off = ptr as usize - base as usize;
                let span_usable = SegmentHeader::span_usable_at(base);
                if let Some(end) = payload_off.checked_add(new_eff) {
                    if end <= span_usable {
                        SegmentHeader::set_large_size_at(base, new_eff);
                        #[cfg(feature = "alloc-stats")]
                        RELOC_INPLACE_LARGE_CALLS
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        return Some(ptr);
                    }
                    // R12-4 (feature `large-reserved-capacity`): the grow no
                    // longer fits the COMMITTED span (`span_usable`), but the
                    // segment may have extra RESERVED-but-uncommitted VA
                    // (`reserved_capacity`, always `>= span_usable`) it can
                    // grow into — committing just the missing tail instead
                    // of falling through to the slow alloc+copy+free path.
                    // See `SegmentHeader::reserved_capacity`'s doc and the
                    // `large-reserved-capacity` feature doc in `Cargo.toml`
                    // for the full R12-3/R12-4 motivation.
                    #[cfg(feature = "large-reserved-capacity")]
                    if self.try_grow_large_reserved_capacity(base, end) {
                        SegmentHeader::set_large_size_at(base, new_eff);
                        #[cfg(feature = "alloc-stats")]
                        RELOC_INPLACE_LARGE_CALLS
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        return Some(ptr);
                    }
                }
            }
            #[cfg(feature = "alloc-stats")]
            RELOC_FASTPATH_DECLINE_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return None;
        }
        // OPT-F: Small→Small same-class in-place.
        if matches!(kind, SegmentKind::Small | SegmentKind::Primordial) {
            let old_size = old_layout
                .size()
                .max(crate::alloc_core::size_classes::MIN_BLOCK);
            let align = old_layout.align();
            let clamped_new = new_size.max(crate::alloc_core::size_classes::MIN_BLOCK);
            if let (Some(old_class), Some(new_class)) = (
                crate::alloc_core::size_classes::SizeClasses::class_for(old_size, align),
                crate::alloc_core::size_classes::SizeClasses::class_for(clamped_new, align),
            ) {
                if new_class == old_class {
                    #[cfg(feature = "alloc-stats")]
                    RELOC_INPLACE_SMALL_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    return Some(ptr);
                }
                // OPT-H STAGE-1 DIAGNOSTIC ONLY (R21-2, task #351,
                // `docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` §6.1/§8 step
                // 1) — OBSERVATION, NOT IMPLEMENTATION. OPT-F declined (the
                // class changed), so this is exactly the case a FUTURE OPT-H
                // mechanism would target: a cross-class Small/Primordial
                // grow. We only COUNT how often OPT-H's six preconditions
                // (design §2.1) would hold here; we do NOT carve, do NOT
                // move `bump`, do NOT return `Some` — the function still
                // falls through to the unchanged `None` below in every case,
                // exactly as before this diagnostic existed.
                //
                // The entire precondition-evaluation block below is gated on
                // `alloc-stats` (not just the two `fetch_add` calls) so that
                // a plain `production` build (which does NOT include
                // `alloc-stats`) pays zero cost here: no extra branch, no
                // extra load, no extra arithmetic on the hot path beyond what
                // OPT-F already computed. This is a stricter gate than the
                // `LARGE_ZERO_PASS_CALLS`/`SMALL_ZERO_PASS_CALLS` precedent
                // needs (those sit on a path that already does multi-KiB
                // work), because this new comparison chain sits directly on
                // the realloc-grow hot path callers execute on every
                // cross-class Small/medium grow, `alloc-stats` or not.
                #[cfg(feature = "alloc-stats")]
                if crate::alloc_core::size_classes::SizeClasses::block_size(new_class)
                    > crate::alloc_core::size_classes::SizeClasses::block_size(old_class)
                {
                    // Precondition 1 holds (growing, cross-class). This is
                    // the Stage-1 denominator. Precondition 2 (segment kind
                    // Small/Primordial) is already established by the
                    // enclosing `if matches!(kind, ...)` above.
                    OPT_H_ATTEMPTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

                    let off = ptr as usize - base as usize;
                    let old_block_size =
                        crate::alloc_core::size_classes::SizeClasses::block_size(old_class);
                    let new_block_size =
                        crate::alloc_core::size_classes::SizeClasses::block_size(new_class);
                    let meta = crate::alloc_core::segment_header::SegmentMeta::new(base);

                    // Precondition 3 (tail-adjacency): this block must be
                    // the segment's current bump tail — the most-recently
                    // carved, not-yet-grown-or-freed block. Reusing the
                    // existing owner-only `bump_of` accessor (same
                    // single-field read `carve_block` itself uses), not
                    // hand-rolling a new bump read.
                    let tail_adjacent = off + old_block_size == meta.bump_of();
                    // Precondition 4 (new-class alignment): the offset must
                    // be a legal carve position for `new_class` — i.e.
                    // indistinguishable from an ordinarily-carved
                    // `new_class` block to every subsystem that later reads
                    // this offset (BinTable free-list reuse on dealloc).
                    let new_class_aligned = off.is_multiple_of(new_block_size);
                    // Precondition 5 (segment capacity): the grown block
                    // must still fit within the segment (same `SEGMENT`
                    // constant OPT-G's Large-path checks and `carve_block`
                    // use — not a hardcoded literal).
                    let fits_segment = off + new_block_size <= crate::alloc_core::os::SEGMENT;
                    // Precondition 6 (lazy-commit frontier). Under
                    // `primordial-lazy-commit`/`small-segment-lazy-commit`,
                    // `carve_block` never advances `bump` past
                    // `committed_payload_end` without first committing the
                    // tail up to at least the new `bump` value (see
                    // `carve_block`'s B2 grow-on-carve block,
                    // `alloc_core_small.rs:1508-1533`: it commits BEFORE
                    // `set_bump`, and only ever sets
                    // `committed_payload_end` to a value `>=` the new
                    // `bump`). So for a block that satisfies precondition 3
                    // (`off + old_block_size == bump`), the frontier is
                    // already `>= bump == off + old_block_size` at every
                    // instant — i.e. the frontier is always at least as far
                    // as the CURRENT tail. This does NOT by itself prove the
                    // frontier already covers `off + new_block_size` (the
                    // GROWN tail may extend past the current frontier if
                    // `new_block_size > old_block_size` pushes past a
                    // `GROW_CHUNK` boundary the frontier hasn't reached yet)
                    // — a real OPT-H implementation would still need to run
                    // `carve_block`'s own commit-frontier step for the
                    // stretch `[bump, off + new_block_size)`. For THIS
                    // observation-only Stage-1 counter, when the lazy-commit
                    // features are OFF there is nothing to commit (trivially
                    // satisfied); when they are ON we do NOT independently
                    // verify the frontier already covers the grown tail —
                    // this is a known, documented overcount for lazy-commit
                    // builds specifically (Stage-1 hit rate on such a build
                    // may be a slight overcount versus a real
                    // implementation's actual hit rate), not a new checked
                    // code path, per this task's explicit scope boundary
                    // (inventing frontier-verification logic here would add
                    // an untested new code path for a precondition that, in
                    // this observation-only task, has zero behavioral
                    // consequence either way). See
                    // `docs/perf/R21_2_OPT_H_STAGE1_HIT_RATE.md` §3's
                    // "overcounting-configuration caveat" for why the
                    // Stage-1 measurement's 0-hit result is trustworthy
                    // despite this overcount, and why a future non-zero
                    // reading under a lazy-commit build must be read as an
                    // upper bound, not an exact count.
                    let lazy_commit_frontier_ok = true;

                    if tail_adjacent && new_class_aligned && fits_segment && lazy_commit_frontier_ok
                    {
                        OPT_H_HITS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
        }
        #[cfg(feature = "alloc-stats")]
        RELOC_FASTPATH_DECLINE_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        None
    }

    /// R12-4 (feature `large-reserved-capacity`): try to grow a Large
    /// segment's COMMITTED span (`span_usable`) up to at least `required_end`
    /// bytes (segment-relative), by committing the missing tail
    /// `[span_usable, page_round(required_end))` — WITHOUT moving the
    /// allocation. Called only from the OPT-G grow path, only after the
    /// existing committed-span check (`required_end <= span_usable`) has
    /// already failed.
    ///
    /// Returns `true` (and leaves `span_usable` advanced to cover
    /// `required_end`) iff:
    ///   1. `required_end <= reserved_capacity` — the grow fits within the
    ///      segment's RESERVED VA span (checked with `checked_add`-free
    ///      arithmetic since `required_end` was already computed via a
    ///      `checked_add` at the call site); and
    ///   2. the OS commit of the missing tail succeeds (`os::commit_pages`
    ///      — can fail on genuine commit-charge exhaustion, in which case
    ///      this returns `false` and the caller falls through to the slow
    ///      alloc+copy+free path, exactly as if `reserved_capacity` had not
    ///      existed).
    ///
    /// Returns `false` (no OS call, header unchanged) if `required_end`
    /// exceeds `reserved_capacity` — the segment has no more VA to grow into
    /// and the caller must fall through to the slow path.
    ///
    /// # Why committing a REAL-PAGE-SAFE round of `required_end`, not
    /// `required_end` itself
    ///
    /// `commit_pages` (like every commit/decommit primitive in this crate)
    /// requires offsets aligned to the RUNTIME OS page size — `required_end`
    /// is an arbitrary byte count (a payload size), not necessarily
    /// page-aligned at all. Rounding UP to the next
    /// [`os::MAX_REALISTIC_PAGE_SIZE`] (64 KiB) boundary (capped at
    /// `reserved_capacity`, which is itself always a runtime-page multiple —
    /// see [`os::Segment::reserve_capacity_exact`]'s contract) commits a
    /// whole number of pages on EVERY host page size while still covering
    /// `required_end`; the extra bytes up to the boundary are committed but
    /// not yet claimed by any allocation — exactly the same "commit whole
    /// pages, track the logical frontier separately" pattern
    /// `alloc-lazy-commit`'s `committed_payload_end` uses for small segments.
    ///
    /// Task #1077 (the fifth escape of the compile-time-page-constant bug
    /// class): this used to round to the compile-time `os::PAGE` (4 KiB),
    /// but `os::commit_pages` -> `aligned_vmem::try_commit_range` validates
    /// its endpoint against the RUNTIME `aligned_vmem::page_size()` — on a
    /// 16 KiB (Apple Silicon) or 64 KiB (aarch64-linux-64k) host a
    /// 4-KiB-only multiple is rejected as `invalid_argument`, the commit
    /// returns `false`, and the grow silently degrades to the slow
    /// alloc+copy+free path: no memory-safety consequence, but the
    /// `large-reserved-capacity` feature is a permanent no-op there and
    /// `tests/large_reserved_capacity.rs`'s same-pointer assertions are a
    /// latent red. 64 KiB is a superset multiple of every page size this
    /// crate supports (see `os::MAX_REALISTIC_PAGE_SIZE`'s invariant), so
    /// one rounding serves all hosts; the ≤ ~60 KiB of extra committed
    /// tail per grow is negligible against the ≥ 4x reserved headroom
    /// (`LARGE_RESERVED_CAP_GROWTH_FACTOR`).
    #[cfg(feature = "large-reserved-capacity")]
    #[inline]
    fn try_grow_large_reserved_capacity(&mut self, base: *mut u8, required_end: usize) -> bool {
        let reserved_capacity = SegmentHeader::reserved_capacity_at(base);
        if required_end > reserved_capacity {
            return false;
        }
        let span_usable = SegmentHeader::span_usable_at(base);
        // `required_end > span_usable` is guaranteed by the call site (this
        // is only reached after the committed-span check already failed),
        // so the commit range below is always non-empty.
        // task #1077: MAX_REALISTIC_PAGE_SIZE (64 KiB), NOT the compile-time
        // `PAGE` (4 KiB) — see the doc block above; the runtime-page-size
        // validator in `try_commit_range` rejects 4-KiB-only multiples on
        // 16/64 KiB-page hosts. The `.min(reserved_capacity)` clamp stays
        // validator-safe: `reserved_capacity` is itself a runtime-page
        // multiple (built from `usable`, which task #1074 already rounds to
        // `aligned_vmem::page_size()` in `alloc_large_slow`).
        let new_span_usable =
            align_up(required_end, crate::alloc_core::os::MAX_REALISTIC_PAGE_SIZE)
                .min(reserved_capacity);
        if !os::commit_pages(base, span_usable, new_span_usable) {
            // Commit-charge exhaustion / genuine OOM on the incremental
            // commit: leave the header untouched (span_usable unchanged —
            // still describes exactly what is actually committed) and let
            // the caller fall through to the slow path.
            return false;
        }
        SegmentHeader::set_span_usable_at(base, new_span_usable);
        true
    }

    /// Try the two in-place realloc fast paths (Large grow-in-span, Small same-class), but the
    /// caller has already proven `base` is live in this core's segment table.
    /// Used by `HeapCore::realloc` to avoid a duplicate `contains_base` probe
    /// after its own ownership check.
    #[cfg(feature = "alloc-global")]
    pub(crate) fn try_realloc_inplace_known_base(
        &mut self,
        base: *mut u8,
        ptr: *mut u8,
        old_layout: Layout,
        new_size: usize,
    ) -> Option<*mut u8> {
        if ptr.is_null() {
            return None;
        }
        self.realloc_inplace_fast_path_known_base(base, ptr, old_layout, new_size)
    }
}
