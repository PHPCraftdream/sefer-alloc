//! Example: compare `System` vs `mimalloc` using the larson and mstress
//! workloads across a thread-count sweep.
//!
//! Run with:
//!   `cargo run --release -p malloc-bench-rs --example compare`

use malloc_bench_rs::{sweep, Config, Workload};
use std::alloc::System;

fn main() {
    let cfg = Config {
        threads: 4,
        steps_per_thread: 100_000,
        working_set: 512,
        mstress_blocks: 256,
    };
    let thread_sweep = [1usize, 2, 4];

    for &workload in &[Workload::Larson, Workload::Mstress] {
        let name = match workload {
            Workload::Larson => "larson",
            Workload::Mstress => "mstress",
        };
        println!("--- workload: {name} ---");
        println!("{:>3}  {:>16}  {:>16}", "T", "System", "mimalloc");

        let sys_results = sweep(workload, &cfg, &thread_sweep, || System);
        let mi_results = sweep(workload, &cfg, &thread_sweep, || mimalloc::MiMalloc);

        for ((t, sys), (_, mi)) in sys_results.iter().zip(mi_results.iter()) {
            println!("{:>3}  {:>14.2} M  {:>14.2} M", t, sys / 1e6, mi / 1e6,);
        }
        println!();
    }

    println!("(M = million ops/sec. op = one alloc+free pair. Higher is better.)");
}
