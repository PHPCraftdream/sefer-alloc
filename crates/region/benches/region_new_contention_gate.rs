// region_new_contention_gate.rs
//
// Measures Region::new() throughput under multi-threaded contention on the
// shared NEXT_REGION_ID atomic (post-#813 fetch_update mechanism), with a
// baseline arm that isolates contention cost from the rest of Region::new().
//
// Methodology: barrier-aligned start, fixed work (not fixed-duration), two arms:
//   - shared_atomic: real load — calls Region::<u64>::new() repeatedly
//   - baseline_local_atomic: approximates Region::new() without contention
//     (one local AtomicUsize fetch_add + one SlotMap allocation per iteration)
//
// Output format: RAW per-sample CSV first, THEN derived summary.
// This is a gate harness, not a permanent benchmark — it measures a specific
// correctness/performance question for task #827.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Barrier;
use std::time::Instant;

use sefer_region::Region;
use slotmap::{DefaultKey, SlotMap};

// Roughly 100-300ms per sample on a single thread on modern hardware.
// Chosen empirically: 200k iterations of Region::new() + drop takes ~100-150ms
// on a typical dev machine, giving enough signal-to-noise for contention detection
// while keeping total runtime reasonable (5 samples × 2 arms × 4 thread counts × ~100ms).
const ITERS_PER_THREAD: u64 = 200_000;

const SAMPLES: usize = 5;

// Thread counts to sweep, capped by available_parallelism().
const THREAD_COUNTS: [usize; 4] = [1, 2, 4, 8];

fn main() {
    let max_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    // Collect all raw data first, then print and compute summary.
    // Raw sample: (arm_name, thread_count, sample_index, total_ops, wall_ns, ops_per_sec)
    let mut raw_data: Vec<(String, usize, usize, u64, u64, f64)> = Vec::new();

    for &arm_name in &["shared_atomic", "baseline_local_atomic"] {
        for thread_count in THREAD_COUNTS {
            if thread_count > max_threads {
                println!(
                    "# SKIP: arm={}, threads={} (exceeds available_parallelism={})",
                    arm_name, thread_count, max_threads
                );
                continue;
            }

            for sample_idx in 0..SAMPLES {
                let wall_ns = if arm_name == "shared_atomic" {
                    run_shared_atomic(thread_count)
                } else {
                    run_baseline_local_atomic(thread_count)
                };

                let total_ops = (thread_count as u64) * ITERS_PER_THREAD;
                let ops_per_sec = total_ops as f64 / (wall_ns as f64 / 1e9);

                raw_data.push((
                    arm_name.to_string(),
                    thread_count,
                    sample_idx,
                    total_ops,
                    wall_ns,
                    ops_per_sec,
                ));
            }
        }
    }

    // ========== Raw CSV output (BEFORE any summary/prose) ==========
    println!("# raw_csv,arm,threads,sample,total_ops,wall_ns,ops_per_sec");
    for (arm, threads, sample, total_ops, wall_ns, ops_per_sec) in &raw_data {
        println!(
            "raw_csv,{},{},{},{},{},{}",
            arm, threads, sample, total_ops, wall_ns, ops_per_sec
        );
    }

    // ========== Summary (derived from raw samples above) ==========
    println!("\n=== Summary (derived from raw samples above) ===\n");

    // Group samples by (arm, thread_count) for summary statistics.
    let mut by_arm_threads: HashMap<(String, usize), Vec<f64>> = HashMap::new();

    for (arm, threads, _sample, _total_ops, _wall_ns, ops_per_sec) in &raw_data {
        by_arm_threads
            .entry((arm.clone(), *threads))
            .or_default()
            .push(*ops_per_sec);
    }

    // Compute and print summary statistics.
    let mut summary_rows: Vec<(String, usize, f64, f64)> = Vec::new();

    for ((arm, threads), mut values) in by_arm_threads {
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let median = values[values.len() / 2];

        assert!(mean.is_finite() && mean > 0.0);
        assert!(median.is_finite() && median > 0.0);

        println!(
            "arm={:<20} threads={}  mean_ops_per_sec={:.0} median_ops_per_sec={:.0}",
            arm, threads, mean, median
        );

        summary_rows.push((arm, threads, mean, median));
    }

    // Compute overhead ratio at max thread count.
    let max_threads_actual = THREAD_COUNTS
        .iter()
        .filter(|&&n| n <= max_threads)
        .max()
        .copied()
        .unwrap();

    if let (Some(&shared_mean), Some(&baseline_mean)) = (
        summary_rows
            .iter()
            .find(|(arm, threads, _, _)| arm == "shared_atomic" && *threads == max_threads_actual)
            .map(|(_, _, mean, _)| mean),
        summary_rows
            .iter()
            .find(|(arm, threads, _, _)| {
                arm == "baseline_local_atomic" && *threads == max_threads_actual
            })
            .map(|(_, _, mean, _)| mean),
    ) {
        let overhead_ratio = shared_mean / baseline_mean;
        println!(
            "\noverhead_ratio(threads={}) = shared_atomic.mean / baseline_local_atomic.mean = {:.3}",
            max_threads_actual, overhead_ratio
        );

        assert!(overhead_ratio.is_finite() && overhead_ratio > 0.0);
    }
}

/// Run the `shared_atomic` arm: repeatedly construct and drop Region<u64>.
/// This exercises the real shared NEXT_REGION_ID atomic with contention.
/// Returns wall_ns: max duration across all threads (time until LAST thread finished).
fn run_shared_atomic(thread_count: usize) -> u64 {
    let barrier = Barrier::new(thread_count);
    let mut max_ns = 0u64;

    std::thread::scope(|s| {
        let handles: Vec<_> = (0..thread_count)
            .map(|_| {
                s.spawn(|| {
                    barrier.wait();
                    let t0 = Instant::now();

                    for _ in 0..ITERS_PER_THREAD {
                        let r: Region<u64> = Region::new();
                        std::hint::black_box(&r);
                    }

                    t0.elapsed().as_nanos() as u64
                })
            })
            .collect();

        for handle in handles {
            let ns = handle.join().unwrap();
            max_ns = max_ns.max(ns);
        }
    });

    max_ns
}

/// Run the `baseline_local_atomic` arm: approximate Region::new() without contention.
///
/// This arm isolates the cost of cross-thread contention on NEXT_REGION_ID from the
/// rest of Region::new()'s work (SlotMap allocation, handle minting, etc.). It performs:
///   (a) one fetch_add(1) on a LOCAL, thread-private AtomicUsize (no cross-thread contention)
///       — same RMW pattern as the real arm, but no cache-line ping-pong
///   (b) one SlotMap::<DefaultKey, u64>::new() allocation and immediate drop
///       — matches the primary allocation cost inside Region::new()
///
/// This is NOT a perfect structural copy of Region::new() — it doesn't reproduce the
/// full Region initialization sequence, but it approximates the dominant work that
/// is NOT the shared atomic RMW. The delta between `shared_atomic` and this baseline
/// is therefore a reasonable estimate of contention cost on NEXT_REGION_ID.
///
/// Returns wall_ns: max duration across all threads (time until LAST thread finished).
fn run_baseline_local_atomic(thread_count: usize) -> u64 {
    let barrier = Barrier::new(thread_count);
    let mut max_ns = 0u64;

    std::thread::scope(|s| {
        let handles: Vec<_> = (0..thread_count)
            .map(|_| {
                s.spawn(|| {
                    // Local atomic, NOT shared — each thread gets its own.
                    let local_counter = AtomicUsize::new(0);

                    barrier.wait();
                    let t0 = Instant::now();

                    for _ in 0..ITERS_PER_THREAD {
                        // (a) RMW operation on local atomic (no contention)
                        local_counter.fetch_add(1, Ordering::Relaxed);

                        // (b) SlotMap allocation + drop (matches Region::new()'s main cost)
                        let sm: SlotMap<DefaultKey, u64> = SlotMap::new();
                        std::hint::black_box(&sm);
                    }

                    t0.elapsed().as_nanos() as u64
                })
            })
            .collect();

        for handle in handles {
            let ns = handle.join().unwrap();
            max_ns = max_ns.max(ns);
        }
    });

    max_ns
}
