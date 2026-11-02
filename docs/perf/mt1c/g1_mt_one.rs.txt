//! G1-MT1 probe (measurement only): one malloc-bench-rs workload, one
//! allocator, one thread count, printing aggregate ops/sec. For perf/flamegraph
//! of a single arm.
//!
//! Usage: g1_mt_one <larson|mstress> <sefer|mi|system> <threads> <steps_per_thread>
#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]

use malloc_bench_rs::{run, Config, Workload};
use sefer_alloc::SeferAlloc;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let workload = match a[1].as_str() {
        "larson" => Workload::Larson,
        "mstress" => Workload::Mstress,
        w => panic!("workload {w}"),
    };
    let cfg = Config {
        threads: a[3].parse().expect("threads"),
        steps_per_thread: a[4].parse().expect("steps"),
        working_set: 768,
        mstress_blocks: 512,
    };
    let ops = match a[2].as_str() {
        "sefer" => run(workload, &cfg, SeferAlloc::new),
        "mi" => run(workload, &cfg, || mimalloc::MiMalloc),
        "system" => run(workload, &cfg, || std::alloc::System),
        x => panic!("allocator {x}"),
    };
    println!("{} {} T={} {:.2} Mops", a[1], a[2], cfg.threads, ops / 1e6);
}
