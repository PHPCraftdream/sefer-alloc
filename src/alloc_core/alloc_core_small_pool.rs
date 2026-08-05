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
#[cfg(feature = "bench-internals")]
use super::reserved_small_segment::ReservedSmallSegment;

// ---------------------------------------------------------------------------
// R29-4 (task #435) — segment-state reconciliation snapshot types.
//
// `SegmentStateAccount` and `SegmentStateReconciliation` are plain-data
// containers returned by `dbg_segment_state_reconciliation`. Defined here
// (the pool/decommit cluster) because the method that populates them lives in
// this file's `impl AllocCore` block; re-exported via `alloc_core::mod.rs` so
// `examples/` and `tests/` can name the return type. `#[doc(hidden)]` —
// measurement-only, not stable public API.
// ---------------------------------------------------------------------------

/// R29-4 MEASUREMENT-ONLY: per-state accounting for a heap's registered
/// segments (count + committed/reserved bytes).
///
/// Gated `bench-internals`: the only consumer is
/// `dbg_segment_state_reconciliation`, itself gated `alloc-decommit +
/// bench-internals` — an ungated definition here is `dead_code` under plain
/// `cargo clippy --features production -- -D warnings` (caught in the R29
/// readonly review, not by this task's own narrower verification, mirroring
/// R29-5's identical promotion-counter gap fixed in the same round).
#[doc(hidden)]
#[cfg(feature = "bench-internals")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SegmentStateAccount {
    /// Number of registered segments classified into this state.
    pub count: usize,
    /// Bytes backed by physical memory for segments in this state
    /// (metadata + committed payload).
    pub committed_bytes: u64,
    /// Total virtual-address reservation bytes for segments in this state.
    pub reserved_bytes: u64,
}

/// R29-4 MEASUREMENT-ONLY: a full per-state reconciliation of every
/// registered segment of one heap. Every non-NULL segment-table slot is
/// classified into exactly ONE state; `total` is the sum of all per-state
/// accounts (plus `unknown_count` segments whose kind byte was corrupt).
/// The identity `sum(per_state.count) + unknown_count == table.count()`
/// holds by construction (every slot classified), making the accounting
/// self-verifying — no unaccounted-for residual bucket.
#[doc(hidden)]
#[cfg(feature = "bench-internals")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SegmentStateReconciliation {
    /// The primordial segment (hosts the SegmentTable registry; one per heap).
    pub primordial: SegmentStateAccount,
    /// An empty small segment retained in the hysteresis pool.
    pub small_pooled: SegmentStateAccount,
    /// A small segment actively serving allocations (`live_count > 0`) or
    /// the current bump-carve target (`base == small_cur`).
    pub small_active: SegmentStateAccount,
    /// An empty small segment (`live_count == 0`) that is NOT pooled, NOT
    /// the current carve target, and NOT decommitted — the "registered
    /// empty but not pooled" transitional/orphan state.
    pub small_empty_orphan: SegmentStateAccount,
    /// A small segment whose payload pages have been decommitted but whose
    /// table slot is still live (the `release_follows == false` retain
    /// path — has ZERO production callers; exists only via a test hook).
    pub small_decommitted_retained: SegmentStateAccount,
    /// A large/huge segment currently serving a live allocation.
    pub large_active: SegmentStateAccount,
    /// A large/huge segment deposited into the per-heap large-object cache
    /// (freed, waiting for reuse; `magic == 0`).
    pub large_cached: SegmentStateAccount,
    /// Sum of all per-state accounts above.
    pub total: SegmentStateAccount,
    /// Segments whose `kind` byte decoded to `Unknown` (corrupt header) —
    /// should always be 0 in a well-formed heap.
    pub unknown_count: usize,
}

#[cfg(feature = "bench-internals")]
impl SegmentStateReconciliation {
    /// Recompute `total` from the per-state accounts. Called internally
    /// after classification completes.
    fn recompute_total(&mut self) {
        let states = [
            self.primordial,
            self.small_pooled,
            self.small_active,
            self.small_empty_orphan,
            self.small_decommitted_retained,
            self.large_active,
            self.large_cached,
        ];
        self.total = states
            .iter()
            .fold(SegmentStateAccount::default(), |acc, s| {
                SegmentStateAccount {
                    count: acc.count + s.count,
                    committed_bytes: acc.committed_bytes + s.committed_bytes,
                    reserved_bytes: acc.reserved_bytes + s.reserved_bytes,
                }
            });
    }
}

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

    /// The process-wide count of M6 decommit invocations
    /// (`decommit_empty_segment` calls). The soak test reads this to assert the
    /// decommit hook actually fires when segments empty (the counterfactual: with
    /// the live-count proviso miswired it stays zero and the test goes red). A
    /// plain relaxed atomic — diagnostic only, no ordering obligation.
    ///
    /// H2 (task #572): NOT `internals`-gated, unlike its `dbg_*` siblings in
    /// this file — [`SeferAlloc::stats`](crate::SeferAlloc::stats) reads this
    /// directly (`src/global/sefer_alloc.rs`'s `decommit_calls` field) to
    /// populate the public, production-reachable [`AllocStats`](crate::AllocStats)
    /// struct, so this is a real production caller, not test-only despite its
    /// name — the same exemption already applied to its three siblings
    /// `dbg_foreign_or_unroutable_frees`/`dbg_segments_reserved_total`/
    /// `dbg_segments_released_total` in `alloc_core_core_diag.rs` (Sol-F1/task
    /// #563's module doc). See
    /// `scripts/verify-alloc-core-dbg-internals-exhaustive.mjs`'s `ALLOWLIST`.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_decommit_count() -> u64 {
        DECOMMIT_CALLS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// TEST-ONLY (Phase 35): the owner-only `live_count` of `ptr`'s segment, or
    /// `None` if `ptr` is foreign / not small/primordial. Lets the soak test
    /// assert a segment reaches `live_count == 0` before decommit.
    #[cfg(feature = "internals")]
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
    #[cfg(feature = "internals")]
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
    #[cfg(feature = "internals")]
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
    #[cfg(feature = "internals")]
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
    #[cfg(feature = "internals")]
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
    ///
    /// # Safety
    ///
    /// The caller MUST guarantee the segment at `ptr`'s base has
    /// `live_count == 0` — i.e. every block previously carved from that
    /// segment has already been freed — BEFORE calling this hook. It drives
    /// `decommit_empty_segment_impl` directly, which returns the segment's
    /// PAYLOAD pages to the OS (`os::decommit_pages`), resets the bump cursor,
    /// empties every class free list, and re-zeros the alloc bitmap. This hook
    /// deliberately does NOT verify `live_count` — the very reason a real
    /// production caller would have invoked it after observing the segment go
    /// empty. Calling it on a segment that still owns LIVE allocations
    /// decommits the backing pages of those live blocks; the next access
    /// through any of them is a use-after-free / access violation. (The
    /// `contains_base_ro` / `Small`-kind checks below reject foreign or
    /// non-`Small` pointers by returning `false`, but they say nothing about
    /// `live_count`.)
    // R29-8 (task #439): `pub unsafe fn` + `bench-internals`-gated. This hook
    // resolves `ptr`'s segment base, checks `contains_base_ro` + `Small` kind,
    // then decommits the payload with NO `live_count` check — a direct instance
    // of the R25-1 (task #395) safe-`pub fn`-that-touches-allocator-state hole
    // CLAUDE.md's benchmark-hook rule targets. `AllocCore` is a crate-root
    // re-exported public type (`src/lib.rs`), so this was reachable from 100%
    // safe code under plain `--features production` (alloc-decommit) — worse
    // exposure than R29-7's `#[doc(hidden)]`-module item. NEW tier-2 site:
    // this file is NOT a tier-1 seam module (no `#![allow(unsafe_code)]`), so
    // the item-level `#[allow(unsafe_code)]` below is required; it mirrors
    // `dbg_dealloc_own_thread_with_base` / `dbg_flush_class_only` in
    // `heap_core_diag.rs`. The body forwards to the SAFE
    // `decommit_empty_segment_impl`; the `unsafe fn` signature exists solely
    // to enforce the `live_count == 0` precondition at the call site.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[allow(unsafe_code)] // R29-8: `unsafe fn` boundary (live_count==0 precondition).
    pub unsafe fn dbg_force_decommit_retain_for(&self, ptr: *mut u8) -> bool {
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
        //
        // R12-11 (task #262): `PageMap` maintenance is diagnostic-only (see
        // its struct doc) — gated behind `page-map-diag` (additionally to
        // this whole module's `alloc-decommit` gate) and elided from the
        // default/production decommit-reset path.
        #[cfg(feature = "page-map-diag")]
        {
            let mut pm = meta.page_map();
            let meta_pages = SegLayout::small_meta_pages();
            for p in meta_pages..super::segment_header::PAGES_PER_SEGMENT {
                pm.set_free(p);
            }
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

    // ── R29-3 (task #434) — segment-lifecycle decomposition hooks ─────────────
    //
    // Measurement-only hooks for decomposing one decommit→reserve segment-
    // lifecycle cycle into its component costs (wall-clock, not iai — see the
    // gate report for why Ir is blind to the kernel-time-dominated OS syscalls
    // + page faults this decomposition hinges on). Each hook calls an EXISTING
    // production function verbatim; they exist solely so a
    // `std::time::Instant`-instrumented example (`examples/r29_3_*`) can reach
    // crate-internal functions. All `bench-internals`-gated (no production
    // caller → CLAUDE.md benchmark-hook rule 2). Hooks accepting a raw pointer
    // are `pub unsafe fn` with `# Safety` (rule 1).

    /// R29-3: ONE full reserve→release cycle (`reserve_small_segment_impl` +
    /// `release_or_pool_empty_segment`) without touching the payload.
    /// Measures components (1+2+3): OS reserve+release, SegmentTable
    /// register+recycle, metadata init — everything a reservation-only
    /// overflow tier could avoid.
    ///
    /// R30-1 (task #450): routes through
    /// [`reserve_small_segment_impl`](Self::reserve_small_segment_impl) —
    /// the cursor-free half of `reserve_small_segment` — NOT
    /// `reserve_small_segment` itself. `reserve_small_segment`'s last
    /// statement publishes the freshly reserved segment as the live
    /// `self.small_cur` bump-carve cursor; this hook immediately releases
    /// that same segment via `release_or_pool_empty_segment`, which (once
    /// the hysteresis pool is full) genuinely returns the OS reservation and
    /// recycles the table slot. Going through the cursor-publishing wrapper
    /// left `small_cur` dangling at an unmapped segment with nothing to
    /// restore it — the very next ordinary small alloc on this heap would
    /// read through it (`pop_free(self.small_cur, ...)`), a use-after-free.
    /// See `docs/CORRECTNESS_OPEN_ITEMS.md` item 5 for the full confirmed
    /// trace. `reserve_small_segment_impl` performs the identical OS/table/
    /// metadata work this hook measures, but never touches `small_cur` —
    /// so this hook cannot disturb any other in-flight allocation on the
    /// heap, however many times it is called.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_full_cycle(&mut self) -> bool {
        match self.reserve_small_segment_impl() {
            Some(base) => {
                self.release_or_pool_empty_segment(base);
                true
            }
            None => false,
        }
    }

    /// R29-3: ONE raw OS reserve+release round-trip (`Segment::reserve` +
    /// `os::release_segment`) with NO table bookkeeping and NO metadata
    /// initialization. Isolates component (1): the OS-level VMA setup/teardown
    /// alone.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_os_roundtrip() -> bool {
        let seg = match os::Segment::reserve(SEGMENT) {
            Some(s) => s,
            None => return false,
        };
        let r = seg.reservation();
        let rl = seg.reservation_len();
        core::mem::forget(seg);
        os::release_segment(r.as_ptr(), rl);
        true
    }

    // ── task #504 (F11 step 2) — reserve-vs-commit SPLIT for the Windows
    // decomposition gate ────────────────────────────────────────────────────
    //
    // `dbg_decomp_os_roundtrip` above lumps reserve+commit into ONE timed
    // region — correct for R29-3's Linux question (where the eager path is
    // the only one that exists) but too coarse for F11's Windows question:
    // on Windows `win_reserve_commit` unconditionally issues TWO separate
    // syscalls (`VirtualAlloc(MEM_RESERVE)` then `VirtualAlloc(MEM_COMMIT)`),
    // and knowing their relative cost is exactly what step 2 needs. These two
    // hooks reuse `os::Segment::reserve_lazy_for_measurement` (reserve the
    // full segment, commit only 1 page up front, via `aligned_vmem::
    // reserve_aligned_lazy`) + `os::commit_pages_for_measurement` (commit the
    // REMAINING pages via `aligned_vmem::commit_range`) — the SAME crate-level
    // `lazy-commit` primitive the opt-in `primordial-lazy-commit`/
    // `small-segment-lazy-commit` POLICY features already call in production,
    // just driven directly by a measurement hook instead of by a policy
    // decision. Both are gated on `bench-internals` alone (forwarding
    // `aligned-vmem/lazy-commit`, NOT any sefer-level lazy-commit policy
    // feature — see `bench-internals`'s own `Cargo.toml` doc), so a plain
    // `production` build never partially-commits a segment via this path.
    //
    // Deliberately raw `os::Segment`-based, NOT `ReservedSmallSegment` — like
    // `dbg_decomp_os_roundtrip` above (NO table bookkeeping, NO metadata
    // init, NO owner-binding), these two hooks isolate PURE OS-level cost.
    // `ReservedSmallSegment` exists to guard against a cross-`AllocCore`
    // release once a segment participates in `self.small_cur`/pool/table
    // state (`dbg_decomp_reserve_and_keep`'s contract); these hooks never
    // publish the segment anywhere, so the caller is trusted to pair one
    // `dbg_decomp_win_reserve_only` with exactly one
    // `dbg_decomp_win_commit_only` and one `dbg_decomp_win_release_only`,
    // the same "measurement code, not production metadata" trust level
    // `dbg_decomp_os_roundtrip` already has.
    //
    // On Unix/miri, `aligned_vmem::reserve_aligned_lazy` falls back to the
    // eager fully-committed path and `commit_range` is a no-op (both crate-
    // documented) — so `dbg_decomp_win_commit_only` measures ~0 ns there,
    // which is the expected, honestly-reported cross-platform behavior, not
    // a bug: there is no separate commit syscall to time on Unix.

    /// task #504 (F11 step 2): reserve a `SEGMENT`-sized, `SEGMENT`-aligned
    /// span with only the FIRST page committed. On Windows this is exactly
    /// `VirtualAlloc(MEM_RESERVE)` (over-reserve + trim) followed by ONE
    /// `VirtualAlloc(MEM_COMMIT, len=PAGE)` — the same two-call shape
    /// `win_reserve_commit` always takes, except the commit length here is
    /// deliberately tiny so [`dbg_decomp_win_commit_only`] below can
    /// separately time committing the (large) remainder. Returns
    /// `(base, reservation_ptr, reservation_len)` — the caller MUST later
    /// release via [`dbg_decomp_win_release_only`], passing back the SAME
    /// `(reservation_ptr, reservation_len)` pair.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_win_reserve_only() -> Option<(*mut u8, *mut u8, usize)> {
        let seg = os::Segment::reserve_lazy_for_measurement(os::PAGE)?;
        let base = seg.as_ptr();
        let r = seg.reservation();
        let rl = seg.reservation_len();
        core::mem::forget(seg);
        Some((base, r.as_ptr(), rl))
    }

    /// task #504 (F11 step 2): commit the remaining `[PAGE, SEGMENT)` range
    /// of a segment previously reserved via [`dbg_decomp_win_reserve_only`]
    /// (which left only the first page committed). On Windows this is
    /// exactly ONE `VirtualAlloc(MEM_COMMIT, len=SEGMENT-PAGE)` call —
    /// isolating that syscall's cost alone, with NO reserve and NO
    /// first-touch page-fault cost mixed in (unlike
    /// [`dbg_decomp_os_roundtrip`], which lumps reserve+commit, or
    /// Measurement B's decommit/recommit/re-touch loop, which mixes commit
    /// with faulting). On Unix/miri this is a documented no-op
    /// (`aligned_vmem::commit_range`'s own fallback) — expected to measure
    /// ~0 ns there, honestly reflecting that Unix has no separate commit
    /// syscall to pay.
    ///
    /// Returns `true` if the range is now committed, `false` on genuine OS
    /// refusal (commit-charge exhaustion).
    ///
    /// # Safety
    ///
    /// `base` MUST be the `base` returned by a [`dbg_decomp_win_reserve_only`]
    /// call whose `[PAGE, SEGMENT)` range is still uncommitted (not yet
    /// committed by a prior call to this same hook), and must not have been
    /// released yet.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    #[allow(unsafe_code)] // task #504: unsafe fn boundary, mirrors dbg_decomp_recommit_payload.
    pub unsafe fn dbg_decomp_win_commit_only(base: *mut u8) -> bool {
        // SAFETY: forwarded from this function's own `# Safety` contract —
        // `base`'s `[PAGE, SEGMENT)` range is within the live reservation and
        // currently reserved-but-uncommitted, matching `commit_pages_for_
        // measurement`'s own contract.
        unsafe { os::commit_pages_for_measurement(base, os::PAGE, SEGMENT) }
    }

    /// task #504 (F11 step 2): release a segment reserved via
    /// [`dbg_decomp_win_reserve_only`] — thin wrapper over
    /// [`os::release_segment`], mirroring [`dbg_decomp_os_roundtrip`]'s own
    /// release call.
    ///
    /// # Safety
    ///
    /// `(reservation_ptr, reservation_len)` MUST be the pair returned by a
    /// [`dbg_decomp_win_reserve_only`] call not yet released.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[allow(unsafe_code)] // task #504: unsafe fn boundary, forwarded contract.
    pub unsafe fn dbg_decomp_win_release_only(reservation_ptr: *mut u8, reservation_len: usize) {
        os::release_segment(reservation_ptr, reservation_len);
    }

    /// R29-3: reserve a small segment and return a typed handle so the
    /// caller can measure first-touch page-fault cost on the payload. The
    /// caller MUST later release it via
    /// [`dbg_decomp_release`](Self::dbg_decomp_release).
    ///
    /// R30-1 (task #450): routes through
    /// [`reserve_small_segment_impl`](Self::reserve_small_segment_impl),
    /// NOT `reserve_small_segment` — same reasoning as
    /// [`dbg_decomp_full_cycle`](Self::dbg_decomp_full_cycle)'s doc comment.
    /// This hook's own paired release
    /// ([`dbg_decomp_release`](Self::dbg_decomp_release)) can genuinely
    /// release the OS reservation; a version of this hook that published
    /// `self.small_cur` first would leave it dangling with no restore point
    /// once the paired release fires.
    ///
    /// R31-4 (task #467): returns [`ReservedSmallSegment`] instead of a
    /// bare `*mut u8` — see that type's module doc
    /// (`reserved_small_segment.rs`) for why. Same underlying reservation
    /// mechanism as before; only the return type changed.
    ///
    /// R31-15 (task #486): the returned handle is now stamped with THIS
    /// `AllocCore`'s `dbg_reservation_owner_id`, so the paired
    /// [`dbg_decomp_release`](Self::dbg_decomp_release) can reject a
    /// cross-core release. See `reserved_small_segment.rs`'s module doc,
    /// "Owner-binding" section, for the full rationale.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_reserve_and_keep(&mut self) -> Option<ReservedSmallSegment> {
        let owner_id = self.dbg_reservation_owner_id;
        self.reserve_small_segment_impl()
            .map(|base| ReservedSmallSegment::new_from_reservation(base, owner_id))
    }

    /// R29-3: release a previously-reserved small segment.
    ///
    /// R31-4 (task #467): takes [`ReservedSmallSegment`] BY VALUE instead of
    /// a bare `*mut u8` — the handle can only have been produced by
    /// [`dbg_decomp_reserve_and_keep`](Self::dbg_decomp_reserve_and_keep) on
    /// SOME `AllocCore` (private field + `pub(super)` constructor forecloses
    /// forging one), and consuming it here by value makes a second release
    /// of the SAME handle a compile error (E0382, use of moved value)
    /// instead of an unchecked runtime hazard.
    ///
    /// R31-15 (task #486, CONFIRMED P0 soundness defect): R31-4 closed
    /// unforgeability and double-release but NOT owner-binding — until this
    /// fix, this was a **safe** `pub fn`, and nothing stopped a caller from
    /// reserving a handle on one `AllocCore` and releasing it on a
    /// DIFFERENT `AllocCore`, mutating the wrong heap's pool/directory/
    /// `SegmentTable` state for a segment it never registered while the
    /// true owner's registration of that same base went stale. Fixed two
    /// ways, layered (see `reserved_small_segment.rs`'s module doc,
    /// "Owner-binding" section, for the full writeup):
    ///
    /// 1. A release-build (non-`debug_assert!`) owner-id check below,
    ///    rejecting a cross-core handle before it ever reaches
    ///    `release_or_pool_empty_segment`.
    /// 2. `unsafe fn` — defence-in-depth for preconditions the owner-id
    ///    check cannot see (the segment must still be live/unreleased on
    ///    the same core that reserved it), matching the established
    ///    `unsafe fn` + `# Safety` pattern this crate uses for every other
    ///    hook of this shape (e.g. `HeapCore::dbg_dealloc_own_thread_with_base`).
    ///
    /// # Safety
    ///
    /// `handle` MUST have been produced by a paired
    /// [`dbg_decomp_reserve_and_keep`](Self::dbg_decomp_reserve_and_keep)
    /// call on THIS SAME `AllocCore` (not merely THE SAME logical owner
    /// under an address-based check — this is enforced structurally by the
    /// owner-id check below, which panics on mismatch even in `--release`),
    /// and the reserved segment must still be live/unreleased (not already
    /// released, unregistered, or otherwise invalidated by another `dbg_*`
    /// hook in the interim).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[allow(unsafe_code)] // R31-15: unsafe fn boundary, mirrors dbg_decomp_decommit_payload.
    pub unsafe fn dbg_decomp_release(&mut self, handle: ReservedSmallSegment) {
        // R31-15 (task #486): PRIMARY guard against a cross-core release —
        // a release-build assert (NOT debug_assert!), because this is
        // exactly the "safe-looking but touches a foreign heap's metadata"
        // hazard CLAUDE.md's benchmark-hook rule targets; a check compiled
        // out in --release would defeat the whole point of adding it.
        //
        // Ordering note: `owner_id` is read out and `into_base()` is called
        // to disarm `ReservedSmallSegment`'s leak-detecting `Drop` impl
        // BEFORE the `assert_eq!` below runs — asserting first, while
        // `handle` is still a live local with its `Drop` impl armed, would
        // unwind straight through `handle`'s own scope, firing its
        // `debug_assert!(false, "...dropped without going through
        // release...")` DURING that unwind — a panic-while-panicking, which
        // Rust aborts on unconditionally (not `--release`-specific), taking
        // down the whole process (observed as a raw
        // STATUS_STACK_BUFFER_OVERRUN abort on Windows) instead of
        // propagating a single clean panic. `into_base()` only extracts the
        // raw pointer value and disarms `Drop` — it does NOT touch any
        // allocator metadata itself, so calling it before the check is safe
        // even on the mismatch path: `base` is then discarded by the
        // `assert_eq!` panic below, WITHOUT `self.release_or_pool_empty_segment`
        // (the actual pool/directory/`SegmentTable` mutation) ever running —
        // the true owner's registration of that base is left exactly as it
        // was, untouched by this call.
        let owner_id = handle.owner_id();
        let base = handle.into_base();
        assert_eq!(
            owner_id, self.dbg_reservation_owner_id,
            "dbg_decomp_release: handle was reserved by a DIFFERENT AllocCore (owner_id \
             mismatch) — releasing it here would mutate the wrong heap's pool/directory/\
             SegmentTable state for a segment this AllocCore never registered"
        );
        // Defence-in-depth (R30-1): releasing the segment the live cursor
        // currently points at would immediately dangle `small_cur`, exactly
        // the hazard this task fixed. Not reachable today (the paired
        // `dbg_decomp_reserve_and_keep` never publishes its result as
        // `small_cur`), but cheap to assert locally rather than rely solely
        // on that non-local invariant holding forever. This is now
        // secondary defence-in-depth, not the primary guard — the primary
        // guard against double-release is the move-consuming signature
        // above (a compile error, not a runtime check).
        debug_assert!(
            base != self.small_cur,
            "dbg_decomp_release: base is the live small_cur cursor — release would dangle it"
        );
        self.release_or_pool_empty_segment(base);
    }

    /// R29-3: decommit (`MADV_DONTNEED`) the payload pages of a live segment,
    /// simulating the decommit a reservation-only tier would perform. After
    /// this call, touching the payload re-faults the pages (the irreducible
    /// recommit+first-touch cost the reservation-only design still pays).
    ///
    /// # Safety
    ///
    /// `base` MUST be a live segment base whose payload is fully committed.
    /// The payload pages are returned to the OS; any live data is discarded.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[allow(unsafe_code)] // R29-3: unsafe fn boundary (raw-pointer precondition).
    pub unsafe fn dbg_decomp_decommit_payload(base: *mut u8) {
        let payload_start = SegLayout::small_meta_end();
        os::decommit_pages(base, payload_start, SEGMENT);
    }

    /// R31-6 (task #469): re-commit the payload pages of a segment previously
    /// decommitted via [`dbg_decomp_decommit_payload`](Self::dbg_decomp_decommit_payload) —
    /// thin wrapper over [`os::recommit_pages`]. On Windows this is a REAL
    /// `VirtualAlloc(MEM_COMMIT)` (Windows `MEM_DECOMMIT` actually unmaps the
    /// backing pages, unlike POSIX `MADV_DONTNEED`, which leaves the mapping
    /// intact and merely drops the physical backing — re-access is implicitly
    /// safe on Unix); on Unix/miri it is a documented no-op (`os::
    /// recommit_pages` / `aligned_vmem::recommit` already fall back that way).
    /// A caller measuring the re-fault cost after a decommit MUST call this
    /// first on every platform — omitting it is exactly the bug this hook
    /// closes (`examples/r29_3_decomposition_gate.rs`'s Measurement B used to
    /// `write_volatile` straight into the just-decommitted range with no
    /// intervening recommit, which crashes on Windows because the pages are
    /// genuinely unmapped there).
    ///
    /// Returns `true` if the range is now committed (writes are safe),
    /// `false` on genuine OS refusal (commit-charge exhaustion) — the caller
    /// MUST NOT write into the range on `false`.
    ///
    /// # Safety
    ///
    /// `base` MUST be a live segment base whose payload was previously
    /// decommitted via [`dbg_decomp_decommit_payload`](Self::dbg_decomp_decommit_payload)
    /// (or was never committed at all — recommit is idempotent on an
    /// already-committed range on every backend this crate supports).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    #[allow(unsafe_code)] // R31-6: unsafe fn boundary, mirrors dbg_decomp_decommit_payload.
    pub unsafe fn dbg_decomp_recommit_payload(base: *mut u8) -> bool {
        let payload_start = SegLayout::small_meta_end();
        os::recommit_pages(base, payload_start, SEGMENT)
    }

    /// R29-3: the `[payload_start, payload_end)` byte range of a small
    /// segment's payload (`[small_meta_end(), SEGMENT)`).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_payload_range() -> (usize, usize) {
        (SegLayout::small_meta_end(), SEGMENT)
    }

    /// R29-3: the OS page size.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_page_size() -> usize {
        os::PAGE
    }

    /// R29-4 (task #435) MEASUREMENT-ONLY: reconcile every registered segment
    /// of this heap into exactly ONE state, with committed/reserved bytes
    /// totalled per state. Iterates every non-NULL segment-table slot
    /// (`table.base_at(i)` for `i in 0..table.count()`), reads each segment's
    /// header, and classifies it into one of seven mutually-exclusive states
    /// (see [`SegmentStateReconciliation`]). The identity
    /// `sum(per_state.count) + unknown_count == table.count()` holds by
    /// construction: every slot is classified, none is skipped.
    ///
    /// **Safety analysis (CLAUDE.md benchmark-hook rule):** this is a plain
    /// SAFE `pub fn`, NOT `unsafe fn`, because:
    /// 1. It does NOT derive a segment base from a caller-provided raw
    ///    pointer — every base comes from `self.table.base_at(i)`, the
    ///    table's OWN non-NULL slot (inherently validated by the table's
    ///    register/recycle invariant). This is a strictly WEAKER access
    ///    pattern than `dbg_live_count_for` / `dbg_is_decommitted_for`
    ///    (which take a caller `*mut u8` and validate via
    ///    `contains_base_ro`), both of which are already safe `pub fn`.
    /// 2. It performs NO mutation — read-only classification.
    /// 3. The per-segment header reads (`SegmentMeta::new(base).header()`,
    ///    field-specific reads for `live_count`/`decommitted`/`pool_prev`)
    ///    are the SAME seam the existing `dbg_*_for` accessors use, on bases
    ///    the table guarantees are live and mapped.
    ///
    /// `bench-internals`-gated (rule 2: no production caller). The
    /// `alloc-decommit` gate is inherited from this file's module-level gate
    /// (every method here is decommit-specific).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_segment_state_reconciliation(&self) -> SegmentStateReconciliation {
        let mut rec = SegmentStateReconciliation::default();
        let n = self.table.count() as usize;
        let small_cur = self.small_cur;
        let seg_bytes = SEGMENT as u64;

        for i in 0..n {
            let base = self.table.base_at(i);
            if base.is_null() {
                continue; // Recycled slot — not counted.
            }

            let kind = SegmentHeader::kind_at(base);
            match kind {
                SegmentKind::Primordial => {
                    rec.primordial.count += 1;
                    rec.primordial.committed_bytes += seg_bytes;
                    rec.primordial.reserved_bytes += seg_bytes;
                }
                SegmentKind::Small => {
                    let meta = SegmentMeta::new(base);
                    let live = meta.live_count_of();
                    let decommitted = meta.is_decommitted();
                    // Pool membership test (same disjunction `unpool_if_present`
                    // / `release_or_pool_empty_segment` use): pooled iff it IS
                    // the head, OR its `pool_prev` is non-null.
                    let is_pooled = self.pool_head == base || !meta.pool_prev_of().is_null();
                    let is_cur = base == small_cur;

                    if decommitted {
                        // Payload pages returned to OS; only metadata region
                        // stays committed.
                        let meta_bytes = SegLayout::small_meta_end() as u64;
                        rec.small_decommitted_retained.count += 1;
                        rec.small_decommitted_retained.committed_bytes += meta_bytes;
                        rec.small_decommitted_retained.reserved_bytes += seg_bytes;
                    } else if is_pooled {
                        rec.small_pooled.count += 1;
                        rec.small_pooled.committed_bytes += seg_bytes;
                        rec.small_pooled.reserved_bytes += seg_bytes;
                    } else if live > 0 || is_cur {
                        rec.small_active.count += 1;
                        rec.small_active.committed_bytes += seg_bytes;
                        rec.small_active.reserved_bytes += seg_bytes;
                    } else {
                        // live == 0, not pooled, not small_cur, not decommitted
                        // — the "registered empty but not pooled" orphan state.
                        rec.small_empty_orphan.count += 1;
                        rec.small_empty_orphan.committed_bytes += seg_bytes;
                        rec.small_empty_orphan.reserved_bytes += seg_bytes;
                    }
                }
                SegmentKind::Large => {
                    let hdr = SegmentMeta::new(base).header();
                    let span = hdr.span_usable as u64;
                    let res_len = hdr.reservation_len as u64;
                    // A cached Large segment has its `magic` field atomically
                    // zeroed at deposit (`alloc_core.rs` large-cache deposit
                    // path); an active Large segment retains `SEGMENT_MAGIC`.
                    let is_cached = hdr.magic == 0;
                    if is_cached {
                        rec.large_cached.count += 1;
                        rec.large_cached.committed_bytes += span;
                        rec.large_cached.reserved_bytes += res_len;
                    } else {
                        rec.large_active.count += 1;
                        rec.large_active.committed_bytes += span;
                        rec.large_active.reserved_bytes += res_len;
                    }
                }
                SegmentKind::Unknown => {
                    rec.unknown_count += 1;
                }
            }
        }

        rec.recompute_total();
        rec
    }
}
