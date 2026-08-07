//! Contended read probe for `SyncRegion<T>`.
//!
//! Measures one-shot vs batched read performance under multi-threaded contention.
//! Single noisy Windows dev host (release profile). This is a manual measurement
//! tool, not an automated test — numbers will vary by host, but the anti-scaling
//! shape (one-shot throughput drops with thread count) vs batched flat scaling
//! should be reproducible.
//!
//! Usage:
//!   cargo run --release --example contended_reads -p sefer-region

use std::hint::black_box;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

use sefer_region::{Handle, SyncRegion};

fn main() {
    // Configuration: warm region with 1,024 entries
    const POP: usize = 1024;
    const READ_OPS_PER_READER: usize = 1_000_000;
    const BATCH_SIZE: usize = 64;

    println!("Contended read probe: SyncRegion<u64> with {} entries", POP);
    println!("One-shot vs batched ({} gets per read() guard)", BATCH_SIZE);
    println!("Single noisy Windows dev host, release profile\n");

    // Pre-populate a warm region
    let region: SyncRegion<u64> = SyncRegion::with_capacity(POP);
    let mut handles: Vec<Handle<u64>> = Vec::with_capacity(POP);
    for i in 0..POP {
        handles.push(region.insert(i as u64));
    }

    let region = Arc::new(region);
    let handles = Arc::new(handles);

    // Test one-shot reads at various thread counts
    for n_threads in [1, 2, 4, 8] {
        run_one_shot(&region, &handles, n_threads, READ_OPS_PER_READER);
    }

    println!();

    // Test batched reads at various thread counts
    for n_threads in [1, 2, 4, 8] {
        run_batched(
            &region,
            &handles,
            n_threads,
            READ_OPS_PER_READER,
            BATCH_SIZE,
        );
    }
}

fn run_one_shot(
    region: &Arc<SyncRegion<u64>>,
    handles: &Arc<Vec<Handle<u64>>>,
    n_threads: usize,
    ops_per_thread: usize,
) {
    let barrier = Arc::new(Barrier::new(n_threads));

    thread::scope(|s| {
        let mut threads = Vec::new();

        for t in 0..n_threads {
            let region = Arc::clone(region);
            let handles = Arc::clone(handles);
            let barrier = Arc::clone(&barrier);

            threads.push(s.spawn(move || {
                // Decorrelated start offset
                let mut idx = (t * 37) % handles.len();

                barrier.wait();
                let start = Instant::now();

                for _ in 0..ops_per_thread {
                    idx = (idx + 1) % handles.len();
                    let h = handles[idx];
                    black_box(region.get_cloned(black_box(h)));
                }

                start.elapsed()
            }));
        }

        let mut ns_per_op_sum = 0.0;
        let mut max_elapsed_secs: f64 = 0.0;
        for thread in threads {
            let elapsed = thread.join().unwrap();
            ns_per_op_sum += elapsed.as_nanos() as f64 / ops_per_thread as f64;
            max_elapsed_secs = max_elapsed_secs.max(elapsed.as_secs_f64());
        }

        // Threads run concurrently (barrier-aligned start), so wall-clock time for
        // the whole parallel region is the SLOWEST thread's own elapsed time, not
        // the sum of per-thread costs. Aggregate throughput = total ops / that
        // wall-clock time.
        let avg_ns_per_op = ns_per_op_sum / n_threads as f64;
        let total_ops = n_threads * ops_per_thread;
        let aggregate_mops = total_ops as f64 / (max_elapsed_secs * 1_000_000.0);

        println!(
            "one-shot n={}: per-thread ns/op={:.1}  aggregate={:.1} Mops/s",
            n_threads, avg_ns_per_op, aggregate_mops
        );
    });
}

fn run_batched(
    region: &Arc<SyncRegion<u64>>,
    handles: &Arc<Vec<Handle<u64>>>,
    n_threads: usize,
    total_ops_per_thread: usize,
    batch_size: usize,
) {
    let barrier = Arc::new(Barrier::new(n_threads));
    let batches_per_thread = total_ops_per_thread / batch_size;

    thread::scope(|s| {
        let mut threads = Vec::new();

        for t in 0..n_threads {
            let region = Arc::clone(region);
            let handles = Arc::clone(handles);
            let barrier = Arc::clone(&barrier);

            threads.push(s.spawn(move || {
                // Decorrelated start offset
                let mut idx = (t * 37) % handles.len();

                barrier.wait();
                let start = Instant::now();

                for _ in 0..batches_per_thread {
                    // Acquire guard once, do multiple gets
                    let guard = region.read();
                    for _ in 0..batch_size {
                        idx = (idx + 1) % handles.len();
                        let h = handles[idx];
                        black_box(guard.get(black_box(h)));
                    }
                    // Guard dropped here
                }

                start.elapsed()
            }));
        }

        let mut ns_per_get_sum = 0.0;
        let mut max_elapsed_secs: f64 = 0.0;
        for thread in threads {
            let elapsed = thread.join().unwrap();
            ns_per_get_sum += elapsed.as_nanos() as f64 / total_ops_per_thread as f64;
            max_elapsed_secs = max_elapsed_secs.max(elapsed.as_secs_f64());
        }

        // See the matching comment in run_one_shot: wall-clock time for the whole
        // parallel region is the slowest thread's own elapsed time, not a sum.
        let avg_ns_per_get = ns_per_get_sum / n_threads as f64;
        let total_ops = n_threads * total_ops_per_thread;
        let aggregate_mops = total_ops as f64 / (max_elapsed_secs * 1_000_000.0);

        println!(
            "batched n={}: per-thread ns/get={:.1}  aggregate={:.1} Mops/s",
            n_threads, avg_ns_per_get, aggregate_mops
        );
    });
}
