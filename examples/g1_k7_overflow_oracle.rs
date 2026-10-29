//! G1-K7 activation oracle (measurement only): the J1′ judge's kernel shapes
//! run round-robin through the real `#[global_allocator]` in one thread, and
//! the calling heap's depot-overflow counters are read before and after each
//! portion (outside any timed window). Prints one JSON line per kernel with
//! the summed counter deltas over all cycles, plus the resolved budget.
//!
//! Usage: g1_k7_overflow_oracle [cycles] [kernels]  (default 200, all 13;
//! `kernels` is a csv subset, run round-robin in registry order)

use sefer_alloc::SeferAlloc;
use std::alloc::{alloc, dealloc, Layout};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const NAMES: [&str; 9] = [
    "bytes",
    "parks",
    "reloads",
    "budget_refusals",
    "os_refusals",
    "decay_flushed",
    "ingress_checks",
    "decommit_calls",
    "committed_bytes",
];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn l(size: usize) -> Layout {
    Layout::from_size_align(size, 8).unwrap()
}

/// Pointer and size tables live outside the allocator under test.
struct Slots {
    p: Vec<*mut u8>,
    s: Vec<usize>,
}

fn pair(size: usize, ops: u64, rng: &mut Rng) {
    for _ in 0..ops {
        let sz = if size == 0 {
            16 + 16 * rng.below(32) as usize
        } else {
            size
        };
        // SAFETY: non-zero layout; freed right away with the same layout.
        unsafe {
            let p = alloc(l(sz));
            assert!(!p.is_null());
            dealloc(p, l(sz));
        }
    }
}

fn bulk(slots: &mut Slots, n: usize, size: usize, passes: u64) {
    for _ in 0..passes {
        for i in 0..n {
            // SAFETY: non-zero layout.
            slots.p[i] = unsafe { alloc(l(size)) };
            assert!(!slots.p[i].is_null());
        }
        for i in 0..n {
            // SAFETY: allocated above with the same layout.
            unsafe { dealloc(slots.p[i], l(size)) };
        }
    }
}

fn churn(
    slots: &mut Slots,
    live: usize,
    ops: u64,
    size_of: &mut dyn FnMut() -> usize,
    rng: &mut Rng,
) {
    for i in 0..live {
        let sz = size_of();
        // SAFETY: non-zero layout.
        slots.p[i] = unsafe { alloc(l(sz)) };
        slots.s[i] = sz;
    }
    for _ in 0..ops {
        let i = rng.below(live as u64) as usize;
        let sz = size_of();
        // SAFETY: slot i holds a live block of size s[i]; replaced by a new one.
        unsafe {
            dealloc(slots.p[i], l(slots.s[i]));
            slots.p[i] = alloc(l(sz));
        }
        slots.s[i] = sz;
    }
    for i in 0..live {
        // SAFETY: every slot holds a live block of size s[i].
        unsafe { dealloc(slots.p[i], l(slots.s[i])) };
    }
}

fn vec_grow(cycles: u64) {
    for _ in 0..cycles {
        let mut v: Vec<u64> = Vec::new();
        for i in 0..512u64 {
            v.push(i);
        }
        std::hint::black_box(&v);
    }
}

fn snap() -> [u64; 9] {
    GLOBAL.dbg_current_depot_overflow().expect("own heap").1
}

fn main() {
    let cycles: u64 = std::env::args().nth(1).map_or(200, |s| s.parse().unwrap());
    let only: Option<Vec<String>> = std::env::args()
        .nth(2)
        .map(|s| s.split(',').map(str::to_string).collect());
    let mut slots = Slots {
        p: vec![core::ptr::null_mut(); 65536],
        s: vec![0; 65536],
    };
    let mut rng = Rng(0xb411_0001);
    // Warm the heap so the accessor sees the owner heap.
    pair(16, 1, &mut rng);
    let ((resolved, effective), _) = GLOBAL.dbg_current_depot_overflow().expect("own heap");
    let kernels = [
        "pair16",
        "pair64",
        "pair_rt",
        "bulk64",
        "bulk16k",
        "churn16",
        "churn64",
        "churn256",
        "churn1024",
        "vec512",
        "larson",
        "bulk64k",
        "bulk16k_32",
    ];
    let mut sum = vec![[0u64; 9]; kernels.len()];
    for _ in 0..cycles {
        for (k, name) in kernels.iter().enumerate() {
            if only.as_ref().is_some_and(|o| !o.iter().any(|w| w == name)) {
                continue;
            }
            let a = snap();
            match *name {
                "pair16" => pair(16, 512, &mut rng),
                "pair64" => pair(64, 512, &mut rng),
                "pair_rt" => pair(0, 512, &mut rng),
                "bulk64" => bulk(&mut slots, 64, 64, 8),
                "bulk16k" => bulk(&mut slots, 16384, 64, 1),
                "churn16" => churn(&mut slots, 256, 128, &mut || 16, &mut Rng(rng.next() | 1)),
                "churn64" => churn(&mut slots, 256, 128, &mut || 64, &mut Rng(rng.next() | 1)),
                "churn256" => churn(&mut slots, 256, 128, &mut || 256, &mut Rng(rng.next() | 1)),
                "churn1024" => churn(&mut slots, 256, 48, &mut || 1024, &mut Rng(rng.next() | 1)),
                "vec512" => vec_grow(24),
                "larson" => {
                    let mut r2 = Rng(rng.next() | 1);
                    let mut r3 = Rng(rng.next() | 1);
                    churn(
                        &mut slots,
                        768,
                        256,
                        &mut || 16 + 16 * r3.below(32) as usize,
                        &mut r2,
                    )
                }
                "bulk64k" => bulk(&mut slots, 65536, 64, 1),
                "bulk16k_32" => bulk(&mut slots, 16384, 32, 1),
                _ => unreachable!(),
            }
            let b = snap();
            for i in 1..9 {
                sum[k][i] += b[i].wrapping_sub(a[i]);
            }
            sum[k][0] = b[0];
            sum[k][8] = b[8];
        }
    }
    println!(
        "{{\"budget_resolved\":{resolved},\"budget_effective\":{effective},\"cycles\":{cycles},\"config_conflicts_total\":{}}}",
        sefer_alloc::registry::config_conflicts_total()
    );
    for (k, name) in kernels.iter().enumerate() {
        if only.as_ref().is_some_and(|o| !o.iter().any(|w| w == name)) {
            continue;
        }
        let fields: Vec<String> = NAMES
            .iter()
            .zip(sum[k])
            .map(|(n, v)| format!("\"{n}\":{v}"))
            .collect();
        println!("{{\"kernel\":\"{name}\",{}}}", fields.join(","));
    }
}
