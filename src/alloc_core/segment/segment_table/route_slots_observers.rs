//! Route-slot views of the segment table: bench-only System-allocation
//! observers and the SC-6 ingress owner hooks.

use super::route_slots::RouteSlots;
use super::SegmentTable;

impl SegmentTable {
    #[cfg(feature = "bench-internals")]
    pub(crate) fn route_slots_system_totals() -> (usize, usize, usize, usize) {
        RouteSlots::system_totals()
    }

    #[cfg(feature = "bench-internals")]
    pub(crate) fn route_slot_bytes() -> usize {
        RouteSlots::slot_bytes()
    }

    /// The route owner id, when routed.
    #[cfg(feature = "alloc-global")]
    #[cfg_attr(feature = "numa-aware", allow(dead_code))]
    pub(crate) fn route_owner(&self) -> Option<usize> {
        self.routes.as_ref().map(RouteSlots::owner)
    }

    /// SC-6: re-arm slot `i`'s small route before its full drain.
    #[cfg(feature = "alloc-global")]
    #[cfg_attr(feature = "numa-aware", allow(dead_code))]
    pub(crate) fn arm_small_route(&self, i: usize, base: *mut u8) {
        if let Some(routes) = &self.routes {
            routes.arm_small(i, base);
        }
    }
}
