// r828_dense_iteration_probe.rs
//
// Compares SlotMap (holey iteration) vs DenseSlotMap (compact iteration) on two axes:
//   1. Iteration axis: full iter() time over 10k populated → 1k live (90% removed) state
//   2. Churn axis: insert/remove throughput on a churny workload
//
// Motivation: SEFER_REGION_DENSE_AND_SHARDED_DESIGN.md §1's open question — does
// DenseSlotMap's compacting behavior actually deliver a measurable win on holey
// workloads, and does it regress on churn?
//
// Output format: RAW per-sample CSV first, THEN derived summary.

use std::collections::HashMap;
use std::time::Instant;

use slotmap::{DefaultKey, DenseSlotMap, SlotMap};

// Fixed-work constants (not fixed-duration).
const INITIAL_COUNT: usize = 100_000; // More values for measurable iteration time
const LIVE_COUNT: usize = 10_000; // 90% removed = 90% tombstone holes
const ITERATIONS_PER_SAMPLE: usize = 1000; // Full passes over the data
const CHURN_OPERATIONS: usize = 50_000; // insert/remove cycles for churn axis

const SAMPLES: usize = 5;

fn main() {
    // Collect all raw data first, then print and compute summary.
    // Raw sample: (arm_name, axis_name, sample_index, metric_name, value)
    let mut raw_data: Vec<(String, String, usize, String, f64)> = Vec::new();

    // Arm A: SlotMap (holey iteration, baseline)
    let slotmap_region_arm = "slotmap_region";

    for sample_idx in 0..SAMPLES {
        // Iteration axis
        let iter_time_ns = measure_slotmap_iteration();

        raw_data.push((
            slotmap_region_arm.to_string(),
            "iteration".to_string(),
            sample_idx,
            "time_ns_per_iter".to_string(),
            iter_time_ns as f64,
        ));

        // Churn axis
        let churn_ops_per_sec = measure_slotmap_churn();

        raw_data.push((
            slotmap_region_arm.to_string(),
            "churn".to_string(),
            sample_idx,
            "ops_per_sec".to_string(),
            churn_ops_per_sec,
        ));
    }

    // Arm B: DenseSlotMap (compact iteration, candidate)
    let dense_slotmap_arm = "dense_slotmap";

    for sample_idx in 0..SAMPLES {
        // Iteration axis
        let iter_time_ns = measure_dense_slotmap_iteration();

        raw_data.push((
            dense_slotmap_arm.to_string(),
            "iteration".to_string(),
            sample_idx,
            "time_ns_per_iter".to_string(),
            iter_time_ns as f64,
        ));

        // Churn axis
        let churn_ops_per_sec = measure_dense_slotmap_churn();

        raw_data.push((
            dense_slotmap_arm.to_string(),
            "churn".to_string(),
            sample_idx,
            "ops_per_sec".to_string(),
            churn_ops_per_sec,
        ));
    }

    // ========== Raw CSV output (BEFORE any summary/prose) ==========
    println!("# raw_csv,arm,axis,sample,metric,value");
    for (arm, axis, sample, metric, value) in &raw_data {
        println!("raw_csv,{},{},{},{},{}", arm, axis, sample, metric, value);
    }

    // ========== Summary (derived from raw samples above) ==========
    println!("\n=== Summary (derived from raw samples above) ===\n");

    // Group samples by (arm, axis, metric) for summary statistics.
    let mut by_arm_axis_metric: HashMap<(String, String, String), Vec<f64>> = HashMap::new();

    for (arm, axis, _sample, metric, value) in &raw_data {
        by_arm_axis_metric
            .entry((arm.clone(), axis.clone(), metric.clone()))
            .or_default()
            .push(*value);
    }

    // Compute and print summary statistics.
    println!("## Iteration axis: time per full iter() over 10k populated → 1k live state");
    println!("\nUnit: nanoseconds per complete iter() pass\n");
    println!(
        "{:<20} | {:<10} | {:<10} | {:<10}",
        "arm", "mean", "median", "speedup"
    );
    println!("{}", "-".repeat(60));

    let mut slotmap_iter_mean: Option<f64> = None;
    let mut dense_iter_mean: Option<f64> = None;

    for ((arm, axis, metric), values) in &mut by_arm_axis_metric {
        if axis != "iteration" || metric != "time_ns_per_iter" {
            continue;
        }

        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let median = values[values.len() / 2];

        assert!(mean.is_finite() && mean > 0.0);
        assert!(median.is_finite() && median > 0.0);

        if arm == slotmap_region_arm {
            slotmap_iter_mean = Some(mean);
        } else if arm == dense_slotmap_arm {
            dense_iter_mean = Some(mean);
        }
    }

    // Print rows with speedup calculation
    if let (Some(slotmap_mean), Some(dense_mean)) = (slotmap_iter_mean, dense_iter_mean) {
        let slotmap_median = by_arm_axis_metric
            .get(&(
                slotmap_region_arm.to_string(),
                "iteration".to_string(),
                "time_ns_per_iter".to_string(),
            ))
            .and_then(|v| v.get(v.len() / 2))
            .copied()
            .unwrap();
        let dense_median = by_arm_axis_metric
            .get(&(
                dense_slotmap_arm.to_string(),
                "iteration".to_string(),
                "time_ns_per_iter".to_string(),
            ))
            .and_then(|v| v.get(v.len() / 2))
            .copied()
            .unwrap();

        println!(
            "{:<20} | {:<10.0} | {:<10.0} | 1.00x (baseline)",
            slotmap_region_arm, slotmap_mean, slotmap_median
        );

        let speedup = slotmap_mean / dense_mean;
        println!(
            "{:<20} | {:<10.0} | {:<10.0} | {:.2}x vs baseline",
            dense_slotmap_arm, dense_mean, dense_median, speedup
        );

        assert!(speedup.is_finite() && speedup > 0.0);
    }

    println!("\n## Churn axis: insert/remove throughput");
    println!("\nUnit: operations (insert+remove) per second\n");
    println!(
        "{:<20} | {:<10} | {:<10} | {:<10}",
        "arm", "mean", "median", "ratio"
    );
    println!("{}", "-".repeat(60));

    let mut slotmap_churn_mean: Option<f64> = None;
    let mut dense_churn_mean: Option<f64> = None;

    for ((arm, axis, metric), values) in &mut by_arm_axis_metric {
        if axis != "churn" || metric != "ops_per_sec" {
            continue;
        }

        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let median = values[values.len() / 2];

        assert!(mean.is_finite() && mean > 0.0);
        assert!(median.is_finite() && median > 0.0);

        if arm == slotmap_region_arm {
            slotmap_churn_mean = Some(mean);
        } else if arm == dense_slotmap_arm {
            dense_churn_mean = Some(mean);
        }
    }

    if let (Some(slotmap_mean), Some(dense_mean)) = (slotmap_churn_mean, dense_churn_mean) {
        let slotmap_median = by_arm_axis_metric
            .get(&(
                slotmap_region_arm.to_string(),
                "churn".to_string(),
                "ops_per_sec".to_string(),
            ))
            .and_then(|v| v.get(v.len() / 2))
            .copied()
            .unwrap();
        let dense_median = by_arm_axis_metric
            .get(&(
                dense_slotmap_arm.to_string(),
                "churn".to_string(),
                "ops_per_sec".to_string(),
            ))
            .and_then(|v| v.get(v.len() / 2))
            .copied()
            .unwrap();

        println!(
            "{:<20} | {:<10.0} | {:<10.0} | 1.00x (baseline)",
            slotmap_region_arm, slotmap_mean, slotmap_median
        );

        let ratio = dense_mean / slotmap_mean;
        println!(
            "{:<20} | {:<10.0} | {:<10.0} | {:.3}x vs baseline",
            dense_slotmap_arm, dense_mean, dense_median, ratio
        );

        assert!(ratio.is_finite() && ratio > 0.0);

        // Explicit regression check
        if ratio < 0.8 {
            println!(
                "\n>>> WARNING: DenseSlotMap shows substantial regression on churn axis ({:.3}x)",
                ratio
            );
        }
    }
}

/// Measure SlotMap iteration time over 10k populated → 1k live state.
/// Returns nanoseconds per complete iter() pass.
fn measure_slotmap_iteration() -> u64 {
    // Set up SlotMap with 10k values
    let mut sm: SlotMap<DefaultKey, u64> = SlotMap::with_capacity(INITIAL_COUNT);
    let mut keys: Vec<DefaultKey> = Vec::with_capacity(INITIAL_COUNT);

    for i in 0..INITIAL_COUNT {
        keys.push(sm.insert(i as u64));
    }

    // Remove 90% to create tombstone holes, leaving LIVE_COUNT alive
    for k in keys.iter().take(INITIAL_COUNT - LIVE_COUNT) {
        sm.remove(*k);
    }

    // Warm-up pass
    let sum: u64 = sm.values().sum();
    std::hint::black_box(sum);

    // Measure ITERATIONS_PER_SAMPLE passes
    let t0 = Instant::now();

    for _ in 0..ITERATIONS_PER_SAMPLE {
        let sum: u64 = sm.values().sum();
        std::hint::black_box(sum);
    }

    let elapsed_ns = t0.elapsed().as_nanos() as u64;
    elapsed_ns / ITERATIONS_PER_SAMPLE as u64
}

/// Measure DenseSlotMap iteration time over 10k populated → 1k live state.
/// Returns nanoseconds per complete iter() pass.
fn measure_dense_slotmap_iteration() -> u64 {
    // Set up DenseSlotMap with 10k values
    let mut sm: DenseSlotMap<DefaultKey, u64> = DenseSlotMap::with_capacity(INITIAL_COUNT);
    let mut keys: Vec<DefaultKey> = Vec::with_capacity(INITIAL_COUNT);

    for i in 0..INITIAL_COUNT {
        keys.push(sm.insert(i as u64));
    }

    // Remove 90% - DenseSlotMap compacts on swap-remove, so no tombstone holes remain
    for k in keys.iter().take(INITIAL_COUNT - LIVE_COUNT) {
        sm.remove(*k);
    }

    // Warm-up pass
    let sum: u64 = sm.values().sum();
    std::hint::black_box(sum);

    // Measure ITERATIONS_PER_SAMPLE passes
    let t0 = Instant::now();

    for _ in 0..ITERATIONS_PER_SAMPLE {
        let sum: u64 = sm.values().sum();
        std::hint::black_box(sum);
    }

    let elapsed_ns = t0.elapsed().as_nanos() as u64;
    elapsed_ns / ITERATIONS_PER_SAMPLE as u64
}

/// Measure SlotMap churn: repeated insert/remove throughput.
/// Returns operations per second.
fn measure_slotmap_churn() -> f64 {
    let mut sm: SlotMap<DefaultKey, u64> = SlotMap::new();
    let mut key: Option<DefaultKey> = None;

    // Warm-up
    for _ in 0..1000 {
        if let Some(k) = key.take() {
            sm.remove(k);
        }
        key = Some(sm.insert(42));
    }

    // Measure
    let t0 = Instant::now();

    for i in 0..CHURN_OPERATIONS {
        if let Some(k) = key.take() {
            sm.remove(k);
        }
        key = Some(sm.insert(i as u64));
    }

    let elapsed_sec = t0.elapsed().as_secs_f64();
    CHURN_OPERATIONS as f64 / elapsed_sec
}

/// Measure DenseSlotMap churn: repeated insert/remove throughput.
/// Returns operations per second.
fn measure_dense_slotmap_churn() -> f64 {
    let mut sm: DenseSlotMap<DefaultKey, u64> = DenseSlotMap::new();
    let mut key: Option<DefaultKey> = None;

    // Warm-up
    for _ in 0..1000 {
        if let Some(k) = key.take() {
            sm.remove(k);
        }
        key = Some(sm.insert(42));
    }

    // Measure
    let t0 = Instant::now();

    for i in 0..CHURN_OPERATIONS {
        if let Some(k) = key.take() {
            sm.remove(k);
        }
        key = Some(sm.insert(i as u64));
    }

    let elapsed_sec = t0.elapsed().as_secs_f64();
    CHURN_OPERATIONS as f64 / elapsed_sec
}
