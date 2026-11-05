//! G1 big-block judge (measurement only): single blocks from 1 MiB to several GiB, one allocator arm per process.
//!
//! Usage: g1_big_judge <cycle|edge|grow|many> <sefer|mi|sys> <size_mib> <reps> [k]
//! Prints one JSON line: scenario, arm, size_mib, reps, k, per-rep phase times (`rep`: `[alloc_ns, touch_ns, free_ns]`), `cold_ns`
//! (the first rep, all phases), `warm_ns` (the minimum over reps 2.., or the first rep when `reps` is 1), peak_rss, end_rss, user_ms,
//! kernel_ms, faults, anon_huge_peak (Linux: THP-backed bytes right after a touch phase) and, with `internals`, the process-wide segment reserve / release counts.
//!
//! Scenarios (one rep = the whole sequence, freed again at its end):
//! - `cycle`: allocate `size_mib` MiB, write one byte per 4 KiB page, free.
//! - `edge`: allocate, write only the first and last byte, free (what a lazily committed block costs).
//! - `grow`: `realloc` doubling from 1 MiB to `size_mib` MiB, touching the pages each step adds, free.
//! - `many`: `k` blocks of `size_mib` MiB, every page touched, freed in reverse order.
//!
//! The allocator under test is a static reached through `#[inline(never)]` shims (the `__rust_alloc` boundary); the process global
//! allocator stays `System`. The machine is shared: a run whose peak RSS exceeds [`RSS_LIMIT`] exits with status 2.
use std::alloc::{GlobalAlloc, Layout, System};
use std::time::Instant;

static SEFER: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::new();
static MI: mimalloc::MiMalloc = mimalloc::MiMalloc;

const ALIGN: usize = 8;
const MIB: usize = 1 << 20;
const PAGE: usize = 4096;
/// Hard bound of one run on a shared machine.
const RSS_LIMIT: u64 = 7 << 30;

#[derive(Clone, Copy)]
enum Arm {
    Sefer,
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
            Arm::Mi => MI.realloc(p, l, new),
            Arm::Sys => System.realloc(p, l, new),
        }
    }
}

/// Writes one byte per page of `p[from..to]`; returns the byte sum so the writes stay observable.
fn touch_pages(p: *mut u8, from: usize, to: usize) {
    let mut o = from;
    while o < to {
        // SAFETY: `o < to <= block size`; the block is live and at least `to` bytes long.
        unsafe { p.add(o).write_volatile(1) };
        o += PAGE;
    }
    // SAFETY: the last byte of the range is inside the block.
    unsafe { p.add(to - 1).write_volatile(1) };
}

/// The largest `anon_huge_bytes` sampled right after a touch phase (outside every timed window).
static ANON_HUGE_PEAK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Linux: anonymous memory the kernel backs with transparent huge pages (`AnonHugePages` of `/proc/self/smaps_rollup`), in bytes; 0 elsewhere.
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

fn sample_huge() {
    ANON_HUGE_PEAK.fetch_max(anon_huge_bytes(), std::sync::atomic::Ordering::Relaxed);
}

fn ns(t: Instant) -> u64 {
    t.elapsed().as_nanos() as u64
}

/// One rep; returns `[alloc_ns, touch_ns, free_ns]`.
fn once(arm: Arm, scen: &str, size: usize, k: usize) -> [u64; 3] {
    let (mut a, mut t, mut f) = (0u64, 0u64, 0u64);
    match scen {
        "cycle" | "edge" => {
            let s = Instant::now();
            let p = a_alloc(arm, size);
            a += ns(s);
            assert!(!p.is_null(), "allocation of {size} bytes failed");
            let s = Instant::now();
            if scen == "cycle" {
                touch_pages(p, 0, size);
            } else {
                // SAFETY: first and last byte of a live block of `size` bytes.
                unsafe {
                    p.write_volatile(1);
                    p.add(size - 1).write_volatile(1);
                }
            }
            t += ns(s);
            sample_huge();
            let s = Instant::now();
            a_dealloc(arm, p, size);
            f += ns(s);
        }
        "grow" => {
            let mut cur = MIB.min(size);
            let s = Instant::now();
            let mut p = a_alloc(arm, cur);
            a += ns(s);
            assert!(!p.is_null(), "allocation of {cur} bytes failed");
            let s = Instant::now();
            touch_pages(p, 0, cur);
            t += ns(s);
            while cur < size {
                let new = (cur * 2).min(size);
                let s = Instant::now();
                p = a_realloc(arm, p, cur, new);
                a += ns(s);
                assert!(!p.is_null(), "reallocation to {new} bytes failed");
                let s = Instant::now();
                touch_pages(p, cur, new);
                t += ns(s);
                cur = new;
            }
            sample_huge();
            let s = Instant::now();
            a_dealloc(arm, p, cur);
            f += ns(s);
        }
        "many" => {
            let mut v = Vec::with_capacity(k);
            for _ in 0..k {
                let s = Instant::now();
                let p = a_alloc(arm, size);
                a += ns(s);
                assert!(!p.is_null(), "allocation of {size} bytes failed");
                let s = Instant::now();
                touch_pages(p, 0, size);
                t += ns(s);
                v.push(p);
            }
            sample_huge();
            while let Some(p) = v.pop() {
                let s = Instant::now();
                a_dealloc(arm, p, size);
                f += ns(s);
            }
        }
        x => panic!("unknown scenario {x}"),
    }
    [a, t, f]
}

/// Process CPU split (ms) and page faults at the end of the run; zeros where unsupported.
#[cfg(windows)]
fn cpu_and_faults() -> (u64, u64, u64) {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        lo: u32,
        hi: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct MemCounters {
        cb: u32,
        page_fault_count: u32,
        rest: [usize; 8],
    }
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessTimes(
            h: isize,
            c: *mut FileTime,
            e: *mut FileTime,
            k: *mut FileTime,
            u: *mut FileTime,
        ) -> i32;
        fn K32GetProcessMemoryInfo(h: isize, p: *mut MemCounters, cb: u32) -> i32;
    }
    let ms = |t: &FileTime| ((u64::from(t.hi) << 32) | u64::from(t.lo)) / 10_000;
    let (mut c, mut e, mut k, mut u) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    let mut m = MemCounters {
        cb: std::mem::size_of::<MemCounters>() as u32,
        ..MemCounters::default()
    };
    // SAFETY: plain Win32 queries on the current process with valid out-pointers.
    unsafe {
        let h = GetCurrentProcess();
        if GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) == 0
            || K32GetProcessMemoryInfo(h, &mut m, m.cb) == 0
        {
            return (0, 0, 0);
        }
    }
    (ms(&u), ms(&k), u64::from(m.page_fault_count))
}

#[cfg(target_os = "linux")]
fn cpu_and_faults() -> (u64, u64, u64) {
    let Ok(t) = std::fs::read_to_string("/proc/self/stat") else {
        return (0, 0, 0);
    };
    let Some(rest) = t.rsplit_once(')').map(|(_, r)| r) else {
        return (0, 0, 0);
    };
    let f: Vec<u64> = rest
        .split_whitespace()
        .map(|x| x.parse::<u64>().unwrap_or(0))
        .collect();
    // stat fields 10 (minflt), 14 (utime), 15 (stime); `rest` starts at field 3; ticks are 10 ms.
    (
        f.get(11).copied().unwrap_or(0) * 10,
        f.get(12).copied().unwrap_or(0) * 10,
        f.get(7).copied().unwrap_or(0),
    )
}

#[cfg(not(any(windows, target_os = "linux")))]
fn cpu_and_faults() -> (u64, u64, u64) {
    (0, 0, 0)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    assert!(
        a.len() >= 5,
        "usage: g1_big_judge <cycle|edge|grow|many> <sefer|mi|sys> <size_mib> <reps> [k]"
    );
    let arm = match a[2].as_str() {
        "sefer" => Arm::Sefer,
        "mi" => Arm::Mi,
        "sys" => Arm::Sys,
        x => panic!("unknown arm {x}"),
    };
    let size_mib: usize = a[3].parse().expect("size_mib");
    let reps: usize = a[4].parse().expect("reps");
    let k: usize = a.get(5).map_or(1, |x| x.parse().expect("k"));
    assert!(reps >= 1 && size_mib >= 1 && k >= 1);
    let size = size_mib * MIB;
    let mut rep = Vec::with_capacity(reps);
    for _ in 0..reps {
        rep.push(once(arm, &a[1], size, k));
    }
    let total = |r: &[u64; 3]| r[0] + r[1] + r[2];
    let cold = total(&rep[0]);
    let warm = rep.iter().skip(1).map(total).min().unwrap_or(cold);
    let m = proc_memstat::snapshot();
    let (user_ms, kernel_ms, faults) = cpu_and_faults();
    #[cfg(feature = "internals")]
    let os = format!(
        ",\"seg_reserved\":{},\"seg_released\":{}",
        sefer_alloc::alloc_core::AllocCore::dbg_segments_reserved_total(),
        sefer_alloc::alloc_core::AllocCore::dbg_segments_released_total()
    );
    #[cfg(not(feature = "internals"))]
    let os = String::new();
    let reps_json: Vec<String> = rep
        .iter()
        .map(|r| format!("[{},{},{}]", r[0], r[1], r[2]))
        .collect();
    println!(
        "{{\"scenario\":\"{}\",\"arm\":\"{}\",\"size_mib\":{size_mib},\"reps\":{reps},\"k\":{k},\"cold_ns\":{cold},\"warm_ns\":{warm},\"rep\":[{}],\"peak_rss\":{},\"end_rss\":{},\"user_ms\":{user_ms},\"kernel_ms\":{kernel_ms},\"faults\":{faults},\"anon_huge_peak\":{}{os}}}",
        a[1],
        a[2],
        reps_json.join(","),
        m.peak_rss.unwrap_or(0),
        m.rss,
        ANON_HUGE_PEAK.load(std::sync::atomic::Ordering::Relaxed)
    );
    if m.peak_rss.unwrap_or(0) > RSS_LIMIT {
        eprintln!("peak RSS above the 7 GiB limit");
        std::process::exit(2);
    }
}
