//! Per-slot cold magazine depot storage, separate from `HeapCore`.

use crate::alloc_core::size_classes::{SIZE_CLASS_TABLE, SMALL_CLASS_COUNT};
use crate::registry::heap_core::state::tcache::TCACHE_CAP;

// G1-K5: classes up to 1024 B have a depot. 16..192 B keep the B1 K;
// 240..1024 B get K = 2 (two magazines covered the measured cross-class
// churn ceiling, docs/perf/G1_K5_DEPOT_GATE.md).
const fn depot_classes() -> usize {
    let mut n = 0;
    while n < SIZE_CLASS_TABLE.len() && SIZE_CLASS_TABLE[n] <= 1024 {
        n += 1;
    }
    n
}
pub(crate) const DEPOT_CLASSES: usize = depot_classes();
pub(crate) const DEPOT_K_MAX: u8 = 8;
pub(crate) const DEPOT_K: [u8; DEPOT_CLASSES] = {
    let mut k = [2u8; DEPOT_CLASSES];
    let small = [8u8, 8, 8, 8, 8, 8, 7, 1];
    let mut c = 0;
    while c < small.len() {
        k[c] = small[c];
        c += 1;
    }
    k
};
pub(crate) const DEPOT_BUDGET_BYTES: usize = 240 * 1024;
/// `DEPOT_K` widened to every small class (0 = no depot), so the inline
/// overflow gate needs no separate `c < DEPOT_CLASSES` test.
pub(crate) const DEPOT_CAP: [u8; SMALL_CLASS_COUNT] = {
    let mut t = [0u8; SMALL_CLASS_COUNT];
    let mut c = 0;
    while c < DEPOT_CLASSES {
        t[c] = DEPOT_K[c];
        c += 1;
    }
    t
};

const fn depot_bytes() -> usize {
    let mut total = 0;
    let mut c = 0;
    while c < DEPOT_CLASSES {
        total += SIZE_CLASS_TABLE[c] * TCACHE_CAP * DEPOT_K[c] as usize;
        c += 1;
    }
    total
}

const _: () = assert!(DEPOT_CLASSES <= SIZE_CLASS_TABLE.len());
const _: () = assert!(SIZE_CLASS_TABLE[DEPOT_CLASSES - 1] == 1024);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[0] == 16);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[1] == 32);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[2] == 48);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[3] == 64);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[4] == 80);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[5] == 112);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[6] == 144);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[7] == 192);
const _: () = assert!(depot_bytes() <= DEPOT_BUDGET_BYTES);
const _: () = assert!(depot_bytes() == 241_408);
// Every depot class parks full magazines: its K fits the store and its free
// park cap is the whole magazine.
const _: () = {
    let mut c = 0;
    while c < DEPOT_CLASSES {
        assert!(DEPOT_K[c] <= DEPOT_K_MAX);
        assert!(crate::registry::heap_core::state::tcache::FREE_PARK_CAP[c] == TCACHE_CAP as u8);
        c += 1;
    }
};

/// Classes `0..8` map to sizes 16, 32, 48, 64, 80, 112, 144, and 192 B with
/// K `[8,8,8,8,8,8,7,1]` (64,256 B of parked blocks); classes `8..18`
/// (240..1024 B) add K = 2 (177,152 B). Total 241,408 B, within the
/// 245,760 B depot budget — the depot FLOOR. Under G1-K7 the overflow
/// ([`DepotOverflow`](crate::registry::heap_core::state::depot_overflow::DepotOverflow))
/// parks further full magazines above it within the heap's budget.
///
/// Only the pointer arrays live here; each class's parked count (floor plus
/// overflow) is `PerClass::parked` in the owner's magazine line (DP-5
/// iteration 2), so the hot-path gates read it without touching this store.
/// All-zero is the valid empty state (the registry slot is OS-zeroed).
pub(crate) struct Depot {
    mags: [[[*mut u8; TCACHE_CAP]; DEPOT_K_MAX as usize]; DEPOT_CLASSES],
    #[cfg(feature = "alloc-decommit")]
    pub(crate) ovf: crate::registry::heap_core::state::depot_overflow::DepotOverflow,
}

// The overflow budget ceiling keeps `parked: u16` in range: floor plus a full
// overflow ring of 16 B magazines.
#[cfg(feature = "alloc-decommit")]
const _: () = assert!(
    DEPOT_K_MAX as usize
        + crate::alloc_core::large_cache_config::DEPOT_BUDGET_MAX
            / crate::alloc_core::large_cache_config::DEPOT_BUDGET_GRAIN
        <= u16::MAX as usize
);
#[cfg(feature = "alloc-decommit")]
const _: () = assert!(
    crate::alloc_core::large_cache_config::DEPOT_BUDGET_GRAIN == SIZE_CLASS_TABLE[0] * TCACHE_CAP
);

impl Depot {
    /// Kani-only: the all-zero empty depot an OS-zeroed registry slot holds.
    #[cfg(kani)]
    pub(crate) const fn empty() -> Self {
        Self {
            mags: [[[core::ptr::null_mut(); TCACHE_CAP]; DEPOT_K_MAX as usize]; DEPOT_CLASSES],
            #[cfg(feature = "alloc-decommit")]
            ovf: crate::registry::heap_core::state::depot_overflow::DepotOverflow::empty(),
        }
    }

    /// Park `magazine` as class `c`'s floor entry `n` (the class's current
    /// floor count). Fails (no change) when the floor already holds
    /// `DEPOT_K[c]`.
    #[inline]
    pub(crate) fn floor_park(
        &mut self,
        c: usize,
        n: usize,
        magazine: &[*mut u8; TCACHE_CAP],
    ) -> bool {
        if n >= DEPOT_K[c] as usize {
            return false;
        }
        self.mags[c][n] = *magazine;
        true
    }

    /// Move class `c`'s newest floor entry into `magazine`; `n` is the
    /// class's current floor count (`1..=DEPOT_K[c]`).
    #[inline]
    pub(crate) fn floor_reload(
        &mut self,
        c: usize,
        n: usize,
        magazine: &mut [*mut u8; TCACHE_CAP],
    ) {
        debug_assert!(n > 0 && n <= DEPOT_K[c] as usize);
        *magazine = self.mags[c][n - 1];
    }
}
