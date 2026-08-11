// r828_batch_guard_probe.rs
//
// Compares three access patterns for SyncRegion batch reads:
//   (a) one-shot: fresh read() call per lookup
//   (b) manual guard-hold: single guard across all lookups
//   (c) closure wrapper: throwaway with_read-style wrapper over manual guard-hold
//
// Motivation: SEFER_REGION_BATCH_READ_API_DESIGN.md's open question 2 —
// does the closure form add measurable overhead, or only ergonomics?
// Expected: (b) and (c) should be within noise; (a) should be ~30x slower.
//
// Output format: RAW per-sample CSV first, THEN derived summary.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use sefer_region::{Region, SyncRegion};

// Fixed-work constants (same N=64 lookups as original audit measurement)
const LOOKUP_COUNT: usize = 64;
const ITERATIONS_PER_SAMPLE: usize = 10_000;
const SAMPLES: usize = 5;

// For concurrent reader arm (8 readers, matching original audit's contention level)
const CONCURRENT_READERS: usize = 8;
const CONCURRENT_ITERATIONS_PER_SAMPLE: usize = 2_000;

// Throwaway closure wrapper defined entirely within this bench file
fn with_read<T, R>(sr: &SyncRegion<T>, f: impl FnOnce(&Region<T>) -> R) -> R {
    f(&sr.read())
}

fn main() {
    // Set up a shared SyncRegion with LOOKUP_COUNT pre-inserted values
    let sr = Arc::new(SyncRegion::new());
    let handles: Vec<sefer_region::Handle<u64>> = (0..LOOKUP_COUNT)
        .map(|i| sr.write().insert(i as u64))
        .collect();

    // Collect all raw data first, then print and compute summary.
    // Raw sample: (arm_name, reader_count, sample_idx, time_ns_per_operation)
    let mut raw_data: Vec<(String, usize, usize, f64)> = Vec::new();

    // Single-threaded arms
    let reader_count = 1;

    for sample_idx in 0..SAMPLES {
        // Arm (a): one-shot per lookup
        let time_ns = run_one_shot(&sr, &handles);
        raw_data.push(("one_shot".to_string(), reader_count, sample_idx, time_ns));

        // Arm (b): manual guard-hold
        let time_ns = run_manual_guard(&sr, &handles);
        raw_data.push((
            "manual_guard".to_string(),
            reader_count,
            sample_idx,
            time_ns,
        ));

        // Arm (c): closure wrapper
        let time_ns = run_closure_wrapper(&sr, &handles);
        raw_data.push((
            "closure_wrapper".to_string(),
            reader_count,
            sample_idx,
            time_ns,
        ));
    }

    // Concurrent arm: 8 readers measuring arm (b) under contention
    let concurrent_reader_count = CONCURRENT_READERS;

    for sample_idx in 0..SAMPLES {
        let time_ns = run_concurrent_manual_guard(Arc::clone(&sr), &handles);
        raw_data.push((
            "concurrent_manual_guard".to_string(),
            concurrent_reader_count,
            sample_idx,
            time_ns,
        ));
    }

    // ========== Raw CSV output (BEFORE any summary/prose) ==========
    println!("# raw_csv,arm,readers,sample,time_ns_per_op");
    for (arm, readers, sample, time_ns) in &raw_data {
        println!("raw_csv,{},{},{},{}", arm, readers, sample, time_ns);
    }

    // ========== Summary (derived from raw samples above) ==========
    println!("\n=== Summary (derived from raw samples above) ===\n");

    // Group samples by (arm, reader_count) for summary statistics.
    let mut by_arm_readers: HashMap<(String, usize), Vec<f64>> = HashMap::new();

    for (arm, readers, _sample, time_ns) in &raw_data {
        by_arm_readers
            .entry((arm.clone(), *readers))
            .or_default()
            .push(*time_ns);
    }

    // Compute and print summary statistics.
    println!(
        "## Single-threaded (1 reader): time per lookup (N = {} lookups per iteration)\n",
        LOOKUP_COUNT
    );
    println!(
        "{:<25} | {:<12} | {:<12} | {:<12}",
        "arm", "mean (ns)", "median (ns)", "ratio vs baseline"
    );
    println!("{}", "-".repeat(70));

    let mut manual_guard_mean: Option<f64> = None;
    let mut closure_mean: Option<f64> = None;
    let mut closure_median: Option<f64> = None;
    let mut one_shot_mean: Option<f64> = None;

    for ((arm, readers), values) in &mut by_arm_readers {
        if *readers != 1 {
            continue;
        }

        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let median = values[values.len() / 2];

        assert!(mean.is_finite() && mean > 0.0);
        assert!(median.is_finite() && median > 0.0);

        match arm.as_str() {
            "manual_guard" => {
                manual_guard_mean = Some(mean);
                println!(
                    "{:<25} | {:<12.1} | {:<12.1} | 1.00x (baseline)",
                    arm, mean, median
                );
            }
            "closure_wrapper" => {
                closure_mean = Some(mean);
                closure_median = Some(median);
                let ratio = mean / manual_guard_mean.unwrap_or(1.0);
                println!(
                    "{:<25} | {:<12.1} | {:<12.1} | {:.3}x vs baseline",
                    arm, mean, median, ratio
                );
            }
            "one_shot" => {
                one_shot_mean = Some(mean);
                let ratio = mean / manual_guard_mean.unwrap_or(1.0);
                println!(
                    "{:<25} | {:<12.1} | {:<12.1} | {:.2}x vs baseline",
                    arm, mean, median, ratio
                );
            }
            _ => {}
        }

        assert!(mean.is_finite() && mean > 0.0);
    }

    // Verify closure wrapper hypothesis
    if let (Some(mg_mean), Some(cw_mean), Some(_cw_median)) =
        (manual_guard_mean, closure_mean, closure_median)
    {
        let ratio = cw_mean / mg_mean;
        println!(
            "\n>>> Closure wrapper overhead check: {:.3}x vs manual guard",
            ratio
        );
        if ratio > 1.1 {
            println!(
                "    WARNING: Closure wrapper shows >10% overhead (unexpected, should inline)"
            );
        } else {
            println!("    PASS: Within noise (expected)");
        }
    }

    // Verify one-shot penalty
    if let (Some(mg_mean), Some(os_mean)) = (manual_guard_mean, one_shot_mean) {
        let penalty = os_mean / mg_mean;
        println!(
            "    One-shot penalty check: {:.2}x vs manual guard",
            penalty
        );
        if penalty < 10.0 {
            println!("    WARNING: One-shot penalty <10x (expected ~30x per original audit)");
        } else {
            println!("    PASS: Substantial penalty confirmed (expected)");
        }
    }

    println!(
        "\n## Concurrent ({} readers): manual guard under contention\n",
        CONCURRENT_READERS
    );
    println!(
        "{:<25} | {:<12} | {:<12}",
        "arm", "mean (ns)", "median (ns)"
    );
    println!("{}", "-".repeat(55));

    for ((arm, readers), values) in &mut by_arm_readers {
        if *readers != CONCURRENT_READERS {
            continue;
        }

        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let median = values[values.len() / 2];

        assert!(mean.is_finite() && mean > 0.0);
        assert!(median.is_finite() && median > 0.0);

        println!("{:<25} | {:<12.1} | {:<12.1}", arm, mean, median);

        // Compare with single-threaded baseline
        if let Some(mg_mean) = manual_guard_mean {
            let contention_overhead = mean / mg_mean;
            println!(
                "    Contention overhead: {:.2}x vs single-threaded baseline",
                contention_overhead
            );
        }
    }
}

/// Arm (a): one-shot — fresh read() call per lookup
fn run_one_shot(sr: &SyncRegion<u64>, handles: &[sefer_region::Handle<u64>]) -> f64 {
    // Warm-up
    for h in handles {
        std::hint::black_box(sr.read().get(*h));
    }

    // Measure
    let t0 = Instant::now();

    for _ in 0..ITERATIONS_PER_SAMPLE {
        for h in handles {
            std::hint::black_box(sr.read().get(*h));
        }
    }

    let elapsed_ns = t0.elapsed().as_nanos() as f64;
    elapsed_ns / (ITERATIONS_PER_SAMPLE * handles.len()) as f64
}

/// Arm (b): manual guard-hold — single guard across all lookups
fn run_manual_guard(sr: &SyncRegion<u64>, handles: &[sefer_region::Handle<u64>]) -> f64 {
    // Warm-up
    {
        let g = sr.read();
        for h in handles {
            std::hint::black_box(g.get(*h));
        }
    }

    // Measure
    let t0 = Instant::now();

    for _ in 0..ITERATIONS_PER_SAMPLE {
        let g = sr.read();
        for h in handles {
            std::hint::black_box(g.get(*h));
        }
    }

    let elapsed_ns = t0.elapsed().as_nanos() as f64;
    elapsed_ns / (ITERATIONS_PER_SAMPLE * handles.len()) as f64
}

/// Arm (c): closure wrapper — same as (b) but through throwaway wrapper
fn run_closure_wrapper(sr: &SyncRegion<u64>, handles: &[sefer_region::Handle<u64>]) -> f64 {
    // Warm-up
    with_read(sr, |r| {
        for h in handles {
            std::hint::black_box(r.get(*h));
        }
    });

    // Measure
    let t0 = Instant::now();

    for _ in 0..ITERATIONS_PER_SAMPLE {
        with_read(sr, |r| {
            for h in handles {
                std::hint::black_box(r.get(*h));
            }
        });
    }

    let elapsed_ns = t0.elapsed().as_nanos() as f64;
    elapsed_ns / (ITERATIONS_PER_SAMPLE * handles.len()) as f64
}

/// Concurrent arm: CONCURRENT_READERS threads each doing manual guard-hold
fn run_concurrent_manual_guard(
    sr: Arc<SyncRegion<u64>>,
    handles: &[sefer_region::Handle<u64>],
) -> f64 {
    // Warm-up
    {
        let g = sr.read();
        for h in handles {
            std::hint::black_box(g.get(*h));
        }
    }

    let barrier = std::sync::Barrier::new(CONCURRENT_READERS);
    let mut max_ns = 0u64;

    std::thread::scope(|s| {
        let thread_handles: Vec<_> = (0..CONCURRENT_READERS)
            .map(|_| {
                s.spawn(|| {
                    barrier.wait();
                    let t0 = Instant::now();

                    for _ in 0..CONCURRENT_ITERATIONS_PER_SAMPLE {
                        let g = sr.read();
                        for h in handles {
                            std::hint::black_box(g.get(*h));
                        }
                    }

                    t0.elapsed().as_nanos() as u64
                })
            })
            .collect();

        for handle in thread_handles {
            let ns = handle.join().unwrap();
            max_ns = max_ns.max(ns);
        }
    });

    let total_ops = (CONCURRENT_READERS * CONCURRENT_ITERATIONS_PER_SAMPLE * handles.len()) as f64;
    max_ns as f64 / total_ops
}
