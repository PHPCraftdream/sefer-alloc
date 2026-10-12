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
}
