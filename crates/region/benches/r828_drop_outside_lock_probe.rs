// r828_drop_outside_lock_probe.rs
//
// Measures tail latency of clearing a SyncRegion with non-trivial Drop:
//   (a) baseline: clear() drops values under write lock
//   (b) two-phase: structurally extract under lock, drop outside lock
//
// Key metric: how long a CONTENDING reader/writer is blocked during clear.
// This is the tail-latency axis that motivates P-perf-4.
//
// Output format: RAW per-sample CSV first, THEN derived summary.

#[path = "common/stats.rs"]
mod stats;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use sefer_region::{Region, SyncRegion};

// Fixed-work constants
const VALUE_COUNT: usize = 10_000;
const DROP_DELAY_US: u64 = 50; // Artificial delay per Drop (microseconds)
const SAMPLES: usize = 5;

// Non-trivial Drop type with artificial delay
struct SlowDrop {
    // Non-trivial allocation ensures Drop does real work
    _data: Vec<u8>,
    _delay_us: u64,
}

impl Drop for SlowDrop {
    fn drop(&mut self) {
        // Artificial delay to simulate expensive destructor
        std::thread::sleep(Duration::from_micros(self._delay_us));
    }
}

fn main() {
    println!(
        "Note: Using SlowDrop with {}μs delay per value",
        DROP_DELAY_US
    );
    println!(
        "      Total clear time ≈ {} ms per sample",
        (VALUE_COUNT * DROP_DELAY_US as usize) / 1000
    );

    // Collect all raw data first, then print and compute summary.
    // Raw sample: (arm_name, sample_idx, clear_time_ns, contended_blocked_ns)
    let mut raw_data: Vec<(String, usize, f64, f64)> = Vec::new();

    for sample_idx in 0..SAMPLES {
        // Arm (a): baseline clear() drops under lock
        let (clear_time_ns, contended_blocked_ns) = run_baseline_clear();
        raw_data.push((
            "baseline_clear".to_string(),
            sample_idx,
            clear_time_ns,
            contended_blocked_ns,
        ));

        // Arm (b): two-phase drop outside lock
        let (clear_time_ns, contended_blocked_ns) = run_two_phase_clear();
        raw_data.push((
            "two_phase_clear".to_string(),
            sample_idx,
            clear_time_ns,
            contended_blocked_ns,
        ));
    }

    // ========== Raw CSV output (BEFORE any summary/prose) ==========
    println!("\n# raw_csv,arm,sample,clear_time_ns,contended_blocked_ns");
    for (arm, sample, clear_time, blocked_ns) in &raw_data {
        println!("raw_csv,{},{},{},{}", arm, sample, clear_time, blocked_ns);
    }

    // ========== Summary (derived from raw samples above) ==========
    println!("\n=== Summary (derived from raw samples above) ===\n");

    // Group samples by arm for summary statistics.
    let mut by_arm: HashMap<String, Vec<(f64, f64)>> = HashMap::new();

    for (arm, _sample, clear_time, blocked_ns) in &raw_data {
        by_arm
            .entry(arm.clone())
            .or_default()
            .push((*clear_time, *blocked_ns));
    }

    // Compute and print summary statistics.
    println!(
        "## Clear operation time ({} values with {}μs Drop delay each)\n",
        VALUE_COUNT, DROP_DELAY_US
    );
    println!(
        "{:<20} | {:<15} | {:<15} | {:<20}",
        "arm", "mean (ms)", "median (ms)", "blocked tail (ms)"
    );
    println!("{}", "-".repeat(80));

    let mut baseline_blocked_mean: Option<f64> = None;
    let mut two_phase_blocked_mean: Option<f64> = None;

    for (arm, values) in &mut by_arm {
        let clear_times: Vec<f64> = values.iter().map(|(t, _)| *t).collect();
        let blocked_times: Vec<f64> = values.iter().map(|(_, b)| *b).collect();

        let (clear_mean, clear_median) = stats::mean_and_median(clear_times);
        let (blocked_mean, blocked_median) = stats::mean_and_median(blocked_times);

        assert!(clear_mean.is_finite() && clear_mean > 0.0);
        assert!(clear_median.is_finite() && clear_median > 0.0);
        assert!(blocked_mean.is_finite() && blocked_mean >= 0.0);
        assert!(blocked_median.is_finite() && blocked_median >= 0.0);

        println!(
            "{:<20} | {:<15.2} | {:<15.2} | {:<20.2}",
            arm,
            clear_mean / 1_000_000.0, // ns -> ms
            clear_median / 1_000_000.0,
            blocked_mean / 1_000_000.0
        );

        if arm == "baseline_clear" {
            baseline_blocked_mean = Some(blocked_mean);
        } else if arm == "two_phase_clear" {
            two_phase_blocked_mean = Some(blocked_mean);
        }
    }

    // Compute tail-latency improvement
    if let (Some(baseline_blocked), Some(two_phase_blocked)) =
        (baseline_blocked_mean, two_phase_blocked_mean)
    {
        let improvement = baseline_blocked / two_phase_blocked.max(1.0);
        println!("\n>>> Tail-latency improvement: {:.2}x", improvement);
        if two_phase_blocked < baseline_blocked * 0.1 {
            println!("    PASS: Two-phase reduces contended blocked time by >90% (expected)");
        } else if two_phase_blocked < baseline_blocked * 0.5 {
            println!("    MODERATE: Two-phase reduces contended blocked time by >50%");
        } else {
            println!(
                "    WARNING: Two-phase shows <50% tail-latency reduction (may be noise-bound)"
            );
        }
    }
}

/// Arm (a): baseline - clear() drops values under write lock
/// Returns (clear_time_ns, contended_blocked_ns)
fn run_baseline_clear() -> (f64, f64) {
    let sr = Arc::new(SyncRegion::new());

    // Fill with SlowDrop values
    for _ in 0..VALUE_COUNT {
        let _ = sr.write().insert(SlowDrop {
            _data: vec![0u8; 1024],
            _delay_us: DROP_DELAY_US,
        });
    }

    let barrier = Arc::new(std::sync::Barrier::new(2));
    // Set only AFTER the writer has actually acquired the write lock (see below) --
    // without this signal, the reader thread races the writer to the lock and may
    // win, making the "blocked" measurement meaningless (this was the exact race
    // artifact found during zero-trust review of the delegated first draft: both
    // arms measured ~0.1-0.2ms regardless of whether the writer held the lock for
    // 5+ seconds). A plain second `barrier.wait()` after acquisition would not fix
    // this either -- the reader could reach that second wait before the writer, so
    // both would still start "together" with no ordering guarantee. A spin-checked
    // flag, set strictly after acquisition and read strictly before the reader's
    // own `read()` attempt, is what actually establishes "writer holds the lock
    // before the reader tries" as a happens-before relationship.
    let lock_acquired = Arc::new(AtomicBool::new(false));

    // Spawn a contending reader that will block during clear
    let sr_clone = Arc::clone(&sr);
    let barrier_clone = Arc::clone(&barrier);
    let lock_acquired_clone = Arc::clone(&lock_acquired);

    let handle = thread::spawn(move || {
        barrier_clone.wait(); // Synchronize thread start
        while !lock_acquired_clone.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        let t0 = Instant::now();

        // The writer is guaranteed to hold the write lock at this point --
        // this read() call is guaranteed to actually block on it.
        let _g = sr_clone.read();

        let blocked_ns = t0.elapsed().as_nanos() as f64;

        // Hold read briefly to confirm we got it
        std::thread::sleep(Duration::from_millis(1));

        blocked_ns
    });

    barrier.wait(); // Synchronize thread start
    let mut guard = sr.write();
    lock_acquired.store(true, Ordering::Release);
    let t0 = Instant::now();

    // Clear drops all values under write lock - blocks the contending reader
    guard.clear();
    drop(guard);

    let clear_time_ns = t0.elapsed().as_nanos() as f64;

    // Wait for reader to complete and get its blocked time
    let contended_blocked_ns = handle.join().unwrap();

    (clear_time_ns, contended_blocked_ns)
}

/// Arm (b): two-phase - structurally extract under lock, drop outside lock
/// Returns (clear_time_ns, contended_blocked_ns)
fn run_two_phase_clear() -> (f64, f64) {
    let sr = Arc::new(SyncRegion::new());

    // Fill with SlowDrop values
    for _ in 0..VALUE_COUNT {
        let _ = sr.write().insert(SlowDrop {
            _data: vec![0u8; 1024],
            _delay_us: DROP_DELAY_US,
        });
    }

    let barrier = Arc::new(std::sync::Barrier::new(2));
    // Same signal-based synchronization as run_baseline_clear() -- see the comment
    // there for why a plain barrier is not sufficient.
    let lock_acquired = Arc::new(AtomicBool::new(false));

    // Spawn a contending reader that will block during write lock acquisition
    let sr_clone = Arc::clone(&sr);
    let barrier_clone = Arc::clone(&barrier);
    let lock_acquired_clone = Arc::clone(&lock_acquired);

    let handle = thread::spawn(move || {
        barrier_clone.wait(); // Synchronize thread start
        while !lock_acquired_clone.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        let t0 = Instant::now();

        // The writer is guaranteed to hold the write lock at this point. Under
        // the two-phase pattern, the writer releases the lock almost immediately
        // (a plain struct swap, not the slow Drop work) -- this read() call
        // should unblock quickly, unlike the baseline arm above.
        let _g = sr_clone.read();

        let blocked_ns = t0.elapsed().as_nanos() as f64;

        // Hold read briefly
        std::thread::sleep(Duration::from_millis(1));

        blocked_ns
    });

    barrier.wait(); // Synchronize thread start
    let mut guard = sr.write();
    lock_acquired.store(true, Ordering::Release);
    let t0 = Instant::now();

    // Two-phase pattern:
    // 1. Structurally replace the Region (fast) while the writer still holds the lock.
    let old_region = std::mem::replace(&mut *guard, Region::new());

    // Drop the guard - this releases the write lock BEFORE dropping values
    drop(guard);

    // 2. Drop the old Region's values OUTSIDE the write lock (slow, but doesn't block readers)
    drop(old_region);

    let clear_time_ns = t0.elapsed().as_nanos() as f64;

    // Wait for reader to complete and get its blocked time
    let contended_blocked_ns = handle.join().unwrap();

    (clear_time_ns, contended_blocked_ns)
}
