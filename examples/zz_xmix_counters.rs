//! B4 X-mix diagnosis (worktree b4-judges): X-mix churn (the exact larson
//! `pick_size` size distribution from `crates/malloc-bench-rs/src/lib.rs`)
//! through the real `#[global_allocator]`, then a per-step readout of the
//! `bench-internals` magazine slow-path counters
//! (`SeferAlloc::dbg_xmix_counters`). NOT a timing probe — run it plain:
//! `cargo run --release --example zz_xmix_counters \
//!    --features "production internals bench-internals"`
//! (production supplies alloc-global + fastbin + alloc-xthread).

use core::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Deterministic, dependency-free PRNG (xorshift64*), copied bit-for-bit from
/// `crates/malloc-bench-rs/src/lib.rs` so the X-mix size distribution matches
/// the macro bench exactly.
struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// Exact copy of `pick_size` from `crates/malloc-bench-rs/src/lib.rs`
/// (~97 % 16..512 B, ~3 % 512 B..8 KiB).
#[inline]
fn pick_size(rng: &mut XorShift64) -> usize {
    let r = rng.next_u64();
    if r.is_multiple_of(32) {
        // ~3% large: 512 B .. 8 KiB.
        512 + (r >> 8) as usize % (8 * 1024 - 512)
    } else {
        // ~97% small: 16 .. 512 B.
        16 + (r >> 8) as usize % (512 - 16)
    }
}

/// X-mix churn: 768 live slots, each step frees one random slot and re-allocs
/// it with a fresh `pick_size` size (same shape as `alloc_only_probe.rs`, the
/// only difference being the size distribution).
fn churn(steps: usize) {
    const WS: usize = 768;
    const SEED: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut rng = XorShift64::new(SEED);
    let mut slots: Vec<(*mut u8, Layout)> = Vec::with_capacity(WS);
    for _ in 0..WS {
        let size = pick_size(&mut rng);
        let l = Layout::from_size_align(size, 8).unwrap();
        // SAFETY: global allocator, freed exactly once below / on resize.
        slots.push((unsafe { GLOBAL.alloc(l) }, l));
    }
    for _ in 0..steps {
        let i = (rng.next_u64() as usize) % WS;
        let (p, l) = slots[i];
        // SAFETY: live block issued above / by a prior step, freed once here.
        unsafe { GLOBAL.dealloc(p, l) };
        let size = pick_size(&mut rng);
        let nl = Layout::from_size_align(size, 8).unwrap();
        // SAFETY: global allocator, freed exactly once (this slot or cleanup).
        slots[i] = (unsafe { GLOBAL.alloc(nl) }, nl);
    }
    for (p, l) in slots {
        // SAFETY: live block, freed exactly once.
        unsafe { GLOBAL.dealloc(p, l) };
    }
}

fn main() {
    let steps: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);

    // Warm-up (registry bootstrap, magazine/segment population) so the
    // counted window starts near steady state, like the callgrind probe.
    churn(10_000);

    let before = SeferAlloc::dbg_xmix_counters();
    churn(steps);
    let after = SeferAlloc::dbg_xmix_counters();

    let d: Vec<u64> = after
        .iter()
        .zip(before.iter())
        .map(|(a, b)| a - b)
        .collect();
    let names = [
        "refill_events",
        "refill_blocks",
        "overflow_events",
        "flush_blocks",
        "r101_events",
    ];
    println!("steps = {steps}");
    let mut i = 0;
    while i < names.len() {
        let (le, gt) = (d[2 * i], d[2 * i + 1]);
        println!(
            "{:<16} le512 = {:>8} ({:.6}/step)   gt512 = {:>8} ({:.6}/step)   total = {:.6}/step",
            names[i],
            le,
            le as f64 / steps as f64,
            gt,
            gt as f64 / steps as f64,
            (le + gt) as f64 / steps as f64,
        );
        i += 1;
    }
}
