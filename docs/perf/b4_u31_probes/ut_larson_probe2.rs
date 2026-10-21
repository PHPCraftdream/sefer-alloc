// ut_larson_probe2.rs — B4-29/U31 chunk-2 probe: malloc-bench-rs larson/mstress
// at T=1, one allocator, optional B4_* observer deltas (bench-internals).
// Usage: ut_larson_probe2 <sefer|mimalloc> <larson|mstress> <steps> [--counters]
//   --counters prints the B4_* observer deltas (sefer only) as one JSON line;
//   the workload itself runs identically with or without it.
#![allow(unsafe_code)]

use malloc_bench_rs::{run, Config, Workload};

#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn counters() -> [u64; 7] {
    sefer_alloc::SeferAlloc::dbg_b4_collapse_counters()
}

#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
fn counters() -> [u64; 7] {
    [0; 7]
}

#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn u31_counters() -> [u64; 4] {
    sefer_alloc::SeferAlloc::dbg_b4_u31_scan_counters()
}

#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
fn u31_counters() -> [u64; 4] {
    [0; 4]
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let alloc = a.get(1).map(String::as_str).unwrap_or("sefer");
    let wl = match a.get(2).map(String::as_str).unwrap_or("larson") {
        "mstress" => Workload::Mstress,
        _ => Workload::Larson,
    };
    let steps: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let want_counters = a.iter().any(|x| x == "--counters");
    assert!(alloc == "sefer" || alloc == "mimalloc");
    let cfg = Config {
        threads: 1,
        steps_per_thread: steps,
        working_set: 768,
        mstress_blocks: 512,
    };
    let before = if want_counters && alloc == "sefer" {
        Some((counters(), u31_counters()))
    } else {
        None
    };
    let r = match alloc {
        "mimalloc" => run(wl, &cfg, || mimalloc::MiMalloc),
        _ => run(wl, &cfg, sefer_alloc::SeferAlloc::new),
    };
    println!("{alloc} {r:.0}");
    if let Some((before, before_u31)) = before {
        let after = counters();
        let after_u31 = u31_counters();
        let delta = |i: usize| after[i] - before[i];
        println!(
            "{{\"workload\":\"{:}\",\"steps\":{steps},\"find_segment_calls\":{},\"scan_calls\":{},\"scan_words\":{},\"foreign_publish_ok\":{},\"directory_lookups\":{},\"shard_lock_spins\":{},\"shard_lock_yields\":{}}}",
            a.get(2).map(String::as_str).unwrap_or("larson"),
            delta(0),
            delta(1),
            delta(2),
            delta(3),
            delta(4),
            delta(5),
            delta(6),
        );
        let u31_seg = after_u31[0] - before_u31[0];
        let u31_base = after_u31[1] - before_u31[1];
        let u31_nopending = after_u31[2] - before_u31[2];
        let u31_disc = after_u31[3] - before_u31[3];
        println!(
            "{{\"workload\":\"{:}\",\"steps\":{steps},\"u31_scan_segments\":{u31_seg},\"u31_holders_base\":{u31_base},\"u31_scan_no_pending\":{u31_nopending},\"u31_discovery_scans\":{u31_disc}}}",
            a.get(2).map(String::as_str).unwrap_or("larson"),
        );
    }
}
