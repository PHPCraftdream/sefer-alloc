//! Deterministic single-thread workload for the ghost-checks zero-cost proof
//! (docs/GHOST_CHECKS_PLAN.md §6, `scripts/ghost_zero_cost.mjs`): the same
//! binary built from the base and the head tree must have identical
//! normalized `.text` and identical Ir under callgrind. A `SeferAlloc`
//! instance (no `#[global_allocator]`, no maintenance thread) runs growth
//! (all live, then all freed), mixed-size churn and one Large round trip.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;

fn main() {
    let a = sefer_alloc::SeferAlloc::new();
    let mut live: Vec<(usize, Layout)> = Vec::with_capacity(16_384);
    // Growth: 16 B blocks, all live before any free.
    let l16 = Layout::from_size_align(16, 8).unwrap();
    for _ in 0..16_384 {
        // SAFETY: valid non-zero layout; freed exactly once below with the same layout.
        let p = unsafe { a.alloc(l16) };
        assert!(!p.is_null());
        live.push((p.expose_provenance(), l16));
    }
    black_box(&live);
    for (addr, l) in live.drain(..) {
        // SAFETY: live allocation from `a` with its own layout, freed once.
        unsafe { a.dealloc(core::ptr::with_exposed_provenance_mut(addr), l) };
    }
    // Churn: sizes 16..2048 in a fixed pattern, a window of 256 live blocks.
    let mut x: u32 = 0x2545_f491;
    for i in 0..65_536usize {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        let size = 16usize << (x % 8);
        let l = Layout::from_size_align(size, 8).unwrap();
        // SAFETY: valid non-zero layout; freed once, either in the window or at the end.
        let p = unsafe { a.alloc(l) };
        assert!(!p.is_null());
        live.push((p.expose_provenance(), l));
        if live.len() > 256 {
            let (addr, l) = live.swap_remove(i % live.len());
            // SAFETY: live allocation from `a` with its own layout, freed once.
            unsafe { a.dealloc(core::ptr::with_exposed_provenance_mut(addr), l) };
        }
    }
    for (addr, l) in live.drain(..) {
        // SAFETY: as above.
        unsafe { a.dealloc(core::ptr::with_exposed_provenance_mut(addr), l) };
    }
    // One Large round trip.
    let big = Layout::from_size_align(1 << 20, 64).unwrap();
    // SAFETY: valid layout; freed once with the same layout.
    let p = unsafe { a.alloc(big) };
    assert!(!p.is_null());
    // SAFETY: as above.
    unsafe { a.dealloc(p, big) };
}
