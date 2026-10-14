//! Bench-only observers of the per-slot route arrays' System allocations.

use super::route_slots::RouteSlots;
use super::SegmentTable;

impl SegmentTable {
    pub(crate) fn route_slots_system_totals() -> (usize, usize, usize, usize) {
        RouteSlots::system_totals()
    }

    pub(crate) fn route_slot_bytes() -> usize {
        RouteSlots::slot_bytes()
    }

    #[cfg(all(
        feature = "alloc-global",
        feature = "alloc-xthread",
        feature = "bench-internals",
        r20_prf01_c3_oracle
    ))]
    pub(crate) fn quiescent_summary_missing_bits(
        &self,
        index: usize,
        base: *mut u8,
        start: usize,
        end: usize,
    ) -> u64 {
        self.routes.as_ref().map_or(0, |routes| {
            routes.quiescent_summary_missing_bits(index, base, start, end)
        })
    }
}
