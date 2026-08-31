//! Per-call `pop` tail-latency probe for `BACKOFF_SPIN_CAP`'s CAS-retry
//! backoff — the axis the cap sweep
//! (`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md`) did not originally measure
//! (round-8 review finding P2-2: per-thread ops over a 1-second window cannot
//! distinguish "one call starved for 100+ ms" from "every call uniformly 10x
//! slow").
//!
//! Observation-only: public `push`/`pop` API (through `ArrayIndexStack`),
//! std-only, no dependency added,
//! no crate behavior changed. The workload mirrors
//! `tests/threaded_conservation.rs` and the bench's contention rows exactly:
//! N threads x M iterations of pop-then-repush-exactly-what-you-popped
//! against a shared `ArrayIndexStack<16, 64>` prefilled with `0..64`, started
//! from a shared barrier. Every `pop` is individually timed.
//!
//! The backoff cap itself is a private `const` in
//! `crates/tagged-index-stack/src/lib.rs`, so an arm at a non-shipped cap is
//! produced by temporarily editing that one line and rebuilding — the same
//! documented substitution the cap sweep used (report §1). This binary cannot
//! observe that const; the `cap_label` it prints comes from `TIS_CAP_LABEL`
//! and is INFORMATIONAL ONLY. The resolved-cap evidence for a run is the
//! captured `const BACKOFF_SPIN_CAP: u32 = ...;` source line taken
//! immediately before each build (see the raw log this probe's output is
//! appended to, `docs/perf/_raw_tis_backoff_per_call_latency.log`).
//!
//! Run (shipped cap 6, no source edit needed):
//!
//! ```text
//! TIS_CAP_LABEL=6 cargo run --release -p tagged-index-stack --example backoff_per_call_latency
//! ```
//!
//! Output: a small header block, then one JSON object per line per
//! (shape, rep). Timing note: the two `Instant::now()` clock reads sit
//! OUTSIDE the timed `pop`, are identical in every arm, so cap-to-cap
//! comparisons stay apples-to-apples; absolute fast-path numbers are
//! inflated by roughly two clock reads. Percentiles are nearest-rank over
//! ALL pops in the run. Numbers published from this probe are derived, with
//! in-script assertions, by
//! `scripts/tis_backoff_cap_sweep_derive_report_data.mjs`.

use std::hint::black_box;
use std::sync::Barrier;
use std::time::Instant;

use tagged_index_stack::ArrayIndexStack;

/// Same width as the bench and the rest of this crate's test suite.
type Stack = ArrayIndexStack<16, { LINKS_SIZE as usize }>;

/// Number of indices in the fused stack's `ArrayLinks` links array, and the
/// exact
/// multiset seeded onto the stack before the threaded phase — the same
/// 64-element shape as `tests/threaded_conservation.rs`, so the measured
/// tail is the tail of the documented use case (a 64-slot free-list).
const LINKS_SIZE: u32 = 64;

/// Parsed from `TIS_SHAPES` (default `"4x20000,8x200000,16x200000"`):
/// `(threads, pop-then-repush iterations per thread)` pairs.
fn parse_shapes(spec: &str) -> Vec<(usize, u32)> {
    spec.split(',')
        .map(|s| {
            let (t, i) = s.split_once('x').expect("shape must be <threads>x<iters>");
            let threads: usize = t.parse().expect("threads must be a usize");
            let iters: u32 = i.parse().expect("iters must be a u32");
            (threads, iters)
        })
        .collect()
}

/// Nearest-rank percentile of an ascending-sorted sample slice, in
/// milliseconds. `q` must be in `(0, 1]`.
fn percentile_ms(sorted: &[u32], q: f64) -> f64 {
    assert!(q > 0.0 && q <= 1.0, "q must be in (0, 1]");
    let n = sorted.len();
    assert!(n > 0, "no samples");
    let rank = ((q * n as f64).ceil() as usize).clamp(1, n);
    sorted[rank - 1] as f64 / 1e6
}

fn main() {
    let cap_label = std::env::var("TIS_CAP_LABEL").unwrap_or_else(|_| "unlabeled".to_string());
    let shapes = parse_shapes(
        &std::env::var("TIS_SHAPES").unwrap_or_else(|_| "4x20000,8x200000,16x200000".to_string()),
    );
    let reps: usize = std::env::var("TIS_REPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);

    println!("=== tagged-index-stack per-call pop latency probe ===");
    println!("cap_label: {cap_label} (informational only; resolved-cap evidence is the captured const line in the raw log)");
    println!(
        "logical_cpus: {}",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0)
    );
    println!("reps_per_shape: {reps}");
    println!(
        "shapes: {}",
        shapes
            .iter()
            .map(|(t, i)| format!("{t}x{i}"))
            .collect::<Vec<_>>()
            .join(",")
    );

    for (threads, iters) in shapes {
        for rep in 1..=reps {
            let stack = Stack::new();
            for i in 0..LINKS_SIZE {
                stack.push(i);
            }

            let barrier = Barrier::new(threads + 1);
            let (per_thread, wall) = std::thread::scope(|s| {
                let stack = &stack;
                let barrier = &barrier;
                let mut handles = Vec::with_capacity(threads);
                for _ in 0..threads {
                    handles.push(s.spawn(move || {
                        let mut samples: Vec<u32> = Vec::with_capacity(iters as usize);
                        barrier.wait();
                        for _ in 0..iters {
                            let t0 = Instant::now();
                            let idx = stack.pop().expect(
                                "per-call latency probe: stack drained -- invariant violated \
                                 (64 prefilled, at most `threads` indices in flight)",
                            );
                            let d = t0.elapsed();
                            black_box(idx);
                            samples.push(d.as_nanos().min(u32::MAX as u128) as u32);
                            stack.push(idx);
                        }
                        samples
                    }));
                }
                barrier.wait();
                let start = Instant::now();
                let per_thread: Vec<Vec<u32>> =
                    handles.into_iter().map(|h| h.join().unwrap()).collect();
                (per_thread, start.elapsed())
            });

            let pop_samples: usize = per_thread.iter().map(|v| v.len()).sum();
            let mut all: Vec<u32> = per_thread.into_iter().flatten().collect();
            all.sort_unstable();
            let p50 = percentile_ms(&all, 0.50);
            let p90 = percentile_ms(&all, 0.90);
            let p99 = percentile_ms(&all, 0.99);
            let p999 = percentile_ms(&all, 0.999);
            let max_ms = *all.last().expect("no samples") as f64 / 1e6;
            let over_1ms = all.iter().filter(|&&d| d > 1_000_000).count();
            let over_10ms = all.iter().filter(|&&d| d > 10_000_000).count();
            let over_100ms = all.iter().filter(|&&d| d > 100_000_000).count();
            let wall_ms = wall.as_secs_f64() * 1e3;

            println!(
                "{{\"cap_label\":\"{cap_label}\",\"threads\":{threads},\"iters\":{iters},\"rep\":{rep},\
                 \"pop_samples\":{pop_samples},\"pop_p50_ms\":{p50:.3},\"pop_p90_ms\":{p90:.3},\
                 \"pop_p99_ms\":{p99:.3},\"pop_p999_ms\":{p999:.3},\"pop_max_ms\":{max_ms:.3},\
                 \"pop_over_1ms\":{over_1ms},\"pop_over_10ms\":{over_10ms},\"pop_over_100ms\":{over_100ms},\
                 \"wall_ms\":{wall_ms:.1}}}"
            );
        }
    }
}
