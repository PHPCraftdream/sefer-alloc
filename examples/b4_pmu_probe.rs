//! B4 A3 PMU probe (measurement only): one allocator, one churn kernel with a
//! configurable live set, run under `perf stat` / `perf record`.
//!
//! Kernel (same shape as the J1′ churn kernel): a ring of `live` slots; per op
//! pick a slot uniformly at random, free its block, allocate a `size`-byte
//! replacement, write its first 16 bytes. `live = 1` is the pair pattern
//! (free the block just allocated). The allocator under test is a static
//! reached through `#[inline(never)]` (size, align) shims, like the judge;
//! the process global allocator stays `System`, so harness memory never
//! touches the allocator under test.
//!
//! Usage: b4_pmu_probe <sefer|mi> <live> <size> <ops> [seed]
//! Prints one line: `alloc live size ops ns_per_op checksum`.
//!
//! Interleaved mode: b4_pmu_probe inter <live> <size> <portion> <rounds> <pollute_kib>
//! Both allocators, each with its own slot ring; per round the portions run
//! in ABBA/BAAB order (A = sefer, B = mi), each `portion` ops, timed with
//! rdtsc. `pollute_kib` > 0 runs a polluter before EVERY portion: a walk over
//! a pollute_kib buffer with data-dependent branches (stand-in for
//! application work between allocator calls). An optional 7th argument
//! `size2` makes every second portion of each arm use `size2` instead of
//! `size` over the SAME ring (the judge's churn kernels share one ring, so
//! its churn16 portions free blocks left by churn64 portions). Prints
//! `inter live size portion pollute_kib sefer_ticks_per_op mi_ticks_per_op`.
//!
//! Bulk mode (G1): b4_pmu_probe bulk <sefer|mi> <n> <size> <passes>
//! Per pass: allocate `n` blocks of `size`, write 8 bytes each, free them in
//! allocation order (the J1′ bulk kernel). Prints
//! `bulk alloc n size passes ns_per_op checksum` (op = one alloc or free).
//!
//! Mixed mode (G1): b4_pmu_probe mixed <sefer|mi> <live> <size> <size2> <portion> <ops>
//! The `churn_sz` kernel over ONE ring, alternating `size` / `size2` every
//! `portion` ops (the J1′ churn kernels share one ring). Prints
//! `mixed alloc live size/size2 portion ops ns_per_op checksum`.

use std::alloc::{GlobalAlloc, Layout};
use std::time::Instant;

type AllocFn = fn(usize) -> *mut u8;
type DeallocFn = fn(*mut u8, usize);
type ArmPair = (AllocFn, DeallocFn);

static SEFER: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::new();
static MI: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[inline(never)]
fn sefer_alloc(size: usize) -> *mut u8 {
    // SAFETY: nonzero size, power-of-two align.
    unsafe { SEFER.alloc(Layout::from_size_align_unchecked(size, 8)) }
}

#[inline(never)]
fn sefer_dealloc(p: *mut u8, size: usize) {
    // SAFETY: `p` came from `sefer_alloc(size)` and is freed once.
    unsafe { SEFER.dealloc(p, Layout::from_size_align_unchecked(size, 8)) }
}

#[inline(never)]
fn mi_alloc(size: usize) -> *mut u8 {
    // SAFETY: nonzero size, power-of-two align.
    unsafe { MI.alloc(Layout::from_size_align_unchecked(size, 8)) }
}

#[inline(never)]
fn mi_dealloc(p: *mut u8, size: usize) {
    // SAFETY: `p` came from `mi_alloc(size)` and is freed once.
    unsafe { MI.dealloc(p, Layout::from_size_align_unchecked(size, 8)) }
}

struct XorShift64(u64);

impl XorShift64 {
    fn below(&mut self, bound: u64) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        ((x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) * bound) >> 32
    }
}

#[inline(never)]
fn churn(
    alloc: fn(usize) -> *mut u8,
    dealloc: fn(*mut u8, usize),
    slots: &mut [*mut u8],
    size: usize,
    ops: u64,
    seed: u64,
) -> u64 {
    let mut rng = XorShift64(seed | 1);
    let mut acc = 0u64;
    for i in 0..ops {
        let idx = rng.below(slots.len() as u64) as usize;
        dealloc(slots[idx], size);
        let p = alloc(size);
        assert!(!p.is_null(), "alloc({size}) returned null");
        // SAFETY: fresh `size`-byte block.
        unsafe { core::ptr::write_bytes(p, (i & 0xff) as u8, 16.min(size)) };
        slots[idx] = p;
        acc = acc.wrapping_add(p as u64);
    }
    acc
}

/// Like `churn`, but each slot remembers its own size (mixed-size ring).
#[inline(never)]
fn churn_sz(
    alloc: fn(usize) -> *mut u8,
    dealloc: fn(*mut u8, usize),
    slots: &mut [(*mut u8, usize)],
    size: usize,
    ops: u64,
    seed: u64,
) -> u64 {
    let mut rng = XorShift64(seed | 1);
    let mut acc = 0u64;
    for i in 0..ops {
        let idx = rng.below(slots.len() as u64) as usize;
        let (old, old_sz) = slots[idx];
        dealloc(old, old_sz);
        let p = alloc(size);
        assert!(!p.is_null(), "alloc({size}) returned null");
        // SAFETY: fresh `size`-byte block.
        unsafe { core::ptr::write_bytes(p, (i & 0xff) as u8, 16.min(size)) };
        slots[idx] = (p, size);
        acc = acc.wrapping_add(p as u64);
    }
    acc
}

#[inline(never)]
fn pollute(buf: &mut [u64], seed: u64) -> u64 {
    let mut x = seed | 1;
    let mut acc = 0u64;
    for (i, w) in buf.iter_mut().enumerate() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        if x & 1 == 0 {
            acc = acc.wrapping_add(*w);
        } else if x & 2 == 0 {
            *w = w.wrapping_add(i as u64);
        } else {
            acc ^= x;
        }
    }
    acc
}

#[cfg(target_arch = "x86_64")]
fn rdtsc() -> u64 {
    // SAFETY: RDTSC is unprivileged on x86_64.
    unsafe { core::arch::x86_64::_rdtsc() }
}

// The probe's numbers are x86_64 TSC ticks and are measured only there; other
// targets (aarch64 CI, Apple silicon) get a monotonic-nanosecond stand-in so
// `cargo test` can build every example.
#[cfg(not(target_arch = "x86_64"))]
fn rdtsc() -> u64 {
    static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    T0.get_or_init(Instant::now).elapsed().as_nanos() as u64
}

fn inter(a: &[String]) {
    let live: usize = a[2].parse().expect("live");
    let size: usize = a[3].parse().expect("size");
    let portion: u64 = a[4].parse().expect("portion");
    let rounds: u64 = a[5].parse().expect("rounds");
    let kib: usize = a[6].parse().expect("pollute_kib");
    let size2: usize = a.get(7).map_or(0, |s| s.parse().expect("size2"));
    let mut buf = vec![1u64; kib * 128];
    let arms: [ArmPair; 2] = [(sefer_alloc, sefer_dealloc), (mi_alloc, mi_dealloc)];
    let mut slots: Vec<Vec<(*mut u8, usize)>> = arms
        .iter()
        .map(|(al, _)| (0..live).map(|_| (al(size), size)).collect())
        .collect();
    for (arm, (al, de)) in arms.iter().enumerate() {
        churn_sz(*al, *de, &mut slots[arm], size, 100_000, 7 + arm as u64);
    }
    let mut portions = [0u64; 2];
    let mut ticks = [0u64; 2];
    let mut acc = 0u64;
    for r in 0..rounds {
        let order: [usize; 4] = if r % 2 == 0 {
            [0, 1, 1, 0]
        } else {
            [1, 0, 0, 1]
        };
        for (j, &arm) in order.iter().enumerate() {
            if kib > 0 {
                acc ^= pollute(&mut buf, r * 4 + j as u64);
            }
            let (al, de) = arms[arm];
            let sz = if size2 > 0 && portions[arm] % 2 == 1 {
                size2
            } else {
                size
            };
            portions[arm] += 1;
            let t0 = rdtsc();
            acc ^= churn_sz(al, de, &mut slots[arm], sz, portion, r * 4 + j as u64);
            ticks[arm] += rdtsc() - t0;
        }
    }
    for (arm, (_, de)) in arms.iter().enumerate() {
        for (p, sz) in slots[arm].drain(..) {
            de(p, sz);
        }
    }
    let ops = (rounds * 2 * portion) as f64;
    println!(
        "inter {live} {size}/{size2} {portion} {kib} {:.3} {:.3} {:x}",
        ticks[0] as f64 / ops,
        ticks[1] as f64 / ops,
        acc
    );
}

fn arm(name: &str) -> ArmPair {
    match name {
        "sefer" => (sefer_alloc, sefer_dealloc),
        "mi" => (mi_alloc, mi_dealloc),
        other => panic!("unknown allocator {other}"),
    }
}

#[inline(never)]
fn bulk_pass(
    alloc: fn(usize) -> *mut u8,
    dealloc: fn(*mut u8, usize),
    slots: &mut [*mut u8],
    size: usize,
) -> u64 {
    let mut acc = 0u64;
    for s in slots.iter_mut() {
        let p = alloc(size);
        assert!(!p.is_null(), "alloc({size}) returned null");
        // SAFETY: fresh `size`-byte block, size >= 8.
        unsafe { core::ptr::write_bytes(p, 0xA5, 8) };
        *s = p;
        acc = acc.wrapping_add(p as u64);
    }
    for s in slots.iter() {
        dealloc(*s, size);
    }
    acc
}

fn bulk(a: &[String]) {
    assert!(
        a.len() >= 6,
        "usage: b4_pmu_probe bulk <sefer|mi> <n> <size> <passes>"
    );
    let (al, de) = arm(&a[2]);
    let n: usize = a[3].parse().expect("n");
    let size: usize = a[4].parse().expect("size");
    let passes: u64 = a[5].parse().expect("passes");
    assert!(n >= 1 && size >= 8);
    let mut slots = vec![core::ptr::null_mut::<u8>(); n];
    let mut acc = 0u64;
    for _ in 0..(passes / 10).max(4) {
        acc ^= bulk_pass(al, de, &mut slots, size);
    }
    let t0 = Instant::now();
    for _ in 0..passes {
        acc ^= bulk_pass(al, de, &mut slots, size);
    }
    let ns = t0.elapsed().as_nanos() as f64 / (2 * n as u64 * passes.max(1)) as f64;
    println!("bulk {} {n} {size} {passes} {ns:.3} {acc:x}", a[2]);
}

fn mixed(a: &[String]) {
    assert!(
        a.len() >= 8,
        "usage: b4_pmu_probe mixed <sefer|mi> <live> <size> <size2> <portion> <ops>"
    );
    let (al, de) = arm(&a[2]);
    let live: usize = a[3].parse().expect("live");
    let size: usize = a[4].parse().expect("size");
    let size2: usize = a[5].parse().expect("size2");
    let portion: u64 = a[6].parse().expect("portion");
    let ops: u64 = a[7].parse().expect("ops");
    assert!(live >= 1 && size >= 1 && size2 >= 1 && portion >= 1);
    let mut slots: Vec<(*mut u8, usize)> = (0..live).map(|_| (al(size), size)).collect();
    let run = |slots: &mut Vec<(*mut u8, usize)>, total: u64, salt: u64| {
        let mut acc = 0u64;
        let mut k = 0u64;
        while k * portion < total {
            let sz = if k.is_multiple_of(2) { size } else { size2 };
            acc ^= churn_sz(al, de, slots, sz, portion, salt + k);
            k += 1;
        }
        (acc, k * portion)
    };
    let (warm, _) = run(&mut slots, (ops / 10).max(10_000), 0x55);
    let t0 = Instant::now();
    let (acc, done) = run(&mut slots, ops, 0xb4a3);
    let ns = t0.elapsed().as_nanos() as f64 / done.max(1) as f64;
    for (p, sz) in slots {
        de(p, sz);
    }
    println!(
        "mixed {} {live} {size}/{size2} {portion} {done} {ns:.3} {:x}",
        a[2],
        acc ^ warm
    );
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("bulk") => return bulk(&a),
        Some("mixed") => return mixed(&a),
        _ => {}
    }
    if a.get(1).map(String::as_str) == Some("inter") {
        assert!(
            a.len() >= 7,
            "usage: b4_pmu_probe inter <live> <size> <portion> <rounds> <pollute_kib>"
        );
        inter(&a);
        return;
    }
    assert!(
        a.len() >= 5,
        "usage: b4_pmu_probe <sefer|mi> <live> <size> <ops> [seed]"
    );
    let (alloc, dealloc): ArmPair = arm(&a[1]);
    let live: usize = a[2].parse().expect("live");
    let size: usize = a[3].parse().expect("size");
    let ops: u64 = a[4].parse().expect("ops");
    let seed: u64 = a.get(5).map_or(0xb4a3, |s| s.parse().expect("seed"));
    assert!(live >= 1 && size >= 1);
    let mut slots: Vec<*mut u8> = (0..live).map(|_| alloc(size)).collect();
    assert!(slots.iter().all(|p| !p.is_null()));
    // Warm-up: reach the steady state before the measured window.
    let warm = churn(
        alloc,
        dealloc,
        &mut slots,
        size,
        (ops / 10).max(10_000),
        seed ^ 0x55,
    );
    let t0 = Instant::now();
    let acc = churn(alloc, dealloc, &mut slots, size, ops, seed);
    let ns = t0.elapsed().as_nanos() as f64 / ops.max(1) as f64;
    for p in slots {
        dealloc(p, size);
    }
    println!("{} {live} {size} {ops} {ns:.3} {:x}", a[1], acc ^ warm);
}
