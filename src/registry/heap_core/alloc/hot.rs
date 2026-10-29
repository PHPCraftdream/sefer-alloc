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

#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
use crate::registry::heap_core::state::tcache::TCACHE_CAP;
use crate::registry::heap_core::HeapCore;

impl HeapCore {
    // Allocation entry points delegate to the substrate; foreign frees are
    // published through pinned route descriptors and reclaimed by the owner.

    /// Task #2000 (registry review P3-1): the shared "clear this block's
    /// magazine-residency bit at issue time" step — a single-block magazine
    /// HIT (a fresh refill's issued block never sets this bit to begin with,
    /// so only the two hit arms, `alloc` and `alloc_small_zeroed_via_magazine`,
    /// need this) clears the physical magazine-residency bit set on admission.
    /// Resolves the stored segment root and clears its residency bit.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[inline(always)]
    pub(in crate::registry::heap_core) fn clear_magazine_on_issue(&self, issued: *mut u8) {
        let base = os::segment_base_of_ptr(issued);
        debug_assert_eq!(
            self.core.canonical_root_for(issued),
            Some(base),
            "small magazine block must map to its segment base"
        );
        let off = issued.addr() - base.addr();
        // B4 (в): flip the state byte MAG → LIVE at issue, preserving the
        // class field (written by the block's own MAG/FREE store).
        SegmentMeta::new(base)
            .state_byte()
            .set_state(off as u32, crate::alloc_core::state_byte::StateByte::LIVE);
    }

    /// Task #2002: the shared tail of [`refill_magazine_slow`](Self::refill_magazine_slow)
    /// and [`refill_magazine_slow_virgin`](Self::refill_magazine_slow_virgin) —
    /// they call two DIFFERENT `AllocCore` substrate refills (plain vs
    /// virgin-mask-tracking) and (the virgin sibling only) do extra
    /// virgin-mask bookkeeping before this point, but from here on the two
    /// were byte-for-byte identical: given `n` freshly-refilled blocks now
    /// resident in `tcache.classes[c].slots[0..n]`, mark the first `n-1` as
    /// magazine-resident (RAD-5), pop the last (`slots[n-1]`) for the caller.
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
        // Pop the top, leave n-1 in the magazine. (Task #2002: the refill
        // tail is state-byte / count bookkeeping only — no stamp pass.)
        let new_cnt = n - 1;
        self.tcache.classes[c].count = new_cnt as u8;
        // S1 (B4-M): the n-1 blocks REMAINING in the magazine were committed
        // MAG directly by the substrate refill (`refill_class_bump_state_internal`
        // → drain/carve `issue_state`), so this tail no longer scans
        // `segment_base_of_ptr` per retained block. Only the ONE block popped
        // to the caller needs a write here: flip its MAG → LIVE (single
        // `set_state`, class bits already correct per И1). Reclaim rejects MAG
        // blocks (same deferred-clear discipline as `alloc_batch`), so the
        // parked blocks are safe in flight between drain and park.
        let issued = self.tcache.classes[c].slots[new_cnt];
        let base = os::segment_base_of_ptr(issued);
        let off = (issued as usize - base as usize) as u32;
        debug_assert_eq!(
            SegmentMeta::new(base).state_byte().read(off) >> 6,
            crate::alloc_core::state_byte::StateByte::MAG,
            "refill-issued block must arrive MAG from the parameterized drain/carve"
        );
        SegmentMeta::new(base)
            .state_byte()
            .set_state(off, crate::alloc_core::state_byte::StateByte::LIVE);
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
    /// segment's [`SmallSidecar`](crate::registry::segment_route::SmallSidecar).
    /// Small free-list misses discover terminal bitmap publications through
    /// canonical table roots and reclaim using the issued sidecar class.
    /// Large requests consume Large descriptor obligations before consulting
    /// the cache or reserving memory. Owner mutation remains single-writer.
    #[must_use]
    #[inline(always)]
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        // Classify once for Large descriptor draining and magazine routing.
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
        let class = crate::alloc_core::size_classes::SizeClasses::class_for_layout(layout);

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
        // Large requests consume descriptor obligations before cache/reserve.
        // Small discovery stays in actual substrate misses; magazine hits
        // perform no table or sidecar-word scan.
        //
        // R6-T10(b): on every Large alloc, probe at most `LARGE_HOT_BUDGET`
        // (= 4) slots via the persistent round-robin cursor
        // `large_hot_cursor`, as `alloc_batch` already does (R12-05); the
        // rotating probe retires remote Large publications over successive
        // allocs, and the full sweep stays on the refill rescue and trim paths.
        // N1-alloc: under fastbin this drain moved INSIDE the
        // `alloc_large_without_magazine` tail (one terminal tail instead of
        // two non-tail calls); only the non-fastbin build keeps it here.
        #[cfg(all(
            feature = "alloc-xthread",
            not(all(feature = "alloc-global", feature = "fastbin"))
        ))]
        {
            if class.is_none() {
                self.drain_large_before_alloc();
            }
        }

        // Terminal Small publications are discovered on actual refill/free-list
        // misses through canonical table roots, not through an eager alloc scan.

        // ── Magazine fast path (P2+P4, fastbin) ─────────────────────────
        // Small-class allocations are served from the per-thread magazine.
        // On a hit: array pop, return — no per-alloc owner bookkeeping.
        // On a miss: batch-refill via the bump-direct refill, then pop one.
        // Routing never consults the header `owner_state`, so no alloc-side
        // stamping is needed on either path.
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
                        // No owner bookkeeping: routing goes through terminal
                        // descriptors, not the header `owner_state`.
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
                        // R6-T13: mask is an identity under the `count <= TCACHE_CAP` invariant; skips the bounds check without asm proof.
                        let issued = self.tcache.classes[c].slots[new_cnt & (TCACHE_CAP - 1)];
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
                        // magazine for the caller. THE HOT PATH: this runs on
                        // EVERY magazine hit under `production`, so it forces
                        // a `segment_base_of_ptr` + bitmap read-modify-write
                        // that this path previously did not pay AT ALL. See
                        // `docs/perf/IAI_BASELINE.md`'s RAD-5 entry for the
                        // measured cost of this specific store on
                        // `small_churn_16b` et al.
                        self.clear_magazine_on_issue(issued);
                        return issued;
                    }

                    // Magazine miss: batch-refill (P4).
                    // We inline the refill here instead of calling a shared
                    // helper because borrowing `self.core`
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
                    // single idle thread's cache. R1-01: the free side is
                    // bound by the SAME budget (`FREE_PARK_CAP`,
                    // `state/tcache.rs`) — a class's magazine can no longer
                    // accumulate more bytes than one refill would have parked
                    // by way of repeated frees either, closing the gap where
                    // this promise held only for a single refill event, not
                    // for the free path.
                    // Magazine miss: refill via the outlined slow path.
                    // `#[cold] #[inline(never)]` keeps the closure/split-borrow
                    // complexity out of `alloc`'s frame (task #164 Ir shaping).
                    return self.refill_magazine_slow(c);
                }
                // not a small class -> fall through to large path
            }
        }

        // Existing path: reclaim+alloc through AllocCore (large, or non-fastbin).
        // Without fastbin every alloc reaches here, so hand the class computed
        // above down instead of reclassifying inside `AllocCore::alloc`.
        #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
        return self.alloc_large_without_magazine(layout, class);
        #[cfg(not(all(feature = "alloc-global", feature = "fastbin")))]
        return self.core.alloc_with_class(layout, class);
    }

    #[cfg(feature = "alloc-xthread")]
    #[cold]
    #[inline(never)]
    fn drain_large_before_alloc(&mut self) {
        #[cfg(feature = "fastbin")]
        self.drain_large_sidecar_ingress_hot_bounded();
        #[cfg(not(feature = "fastbin"))]
        self.drain_large_sidecar_ingress();
    }

    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[cold]
    #[inline(never)]
    fn alloc_large_without_magazine(&mut self, layout: Layout, class: Option<usize>) -> *mut u8 {
        // N1-alloc: the Large-drain probe moved here from `alloc_with_class`
        // so the magazine-hit path never pays for it and the Large arm is a
        // single terminal tail call (drain + substrate alloc in one cold
        // frame; guard order unchanged: drain still precedes the alloc).
        #[cfg(feature = "alloc-xthread")]
        if class.is_none() {
            self.drain_large_before_alloc();
        }
        self.core.alloc(layout)
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
    /// caller's own `alloc_zeroed` pop, see `refill_class_bump_virgin`'s
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
            // R6-T13: mask is an identity under the `count <= TCACHE_CAP` invariant; skips the bounds check without asm proof.
            let issued = self.tcache.classes[c].slots[new_cnt & (TCACHE_CAP - 1)];
            // Read the slot's virgin bit BEFORE clearing it (this pop is the
            // block's last moment inside allocator bookkeeping — the mask
            // invariant "bits >= count are 0" must hold the instant `count`
            // drops, exactly like the plain `alloc` arm).
            let bit = 1u16 << new_cnt;
            let is_virgin = (self.tcache.classes[c].virgin_mask & bit) != 0;
            self.tcache.classes[c].virgin_mask &= !bit;
            self.clear_magazine_on_issue(issued);
            // F7 (task #495): NO stamp here — magazine-resident blocks were
            // placed there by the magazine refill paths
            // (`refill_magazine_slow`, `refill_magazine_slow_virgin`) or the
            // free path's own push
            // (`dealloc_own_thread` in `free/dealloc.rs` /
            // `dealloc_own_thread_with_base` in `free/dealloc_own_base.rs`).
            // Routing no longer depends on a per-alloc owner stamp, so no
            // owner bookkeeping is needed on this pop.
            return (issued, is_virgin);
        }
        // Magazine miss: the virgin-tracking sibling of `refill_magazine_slow`
        // (same UBFIX-10/RAD-4b drains, same refill/issue shape) that
        // additionally threads the per-slot virgin mask through the refill
        // and reports the ISSUED block's own bit back to the caller.
        self.refill_magazine_slow_virgin(c)
    }

    /// R13-3 (task #273): virgin-tracking sibling of
    /// [`refill_magazine_slow`](Self::refill_magazine_slow), consumed ONLY by
    /// [`alloc_small_zeroed_via_magazine`](Self::alloc_small_zeroed_via_magazine).
    /// Identical drain/refill/issue shape (see that function's doc for
    /// the bounded Large-only probe) plus: calls
    /// [`AllocCore::refill_class_bump_virgin_internal`] instead of the ordinary refill,
    /// stores the resulting per-slot virgin mask into
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
        // B4 depot (DP-3): a parked magazine holds freed blocks — never virgin
        // (`depot_reload` clears the mask).
        if self.depot_may_reload(c) && self.depot_reload_tick(c) {
            return (self.finish_magazine_refill(c, TCACHE_CAP), false);
        }
        // G1-K7: a segment refill is a cold event for the overflow decay
        // while the class is armed — counted first, while only `self` and
        // `c` are live.
        #[cfg(feature = "alloc-decommit")]
        if self.depot_may_tick(c) {
            self.depot_segment_tick(c);
        }
        // UBFIX-10 / RAD-4b (see `refill_magazine_slow`'s doc for the full
        // rationale — identical placement, identical cheap-when-empty shape).
        self.drain_large_sidecar_ingress_hot_bounded();

        // PF-7: `FREE_PARK_CAP[c]` equals `refill_n_for_class(block_size(c))`
        // by construction (`tests/r6_t39_free_park_cap_equals_refill.rs`
        // pins it); read the table instead of dividing on every refill miss.
        let want = crate::registry::heap_core::state::tcache::FREE_PARK_CAP[c] as usize;
        let mut virgin_mask: u16 = 0;
        let n = self.refill_with_large_rescue(|heap| {
            let cur = &mut heap.tcache.classes[c];
            heap.core.refill_class_bump_virgin_state_internal(
                c,
                &mut cur.slots[0..want],
                &mut virgin_mask,
                crate::alloc_core::state_byte::StateByte::MAG,
            )
        });
        if n == 0 {
            return (::core::ptr::null_mut(), false);
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
        // Task #2002: mark_magazine / count tail,
        // shared verbatim with `refill_magazine_slow` — see
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
    /// The Large branch consumes the same descriptor obligations as
    /// [`alloc`](Self::alloc), then calls `alloc_large` directly to
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
        let class = crate::alloc_core::size_classes::SizeClasses::class_for_layout(layout);

        // The virgin signal flows through the magazine path, preserving warm
        // reuse and the same owner-side pending-publication discovery as alloc.
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
        // Without fastbin, use the direct substrate virginity result. Small
        // sidecar discovery remains miss-only; do not scan the full table on
        // every scalar calloc. Large obligations are swept by Large slow paths
        // and exclusive cold trim/maintenance.
        #[cfg(all(feature = "virgin-zero-skip", not(feature = "fastbin")))]
        if let Some(class_idx) = class {
            let (ptr, is_virgin) = self.core.alloc_small_with_virgin(class_idx);
            if !ptr.is_null() {
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
            if !ptr.is_null() {
                Node::zero(ptr, size);
            }
            return ptr;
        }

        // Large descriptor obligations are consumed before physical cache reuse.
        // R6-T10(b): bounded probe on every Large alloc (see `alloc_with_class`).
        #[cfg(feature = "alloc-xthread")]
        {
            #[cfg(feature = "fastbin")]
            self.drain_large_sidecar_ingress_hot_bounded();
            #[cfg(not(feature = "fastbin"))]
            self.drain_large_sidecar_ingress();
        }

        let (ptr, is_fresh) = self.core.alloc_large(size, align);
        if !ptr.is_null() && !is_fresh {
            // Reused (cache-hit) segment — or ANY allocation under miri
            // (R9-1: miri's System.alloc fallback does not zero, so
            // `alloc_large` withholds the freshness signal there): NOT
            // OS-zero-guaranteed — must explicitly zero the user span.
            // Fresh real-OS reservations skip this (the OS zero-fills the
            // whole reserved span at reserve time).
            #[cfg(feature = "alloc-stats")]
            crate::alloc_core::LARGE_ZERO_PASS_CALLS
                .fetch_add(1, ::core::sync::atomic::Ordering::Relaxed);
            Node::zero(ptr, size);
        }
        ptr
    }

    /// Task #164 (Ir shaping): outlined refill-miss path. All split-borrow
    /// and closure complexity lives here, behind a `#[cold] #[inline(never)]`
    /// call boundary, so `alloc`'s own frame is not bloated by register spills
    /// from the closure / `split_at_mut` machinery. Returns the popped pointer
    /// (the block to hand out), or null on true OOM.
    ///
    /// On a genuine magazine miss, consume pending Large descriptor obligations
    /// too, so Small-only churn can retire them. Each miss probes at most four
    /// active Large slots, never Small sidecar words; hits pay none of this work.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[cold]
    #[inline(never)]
    fn refill_magazine_slow(&mut self, c: usize) -> *mut u8 {
        // B4 depot (DP-3): reload a parked full magazine (blocks stay MAG).
        // G1-K7: an overflow reload counts a decay event inside the cold
        // helper; floor reloads stay K5's path.
        if self.depot_may_reload(c) && self.depot_reload_tick(c) {
            return self.finish_magazine_refill(c, TCACHE_CAP);
        }
        // G1-K7: a segment refill is a cold event for the overflow decay
        // while the class is armed — counted first, while only `self` and
        // `c` are live.
        #[cfg(feature = "alloc-decommit")]
        if self.depot_may_tick(c) {
            self.depot_segment_tick(c);
        }
        // Bounded Large-only probe; Small discovery occurs in the refill.
        self.drain_large_sidecar_ingress_hot_bounded();

        // PF-7: `FREE_PARK_CAP[c]` equals `refill_n_for_class(block_size(c))`
        // by construction (`tests/r6_t39_free_park_cap_equals_refill.rs`
        // pins it); read the table instead of dividing on every refill miss.
        let want = crate::registry::heap_core::state::tcache::FREE_PARK_CAP[c] as usize;
        // Write directly into this class's empty magazine. Residency checks
        // belong to the owner retirement primitive, not a caller closure.
        let n = self.refill_with_large_rescue(|heap| {
            let cur = &mut heap.tcache.classes[c];
            heap.core.refill_class_bump_state_internal(
                c,
                &mut cur.slots[0..want],
                crate::alloc_core::state_byte::StateByte::MAG,
            )
        });
        if n == 0 {
            return ::core::ptr::null_mut();
        }
        // Task #2002: mark_magazine / count tail,
        // shared verbatim with `refill_magazine_slow_virgin`
        // — see `finish_magazine_refill`'s own doc for the full rationale.
        self.finish_magazine_refill(c, n)
    }

    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[cold]
    pub(super) fn refill_with_large_rescue(
        &mut self,
        mut refill: impl FnMut(&mut Self) -> usize,
    ) -> usize {
        let n = refill(self);
        if n != 0 {
            return n;
        }
        self.drain_large_sidecar_ingress_rescue();
        refill(self)
    }

    /// Logical full-table model: a refill succeeds only after an active Large
    /// route retires. The sweep and retry are the production helper above.
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "internals",
        feature = "bench-internals"
    ))]
    #[doc(hidden)]
    pub fn dbg_large_rescue_refill_model(&mut self) -> (usize, usize) {
        let live_before = self.core.dbg_active_kind_census().1;
        let mut attempts = 0;
        let n = self.refill_with_large_rescue(|heap| {
            attempts += 1;
            usize::from(heap.core.dbg_active_kind_census().1 < live_before)
        });
        (n, attempts)
    }
}
