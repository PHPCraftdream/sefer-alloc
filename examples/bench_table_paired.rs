//! Paired short-sample wall-clock judge for the canonical `bench:table`
//! groups (measurement only). The criterion run behind `npm run bench:table`
//! measures each allocator in its own long window, so host noise that moves
//! between windows lands on one arm only. Here every round measures every arm
//! once per (group, size) in a short sample — a triplet of samples taken
//! back to back (setup and warm-up of all three arms first, then the three
//! timed samples with nothing in between, so the triplet sees one noise
//! state), in a rotated arm order; the statistic is the minimum over rounds
//! per arm (the least disturbed sample), with the median printed next to it.
//!
//! Groups and bodies are those of `benches/global_alloc.rs`:
//! - `direct`: alloc `OPS` blocks, then free them (warm bulk burst);
//! - `churn`: a prefilled working set of `CHURN_WORKING_SET`, `OPS` random
//!   free + alloc steps per call;
//! - `churn_write`: as `churn`, writing 16 B into each new block.
//!
//! Arms: `SeferAlloc::new()`, `mimalloc::MiMalloc`, `std::alloc::System`,
//! called through `GlobalAlloc` directly (none is the process allocator).
//!
//! Usage: `bench_table_paired [rounds] [calls] [batch]` (default 200 rounds,
//! 2 calls per sample = 2048 pairs, tens of microseconds per sample; `batch`
//! fresh churn working sets per sample, default 1 = one long-lived set in
//! steady state; criterion's `iter_batched` prefills several before timing). Output: one JSON line per sample; tables are
//! derived by `scripts/bench-table-paired.mjs`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::time::Instant;

use sefer_alloc::SeferAlloc;

static SEFER: SeferAlloc = SeferAlloc::new();
static MI: mimalloc::MiMalloc = mimalloc::MiMalloc;
static SYS: System = System;

const SIZES: [usize; 4] = [16, 64, 256, 1024];
const OPS: usize = 1024;
const CHURN_WORKING_SET: usize = 256;
const GROUPS: [&str; 3] = ["direct", "churn", "churn_write"];
const ARMS: [&str; 3] = ["sefer", "mimalloc", "system"];

struct XorShift64(u64);

impl XorShift64 {
    const fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_usize(&mut self) -> usize {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D) as usize
    }
}

fn direct<A: GlobalAlloc>(a: &A, layout: Layout) {
    let mut ptrs = [core::ptr::null_mut::<u8>(); OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: non-zero layout.
        *slot = unsafe { a.alloc(layout) };
    }
    black_box(&ptrs);
    for &p in &ptrs {
        if !p.is_null() {
            // SAFETY: allocated above by `a` with `layout`, freed once.
            unsafe { a.dealloc(p, layout) };
        }
    }
}

/// The `churn_write` store of `benches/global_alloc.rs`: two volatile words
/// at offsets 0 and 8 of a fresh block.
fn write16(p: *mut u8) {
    if !p.is_null() {
        // SAFETY: `p` is a fresh block of at least 16 bytes.
        unsafe {
            core::ptr::write_volatile(p.cast::<u64>(), 0xA5A5_A5A5_A5A5_A5A5);
            core::ptr::write_volatile(p.cast::<u64>().add(1), 0xA5A5_A5A5_A5A5_A5A5);
        }
    }
}

fn churn_step<A: GlobalAlloc>(a: &A, layout: Layout, live: &mut [*mut u8], write: bool) {
    let mut rng = XorShift64::new(0xCAFE);
    for _ in 0..OPS {
        let i = rng.next_usize() % live.len();
        if !live[i].is_null() {
            // SAFETY: slot i holds a live block of `a` with `layout`.
            unsafe { a.dealloc(live[i], layout) };
        }
        // SAFETY: non-zero layout.
        let p = unsafe { a.alloc(layout) };
        if write {
            write16(p);
        }
        live[i] = p;
    }
    black_box(&live);
}

/// Untimed setup of one arm for `group`: `batch` freshly prefilled churn
/// working sets (none for `direct`, which gets one warm-up call instead).
/// Churn sets are not warmed: like criterion's `iter_batched` setup, every
/// timed churn call starts from a fresh prefill.
fn prepare<A: GlobalAlloc>(a: &A, group: &str, layout: Layout, batch: usize) -> Vec<Vec<*mut u8>> {
    if group == "direct" {
        direct(a, layout);
        return Vec::new();
    }
    let write = group == "churn_write";
    (0..batch)
        .map(|_| {
            (0..CHURN_WORKING_SET)
                .map(|_| {
                    // SAFETY: non-zero layout.
                    let p = unsafe { a.alloc(layout) };
                    if write {
                        write16(p);
                    }
                    p
                })
                .collect()
        })
        .collect()
}

/// One timed sample, in ns per alloc+free pair: `calls` bodies of `direct`,
/// or `calls` churn bodies on each prepared working set.
fn measure<A: GlobalAlloc>(
    a: &A,
    group: &str,
    layout: Layout,
    sets: &mut [Vec<*mut u8>],
    calls: usize,
) -> f64 {
    let write = group == "churn_write";
    let t = Instant::now();
    if group == "direct" {
        for _ in 0..calls {
            direct(a, layout);
        }
    } else {
        for live in sets.iter_mut() {
            for _ in 0..calls {
                churn_step(a, layout, live, write);
            }
        }
    }
    t.elapsed().as_nanos() as f64 / (calls * sets.len().max(1) * OPS) as f64
}

fn finish<A: GlobalAlloc>(a: &A, layout: Layout, sets: &[Vec<*mut u8>]) {
    for &p in sets.iter().flatten() {
        if !p.is_null() {
            // SAFETY: every slot holds a live block of `a` with `layout`.
            unsafe { a.dealloc(p, layout) };
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let rounds: usize = args.next().map_or(200, |s| s.parse().unwrap());
    let calls: usize = args.next().map_or(2, |s| s.parse().unwrap());
    let batch: usize = args.next().map_or(1, |s| s.parse().unwrap());
    assert!(rounds > 0 && calls > 0 && batch > 0);
    let mut k = 0usize;
    for round in 0..rounds {
        for group in GROUPS {
            for size in SIZES {
                let layout = Layout::from_size_align(size, 8).unwrap();
                let mut ls = prepare(&SEFER, group, layout, batch);
                let mut lm = prepare(&MI, group, layout, batch);
                let mut ly = prepare(&SYS, group, layout, batch);
                let mut ns = [0f64; 3];
                // The triplet: three timed samples back to back, rotated order.
                for j in 0..ARMS.len() {
                    let i = (k + j) % ARMS.len();
                    ns[i] = match i {
                        0 => measure(&SEFER, group, layout, &mut ls, calls),
                        1 => measure(&MI, group, layout, &mut lm, calls),
                        _ => measure(&SYS, group, layout, &mut ly, calls),
                    };
                }
                finish(&SEFER, layout, &ls);
                finish(&MI, layout, &lm);
                finish(&SYS, layout, &ly);
                let b = if group == "direct" { 1 } else { batch };
                for (i, arm) in ARMS.iter().enumerate() {
                    println!(
                        "{{\"round\":{round},\"group\":\"{group}\",\"size\":{size},\"arm\":\"{arm}\",\"order\":{},\"calls\":{calls},\"batch\":{b},\"pairs\":{},\"ns_per_pair\":{:.3}}}",
                        (i + ARMS.len() - k % ARMS.len()) % ARMS.len(),
                        calls * b * OPS,
                        ns[i]
                    );
                }
                k += 1;
            }
        }
    }
}
