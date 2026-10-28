//! Per-slot cold magazine depot storage, separate from `HeapCore`.

use crate::alloc_core::size_classes::{SIZE_CLASS_TABLE, SMALL_CLASS_COUNT};
use crate::registry::heap_core::state::tcache::TCACHE_CAP;

// G1-K5a: one depot magazine (K = 1) for every class from 240 B to 1024 B.
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
    let mut k = [4u8; DEPOT_CLASSES];
    let small = [8u8, 8, 8, 8, 8, 8, 7, 1];
    let mut c = 0;
    while c < small.len() {
        k[c] = small[c];
        c += 1;
    }
    k
};
pub(crate) const DEPOT_BUDGET_BYTES: usize = 448 * 1024;
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
const _: () = assert!(DEPOT_K[0] <= DEPOT_K_MAX);
const _: () = assert!(DEPOT_K[1] <= DEPOT_K_MAX);
const _: () = assert!(DEPOT_K[2] <= DEPOT_K_MAX);
const _: () = assert!(DEPOT_K[3] <= DEPOT_K_MAX);
const _: () = assert!(DEPOT_K[4] <= DEPOT_K_MAX);
const _: () = assert!(DEPOT_K[5] <= DEPOT_K_MAX);
const _: () = assert!(DEPOT_K[6] <= DEPOT_K_MAX);
const _: () = assert!(DEPOT_K[7] <= DEPOT_K_MAX);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[0] == 16);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[1] == 32);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[2] == 48);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[3] == 64);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[4] == 80);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[5] == 112);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[6] == 144);
const _: () = assert!(crate::alloc_core::size_classes::SIZE_CLASS_TABLE[7] == 192);
const _: () = assert!(depot_bytes() <= DEPOT_BUDGET_BYTES);
const _: () =
    assert!(crate::registry::heap_core::state::tcache::FREE_PARK_CAP[0] == TCACHE_CAP as u8);
const _: () =
    assert!(crate::registry::heap_core::state::tcache::FREE_PARK_CAP[1] == TCACHE_CAP as u8);
const _: () =
    assert!(crate::registry::heap_core::state::tcache::FREE_PARK_CAP[2] == TCACHE_CAP as u8);
const _: () =
    assert!(crate::registry::heap_core::state::tcache::FREE_PARK_CAP[3] == TCACHE_CAP as u8);
const _: () =
    assert!(crate::registry::heap_core::state::tcache::FREE_PARK_CAP[4] == TCACHE_CAP as u8);
const _: () =
    assert!(crate::registry::heap_core::state::tcache::FREE_PARK_CAP[5] == TCACHE_CAP as u8);
const _: () =
    assert!(crate::registry::heap_core::state::tcache::FREE_PARK_CAP[6] == TCACHE_CAP as u8);
const _: () =
    assert!(crate::registry::heap_core::state::tcache::FREE_PARK_CAP[7] == TCACHE_CAP as u8);

/// Classes `0..8` map to sizes 16, 32, 48, 64, 80, 112, 144, and 192 B.
/// With K `[8,8,8,8,8,8,7,1]`, weighted block-bytes are 4,016; multiplying
/// by 16 gives 64,256 B, within the 65,536 B depot budget.
///
/// Only the pointer arrays live here; each class's parked count is
/// `PerClass::parked` in the owner's magazine line (DP-5 iteration 2), so the
/// hot-path gates read it without touching this store.
pub(crate) struct Depot {
    mags: [[[*mut u8; TCACHE_CAP]; DEPOT_K_MAX as usize]; DEPOT_CLASSES],
}

impl Depot {
    /// Kani-only: the all-zero empty depot an OS-zeroed registry slot holds.
    #[cfg(kani)]
    pub(crate) const fn empty() -> Self {
        Self {
            mags: [[[core::ptr::null_mut(); TCACHE_CAP]; DEPOT_K_MAX as usize]; DEPOT_CLASSES],
        }
    }

    /// Park `magazine` as class `c`'s next entry; `n` is that class's parked
    /// count. Fails (no change) when the class already holds `DEPOT_K[c]`.
    #[inline]
    pub(crate) fn depot_park(
        &mut self,
        c: usize,
        n: &mut u8,
        magazine: &[*mut u8; TCACHE_CAP],
    ) -> bool {
        let i = *n as usize;
        if i >= DEPOT_K[c] as usize {
            return false;
        }
        self.mags[c][i] = *magazine;
        *n += 1;
        true
    }

    /// Move class `c`'s most recently parked magazine into `magazine`.
    /// Fails (no change) when `n == 0`.
    #[inline]
    pub(crate) fn depot_reload(
        &mut self,
        c: usize,
        n: &mut u8,
        magazine: &mut [*mut u8; TCACHE_CAP],
    ) -> bool {
        if *n == 0 {
            return false;
        }
        *n -= 1;
        *magazine = self.mags[c][*n as usize];
        true
    }
}
