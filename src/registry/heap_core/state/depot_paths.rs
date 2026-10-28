//! Cold owner-side parking and reload for full magazines: the depot floor
//! and, under `alloc-decommit`, the G1-K7 overflow above it.

use crate::registry::heap_core::state::depot::{DEPOT_CLASSES, DEPOT_K};
use crate::registry::heap_core::state::tcache::TCACHE_CAP;
use crate::registry::heap_core::HeapCore;

impl HeapCore {
    /// Whether a full magazine of class `c` can park: room in the floor, or
    /// a non-zero budget whose share for `c` still has room. The free path's
    /// inline gate (the shared byte budget is checked in `depot_park`).
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[inline(always)]
    pub(crate) fn depot_may_park(&self, c: usize) -> bool {
        let k = crate::registry::heap_core::state::depot::DEPOT_CAP[c] as usize;
        let parked = self.tcache.classes[c].parked as usize;
        if parked < k {
            return true;
        }
        #[cfg(feature = "alloc-decommit")]
        {
            use crate::registry::heap_core::state::depot_overflow::DepotOverflow;
            let budget = self.depot_budget as usize;
            k != 0 && budget != 0 && parked - k < DepotOverflow::cap_for(budget, c)
        }
        #[cfg(not(feature = "alloc-decommit"))]
        false
    }

    /// Park a full magazine: into the floor while it has room, then into the
    /// overflow within the heap's budget. `false` (no change) otherwise.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[cold]
    #[inline(never)]
    pub(crate) fn depot_park(&mut self, c: usize) -> bool {
        if c >= DEPOT_CLASSES || DEPOT_K[c] == 0 {
            return false;
        }
        #[cfg(feature = "alloc-decommit")]
        let budget = self.depot_budget as usize;
        let Some(depot) = self.depot.as_deref_mut() else {
            return false;
        };
        let class = &mut self.tcache.classes[c];
        if class.count as usize != TCACHE_CAP {
            return false;
        }
        let n = class.parked as usize;
        let k = DEPOT_K[c] as usize;
        let ok = if n < k {
            depot.floor_park(c, n, &class.slots)
        } else {
            #[cfg(feature = "alloc-decommit")]
            {
                budget != 0 && depot.ovf.park(c, n - k, &class.slots, budget)
            }
            #[cfg(not(feature = "alloc-decommit"))]
            {
                false
            }
        };
        if !ok {
            return false;
        }
        class.parked += 1;
        #[cfg(feature = "virgin-zero-skip")]
        {
            class.virgin_mask = 0;
        }
        true
    }

    /// Reload a parked full magazine — the overflow's newest first, then the
    /// floor's; its blocks remain MAG and live. No decay tick here: the refill
    /// callers count the cold event, the teardown drain must not.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[cold]
    #[inline(never)]
    pub(crate) fn depot_reload(&mut self, c: usize) -> bool {
        if c >= DEPOT_CLASSES || DEPOT_K[c] == 0 {
            return false;
        }
        let Some(depot) = self.depot.as_deref_mut() else {
            return false;
        };
        let class = &mut self.tcache.classes[c];
        let n = class.parked as usize;
        if class.count != 0 || n == 0 {
            return false;
        }
        let k = DEPOT_K[c] as usize;
        #[cfg(feature = "alloc-decommit")]
        let mut ingress = false;
        if n > k {
            #[cfg(feature = "alloc-decommit")]
            {
                depot.ovf.reload(c, n - k, &mut class.slots);
                ingress = depot.ovf.ingress_due();
            }
            #[cfg(not(feature = "alloc-decommit"))]
            unreachable!("parked never exceeds DEPOT_K without the overflow");
        } else {
            depot.floor_reload(c, n, &mut class.slots);
        }
        class.parked -= 1;
        #[cfg(feature = "virgin-zero-skip")]
        {
            class.virgin_mask = 0;
        }
        class.count = TCACHE_CAP as u8;
        // Overflow reloads skip the refill's bounded remote-ingress check;
        // run it every 8th so long reload runs keep retiring obligations.
        #[cfg(all(feature = "alloc-decommit", feature = "alloc-xthread"))]
        if ingress {
            self.drain_large_sidecar_ingress_hot_bounded();
        }
        #[cfg(all(feature = "alloc-decommit", not(feature = "alloc-xthread")))]
        let _ = ingress;
        true
    }
}
