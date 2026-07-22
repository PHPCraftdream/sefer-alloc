//! R11-4 — batched deallocation for [`HeapCore`] (`batch-api` feature).
//!
//! This file holds the `impl HeapCore { .. }` block for
//! [`HeapCore::dealloc_batch`], the counterpart of
//! [`HeapCore::alloc_batch`](super::heap_core_alloc) on the free side. Pure
//! new surface (no existing method's behavior changes) — see the module doc
//! comment on [`dealloc_batch`](HeapCore::dealloc_batch) for the full design
//! and the magazine-vs-`flush_class` trade-off it makes explicit.

#[cfg(feature = "batch-api")]
use core::alloc::Layout;

#[cfg(all(feature = "batch-api", feature = "alloc-global", feature = "fastbin"))]
use crate::alloc_core::os;
#[cfg(all(
    feature = "batch-api",
    feature = "hardened",
    feature = "alloc-global",
    feature = "fastbin"
))]
use crate::alloc_core::segment_header::SegmentHeader;
#[cfg(all(
    feature = "batch-api",
    feature = "hardened",
    feature = "alloc-global",
    feature = "fastbin"
))]
use crate::alloc_core::segment_header::SegmentKind;
#[cfg(all(feature = "batch-api", feature = "alloc-global", feature = "fastbin"))]
use crate::alloc_core::segment_header::SegmentMeta;
#[cfg(all(feature = "batch-api", feature = "alloc-global", feature = "fastbin"))]
use crate::alloc_core::size_classes::{SizeClasses, MIN_BLOCK};

use super::heap_core::HeapCore;
#[cfg(all(feature = "batch-api", feature = "alloc-global", feature = "fastbin"))]
use super::tcache::TCACHE_CAP;

impl HeapCore {
    /// R11-4 — **batched deallocation**.
    ///
    /// # ⚠ EXPERIMENTAL / UNSTABLE
    ///
    /// This API has NO semver guarantees. It may change signature, behavior,
    /// or be removed entirely in any release without a major version bump,
    /// for as long as the `batch-api` feature (which requires
    /// `experimental`) remains unstable. Use at your own risk in production
    /// code.
    ///
    /// `#[doc(hidden)]` experimental surface gated behind the `batch-api`
    /// Cargo feature (see [`HeapCore::alloc_batch`]'s doc for the
    /// API-boundary rationale this mirrors). Frees every non-null block in
    /// `blocks`, all classified by
    /// the SAME `layout` (the same one-`layout`-per-call contract
    /// `alloc_batch` establishes — see `SeferAlloc::dealloc_batch`'s doc).
    ///
    /// ## Mechanism — "fast-path the safe subset, scalar-fallback the rest"
    ///
    /// 1. Classify `layout` ONCE. Large-classified (`class_for` → `None`) or
    ///    non-`fastbin` builds: no magazine, no `flush_class` — loop the
    ///    existing scalar [`dealloc`](Self::dealloc) exactly as before this
    ///    task (Large frees are already segment-granularity; there is no
    ///    batching win available at the bitmap level for them).
    /// 2. Small-classified under `fastbin`: partition `blocks` into
    ///    "this-heap-owned" (passes [`AllocCore::contains_base`], the SAME
    ///    O(1) ownership test [`dealloc_routing`](Self::dealloc_routing)
    ///    uses) vs. everything else (foreign, cross-thread-owned, null).
    ///    Owned blocks go through the batched fast path below; everything
    ///    else falls back to the scalar [`dealloc`](Self::dealloc) per block
    ///    — the exact same correct routing (foreign no-op / cross-thread
    ///    ring push) it uses today. This means a batch mixing owned and
    ///    foreign/cross-thread pointers is still handled correctly: only the
    ///    confidently-safe subset takes the fast path.
    ///
    /// ## The batched fast path: magazine-first, `flush_class`-overflow
    ///
    /// For the owned Small subset, this reuses the SAME FIVE guards, in the
    /// SAME order,
    /// [`dealloc_own_thread_with_base`](Self::dealloc_own_thread_with_base)
    /// applies per block — (1) [`hardened`] F7 Large-segment-kind guard
    /// (`SegmentHeader::kind_at(base) == SegmentKind::Large`), (2)
    /// [`hardened`] H1 interior-pointer guard (`off % block_size(c) != 0`),
    /// (3) in-magazine-residency bitmap, (4) [under `alloc-decommit`]
    /// stale-free `off >= bump`, (5) flushed alloc-bitmap `is_free` —
    /// calling the identical `pub(crate)` accessors (`SegmentHeader::kind_at`,
    /// `SizeClasses::block_size`, `magazine_bitmap().is_in_magazine`,
    /// `meta.bump_of()`, `alloc_bitmap().is_free`) in the identical order,
    /// NOT a redesigned oracle. A block that fails any guard is a benign
    /// no-op (double-free / interior-pointer / Large-in-small-layout /
    /// stale-free all degrade safely), matching the scalar contract exactly.
    /// F7 and H1 matter here specifically because this method's ownership
    /// gate is [`AllocCore::contains_base`], which does NOT distinguish
    /// Small vs. Large segments (both are "this heap's registered
    /// segments") — without F7/H1 a caller-contract-violating Large-via-
    /// small-layout free or an interior-pointer free would fall through to
    /// the M2 oracles and read/write the Large block's own payload bytes as
    /// if they were a Small segment's bitmap, exactly the corruption F7's
    /// own doc comment (`heap_core_free.rs`) warns against.
    ///
    /// Accepted blocks are pushed into the magazine array DIRECTLY (batched
    /// slot writes instead of the scalar path's one-push-then-maybe-flush
    /// per block) up to `TCACHE_CAP`; any further accepted blocks — the
    /// batch's overflow past magazine capacity — are routed straight to
    /// [`AllocCore::flush_class`] in ONE call (which internally groups them
    /// into same-segment runs and does the batched bitmap/BinTable RMW — see
    /// that method's doc comment), instead of the scalar path's dribble of
    /// `FLUSH_N`(8)-block half-flushes interleaved with individual pushes.
    /// This is the genuine batching win over N scalar `dealloc` calls: for a
    /// large same-class batch, one (or a few, if the magazine independently
    /// fills mid-batch — it cannot, since this path never triggers a
    /// magazine-overflow flush of its own) `flush_class` call replaces what
    /// would otherwise be `ceil((N - remaining_capacity) / FLUSH_N)`
    /// separate half-flush calls, each re-paying the per-run `SegmentMeta`/
    /// `bin_table`/`bump_of` setup `flush_class`/`flush_run` already hoist
    /// per run — now hoisted across the WHOLE overflow batch's runs in one
    /// pass instead of re-derived at every 8-block boundary.
    ///
    /// ## Trade-off — freed blocks are NOT all left magazine-warm (stated
    /// explicitly, per task R11-4's requirement)
    ///
    /// The scalar path always keeps freed blocks in the magazine until it
    /// hits `TCACHE_CAP`, so a same-thread same-class re-`alloc` right after
    /// a free is very likely a magazine hit. This batched path preserves
    /// that property ONLY up to `TCACHE_CAP` blocks per call — anything
    /// beyond that is routed straight to `flush_class`, bypassing the
    /// magazine entirely, so those blocks are NOT warm for a subsequent
    /// scalar `alloc` (it would pay a substrate fill instead of a magazine
    /// pop). This is judged acceptable for `dealloc_batch`'s use case: a
    /// caller freeing many blocks at once (the whole reason to call a batch
    /// API) is unlikely to immediately re-allocate the exact same class
    /// right after a bulk free — and the magazine-first ordering means the
    /// LAST `TCACHE_CAP` (or fewer) blocks of the batch (in `blocks` order)
    /// still land warm, so a small batch (`N <= TCACHE_CAP`) is byte-for-byte
    /// as warm as the scalar loop, and only a large batch's *excess* over
    /// magazine capacity gives up warmth in exchange for fewer, larger
    /// `flush_class` calls.
    ///
    /// ## Safety
    ///
    /// Same contract as [`GlobalAlloc::dealloc`](core::alloc::GlobalAlloc::dealloc)
    /// for every non-null `blocks[i]`, with `layout` matching every entry's
    /// allocation (the shared-`layout`-per-call contract `alloc_batch`
    /// already establishes): each non-null entry is the exact start pointer
    /// of a currently-LIVE allocation made by this allocator, freed **at most
    /// once** across the whole `blocks` slice (no duplicate entries). Null
    /// entries are always safe (skipped, matching the scalar contract).
    #[cfg(feature = "batch-api")]
    #[doc(hidden)]
    #[allow(unsafe_code)] // R6-MS-3: `unsafe fn` boundary (caller-pointer contract).
    pub unsafe fn dealloc_batch(&mut self, layout: Layout, blocks: &[*mut u8]) {
        #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
        {
            let size = layout.size().max(MIN_BLOCK);
            let align = layout.align();
            if let Some(c) = SizeClasses::class_for(size, align) {
                // SAFETY: forwarded to the batched Small fast path below,
                // which upholds the same per-block contract as `dealloc`.
                #[allow(unsafe_code)] // R6-MS-3: unsafe call into the batched fast path.
                unsafe {
                    self.dealloc_batch_small(c, layout, blocks)
                };
                return;
            }
        }
        // Large-classified, or non-`fastbin`/non-`alloc-global` build: no
        // magazine, no `flush_class` substrate exists for this class — loop
        // the existing scalar path exactly as before this task. Large frees
        // are already segment-granularity; there is no bitmap-level batching
        // win available for them (see this method's doc comment).
        for &p in blocks {
            if !p.is_null() {
                // SAFETY: caller upholds the dealloc-batch contract for `p`.
                #[allow(unsafe_code)] // R6-MS-1/2: unsafe call into scalar `dealloc`.
                unsafe {
                    self.dealloc(p, layout)
                };
            }
        }
    }

    /// The Small-classified batched fast path (R11-4). Partitions `blocks`
    /// into this-heap-owned vs. everything else, magazine-first-fills the
    /// owned subset up to `TCACHE_CAP`, and routes any overflow past that
    /// capacity to ONE [`AllocCore::flush_class`] call. See
    /// [`dealloc_batch`](Self::dealloc_batch)'s doc comment for the full
    /// mechanism and trade-off.
    ///
    /// `layout` is the CALLER'S ORIGINAL layout (not a reconstruction from
    /// `block_size(c)`): it must be threaded through unchanged because the
    /// scalar `dealloc` fallback below (for the non-owned subset) derives
    /// its OWN class from `layout.size()`/`layout.align()` again — under
    /// `alloc-xthread`, a cross-thread free's ring entry is tagged with that
    /// re-derived class (`dealloc_foreign_routing`'s
    /// `SizeClasses::class_for(size, layout.align())`), and a synthetic
    /// `align = 1` reconstruction could pick a DIFFERENT class than the one
    /// `c` (`class_for(size, layout.align())` is alignment-sensitive: a
    /// class is only valid when `block_size % align == 0`), corrupting the
    /// remote owner's freelist class tag for that block.
    #[cfg(all(feature = "batch-api", feature = "alloc-global", feature = "fastbin"))]
    #[inline]
    #[allow(unsafe_code)] // R6-MS-3: `unsafe fn` boundary (caller-pointer contract).
    unsafe fn dealloc_batch_small(&mut self, c: usize, layout: Layout, blocks: &[*mut u8]) {
        // ── Overflow staging buffer. Bounded by `blocks.len()`, but we never
        //    allocate: a fixed on-stack chunk is flushed to `flush_class` in
        //    pieces if `blocks` is larger than this chunk. `STAGE_CAP` is
        //    chosen generously above `TCACHE_CAP` so the common realistic
        //    batch (`batch_tcache` bench shape: up to a few hundred blocks)
        //    fits in ONE `flush_class` call; a batch larger than the chunk
        //    simply flushes in `STAGE_CAP`-sized pieces (still far fewer,
        //    larger calls than the scalar path's `FLUSH_N`(8)-block dribble).
        // ─────────────────────────────────────────────────────────────────
        const STAGE_CAP: usize = 512;
        let mut stage: [*mut u8; STAGE_CAP] = [core::ptr::null_mut(); STAGE_CAP];
        let mut staged: usize = 0;

        for &p in blocks {
            if p.is_null() {
                continue;
            }
            let base = os::segment_base_of_ptr(p);
            // Ownership gate (task R11-4 requirement): the SAME O(1) test
            // `dealloc_routing` uses. A block that is not one of THIS heap's
            // registered segments (foreign, or cross-thread-owned under
            // `alloc-xthread`) is NOT safe for the batched fast path below
            // (which reads/writes `base`'s metadata with no further
            // membership check, mirroring `flush_class`'s own `# Safety`
            // contract) — fall back to the scalar, fully-correct `dealloc`
            // for that one block.
            if !self.core.contains_base(base) {
                // SAFETY: caller upholds the dealloc-batch contract for `p`;
                // `dealloc` performs its own ownership routing (foreign
                // no-op / cross-thread ring push) for this individual block.
                #[allow(unsafe_code)] // R6-MS-1/2: unsafe call into scalar `dealloc`.
                unsafe {
                    self.dealloc(p, layout)
                };
                continue;
            }

            let off = (p as usize - base as usize) as u32;

            // ── F7 (task #25): Large-segment kind guard (HARDENED) ──
            // Identical guard, identical order, to
            // `dealloc_own_thread_with_base` (`heap_core_free.rs`): this
            // method's ownership gate above (`contains_base`) does NOT
            // distinguish Small vs. Large — both are "this heap's
            // registered segments" — so a caller-contract-violating free
            // (small-classified `layout`, but `ptr` actually lives in a
            // LARGE segment) would otherwise fall through to the M2 oracles
            // below and read/write the Large block's own payload bytes as
            // if they were a Small segment's bitmap. Reject as a no-op
            // BEFORE the oracles, exactly as the scalar path does.
            #[cfg(feature = "hardened")]
            {
                if SegmentHeader::kind_at(base) == SegmentKind::Large {
                    continue; // Large-segment free via small layout — no-op
                }
            }

            // ── H1 (task #167): interior-pointer guard (HARDENED) ──
            // Identical guard, identical order, to
            // `dealloc_own_thread_with_base`: a block start of class `c`
            // always sits at a segment offset that is a whole multiple of
            // `block_size(c)`. An INTERIOR pointer is blind to the M2
            // oracles below (bitmap granularity can alias a different bit),
            // so reject it as a no-op here too.
            #[cfg(feature = "hardened")]
            {
                let off_h = (p as usize).wrapping_sub(base as usize);
                let bs = SizeClasses::block_size(c);
                if !off_h.is_multiple_of(bs) {
                    continue; // interior-pointer free — no-op
                }
            }

            let meta = SegmentMeta::new(base);

            // M2 oracle 1 (identical accessor + order to
            // `dealloc_own_thread_with_base`): in-magazine double-free.
            if meta.magazine_bitmap().is_in_magazine(off) {
                continue; // in-magazine double-free — no-op
            }
            // M2 oracle 2: decommit stale-free guard (same accessor,
            // same gate).
            #[cfg(feature = "alloc-decommit")]
            if (off as usize) >= meta.bump_of() {
                continue;
            }
            // M2 oracle 3: flushed-then-double-freed guard (same accessor).
            if meta.alloc_bitmap().is_free(off) {
                continue; // flushed-then-double-freed — no-op
            }

            // Accepted. Magazine-first: fill up to `TCACHE_CAP` directly
            // (batched slot writes — no per-block flush check).
            let cnt = self.tcache.classes[c].count as usize;
            if cnt < TCACHE_CAP {
                meta.magazine_bitmap().mark_magazine(off);
                self.tcache.classes[c].slots[cnt] = p;
                self.tcache.classes[c].count = (cnt + 1) as u8;
                // R13-3 (task #273): a batched-freed block is, like the
                // scalar push in `dealloc_own_thread_with_base`, never
                // virgin (dispatch conjunct) — defensive clear of slot
                // `cnt`'s bit, matching that function's identical comment
                // (the mask invariant already guarantees it reads 0 here).
                #[cfg(feature = "virgin-zero-skip")]
                {
                    self.tcache.classes[c].virgin_mask &= !(1u16 << cnt);
                }
                continue;
            }

            // Magazine is full: stage for the batched `flush_class`
            // overflow instead of the scalar path's dribbled half-flush.
            if staged == STAGE_CAP {
                // Stage buffer full: flush what we have in one call, then
                // keep staging. `flush_class` groups same-segment runs
                // internally, so flushing in `STAGE_CAP`-sized pieces still
                // amortises far better than the scalar path's FLUSH_N(8)
                // dribble.
                // SAFETY (R6-MS-3): every entry of `stage[..staged]` is a
                // live small-class-`c` allocation owned by this heap (each
                // passed the ownership gate + all three M2 oracles above),
                // each freed exactly once across this whole call.
                #[allow(unsafe_code)] // R6-MS-3: unsafe call into `AllocCore::flush_class`.
                unsafe {
                    self.core.flush_class(c, &stage[..staged])
                };
                staged = 0;
            }
            stage[staged] = p;
            staged += 1;
        }

        if staged > 0 {
            // SAFETY (R6-MS-3): same justification as the mid-loop flush
            // above — every entry of `stage[..staged]` is a live
            // small-class-`c` allocation owned by this heap, freed exactly
            // once here.
            #[allow(unsafe_code)] // R6-MS-3: unsafe call into `AllocCore::flush_class`.
            unsafe {
                self.core.flush_class(c, &stage[..staged])
            };
        }
    }
}
