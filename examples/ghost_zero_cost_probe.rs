//! Deterministic single-thread ghost-checks zero-cost workload; run the same
//! probe against base and head. A local `SeferAlloc` instance handles growth,
//! mixed-size churn, and one Large round trip, without a global allocator
//! override or a maintenance thread.
//!
//! `#[inline(never)]` phases isolate loop frames, and `black_box` arguments
//! inhibit codegen specialization. Register allocation in a single `main`
//! frame caused spurious Ir deltas on P2.2/P2.3. See
//! `docs/perf/GHOST_ZERO_COST_PROBE_STABILITY_20261121.md` and
//! `docs/GHOST_CHECKS_PLAN.md` §6.
#![allow(unsafe_code)]

use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;
use std::ptr::with_exposed_provenance_mut;

type Block = (usize, Layout);

#[inline(never)]
fn growth(a: &SeferAlloc, live: &mut Vec<Block>) {
    let l16 = Layout::from_size_align(16, 8).unwrap();
    for _ in 0..16_384 {
        // SAFETY: l16 is a valid nonzero layout.
        let p = unsafe { GlobalAlloc::alloc(a, l16) };
        assert!(!p.is_null());
        live.push((p.expose_provenance(), l16));
    }
    black_box(&*live);
    for (addr, layout) in live.drain(..) {
        // SAFETY: drain takes each live address exposed from a once, with its original layout.
        unsafe { GlobalAlloc::dealloc(a, with_exposed_provenance_mut(addr), layout) };
    }
}

#[inline(never)]
fn churn(a: &SeferAlloc, live: &mut Vec<Block>) {
    let mut x: u32 = 0x2545_f491;
    for i in 0..65_536usize {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        let size = 16usize << (x % 8);
        let layout = Layout::from_size_align(size, 8).unwrap();
        // SAFETY: layout is valid and nonzero (16..=2048 bytes, alignment 8).
        let p = unsafe { GlobalAlloc::alloc(a, layout) };
        assert!(!p.is_null());
        live.push((p.expose_provenance(), layout));
        if live.len() > 256 {
            let (addr, layout) = live.swap_remove(i % live.len());
            // SAFETY: removal takes a still-live address exposed from a and its original layout.
            unsafe { GlobalAlloc::dealloc(a, with_exposed_provenance_mut(addr), layout) };
        }
    }
    for (addr, layout) in live.drain(..) {
        // SAFETY: drain takes only the remaining live addresses exposed from a, with their layouts.
        unsafe { GlobalAlloc::dealloc(a, with_exposed_provenance_mut(addr), layout) };
    }
}

#[inline(never)]
fn large(a: &SeferAlloc) {
    let layout = Layout::from_size_align(1 << 20, 64).unwrap();
    // SAFETY: layout is valid and nonzero (1 MiB, alignment 64).
    let p = unsafe { GlobalAlloc::alloc(a, layout) };
    assert!(!p.is_null());
    // SAFETY: p is the live result from a above, freed once with the original layout.
    unsafe { GlobalAlloc::dealloc(a, p, layout) };
}

fn main() {
    let a = SeferAlloc::new();
    // No global allocator override: the same system-backed buffer serves both phases.
    let mut live = Vec::with_capacity(16_384);
    growth(black_box(&a), black_box(&mut live));
    churn(black_box(&a), black_box(&mut live));
    large(black_box(&a));
}
