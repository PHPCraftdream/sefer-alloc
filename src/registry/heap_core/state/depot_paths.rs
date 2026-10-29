//! Cold owner-side parking and reload for full magazines: the depot floor
//! and, under `alloc-decommit`, the G1-K7 overflow above it.

use crate::registry::heap_core::state::depot::{DEPOT_CLASSES, DEPOT_K};
#[cfg(feature = "alloc-decommit")]
use crate::registry::heap_core::state::depot_overflow::DepotOverflow;
use crate::registry::heap_core::state::tcache::TCACHE_CAP;
#[cfg(feature = "alloc-decommit")]
use crate::registry::heap_core::state::tcache::{PerClass, Tcache};
use crate::registry::heap_core::HeapCore;

/// G1-K7: `PerClass::limit` with the overflow open for class `c` under
/// `budget` — the floor plus the class's ring.
#[cfg(feature = "alloc-decommit")]
pub(crate) fn open_limit(c: usize, budget: usize) -> u16 {
    let k = crate::registry::heap_core::state::depot::DEPOT_CAP[c] as usize;
    if k == 0 || budget == 0 {
        return k as u16;
    }
    (k + DepotOverflow::cap_for(budget, c)) as u16
}

/// G1-K7: reopen the latched classes in `ready` (bit `c`).
#[cfg(feature = "alloc-decommit")]
pub(crate) fn reopen(tcache: &mut Tcache, mut ready: u32, budget: usize) {
    while ready != 0 {
        let c = ready.trailing_zeros() as usize;
        ready &= ready - 1;
        tcache.classes[c].limit = open_limit(c, budget);
    }
}

/// G1-K7: park `class`'s full magazine above the floor; on refusal latch its
/// limit at the current depth so the free path stops calling until the
/// overflow shrinks.
#[cfg(feature = "alloc-decommit")]
#[cold]
#[inline(never)]
fn park_overflow(
    ovf: &mut DepotOverflow,
    class: &mut PerClass,
    c: usize,
    k: usize,
    budget: usize,
) -> bool {
    let n = class.parked as usize;
    if budget != 0 && ovf.park(c, n - k, &class.slots, budget) {
        return true;
    }
    class.limit = class.parked;
    ovf.starve(c);
    false
}

/// G1-K7: reload class `c`'s newest overflow record; reopens latched classes
/// the freed bytes make room for. Returns whether the bounded remote-ingress
/// check is due.
#[cfg(feature = "alloc-decommit")]
#[cold]
#[inline(never)]
fn reload_overflow(
    ovf: &mut DepotOverflow,
    tcache: &mut Tcache,
    c: usize,
    k: usize,
    budget: usize,
) -> bool {
    let class = &mut tcache.classes[c];
    ovf.reload(c, class.parked as usize - k, &mut class.slots);
    let ingress = ovf.ingress_due();
    let ready = ovf.take_unstarved(budget);
    if ready != 0 {
        reopen(tcache, ready, budget);
    }
    ingress
}

impl HeapCore {
    /// Whether a full magazine of class `c` can park (the free path's inline
    /// gate): `parked` below the class's limit — the floor, plus its overflow
    /// ring while the overflow accepts this class.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[inline(always)]
    pub(crate) fn depot_may_park(&self, c: usize) -> bool {
        let class = &self.tcache.classes[c];
        class.parked < class.limit
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
                park_overflow(&mut depot.ovf, class, c, k, budget)
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
        #[cfg(feature = "alloc-decommit")]
        let budget = self.depot_budget as usize;
        let Some(depot) = self.depot.as_deref_mut() else {
            return false;
        };
        let n = self.tcache.classes[c].parked as usize;
        if self.tcache.classes[c].count != 0 || n == 0 {
            return false;
        }
        let k = DEPOT_K[c] as usize;
        #[cfg(feature = "alloc-decommit")]
        let mut ingress = false;
        if n > k {
            #[cfg(feature = "alloc-decommit")]
            {
                ingress = reload_overflow(&mut depot.ovf, &mut self.tcache, c, k, budget);
            }
            #[cfg(not(feature = "alloc-decommit"))]
            unreachable!("parked never exceeds DEPOT_K without the overflow");
        } else {
            depot.floor_reload(c, n, &mut self.tcache.classes[c].slots);
        }
        let class = &mut self.tcache.classes[c];
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
