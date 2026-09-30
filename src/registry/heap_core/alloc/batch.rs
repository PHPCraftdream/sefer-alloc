//! Batch-allocation entry points for [`HeapCore`] (mechanical split of
//! `heap_core_alloc.rs`: the `alloc_batch` family — both `cfg` variants of
//! `alloc_batch` plus `alloc_batch_large`). Pure code-movement split of the
//! same `impl HeapCore` block; no behavior changed.

#[cfg(feature = "batch-api")]
use ::core::alloc::Layout;
#[cfg(all(feature = "alloc-stats", feature = "fastbin", feature = "batch-api"))]
use ::core::sync::atomic::Ordering;

#[cfg(all(feature = "alloc-global", feature = "fastbin", feature = "batch-api"))]
use crate::alloc_core::os;

use crate::registry::heap_core::HeapCore;

impl HeapCore {
    /// R10-7 (Part 2) — **tcache-aware batch allocation**.
    ///
    /// # ⚠ EXPERIMENTAL / UNSTABLE
    ///
    /// This API has NO semver guarantees. It may change signature, behavior,
    /// or be removed entirely in any release without a major version bump,
    /// for as long as the `batch-api` feature (which requires
    /// `experimental`) remains unstable. Use at your own risk in production
    /// code.
    ///
    /// `#[doc(hidden)]` — NOT committed public API — gated behind the
    /// `batch-api` Cargo feature so it is invisible to a default `production`
    /// build and cannot land in the semver/ABI surface by accident; reachable
    /// via [`SeferAlloc::alloc_batch`](crate::global::SeferAlloc::alloc_batch),
    /// which carries the same experimental marker on its own (visible)
    /// rustdoc entry (R12-12). Fills `out` with up to `out.len()` live
    /// blocks of `layout` (same validity contract as a single `alloc`),
    /// returning the count written (0 only on true OOM).
    ///
    /// Design — "drain what's already warm, batch-refill only the miss":
    /// 1. classify ONCE (vs N times for N scalar `alloc` calls).
    /// 2. DRAIN the per-class magazine directly into `out` — the exact
    ///    magazine-hit fast path, looped (pop + [hardened] gen bump). Reuses
    ///    the blocks already warmed there instead of carving/refilling around
    ///    them.
    /// 3. The ordinary `refill_class_bump` fills the remainder directly into
    ///    `out`, using the same metadata-guarded discovery as scalar refill.
    ///
    /// Magazine-residency bits for the drained prefix remain SET through the
    /// remainder refill. The owner retirement primitive reads those physical
    /// bits for every class, so an in-flight output block cannot be linked back
    /// into the substrate. After refill, one bulk pass clears the prefix bits
    /// before the output is handed to the caller.
    ///
    /// Genuinely different from R8-7/R9-9's measured arm, which called the
    /// `AllocCore`-level batch primitive directly, BYPASSING the magazine
    /// entirely. This path drains the magazine first (the warm layer the scalar
    /// path uses) and only batch-refills the deficit — closer to what a real
    /// public batch API would do.
    ///
    /// Correctness vs N scalar `alloc` calls: each returned block undergoes the
    /// SAME state transition as a single `alloc` (live + bitmap-allocated,
    /// segment-owner-stamped, hardened-gen-bumped at issue). The magazine is
    /// left with `count[c]` reduced by however many were drained; the next
    /// scalar `alloc` on a now-empty magazine takes the normal
    /// `refill_magazine_slow` miss path. The pre-existing cross-thread
    /// double-free residual (the "THIRD leg" documented at
    /// `free/dealloc_own_base.rs`'s `dealloc_own_thread_with_base`) is UNCHANGED —
    /// this path reuses the exact same refill + drain primitives, introducing
    /// no new invariant.
    #[cfg(all(feature = "fastbin", feature = "batch-api"))]
    #[doc(hidden)]
    #[must_use]
    pub fn alloc_batch(&mut self, layout: Layout, out: &mut [*mut u8]) -> usize {
        use crate::alloc_core::size_classes::{SizeClasses, MIN_BLOCK};

        let want = out.len();
        if want == 0 {
            return 0;
        }
        let size = layout.size().max(MIN_BLOCK);
        let align = layout.align();
        let class = SizeClasses::class_for(size, align);

        let Some(c) = class else {
            // Large-classified (or align beyond the small range): no magazine —
            // loop the substrate. Batching does not help the dedicated-segment
            // Large path; correctness == N scalar `alloc` calls.
            return self.alloc_batch_large(out, layout);
        };

        let mut filled = 0usize;

        // ── (1) Drain the warm magazine into `out` (the magazine-hit fast
        //     path, looped): decrement count + [hardened] bump the issue
        //     generation. ────────────────────────────────────────────────────
        //
        // R10-7 follow-up (double-free-deviates-into-double-issue fix):
        // UNLIKE the scalar `alloc` magazine-hit arm, this loop does NOT
        // call `clear_magazine(off)` per pop. The residency bits for ALL
        // drained blocks are left SET through step 2 below and cleared in
        // ONE bulk pass after step 2 returns (see step 3 + the
        // "deferred-clear rationale" in this method's doc comment). A
        // stale cross-thread double-free ring entry for one of these
        // blocks is then rejected by step 2's predicate (which now
        // actually consults the bit for class `c`) — instead of being
        // amplified into a duplicate pointer.
        while filled < want {
            let cnt = self.tcache.classes[c].count as usize;
            if cnt == 0 {
                break;
            }
            let new_cnt = cnt - 1;
            self.tcache.classes[c].count = new_cnt as u8;
            // R13-3 (task #273): clear the popped slot's virgin bit — same
            // "bits >= count are 0" invariant the scalar `alloc` magazine-hit
            // arm maintains, and load-bearing here: this drained block is
            // handed straight to `alloc_batch`'s caller (a plain, uninitialised
            // `*mut u8` — `alloc_batch` has no `_zeroed` variant, so the bit's
            // VALUE is irrelevant to this call's own correctness), but a LATER
            // push back into this now-vacated physical slot index must not
            // observe a stale set bit from whatever virgin block last lived here.
            #[cfg(feature = "virgin-zero-skip")]
            {
                self.tcache.classes[c].virgin_mask &= !(1u16 << new_cnt);
            }
            #[cfg(feature = "alloc-stats")]
            if let Some(hits) = self.tcache_hits {
                hits.store(
                    hits.load(Ordering::Relaxed).wrapping_add(1),
                    Ordering::Relaxed,
                );
            }
            let issued = self.tcache.classes[c].slots[new_cnt];
            // X7 Ф3 (task #191) touch (a): bump the generation at ISSUE.
            #[cfg(feature = "hardened")]
            {
                let base = os::segment_base_of_ptr(issued);
                let off = (issued as usize) - (base as usize);
                // SAFETY: `base` is a live, exclusively-owned segment; `off`
                // is a MIN_BLOCK-aligned offset.
                #[allow(unsafe_code)]
                unsafe {
                    Self::bump_gen_on_issue(base, off);
                }
            }
            out[filled] = issued;
            filled += 1;
        }

        // Record how many were drained from the magazine: their residency
        // bits are still SET (the deferred-clear contract step 2 + step 3
        // below rely on).
        let magazine_drained = filled;

        // Refill the remainder directly into out. The owner-side retirement
        // primitive reads the physical magazine bitmap itself, including the
        // residency bits held through this batch's in-flight output phase.
        if filled < want {
            // Opportunistic drains (same placement as `refill_magazine_slow`:
            // magazine-miss-only). `fastbin` implies `alloc-xthread`, so both
            // exist here.
            self.drain_large_sidecar_ingress();

            let n = self.core.refill_class_bump(c, &mut out[filled..]);
            // P4 stamp-dedupe + hardened gen bump. EVERY refilled block is
            // issued to the caller here (none stay in the magazine), so all
            // get the issue touch — unlike `refill_magazine_slow`, which only
            // bumps the one popped block (the n-1 retained are bumped on
            // their later pops).
            let mut prev_base = usize::MAX;
            for &p in &out[filled..(filled + n)] {
                if !p.is_null() {
                    let base = os::segment_base_of_ptr(p) as usize;
                    if base != prev_base {
                        self.stamp_segment_owner(p);
                        prev_base = base;
                    }
                    #[cfg(feature = "hardened")]
                    {
                        let off = (p as usize) - base;
                        // SAFETY: `base` is a live, exclusively-owned segment;
                        // `off` is a MIN_BLOCK-aligned offset.
                        #[allow(unsafe_code)]
                        unsafe {
                            Self::bump_gen_on_issue(base as *mut u8, off);
                        }
                    }
                }
            }
            filled += n;
        }

        // Clear the drained prefix's physical magazine-residency bits only
        // after the metadata-guarded refill has finished, before caller issue.
        //
        // Coalescing note: per-block clear (one byte RMW per block via
        // `clear_magazine`). The drained blocks tend to cluster by segment
        // (consecutive pops from one magazine class, which was typically
        // filled by a same-segment refill), so a further word-merge
        // (accumulate masks per bitmap byte → one RMW per byte instead of
        // per block) is the natural follow-up — but it needs a new
        // `SegmentBitmap` primitive and is more API surface than this
        // bug fix warrants. The deferred-clear design itself does not
        // regress: this loop does exactly the same number of RMWs the
        // old per-pop clear did, just batched at the end.
        for &p in &out[..magazine_drained] {
            let _ = self.clear_magazine_on_issue(p);
        }

        filled
    }

    /// R10-7 (Part 2) — non-`fastbin` fallback: no magazine to drain, so
    /// batch-allocation loops the substrate `alloc`. Correctness is identical to
    /// N scalar `alloc` calls; the only amortisation is the single
    /// classification hoist (the TLS lookup is amortised at the `SeferAlloc`
    /// wrapper, not here).
    ///
    /// G3 added Large/Small routing: Large-classified batches delegate to
    /// `alloc_batch_large` (drain-equipped), Small batches carry the same
    /// unconditional `drain_heap_overflow` prelude the scalar non-`fastbin`
    /// path has, so batch correctness/retention behaviour now matches N
    /// scalar `alloc` calls on the reclamation axis too.
    ///
    /// # ⚠ EXPERIMENTAL / UNSTABLE
    ///
    /// Same `batch-api` (requires `experimental`) no-semver-guarantees status
    /// as the `fastbin` variant above — see that doc comment (R12-12).
    #[cfg(not(feature = "fastbin"))]
    #[cfg(feature = "batch-api")]
    #[doc(hidden)]
    #[must_use]
    pub fn alloc_batch(&mut self, layout: Layout, out: &mut [*mut u8]) -> usize {
        use crate::alloc_core::size_classes::{SizeClasses, MIN_BLOCK};

        // G3 (P2): ONE call carries ONE shared `layout` for the whole batch,
        // so classify ONCE at the top — the same single-classification shape
        // the `fastbin` `alloc_batch` above and the scalar `alloc` already
        // use — and route on it, instead of looping blind.
        let size = layout.size().max(MIN_BLOCK);
        let align = layout.align();
        let class = SizeClasses::class_for(size, align);

        // G3 (P2): Large-classified batches must take the SAME drain-equipped
        // Large loop the fastbin batch uses — delegate the whole call to
        // `alloc_batch_large` (which performs the `drain_large_deferred_free`
        // housekeeping) instead of duplicating a second generic loop here.
        if class.is_none() {
            return self.alloc_batch_large(out, layout);
        }

        let mut filled = 0usize;
        for slot in out.iter_mut() {
            let p = self.core.alloc(layout);
            if p.is_null() {
                break;
            }
            self.stamp_segment_owner(p);
            *slot = p;
            filled += 1;
        }
        filled
    }

    /// Large batch slow path: consume descriptor obligations once before
    /// issuing the batch and considering physical Large-cache reuse.
    #[cfg(feature = "batch-api")]
    fn alloc_batch_large(&mut self, out: &mut [*mut u8], layout: Layout) -> usize {
        // A batch-only owner must consume pending Large obligations too.
        #[cfg(feature = "alloc-xthread")]
        {
            self.drain_large_sidecar_ingress();
        }

        let mut filled = 0usize;
        for slot in out.iter_mut() {
            let p = self.core.alloc(layout);
            if p.is_null() {
                break;
            }
            self.stamp_segment_owner(p);
            *slot = p;
            filled += 1;
        }
        filled
    }
}
