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
    /// returning the count written. Zero means either `out` was empty or
    /// no slot of a non-empty request could be allocated.
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
            // Same miss-only placement as `refill_magazine_slow`: bounded probe
            // first, one full rescue sweep only if the refill comes back empty.
            self.drain_large_sidecar_ingress_hot_bounded();

            let n = self.refill_with_large_rescue(|heap| {
                heap.core.refill_class_bump_internal(c, &mut out[filled..])
            });
            // Stamp each distinct refilled source segment.
            let mut prev_base = usize::MAX;
            for &p in &out[filled..(filled + n)] {
                if !p.is_null() {
                    let base = os::segment_base_of_ptr(p) as usize;
                    if base != prev_base {
                        self.stamp_segment_owner(p);
                        prev_base = base;
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
            self.clear_magazine_on_issue(p);
        }

        filled
    }

    /// R10-7 (Part 2) — non-`fastbin` fallback: no magazine to drain, so
    /// batch-allocation loops the substrate `alloc`. Correctness is identical to
    /// N scalar `alloc` calls; the only amortisation is the single
    /// classification hoist (the TLS lookup is amortised at the `SeferAlloc`
    /// wrapper, not here).
    ///
    /// Large batches delegate to `alloc_batch_large` for terminal sidecar
    /// housekeeping. Small batches use `AllocCore::alloc_with_class`, whose
    /// free-list discovery consumes routed candidate sidecars on misses.
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

        // Large batches share `alloc_batch_large`: it uses
        // `drain_large_sidecar_ingress` without fastbin, or a bounded probe
        // followed by a no-progress rescue sweep with fastbin.
        if class.is_none() {
            return self.alloc_batch_large(out, layout);
        }

        let mut filled = 0usize;
        for slot in out.iter_mut() {
            // Ph3b: the class was classified ONCE at the top of this method
            // (and the Large case already left through `alloc_batch_large`),
            // so hand it down instead of letting `AllocCore::alloc`
            // re-derive it from the same `layout`. `alloc_with_class` is the
            // twin the scalar non-`fastbin` `alloc` uses for exactly this
            // purpose (`alloc/hot.rs`).
            let p = self.core.alloc_with_class(layout, class);
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
        // Bounded probe up front; one full rescue sweep only if the batch
        // cannot make progress (same shape as `refill_with_large_rescue`).
        #[cfg(feature = "fastbin")]
        self.drain_large_sidecar_ingress_hot_bounded();
        #[cfg(all(feature = "alloc-xthread", not(feature = "fastbin")))]
        self.drain_large_sidecar_ingress();

        let mut filled = 0usize;
        for slot in out.iter_mut() {
            #[cfg(feature = "fastbin")]
            let p = if filled == 0 {
                let mut first = core::ptr::null_mut();
                let _ = self.refill_with_large_rescue(|heap| {
                    first = heap.core.alloc(layout);
                    usize::from(!first.is_null())
                });
                first
            } else {
                self.core.alloc(layout)
            };
            #[cfg(not(feature = "fastbin"))]
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
