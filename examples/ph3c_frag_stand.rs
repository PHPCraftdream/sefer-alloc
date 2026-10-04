//! Ph3c step-1′ fragmentation diagnostic stand (single binary, 1 thread).
//!
//! Installs `SeferAlloc` as the real `#[global_allocator]` (`Profile::default()`,
//! no env overrides) and drives one of four deterministic fill/churn workloads
//! (`w1`..`w4`, first CLI argument) to characterize resident-memory
//! fragmentation of the small-class path. This is DIAGNOSTIC evidence only —
//! not a deciding gate measurement; the phase-1′ runner parameterizes tree
//! paths around it. See
//! `docs/design/2026-10-02-adr-addendum-ph3c-step1prime.md` §4.
//!
//! Emits `RESULT key=value` lines (same protocol as `paired_ab_*` examples):
//! `rss_empty_kib` / `rss_end_kib` (Linux `/proc/self/status` `VmRSS`),
//! `commit_empty_kib` / `commit_end_kib` (`proc_probe::snapshot()`), oracle
//! counters, and `live_requested_bytes`. Deterministic: fixed-seed LCG, no
//! hash-map iteration anywhere.
//!
//! **Build:** `cargo build --release --example ph3c_frag_stand --features "production internals bench-internals"`

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

use std::alloc::Layout;

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;
const TARGET_LIVE: usize = 64 * MIB;
const W3_ITERS: u64 = 10_000_000;
/// Same alignment for every block, like real small-class traffic.
const ALIGN: usize = 16;

// Small size classes, copied literally from the compile-time-built
// `SIZE_CLASS_TABLE` (`src/alloc_core/platform/size_classes.rs`: MIN_BLOCK=16,
// growth 5/4 rounded up to a multiple of 16, plus the `EXTRAS` block 256..16384;
// the table is `pub(crate)`, hence the literal copy). W1 = classes in [16,128],
// W2 = classes in [16,2048], W3 adds classes up to 4096.
const W1_SIZES: &[usize] = &[16, 32, 48, 64, 80, 112];
const W2_SIZES: &[usize] = &[
    16, 32, 48, 64, 80, 112, 144, 192, 240, 256, 304, 384, 480, 512, 608, 768, 960, 1024, 1200,
    1504, 1888, 2048,
];
const W3_CLASSES: &[usize] = &[
    16, 32, 48, 64, 80, 112, 144, 192, 240, 256, 304, 384, 480, 512, 608, 768, 960, 1024, 1200,
    1504, 1888, 2048, 2368, 2960, 3712, 4096,
];

// --- deterministic LCG (fixed seed, drives ALL of W3's op order) -----------
struct Lcg(u64);

impl Lcg {
    const fn new() -> Self {
        Self(0x9E37_79B9_7F4A_7C15)
    }
    #[inline]
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
    /// Top 32 bits (LCG low bits are weak) as the uniform draw.
    #[inline]
    fn below(&mut self, n: u64) -> u64 {
        (self.next() >> 32) % n
    }
}

/// Log-uniform requested size in [16, 4096]: uniform octave, uniform offset
/// inside the octave, rounded DOWN to the nearest actual size class.
fn log_uniform_size(rng: &mut Lcg) -> usize {
    const LO: u64 = 4; // 16 = 2^4
    const HI: u64 = 12; // 4096 = 2^12
    let exp = LO + rng.below(HI - LO + 1);
    let base: u64 = 1 << exp;
    let raw = base + rng.below(base); // uniform within the octave
    let mut idx = W3_CLASSES.len() - 1;
    while W3_CLASSES[idx] as u64 > raw {
        idx -= 1;
    }
    W3_CLASSES[idx]
}

// --- alloc/dealloc primitives (route through the installed global allocator) -

fn alloc_block(size: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    // SAFETY: non-zero size, power-of-two alignment <= 16.
    let p = unsafe { std::alloc::alloc(layout) };
    assert!(!p.is_null(), "alloc({size}) failed — stand is invalid");
    // SAFETY: `p` covers at least `size >= 1` bytes.
    unsafe { std::ptr::write_volatile(p, 0xA5u8) };
    p
}

fn free_block(p: *mut u8, size: usize) {
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    // SAFETY: `p` was allocated with exactly this layout and is freed once.
    unsafe { std::alloc::dealloc(p, layout) };
}

// --- RSS from /proc/self/status VmRSS (protocol-literal); stub elsewhere ----

#[cfg(target_os = "linux")]
fn vm_rss_kib() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<u64>()
                .unwrap_or(0);
        }
    }
    0
}
#[cfg(not(target_os = "linux"))]
fn vm_rss_kib() -> u64 {
    0 // non-Linux host: protocol-literal VmRSS unavailable
}

fn commit_kib() -> u64 {
    proc_probe::snapshot().charged_or_reserved_bytes() / 1024
}

// --- workloads ---------------------------------------------------------------

fn run_w1() -> usize {
    let mut live_bytes = 0usize;
    let mut i = 0usize;
    while live_bytes < TARGET_LIVE {
        let size = W1_SIZES[i % W1_SIZES.len()];
        alloc_block(size);
        live_bytes += size;
        i += 1;
    }
    live_bytes
}

fn run_w2() -> usize {
    let mut live_bytes = 0usize;
    let mut i = 0usize;
    while live_bytes < TARGET_LIVE {
        let size = W2_SIZES[i % W2_SIZES.len()];
        alloc_block(size);
        live_bytes += size;
        i += 1;
    }
    live_bytes
}

/// Fill to ~64 MiB, then exactly `W3_ITERS` free-of-random-live + alloc steps,
/// tracked precisely via `Vec<(ptr, size)>` + `swap_remove`.
fn run_w3() -> usize {
    let mut rng = Lcg::new();
    let mut live: Vec<(*mut u8, usize)> = Vec::with_capacity(TARGET_LIVE / 32);
    let mut live_bytes = 0usize;
    while live_bytes < TARGET_LIVE {
        let size = log_uniform_size(&mut rng);
        live.push((alloc_block(size), size));
        live_bytes += size;
    }
    for _ in 0..W3_ITERS {
        let idx = rng.below(live.len() as u64) as usize;
        let (p, size) = live.swap_remove(idx);
        free_block(p, size);
        live_bytes -= size;
        let nsize = log_uniform_size(&mut rng);
        live.push((alloc_block(nsize), nsize));
        live_bytes += nsize;
    }
    live_bytes
}

/// Same sizes/order as W2, but grouped: every block of class 1, then class 2, …
fn run_w4() -> usize {
    // Grouped fill: full passes per class until the 64 MiB budget is met.
    let mut live_bytes = 0usize;
    for &size in W2_SIZES {
        let mut cls_bytes = 0usize;
        while cls_bytes < TARGET_LIVE / W2_SIZES.len() {
            alloc_block(size);
            cls_bytes += size;
        }
        live_bytes += cls_bytes;
    }
    // Top up with the largest class if rounding left us short.
    let last = *W2_SIZES.last().unwrap();
    while live_bytes < TARGET_LIVE {
        alloc_block(last);
        live_bytes += last;
    }
    live_bytes
}

fn main() {
    let mut args = std::env::args();
    let _bin = args.next();
    let wl = args.next();
    let workload = match wl.as_deref() {
        Some(w @ ("w1" | "w2" | "w3" | "w4")) => w,
        _ => {
            eprintln!("usage: ph3c_frag_stand <w1|w2|w3|w4>");
            std::process::exit(2);
        }
    };

    // Baselines AFTER allocator install + config readback attempt, BEFORE load.
    proc_probe::emit("workload", workload);
    let rss_empty = vm_rss_kib();
    let commit_empty = commit_kib();
    let stats_empty = GLOBAL.stats();
    proc_probe::emit_u64("rss_empty_kib", rss_empty);
    proc_probe::emit_u64("commit_empty_kib", commit_empty);
    proc_probe::emit_u64("config_conflicts_before", stats_empty.config_conflicts);
    // No public API reads back the resolved profile/config — reported as
    // unavailable (src must not change for this stand).
    proc_probe::emit("resolved_config", "unavailable");

    let live = match workload {
        "w1" => run_w1(),
        "w2" => run_w2(),
        "w3" => run_w3(),
        _ => run_w4(),
    };

    let stats_end = GLOBAL.stats();
    assert_eq!(
        stats_end.config_conflicts - stats_empty.config_conflicts,
        0,
        "config_conflicts delta must be zero"
    );
    proc_probe::emit_u64("live_requested_bytes", live as u64);
    proc_probe::emit_u64("rss_end_kib", vm_rss_kib());
    proc_probe::emit_u64("commit_end_kib", commit_kib());
    proc_probe::emit_u64("segments_reserved_total", stats_end.segments_reserved_total);
    proc_probe::emit_u64("config_conflicts_after", stats_end.config_conflicts);
    if workload == "w3" {
        proc_probe::emit_u64("ops_w3", W3_ITERS);
    }
    // Leak deliberately: live blocks are raw pointers; the process exits.
    std::process::exit(0);
}
