//! B4-25 ceiling probe (`docs/perf/B4_25_REMOTE_CEILING.md`): one
//! malloc-bench-rs workload at T threads under one allocator, for callgrind.
//! Usage: `b425_mt_probe <sefer|mimalloc> <mstress|larson> <steps> <threads>`.
use malloc_bench_rs::{run, Config, Workload};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let alloc = a.get(1).map_or("sefer", String::as_str);
    let wl = match a.get(2).map_or("mstress", String::as_str) {
        "larson" => Workload::Larson,
        _ => Workload::Mstress,
    };
    let steps: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(50_000);
    let threads: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(1);
    let cfg = Config {
        threads,
        steps_per_thread: steps,
        working_set: 768,
        mstress_blocks: 512,
    };
    let r = match alloc {
        "mimalloc" => run(wl, &cfg, || mimalloc::MiMalloc),
        _ => run(wl, &cfg, sefer_alloc::SeferAlloc::new),
    };
    println!("{alloc} {r:.0}");
}
