//! Cold owner-side parking and reload for full magazines: the depot floor
//! (K5's path, unchanged) and, under `alloc-decommit`, the G1-K7 overflow
//! above it, reached only once the floor is full (park) or empty (reload).

use crate::registry::heap_core::state::depot::{DEPOT_CLASSES, DEPOT_K};
#[cfg(feature = "alloc-decommit")]
use crate::registry::heap_core::state::depot_overflow::DepotOverflow;
use crate::registry::heap_core::state::tcache::TCACHE_CAP;
#[cfg(feature = "alloc-decommit")]
use crate::registry::heap_core::state::tcache::{Tcache, OVF_HINT, OVF_TICK};
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

impl HeapCore {
    /// Whether a full magazine of class `c` can park (the free path's inline
    /// gate): the floor has room, or the class's overflow is open.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[inline(always)]
    pub(crate) fn depot_may_park(&self, c: usize) -> bool {
        let class = &self.tcache.classes[c];
        (class.parked as u16) < class.limit
    }

    /// Whether class `c` has a parked magazine to reload (the refill path's
    /// inline gate): its floor, or (G1-K7) its overflow ring's hint.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[inline(always)]
    pub(crate) fn depot_may_reload(&self, c: usize) -> bool {
        let class = &self.tcache.classes[c];
        // `parked` and `ovf` are adjacent bytes: one 16-bit load; the tick
        // flag is masked out.
        #[cfg(feature = "alloc-decommit")]
        {
            u16::from_le_bytes([class.parked, class.ovf]) & (0x00FF | (OVF_HINT as u16) << 8) != 0
        }
        #[cfg(not(feature = "alloc-decommit"))]
        {
            class.parked != 0
        }
    }

    /// G1-K7: whether a segment refill of class `c` counts a decay event
    /// (the refill path's inline gate; the byte sits next to `parked`, which
    /// the refill has just read).
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "alloc-decommit"
    ))]
    #[inline(always)]
    pub(crate) fn depot_may_tick(&self, c: usize) -> bool {
        self.tcache.classes[c].ovf & OVF_TICK != 0
    }

    /// G1-K7: a segment refill of armed class `c` — one decay event while
    /// the overflow holds bytes; once it is empty, `c` drops its tick flag
    /// until the next overflow park re-arms it.
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "alloc-decommit"
    ))]
    #[cold]
    #[inline(never)]
    pub(crate) fn depot_segment_tick(&mut self, c: usize) {
        let Some(depot) = self.depot.as_deref_mut() else {
            self.tcache.classes[c].ovf &= !OVF_TICK;
            return;
        };
        if depot.ovf.bytes() == 0 {
            depot.ovf.disarm(c);
            self.tcache.classes[c].ovf &= !OVF_TICK;
            return;
        }
        self.depot_tick();
    }

    /// Park a full magazine for a class with configured depot capacity: the
    /// floor first; once it is full, the G1-K7 overflow within the heap's
    /// budget. `false` (no change) otherwise.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[cold]
    #[inline(never)]
    pub(crate) fn depot_park(&mut self, c: usize) -> bool {
        if c >= DEPOT_CLASSES || DEPOT_K[c] == 0 {
            return false;
        }
        let Some(depot) = self.depot.as_deref_mut() else {
            return false;
        };
        let class = &mut self.tcache.classes[c];
        if class.count as usize != TCACHE_CAP {
            return false;
        }
        if !depot.depot_park(c, &mut class.parked, &class.slots) {
            #[cfg(feature = "alloc-decommit")]
            return self.park_overflow(c);
            #[cfg(not(feature = "alloc-decommit"))]
            return false;
        }
        #[cfg(feature = "alloc-decommit")]
        {
            class.floor_mark = 1;
        }
        #[cfg(feature = "virgin-zero-skip")]
        {
            class.virgin_mask = 0;
        }
        true
    }

    /// Reload a parked full magazine — the floor's newest first, then the
    /// overflow's newest; its blocks remain MAG and live. Counts no decay
    /// tick: the teardown drain uses it ([`Self::depot_reload_tick`] is the
    /// refill's).
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
        if class.count != 0 || !depot.depot_reload(c, &mut class.parked, &mut class.slots) {
            #[cfg(feature = "alloc-decommit")]
            if class.count == 0 {
                return self.reload_overflow(c);
            }
            return false;
        }
        #[cfg(feature = "virgin-zero-skip")]
        {
            class.virgin_mask = 0;
        }
        class.count = TCACHE_CAP as u8;
        true
    }

    /// The refill's reload: [`Self::depot_reload`], except that an overflow
    /// reload counts one cold event for the overflow decay.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[cold]
    #[inline(never)]
    pub(crate) fn depot_reload_tick(&mut self, c: usize) -> bool {
        if c >= DEPOT_CLASSES || DEPOT_K[c] == 0 {
            return false;
        }
        let Some(depot) = self.depot.as_deref_mut() else {
            return false;
        };
        let class = &mut self.tcache.classes[c];
        if class.count != 0 || !depot.depot_reload(c, &mut class.parked, &mut class.slots) {
            #[cfg(feature = "alloc-decommit")]
            if class.count == 0 {
                return self.reload_overflow_tick(c);
            }
            return false;
        }
        #[cfg(feature = "virgin-zero-skip")]
        {
            class.virgin_mask = 0;
        }
        class.count = TCACHE_CAP as u8;
        true
    }

    /// G1-K7: park class `c`'s full magazine above its full floor. The ring
    /// takes the record, the magazine is then empty (`count = 0`; the free
    /// path starts the next one), and only after that the decay counts the
    /// event — it touches long-parked records, never this one. The park arms
    /// the segment-refill tick of every class not yet armed. On refusal, or
    /// once the ring is full, the class's limit latches at the floor so the
    /// free path stops calling until the overflow shrinks.
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "alloc-decommit"
    ))]
    #[cold]
    #[inline(never)]
    fn park_overflow(&mut self, c: usize) -> bool {
        let budget = self.depot_budget as usize;
        let Some(depot) = self.depot.as_deref_mut() else {
            return false;
        };
        let ovf = &mut depot.ovf;
        let class = &mut self.tcache.classes[c];
        debug_assert_eq!(class.parked, DEPOT_K[c], "the floor fills first");
        debug_assert_eq!(class.count as usize, TCACHE_CAP);
        let n = ovf.len(c);
        if budget == 0 || !ovf.park(c, n, &class.slots, budget) {
            class.limit = class.parked as u16;
            ovf.starve(c);
            return false;
        }
        class.ovf |= OVF_HINT;
        if n + 1 >= ovf.cap(c) {
            class.limit = class.parked as u16;
            ovf.starve(c);
        }
        class.count = 0;
        #[cfg(feature = "virgin-zero-skip")]
        {
            class.virgin_mask = 0;
        }
        let mut fresh = ovf.arm();
        while fresh != 0 {
            let k = fresh.trailing_zeros() as usize;
            fresh &= fresh - 1;
            self.tcache.classes[k].ovf |= OVF_TICK;
        }
        self.depot_tick();
        true
    }

    /// G1-K7: reload class `c`'s newest overflow record into its empty
    /// magazine (the floor is empty). The ring length decides, not the
    /// `PerClass::ovf` hint, which is re-synced here. Reopens latched classes
    /// the freed bytes make room for; every 8th reload runs the bounded
    /// remote-ingress check the refill otherwise skips. Counts no decay tick.
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "alloc-decommit"
    ))]
    #[cold]
    #[inline(never)]
    fn reload_overflow(&mut self, c: usize) -> bool {
        let budget = self.depot_budget as usize;
        let Some(depot) = self.depot.as_deref_mut() else {
            return false;
        };
        let ovf = &mut depot.ovf;
        let class = &mut self.tcache.classes[c];
        let n = ovf.len(c);
        if n == 0 {
            class.ovf &= !OVF_HINT;
            return false;
        }
        debug_assert!(
            class.parked == 0 && class.count == 0,
            "the floor empties first"
        );
        ovf.reload(c, n, &mut class.slots);
        class.ovf = class.ovf & !OVF_HINT | if n > 1 { OVF_HINT } else { 0 };
        #[cfg(feature = "virgin-zero-skip")]
        {
            class.virgin_mask = 0;
        }
        class.count = TCACHE_CAP as u8;
        let ingress = ovf.ingress_due();
        let ready = ovf.take_unstarved(budget);
        if ready != 0 {
            reopen(&mut self.tcache, ready, budget);
        }
        #[cfg(feature = "alloc-xthread")]
        if ingress {
            self.drain_large_sidecar_ingress_hot_bounded();
        }
        #[cfg(not(feature = "alloc-xthread"))]
        let _ = ingress;
        true
    }

    /// G1-K7: [`Self::reload_overflow`] for the refill — the reload, then one
    /// cold event for the overflow decay (the magazine is already live).
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "alloc-decommit"
    ))]
    #[cold]
    #[inline(never)]
    fn reload_overflow_tick(&mut self, c: usize) -> bool {
        if !self.reload_overflow(c) {
            return false;
        }
        self.depot_tick();
        true
    }
}
