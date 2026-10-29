//! Paired short-sample wall-clock judge for the canonical `bench:table`
//! groups (measurement only). The criterion run behind `npm run bench:table`
//! measures each allocator in its own long window, so host noise that moves
//! between windows lands on one arm only. Here every round measures every arm
//! once per (group, size) in a short sample — a triplet of samples taken
//! back to back (untimed setup of all three arms first, then the three timed
//! samples with nothing in between, so the triplet sees one noise state). The
//! arm order of a cell walks all six permutations, shifted by one per round.
//!
//! Groups and bodies are those of `benches/global_alloc.rs`:
//! - `direct`: alloc `OPS` blocks, then free them (warm bulk burst; one
//!   untimed warm-up body per arm before the triplet);
//! - `churn`: a working set of `CHURN_WORKING_SET`, `OPS` random free + alloc
//!   steps per body;
//! - `churn_write`: as `churn`, writing 16 B into each new block.
//!
//! Churn regime:
//! - `steady`: one working set per (group, size, arm) is prefilled once and
//!   stays alive for the whole run; a sample is `calls` bodies on it;
//! - `fresh:K`: a sample prefills `K` new sets (untimed) and runs ONE body
//!   on each, then frees them — the shape of criterion's `iter_batched`
//!   (one body per setup result).
//!
//! Arms: `SeferAlloc::new()`, `mimalloc::MiMalloc`, `std::alloc::System`,
//! called through `GlobalAlloc` directly (none is the process allocator).
//!
//! Usage: `bench_table_paired [rounds] [calls] [regime]` (default 200 rounds,
//! 2 calls, `steady`). Output: one JSON line per sample; tables are derived
//! by `scripts/bench-table-paired.mjs`.

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

/// Churn regime, see the module doc.
#[derive(Clone, Copy)]
enum Regime {
    Steady,
    Fresh(usize),
}

/// All six orders of the three arms.
const PERMS: [[usize; 3]; 6] = [
    [0, 1, 2],
    [0, 2, 1],
    [1, 0, 2],
    [1, 2, 0],
    [2, 0, 1],
    [2, 1, 0],
];

/// One prefilled churn working set (untimed).
fn fill<A: GlobalAlloc>(a: &A, layout: Layout, write: bool) -> Vec<*mut u8> {
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
}

fn free_all<A: GlobalAlloc>(a: &A, layout: Layout, sets: &[Vec<*mut u8>]) {
    for &p in sets.iter().flatten() {
        if !p.is_null() {
            // SAFETY: every slot holds a live block of `a` with `layout`.
            unsafe { a.dealloc(p, layout) };
        }
    }
}

/// Untimed setup of one arm's sample: the warm-up body for `direct`; for
/// churn, the persistent set (`steady`, created on first use) or `K` fresh
/// sets.
fn setup<A: GlobalAlloc>(
    a: &A,
    group: &str,
    layout: Layout,
    regime: Regime,
    sets: &mut Vec<Vec<*mut u8>>,
) {
    let write = group == "churn_write";
    match (group, regime) {
        ("direct", _) => direct(a, layout),
        (_, Regime::Steady) => {
            if sets.is_empty() {
                sets.push(fill(a, layout, write));
            }
        }
        (_, Regime::Fresh(k)) => {
            sets.extend((0..k).map(|_| fill(a, layout, write)));
        }
    }
}

/// One timed sample; returns (ns per alloc+free pair, pairs).
fn sample<A: GlobalAlloc>(
    a: &A,
    group: &str,
    layout: Layout,
    regime: Regime,
    sets: &mut [Vec<*mut u8>],
    calls: usize,
) -> (f64, usize) {
    let write = group == "churn_write";
    let t = Instant::now();
    let bodies = match (group, regime) {
        ("direct", _) => {
            for _ in 0..calls {
                direct(a, layout);
            }
            calls
        }
        (_, Regime::Steady) => {
            for _ in 0..calls {
                churn_step(a, layout, &mut sets[0], write);
            }
            calls
        }
        (_, Regime::Fresh(_)) => {
            for live in sets.iter_mut() {
                churn_step(a, layout, live, write);
            }
            sets.len()
        }
    };
    let pairs = bodies * OPS;
    (t.elapsed().as_nanos() as f64 / pairs as f64, pairs)
}

/// Per-sample teardown: fresh sets are freed, the steady set is kept.
fn teardown<A: GlobalAlloc>(a: &A, layout: Layout, regime: Regime, sets: &mut Vec<Vec<*mut u8>>) {
    if let Regime::Fresh(_) = regime {
        free_all(a, layout, sets);
        sets.clear();
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let rounds: usize = args.next().map_or(200, |s| s.parse().unwrap());
    let calls: usize = args.next().map_or(2, |s| s.parse().unwrap());
    let regime_arg = args.next().unwrap_or_else(|| "steady".to_string());
    let regime = match regime_arg.strip_prefix("fresh:") {
        Some(k) => Regime::Fresh(k.parse().unwrap()),
        None if regime_arg == "steady" => Regime::Steady,
        None => panic!("regime must be `steady` or `fresh:K`"),
    };
    assert!(rounds > 0 && calls > 0 && !matches!(regime, Regime::Fresh(0)));
    let cells = GROUPS.len() * SIZES.len();
    // sets[cell][arm]: churn working sets of that arm (persistent for `steady`).
    let mut sets: Vec<[Vec<Vec<*mut u8>>; 3]> = (0..cells).map(|_| Default::default()).collect();
    for round in 0..rounds {
        for (gi, group) in GROUPS.into_iter().enumerate() {
            for (si, size) in SIZES.into_iter().enumerate() {
                let cell = gi * SIZES.len() + si;
                let layout = Layout::from_size_align(size, 8).unwrap();
                let [ss, sm, sy] = &mut sets[cell];
                setup(&SEFER, group, layout, regime, ss);
                setup(&MI, group, layout, regime, sm);
                setup(&SYS, group, layout, regime, sy);
                let perm = PERMS[(round + cell) % PERMS.len()];
                let mut ns = [(0f64, 0usize); 3];
                // The triplet: three timed samples back to back.
                for &i in &perm {
                    ns[i] = match i {
                        0 => sample(&SEFER, group, layout, regime, ss, calls),
                        1 => sample(&MI, group, layout, regime, sm, calls),
                        _ => sample(&SYS, group, layout, regime, sy, calls),
                    };
                }
                teardown(&SEFER, layout, regime, ss);
                teardown(&MI, layout, regime, sm);
                teardown(&SYS, layout, regime, sy);
                let regime_name = if group == "direct" {
                    "direct"
                } else {
                    regime_arg.as_str()
                };
                for (i, arm) in ARMS.iter().enumerate() {
                    let order = perm.iter().position(|&x| x == i).unwrap();
                    println!(
                        "{{\"round\":{round},\"group\":\"{group}\",\"size\":{size},\"arm\":\"{arm}\",\"order\":{order},\"calls\":{calls},\"regime\":\"{regime_name}\",\"pairs\":{},\"ns_per_pair\":{:.3}}}",
                        ns[i].1, ns[i].0
                    );
                }
            }
        }
    }
    for (cell, [ss, sm, sy]) in sets.iter().enumerate() {
        let layout = Layout::from_size_align(SIZES[cell % SIZES.len()], 8).unwrap();
        free_all(&SEFER, layout, ss);
        free_all(&MI, layout, sm);
        free_all(&SYS, layout, sy);
    }
}
