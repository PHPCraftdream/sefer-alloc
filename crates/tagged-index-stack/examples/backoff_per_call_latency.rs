//! Per-call `pop` tail-latency probe for `BACKOFF_SPIN_CAP`'s CAS-retry
//! backoff — the axis the cap sweep
//! (`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` — a repository file, not part
//! of the published package) did not originally measure: per-thread ops over
//! a 1-second window cannot
//! distinguish "one call starved for 100+ ms" from "every call uniformly 10x
//! slow").
//!
//! Observation-only: public `push`/`pop` API (through `ArrayIndexStack`),
//! std-only, no dependency added,
//! no crate behavior changed. The workload mirrors
//! `tests/threaded_conservation.rs` and the bench's contention rows exactly:
//! N threads x M iterations of pop-then-repush-exactly-what-you-popped
//! against a shared `ArrayIndexStack<16, 64>` prefilled with `0..64`, started
//! through a ready/window barrier pair (the bench's published-window
//! protocol): workers rendezvous at the ready barrier, the coordinator records
//! the wall-clock start, and only then does the window barrier release them
//! into the counted work — no counted pop can precede the start the `wall_ms`
//! denominator is derived from. `wall_ms` is nonetheless a
//! coordinator-to-last-join ENVELOPE, not a tight pure-work time: the second
//! barrier only prevents counted work from STARTING before the start timestamp,
//! while the denominator still includes each worker's own release overhead, its
//! last iteration's tail, the final `.join()` wait, and any OS-scheduling
//! overshoot past the nominal deadline — an upper bound with real but bounded
//! slack. Every `pop` is individually timed.
//!
//! The backoff cap itself is a private `const` in
//! `crates/tagged-index-stack/src/imp.rs`, so an arm at a non-shipped cap is
//! produced by temporarily editing that one line and rebuilding — the same
//! documented substitution the cap sweep used (report §1). This binary cannot
//! observe that const; the `cap_label` it prints comes from `TIS_CAP_LABEL`
//! and is INFORMATIONAL ONLY; because it is interpolated into the JSON lines
//! unescaped, it is validated at read time against the non-empty
//! `[A-Za-z0-9_.-]+` alphabet, and any other value aborts with exit code 2
//! before any output. The resolved-cap evidence for a run is the
//! captured `const BACKOFF_SPIN_CAP: u32 = ...;` source line taken
//! immediately before each build (see the raw log this probe's output is
//! appended to, `docs/perf/_raw_tis_backoff_per_call_latency.log` — a
//! repository file, not part of the published package).
//!
//! Run (shipped cap 6, no source edit needed):
//!
//! ```text
//! TIS_CAP_LABEL=6 cargo run --release -p tagged-index-stack --example backoff_per_call_latency
//! ```
//!
//! Output: a small header block, then one JSON object per line per
//! (shape, rep) — each carries a `pop_clamp_saturated` count of samples that
//! hit the `u32::MAX`-ns (~4295 ms) recording ceiling, and a final summary
//! line totals it across the run. Timing note: each `pop` is timed by
//! bracketing it DIRECTLY — `t0 = Instant::now()` immediately before the call,
//! `t0.elapsed()` immediately after — so the timer pair IS the timed region's
//! boundaries, not something outside it, and parts of the two clock reads' own
//! call overhead are unavoidably counted inside every sample. Because that
//! identical bracketing pattern applies to every arm, cap-to-cap comparisons
//! stay apples-to-apples; the ABSOLUTE fast-path numbers carry the bracket
//! overhead on top of the true `pop` cost. This probe does not assert a
//! correction factor: each row instead publishes an empty-bracket baseline
//! (`bracket_baseline_p50_ns` / `bracket_baseline_p99_ns` /
//! `bracket_baseline_max_ns`) measured the same way — `Instant::now()` and
//! `elapsed()` back to back with NO `pop`/`push` between the reads, same thread
//! count, nearest-rank percentiles over the baseline samples through the same
//! statistical machinery (`percentile_ns`, which `percentile_ms` delegates
//! to) — so a reader can judge the absolute-number
//! overhead magnitude for themselves. The baseline is published alongside,
//! never subtracted from, the pop percentiles. The baseline fields are integer
//! NANOSECONDS, not the row's 3-decimal milliseconds: the bracket floor is tens
//! of ns, far below the 0.001 ms at which a `_ms` field stops reading as
//! 0.000.
//! A baseline percentile of 0 is a genuine reading, not a defect: where the
//! host clock's granularity is coarser than the bracket floor (e.g. a 10 MHz
//! QPC tick = 100 ns), sub-tick brackets quantize to 0 or one tick, and
//! p99/max then bound the floor at about one tick.
//! Percentiles are nearest-rank over ALL pops in the run. Numbers published
//! from this probe are derived, with
//! in-script assertions, by
//! `scripts/tis_backoff_cap_sweep_derive_report_data.mjs` (a repository
//! script, not part of the published package).

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

/// Per-thread sample count for the empty-bracket baseline each row carries:
/// large enough that nearest-rank p99 over the pooled samples is
/// well-resolved, small enough that the baseline phase adds only
/// milliseconds per row. Measured with the same two-clock-read bracket and
/// the same thread count as the row's pop samples; never subtracted from
/// them (see the module-level timing note).
const BASELINE_ITERS_PER_THREAD: u32 = 20_000;

/// Fail fast at the argument-parsing boundary: a misconfigured probe run must
/// exit with a message naming the parameter, the value received, and the valid
/// range — not surface later as a mid-run panic that looks like a crate bug,
/// or as empty output.
fn die(msg: String) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(2);
}

/// Parsed from `TIS_SHAPES` (default `"4x20000,8x200000,16x200000"`):
/// `(threads, pop-then-repush iterations per thread)` pairs.
/// Validation: each entry must be `<threads>x<iters>` with `threads` a usize
/// in `1..=LINKS_SIZE` (only LINKS_SIZE indices are prefilled, so more threads
/// than that makes `pop()` legitimately return `None` mid-run) and `iters` a
/// u32 `>= 1` (zero iterations produce no samples).
fn parse_shapes(spec: &str) -> Vec<(usize, u32)> {
    spec.split(',')
        .map(|s| {
            let Some((t, i)) = s.split_once('x') else {
                die(format!(
                    "TIS_SHAPES entry {s:?}: expected <threads>x<iters>"
                ))
            };
            let threads: usize = match t.parse() {
                Ok(v) => v,
                Err(_) => die(format!(
                    "TIS_SHAPES entry {s:?}: threads {t:?} must be a usize"
                )),
            };
            let iters: u32 = match i.parse() {
                Ok(v) => v,
                Err(_) => die(format!("TIS_SHAPES entry {s:?}: iters {i:?} must be a u32")),
            };
            if threads < 1 || threads > LINKS_SIZE as usize {
                die(format!(
                    "TIS_SHAPES entry {s:?}: threads {threads} must be in 1..=64 (LINKS_SIZE) -- \
                     only LINKS_SIZE indices are prefilled, so more threads than that makes \
                     pop() legitimately return None mid-run"
                ));
            }
            if iters < 1 {
                die(format!(
                    "TIS_SHAPES entry {s:?}: iters {iters} must be >= 1 -- zero iterations \
                     produce no samples"
                ));
            }
            (threads, iters)
        })
        .collect()
}

/// Nearest-rank percentile of an ascending-sorted sample slice, in the
/// samples' own unit (NANOSECONDS). `q` must be in `(0, 1]`.
fn percentile_ns(sorted: &[u32], q: f64) -> u64 {
    assert!(q > 0.0 && q <= 1.0, "q must be in (0, 1]");
    let n = sorted.len();
    assert!(n > 0, "no samples");
    let rank = ((q * n as f64).ceil() as usize).clamp(1, n);
    sorted[rank - 1] as u64
}

/// Nearest-rank percentile of an ascending-sorted sample slice. Samples are
/// per-call latencies in NANOSECONDS; the returned value is that percentile
/// converted to MILLISECONDS (`nanos / 1e6`). `q` must be in `(0, 1]`.
fn percentile_ms(sorted: &[u32], q: f64) -> f64 {
    percentile_ns(sorted, q) as f64 / 1e6
}

fn main() {
    // The label is interpolated into the JSONL lines without escaping, so it is
    // restricted to this allowlist and anything else aborts via `die` before any
    // output; the label is INFORMATIONAL ONLY so this costs nothing.
    let cap_label = match std::env::var("TIS_CAP_LABEL") {
        Ok(raw) => {
            if !raw.is_empty()
                && raw
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
            {
                raw
            } else {
                die(format!(
                    "TIS_CAP_LABEL={raw:?}: label must be non-empty and match [A-Za-z0-9_.-] -- \
                     it is interpolated into the JSON output unescaped, so any other character \
                     could produce invalid JSONL"
                ))
            }
        }
        Err(_) => "unlabeled".to_string(),
    };
    let shapes = parse_shapes(
        &std::env::var("TIS_SHAPES").unwrap_or_else(|_| "4x20000,8x200000,16x200000".to_string()),
    );
    let reps: usize = match std::env::var("TIS_REPS") {
        Ok(raw) => raw
            .parse()
            .unwrap_or_else(|_| die(format!("TIS_REPS={raw:?} must be a usize"))),
        Err(_) => 3,
    };
    if reps == 0 {
        die(
            "TIS_REPS=0 is invalid: reps must be >= 1 (zero reps produce no result rows)"
                .to_string(),
        );
    }

    let mut run_clamp_saturated: u64 = 0;

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

            // Published-window protocol, same shape as the contention phases of
            // `benches/tagged_index_stack_bench.rs`: workers rendezvous at
            // `barrier_ready` once setup is done; the coordinator then records the
            // wall-clock start; only `barrier_window` releases workers into the
            // counted work, and a worker cannot pass `barrier_window.wait()` until
            // the coordinator arrives there (which it does only after its
            // `Instant::now()`), so no counted pop/push can precede the clock read
            // the `wall_ms` denominator derives from. The old single barrier let
            // work begin before the coordinator timestamped the run, shortening
            // `wall_ms`.
            //
            // `wall_ms` is a coordinator-to-last-join ENVELOPE, not a tight
            // measurement of pure work time: the second barrier prevents
            // counted work from starting before `start` is taken, but the
            // elapsed denominator still includes each worker's own release
            // overhead past the window barrier, that worker's last
            // iteration's tail, the final `.join()` wait, and any
            // OS-scheduling overshoot past the nominal deadline — an upper
            // bound with real but bounded slack.
            let barrier_ready = Barrier::new(threads + 1);
            let barrier_window = Barrier::new(threads + 1);
            let (per_thread, wall) = std::thread::scope(|s| {
                let stack = &stack;
                let barrier_ready = &barrier_ready;
                let barrier_window = &barrier_window;
                let mut handles = Vec::with_capacity(threads);
                for _ in 0..threads {
                    handles.push(s.spawn(move || {
                        let mut samples: Vec<u32> = Vec::with_capacity(iters as usize);
                        // Samples that hit the u32::MAX-ns recording ceiling below —
                        // counted, never silent: a saturated sample is recorded at
                        // exactly ~4295 ms, which is a floor, not a measurement.
                        let mut clamp_saturated: u64 = 0;
                        barrier_ready.wait();
                        barrier_window.wait();
                        for _ in 0..iters {
                            let t0 = Instant::now();
                            let idx = stack.pop().expect(
                                "per-call latency probe: stack drained -- invariant violated \
                                 (64 prefilled, at most `threads` indices in flight)",
                            );
                            let nanos = t0.elapsed().as_nanos();
                            if nanos > u32::MAX as u128 {
                                clamp_saturated += 1;
                            }
                            black_box(idx);
                            samples.push(nanos.min(u32::MAX as u128) as u32);
                            stack.push(idx);
                        }
                        (samples, clamp_saturated)
                    }));
                }
                barrier_ready.wait();
                let start = Instant::now();
                barrier_window.wait();
                let per_thread: Vec<(Vec<u32>, u64)> =
                    handles.into_iter().map(|h| h.join().unwrap()).collect();
                (per_thread, start.elapsed())
            });

            // Empty-bracket baseline (review P3-2): the same two-clock-read
            // bracket as the pop samples, with NO `pop`/`push` between the
            // reads, same thread count, so the row carries its own
            // calibration of how much of a sample can be pure bracket
            // overhead. Published alongside the pop percentiles, never
            // subtracted from them.
            let mut baseline: Vec<u32> = std::thread::scope(|s| {
                let mut handles = Vec::with_capacity(threads);
                for _ in 0..threads {
                    handles.push(s.spawn(move || {
                        let mut samples: Vec<u32> =
                            Vec::with_capacity(BASELINE_ITERS_PER_THREAD as usize);
                        for _ in 0..BASELINE_ITERS_PER_THREAD {
                            let t0 = Instant::now();
                            let nanos = t0.elapsed().as_nanos();
                            samples.push(nanos.min(u32::MAX as u128) as u32);
                        }
                        samples
                    }));
                }
                handles
                    .into_iter()
                    .flat_map(|h| h.join().unwrap())
                    .collect()
            });
            baseline.sort_unstable();
            let baseline_p50 = percentile_ns(&baseline, 0.50);
            let baseline_p99 = percentile_ns(&baseline, 0.99);
            let baseline_max_ns = *baseline.last().expect("no baseline samples") as u64;

            let pop_samples: usize = per_thread.iter().map(|(v, _)| v.len()).sum();
            let clamp_saturated: u64 = per_thread.iter().map(|(_, c)| c).sum();
            run_clamp_saturated += clamp_saturated;
            let mut all: Vec<u32> = per_thread.into_iter().flat_map(|(v, _)| v).collect();
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
                 \"pop_clamp_saturated\":{clamp_saturated},\
                 \"bracket_baseline_p50_ns\":{baseline_p50},\"bracket_baseline_p99_ns\":{baseline_p99},\
                 \"bracket_baseline_max_ns\":{baseline_max_ns},\"wall_ms\":{wall_ms:.1}}}"
            );
        }
    }

    println!(
        "=== clamp-saturation: {run_clamp_saturated} pop sample(s) hit the u32::MAX ns (~4295 ms) recording ceiling ==="
    );
    if run_clamp_saturated > 0 {
        println!(
            "    NON-ZERO: affected rows' pop_p999_ms/pop_max_ms are the floor value, not genuine timings."
        );
    }
}
