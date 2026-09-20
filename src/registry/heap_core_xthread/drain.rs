//! Deferred-free drain paths of the former flat `heap_core_xthread.rs`
//! (reorg step 4): the Large deferred-free stack push/drain pair and the
//! slot-resident `HeapOverflow` second-chance ring drain. Pure code-movement
//! sibling files; no behavior changed.

use core::sync::atomic::AtomicPtr;

// `os`/`SegmentMeta` are read only inside `drain_heap_overflow`'s
// `#[cfg(feature = "fastbin")]` arm, and `SegmentHeader` only inside its two
// `#[cfg(feature = "alloc-segment-directory")]` directory-sync blocks — gate
// the imports to match, so feature matrices without those features do not
// warn them unused.
#[cfg(feature = "fastbin")]
use crate::alloc_core::os;
#[cfg(feature = "alloc-segment-directory")]
use crate::alloc_core::segment_header::SegmentHeader;
#[cfg(feature = "fastbin")]
use crate::alloc_core::segment_header::SegmentMeta;
use crate::alloc_core::{node::Node, AllocCore};

use crate::registry::heap_core::HeapCore;

impl HeapCore {
    /// 0.3.0 (task A1); extracted for #132: push a Large/huge segment `base`
    /// onto the OWNING heap's deferred-free stack, given `head` — the
    /// owner's `thread_free_head()` (a `*const AtomicPtr<u8>`, obtained by a
    /// REMOTE freer from `owner_thread_free_at(segment_base)`). Called from
    /// [`dealloc_routing`](Self::dealloc_routing) in place of the old
    /// permanent-leak no-op.
    ///
    /// Thin delegation to the shared
    /// [`alloc_core::deferred_large::push_large_deferred_free`] primitive
    /// (byte-for-byte the same push/CAS/double-push-guard logic this method
    /// used to inline — see that function's doc comment for the full
    /// mechanism and the double-push-guard hardening rationale). The
    /// primitive takes `&AtomicPtr<u8>` directly, so the pointer-to-reference
    /// deref of `head` stays HERE (via the `alloc_core::node` seam, same
    /// discipline as `deferred_next_atomic`/`owner_state_atomic`) rather
    /// than inside the shared (seam-free) module.
    #[cfg(feature = "alloc-xthread")]
    pub(super) fn push_large_deferred_free(head: *const AtomicPtr<u8>, base: *mut u8) {
        // `registry::heap_core` is NOT an allowed `unsafe` seam (see
        // `src/lib.rs`'s seam whitelist), so the pointer-to-reference deref
        // is delegated to `Node::atomic_ptr_ref` (the `alloc_core::node`
        // seam), same discipline as
        // `deferred_next_atomic`/`owner_state_atomic`.
        let head_ref: &AtomicPtr<u8> = Node::atomic_ptr_ref(head);
        crate::alloc_core::deferred_large::push_large_deferred_free(head_ref, base);
    }

    /// 0.3.0 (task A1); extracted for #132: drain this heap's deferred-free
    /// stack, reclaiming every queued Large/huge segment base via
    /// [`AllocCore::reclaim_large_segment`]. Called by the OWNER on its own
    /// `alloc_large` slow path, before reserving a fresh segment, so a
    /// cross-thread-freed large segment becomes available for reuse (via the
    /// `alloc-decommit` large-cache) or is released to the OS immediately
    /// (without `alloc-decommit`) — either way its `SegmentTable` slot is
    /// freed for reuse (the fix for the A1 permanent-leak bug).
    ///
    /// Thin delegation to the shared
    /// [`alloc_core::deferred_large::drain_large_deferred_free`] primitive
    /// (byte-for-byte the same pop-loop/reclaim logic this method used to
    /// inline).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn drain_large_deferred_free(&mut self) {
        // task H1: drain through the `&'static` slot handle, NOT an inline
        // field. `None` only in the pre-bind window — nothing could have been
        // pushed yet then (a remote push needs the stamp, which needs the
        // handle), so an empty-stack no-op is correct. Resolving the handle
        // BEFORE forming the `&mut self.core` split-borrow keeps the head
        // reference (a `&'static` into the slot, outside this `&mut HeapCore`)
        // disjoint from the core borrow.
        if let Some(head) = self.thread_free {
            crate::alloc_core::deferred_large::drain_large_deferred_free(head, &mut self.core);
        }
    }

    /// RAD-4b (task #72): drain THIS heap's slot-resident
    /// [`HeapOverflow`](crate::registry::heap_overflow::HeapOverflow) ring — the
    /// second-chance queue [`push_to_heap_overflow`](Self::push_to_heap_overflow)
    /// falls back to once a segment's own `RemoteFreeRing` AND its bounded
    /// retry are both exhausted (see that method's doc comment for the full
    /// design). Called by the OWNER on the SAME opportunistic schedule the
    /// per-segment rings are already drained (every magazine-miss slow path
    /// — see [`refill_magazine_slow`](Self::refill_magazine_slow) — and every
    /// `find_segment_with_free` scan), so overflow entries are reclaimed with
    /// the same liveness assumption every lazy-drain path in this allocator
    /// already relies on ("the owner drains on its own next `alloc()`").
    ///
    /// Each entry's `(base, packed)` pair is reclaimed via
    /// `AllocCore::reclaim_offset` (or, under `fastbin`, the
    /// magazine-checked `reclaim_offset_checked` — mirrors
    /// `dbg_drain_all_rings_impl`'s identical dual-path split) — the SAME
    /// defensively-guarded reclaim primitive the per-segment ring drain
    /// already uses, so a stale/garbled `base` (e.g. a segment recycled
    /// between push and drain) is rejected by its own magic/kind/bounds
    /// checks exactly as it would be for a per-segment ring entry, not
    /// specially trusted here.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn drain_heap_overflow(&mut self) {
        // RAD-4b: resolve through the pre-planted `&'static` handle (planted
        // by `bind_overflow` at claim time), NOT a fresh `bootstrap::ensure()`
        // + array index on every call — see the `overflow` field's doc
        // comment for the churn-gate cost this hoist recovers. `None` only in
        // the transient pre-bind window (never observed on any alloc/free
        // path — this drain runs only after a claimed heap's `alloc()`).
        let Some(overflow) = self.overflow else {
            return;
        };
        // RAD-4b iai churn-gate discipline: skip the full drain protocol
        // entirely on the overwhelmingly common "nothing has ever overflowed
        // into this ring" case — a single `Relaxed` load compared against our
        // own cached `tail`, mirroring the `last_stamped_segment` OPT-C cache
        // and `RemoteFreeRing`'s own documented `is_likely_empty` idiom. See
        // `HeapOverflow::is_likely_empty`'s doc comment for the full
        // soundness argument.
        if overflow.is_likely_empty(self.overflow_tail_cache) {
            return;
        }
        #[cfg(feature = "alloc-decommit")]
        let small_cur = self.core.small_cur();

        // R11-2 (Bug 2 — deferred pool/release finalization): collect segment
        // bases that go fully empty (`live_count` hits 0) during this drain
        // pass, to finalize via `release_or_pool_empty_segment` AFTER the
        // drain fully returns. This MUST be deferred, not done inline: a
        // single `overflow.drain(...)` call can process entries targeting
        // MANY different segment bases, and the SAME base can appear in
        // multiple entries. If base X goes empty at entry #2 of 5 targeting
        // base X, calling `release_or_pool_empty_segment(base X)` right there
        // would free/decommit/repurpose base X's metadata — and entries
        // #3–5 (still queued in this same drain pass) would then call
        // `reclaim_offset` against freed/decommitted memory.
        //
        // **Capacity.** `EMPTIED_BASES_CAP = 64`: going fully empty via this
        // SECOND-CHANCE overflow ring in one opportunistic drain call is a
        // rare tail event (requires ALL of a segment's live blocks to be
        // freed through the overflow ring, not through normal dealloc or the
        // per-segment ring). Under miri, `HEAP_OVERFLOW_CAP == 64`, so at
        // most 64 distinct bases can be drained in one pass — 64 covers
        // miri exactly. For native (`HEAP_OVERFLOW_CAP = 2048`), 64 gives
        // generous headroom for the realistic tail.
        //
        // **Capacity-exceeded case (R12-6: closed, was previously a silent
        // gap).** If a 65th distinct base goes empty, it is simply NOT
        // collected into `emptied_bases` — the segment stays as an ordinary
        // registered segment (its BinTable is populated with the freed
        // blocks, so `find_segment_with_free` will find and reuse it; it is
        // never leaked or unreachable). Before R12-6 it was ALSO never
        // finalized this pass except by chance on a future emptying through a
        // normal path — leaving native's realistic tail (`HEAP_OVERFLOW_CAP =
        // 2048` allows up to 2048 distinct bases in one drain, far past the
        // 64-slot dedup buffer) at inflated RSS/commit and outside the
        // pool-cap budget indefinitely, not merely "until next touched".
        // R12-6 closes this: `emptied_overflowed` below records whether the
        // buffer was actually exhausted, and if so a single post-drain sweep
        // (`AllocCore::finalize_orphaned_empty_segments`) scans every
        // registered segment for the ones the buffer had no room for and
        // finalizes them too. This is O(registered segments) instead of O(1),
        // but it runs ONLY on this genuinely rare tail (>64 DISTINCT bases
        // emptied by second-chance overflow reclaims alone, in one
        // opportunistic drain) — the common case (buffer never overflows)
        // still pays nothing beyond the existing dedup scan.
        //
        // **Dedup.** Under correctly-functioning ring data, `dec_live_and_
        // maybe_decommit` returns `true` at most once per base per drain
        // pass: `live_count` is monotonically non-increasing across this
        // pass (nothing here allocates, so nothing re-increments it), and
        // once it hits 0 any FURTHER entry for the same base necessarily
        // targets an already-freed block — rejected by `reclaim_offset`'s
        // `is_free` bitmap guard BEFORE `dec_live_and_maybe_decommit` is
        // ever called for it. So under normal operation the dedup scan is
        // defensive (belt-and-suspenders), not load-bearing.
        //
        // It is NOT purely theoretical, though: `SegmentMeta::dec_live` uses
        // `saturating_sub`, specifically so a live-count underflow "keeps
        // the counter sane rather than wrapping to `u32::MAX`" (see that
        // method's own doc comment) — which means if the `is_free` guard
        // were ever bypassed (a garbled/corrupt ring entry, or a future bug
        // elsewhere that lets an already-reclaimed offset re-enter the
        // ring), `dec_live` would clamp at 0 and `dec_live_and_maybe_
        // decommit` WOULD return `true` again for the same base in the same
        // pass. The dedup array is what keeps THAT scenario from calling
        // `release_or_pool_empty_segment` on the same base twice (a
        // double-pool/double-release, guarded again by that function's own
        // `debug_assert!` — see its doc comment). So this is real
        // defence-in-depth against the saturating-arithmetic edge case, not
        // dead code.
        // Only read by the `alloc-decommit`-gated declarations/usages below
        // — gate the constant itself so a build without `alloc-decommit`
        // (e.g. `hardened medium-classes`) does not warn it unused
        // (R23-5, task #374).
        #[cfg(feature = "alloc-decommit")]
        const EMPTIED_BASES_CAP: usize = 64;
        #[cfg(feature = "alloc-decommit")]
        let mut emptied_bases: [*mut u8; EMPTIED_BASES_CAP] =
            [core::ptr::null_mut(); EMPTIED_BASES_CAP];
        #[cfg(feature = "alloc-decommit")]
        let mut emptied_count: usize = 0;
        // R12-6: set when a distinct empty-transition is observed but the
        // dedup buffer above is already full (the 65th+ distinct base case
        // documented above) — signals that the rare post-drain fallback
        // sweep is needed after this drain returns.
        #[cfg(feature = "alloc-decommit")]
        let mut emptied_overflowed = false;

        #[cfg(feature = "fastbin")]
        {
            // No "class `c` currently being refilled" context exists at this
            // call site (unlike `refill_class_bump_checked`'s closure in
            // `refill_magazine_slow`, which special-cases `k == c` because
            // `count[c] == 0` is a load-bearing invariant for THAT specific
            // refill) — this drain reclaims entries of ANY class, so the
            // predicate unconditionally checks the magazine-residency bitmap,
            // mirroring `dbg_drain_all_rings_impl`'s general-purpose pattern.
            self.overflow_tail_cache = overflow.drain(|base, packed| {
                if AllocCore::reclaim_offset_checked(base, packed, &|ptr, _k| {
                    let pbase = os::segment_base_of_ptr(ptr);
                    let poff = (ptr as usize - pbase as usize) as u32;
                    SegmentMeta::new(pbase)
                        .magazine_bitmap()
                        .is_in_magazine(poff)
                }) {
                    // R11-2 (Bug 1): sync the segment directory inline per
                    // successful reclaim — mirrors the ESTABLISHED pattern
                    // in `drain_dirty_segments` / `find_segment_with_free_impl`'s
                    // per-segment ring drain, but with a per-entry immediate
                    // sync (1u64 << class_idx) instead of a batched bitmask,
                    // because `HeapOverflow` is a cross-segment MPSC ring
                    // (one drain call can touch many different bases).
                    //
                    // R13-12 (task #285): `sync_directory_for_segment_classes`
                    // lives in the `impl AllocCore` block gated
                    // `#[cfg(feature = "alloc-segment-directory")]`
                    // (`alloc_core_small.rs`) — a feature independent from
                    // `alloc-xthread`/`fastbin`. No combination of THIS
                    // module's own features pulls it in, so the call must be
                    // gated here too (mirrors every other call site of this
                    // method: `alloc_core_small.rs:895`, `:1220`, `:2170`,
                    // `alloc_core_small_reclaim.rs:529`). Without the
                    // directory sync the drain still reclaims the blocks
                    // (BinTable mutation happens unconditionally above); only
                    // the directory sidecar's bitmap goes unsynced, which is
                    // fine because the sidecar itself does not exist without
                    // this feature.
                    #[cfg(feature = "alloc-segment-directory")]
                    {
                        let sid = SegmentHeader::segment_id_at(base) as usize;
                        let class_idx =
                            crate::alloc_core::remote_free_ring::entry_class_idx(packed);
                        self.core
                            .sync_directory_for_segment_classes(base, sid, 1u64 << class_idx);
                    }
                    // R11-2 (Bug 2): collect the base for deferred
                    // pool/release if the segment just went empty.
                    #[cfg(feature = "alloc-decommit")]
                    {
                        if AllocCore::dec_live_and_maybe_decommit(base, small_cur) {
                            let already =
                                emptied_bases.iter().take(emptied_count).any(|&b| b == base);
                            if !already {
                                if emptied_count < EMPTIED_BASES_CAP {
                                    emptied_bases[emptied_count] = base;
                                    emptied_count += 1;
                                } else {
                                    // R12-6: a distinct 65th+ base emptied via
                                    // this drain's overflow-ring reclaims —
                                    // the dedup buffer has no room left.
                                    // Recorded here so the post-drain
                                    // fallback sweep below picks it (and any
                                    // sibling overflow bases) up.
                                    emptied_overflowed = true;
                                }
                            }
                        }
                    }
                }
            });
        }
        #[cfg(not(feature = "fastbin"))]
        {
            self.overflow_tail_cache = overflow.drain(|base, packed| {
                if AllocCore::reclaim_offset(base, packed) {
                    // R13-12 (task #285): see the symmetric `#[cfg]` note in
                    // the `fastbin` arm above — `sync_directory_for_segment_classes`
                    // requires `alloc-segment-directory`, which is independent
                    // of the features gating this file/arm.
                    #[cfg(feature = "alloc-segment-directory")]
                    {
                        let sid = SegmentHeader::segment_id_at(base) as usize;
                        let class_idx =
                            crate::alloc_core::remote_free_ring::entry_class_idx(packed);
                        self.core
                            .sync_directory_for_segment_classes(base, sid, 1u64 << class_idx);
                    }
                    #[cfg(feature = "alloc-decommit")]
                    {
                        if AllocCore::dec_live_and_maybe_decommit(base, small_cur) {
                            let already =
                                emptied_bases.iter().take(emptied_count).any(|&b| b == base);
                            if !already {
                                if emptied_count < EMPTIED_BASES_CAP {
                                    emptied_bases[emptied_count] = base;
                                    emptied_count += 1;
                                } else {
                                    // R12-6: a distinct 65th+ base emptied via
                                    // this drain's overflow-ring reclaims —
                                    // the dedup buffer has no room left.
                                    // Recorded here so the post-drain
                                    // fallback sweep below picks it (and any
                                    // sibling overflow bases) up.
                                    emptied_overflowed = true;
                                }
                            }
                        }
                    }
                }
            });
        }

        // R11-2 (Bug 2): finalize each emptied base now that the drain has
        // fully returned. Safe: no more entries will be processed against
        // any base in this drain pass, so releasing/pooling cannot race
        // with an in-flight reclaim.
        #[cfg(feature = "alloc-decommit")]
        for &base in emptied_bases.iter().take(emptied_count) {
            self.core.release_or_pool_empty_segment(base);
        }

        // R12-6: the dedup buffer overflowed (more than `EMPTIED_BASES_CAP`
        // distinct bases emptied via this drain's overflow-ring reclaims
        // alone) — run the rare post-drain fallback sweep to finalize the
        // ones the buffer had no room for. Same "drain has fully returned"
        // safety argument as the loop above: every overflow entry has
        // already been reclaimed by this point, so no further reclaim can
        // race a release/pool of any base.
        #[cfg(feature = "alloc-decommit")]
        if emptied_overflowed {
            self.core.finalize_orphaned_empty_segments(small_cur);
        }
    }
}
