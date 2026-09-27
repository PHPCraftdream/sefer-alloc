//! [`SegmentStateReconciliation`] — the full per-state segment-state snapshot
//! of one heap, built from [`SegmentStateAccount`](super::SegmentStateAccount)
//! records (mechanical split of the former flat `alloc_core_small_pool.rs`;
//! pure code movement, no behavior changed).

use super::segment_state_account::SegmentStateAccount;

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
    ///
    /// `pub(super)`: moved out of the same file as its caller
    /// (`dbg_segment_state_reconciliation`, now in `alloc_core_small_pool_impl.rs`)
    /// during the R2-24 mod.rs-is-reexports-only split — a sibling module
    /// needs at least `pub(super)` visibility to reach it; narrowed no
    /// further than the enclosing `alloc_core_small_pool` module.
    pub(super) fn recompute_total(&mut self) {
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
