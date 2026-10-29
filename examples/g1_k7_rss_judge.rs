//! G1-K7 run 3 (Addendum 6 rule 3, measurement only): process RSS of one
//! workload under one arm, through the real `#[global_allocator]`. One
//! process per (arm, workload, repetition); `run3.sh` spawns them and rotates
//! the arm order. The file is identical in the K5 tree and the K7 tree.
//!
//! Arm, fixed at build time:
//! - `--cfg g1_mi`: mimalloc (reference, not judged);
//! - `--cfg g1_k7`: SeferAlloc with `depot_budget_bytes(E)`, E = build-time
//!   env `G1_K7_E` in bytes (K7 tree only);
//! - neither: `SeferAlloc::new()` (the K5 tree).
//!
//! Usage: `g1_k7_rss_judge <W1|W2|W3|W5i|W5c>`
//! - W1: the oracle's G11 kernel shapes round-robin, `CYCLES` cycles;
//! - W2: bulk64k (65 536 × 64 B live, then freed), `CYCLES` passes;
//! - W3: bulk16k_32 (16 384 × 32 B), `CYCLES` passes;
//! - W5i / W5c: phase change. Fill: bulk16k (64 B) for `W5_FILL` passes (the
//!   K7 overflow saturates: the last `SAT` passes leave its bytes unchanged,
//!   checked by the analysis); P0. Then the 64 B class stops for good:
//!   W5i idles 1 s (P2); W5c runs `W5_EVENTS` rounds of 4096 × 256 B
//!   allocated then freed (P1) — a bulk256 park/reload workload: its
//!   refills come from segments or from the overflow, and both tick the
//!   decay; the number of segment refills is not fixed.
//!
//! Every allocated block gets one byte written, so payload pages are
//! resident in every arm. The workload runs on a worker thread that exits.
//! Snapshots: S1 = the process peak RSS (PeakWorkingSetSize / VmHWM) read at
//! S2, with the overflow's maximum committed record bytes over the per-pass
//! checkpoints (its allowance basis); S2 = RSS after the last free, thread
//! alive; S3 = RSS 1 s after the thread exited (teardown releases the record
//! pages, so its allowance is 0). W5 snapshots are taken with the thread
//! alive. Every live-thread snapshot carries all 11 overflow counters of the
//! worker heap (K7 arm; zeros otherwise). A failed RSS read panics.
//!
//! Output: one JSON line; the statistic and the loss rule live in the
//! analysis script (`scripts/g1_k7_a6_rss.mjs`), not here.

#![allow(unexpected_cfgs)]

#[cfg(all(g1_mi, g1_k7))]
compile_error!("g1_mi and g1_k7 are exclusive arms");

use std::alloc::{alloc, dealloc, Layout};
use std::time::Duration;

#[cfg(g1_mi)]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(g1_k7)]
const E: usize = parse(option_env!("G1_K7_E"));

#[cfg(g1_k7)]
#[global_allocator]
static GLOBAL: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::with_config(
    sefer_alloc::LargeCacheConfig::DEFAULT.depot_budget_bytes(E),
);

#[cfg(not(any(g1_k7, g1_mi)))]
#[global_allocator]
static GLOBAL: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::new();

const CYCLES: u64 = 50;
const W5_FILL: u64 = 64;
const W5_EVENTS: u64 = 50;
const NAMES: [&str; 11] = [
    "ovf_bytes",
    "parks",
    "reloads",
    "budget_refusals",
    "os_refusals",
    "decay_flushed",
    "ingress_checks",
    "decommit_calls",
    "committed",
    "arms",
    "disarms",
];

/// Decimal bytes from a build-time env value; a missing value is a build error.
#[cfg(g1_k7)]
const fn parse(s: Option<&str>) -> usize {
    let b = match s {
        Some(s) => s.as_bytes(),
        None => panic!("g1_k7 build needs G1_K7_E"),
    };
    assert!(!b.is_empty(), "G1_K7_E is empty");
    let mut v = 0usize;
    let mut i = 0;
    while i < b.len() {
        assert!(b[i].is_ascii_digit(), "G1_K7_E must be decimal bytes");
        v = v * 10 + (b[i] - b'0') as usize;
        i += 1;
    }
    v
}

fn arm() -> &'static str {
    if cfg!(g1_mi) {
        "mimalloc"
    } else if cfg!(g1_k7) {
        "k7"
    } else {
        "k5"
    }
}

/// `(requested, resolved, effective)` budget of the calling thread's heap;
/// zeros outside the K7 arm.
#[cfg(g1_k7)]
fn budget() -> (usize, usize, usize) {
    let ((resolved, effective), _) = GLOBAL.dbg_current_depot_overflow().expect("own heap");
    (E, resolved, effective)
}

#[cfg(not(g1_k7))]
fn budget() -> (usize, usize, usize) {
    (0, 0, 0)
}

/// The 11 overflow counters of the calling thread's heap; zeros outside K7.
#[cfg(g1_k7)]
fn ovf() -> [u64; 11] {
    GLOBAL.dbg_current_depot_overflow().expect("own heap").1
}

#[cfg(not(g1_k7))]
fn ovf() -> [u64; 11] {
    [0; 11]
}

fn snap_json(name: &str) -> String {
    let c = ovf();
    let fields: Vec<String> = NAMES
        .iter()
        .zip(c)
        .map(|(n, v)| format!("\"{n}\":{v}"))
        .collect();
    format!(
        "\"{name}\":{{\"rss\":{},{}}}",
        rss::current(),
        fields.join(",")
    )
}

/// Maximum overflow committed record bytes over the checkpoints.
struct Peak(u64);
impl Peak {
    fn check(&mut self) {
        self.0 = self.0.max(ovf()[8]);
    }
}

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

/// Allocates and touches one byte of the block.
fn get(size: usize) -> *mut u8 {
    // SAFETY: non-zero layout; the write is inside the fresh block.
    unsafe {
        let p = alloc(l(size));
        assert!(!p.is_null());
        p.write(1);
        p
    }
}

/// Pointer and size tables, allocated once before the workload.
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
        let p = get(sz);
        // SAFETY: allocated just above with the same layout.
        unsafe { dealloc(p, l(sz)) };
    }
}

fn bulk(slots: &mut Slots, n: usize, size: usize, passes: u64, peak: &mut Peak) {
    for _ in 0..passes {
        for i in 0..n {
            slots.p[i] = get(size);
        }
        for i in 0..n {
            // SAFETY: allocated above with the same layout.
            unsafe { dealloc(slots.p[i], l(size)) };
        }
        peak.check();
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
        slots.p[i] = get(sz);
        slots.s[i] = sz;
    }
    for _ in 0..ops {
        let i = rng.below(live as u64) as usize;
        let sz = size_of();
        // SAFETY: slot i holds a live block of size s[i]; replaced by a new one.
        unsafe { dealloc(slots.p[i], l(slots.s[i])) };
        slots.p[i] = get(sz);
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

fn g11_cycle(slots: &mut Slots, rng: &mut Rng, peak: &mut Peak) {
    pair(16, 512, rng);
    pair(64, 512, rng);
    pair(0, 512, rng);
    bulk(slots, 64, 64, 8, peak);
    bulk(slots, 16384, 64, 1, peak);
    for size in [16, 64, 256] {
        churn(slots, 256, 128, &mut || size, &mut Rng(rng.next() | 1));
    }
    churn(slots, 256, 48, &mut || 1024, &mut Rng(rng.next() | 1));
    vec_grow(24);
    let mut r2 = Rng(rng.next() | 1);
    let mut r3 = Rng(rng.next() | 1);
    churn(
        slots,
        768,
        256,
        &mut || 16 + 16 * r3.below(32) as usize,
        &mut r2,
    );
    peak.check();
}

/// Runs `w` on the calling (worker) thread; returns the live-thread
/// snapshots as JSON fields and the budget read-back.
fn work(w: &str) -> (Vec<String>, (usize, usize, usize)) {
    let mut slots = Slots {
        p: vec![core::ptr::null_mut(); 65536],
        s: vec![0; 65536],
    };
    let mut rng = Rng(0xb411_0001);
    pair(16, 1, &mut rng);
    let b = budget();
    let mut peak = Peak(0);
    let mut out = Vec::new();
    match w {
        "W1" => (0..CYCLES).for_each(|_| g11_cycle(&mut slots, &mut rng, &mut peak)),
        "W2" => bulk(&mut slots, 65536, 64, CYCLES, &mut peak),
        "W3" => bulk(&mut slots, 16384, 32, CYCLES, &mut peak),
        "W5i" | "W5c" => {
            let mut fill = Vec::with_capacity(W5_FILL as usize);
            for _ in 0..W5_FILL {
                bulk(&mut slots, 16384, 64, 1, &mut peak);
                fill.push(ovf()[0].to_string());
            }
            out.push(format!("\"fill_bytes\":[{}]", fill.join(",")));
            out.push(snap_json("P0"));
            if w == "W5i" {
                std::thread::sleep(Duration::from_secs(1));
                out.push(snap_json("P2"));
            } else {
                bulk(&mut slots, 4096, 256, W5_EVENTS, &mut peak);
                out.push(snap_json("P1"));
            }
        }
        _ => panic!("unknown workload {w}"),
    }
    peak.check();
    let s2 = snap_json("S2");
    out.push(format!(
        "\"S1\":{{\"rss\":{},\"committed_max\":{}}}",
        rss::peak(),
        peak.0
    ));
    out.push(s2);
    drop(slots);
    (out, b)
}

fn main() {
    let w = std::env::args()
        .nth(1)
        .expect("usage: g1_k7_rss_judge <W1|W2|W3|W5i|W5c>");
    let c0 = sefer_alloc::registry::config_conflicts_total();
    let rss0 = rss::current();
    let w2 = w.clone();
    let (fields, (req, res, eff)) = std::thread::spawn(move || work(&w2)).join().unwrap();
    std::thread::sleep(Duration::from_secs(1));
    let s3 = rss::current();
    let c1 = sefer_alloc::registry::config_conflicts_total();
    println!(
        "{{\"arm\":\"{}\",\"workload\":\"{w}\",\"budget_requested\":{req},\"budget_resolved\":{res},\"budget_effective\":{eff},\"config_conflicts_delta\":{},\"pid\":{},\"rss_start\":{rss0},{},\"S3\":{{\"rss\":{s3}}}}}",
        arm(),
        c1 - c0,
        std::process::id(),
        fields.join(","),
    );
}

mod rss {
    /// Current process RSS in bytes; panics if it cannot be read.
    pub fn current() -> u64 {
        let v = imp::read().0;
        assert!(v > 0, "RSS read failed");
        v
    }

    /// Process peak RSS in bytes; panics if it cannot be read.
    pub fn peak() -> u64 {
        let v = imp::read().1;
        assert!(v > 0, "peak RSS read failed");
        v
    }

    #[cfg(target_os = "linux")]
    mod imp {
        pub fn read() -> (u64, u64) {
            let text = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
            let field = |k: &str| {
                text.lines()
                    .find_map(|l| l.strip_prefix(k))
                    .and_then(|r| r.split_whitespace().next()?.parse::<u64>().ok())
                    .map_or(0, |kb| kb * 1024)
            };
            (field("VmRSS:"), field("VmHWM:"))
        }
    }

    #[cfg(target_os = "windows")]
    mod imp {
        use std::os::raw::c_ulong;

        #[repr(C)]
        struct Counters {
            cb: c_ulong,
            page_fault_count: c_ulong,
            peak_working_set_size: usize,
            working_set_size: usize,
            rest: [usize; 6],
        }

        #[cfg_attr(target_env = "msvc", link(name = "psapi"))]
        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn GetProcessMemoryInfo(
                process: *mut std::ffi::c_void,
                counters: *mut Counters,
                cb: c_ulong,
            ) -> i32;
        }

        pub fn read() -> (u64, u64) {
            let mut c = Counters {
                cb: core::mem::size_of::<Counters>() as c_ulong,
                page_fault_count: 0,
                peak_working_set_size: 0,
                working_set_size: 0,
                rest: [0; 6],
            };
            let cb = c.cb;
            // SAFETY: Win32 call with a pseudo-handle and a zeroed,
            // correctly sized PROCESS_MEMORY_COUNTERS on the stack.
            let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c, cb) };
            if ok == 0 {
                return (0, 0);
            }
            (c.working_set_size as u64, c.peak_working_set_size as u64)
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    mod imp {
        pub fn read() -> (u64, u64) {
            (0, 0)
        }
    }
}
