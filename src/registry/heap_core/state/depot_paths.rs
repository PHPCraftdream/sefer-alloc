//! Cold owner-side parking and reload for full magazines.

use crate::registry::heap_core::state::depot::{DEPOT_CLASSES, DEPOT_K};
use crate::registry::heap_core::state::tcache::TCACHE_CAP;
use crate::registry::heap_core::HeapCore;

impl HeapCore {
    /// Park a full magazine for a class with configured depot capacity.
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
        if !depot.depot_park(c, &class.slots) {
            return false;
        }
        #[cfg(feature = "virgin-zero-skip")]
        {
            class.virgin_mask = 0;
        }
        true
    }

    /// Reload a parked full magazine; its blocks remain MAG and live.
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
        if class.count != 0 || !depot.depot_reload(c, &mut class.slots) {
            return false;
        }
        #[cfg(feature = "virgin-zero-skip")]
        {
            class.virgin_mask = 0;
        }
        class.count = TCACHE_CAP as u8;
        true
    }
}
