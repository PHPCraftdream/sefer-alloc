//! Mechanism-2 empty-small-segment pool + M6 decommit cluster of [`AllocCore`]
//! (mechanical split of `alloc_core.rs`).
//!
//! This file holds an additional `impl AllocCore { .. }` block carrying the
//! empty-small-segment hysteresis pool and the decommit/live-count methods. It
//! is a pure code-movement sibling of `alloc_core.rs`; no behavior changed. The
//! whole module is `alloc-decommit`-gated because every method here is.

use core::ptr;

use super::node::Node;
use super::os::{self, SEGMENT};
use super::segment_header::{
    Layout as SegLayout, SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL,
};

use super::alloc_core::{AllocCore, DECOMMIT_CALLS};

impl AllocCore {
    /// Phase 35 (M6 decommit) — the shared dec-then-maybe-decommit step, called
    /// after a block returns to a segment's free list (own-thread `dealloc_small`
    /// or owner-side `reclaim_offset`). It decrements the owner-only `live_count`
    /// and, if the segment just went empty (`live_count == 0`) AND is not the
    /// current carve target (`base != small_cur`), returns the segment's payload
    /// pages to the OS, resets the segment, releases the OS reservation, and
    /// recycles the table slot (task #60, variant B).
    ///
    /// **Self-less** (associated fn) so the self-less `reclaim_offset` can call
    /// it; the `small_cur` snapshot and `table` raw pointer are threaded in from
    /// the owner. The raw pointer is sound because `AllocCore` is single-owner
    /// (owner thread is the sole writer of its segments' metadata and table).
    ///
    /// ## Why M6 is decommit-safe WITHOUT an M11 epoch barrier (design §1)
    ///
    /// The original plan (§2.5) reached for `crossbeam-epoch` because the OLD
    /// intrusive cross-thread-free model wrote the free-list `next` pointer INSIDE
    /// the block — a late cross-thread freer could write into a page we had just
    /// decommitted (UAF / write-to-unmapped). Variant-2 (Phase 12.6) dissolved
    /// that: the cross-thread freer NEVER dereferences the block — it pushes
    /// `(offset|class)` into the `RemoteFreeRing`, which lives in the segment's
    /// METADATA (the metadata pages are NEVER decommitted — we decommit only
    /// `[small_meta_end, SEGMENT)`). The decommit is therefore safe without epoch:
    ///
    ///   1. We decommit the payload ONLY at `live_count == 0` → there is not one
    ///      live block in the decommitted range; nothing to UAF.
    ///   2. A late VALID cross-thread free at `live_count == 0` is impossible:
    ///      every block is already free, so a further free of one is a double-free
    ///      (the bitmap `is_free` guard below makes it a no-op before any write).
    ///   3. `reclaim_offset` on a stale ring entry computes the block address via
    ///      `Node::deref` (pure arithmetic — NO memory access) and then reads
    ///      `magic` / `kind` / **bitmap `is_free`** — ALL in the never-decommitted
    ///      metadata — and for a free block (and at `live==0` ALL are free) does a
    ///      no-op BEFORE touching the block. The decommitted page is never read or
    ///      written.
    ///   4. `reclaim` (drain) and `decommit` both run owner-side, so they are
    ///      serialized on the owning thread — there is no reclaim-vs-decommit race
    ///      on one segment.
    ///
    /// ⇒ No UAF, no write to decommitted memory. `crossbeam-epoch` is NOT needed;
    /// none is added. (Full argument: `docs/PHASE35_DECOMMIT_DESIGN.md` §1.)
    ///
    /// ## Slot recycle (task #60)
    ///
    /// After decommit + reset, [`decommit_empty_segment`] also releases the OS
    /// reservation for the segment and NULLs the table slot (via `table`). This
    /// lifts the 1024-segment hard cap: the freed slot can be reused immediately
    /// by the next `register` call, so long-running workloads never exhaust the
    /// table. Both the OS release and the slot NULL happen atomically inside
    /// `decommit_empty_segment`; there is no window where the OS segment is
    /// released but the slot is still non-NULL.
    /// Returns `true` if decommit fired (the segment became empty, was
    /// decommitted, and needs slot recycling). The caller is responsible for
    /// calling `self.table.recycle(base)` when `true` is returned — but ONLY
    /// after any in-progress ring drain for `base` has completed, so that
    /// stale ring entries can still read the (still-committed) metadata.
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn dec_live_and_maybe_decommit(base: *mut u8, small_cur: *mut u8) -> bool {
        let mut meta = SegmentMeta::new(base);
        let live = meta.dec_live();
        // Only an empty, non-current, not-already-decommitted segment is
        // eligible for release/pool. The current carve target stays committed
        // (we are about to bump-allocate into it); already-decommitted is
        // idempotent.
        if live != 0 || base == small_cur || meta.is_decommitted() {
            return false;
        }
        // NEVER decommit the PRIMORDIAL segment: its metadata extends to
        // `primordial_meta_end()` (it hosts the self-hosted registry between
        // `small_meta_end()` and `primordial_meta_end()`), but the decommit reset
        // computes the payload start at `small_meta_end()`. Decommitting from
        // there would return the registry pages to the OS and reset page-map /
        // bump over the registry — corrupting the substrate. Only `Small`
        // segments (whose payload genuinely starts at `small_meta_end()`) are
        // eligible. A field-specific `kind` read (disjoint from the owner's
        // `bump`/`live_count` writes; race-free like the other `kind_at` reads).
        if !matches!(SegmentHeader::kind_at(base), SegmentKind::Small) {
            return false;
        }
        // Mechanism 2 (task #51): the reset (`decommit_empty_segment_for_release`)
        // is NO LONGER performed here. This fn is self-less (called from the
        // self-less `reclaim_offset`), so it cannot consult the per-`AllocCore`
        // pool. It now reports ONLY "this segment just emptied and is eligible
        // for release-or-pool"; the `&mut self` caller then routes to
        // [`release_or_pool_empty_segment`](Self::release_or_pool_empty_segment),
        // which either pools it (leaving `bump`/free-lists intact so the blocks
        // stay reusable) or does the release-follows reset + `table.recycle`.
        // Moving the reset to the caller is what makes pooling correct: the
        // former in-place `set_bump(payload_start)` would push every freed
        // block's offset `>= bump`, making a pooled segment's free-list blocks
        // unreachable.
        true
    }

    /// E3 (task W4) — batched dec-then-maybe-decommit for a same-segment flush
    /// run. Subtracts `k` (the number of accepted blocks in the run) from
    /// `live_count` in ONE `sub_live` and makes the SAME decommit decision the
    /// per-block loop would make.
    ///
    /// ## Byte-identical to `k` sequential `dec_live_and_maybe_decommit` calls
    ///
    /// `flush_run`'s doc already proves that within a same-segment run `live`
    /// can only reach 0 at the LAST accepted block (every still-un-flushed
    /// same-segment block counts as live, so the segment empties iff the run
    /// flushes ALL its remaining live blocks — and then only at block `k`). So:
    ///   - The final `live_count` is identical: `sub_live(k)` == `k` `dec_live`s.
    ///   - Decommit fires at most once, on the SAME transition (the k-th block
    ///     that brings `live` to 0), under the SAME proviso
    ///     (`live == 0 && base != small_cur && !is_decommitted && kind == Small`)
    ///     — the per-block loop's earlier iterations all had `live > 0` and so
    ///     never entered the decommit branch. Checking the proviso ONCE on the
    ///     post-`sub_live` value therefore reproduces the loop exactly.
    ///
    /// Returns `true` iff decommit fired (caller runs `table.recycle`).
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(super) fn dec_live_batch_and_maybe_decommit(
        base: *mut u8,
        k: u32,
        small_cur: *mut u8,
    ) -> bool {
        if k == 0 {
            return false;
        }
        let mut meta = SegmentMeta::new(base);
        let live = meta.sub_live(k);
        if live != 0 || base == small_cur || meta.is_decommitted() {
            return false;
        }
        // Same PRIMORDIAL exclusion as `dec_live_and_maybe_decommit`: only a
        // `Small` segment's payload genuinely starts at `small_meta_end()`.
        if !matches!(SegmentHeader::kind_at(base), SegmentKind::Small) {
            return false;
        }
        // Mechanism 2 (task #51): as in `dec_live_and_maybe_decommit`, the reset
        // is NO LONGER done here — the caller (`flush_run`) routes the `true`
        // return through `release_or_pool_empty_segment`.
        true
    }

    /// Mechanism 2 (task #51) — decide the fate of a small segment that just
    /// emptied (`dec_live_and_maybe_decommit` / `dec_live_batch_and_maybe_decommit`
    /// returned `true` for it): either RETAIN it in the empty-small-segment
    /// hysteresis pool (kept registered + committed, free-lists intact), or
    /// RELEASE it (the pre-Mechanism-2 behaviour: release-follows reset +
    /// `table.recycle`).
    ///
    /// Called from every site that observes a small segment reach
    /// `live_count == 0` — `dealloc_small`, the ring-drain in
    /// `find_segment_with_free_impl`, `flush_run`, and the test-only
    /// `dbg_drain_all_rings_impl` — in place of the former unconditional
    /// `self.table.recycle(base)`.
    ///
    /// ## Admission rule (bounded, synchronous — no reliance on a later tick)
    ///
    /// If the pool is enabled (`pool_cap > 0`) and NOT already full
    /// (`pooled_count < pool_cap`), the segment is admitted: pushed onto the
    /// pool array and left EXACTLY as it was the instant it emptied — still
    /// registered in the `SegmentTable`, pages still committed, `bump` wherever
    /// it was (near segment end, fully carved), `decommitted == false`, every
    /// class free list still populated with the blocks that were just freed.
    /// NOTHING is reset. A later `find_segment_with_free` finds those free
    /// blocks and reuses them in place (removing the segment from the pool via
    /// `unpool_if_present`) — the reuse costs NO OS reserve/release round-trip,
    /// which is the hysteresis win. (A pooled segment is never re-inserted as a
    /// fresh CARVE target: it is fully-carved, so `reserve_small_segment` always
    /// takes a genuinely fresh OS segment — the pool is a free-list reserve, not
    /// a carve reserve.)
    ///
    /// This holds identically under `alloc-lazy-commit` (R8-10, task #223):
    /// pool admission NEVER decommits or resets metadata, on either the eager
    /// or the lazy-commit path. A prior design (B3, R7 Workstream B) had the
    /// lazy-commit leg decommit the payload above the initial lazy chunk and
    /// reset `bump`/free-lists/`is_decommitted` on admission, turning the
    /// pooled segment into a "clean carve target" for `reserve_small_segment`
    /// to pop directly. That defeated the hysteresis pool's entire purpose: a
    /// segment admitted as "the warmest entry, expected back imminently" was
    /// immediately decommitted, so first-reuse always paid a recommit — 50-75×
    /// more `commit_range`/decommit syscalls per empty→pool→reuse→refill cycle
    /// than the eager path, which pays zero. The segment now stays exactly as
    /// committed as it was on emptying, and reuse goes through the SAME
    /// `find_segment_with_free` free-list path as the eager leg — no OS
    /// syscalls on the hot reuse edge, lazy-commit or not.
    ///
    /// If the pool is disabled OR already full, the segment is released
    /// immediately here — the pool never holds MORE than `pool_cap` at any
    /// instant, mid-scan or otherwise (this is the synchronous budget cap that
    /// keeps `regression_c3_unbounded_recycle`'s bound tight and predictable:
    /// at most `pool_cap` retained, ever).
    ///
    /// ## Stale-ring-while-pooled soundness (no special-casing needed)
    ///
    /// A pooled segment stays a NORMAL registered small segment — it is scanned
    /// by `find_segment_with_free_impl`'s ring drain exactly like any other, and
    /// receives NO "skip while pooled" treatment. This is sound because at
    /// `live_count == 0` EVERY block in the segment is already free, so any
    /// cross-thread free arriving for one of its offsets is necessarily a
    /// DOUBLE-FREE of an already-free block. `reclaim_offset` handles that with
    /// its existing bitmap `is_free` guard (a no-op that returns `false` BEFORE
    /// any `write_next`) — the SAME guard that already protected an
    /// about-to-be-decommitted empty segment (design §1.2). Crucially, because
    /// pooling does NOT reset `bump` (unlike the release path), the `off >= bump`
    /// guard does NOT fire for the segment's real block offsets; the `is_free`
    /// guard is what catches the double-free. Both are no-ops, both touch only
    /// never-decommitted metadata, and the payload stays committed the whole
    /// time — so there is no UAF and no write to unmapped memory (the M6 §1
    /// safety argument holds verbatim, and is in fact STRICTLY weaker to satisfy
    /// here since the payload is never even decommitted while pooled). Once the
    /// segment is un-pooled (reused via `find_segment_with_free`) and allocation
    /// resumes, its `live_count` rises and it behaves as an ordinary registered
    /// segment. Every empty-observing site `continue`s / returns after this
    /// call, so it yields `()`: the caller does not need to distinguish pooled
    /// from released.
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    pub(crate) fn release_or_pool_empty_segment(&mut self, base: *mut u8) {
        // Defence-in-depth against a double-entry: a segment that is already
        // pooled must never be pushed again (a duplicate base → later
        // double-recycle / a corrupt list). By construction this cannot
        // happen — a pooled segment is `unpool_if_present`-removed the
        // instant it is reused, so it carries no live block until
        // re-emptied, and re-emptying requires reuse first — but the guard
        // is O(1) and makes the invariant local and robust. Full membership
        // test, same disjunction `unpool_if_present` uses below: `base` is
        // pooled iff it IS the head, OR its `pool_prev` is non-null (a
        // not-pooled segment always has `pool_prev == null` — see
        // `SegmentHeader::small`/`large`'s initial state and
        // `pool_unlink`'s removal-time reset — and can never equal
        // `pool_head`, since the head is by definition pooled).
        debug_assert!(
            self.pool_head != base && SegmentMeta::new(base).pool_prev_of().is_null(),
            "double-pool of an already-pooled segment"
        );
        // Admit to the pool if enabled and there is room: push-front (this
        // segment becomes the new HEAD — the warmest entry, mirroring the old
        // array's "push at pooled_count" LIFO insertion).
        if self.pooled_count < self.pool_cap {
            // R8-10 (task #223): pool admission never decommits or resets
            // metadata, identically on the eager and `alloc-lazy-commit`
            // paths. The segment stays fully committed (or, under lazy-commit,
            // as committed as it was the instant it emptied) with free lists
            // intact, and reuse happens via `find_segment_with_free`'s
            // free-list path — see the doc comment above for why the former
            // B3 decommit-on-admission design was a 50-75× regression, not a
            // savings.
            Self::pool_push_front(
                &mut self.pool_head,
                &mut self.pool_tail,
                &mut self.pooled_count,
                base,
            );
            return; // pooled — base still valid/registered
        }
        // Pool disabled or full: release immediately (pre-Mechanism-2 path).
        // R7-A2: clear directory bits BEFORE the slot is recycled (the segment
        // metadata is still readable here; after recycle the slot is NULL and
        // the OS reservation may be released).
        #[cfg(feature = "alloc-segment-directory")]
        {
            let slot_idx = SegmentHeader::segment_id_at(base) as usize;
            self.clear_segment_directory(slot_idx);
        }
        Self::release_empty_segment_now(&mut SegmentMeta::new(base), base);
        self.table.recycle(base);
    }

    /// R12-6 (P1) — rare post-drain fallback for
    /// [`HeapCore::drain_heap_overflow`](crate::registry::heap_core_xthread)'s
    /// `EMPTIED_BASES_CAP`-bounded (64-entry) dedup buffer: catch any
    /// distinct segment that went fully empty via that drain's overflow-ring
    /// reclaims but did NOT fit in the buffer (the 65th+ distinct base in a
    /// single drain pass — possible only on native, where `HEAP_OVERFLOW_CAP
    /// = 2048` genuinely allows more than 64 distinct bases to empty in one
    /// call; under miri `HEAP_OVERFLOW_CAP == 64 == EMPTIED_BASES_CAP`, so
    /// this fallback is structurally never needed there).
    ///
    /// A base that overflowed the buffer is left exactly as
    /// `dec_live_and_maybe_decommit` left it: `live_count == 0`, still an
    /// ordinary registered `Small` segment, free-lists populated (so it is
    /// already reusable via `find_segment_with_free` — nothing is leaked or
    /// unreachable). What it is missing is the finalization call
    /// ([`release_or_pool_empty_segment`](Self::release_or_pool_empty_segment))
    /// that would have pooled or released it — so it sits at an inflated RSS
    /// footprint and outside the pool-cap accounting until it next happens to
    /// empty through an ordinary (non-overflow) path.
    ///
    /// This performs ONE linear sweep of every registered segment (the same
    /// index-driven `table.base_at(i)` idiom `find_segment_with_free_impl`'s
    /// linear-scan fallback and `drain_dirty_segments` already use — chosen
    /// specifically because `base_at` performs a single self-contained
    /// pointer read with no borrow of `self.table` outliving the call, so it
    /// can be freely interleaved with the `&mut self` `release_or_pool_
    /// empty_segment` call below, which can itself call `table.recycle`),
    /// finalizing every `Small` segment that is empty, not the current carve
    /// target, not already decommitted, and not already a pool member (the
    /// same eligibility test `dec_live_and_maybe_decommit` applies, plus the
    /// pool-membership check `release_or_pool_empty_segment`'s own
    /// `debug_assert!` requires — checked here explicitly, since this is an
    /// after-the-fact sweep rather than a fresh 0-transition observation).
    ///
    /// **Cost.** O(registered segments) — NOT run on every drain, only when
    /// the caller observed the dedup buffer actually overflowed (a rare tail
    /// event: it requires more than 64 DISTINCT segments to go fully empty
    /// via the second-chance overflow ring alone, in a single opportunistic
    /// drain call). The common case (buffer never overflows) pays nothing.
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    pub(crate) fn finalize_orphaned_empty_segments(&mut self, small_cur: *mut u8) {
        let n = self.table.count() as usize;
        for i in 0..n {
            let base = self.table.base_at(i);
            if base.is_null() {
                continue; // Recycled slot.
            }
            if base == small_cur {
                continue; // Current carve target — never finalized.
            }
            let meta = SegmentMeta::new(base);
            if meta.live_count_of() != 0 || meta.is_decommitted() {
                continue; // Not empty, or already released.
            }
            // Only `Small` segments are release/pool-eligible (mirrors
            // `dec_live_and_maybe_decommit`'s own PRIMORDIAL exclusion).
            if !matches!(SegmentHeader::kind_at(base), SegmentKind::Small) {
                continue;
            }
            // Already a pool member — nothing to finalize (same disjunction
            // `unpool_if_present`/`release_or_pool_empty_segment`'s
            // `debug_assert!` use: pooled iff it IS the head, or its
            // `pool_prev` is non-null).
            if self.pool_head == base || !meta.pool_prev_of().is_null() {
                continue;
            }
            self.release_or_pool_empty_segment(base);
        }
    }

    /// RAD-3 (E2, task #56) — push `base` onto the FRONT (head) of the
    /// intrusive pool list: `base` becomes the new warmest entry.
    /// Self-less (`&mut *mut u8` / `&mut usize` params rather than `&mut
    /// self`) so [`release_or_pool_empty_segment`](Self::release_or_pool_empty_segment)
    /// can call it while other `self` fields are still in scope, mirroring
    /// the existing self-less helper pattern this file already uses
    /// (`dec_live_and_maybe_decommit`, `release_empty_segment_now`).
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    fn pool_push_front(head: &mut *mut u8, tail: &mut *mut u8, count: &mut usize, base: *mut u8) {
        let mut meta = SegmentMeta::new(base);
        meta.set_pool_prev(ptr::null_mut());
        meta.set_pool_next(*head);
        if (*head).is_null() {
            // Pool was empty: `base` is both head and tail.
            *tail = base;
        } else {
            // Link the OLD head's `pool_prev` back to `base`.
            SegmentMeta::new(*head).set_pool_prev(base);
        }
        *head = base;
        *count += 1;
    }

    /// RAD-3 (E2, task #56) — unlink `base` from the intrusive pool list,
    /// given it is CURRENTLY a member (caller's contract — callers first
    /// establish membership via a head/tail/count check, exactly like the old
    /// `remove_pool_slot`'s callers located a known array index first).
    /// Patches the neighbours' links and, if `base` was the head or tail,
    /// updates `head`/`tail` accordingly. Self-less for the same reason as
    /// [`pool_push_front`](Self::pool_push_front).
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    fn pool_unlink(head: &mut *mut u8, tail: &mut *mut u8, count: &mut usize, base: *mut u8) {
        let meta = SegmentMeta::new(base);
        let prev = meta.pool_prev_of();
        let next = meta.pool_next_of();
        if prev.is_null() {
            *head = next;
        } else {
            SegmentMeta::new(prev).set_pool_next(next);
        }
        if next.is_null() {
            *tail = prev;
        } else {
            SegmentMeta::new(next).set_pool_prev(prev);
        }
        // Clear the removed segment's own links (defence-in-depth: a stale
        // link left dangling here would corrupt a LATER re-admission if this
        // segment is pooled again — `release_or_pool_empty_segment`'s
        // `pool_push_front` always sets `pool_prev`/`pool_next` fresh on
        // (re-)admission, so this reset is not load-bearing today, but it
        // keeps a not-currently-pooled segment's links at the same `null`
        // sentinel a freshly-constructed header carries, matching
        // `SegmentHeader::small`/`large`'s initial state).
        SegmentMeta::new(base).set_pool_next(ptr::null_mut());
        SegmentMeta::new(base).set_pool_prev(ptr::null_mut());
        *count -= 1;
    }

    /// Mechanism 2 (task #51) — the release-follows reset + the caller's
    /// `table.recycle` were previously inlined at each empty-observing site (as
    /// `decommit_empty_segment_for_release` + `self.table.recycle(base)`). This
    /// helper is the reset half, kept self-less so the release branch of
    /// `release_or_pool_empty_segment` and the pool-eviction path can share it.
    /// It is byte-identical to the pre-Mechanism-2 release path: it performs the
    /// release-follows fast reset (`set_bump(payload_start)` +
    /// `set_decommitted(true)`) so the intra-drain `off >= bump` stale-ring
    /// guard still fires before the whole reservation goes back to the OS.
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    fn release_empty_segment_now(meta: &mut SegmentMeta, base: *mut u8) {
        Self::decommit_empty_segment_for_release(meta, base);
    }

    /// RAD-3 (E2, task #56; formerly Mechanism 2 task #51) — pop the
    /// most-recently-pooled (HEAD, warmest) empty small segment, or `None` if
    /// the pool is empty. Used by `drain_small_pool` to walk the whole pool
    /// when releasing it (the eviction order does not matter there). Pooled
    /// segments are NOT re-inserted as carve targets: they are reused in
    /// place via `find_segment_with_free`'s free-list path (which calls
    /// `unpool_if_present`), so this pop is a pure removal primitive, not a
    /// "hand back a fresh segment" one.
    ///
    /// O(1): the head IS the warmest entry by construction (every admission
    /// pushes to the front — see [`pool_push_front`](Self::pool_push_front)),
    /// so no scan is needed (the old array version scanned ≤4 entries for the
    /// max insertion-sequence; the intrusive list makes that comparison free
    /// by maintaining the order structurally).
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    pub(super) fn pop_pooled_segment(&mut self) -> Option<*mut u8> {
        if self.pool_head.is_null() {
            debug_assert_eq!(self.pooled_count, 0, "head null but pooled_count != 0");
            return None;
        }
        let base = self.pool_head;
        Self::pool_unlink(
            &mut self.pool_head,
            &mut self.pool_tail,
            &mut self.pooled_count,
            base,
        );
        Some(base)
    }

    /// RAD-3 (E2, task #56; formerly Mechanism 2 task #51): if `base` is
    /// currently retained in the hysteresis pool, remove it (it is being
    /// reused via `find_segment_with_free`'s free-list path, so it is no
    /// longer an empty-and-idle pooled segment). Removing on reuse is what
    /// prevents a re-populated-then-re-emptied segment from being pushed into
    /// the pool a SECOND time (a double-entry → later double-recycle / a
    /// corrupt list).
    ///
    /// **O(1) membership test, no list walk.** A pooled segment always has
    /// EITHER `pool_prev_of() != null` (it is not the head) OR
    /// `pool_head == base` (it IS the head — the only pooled entry whose
    /// `pool_prev` is null). This is exhaustive: a NOT-pooled segment's
    /// `pool_prev` is always null (see `SegmentHeader::small`/`large`'s
    /// initial state and `pool_unlink`'s removal-time reset) AND it can never
    /// equal `pool_head` (the head is by definition pooled), so the
    /// disjunction is both necessary and sufficient for "is `base` pooled"
    /// without walking the list.
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    pub(super) fn unpool_if_present(&mut self, base: *mut u8) {
        let is_pooled = self.pool_head == base || !SegmentMeta::new(base).pool_prev_of().is_null();
        if is_pooled {
            Self::pool_unlink(
                &mut self.pool_head,
                &mut self.pool_tail,
                &mut self.pooled_count,
                base,
            );
        }
    }

    /// Mechanism 2 (task #51) — the small-pool decay tick. Mirrors the SHAPE of
    /// [`maybe_decay_large_cache`](Self::maybe_decay_large_cache): a fast
    /// early-exit when there is nothing to reclaim (pool empty) avoids the
    /// `Instant::now()` syscall on the overwhelmingly common path, so idle and
    /// small-only workloads that never fill the pool pay near-zero. When the
    /// pool is non-empty AND the configured interval has elapsed since the last
    /// tick, it evicts the single FIFO-OLDEST (smallest-seq, coldest) pooled
    /// segment — release-follows reset + `table.recycle`. Repeated ticks drain
    /// the pool to zero when the workload goes quiet, so pooled retention is
    /// TEMPORARY, not merely bounded.
    ///
    /// Called from [`reserve_small_segment`]'s cold path AFTER a pool miss — the
    /// natural "small churn is happening but the pool did not help this time"
    /// clock edge — and NOT on any hot alloc/free path. The trigger is chosen
    /// there rather than at the large-cache sites because a SMALL-segment
    /// workload may never call `alloc_large`, so hooking the large-path decay
    /// tick would never fire for it; `reserve_small_segment` is the cheapest
    /// small-path edge that is already cold (only reached on segment
    /// exhaustion) and is the exact place a stale pool should be trimmed.
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    pub(super) fn maybe_decay_small_pool(&mut self) {
        // Fast early-exit: nothing pooled → nothing to reclaim, skip the clock.
        if self.pooled_count == 0 {
            return;
        }
        let now = std::time::Instant::now();
        let elapsed = match self.last_pool_decay_tick {
            Some(t) => now.duration_since(t),
            None => {
                // First call: prime the timer without evicting (same anti-thrash
                // guard as the large-cache decay's first-call priming).
                self.last_pool_decay_tick = Some(now);
                return;
            }
        };
        // Reuse the large-cache decay interval as the process-wide "decay tick"
        // period — one knob governs both hysteresis buffers' idle-drain cadence.
        if elapsed < self.decay_config.decay_interval {
            return;
        }
        self.last_pool_decay_tick = Some(now);
        // Evict the FIFO-oldest (coldest) pooled segment — the list TAIL by
        // construction (every admission pushes to the HEAD, so the tail is
        // always the least-recently-pooled entry; O(1), no scan needed,
        // unlike the old array's min-seq scan).
        let base = self.pool_tail;
        debug_assert!(!base.is_null(), "pooled_count > 0 but pool_tail is null");
        Self::pool_unlink(
            &mut self.pool_head,
            &mut self.pool_tail,
            &mut self.pooled_count,
            base,
        );
        // R7-A2: clear directory bits before the slot is recycled.
        #[cfg(feature = "alloc-segment-directory")]
        {
            let slot_idx = SegmentHeader::segment_id_at(base) as usize;
            self.clear_segment_directory(slot_idx);
        }
        Self::release_empty_segment_now(&mut SegmentMeta::new(base), base);
        self.table.recycle(base);
    }

    /// TEST-ONLY (Phase 35): the process-wide count of M6 decommit invocations
    /// (`decommit_empty_segment` calls). The soak test reads this to assert the
    /// decommit hook actually fires when segments empty (the counterfactual: with
    /// the live-count proviso miswired it stays zero and the test goes red). A
    /// plain relaxed atomic — diagnostic only, no ordering obligation.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_decommit_count() -> u64 {
        DECOMMIT_CALLS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// TEST-ONLY (Phase 35): the owner-only `live_count` of `ptr`'s segment, or
    /// `None` if `ptr` is foreign / not small/primordial. Lets the soak test
    /// assert a segment reaches `live_count == 0` before decommit.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_live_count_for(&self, ptr: *mut u8) -> Option<u32> {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base_ro(base) {
            return None;
        }
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return None;
        }
        Some(SegmentMeta::new(base).live_count_of())
    }

    /// TEST-ONLY (Mechanism 2, task #51): the number of empty small segments
    /// currently retained in the hysteresis pool. Lets the
    /// `regression_c3_unbounded_recycle` test prove the retention is BOUNDED
    /// (`<= pool_cap`), and the `small_segment_pool` tests assert pool
    /// occupancy across admit/pop/evict transitions.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub fn dbg_pooled_count(&self) -> usize {
        self.pooled_count
    }

    /// TEST-ONLY (Mechanism 2, task #51; RAD-3/E2 task #56): the resolved
    /// runtime pool cap (`min(pool_segments, pool_byte_cap / SEGMENT)`; `0` =
    /// pool disabled). NO compile-time upper bound since RAD-3 — the value
    /// returned here is always the HONEST cap the caller configured, never
    /// silently clamped. Lets tests assert the config resolution.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub fn dbg_pool_cap(&self) -> usize {
        self.pool_cap
    }

    /// TEST-ONLY (Mechanism 2, task #51): forcibly DRAIN the hysteresis pool —
    /// release every pooled segment to the OS (reset + `table.recycle`) exactly
    /// as the pool-full eviction path does. Returns the number of segments
    /// drained. This is the "eventual drain" primitive the
    /// `regression_c3_unbounded_recycle` test uses to prove that a pooled
    /// segment is NOT permanently pinned: after draining the pool, every
    /// previously-pooled slot is genuinely recycled (unregistered), converging
    /// to full recycling. A production analogue (decay-tick draining) is wired
    /// into `maybe_decay_small_pool`; this seam gives tests a deterministic,
    /// sleep-free trigger.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_drain_small_pool(&mut self) -> usize {
        self.drain_small_pool()
    }

    /// Mechanism 2 (task #51): release every pooled small segment (reset +
    /// `table.recycle`), returning the count drained. Used both by the
    /// large-alloc OS-reservation-failure fallback (the pool is a reclaimable
    /// soft reserve — see `alloc_large_slow`), by the `dbg_drain_small_pool`
    /// test seam, and by the production teardown-trim path
    /// (`HeapCore::trim_for_recycle`, task #95 / N1).
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn drain_small_pool(&mut self) -> usize {
        let mut drained = 0usize;
        while let Some(base) = self.pop_pooled_segment() {
            // R7-A2: clear directory bits before the slot is recycled.
            #[cfg(feature = "alloc-segment-directory")]
            {
                let slot_idx = SegmentHeader::segment_id_at(base) as usize;
                self.clear_segment_directory(slot_idx);
            }
            Self::release_empty_segment_now(&mut SegmentMeta::new(base), base);
            self.table.recycle(base);
            drained += 1;
        }
        drained
    }

    /// TEST-ONLY (Phase 35): whether `ptr`'s segment is currently decommitted, or
    /// `None` if `ptr` is foreign / not small/primordial.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_is_decommitted_for(&self, ptr: *mut u8) -> Option<bool> {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base_ro(base) {
            return None;
        }
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
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_force_decommit_retain_for(&self, ptr: *mut u8) -> bool {
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

    /// PERF-4 (task #14): the production decommit-on-empty primitive. Every
    /// production caller that observes a
    /// segment empty (`dealloc_small`, the ring-drain in `find_segment_with_free`,
    /// `flush_run`) calls `self.table.recycle(base)` the instant decommit fires —
    /// and `recycle` returns the ENTIRE reservation to the OS
    /// (`os::release_segment` → `MEM_RELEASE` / `munmap`), which supersedes the
    /// payload `decommit_pages` call and discards every metadata page. On that
    /// path the only load-bearing action is `meta.set_bump(payload_start)`: within
    /// a single ring drain, subsequent stale ring entries for the same `base` are
    /// rejected by the `off >= bump` guard in `reclaim_offset` BEFORE they ever
    /// consult the alloc bitmap / bin table / page map (see the guard ordering in
    /// `reclaim_offset` / `dealloc_small`). Everything the full reset does beyond
    /// `set_bump` — the `os::decommit_pages` syscall on ~4 MiB of payload, zeroing
    /// 49 `BinTable` heads, re-marking ~1 KiB of page-map entries, the 32 KiB
    /// `AllocBitmap` byte-wise re-init — produces state
    /// that is unmapped microseconds later by the release. This variant elides all
    /// of it. The `set_decommitted(true)` flag is likewise unnecessary (the slot
    /// is about to be NULLed), but is kept cheap-and-harmless for semantic parity
    /// with the guard used by `dec_live_and_maybe_decommit`. See the checkpoint
    /// `docs/checkpoints/2026-07-08-perf4-decommit-churn-investigation.md`.
    #[cfg(feature = "alloc-decommit")]
    fn decommit_empty_segment_for_release(meta: &mut SegmentMeta, base: *mut u8) {
        Self::decommit_empty_segment_impl(meta, base, true);
    }

    /// Shared body of the decommit variants. `release_follows == true` means
    /// the caller recycles (releases the whole reservation to the OS) immediately
    /// after this returns, so every metadata reset except the `bump` cursor is
    /// dead work and is skipped. `release_follows == false` is the full reset that
    /// leaves the segment in the table for a future recommit-on-reuse carve.
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    fn decommit_empty_segment_impl(meta: &mut SegmentMeta, base: *mut u8, release_follows: bool) {
        // Test seam: count the invocation (diagnostic; relaxed). Counted on BOTH
        // variants so the soak / regression tests (`dbg_decommit_count`) observe
        // the same number of decommit events as before this optimization.
        DECOMMIT_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let payload_start = SegLayout::small_meta_end();
        if release_follows {
            // Release-follows fast path: the ONLY load-bearing action is resetting
            // the bump cursor so the intra-drain `off >= bump` stale-ring guard
            // still fires; the whole reservation is about to go back to the OS.
            meta.set_bump(payload_start);
            meta.set_decommitted(true);
            return;
        }
        // B3 (R7 Workstream B): lazy-commit-aware retain decommit.
        //
        // Under `small-segment-lazy-commit` (R12-9, task #260: gated on this
        // sub-feature specifically — this function is reachable ONLY for
        // `SegmentKind::Small` segments, never `Primordial`; see
        // `dec_live_and_maybe_decommit`'s "NEVER decommit the PRIMORDIAL
        // segment" guard, the sole route by which a segment reaches this
        // `release_follows == false` arm), decommit ONLY the payload pages
        // ABOVE the initial lazy chunk: `[meta_end + LAZY_FIRST_CHUNK,
        // SEGMENT)`. The initial chunk `[meta_end, meta_end +
        // LAZY_FIRST_CHUNK)` stays committed so the reused segment is
        // immediately carveable without a recommit syscall (fault-free,
        // matching a freshly reserved lazy segment). The frontier is reset
        // to `meta_end + LAZY_FIRST_CHUNK` — the same value a fresh
        // `reserve_small_segment` sets under the lazy path.
        //
        // On the eager path (feature-OFF, Unix, miri, numa-aware), the whole
        // payload `[meta_end, SEGMENT)` is decommitted as before, and the
        // frontier is not touched (it is SEGMENT throughout on the eager path).
        // This keeps the feature-OFF behaviour byte-identical.
        //
        // Metadata and the remote-free ring are NEVER decommitted: they live in
        // `[0, meta_end)`, which is entirely below the decommit range.
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
            let initial_frontier =
                SegLayout::small_decommit_start() + super::alloc_core_small::LAZY_FIRST_CHUNK;
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
        // 2a. Reset the bump cursor to the payload start (segment is blank). This
        //     is the load-bearing reset for the post-decommit stale-free guard:
        //     after this, every prior block offset in the payload is `>= bump`, so
        //     a late free / double-free / stale reclaim targeting this segment is
        //     rejected by the `off >= bump` check in `dealloc_small` /
        //     `reclaim_offset` BEFORE it writes a `next` pointer into a (now
        //     decommitted / unmapped) payload page.
        meta.set_bump(payload_start);
        // 2b. Empty every class free list.
        let mut bt = meta.bin_table();
        for c in 0..super::size_classes::SMALL_CLASS_COUNT {
            bt.set_head(c, FREE_LIST_NULL);
        }
        // 2c. Re-mark every payload page `Free` in the page map (metadata pages
        //     keep their `Meta` marking). Payload pages are `[meta_pages,
        //     PAGES_PER_SEGMENT)`.
        let mut pm = meta.page_map();
        let meta_pages = SegLayout::small_meta_pages();
        for p in meta_pages..super::segment_header::PAGES_PER_SEGMENT {
            pm.set_free(p);
        }
        // 2d. Zero the alloc bitmap (every slot "allocated / not-a-block" — the
        //     init state; with no live blocks and an empty free list this is the
        //     correct clean state). Re-init in place over the bitmap bytes.
        super::alloc_bitmap::AllocBitmap::init_in_place(Node::offset(
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
        super::magazine_bitmap::MagazineBitmap::init_in_place(Node::offset(
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
