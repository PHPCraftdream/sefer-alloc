//! Per-slot cold magazine depot storage, separate from `HeapCore`.

use crate::alloc_core::size_classes::SIZE_CLASS_TABLE;
use crate::registry::heap_core::state::tcache::TCACHE_CAP;

pub(crate) const DEPOT_CLASSES: usize = 8;
pub(crate) const DEPOT_K_MAX: u8 = 8;
pub(crate) const DEPOT_K: [u8; DEPOT_CLASSES] = [8, 8, 8, 8, 8, 8, 7, 1];
pub(crate) const DEPOT_BUDGET_BYTES: usize = 64 * 1024;

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
pub(crate) struct Depot {
    mags: [[[*mut u8; TCACHE_CAP]; DEPOT_K_MAX as usize]; DEPOT_CLASSES],
    n: [u8; DEPOT_CLASSES],
}

impl Depot {
    #[inline]
    pub(crate) fn len(&self, c: usize) -> usize {
        self.n[c] as usize
    }

    #[inline]
    pub(crate) fn depot_park(&mut self, c: usize, magazine: &[*mut u8; TCACHE_CAP]) -> bool {
        let n = self.len(c);
        if n >= DEPOT_K[c] as usize {
            return false;
        }
        self.mags[c][n] = *magazine;
        self.n[c] = (n + 1) as u8;
        true
    }

    #[inline]
    pub(crate) fn depot_reload(&mut self, c: usize, magazine: &mut [*mut u8; TCACHE_CAP]) -> bool {
        let n = self.len(c);
        if n == 0 {
            return false;
        }
        self.n[c] = (n - 1) as u8;
        *magazine = self.mags[c][n - 1];
        true
    }
}
