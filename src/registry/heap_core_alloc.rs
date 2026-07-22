//! Allocation entry points for [`HeapCore`] (mechanical split of
//! `heap_core.rs`, task R6-CQ-7b).
//!
//! This file holds the `impl HeapCore { .. }` block for the alloc-side hot
//! path: `alloc`, `alloc_zeroed`, and the magazine-miss refill slow path
//! (`refill_magazine_slow`). Pure code-movement sibling of `heap_core.rs`;
//! no behavior changed.

use core::alloc::Layout;
#[cfg(all(feature = "alloc-stats", feature = "alloc-global", feature = "fastbin"))]
use core::sync::atomic::Ordering;

use crate::alloc_core::node::Node;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
use crate::alloc_core::os;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
use crate::alloc_core::segment_header::SegmentMeta;

use super::heap_core::HeapCore;

impl HeapCore {
    // -----------------------------------------------------------------------
    // Allocation entry points (12.3). Delegate to the substrate; under
    // `alloc-xthread` also drain the TFS and stamp segment ownership.
    // -----------------------------------------------------------------------

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
                        {
                            let base = os::segment_base_of_ptr(issued);
                            let off = (issued as usize - base as usize) as u32;
                            SegmentMeta::new(base).magazine_bitmap().clear_magazine(off);
                        }
                        // X7 Ф3 (task #191) touch (a): bump the generation at
                        // ISSUE. The block leaves the allocator's bookkeeping
                        // (the magazine) and enters the caller's hands — this
                        // is the life transition. Compiled ONLY under
                        // `hardened`; non-hardened is byte-identical (the
                        // `cfg(not)` branch is a bare passthrough).
                        #[cfg(feature = "hardened")]
                        {
                            let base = os::segment_base_of_ptr(issued);
                            let off = (issued as usize) - (base as usize);
                            // SAFETY: `base` is a live, exclusively-owned
                            // segment; `off` is a MIN_BLOCK-aligned offset.
                            #[allow(unsafe_code)]
                            unsafe {
                                crate::alloc_core::segment_header::bump_gen(base, off)
                            };
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

        #[cfg(feature = "virgin-zero-skip")]
        if let Some(class_idx) = class {
            // R12-10 (task #261, `virgin-zero-skip`): the PRODUCTION win.
            // Bypass the per-thread magazine entirely for this call (mirroring
            // exactly how the Large branch below bypasses `self.alloc()` to
            // reach `alloc_large`'s freshness tuple directly) and call
            // `AllocCore::alloc_small_with_virgin` so the virgin-carve signal
            // survives to this point.
            let (ptr, is_virgin) = self.core.alloc_small_with_virgin(class_idx);
            if !ptr.is_null() {
                self.stamp_segment_owner(ptr);
                if !is_virgin {
                    #[cfg(feature = "alloc-stats")]
                    crate::alloc_core::SMALL_ZERO_PASS_CALLS
                        .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
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
            let ptr = self.alloc(layout);
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
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
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

        let want = super::tcache::refill_n_for_class(SizeClasses::block_size(c));
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
            return core::ptr::null_mut(); // true OOM
        }
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
        // `AllocBitmap` inside `refill_class_bump_checked`, unchanged, PLUS
        // this `mark_magazine` for every block landing in the magazine). The
        // block at `new_cnt` is popped to the caller below and must NOT be
        // marked (it is being issued, not retained).
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
                crate::alloc_core::segment_header::bump_gen(base, off)
            };
        }
        issued
    }

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
    /// `heap_core_free.rs`'s `dealloc_own_thread_with_base`) is UNCHANGED —
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
        //     — see `heap_core_free.rs`'s magazine-push double-free guard).
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
    /// # ⚠ EXPERIMENTAL / UNSTABLE
    ///
    /// Same `batch-api` (requires `experimental`) no-semver-guarantees status
    /// as the `fastbin` variant above — see that doc comment (R12-12).
    #[cfg(not(feature = "fastbin"))]
    #[cfg(feature = "batch-api")]
    #[doc(hidden)]
    #[must_use]
    pub fn alloc_batch(&mut self, layout: Layout, out: &mut [*mut u8]) -> usize {
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
    /// needs it), matching `HeapCore::alloc`'s Large fallthrough.
    #[cfg(feature = "batch-api")]
    fn alloc_batch_large(&mut self, out: &mut [*mut u8], layout: Layout) -> usize {
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
