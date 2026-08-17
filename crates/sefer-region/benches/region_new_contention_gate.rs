// region_new_contention_gate.rs
//
// Measures Region::new() throughput under multi-threaded contention on the
// shared NEXT_REGION_ID atomic (post-#813 fetch_update mechanism), with two
// baseline arms that separate the two costs conflated by a naive single-baseline
// comparison: (a) cache-line contention on a SHARED atomic, and (b) the cost of
// the fetch_update-based CAS retry loop itself vs. a plain fetch_add (#813
// changed the primitive AND introduced sharing at the same time — a baseline
// that only varies "shared vs local" cannot tell you which one a given delta
// belongs to).
//
// Methodology: barrier-aligned start, fixed work (not fixed-duration), three arms:
//   - shared_atomic: real load — calls Region::<u64>::new() repeatedly (shared
//     NEXT_REGION_ID, fetch_update/CAS-loop primitive)
//   - shared_fetch_add: isolates cache-line contention alone — a SHARED AtomicUsize,
//     but fetch_add (not fetch_update), so the primitive matches baseline_local_atomic
//     and only "shared vs local" varies between this arm and that one
//   - baseline_local_atomic: no contention at all — a thread-LOCAL AtomicUsize with
//     fetch_add, so the primitive matches shared_fetch_add and only "shared vs local"
//     varies between that arm and this one
//
// Reading the three arms together: (shared_fetch_add vs baseline_local_atomic) isolates
// pure cache-line contention cost (same primitive, different sharing); (shared_atomic vs
// shared_fetch_add) isolates the CAS-retry-loop-vs-xadd cost (same sharing, different
// primitive) — exactly what #813 changed. Neither single baseline alone can separate these.
//
// Output format: RAW per-sample CSV first, THEN derived summary.
// This is a gate harness, not a permanent benchmark — it measures a specific
// correctness/performance question for task #827 (extended per the #832 closing
// review's F-C6 finding, which found the original two-arm design conflated the two
// costs above).

#[path = "common/stats.rs"]
mod stats;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
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

    // Arm table: (name, function). Compile-time total coverage — adding a row without
    // a function entry causes a compile error, not a silent fallback.
    #[allow(clippy::type_complexity)]
    const ARM_TABLE: [(&str, fn(usize) -> u64); 3] = [
        ("shared_atomic", run_shared_atomic),
        ("shared_fetch_add", run_shared_fetch_add),
        ("baseline_local_atomic", run_baseline_local_atomic),
    ];

    for (arm_name, arm_fn) in ARM_TABLE {
        for thread_count in THREAD_COUNTS {
            if thread_count > max_threads {
                println!(
                    "# SKIP: arm={}, threads={} (exceeds available_parallelism={})",
                    arm_name, thread_count, max_threads
                );
                continue;
            }

            for sample_idx in 0..SAMPLES {
                let wall_ns = arm_fn(thread_count);

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
    let by_arm_threads = stats::group_by_key(raw_data.iter().map(
        |(arm, threads, _sample, _total_ops, _wall_ns, ops_per_sec)| {
            ((arm.clone(), *threads), *ops_per_sec)
        },
    ));

    // Compute and print summary statistics.
    let mut summary_rows: Vec<(String, usize, f64, f64)> = Vec::new();

    for ((arm, threads), values) in by_arm_threads {
        let (mean, median) = stats::mean_and_median(values);

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

    let find_mean = |name: &str| -> Option<f64> {
        summary_rows
            .iter()
            .find(|(arm, threads, _, _)| arm == name && *threads == max_threads_actual)
            .map(|(_, _, mean, _)| *mean)
    };

    if let (Some(shared_mean), Some(fetch_add_mean), Some(baseline_mean)) = (
        find_mean("shared_atomic"),
        find_mean("shared_fetch_add"),
        find_mean("baseline_local_atomic"),
    ) {
        // Total overhead: real Region::new() vs. no contention at all.
        let overhead_ratio = shared_mean / baseline_mean;
        println!(
            "\noverhead_ratio(threads={}) = shared_atomic.mean / baseline_local_atomic.mean = {:.3}",
            max_threads_actual, overhead_ratio
        );
        assert!(overhead_ratio.is_finite() && overhead_ratio > 0.0);

        // Decomposition (F-C6 fix): separates (a) cache-line contention on a shared
        // atomic (same fetch_add primitive, shared vs. local) from (b) the cost of
        // the fetch_update/CAS-loop primitive itself vs. a plain fetch_add (same
        // sharing regime, different primitive) -- #813 changed BOTH at once, so a
        // single baseline cannot attribute the gap to either one alone.
        let contention_ratio = fetch_add_mean / baseline_mean;
        println!(
            "contention_ratio(threads={}) = shared_fetch_add.mean / baseline_local_atomic.mean = {:.3} (cache-line contention alone, same fetch_add primitive)",
            max_threads_actual, contention_ratio
        );
        assert!(contention_ratio.is_finite() && contention_ratio > 0.0);

        let cas_primitive_ratio = shared_mean / fetch_add_mean;
        println!(
            "cas_primitive_ratio(threads={}) = shared_atomic.mean / shared_fetch_add.mean = {:.3} (fetch_update/CAS-loop cost vs. fetch_add, same sharing regime)",
            max_threads_actual, cas_primitive_ratio
        );
        assert!(cas_primitive_ratio.is_finite() && cas_primitive_ratio > 0.0);
    }
}

/// Run the `shared_fetch_add` arm: isolates cache-line contention from the CAS-loop
/// primitive cost. Same SlotMap-allocation shape as `baseline_local_atomic`, but the
/// AtomicUsize is SHARED across threads (like `shared_atomic`'s NEXT_REGION_ID) and
/// uses a plain `fetch_add` (like `baseline_local_atomic`'s primitive, unlike
/// `shared_atomic`'s `fetch_update`/CAS retry loop). Comparing this arm to
/// `baseline_local_atomic` isolates pure cache-line contention; comparing
/// `shared_atomic` to this arm isolates the CAS-loop-vs-xadd cost alone (see F-C6 in
/// docs/reviews/2026-08-11-sefer-region-f1-f13-perf-closing-review.md).
/// Returns wall_ns: max duration across all threads (time until LAST thread finished).
fn run_shared_fetch_add(thread_count: usize) -> u64 {
    let barrier = Barrier::new(thread_count);
    let shared_counter = Arc::new(AtomicUsize::new(0));
    let mut max_ns = 0u64;

    std::thread::scope(|s| {
        let barrier = &barrier;
        let handles: Vec<_> = (0..thread_count)
            .map(|_| {
                let shared_counter = Arc::clone(&shared_counter);
                s.spawn(move || {
                    barrier.wait();
                    let t0 = Instant::now();

                    for _ in 0..ITERS_PER_THREAD {
                        // (a) RMW operation on a SHARED atomic (real cross-thread
                        // contention), but fetch_add -- not fetch_update/CAS -- so the
                        // primitive matches baseline_local_atomic's, isolating sharing
                        // from primitive choice.
                        shared_counter.fetch_add(1, Ordering::Relaxed);

                        // (b) SlotMap allocation + drop, matching the other two arms.
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
