//! Mechanism-2 empty-small-segment pool + M6 decommit cluster of [`AllocCore`]
//! (mechanical split of `alloc_core.rs`).
//!
//! This file holds an additional `impl AllocCore { .. }` block carrying the
//! empty-small-segment hysteresis pool and the decommit/live-count methods. It
//! is a pure code-movement sibling of `alloc_core.rs`; no behavior changed. The
//! whole module is `alloc-decommit`-gated because every method here is.

use core::ptr;

use crate::alloc_core::alloc_core::{AllocCore, DECOMMIT_CALLS};
// `os` is consulted here only by the `internals`-gated `dbg_live_count_for`
// accessor, and `SEGMENT` only by `dbg_segment_state_reconciliation`'s byte
// accounting (additionally `bench-internals`-gated) — gate each import to
// its consumer so plain `alloc-decommit` builds stay warning-clean.
#[cfg(feature = "internals")]
use crate::alloc_core::os;
#[cfg(all(feature = "internals", feature = "bench-internals"))]
use crate::alloc_core::os::SEGMENT;
// `SegmentHeader`/`SegmentKind` are consumed only inside this module's
// `alloc-decommit`-gated methods (the module declaration itself is
// `alloc-decommit`-gated), so they are always used whenever compiled.
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind, SegmentMeta};
// `Layout as SegLayout` is consulted here only by
// `dbg_segment_state_reconciliation`'s decommitted-retained byte accounting —
// gate it to that method's full gate so plainer configs stay warning-clean.
#[cfg(all(
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals"
))]
use crate::alloc_core::segment_header::Layout as SegLayout;

// Mechanical-split siblings of the former flat `alloc_core_small_pool.rs`.
// Both keep `cfg` gates here (not only on their items) so their compiled-file
// sets stay identical to when the code lived directly in
// `alloc_core_small_pool.rs`: `decommit` carries the decommit-cluster items
// (every one `alloc-decommit`-gated), `decomp_hooks` the measurement-only
// decomposition hooks (every one `internals` + `alloc-decommit` +
// `bench-internals`-gated — compiling it under any narrower gate would only
// leave its file-level `use` block unused).
#[cfg(feature = "alloc-decommit")]
mod decommit;
#[cfg(all(
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals"
))]
mod decomp_hooks;

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
    /// The OS COMMIT CHARGE implied by each segment's frontier/backend
    /// contract (metadata + committed payload, in virtual bytes
    /// committed) — NOT a measured RSS figure.
    pub committed_bytes: u64,
    /// Total virtual-address reservation bytes for segments in this state.
    pub reserved_bytes: u64,
}

/// R29-4 MEASUREMENT-ONLY: a full per-state reconciliation of one heap,
/// built from TWO separate enumerations: the live segment-table slots
/// (the walk classifies every non-NULL slot into exactly ONE state) and
/// the occupied large-cache slots (enumerated separately into
/// `large_cached` — a cache deposit unregisters the segment BEFORE zeroing
/// its header magic, so cached entries are never visible to the table
/// walk). `total` is the sum of all per-state accounts (including
/// `large_cached`), plus `unknown_count` segments whose kind byte decoded
/// to `Unknown`.
///
/// R2-14 (corrected identity):
/// `total.count + unknown_count + table_recycled_null_slots
/// == table_high_water + large_cached.count` — every LIVE table slot
/// (`table_high_water` minus the NULL recycled slots) is classified
/// exactly once into a table-walk state or `unknown_count`; `large_cached`
/// then adds the separately-enumerated cache entries. The pre-R2-14 claim
/// `sum(per_state.count) + unknown_count == table.count()` was FALSE: the
/// walk skips NULL slots while `SegmentTable::count()` is a HIGH-WATER
/// mark (slots ever written, including recycled holes), and cached Large
/// segments were invisible to the walk entirely.
///
/// `committed_bytes` is the OS commit charge implied by each segment's
/// frontier/backend contract (virtual bytes committed), NOT a measured
/// RSS figure.
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
    /// Snapshot of `SegmentTable::count()` at walk time: the number of
    /// slots EVER written (the table's high-water mark), including
    /// currently-NULL recycled holes. Deliberately NOT the live-segment
    /// count — the live count is `table_high_water -
    /// table_recycled_null_slots`.
    pub table_high_water: usize,
    /// NULL (recycled) slots within `0..table_high_water` — skipped by the
    /// classification walk, counted here so callers can reconcile `total`
    /// against `table_high_water` (see the struct-level corrected
    /// identity).
    pub table_recycled_null_slots: usize,
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
    pub(in crate::alloc_core) fn dec_live_batch_and_maybe_decommit(
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
    fn pool_push_front(head: &mut *mut u8, tail: &mut *mut u8, count: &mut u32, base: *mut u8) {
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
    fn pool_unlink(head: &mut *mut u8, tail: &mut *mut u8, count: &mut u32, base: *mut u8) {
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
    pub(in crate::alloc_core) fn pop_pooled_segment(&mut self) -> Option<*mut u8> {
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
    pub(in crate::alloc_core) fn unpool_if_present(&mut self, base: *mut u8) {
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
    pub(in crate::alloc_core) fn maybe_decay_small_pool(&mut self) {
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
        // Widening `as` cast (task #1998: storage narrowed to u32, accessor
        // stays usize) — always lossless: `usize` is at least as wide as
        // `u32` on every platform this crate supports.
        self.pooled_count as usize
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
        // Widening `as` cast (task #1998: storage narrowed to u32, accessor
        // stays usize) — always lossless: `usize` is at least as wide as
        // `u32` on every platform this crate supports.
        self.pool_cap as usize
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

    /// R29-4 (task #435) MEASUREMENT-ONLY: reconcile this heap's segment
    /// state via a DUAL enumeration (R2-14): (1) every LIVE (non-NULL)
    /// segment-table slot — `table.base_at(i)` for `i in
    /// 0..table.count()`, skipping NULL recycled slots — is classified
    /// into exactly ONE of seven mutually-exclusive states (see
    /// [`SegmentStateReconciliation`]); (2) every OCCUPIED slot of the
    /// per-heap combined large-cache array is enumerated separately into
    /// `large_cached`, because cached Large segments are NOT in the table
    /// (a deposit unregisters the segment first) and so can never be
    /// classified from table slots.
    ///
    /// The identity `total.count + unknown_count + table_recycled_null_slots
    /// == table_high_water + large_cached.count` holds by construction:
    /// every live table slot is classified exactly once (or counted as
    /// `unknown_count`), every skipped NULL slot is counted in
    /// `table_recycled_null_slots`, and `large_cached` accounts for the
    /// separately-enumerated cache slots. `table_high_water`
    /// (`SegmentTable::count()`) is a HIGH-WATER mark — slots ever written,
    /// including recycled holes — NOT the live-segment count.
    ///
    /// **Safety analysis (CLAUDE.md benchmark-hook rule):** this is a plain
    /// SAFE `pub fn`, NOT `unsafe fn`, because:
    /// 1. It does NOT derive a segment base from a caller-provided raw
    ///    pointer — every base comes from `self.table.base_at(i)`, the
    ///    table's OWN non-NULL slot (inherently validated by the table's
    ///    register/recycle invariant), and the cache-side reads go through
    ///    the existing safe `&self` accessors `large_cache_scan_bound()` /
    ///    `large_cache_slot_get(idx)` (`alloc_core_large_cache.rs`) — no new
    ///    unsafe, no raw-pointer parameters. This is a strictly WEAKER
    ///    access pattern than `dbg_live_count_for` /
    ///    `dbg_is_decommitted_for` (which take a caller `*mut u8` and
    ///    validate via `contains_base_ro`), both of which are already safe
    ///    `pub fn`.
    /// 2. It performs NO mutation — read-only classification.
    /// 3. The per-segment header reads (`SegmentMeta::new(base).header()`,
    ///    field-specific reads for `live_count`/`decommitted`/`pool_prev`/
    ///    the committed frontier) are the SAME seam the existing `dbg_*_for`
    ///    accessors use, on bases the table guarantees are live and mapped.
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
        // R2-14: `SegmentTable::count()` is the table's HIGH-WATER mark
        // (slots ever written, including currently-NULL recycled holes) —
        // snapshot it verbatim; it is deliberately NOT the live-segment
        // count.
        rec.table_high_water = n;
        let small_cur = self.small_cur;
        let seg_bytes = SEGMENT as u64;

        for i in 0..n {
            let base = self.table.base_at(i);
            if base.is_null() {
                // Recycled slot: below the high-water mark but no longer
                // live. Skipped by the classification walk — counted here so
                // callers can reconcile `total` against `table_high_water`
                // (see the struct-level corrected identity).
                rec.table_recycled_null_slots += 1;
                continue;
            }

            // Commit charge for this segment (metadata + committed payload):
            // the owner-only `committed_payload_end` frontier — the byte
            // offset up to which payload pages are committed, set at
            // bootstrap (`alloc_core/bootstrap.rs`) or small-segment
            // reservation (`alloc_core_small/reserve.rs`) and advanced by
            // grow-on-carve (`alloc_core_small/mod.rs`). On eager backends
            // the field is inert (never stamped, and the accessor below is
            // cfg'd out of such builds) while the backend commits the WHOLE
            // segment at reservation time, so SEGMENT is the contract value
            // there. The `small_decommitted_retained` state deliberately
            // does NOT use this value — see that branch.
            #[cfg(any(
                feature = "primordial-lazy-commit",
                feature = "small-segment-lazy-commit"
            ))]
            let frontier_committed = SegmentMeta::new(base).committed_payload_end_of() as u64;
            #[cfg(not(any(
                feature = "primordial-lazy-commit",
                feature = "small-segment-lazy-commit"
            )))]
            let frontier_committed = seg_bytes;

            let kind = SegmentHeader::kind_at(base);
            match kind {
                SegmentKind::Primordial => {
                    // Lazy-commit frontier (SEGMENT on eager backends — set
                    // at bootstrap, advanced by grow-on-carve); the whole 4
                    // MiB reservation stays reserved either way.
                    rec.primordial.count += 1;
                    rec.primordial.committed_bytes += frontier_committed;
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
                        // Payload pages returned to OS; only the committed
                        // prefix below the decommit boundary stays mapped.
                        // Task #1081 (F6 sweep): was the TIGHT
                        // `small_meta_end()` — same const-vs-runtime-page
                        // class as the decomp hooks above; it under-reported
                        // on >4 KiB-page hosts by the boundary round-up (and
                        // by the retained initial chunk under the lazy
                        // policy, whose decommit starts at
                        // `small_decommit_start() + LAZY_FIRST_CHUNK` — see
                        // `decommit_empty_segment_impl`'s frontier reset).
                        // cfg-accurate for both policy worlds. Task #1087
                        // (finding M4): this state has zero PRODUCTION
                        // callers but is test-reachable —
                        // `dbg_force_decommit_retain_for` (same gate, this
                        // file) drives exactly the retain leg, and
                        // `tests/segment_state_reconciliation_oracle.rs`
                        // pins this formula in both policy worlds (see
                        // docs/CORRECTNESS_OPEN_ITEMS.md item 74).
                        //
                        // R2-14: this branch keeps the formula below and does
                        // NOT read the frontier — on the eager path the
                        // decommit does not reset the frontier (it stays
                        // stale at SEGMENT — see `decommit.rs`), so a frontier
                        // read would over-report here; on the lazy path the
                        // retain leg resets the frontier to exactly this
                        // formula's value.
                        #[cfg(feature = "small-segment-lazy-commit")]
                        let meta_bytes = (SegLayout::small_decommit_start()
                            + crate::alloc_core::alloc_core_small::LAZY_FIRST_CHUNK)
                            as u64;
                        #[cfg(not(feature = "small-segment-lazy-commit"))]
                        let meta_bytes = SegLayout::small_decommit_start() as u64;
                        rec.small_decommitted_retained.count += 1;
                        rec.small_decommitted_retained.committed_bytes += meta_bytes;
                        rec.small_decommitted_retained.reserved_bytes += seg_bytes;
                    } else if is_pooled {
                        // Pool admission never decommits or resets metadata
                        // (`release_or_pool_empty_segment`'s doc): a pooled
                        // segment keeps its pages committed exactly as at
                        // the instant it emptied, so the frontier IS the
                        // commit charge (SEGMENT on eager backends).
                        rec.small_pooled.count += 1;
                        rec.small_pooled.committed_bytes += frontier_committed;
                        rec.small_pooled.reserved_bytes += seg_bytes;
                    } else if live > 0 || is_cur {
                        // The frontier is the live grow-on-carve commit point
                        // of an actively-carved segment (SEGMENT on eager
                        // backends).
                        rec.small_active.count += 1;
                        rec.small_active.committed_bytes += frontier_committed;
                        rec.small_active.reserved_bytes += seg_bytes;
                    } else {
                        // live == 0, not pooled, not small_cur, not decommitted
                        // — the "registered empty but not pooled" orphan state.
                        // Same frontier discipline as active/pooled (SEGMENT
                        // on eager backends).
                        rec.small_empty_orphan.count += 1;
                        rec.small_empty_orphan.committed_bytes += frontier_committed;
                        rec.small_empty_orphan.reserved_bytes += seg_bytes;
                    }
                }
                SegmentKind::Large => {
                    let hdr = SegmentMeta::new(base).header();
                    let span = hdr.span_usable as u64;
                    let res_len = hdr.reservation_len as u64;
                    // R2-14: every registered Large slot is ACTIVE. A
                    // large-cache deposit unregisters the segment BEFORE
                    // zeroing its magic (`alloc_core/mem/mod.rs` deposit
                    // path; `alloc_core/large.rs` remote reclaim path), so a
                    // cached entry is never visible to this walk — cached
                    // segments are enumerated from the per-heap cache array
                    // below, and a registered Large slot always carries
                    // SEGMENT_MAGIC (the old `hdr.magic == 0` cached branch
                    // was structurally dead).
                    rec.large_active.count += 1;
                    rec.large_active.committed_bytes += span;
                    rec.large_active.reserved_bytes += res_len;
                }
                SegmentKind::Unknown => {
                    rec.unknown_count += 1;
                }
            }
        }

        // R2-14 — second enumeration: the OCCUPIED slots of the per-heap
        // combined large-cache array (base + extension). Deposits keep the
        // pages COMMITTED (no decommit on deposit — see the deposit-site
        // comment in `alloc_core/mem/mod.rs`), so `usable_size` (the
        // carried-forward committed span) is the commit charge, and
        // `reservation_len` is the real OS reservation descriptor for
        // reserved bytes (the R12-4 `reserved_capacity` VA span is
        // deliberately NOT reported as reserved). Reads go through the
        // existing safe `&self` accessors — no new unsafe.
        let bound = self.large_cache_scan_bound();
        for idx in 0..bound {
            if let Some(entry) = self.large_cache_slot_get(idx) {
                rec.large_cached.count += 1;
                rec.large_cached.committed_bytes += entry.usable_size as u64;
                rec.large_cached.reserved_bytes += entry.reservation_len as u64;
            }
        }

        rec.recompute_total();
        rec
    }
}
