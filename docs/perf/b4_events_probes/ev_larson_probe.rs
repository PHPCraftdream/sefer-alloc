// Callgrind probe: one malloc-bench-rs workload, T=1, for one allocator.
use malloc_bench_rs::{run, Config, Workload};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let alloc = a.get(1).map(String::as_str).unwrap_or("sefer");
    let wl = match a.get(2).map(String::as_str).unwrap_or("larson") { "mstress" => Workload::Mstress, _ => Workload::Larson };
    let steps: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let cfg = Config { threads: 1, steps_per_thread: steps, working_set: 768, mstress_blocks: 512 };
    let r = match alloc { "mimalloc" => run(wl, &cfg, || mimalloc::MiMalloc), _ => run(wl, &cfg, sefer_alloc::SeferAlloc::new) };
    println!("{alloc} {r:.0}");
}
