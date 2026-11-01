//! G1 wild judge (measurement only): GiB-scale heaps and seeded random mixed
//! workloads, one allocator arm per process.
//!
//! Usage: g1_wild_judge <scenario> <sefer|sefer_pool|mi|sys> <seed> [scale]
//! Prints one JSON line: scenario, arm, seed, scale, ops, ns_per_op,
//! peak_rss, end_rss, checksum, anon_huge_peak, anon_huge_end (Linux THP bytes).
//!
//! `sefer_pool` is a diagnostic ceiling, not a product setting: the same allocator
//! with the empty-small-segment pool raised to 1024 segments / 4 GiB, so no
//! segment is released to the OS inside `free` (WG-M0).
//!
//! The allocator under test is a static reached through `#[inline(never)]`
//! shims (the `__rust_alloc` boundary); the process global allocator stays
//! `System`, so the harness's own slot vectors never touch the arm. Sizes and
//! ops come from a xorshift64* stream seeded by `seed`, so every arm runs the
//! same op sequence. `scale` (default 1.0) multiplies byte targets and op
//! counts. Every block gets its first and last byte written (the memory is
//! touched, so RSS reflects what the arm commits).
//!
//! `WJ_SPLIT=1` times every alloc/free call (stderr). `WJ_PROF=1` (WILD-1) adds
//! `wall_ns` and a `prof` object to the JSON line: the burst phases of `wild`/`phase`
//! timed as units, blocks above 256 KiB timed per call (alloc includes the first/last
//! byte write), realloc split by large/small with a moved count, and the `free_all`
//! teardown; `rest` = `wall_ns` minus those buckets (computed by the report script).
//!
//! Scenarios:
//! - `grow64`, `grow1k`: fixed-size blocks until 1 GiB live, free in order.
//! - `growmix`: 16 B..4 KiB log-uniform until 1 GiB live, free shuffled.
//! - `regrow`: `growmix` to 512 MiB, free every second block, grow back to
//!   512 MiB, free all (reuse of a holed heap).
//! - `large`: 64 KiB..32 MiB log-uniform until 1 GiB live, 5 000 random
//!   replacements, free all.
//! - `wild`: 50 000 slots, 1 000 000 ops: replace / alloc / free / realloc /
//!   LIFO bursts, heavy-tailed sizes (8 B..4 MiB).
//! - `burst` (WILD-1): only the LIFO bursts of `wild` (1..1000 blocks of 8..128 B,
//!   freed in reverse), 6 000 of them at scale 1, on a clean heap.
//! - `phase`: `wild` whose size distribution switches every 100 000 ops
//!   between tiny, medium and big.
//!
//! Bounds at scale 1: at most 1 GiB live (GiB scenarios) or 256 MiB live
//! (`wild`, `phase`: over the cap every op frees); a run takes about a second.
//! The machine is shared: `scale` is capped at 2 and a run whose peak RSS
//! exceeds [`RSS_LIMIT`] (4 GiB) exits with status 2.
use std::alloc::{GlobalAlloc, Layout, System};
use std::time::Instant;

static SEFER: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::new();
// WG-M0 diagnostic ceiling: empty small segments are retained (pool of 1024
// segments / 4 GiB) instead of released, so no release happens inside `free`.
static SEFER_POOL: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::with_config(
    sefer_alloc::LargeCacheConfig::DEFAULT.pool(
        sefer_alloc::SmallSegmentPoolConfig::new()
            .pool_segments(1024)
            .pool_byte_cap(4 << 30),
    ),
);
static MI: mimalloc::MiMalloc = mimalloc::MiMalloc;

const ALIGN: usize = 8;
const GIB: f64 = (1u64 << 30) as f64;
const WILD_CAP: f64 = 256.0 * (1u64 << 20) as f64;
/// Hard bound of one run on a shared machine.
const RSS_LIMIT: u64 = 4 << 30;

#[derive(Clone, Copy)]
enum Arm {
    Sefer,
    SeferPool,
    Mi,
    Sys,
}

#[inline(never)]
fn a_alloc(arm: Arm, size: usize) -> *mut u8 {
    // SAFETY: nonzero size, power-of-two align.
    let l = unsafe { Layout::from_size_align_unchecked(size, ALIGN) };
    // SAFETY: valid nonzero layout.
    unsafe {
        match arm {
            Arm::Sefer => SEFER.alloc(l),
            Arm::SeferPool => SEFER_POOL.alloc(l),
            Arm::Mi => MI.alloc(l),
            Arm::Sys => System.alloc(l),
        }
    }
}

#[inline(never)]
fn a_dealloc(arm: Arm, p: *mut u8, size: usize) {
    // SAFETY: `p` came from this arm with `size` bytes and is freed once.
    unsafe {
        let l = Layout::from_size_align_unchecked(size, ALIGN);
        match arm {
            Arm::Sefer => SEFER.dealloc(p, l),
            Arm::SeferPool => SEFER_POOL.dealloc(p, l),
            Arm::Mi => MI.dealloc(p, l),
            Arm::Sys => System.dealloc(p, l),
        }
    }
}

#[inline(never)]
fn a_realloc(arm: Arm, p: *mut u8, old: usize, new: usize) -> *mut u8 {
    // SAFETY: `p` is a live block of `old` bytes from this arm; `new` > 0.
    unsafe {
        let l = Layout::from_size_align_unchecked(old, ALIGN);
        match arm {
            Arm::Sefer => SEFER.realloc(p, l, new),
            Arm::SeferPool => SEFER_POOL.realloc(p, l, new),
            Arm::Mi => MI.realloc(p, l, new),
            Arm::Sys => System.realloc(p, l, new),
        }
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (((self.next() >> 32) * n as u64) >> 32) as usize
    }
    /// Log-uniform in `[lo, hi]`.
    fn log_size(&mut self, lo: usize, hi: usize) -> usize {
        let (a, b) = ((lo as f64).ln(), (hi as f64).ln());
        let u = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        ((a + (b - a) * u).exp() as usize).clamp(lo, hi)
    }
}

/// Heavy tail: 70 % 8..256 B, 20 % 256 B..32 KiB, 9 % 32 KiB..1 MiB, 1 % 1..4 MiB.
fn wild_size(r: &mut Rng) -> usize {
    match r.below(100) {
        0..=69 => r.log_size(8, 256),
        70..=89 => r.log_size(256, 32 << 10),
        90..=98 => r.log_size(32 << 10, 1 << 20),
        _ => r.log_size(1 << 20, 4 << 20),
    }
}

fn phase_size(r: &mut Rng, phase: usize) -> usize {
    match phase % 3 {
        0 => r.log_size(8, 64),
        1 => r.log_size(1 << 10, 16 << 10),
        _ => r.log_size(64 << 10, 1 << 20),
    }
}

/// `WJ_PROF=1` buckets (WILD-1): (ns, count[, moved]). Per-op timers only for blocks
/// above 256 KiB and for realloc; bursts are timed as units (a per-op timer costs about
/// as much as a small alloc); `rest` is the run's wall time minus all buckets.
#[derive(Default, Clone, Copy)]
struct Prof {
    burst_alloc: (u64, u64),
    burst_free: (u64, u64),
    large_alloc: (u64, u64),
    large_free: (u64, u64),
    large_realloc: (u64, u64, u64),
    small_realloc: (u64, u64, u64),
    teardown: (u64, u64),
}

struct Run {
    arm: Arm,
    ops: u64,
    sum: u64,
    live: usize,
    /// `WJ_SPLIT=1`: ns and count per (alloc|free) x (block <= 256 KiB | larger).
    split: Option<[[(u64, u64); 2]; 2]>,
    /// `WJ_PROF=1`: see [`Prof`].
    prof: Option<Prof>,
    /// Inside `free_all`: frees count towards `Prof::teardown` only.
    teardown: bool,
}

impl Run {
    fn note(&mut self, op: usize, size: usize, t: Option<Instant>) {
        if let (Some(s), Some(t)) = (self.split.as_mut(), t) {
            let e = &mut s[op][usize::from(size > 256 << 10)];
            e.0 += t.elapsed().as_nanos() as u64;
            e.1 += 1;
        }
    }
    fn alloc(&mut self, size: usize) -> *mut u8 {
        let t = self.split.map(|_| Instant::now());
        let tp = (self.prof.is_some() && size > 256 << 10).then(Instant::now);
        let p = a_alloc(self.arm, size);
        self.note(0, size, t);
        assert!(!p.is_null(), "allocation of {size} bytes failed");
        // SAFETY: `p` is a fresh block of `size` >= 1 bytes.
        unsafe {
            p.write(size as u8);
            p.add(size - 1).write(1);
        }
        if let (Some(f), Some(tp)) = (self.prof.as_mut(), tp) {
            f.large_alloc.0 += tp.elapsed().as_nanos() as u64;
            f.large_alloc.1 += 1;
        }
        self.ops += 1;
        self.live += size;
        self.sum = self.sum.wrapping_add(size as u64);
        p
    }
    fn free(&mut self, p: *mut u8, size: usize) {
        // SAFETY: `p` is a live block of `size` bytes written by `alloc`.
        self.sum = self.sum.wrapping_add(unsafe { p.read() } as u64);
        let t = self.split.map(|_| Instant::now());
        let tp = (self.prof.is_some() && !self.teardown && size > 256 << 10).then(Instant::now);
        a_dealloc(self.arm, p, size);
        self.note(1, size, t);
        if let (Some(f), Some(tp)) = (self.prof.as_mut(), tp) {
            f.large_free.0 += tp.elapsed().as_nanos() as u64;
            f.large_free.1 += 1;
        }
        self.ops += 1;
        self.live -= size;
    }
    fn realloc(&mut self, p: *mut u8, old: usize, new: usize) -> *mut u8 {
        let tp = self.prof.is_some().then(Instant::now);
        let q = a_realloc(self.arm, p, old, new);
        assert!(!q.is_null(), "reallocation to {new} bytes failed");
        // SAFETY: `q` holds `new` >= 1 bytes.
        unsafe { q.add(new - 1).write(2) };
        if let (Some(f), Some(tp)) = (self.prof.as_mut(), tp) {
            let e = if old.max(new) > 256 << 10 {
                &mut f.large_realloc
            } else {
                &mut f.small_realloc
            };
            e.0 += tp.elapsed().as_nanos() as u64;
            e.1 += 1;
            e.2 += u64::from(q != p);
        }
        self.ops += 1;
        self.live = self.live - old + new;
        q
    }
}

type Blocks = Vec<(*mut u8, usize)>;

fn grow_fixed(run: &mut Run, size: usize, target: f64) {
    let n = (target / size as f64) as usize;
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        v.push(run.alloc(size));
    }
    ANON_HUGE_PEAK.fetch_max(anon_huge_bytes(), std::sync::atomic::Ordering::Relaxed);
    for p in v {
        run.free(p, size);
    }
}

fn shuffle<T>(r: &mut Rng, v: &mut [T]) {
    for i in (1..v.len()).rev() {
        v.swap(i, r.below(i + 1));
    }
}

fn grow_mix(run: &mut Run, r: &mut Rng, target: f64, v: &mut Blocks) {
    while (run.live as f64) < target {
        let s = r.log_size(16, 4096);
        v.push((run.alloc(s), s));
    }
}

/// Linux: anonymous memory the kernel backs with transparent huge pages
/// (`AnonHugePages` of `/proc/self/smaps_rollup`), in bytes; 0 elsewhere.
fn anon_huge_bytes() -> u64 {
    std::fs::read_to_string("/proc/self/smaps_rollup")
        .ok()
        .and_then(|t| {
            t.lines()
                .find(|l| l.starts_with("AnonHugePages:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
        })
        .map_or(0, |kb| kb * 1024)
}

/// The largest `anon_huge_bytes` seen at a live peak (before a free-all).
static ANON_HUGE_PEAK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn free_all(run: &mut Run, v: Blocks) {
    ANON_HUGE_PEAK.fetch_max(anon_huge_bytes(), std::sync::atomic::Ordering::Relaxed);
    let t = run.prof.is_some().then(Instant::now);
    run.teardown = true;
    let n = v.len() as u64;
    for (p, s) in v {
        run.free(p, s);
    }
    run.teardown = false;
    if let (Some(f), Some(t)) = (run.prof.as_mut(), t) {
        f.teardown.0 += t.elapsed().as_nanos() as u64;
        f.teardown.1 += n;
    }
}

fn wild(run: &mut Run, r: &mut Rng, scale: f64, phased: bool) {
    let slots = 50_000usize;
    let ops = (1_000_000.0 * scale) as usize;
    let cap = (WILD_CAP * scale) as usize;
    let mut v: Blocks = vec![(core::ptr::null_mut(), 0); slots];
    for i in 0..ops {
        let phase = i / 100_000;
        let size = |r: &mut Rng| {
            if phased {
                phase_size(r, phase)
            } else {
                wild_size(r)
            }
        };
        let k = r.below(slots);
        let op = if run.live > cap { 70 } else { r.below(100) };
        match op {
            0..=44 => {
                if !v[k].0.is_null() {
                    run.free(v[k].0, v[k].1);
                }
                let s = size(r);
                v[k] = (run.alloc(s), s);
            }
            45..=64 => {
                if v[k].0.is_null() {
                    let s = size(r);
                    v[k] = (run.alloc(s), s);
                }
            }
            65..=84 => {
                if !v[k].0.is_null() {
                    run.free(v[k].0, v[k].1);
                    v[k] = (core::ptr::null_mut(), 0);
                }
            }
            85..=94 => {
                if !v[k].0.is_null() {
                    let s = size(r);
                    v[k] = (run.realloc(v[k].0, v[k].1, s), s);
                }
            }
            _ => burst(run, r),
        }
    }
    free_all(run, v.into_iter().filter(|b| !b.0.is_null()).collect());
}

/// One LIFO burst of `wild`: 1..1000 blocks of 8..128 B (log-uniform), allocated, then
/// freed in reverse. `WJ_PROF` times the two phases as units.
fn burst(run: &mut Run, r: &mut Rng) {
    let n = 1 + r.below(1000);
    let mut b = Vec::with_capacity(n);
    let t0 = run.prof.is_some().then(Instant::now);
    for _ in 0..n {
        let s = r.log_size(8, 128);
        b.push((run.alloc(s), s));
    }
    let t1 = run.prof.is_some().then(Instant::now);
    while let Some((p, s)) = b.pop() {
        run.free(p, s);
    }
    if let (Some(f), Some(t0), Some(t1)) = (run.prof.as_mut(), t0, t1) {
        f.burst_alloc.0 += (t1 - t0).as_nanos() as u64;
        f.burst_alloc.1 += n as u64;
        f.burst_free.0 += t1.elapsed().as_nanos() as u64;
        f.burst_free.1 += n as u64;
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    assert!(
        a.len() >= 4,
        "usage: g1_wild_judge <scenario> <sefer|sefer_pool|mi|sys> <seed> [scale]"
    );
    let arm = match a[2].as_str() {
        "sefer" => Arm::Sefer,
        "sefer_pool" => Arm::SeferPool,
        "mi" => Arm::Mi,
        "sys" => Arm::Sys,
        x => panic!("unknown arm {x}"),
    };
    let seed: u64 = a[3].parse().expect("seed");
    let scale: f64 = a.get(4).map_or(1.0, |s| s.parse().expect("scale"));
    assert!(
        scale > 0.0 && scale <= 2.0,
        "scale {scale} outside (0, 2]: the machine is shared"
    );
    let mut r = Rng(seed | 1);
    let mut run = Run {
        arm,
        ops: 0,
        sum: 0,
        live: 0,
        split: std::env::var_os("WJ_SPLIT").map(|_| [[(0, 0); 2]; 2]),
        prof: std::env::var_os("WJ_PROF").map(|_| Prof::default()),
        teardown: false,
    };
    let t = Instant::now();
    match a[1].as_str() {
        "grow64" => grow_fixed(&mut run, 64, GIB * scale),
        "grow1k" => grow_fixed(&mut run, 1024, GIB * scale),
        "growmix" => {
            let mut v = Vec::new();
            grow_mix(&mut run, &mut r, GIB * scale, &mut v);
            shuffle(&mut r, &mut v);
            free_all(&mut run, v);
        }
        "regrow" => {
            let half = GIB * scale / 2.0;
            let mut v = Vec::new();
            grow_mix(&mut run, &mut r, half, &mut v);
            let mut keep = Vec::with_capacity(v.len());
            for (i, b) in v.into_iter().enumerate() {
                if i % 2 == 1 {
                    run.free(b.0, b.1);
                } else {
                    keep.push(b);
                }
            }
            grow_mix(&mut run, &mut r, half, &mut keep);
            free_all(&mut run, keep);
        }
        "large" => {
            let mut v = Vec::new();
            while (run.live as f64) < GIB * scale {
                let s = r.log_size(64 << 10, 32 << 20);
                v.push((run.alloc(s), s));
            }
            for _ in 0..(5_000.0 * scale) as usize {
                let k = r.below(v.len());
                run.free(v[k].0, v[k].1);
                let s = r.log_size(64 << 10, 32 << 20);
                v[k] = (run.alloc(s), s);
            }
            free_all(&mut run, v);
        }
        "burst" => {
            for _ in 0..(6_000.0 * scale) as usize {
                burst(&mut run, &mut r);
            }
        }
        "wild" => wild(&mut run, &mut r, scale, false),
        "phase" => wild(&mut run, &mut r, scale, true),
        x => panic!("unknown scenario {x}"),
    }
    let wall_ns = t.elapsed().as_nanos();
    let ns = wall_ns as f64 / run.ops as f64;
    if let Some(s) = run.split {
        for (op, name) in ["alloc", "free"].iter().enumerate() {
            for (b, band) in ["<=256K", ">256K"].iter().enumerate() {
                let (t, n) = s[op][b];
                eprintln!(
                    "{name} {band}: n={n} total_ms={} ns_each={}",
                    t / 1_000_000,
                    t.checked_div(n).unwrap_or(0)
                );
            }
        }
    }
    let m = proc_memstat::snapshot();
    // Under `internals`: SeferAlloc's process-wide segment reserve / release
    // counts (the OS traffic of the run; 0 for the other arms).
    #[cfg(feature = "internals")]
    let os = format!(
        ",\"seg_reserved\":{},\"seg_released\":{},\"config_conflicts\":{}",
        sefer_alloc::alloc_core::AllocCore::dbg_segments_reserved_total(),
        sefer_alloc::alloc_core::AllocCore::dbg_segments_released_total(),
        sefer_alloc::registry::config_conflicts_total()
    );
    #[cfg(not(feature = "internals"))]
    let os = String::new();
    // Path activation of TH-1 (Linux): huge-page-backed bytes at the live
    // peak and at the end of the run.
    let os = format!(
        "{os},\"anon_huge_peak\":{},\"anon_huge_end\":{}",
        ANON_HUGE_PEAK
            .load(std::sync::atomic::Ordering::Relaxed)
            .max(anon_huge_bytes()),
        anon_huge_bytes()
    );
    // WILD-2 path-activation oracle (`bench-internals` builds only; counts, not timing).
    #[cfg(feature = "bench-internals")]
    let os = format!(
        "{os},\"lane_second_chance\":{},\"tier1_hits\":{},\"tier1_misses\":{}",
        sefer_alloc::alloc_core::AllocCore::dbg_lane_second_chance_hits(),
        sefer_alloc::alloc_core::AllocCore::dbg_contains_base_tier1_hits(),
        sefer_alloc::alloc_core::AllocCore::dbg_contains_base_tier1_misses()
    );
    // WILD-1 (`WJ_PROF=1`): bucket times in ns, counts, and the wall time of the run.
    let os = match run.prof {
        Some(f) => format!(
            "{os},\"wall_ns\":{},\"prof\":{{\"burst_alloc\":[{},{}],\"burst_free\":[{},{}],\"large_alloc\":[{},{}],\"large_free\":[{},{}],\"large_realloc\":[{},{},{}],\"small_realloc\":[{},{},{}],\"teardown\":[{},{}]}}",
            wall_ns,
            f.burst_alloc.0,
            f.burst_alloc.1,
            f.burst_free.0,
            f.burst_free.1,
            f.large_alloc.0,
            f.large_alloc.1,
            f.large_free.0,
            f.large_free.1,
            f.large_realloc.0,
            f.large_realloc.1,
            f.large_realloc.2,
            f.small_realloc.0,
            f.small_realloc.1,
            f.small_realloc.2,
            f.teardown.0,
            f.teardown.1
        ),
        None => os,
    };
    println!(
        "{{\"scenario\":\"{}\",\"arm\":\"{}\",\"seed\":{seed},\"scale\":{scale},\"ops\":{},\"ns_per_op\":{ns:.3},\"peak_rss\":{},\"end_rss\":{},\"checksum\":{}{os}}}",
        a[1],
        a[2],
        run.ops,
        m.peak_rss.unwrap_or(0),
        m.rss,
        run.sum
    );
    if m.peak_rss.unwrap_or(0) > RSS_LIMIT {
        eprintln!("peak RSS above the 4 GiB limit");
        std::process::exit(2);
    }
}
