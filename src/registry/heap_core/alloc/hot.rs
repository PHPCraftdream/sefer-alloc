//! Allocation entry points for [`HeapCore`] (mechanical split of
//! `heap_core.rs`, task R6-CQ-7b).
//!
//! This file holds the `impl HeapCore { .. }` block for the alloc-side hot
//! path: `alloc`, `alloc_zeroed`, and the magazine-miss refill slow path
//! (`refill_magazine_slow`). Pure code-movement sibling of `heap_core.rs`;
//! no behavior changed.

use ::core::alloc::Layout;
#[cfg(all(feature = "alloc-stats", feature = "alloc-global", feature = "fastbin"))]
use ::core::sync::atomic::Ordering;

use crate::alloc_core::node::Node;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
use crate::alloc_core::os;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
use crate::alloc_core::segment_header::SegmentMeta;

use crate::registry::heap_core::HeapCore;

impl HeapCore {
    // -----------------------------------------------------------------------
    // Allocation entry points (12.3). Delegate to the substrate; under
    // `alloc-xthread` also drain the TFS and stamp segment ownership.
    // -----------------------------------------------------------------------

    /// Task #2000 (registry review P3-1): the shared "clear this block's
    /// magazine-residency bit at issue time" step — a single-block magazine
    /// HIT (a fresh refill's issued block never sets this bit to begin with,
    /// so only the two hit arms, `alloc` and `alloc_small_zeroed_via_magazine`,
    /// need this) clears the RAD-5 (E4) bit `refill_class_bump[_checked]`'s
    /// `mark_magazine` set on admission. Returns `(base, off)` so an
    /// immediately-following `hardened` generation bump
    /// ([`bump_gen_on_issue`](Self::bump_gen_on_issue)) can reuse them instead
    /// of re-deriving `base` via a second `segment_base_of_ptr` call — before
    /// this task both hit arms recomputed `base`/`off` independently for the
    /// clear and the hardened bump back to back, a real (if small)
    /// double-computation this extraction closes as a side effect of
    /// deduplication, not a separate fix.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[inline(always)]
    pub(in crate::registry::heap_core) fn clear_magazine_on_issue(
        issued: *mut u8,
    ) -> (*mut u8, usize) {
        let base = os::segment_base_of_ptr(issued);
        let off = issued as usize - base as usize;
        SegmentMeta::new(base)
            .magazine_bitmap()
            .clear_magazine(off as u32);
        (base, off)
    }

    /// Task #2000: the shared "bump the block's generation at magazine issue"
    /// step (X7 Ф3, task #191) — six near-identical inline copies of this
    /// exact call existed across `hot.rs`/`batch.rs`/`diag_probes.rs` before
    /// this task. The block leaves the allocator's bookkeeping (the
    /// magazine) and enters the caller's hands at this life transition;
    /// compiled ONLY under `hardened` (non-hardened builds never call this —
    /// every call site keeps its own `#[cfg(feature = "hardened")]` guard).
    ///
    /// # Safety
    ///
    /// `base` must be a live, exclusively-owned segment; `off` must be a
    /// MIN_BLOCK-aligned offset of a real block within it — the same
    /// precondition every inlined call site this replaces already
    /// documented locally.
    #[cfg(feature = "hardened")]
    #[inline(always)]
    #[allow(unsafe_code)]
    pub(super) unsafe fn bump_gen_on_issue(base: *mut u8, off: usize) {
        // SAFETY: forwarded from this fn's own contract, documented above.
        unsafe { crate::alloc_core::segment_header::bump_gen(base, off) };
    }

    /// Task #2002: the shared tail of [`refill_magazine_slow`](Self::refill_magazine_slow)
    /// and [`refill_magazine_slow_virgin`](Self::refill_magazine_slow_virgin) —
    /// they call two DIFFERENT `AllocCore` substrate refills (plain vs
    /// virgin-mask-tracking) and (the virgin sibling only) do extra
    /// virgin-mask bookkeeping before this point, but from here on the two
    /// were byte-for-byte identical: given `n` freshly-refilled blocks now
    /// resident in `tcache.classes[c].slots[0..n]`, stamp each distinct
    /// source segment (P4 hoist + Э11 stamp-dedupe), mark the first `n-1` as
    /// magazine-resident (RAD-5), pop the last (`slots[n-1]`) for the caller,
    /// and (hardened only) bump its generation at issue.
    ///
    /// # Preconditions
    ///
    /// `n >= 1` — both callers already return their own OOM signal (`null`
    /// for the plain sibling, `(null, false)` for the virgin one) on `n == 0`
    /// BEFORE calling this, so this function never sees that case. `n` must
    /// be the exact count the caller's own substrate refill call just wrote
    /// into `tcache.classes[c].slots[0..n]`.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[inline(always)]
    fn finish_magazine_refill(&mut self, c: usize, n: usize) -> *mut u8 {
        debug_assert!(
            n >= 1,
            "finish_magazine_refill requires n >= 1 (OOM is the caller's job)"
        );
        // P4 stamp hoist + Э11 (task #161) stamp-dedupe: stamp each
        // pulled block's source segment, but call `stamp_segment_owner`
        // only when the block's segment base CHANGES from the previous
        // block's. Idempotent per segment; one stamp per distinct source.
        let mut prev_base = usize::MAX;
        for i in 0..n {
            let p = self.tcache.classes[c].slots[i];
            if !p.is_null() {
                let base = os::segment_base_of_ptr(p) as usize;
                if base != prev_base {
                    self.stamp_segment_owner(p);
                    prev_base = base;
                }
            }
        }
        // Pop the top, leave n-1 in the magazine.
        let new_cnt = n - 1;
        self.tcache.classes[c].count = new_cnt as u8;
        // RAD-5: mark the n-1 blocks REMAINING in the magazine as
        // magazine-resident (refill = existing `mark_alloc`/leave-unset on
        // `AllocBitmap` inside the caller's substrate refill call, unchanged,
        // PLUS this `mark_magazine` for every block landing in the
        // magazine). The block at `new_cnt` is popped to the caller below and
        // must NOT be marked (it is being issued, not retained).
        for &p in &self.tcache.classes[c].slots[0..new_cnt] {
            let pbase = os::segment_base_of_ptr(p);
            let poff = (p as usize - pbase as usize) as u32;
            SegmentMeta::new(pbase)
                .magazine_bitmap()
                .mark_magazine(poff);
        }
        let issued = self.tcache.classes[c].slots[new_cnt];
        // X7 Ф3 (task #191) touch (a): bump the generation at ISSUE. The block
        // leaves the allocator's bookkeeping (the magazine) and enters the
        // caller's hands — this is the life transition. This is the refill
        // path's issue point (the refill fills n slots, then pops ONE off the
        // top for the caller; the remaining n-1 are still allocator-owned in
        // the magazine and are bumped on THEIR respective pops). Compiled ONLY
        // under `hardened`; non-hardened is byte-identical.
        #[cfg(feature = "hardened")]
        {
            let base = os::segment_base_of_ptr(issued);
            let off = (issued as usize) - (base as usize);
            // SAFETY: `base` is a live, exclusively-owned segment; `off` is a
            // MIN_BLOCK-aligned offset.
            #[allow(unsafe_code)]
            unsafe {
                Self::bump_gen_on_issue(base, off);
            }
        }
        issued
    }

    /// Allocate `layout.size()` bytes satisfying `layout.align()`. Returns a
    /// non-null `*mut u8` on success, or null on OOM. Memory is
    /// **uninitialised** (matching `GlobalAlloc::alloc`).
    ///
    /// Own-thread path: delegates to [`AllocCore::alloc`] (the single-thread
    /// substrate, no adoption hook — a heap owns its segments exclusively and
    /// never pulls in segments from other heaps). Under `alloc-xthread`,
    /// cross-thread frees that targeted this heap's segments sit in each
    /// segment's [`RemoteFreeRing`](crate::alloc_core::remote_free_ring) and are
    /// reclaimed LAZILY by [`AllocCore::find_segment_with_free`] on a free-list
    /// miss (it drains every owned segment's ring via `reclaim_offset`, which
    /// trusts the class carried in the ring entry — never the owner's `page_map`,
    /// unreliable for mixed-class pages, §13). This is the `ShardedRegion` 7b
    /// shard-reuse discipline; everything else is single-writer (this thread
    /// owns the slot, ergo its segments).
    #[must_use]
    #[inline(always)]
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        // 0.3.0 (task A1): drain this heap's cross-thread Large-segment
        // deferred-free stack before a Large-classified request reaches
        // `AllocCore::alloc_large`'s slow path. Mirrors the RemoteFreeRing's
        // lazy-drain discipline (see the comment block below) but is scoped
        // to ONLY the Large-request case (checked here, not inside
        // `AllocCore`, because `AllocCore` has no `HeapCore` back-reference
        // to drain from) — small-classified requests pay zero cost for this
        // check beyond the one `class_for` call already needed below.
        // Э9 (P7.1, task #160): classify ONCE. `size`, `align` and
        // `class_for(size, align)` are pure functions of `layout`; they were
        // previously computed TWICE per alloc under production (once in the
        // xthread Large-drain check, once in the fastbin magazine-routing
        // block). We compute them a single time here and thread the result
        // through both consumers. The binding is gated on `any(...)` so it
        // exists whenever EITHER consumer is compiled in, and each consuming
        // block stays behind its own cfg. Behaviour is byte-identical
        // (`class_for` is pure → same index; the A1 Large-drain fires for
        // exactly the same Large-classified layouts).
        #[cfg(any(
            feature = "alloc-xthread",
            all(feature = "alloc-global", feature = "fastbin")
        ))]
        let size = layout
            .size()
            .max(crate::alloc_core::size_classes::MIN_BLOCK);
        #[cfg(any(
            feature = "alloc-xthread",
            all(feature = "alloc-global", feature = "fastbin")
        ))]
        let align = layout.align();
        #[cfg(any(
            feature = "alloc-xthread",
            all(feature = "alloc-global", feature = "fastbin")
        ))]
        let class = crate::alloc_core::size_classes::SizeClasses::class_for(size, align);

        // #1982: hand the single classification to the shared body instead of
        // letting a second caller re-derive it. `alloc_zeroed`'s small arm
        // calls `alloc_with_class` directly with the `class` it already
        // computed, so a calloc-shaped small request classifies ONCE for the
        // whole call chain — the call-chain half of the Э9 "classify ONCE"
        // rule that had only ever been applied inside this function.
        #[cfg(any(
            feature = "alloc-xthread",
            all(feature = "alloc-global", feature = "fastbin")
        ))]
        {
            self.alloc_with_class(layout, class)
        }
        #[cfg(not(any(
            feature = "alloc-xthread",
            all(feature = "alloc-global", feature = "fastbin")
        )))]
        {
            self.alloc_with_class(layout)
        }
    }

    /// The body of [`alloc`](Self::alloc), taking the size-class
    /// classification as a parameter so callers that already computed it do
    /// not pay for it twice (#1982).
    ///
    /// `class` is the `Option<usize>` from
    /// `SizeClasses::class_for(size.max(MIN_BLOCK), align)` — `None` means
    /// Large-classified. The parameter is `#[cfg]`-gated to exactly the
    /// configurations whose bodies consume it (`alloc-xthread`'s Large-drain
    /// check and the `alloc-global + fastbin` magazine routing), so a build
    /// with neither feature neither passes nor computes a classification,
    /// exactly as before this split.
    ///
    /// `#[inline(always)]`: this is a pure extraction of `alloc`'s own body,
    /// and `alloc` is itself `#[inline(always)]` — the split must not put a
    /// call boundary on the hottest path in the allocator.
    #[must_use]
    #[inline(always)]
    fn alloc_with_class(
        &mut self,
        layout: Layout,
        #[cfg(any(
            feature = "alloc-xthread",
            all(feature = "alloc-global", feature = "fastbin")
        ))]
        class: Option<usize>,
    ) -> *mut u8 {
        // 0.3.0 (task A1): drain this heap's cross-thread Large-segment
        // deferred-free stack before a Large-classified request reaches
        // `AllocCore::alloc_large`'s slow path. Uses the single `class`
        // computed above (Large ⇔ `class.is_none()`).
        #[cfg(feature = "alloc-xthread")]
        {
            if class.is_none() {
                self.drain_large_deferred_free();
            }
        }

        // RAD-4b (task #72): opportunistically drain this heap's
        // slot-resident `HeapOverflow` second-chance ring — see
        // `push_to_heap_overflow`'s doc comment for the full design. Under
        // `fastbin`, the drain is placed INSIDE `refill_magazine_slow`
        // instead (a `#[cold] #[inline(never)]` magazine-MISS-only path —
        // see that function), so the magazine-HIT fast path this file's own
        // churn benchmarks measure pays NOTHING extra: adding an unconditional
        // two-atomic-load check here, ahead of the magazine fast path below,
        // would tax every alloc including hits, which is exactly the
        // hot-path leak the task's iai gate exists to catch. Builds WITHOUT
        // `fastbin` have no magazine and hence no `refill_magazine_slow`
        // cold-path hook, so for them this call is the only opportunistic
        // site — unconditional here, but that configuration has no magazine
        // fast path to protect in the first place.
        #[cfg(all(feature = "alloc-xthread", not(feature = "fastbin")))]
        {
            self.drain_heap_overflow();
        }

        // Cross-thread-freed blocks are reclaimed LAZILY, inside
        // `AllocCore::find_segment_with_free` (the alloc-slow-path drains each
        // owned segment's `RemoteFreeRing` → `reclaim_offset`). We do NOT drain
        // eagerly on every alloc: that was a redundant deviation from the
        // `ShardedRegion` lazy discipline, and draining-before-alloc under a
        // real allocation workload (the installed `#[global_allocator]` serving
        // libtest's own cross-thread frees) corrupted the free list, while the
        // lazy slow-path drain handles the identical workload correctly
        // (verified: `global_alloc_installed` + `race_repro` ×5). Reclaim
        // completeness is preserved — the owner drains a segment's ring the
        // moment it needs a free block from it; until then cross-thread frees
        // sit in the bounded ring (overflow → bounded leak, the original 7b
        // discipline).

        // ── Magazine fast path (P2+P4, fastbin) ─────────────────────────
        // Small-class allocations are served from the per-thread magazine.
        // On a hit: array pop, return — NO per-alloc stamp (P4 hoist).
        // On a miss: batch-refill via `refill_class_stamped` (stamps each
        // distinct source segment exactly once inside the refill), then pop
        // one. The large path still stamps per-alloc (it does not go
        // through the magazine/refill).
        #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
        {
            // Э9 (P7.1): `size`, `align`, `class` come from the single
            // classification hoisted above — no recompute here.
            // C1 (0.3.0): the magazine fast path used to be gated on
            // `align <= SMALL_ALIGN_MAX` (16), so every align>16 request
            // (tokio `Cell` at align=128, page-aligned buffers, etc.) fell
            // through to the substrate on EVERY alloc/dealloc, bypassing the
            // magazine entirely. This is unnecessary: `class_for(size, align)`
            // already guarantees (for any `Some(c)` it returns) that
            // `block_size(c) % align == 0` — see its divisibility-walk slow
            // path in `size_classes.rs`. Every block carved for class `c` sits
            // at an offset that is a multiple of `block_size(c)` (see
            // `carve_block`'s `align_up(bump, block_size)`), and the segment
            // itself is 4 MiB (SEGMENT)-aligned, so any block of class `c` is
            // automatically `align`-aligned regardless of what `align` was —
            // the SAME guarantee the substrate's own `alloc_small` relies on.
            // Keying the magazine purely by `class_idx` (derived from the
            // caller-supplied `Layout` on both alloc and dealloc, per the
            // `GlobalAlloc` contract) is therefore sound for any align that
            // `class_for` accepted. Cross-thread routing is unaffected: this
            // whole block is the OWN-THREAD path (`dealloc_routing` decides
            // ownership BEFORE reaching `dealloc_own_thread`/the magazine).
            {
                if let Some(c) = class {
                    let cnt = self.tcache.classes[c].count as usize;
                    if cnt > 0 {
                        // Magazine hit: pop from the top of the stack.
                        // P4: NO stamp here — the block's source segment was
                        // already stamped during the refill that originally
                        // pulled it. The OPT-C cache guarantees the segment
                        // header still carries our ownership.
                        let new_cnt = cnt - 1;
                        self.tcache.classes[c].count = new_cnt as u8;
                        // Э5 (task #145): load+store instead of `fetch_add` — no
                        // `lock xadd` on the churn hot path. SOUND because this
                        // thread is the SOLE WRITER of ITS OWN counter: it is a
                        // per-heap/per-slot counter and this magazine-hit path
                        // runs only on the owning thread (the single-writer
                        // invariant `tls_heap.rs` establishes — `current_for_alloc`
                        // yields `Own(&mut HeapCore)` only to the thread that won
                        // the slot's claim CAS). No other thread ever increments
                        // it, so a non-atomic RMW split into a Relaxed load +
                        // Relaxed store cannot lose an update. The remote
                        // `stats()` reader (`tcache_hits_total`) still does a
                        // Relaxed atomic load and observes a monotonically
                        // non-decreasing value — identical visibility to the old
                        // `fetch_add(Relaxed)` (Relaxed gives no ordering either
                        // way; only atomicity of the single word, which `store`
                        // preserves). Only the lock prefix is dropped.
                        //
                        // W3: the counter STORAGE lives in the owning `HeapSlot`
                        // (closing the Stacked-Borrows aliasing gap — see the
                        // `TcacheHitCounter` module comment); `self.tcache_hits`
                        // is the stable `&'static AtomicU64` handle `claim`
                        // planted at bind time. `Some` on every alloc path (alloc
                        // only runs after `claim` bound it). Same 2 mem-ops as
                        // before, now to the slot's field rather than an inline
                        // one. Safe reference; no `unsafe` (deny-unsafe module).
                        //
                        // W3 Part B: the per-hit bump is gated behind
                        // `alloc-stats` (default OFF, NOT in `production`) —
                        // when off it compiles OUT of the churn hot path and
                        // `stats().tcache_hits` reads 0; when on it costs ~2
                        // mem-ops + one `Option` branch per hit. See the
                        // `alloc-stats` feature doc in Cargo.toml and the Part-B
                        // Ir measurement in the task W3 report.
                        #[cfg(feature = "alloc-stats")]
                        if let Some(hits) = self.tcache_hits {
                            hits.store(
                                hits.load(Ordering::Relaxed).wrapping_add(1),
                                Ordering::Relaxed,
                            );
                        }
                        let issued = self.tcache.classes[c].slots[new_cnt];
                        // R13-3 (task #273): clear this slot's virgin bit on
                        // EVERY pop through the plain `alloc` fast path, not
                        // just the `alloc_zeroed`-aware pop below. Maintains
                        // the `PerClass::virgin_mask` invariant ("bits >=
                        // count are always 0") unconditionally — a slot that
                        // WAS virgin and is popped here (a caller using plain
                        // `alloc`, not `alloc_zeroed`) must not leave a stale
                        // set bit that a LATER push into this same physical
                        // index could be misread through. Single `u16` AND,
                        // compiled out entirely (the field does not exist)
                        // when `virgin-zero-skip` is off.
                        #[cfg(feature = "virgin-zero-skip")]
                        {
                            self.tcache.classes[c].virgin_mask &= !(1u16 << new_cnt);
                        }
                        // RAD-5 (E4) GO/NO-GO EXPERIMENT: clear the
                        // magazine-residency bit — this block leaves the
                        // magazine for the caller. THE HOT PATH: unlike the
                        // `hardened`-only gen-table bump below, this runs on
                        // EVERY magazine hit under `production`, so it forces
                        // a `segment_base_of_ptr` + bitmap read-modify-write
                        // that this path previously did not pay AT ALL. See
                        // `docs/perf/IAI_BASELINE.md`'s RAD-5 entry for the
                        // measured cost of this specific store on
                        // `small_churn_16b` et al.
                        #[cfg_attr(not(feature = "hardened"), allow(unused_variables))]
                        let (base, off) = Self::clear_magazine_on_issue(issued);
                        // X7 Ф3 (task #191) touch (a): bump the generation at
                        // ISSUE. The block leaves the allocator's bookkeeping
                        // (the magazine) and enters the caller's hands — this
                        // is the life transition. Compiled ONLY under
                        // `hardened`; non-hardened is byte-identical (the
                        // `cfg(not)` branch is a bare passthrough).
                        #[cfg(feature = "hardened")]
                        {
                            // SAFETY: `base`/`off` describe the block just
                            // cleared above — a live, exclusively-owned
                            // segment and a MIN_BLOCK-aligned offset within
                            // it (`clear_magazine_on_issue`'s own contract).
                            #[allow(unsafe_code)]
                            unsafe {
                                Self::bump_gen_on_issue(base, off);
                            }
                        }
                        return issued;
                    }

                    // Magazine miss: batch-refill + stamp hoist (P4).
                    // We inline the refill+stamp here instead of calling
                    // `refill_class_stamped` because borrowing `self.core`
                    // and `self.tcache.classes[c].slots` separately avoids a
                    // double-mutable-borrow conflict on `self`.
                    //
                    // P3 (Э1, task #147): the miss refills via
                    // `refill_class_bump` — bump-direct batched carve. On a
                    // cold miss it drains existing free blocks first
                    // (pop_free / find_segment_with_free, which reclaims
                    // cross-thread frees — source order preserved), then
                    // bump-carves the remaining slots DIRECTLY into the
                    // magazine, skipping the old carve→BinTable→pop_free
                    // round-trip (a tautology on freshly-carved virgin
                    // blocks — bit 0 is already "allocated", so setting it
                    // free and immediately clearing it was pure overhead).
                    // D1/M2 end-state is byte-identical to the former
                    // `refill_class` (see `refill_class_bump`'s proofs).
                    //
                    // P3 (task #147): the P7 alloc-side bulk-bypass and the
                    // `alloc_streak` counter are RETIRED. bump-direct IS the
                    // ideal bulk path — a magazine miss now carves straight
                    // into the magazine at near-`memcpy` cost, so the
                    // "skip the magazine on an alloc-without-free streak"
                    // heuristic no longer buys anything. Retiring the alloc
                    // side also retires the dealloc-side companion flush
                    // (see `dealloc_own_thread`): without a streak counter it
                    // could never fire, so keeping it would be dead code.
                    //
                    // D3: the refill amount is a per-class BYTE budget, not
                    // the fixed `TCACHE_CAP` for every class — see
                    // `refill_n_for_class`. Small classes still get the full
                    // `TCACHE_CAP` (unchanged behaviour); large small-classes
                    // (block_size approaching SMALL_MAX) get fewer blocks per
                    // refill, so one magazine miss cannot park megabytes in a
                    // single idle thread's cache.
                    // Magazine miss: refill via the outlined slow path.
                    // `#[cold] #[inline(never)]` keeps the closure/split-borrow
                    // complexity out of `alloc`'s frame (task #164 Ir shaping).
                    return self.refill_magazine_slow(c);
                }
                // not a small class -> fall through to large path
            }
        }

        // Existing path: reclaim+alloc through AllocCore (large, or non-fastbin).
        let ptr = self.core.alloc(layout);
        if !ptr.is_null() {
            self.stamp_segment_owner(ptr);
        }
        ptr
    }

    /// R13-3 (task #273): magazine-aware pop for [`alloc_zeroed`](Self::alloc_zeroed)'s
    /// small arm. Structurally the SAME hit/miss shape as [`alloc`](Self::alloc)'s
    /// own magazine fast path (array pop on a hit, [`refill_magazine_slow`](
    /// Self::refill_magazine_slow) on a miss) — recovering the fast path
    /// `alloc_zeroed` lost under the R12-10 magazine-bypass design — PLUS
    /// reading (and, on a hit, clearing) the popped slot's
    /// `PerClass::virgin_mask` bit so the caller can still skip `Node::zero`
    /// for a genuinely virgin block sitting in the magazine (the R13-3 fix's
    /// whole point: a magazine HIT is not always non-virgin — a `carve_batch`
    /// refill can park still-virgin blocks in the magazine ahead of the
    /// caller's own `alloc_zeroed` pop, see `refill_class_bump_virgin_checked`'s
    /// doc). Returns `(ptr, is_virgin)`; `is_virgin` is always `false` when
    /// `ptr` is null.
    ///
    /// A deliberate, SEPARATE call site from `alloc`'s own magazine-hit arm
    /// (not a shared helper) so the plain `alloc`/`GlobalAlloc::alloc` fast
    /// path — the hottest path in the allocator per CLAUDE.md — pays NOT ONE
    /// EXTRA INSTRUCTION for this feature: the mask is still kept correct on
    /// that path (see the `virgin_mask` clear this file's `alloc` magazine-hit
    /// arm already added), but `alloc`'s own pop never READS it, only clears
    /// it (a plain AND-with-inverted-bit, no branch on the bit's value).
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "virgin-zero-skip"
    ))]
    #[inline]
    fn alloc_small_zeroed_via_magazine(&mut self, c: usize) -> (*mut u8, bool) {
        let cnt = self.tcache.classes[c].count as usize;
        if cnt > 0 {
            let new_cnt = cnt - 1;
            self.tcache.classes[c].count = new_cnt as u8;
            #[cfg(feature = "alloc-stats")]
            if let Some(hits) = self.tcache_hits {
                hits.store(
                    hits.load(Ordering::Relaxed).wrapping_add(1),
                    Ordering::Relaxed,
                );
            }
            let issued = self.tcache.classes[c].slots[new_cnt];
            // Read the slot's virgin bit BEFORE clearing it (this pop is the
            // block's last moment inside allocator bookkeeping — the mask
            // invariant "bits >= count are 0" must hold the instant `count`
            // drops, exactly like the plain `alloc` arm).
            let bit = 1u16 << new_cnt;
            let is_virgin = (self.tcache.classes[c].virgin_mask & bit) != 0;
            self.tcache.classes[c].virgin_mask &= !bit;
            #[cfg_attr(not(feature = "hardened"), allow(unused_variables))]
            let (base, off) = Self::clear_magazine_on_issue(issued);
            #[cfg(feature = "hardened")]
            {
                // SAFETY: `base`/`off` describe the block just cleared
                // above — a live, exclusively-owned segment and a
                // MIN_BLOCK-aligned offset within it
                // (`clear_magazine_on_issue`'s own contract).
                #[allow(unsafe_code)]
                unsafe {
                    Self::bump_gen_on_issue(base, off);
                }
            }
            // F7 (task #495): NO stamp here — same P4 reasoning as `alloc`'s
            // own magazine-hit arm above. Every block that can ever sit in
            // the magazine was placed there by one of exactly three
            // producers: `refill_magazine_slow`, `refill_magazine_slow_virgin`
            // (both stamp each distinct source segment via their
            // stamp-dedupe loop before any block lands in `tcache.classes`),
            // or the free path's own push
            // (`dealloc_own_thread` in `free/dealloc.rs` /
            // `dealloc_own_thread_with_base` in `free/dealloc_own_base.rs`,
            // itself reachable only via
            // `self.core.contains_base(base)` — i.e. only for a block whose
            // segment is already registered in THIS heap's own `AllocCore`,
            // which can only be true if this heap already stamped it on a
            // prior alloc). No path can pop a magazine-resident block whose
            // segment was never stamped with `self.id`, so this call was
            // pure redundant overhead (an extra `segment_base_of_ptr`
            // recompute — `clear_magazine` above already computed the same
            // base — plus a `last_stamped_segment` compare and a Relaxed
            // load of the owner-state word) under the arm's own P4
            // guarantee. See `docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md`'s
            // dated addendum for the corrected A/B framing.
            return (issued, is_virgin);
        }
        // Magazine miss: the virgin-tracking sibling of `refill_magazine_slow`
        // (same UBFIX-10/RAD-4b drains, same refill/stamp/issue shape) that
        // additionally threads the per-slot virgin mask through the refill
        // and reports the ISSUED block's own bit back to the caller.
        self.refill_magazine_slow_virgin(c)
    }

    /// R13-3 (task #273): virgin-tracking sibling of
    /// [`refill_magazine_slow`](Self::refill_magazine_slow), consumed ONLY by
    /// [`alloc_small_zeroed_via_magazine`](Self::alloc_small_zeroed_via_magazine).
    /// Identical drain/refill/stamp/issue shape (see that function's doc for
    /// the UBFIX-10/RAD-4b drain rationale, unchanged here) plus: calls
    /// [`AllocCore::refill_class_bump_virgin_checked`] instead of the plain
    /// `_checked` variant, stores the resulting per-slot virgin mask into
    /// `PerClass::virgin_mask` for the `n-1` blocks retained in the magazine,
    /// and reports the ONE block popped to the caller's own virgin bit
    /// (cleared from the mask before return, maintaining the "bits >= count
    /// are 0" invariant exactly like the plain miss path does for the
    /// magazine-residency bitmap).
    ///
    /// **`alloc-stats` parity check (R14-10/task #295):** this function does
    /// NOT bump any counter on the refill itself, exactly like
    /// [`refill_magazine_slow`](Self::refill_magazine_slow) — verified not a
    /// drift. `tcache_hits` (the only per-event `alloc-stats` counter either
    /// miss path could plausibly touch) is bumped ONLY by the respective
    /// caller's own magazine-HIT arm (`alloc`'s hit arm and
    /// `alloc_small_zeroed_via_magazine`'s hit arm, both above this
    /// function), never by the miss/refill path in either sibling — there is
    /// no separate "tcache miss" counter anywhere in this crate to bump. Both
    /// refill siblings are symmetric on this axis.
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "virgin-zero-skip"
    ))]
    #[cold]
    #[inline(never)]
    fn refill_magazine_slow_virgin(&mut self, c: usize) -> (*mut u8, bool) {
        use crate::alloc_core::size_classes::SizeClasses;

        // UBFIX-10 / RAD-4b (see `refill_magazine_slow`'s doc for the full
        // rationale — identical placement, identical cheap-when-empty shape).
        self.drain_large_deferred_free();
        self.drain_heap_overflow();

        let want = crate::registry::heap_core::state::tcache::refill_n_for_class(
            SizeClasses::block_size(c),
        );
        let (_before, rest) = self.tcache.classes.split_at_mut(c);
        let (cur, _after) = rest.split_first_mut().expect("c < SMALL_CLASS_COUNT");
        let mut virgin_mask: u16 = 0;
        let n = self.core.refill_class_bump_virgin_checked(
            c,
            &mut cur.slots[0..want],
            &|ptr, k| {
                if k == c {
                    return false;
                }
                let pbase = os::segment_base_of_ptr(ptr);
                let poff = (ptr as usize - pbase as usize) as u32;
                SegmentMeta::new(pbase)
                    .magazine_bitmap()
                    .is_in_magazine(poff)
            },
            &mut virgin_mask,
        );
        if n == 0 {
            return (::core::ptr::null_mut(), false); // true OOM
        }
        // Store the retained blocks' virgin bits (indices `0..new_cnt`); the
        // popped block (index `new_cnt`) is reported via the return value and
        // must NOT leave a stale set bit in the retained mask (same "bits >=
        // count are 0" invariant every other mutation site maintains).
        // Computed BEFORE the shared tail (which also computes/writes
        // `new_cnt`'s `count` field, a DIFFERENT field from `virgin_mask` —
        // no ordering dependency between the two field writes).
        let new_cnt = n - 1;
        let retained_bits_mask: u16 = if new_cnt >= 16 {
            u16::MAX
        } else {
            (1u16 << new_cnt) - 1
        };
        self.tcache.classes[c].virgin_mask = virgin_mask & retained_bits_mask;
        let issued_bit = 1u16 << new_cnt;
        let is_virgin = (virgin_mask & issued_bit) != 0;
        // Task #2002: stamp-dedupe / mark_magazine / count / hardened
        // bump_gen tail, shared verbatim with `refill_magazine_slow` — see
        // `finish_magazine_refill`'s own doc for the full rationale.
        let issued = self.finish_magazine_refill(c, n);
        (issued, is_virgin)
    }

    /// Allocate `layout.size()` bytes of **zeroed** memory.
    ///
    /// # Fresh-reservation skip (task #221 / R8-8)
    ///
    /// For a Large-classified request this mirrors the freshness-skip logic in
    /// [`AllocCore::alloc_zeroed`]: a genuinely fresh OS reservation is already
    /// zero-filled by the OS, so `Node::zero` is SKIPPED; a `large_cache` HIT
    /// (a reused segment that may hold the prior occupant's bytes) is zeroed
    /// explicitly. Small-classified requests delegate to `self.alloc` +
    /// unconditional `Node::zero` (byte-identical to the pre-task path)
    /// UNLESS the opt-in `virgin-zero-skip` feature (R12-10, task #261) is
    /// enabled, in which case a genuinely virgin (never-before-served)
    /// bump-carved small block ALSO gets the skip — see
    /// `AllocCore::alloc_small_with_virgin`'s doc for the exact virginity
    /// predicate. A free-list-served (reused) small block is NEVER treated
    /// as virgin and is always zeroed explicitly, exactly as before this
    /// feature existed.
    ///
    /// The Large branch replays the SAME Large-relevant prelude [`alloc`](Self::alloc)
    /// performs before reaching `AllocCore::alloc_large` —
    /// [`drain_large_deferred_free`](Self::drain_large_deferred_free) (A1,
    /// `alloc-xthread`) and [`drain_heap_overflow`](Self::drain_heap_overflow)
    /// (RAD-4b, `alloc-xthread` without `fastbin`, where it is otherwise hosted
    /// in the magazine-miss slow path) — then calls `alloc_large` DIRECTLY to
    /// obtain the freshness tuple (which `self.core.alloc` discards). The
    /// `virgin-zero-skip` Small branch mirrors this shape exactly: it
    /// bypasses `self.alloc()` (the magazine fast path) to reach
    /// `AllocCore::alloc_small_with_virgin` directly, so the virgin signal
    /// (which the magazine's `PerClass.slots: [*mut u8; TCACHE_CAP]` has no
    /// room to carry) is never lost. This is an additive restructuring of
    /// THIS method's own dispatch only; `HeapCore::alloc`'s body (the plain
    /// `alloc`/`GlobalAlloc::alloc` magazine fast path) is untouched.
    #[must_use]
    #[inline]
    pub fn alloc_zeroed(&mut self, layout: Layout) -> *mut u8 {
        let size = layout
            .size()
            .max(crate::alloc_core::size_classes::MIN_BLOCK);
        let align = layout.align();
        let class = crate::alloc_core::size_classes::SizeClasses::class_for(size, align);

        // R13-3 (task #273, P1 — resource-defect promotion): the R12-10
        // magazine-BYPASS design regressed warm-reuse steady-state (every
        // `alloc_zeroed` call — including a magazine HIT that would
        // otherwise be a pure array-pop — paid the substrate's free-list /
        // ring-drain / carve machinery) and, independently, dropped the
        // `alloc-xthread` drain prelude the plain `alloc`/`refill_magazine_slow`
        // path gets for free (see the `not(fastbin)` branch below and
        // `refill_magazine_slow`'s own UBFIX-10/RAD-4b drains) — a heap that
        // calls ONLY `alloc_zeroed` never drained its `HeapOverflow`/deferred
        // large-free stacks. Fixed by threading the virgin signal THROUGH the
        // magazine (`PerClass::virgin_mask`) instead of bypassing it: this
        // call now goes back through `self.alloc`'s own hit/miss machinery in
        // both `cfg` arms below, so both the fast-path win AND the drain
        // prelude are recovered structurally (same code path as `alloc`),
        // not re-implemented ad hoc.
        #[cfg(all(feature = "virgin-zero-skip", feature = "fastbin"))]
        if let Some(class_idx) = class {
            let (ptr, is_virgin) = self.alloc_small_zeroed_via_magazine(class_idx);
            if !ptr.is_null() && !is_virgin {
                #[cfg(feature = "alloc-stats")]
                crate::alloc_core::SMALL_ZERO_PASS_CALLS
                    .fetch_add(1, ::core::sync::atomic::Ordering::Relaxed);
                Node::zero(ptr, size);
            }
            return ptr;
        }
        // `virgin-zero-skip` without `fastbin`: there is no magazine to
        // plumb the signal through (the feature requires only
        // `alloc-decommit`, not `fastbin` — see `Cargo.toml`), so this
        // build keeps the R12-10 direct-substrate shape. Unlike R12-10,
        // it now ALSO replicates the Large branch's drain prelude below
        // (Defect 2 fix): without `fastbin` there is no
        // `refill_magazine_slow` to host the M-9/RAD-4b opportunistic
        // drains, so a heap calling ONLY `alloc_zeroed` would otherwise
        // never drain its cross-thread deferred-free stacks either.
        #[cfg(all(feature = "virgin-zero-skip", not(feature = "fastbin")))]
        if let Some(class_idx) = class {
            #[cfg(feature = "alloc-xthread")]
            {
                self.drain_large_deferred_free();
                self.drain_heap_overflow();
            }
            let (ptr, is_virgin) = self.core.alloc_small_with_virgin(class_idx);
            if !ptr.is_null() {
                self.stamp_segment_owner(ptr);
                if !is_virgin {
                    #[cfg(feature = "alloc-stats")]
                    crate::alloc_core::SMALL_ZERO_PASS_CALLS
                        .fetch_add(1, ::core::sync::atomic::Ordering::Relaxed);
                    Node::zero(ptr, size);
                }
            }
            return ptr;
        }
        #[cfg(not(feature = "virgin-zero-skip"))]
        if class.is_some() {
            // Small-classified: delegate ENTIRELY to the existing `alloc` +
            // unconditional `Node::zero` (byte-identical to the pre-task
            // path — this is the `virgin-zero-skip`-OFF behaviour).
            //
            // #1982: call `alloc_with_class` rather than `alloc`, handing it
            // the `class` computed at the top of this function. `alloc`'s only
            // work before its shared body IS that classification, so this is
            // the same path with the duplicate `class_for` removed — the
            // call-chain half of Э9's "classify ONCE" (P7.1, task #160), which
            // had only ever been applied WITHIN `alloc`. Plain `production`
            // does not enable `virgin-zero-skip`, so this is the arm every
            // calloc-shaped small allocation actually takes there.
            #[cfg(any(
                feature = "alloc-xthread",
                all(feature = "alloc-global", feature = "fastbin")
            ))]
            let ptr = self.alloc_with_class(layout, class);
            #[cfg(not(any(
                feature = "alloc-xthread",
                all(feature = "alloc-global", feature = "fastbin")
            )))]
            let ptr = self.alloc_with_class(layout);
            if !ptr.is_null() {
                Node::zero(ptr, size);
            }
            return ptr;
        }

        // Large-classified: replicate `alloc`'s Large-relevant prelude (the two
        // drains below — copied verbatim from `alloc`'s own prelude), THEN call
        // `alloc_large` directly for the freshness tuple. `alloc` gates
        // `drain_large_deferred_free` on `class.is_none()`; we are already in
        // the Large branch, so the `if` collapses to an unconditional call.
        #[cfg(feature = "alloc-xthread")]
        {
            self.drain_large_deferred_free();
        }
        #[cfg(all(feature = "alloc-xthread", not(feature = "fastbin")))]
        {
            self.drain_heap_overflow();
        }

        let (ptr, is_fresh) = self.core.alloc_large(size, align);
        if !ptr.is_null() {
            self.stamp_segment_owner(ptr);
            if !is_fresh {
                // Reused (cache-hit) segment — or ANY allocation under miri
                // (R9-1: miri's std::alloc fallback does not zero, so
                // `alloc_large` withholds the freshness signal there): NOT
                // OS-zero-guaranteed — must explicitly zero the user span.
                // Fresh real-OS reservations skip this (the OS zero-fills the
                // whole reserved span at reserve time).
                #[cfg(feature = "alloc-stats")]
                crate::alloc_core::LARGE_ZERO_PASS_CALLS
                    .fetch_add(1, ::core::sync::atomic::Ordering::Relaxed);
                Node::zero(ptr, size);
            }
        }
        ptr
    }

    /// Task #164 (Ir shaping): outlined refill-miss path. All split-borrow
    /// and closure complexity lives here, behind a `#[cold] #[inline(never)]`
    /// call boundary, so `alloc`'s own frame is not bloated by register spills
    /// from the closure / `split_at_mut` machinery. Returns the popped pointer
    /// (the block to hand out), or null on true OOM.
    ///
    /// UBFIX-10 (M-9): opportunistic Large-deferred-free drain. Before this
    /// task, `drain_large_deferred_free` was called ONLY from the two
    /// Large-classified sites in [`alloc`](Self::alloc)/[`realloc`](Self::realloc)
    /// — a heap that stopped allocating Large blocks entirely (e.g. a workload
    /// that starts Large-heavy and settles into Small-only churn) never drained
    /// again, so any cross-thread-freed Large segments queued on its deferred
    /// stack stayed mapped-but-dead for the rest of the process's life
    /// (unbounded resource retention, not UB — see
    /// `docs/reviews/2026-07-10-ub-audit-final-synthesis.md` M-9). This is the
    /// SMALL-path drain site: every magazine MISS (never a hit — this function
    /// runs only when the fast-path pop in `alloc` found `count[c] == 0`)
    /// opportunistically reclaims any queued Large segments too, so a
    /// Small-only workload still recovers them. Placement here (rather than
    /// unconditionally in `alloc`) keeps the check off the actual hot path —
    /// `refill_magazine_slow` is `#[cold] #[inline(never)]`, reached only on a
    /// miss, so the extra call costs nothing on the magazine-hit fast path
    /// this file's own churn benchmarks measure.
    ///
    /// The call is the SAME cheap-precheck shape draining always has:
    /// `drain_large_deferred_free`'s pop loop starts with a single Acquire
    /// load of the stack head and returns immediately if it is null (see
    /// `alloc_core::deferred_large::drain_large_deferred_free`) — an empty
    /// stack costs exactly one atomic load here, no CAS, no further work.
    /// `fastbin` requires `alloc-xthread` (`Cargo.toml`: `fastbin =
    /// ["alloc-global", "alloc-xthread"]`), so the call is unconditional
    /// inside this `fastbin`-gated function — no extra `cfg` needed.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[cold]
    #[inline(never)]
    fn refill_magazine_slow(&mut self, c: usize) -> *mut u8 {
        use crate::alloc_core::size_classes::SizeClasses;

        // UBFIX-10 (M-9): opportunistic drain on every magazine miss — see
        // the doc comment above. Cheap when empty (one Acquire load).
        self.drain_large_deferred_free();

        // RAD-4b (task #72): opportunistic drain of this heap's
        // `HeapOverflow` second-chance ring — same placement rationale as
        // the M-9 drain immediately above (magazine-MISS-only, so the
        // magazine-HIT fast path in `alloc` pays nothing extra). See
        // `push_to_heap_overflow`'s doc comment for the full design and
        // `alloc`'s matching non-fastbin call site.
        self.drain_heap_overflow();

        let want = crate::registry::heap_core::state::tcache::refill_n_for_class(
            SizeClasses::block_size(c),
        );
        // Task #164 / PERF-PASS-5 (G7): zero-copy split borrow. The refill
        // writes DIRECTLY into `tcache.classes[c].slots` (no buffer, no
        // copy). `split_at_mut`/`split_first_mut` is still needed to obtain
        // `cur: &mut PerClass` (the refill's write target) while the rest of
        // `self` stays usable inside the closure below.
        //
        // RAD-5 (E4) GO/NO-GO EXPERIMENT: the magazine predicate closure used
        // to scan OTHER classes' magazine slots (`entry.slots[0..cnt]`,
        // O(cnt) per candidate offset drained from a remote ring, via the
        // `before`/`after` split halves). Replaced with an O(1) probe of the
        // second (magazine-residency) bitmap — the probe is keyed by segment
        // offset, not by class, so `before`/`after` are no longer read (only
        // `cur.slots` as the write target survives from the original split).
        //
        // KEY INVARIANT (load-bearing): at refill time, `count[c] == 0` —
        // the refill runs ONLY on a magazine miss (the pop in `alloc` failed
        // because `cnt == 0`). So the predicate for class `c` itself is
        // trivially false (0 slots to scan), and the mutable borrow of
        // `classes[c].slots` (for the refill output) is never read by the
        // closure.
        let (_before, rest) = self.tcache.classes.split_at_mut(c);
        let (cur, _after) = rest.split_first_mut().expect("c < SMALL_CLASS_COUNT");
        let n = self
            .core
            .refill_class_bump_checked(c, &mut cur.slots[0..want], &|ptr, k| {
                if k == c {
                    return false;
                }
                let pbase = os::segment_base_of_ptr(ptr);
                let poff = (ptr as usize - pbase as usize) as u32;
                SegmentMeta::new(pbase)
                    .magazine_bitmap()
                    .is_in_magazine(poff)
            });
        if n == 0 {
            return ::core::ptr::null_mut(); // true OOM
        }
        // Task #2002: stamp-dedupe / mark_magazine / count / hardened
        // bump_gen tail, shared verbatim with `refill_magazine_slow_virgin`
        // — see `finish_magazine_refill`'s own doc for the full rationale.
        self.finish_magazine_refill(c, n)
    }
}
