//! Wallclock harness for the P3-1/P3-2 A/B study (link-ordering and
//! strong-vs-weak CAS variants of `crates/tagged-index-stack`).
//!
//! Workload: N threads of pop-then-repush-exactly-what-you-popped against a
//! shared `ArrayIndexStack<16, 256>` prefilled with `0..64`, started from a
//! barrier under the same published-window protocol as
//! `crates/tagged-index-stack/benches/tagged_index_stack_bench.rs`
//! (coordinator publishes the timed window after full rendezvous; workers
//! warm up uncounted until the shared window opens; each worker counts only
//! completed repushes inside the window).
//!
//! The driver spawns this binary once per (variant, sample) and reads ONE
//! JSON line from stdout (the final line of output). CAS-retry counters come
//! from the scratch crate's `retry_counts_for_test()` (gated on the scratch
//! crate's `test-internals` feature, default-on there only); the driver
//! snapshots deltas across the timed window by reading before/after values
//! reported here.
//!
//! 100% safe code — public `push`/`pop` API only.

#![deny(unsafe_code)]

use std::env;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Barrier, OnceLock};
use std::time::{Duration, Instant};

use {{CRATE_NAME}}::ArrayIndexStack;

/// Same 16-bit index width as the bench and the rest of the crate's suite.
type Stack = ArrayIndexStack<16, LINKS>;

/// Number of indices in the fused stack's `ArrayLinks` links array.
const LINKS: usize = 256;

/// Indices prefilled before any timing (the 64-slot free-list shape used by
/// `tests/threaded_conservation.rs` and the tail-latency example).
const PREFILL: u32 = 64;

/// Check the clock every N iterations in the warm-up and counted loops:
/// checking every iteration would make the clock read a significant
/// fraction of a two-atomic-op iteration (same cadence rationale as the
/// bench's `DEADLINE_CHECK_INTERVAL`).
const DEADLINE_CHECK_INTERVAL: u32 = 64;

/// Fail fast at the argument-parsing boundary: a misconfigured run must exit
/// with a message naming the parameter, the value received, and the valid
/// range — not surface later as a mid-run panic that looks like a crate bug.
/// (Same input-validation discipline as
/// `crates/tagged-index-stack/examples/backoff_per_call_latency.rs`.)
fn die(msg: String) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(2);
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> T {
    match env::var(key) {
        Err(env::VarError::NotPresent) => default,
        Err(env::VarError::NotUnicode(_)) => {
            die(format!("{key}: value is not valid Unicode"));
        }
        Ok(raw) => match raw.trim().parse::<T>() {
            Ok(v) => v,
            Err(_) => die(format!(
                "{key}: invalid value {raw:?} (expected {})",
                std::any::type_name::<T>()
            )),
        },
    }
}

fn main() {
    // ── Config + validation (BEFORE spawning threads) ─────────────────────
    let threads: usize = parse_env("TIS_AB_THREADS", 4usize);
    let window_ms: u64 = parse_env("TIS_AB_WINDOW_MS", 1_000u64);
    let smoke = env::var("TIS_AB_SMOKE").as_deref() == Ok("1");
    let variant = env::var("TIS_AB_VARIANT").unwrap_or_else(|_| String::from("unlabeled"));

    if !(1..=256).contains(&threads) {
        die(format!(
            "TIS_AB_THREADS: value {threads} out of range (valid: 1..=256)"
        ));
    }
    if window_ms < 50 {
        die(format!(
            "TIS_AB_WINDOW_MS: value {window_ms} out of range (valid: >= 50 ms)"
        ));
    }

    // ── Stack prefill BEFORE any timing ────────────────────────────────────
    let stack = Stack::new();
    for i in 0..PREFILL {
        stack.push(i);
    }

    // Retry counters are process-global and cumulative, never reset: the
    // delta across the window is what the driver consumes.
    let (pop_retries_before, push_retries_before) = {{CRATE_NAME}}::retry_counts_for_test();

    // ── Published-window protocol (bench shape) ────────────────────────────
    let timed_start_cell: OnceLock<Instant> = OnceLock::new();
    let barrier_ready = Barrier::new(threads + 1);
    let barrier_window = Barrier::new(threads + 1);
    let barrier_done = Barrier::new(threads + 1);
    let ops_total_cell = AtomicU64::new(0);

    let elapsed_ms = std::thread::scope(|s| {
        let stack = &stack;
        let timed_start_cell = &timed_start_cell;
        let barrier_ready = &barrier_ready;
        let barrier_window = &barrier_window;
        let barrier_done = &barrier_done;
        let ops_total_cell = &ops_total_cell;

        for _thread_id in 0..threads {
            s.spawn(move || {
                barrier_ready.wait();
                barrier_window.wait();
                let timed_start = *timed_start_cell
                    .get()
                    .expect("coordinator publishes the timed window before releasing barrier_window");
                let deadline = timed_start + Duration::from_millis(window_ms);

                // Uncounted warm-up until the SHARED window opens: caches,
                // branch predictors and the contention steady-state settle
                // before any op is counted, and every thread's counted
                // window is the same one.
                let mut since_check = 0u32;
                loop {
                    if let Some(idx) = stack.pop() {
                        stack.push(idx);
                    }
                    since_check += 1;
                    if since_check >= DEADLINE_CHECK_INTERVAL {
                        since_check = 0;
                        if Instant::now() >= timed_start {
                            break;
                        }
                    }
                }

                // Counted window: pop-then-repush; only COMPLETED repushes
                // count (a None pop under transient drain counts nothing).
                let mut ops = 0u64;
                since_check = 0;
                loop {
                    if let Some(idx) = stack.pop() {
                        stack.push(idx);
                        ops += 1;
                    }
                    since_check += 1;
                    if since_check >= DEADLINE_CHECK_INTERVAL {
                        since_check = 0;
                        if Instant::now() >= deadline {
                            break;
                        }
                    }
                }
                ops_total_cell.fetch_add(ops, Ordering::Relaxed);
                barrier_done.wait();
            });
        }

        // Coordinator: rendezvous, publish the shared window, release.
        barrier_ready.wait();
        let timed_start = Instant::now();
        timed_start_cell
            .set(timed_start)
            .expect("timed window published exactly once");
        barrier_window.wait();
        barrier_done.wait();
        u64::try_from(
            timed_start
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)),
        )
        .unwrap_or(u64::MAX)
    });

    let ops_total = ops_total_cell.load(Ordering::Relaxed);
    let elapsed_ms_f = elapsed_ms.max(1) as f64;
    let ops_per_sec = ops_total as f64 / (elapsed_ms_f / 1000.0);
    let (pop_retries_after, push_retries_after) = {{CRATE_NAME}}::retry_counts_for_test();

    // EXACTLY ONE JSON line per invocation.
    println!(
        "{{\"variant\":\"{variant}\",\"threads\":{threads},\"window_ms\":{window_ms},\
\"elapsed_ms\":{elapsed_ms},\"ops_total\":{ops_total},\"ops_per_sec\":{ops_per_sec:.2},\
\"push_retries\":{},\"pop_retries\":{},\"smoke\":{smoke}}}",
        push_retries_after.saturating_sub(push_retries_before),
        pop_retries_after.saturating_sub(pop_retries_before),
    );
}
