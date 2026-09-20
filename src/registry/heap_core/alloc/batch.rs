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
#[cfg(all(feature = "alloc-global", feature = "fastbin", feature = "batch-api"))]
use crate::alloc_core::segment_header::SegmentMeta;

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
    /// 3. for the REMAINDER once the magazine is exhausted: the `AllocCore`
    ///    batch-refill primitive (`refill_class_bump_checked`) fills the rest
    ///    DIRECTLY into `out` in one freelist-drain / bump-carve pass (NOT via
    ///    the magazine), with the magazine-residency predicate +
    ///    segment-owner stamping `refill_magazine_slow` uses. No block is
    ///    parked in the magazine — they all go to the caller.
    ///
    /// ## R10-7 follow-up — deferred magazine-residency bit clear
    ///
    /// Step 1 does NOT call `clear_magazine` per pop (unlike the scalar
    /// `alloc` magazine-hit arm, which clears the bit immediately). The bits
    /// for all `magazine_drained` blocks are left SET through step 2 and
    /// cleared in ONE bulk pass AFTER step 2 returns. Two compounding reasons
    /// (both pinned by `tests/r10_7_alloc_batch_xthread_double_free.rs`):
    ///
    /// 1. **The bit-clear-too-early hazard.** If a caller-side cross-thread
    ///    double-free of one of these blocks left a stale ring entry, step 2's
    ///    internal `drain_dirty_segments` / `find_segment_with_free_checked`
    ///    would encounter it. Once the residency bit is cleared, the
    ///    `is_in_magazine` guard cannot distinguish "block was drained to
    ///    `out` (in-flight, not yet handed back)" from "block was handed out
    ///    long ago" — so `reclaim_offset_checked` links the stale entry onto
    ///    the freelist, and `drain_freelist_batch` immediately re-issues it
    ///    into `out[filled..]`: a duplicate of the pointer already sitting in
    ///    `out[0..magazine_drained]`.
    /// 2. **The `if k == c { return false; }` short-circuit is unsound here.**
    ///    `refill_magazine_slow`'s OWN predicate opens with this shortcut,
    ///    justified by its KEY INVARIANT (`count[c] == 0` at refill time, so
    ///    nothing of class `c` has been claimed). `alloc_batch` violates that
    ///    precondition — step 1 has already pulled `magazine_drained` class-`c`
    ///    blocks into `out`, so the shortcut unconditionally skips the
    ///    magazine-residency check for EXACTLY the class under refill. This
    ///    closure therefore drops the shortcut and consults `is_in_magazine`
    ///    for ALL classes including `c`, so the deferred SET bits from step 1
    ///    actually do their protective work.
    ///
    /// The two halves are inseparable: deferring the clear alone would
    /// accomplish nothing (the shortcut skips the check for class `c`), and
    /// dropping the shortcut alone would accomplish nothing (the bit is
    /// already cleared by step 1). Only together do they close the window.
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
                    crate::alloc_core::segment_header::bump_gen(base, off)
                };
            }
            out[filled] = issued;
            filled += 1;
        }

        // Record how many were drained from the magazine: their residency
        // bits are still SET (the deferred-clear contract step 2 + step 3
        // below rely on).
        let magazine_drained = filled;

        // ── (2) Refill the REMAINDER directly into `out[filled..]` via the
        //     AllocCore batch-refill primitive — one freelist-drain /
        //     bump-carve pass, NOT via the magazine. Same predicate +
        //     stamping as `refill_magazine_slow`. ──────────────────────────
        //
        // R10-7 follow-up: the predicate closure NO LONGER opens with
        // `if k == c { return false; }`. That short-circuit is sound ONLY
        // in `refill_magazine_slow`'s own context (KEY INVARIANT: at ITS
        // refill time, `count[c] == 0`, so nothing of class `c` has been
        // claimed by that call); `alloc_batch` violates the precondition
        // because step 1 above has already pulled `magazine_drained`
        // class-`c` blocks into `out[0..magazine_drained]`. With the bit
        // still SET (deferred clear), consulting `is_in_magazine` for
        // `k == c` is exactly what protects those in-flight blocks: a
        // stale cross-thread double-free ring entry for one of them now
        // reads `true` and is rejected by `reclaim_offset_checked`'s
        // existing guard chain — instead of being linked onto the freelist
        // (which `drain_freelist_batch` would then pull straight back
        // into `out[filled..]`, producing a duplicate of the pointer
        // already in `out[0..magazine_drained]`).
        //
        // `_k` is unused because the residency bitmap is keyed by segment
        // OFFSET, not by class — the bitmap probe is O(1) regardless of
        // which class the ring entry carries.
        if filled < want {
            // Opportunistic drains (same placement as `refill_magazine_slow`:
            // magazine-miss-only). `fastbin` implies `alloc-xthread`, so both
            // exist here.
            self.drain_large_deferred_free();
            self.drain_heap_overflow();
            let n = self
                .core
                .refill_class_bump_checked(c, &mut out[filled..], &|ptr, _k| {
                    let pbase = os::segment_base_of_ptr(ptr);
                    let poff = (ptr as usize - pbase as usize) as u32;
                    SegmentMeta::new(pbase)
                        .magazine_bitmap()
                        .is_in_magazine(poff)
                });
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
                            crate::alloc_core::segment_header::bump_gen(base as *mut u8, off)
                        };
                    }
                }
            }
            filled += n;
        }

        // ── (3) Bulk-clear the magazine-residency bits for the blocks step 1
        //     drained into `out[0..magazine_drained]`. Their bits were
        //     intentionally left SET through step 2 (see the deferred-clear
        //     rationale above) so the predicate could protect them against
        //     stale ring entries. By this point `refill_class_bump_checked`
        //     has returned — its internal ring-drain / freelist-drain /
        //     bump-carve will not touch these blocks again before
        //     `alloc_batch` returns — so the SET bits have served their
        //     purpose and must now be cleared to restore the invariant that
        //     a handed-out block reads "not magazine-resident" (the
        //     own-thread free path's `is_in_magazine` oracle relies on this
        //     — see `free/dealloc_own_base.rs`'s magazine-push double-free guard).
        //     ──────────────────────────────────────────────────────────────
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
            let base = os::segment_base_of_ptr(p);
            let off = (p as usize - base as usize) as u32;
            SegmentMeta::new(base).magazine_bitmap().clear_magazine(off);
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

        // G3 (P2): non-fastbin Small batches get the SAME unconditional
        // `drain_heap_overflow` prelude the scalar non-fastbin path carries
        // (there is no magazine / `refill_magazine_slow` cold path in this
        // configuration to place it in — identical placement rationale as the
        // scalar path's own comment). NOT `drain_large_deferred_free`: that
        // stack is Large-segment-only and the scalar path gates it on
        // `class.is_none()` for the same reason.
        #[cfg(all(feature = "alloc-xthread", not(feature = "fastbin")))]
        {
            self.drain_heap_overflow();
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

    /// Shared Large-path loop for `alloc_batch` (no magazine for Large
    /// classes). Stamps each block's owning segment (cross-thread routing
    /// needs it), matching `HeapCore::alloc`'s Large fallthrough. Performs the
    /// SAME `drain_large_deferred_free` housekeeping the scalar `alloc`
    /// performs before a Large request (G3: a batch-only owner never runs the
    /// scalar path, so without it here, cross-thread-freed Large segments are
    /// never reclaimed and SegmentTable slots grow O(batches)).
    #[cfg(feature = "batch-api")]
    fn alloc_batch_large(&mut self, out: &mut [*mut u8], layout: Layout) -> usize {
        // G3 (P2): drain this heap's cross-thread Large-segment deferred-free
        // stack before the Large-classified loop below, exactly as the scalar
        // `alloc` does on every Large request (the `class.is_none()` guard
        // there is already satisfied by construction here — this function's
        // ONLY caller is `alloc_batch`'s Large branch). Without it, a
        // batch-only owner never runs the scalar path, so cross-thread-freed
        // Large segments queue on the deferred stack unboundedly: held
        // segments and SegmentTable slots grow O(batches) instead of reaching
        // a bounded steady state, until a real OOM or `table.register`
        // failure. This is owner-side housekeeping, NOT magazine hot-path
        // work: this function has no magazine fast path to protect.
        #[cfg(feature = "alloc-xthread")]
        {
            self.drain_large_deferred_free();
        }

        // Match the scalar non-fastbin allocation prelude.
        #[cfg(all(feature = "alloc-xthread", not(feature = "fastbin")))]
        self.drain_heap_overflow();

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
