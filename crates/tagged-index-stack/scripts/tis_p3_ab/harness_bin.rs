//! Wallclock harness for the P3-1/P3-2 A/B study (link-ordering and
//! strong-vs-weak CAS variants of `crates/tagged-index-stack`).
//!
//! Workload: N threads of pop-then-repush-exactly-what-you-popped against a
//! shared `ArrayIndexStack<16, 256>` prefilled with `0..64`, started from a
//! barrier under the same published-window protocol as
//! `crates/tagged-index-stack/benches/tagged_index_stack_bench.rs`
//! (coordinator publishes a FUTURE window anchor, `now + WARMUP`, after
//! full rendezvous; workers warm up uncounted until that anchor arrives,
//! entry-lateness-guarded; each worker then counts completed repushes
//! against the shared window with a BOUNDED OVERSHOOT, not an exact
//! `[timed_start, deadline)` cut — the deadline is only checked once per
//! `DEADLINE_CHECK_INTERVAL` iterations, AFTER that batch of work, so up
//! to `DEADLINE_CHECK_INTERVAL - 1` repushes per worker can complete past
//! `deadline` and still be counted; same honest-overshoot posture the
//! bench documents on its own timed loop).
//!
//! The driver spawns this binary once per (variant, sample) and reads ONE
//! JSON line from stdout (the final line of output). CAS-retry counters come
//! from the scratch crate's `retry_counts_for_test()` (gated on the scratch
//! crate's `test-internals` feature, default-on there only); the driver
//! snapshots deltas across the timed window by reading before/after values
//! reported here.
//!
//! Uses the public `push`/`pop` API only; `push` is `unsafe fn` (see
//! `crates/tagged-index-stack/src/imp.rs`'s `StackOps::push_index` doc for
//! the three-clause caller contract: link domain + liveness + exclusive ownership). The three call
//! sites below each carry a `// SAFETY:` justification and a
//! statement-scoped `#[allow(unsafe_code)]`; `#![deny(unsafe_code)]` below
//! still covers every other line in this file — this is not a blanket
//! "100% safe code" template anymore, since `3e83b1c` turned `push` unsafe.

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
/// bench's `DEADLINE_CHECK_INTERVAL`). Consequence in the COUNTED loop:
/// the check runs AFTER the batch it bounds, so up to `N - 1` repushes
/// per worker execute past `deadline` and are still counted — a bounded
/// overshoot carried by both the numerator and the shared elapsed
/// denominator, not an exact window cut (see the counted-loop comment).
const DEADLINE_CHECK_INTERVAL: u32 = 64;

/// Uncounted warm-up lead before the timed window opens: the coordinator
/// publishes `timed_start = Instant::now() + WARMUP` (a FUTURE anchor) at
/// full rendezvous, and workers run the workload uncounted until that
/// anchor arrives — letting caches, branch predictors and the contention
/// steady-state settle so the first counted iterations are representative
/// rather than cold-start-shaped. Same value as the bench's `WARMUP`
/// (`crates/tagged-index-stack/benches/tagged_index_stack_bench.rs`):
/// settling depends on the workload — the identical pop-then-repush shape
/// — not on the runtime-selected window length, so the fixed 200 ms is
/// correct even for the runner's short `--window-ms` smoke runs.
const WARMUP: Duration = Duration::from_millis(200);

/// Upper bound on how late a worker may enter the counted window after it
/// opens. Under the published-window protocol the coordinator computes the
/// window only AFTER every worker has reached the ready barrier, so a
/// worker's normal path from barrier release through warm-up to window
/// entry is one clock-check granularity (microseconds). Entering more than
/// `MAX_WINDOW_ENTRY_LATENESS` late means the thread was descheduled on
/// that path: its count would silently miss that fraction of the window
/// while the denominator still covers the full window — exactly the
/// failure mode this harness must never paper over — so the sample aborts
/// loudly instead of reporting a plausible-looking number. Same value as
/// the bench's `MAX_WINDOW_ENTRY_LATENESS`: the guarded path's length is
/// independent of `window_ms`.
const MAX_WINDOW_ENTRY_LATENESS: Duration = Duration::from_millis(100);

/// Fail fast on an unrecoverable harness error: a misconfigured run must
/// exit with a message naming the parameter, the value received, and the
/// valid range — not surface later as a mid-run panic that looks like a
/// crate bug — and a worker detecting a published-window protocol
/// violation (the entry-lateness guard in `main`) must kill the process
/// loudly too: a worker `panic!` cannot propagate through the `barrier_done`
/// rendezvous (a missing participant would deadlock every waiter), so a
/// hard `std::process::exit` is the only loud exit a worker has.
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
        // SAFETY: domain — `i` ranges over `0..PREFILL` (64), a strict
        // subset of `Stack`'s link domain `0..LINKS` (256). Liveness —
        // `stack` was just constructed by `Stack::new()` above and `i` has
        // never been pushed on it before, so it cannot currently be
        // reachable through any head sharing this stack's link cells.
        // Exclusive ownership — this loop runs alone on the main thread
        // BEFORE the `std::thread::scope` below spawns any worker, so no
        // other push of `i` can exist, let alone run concurrently with or
        // begin before this call returns; each `i` is fresh and pushed
        // exactly once, and on the `Ok(())` the `.expect` below demands,
        // publish/recycle authority for `i` transfers to the stack
        // (push_index clause 3).
        #[allow(unsafe_code)]
        unsafe {
            stack.push(i)
        }
        .expect("bounded measurement run never reaches TAG_MAX");
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

                // Uncounted warm-up until the SHARED window opens:
                // `timed_start` is a FUTURE anchor (`now + WARMUP`,
                // published before `barrier_window` released us), so this
                // loop genuinely runs the workload for ~WARMUP before any
                // op is counted — caches, branch predictors and the
                // contention steady-state settle — and every thread's
                // counted window is the same one.
                let mut since_check = 0u32;
                loop {
                    if let Some(idx) = stack.pop() {
                        // SAFETY: domain — `idx` was just returned by this
                        // stack's own `pop()`, so by the stack's invariant it
                        // is already a valid member of `stack`'s link domain.
                        // Liveness — `pop()` returning `Some(idx)` means this
                        // thread's CAS actually removed `idx` from the head
                        // chain, so it is not currently reachable through
                        // `stack`'s head and has not been re-pushed since.
                        // Exclusive ownership — that same successful `pop`
                        // transferred publish/recycle authority for `idx` to
                        // THIS thread (a pop is the only way an index leaves
                        // the stack, and only the winning popper's CAS takes
                        // a given published instance), and this thread
                        // re-pushes `idx` synchronously without ever sharing
                        // it, so no other push of `idx` can run concurrently
                        // with or begin before this call returns
                        // (push_index clause 3).
                        #[allow(unsafe_code)]
                        unsafe {
                            stack.push(idx)
                        }
                        .expect("bounded measurement run never reaches TAG_MAX");
                    }
                    since_check += 1;
                    if since_check >= DEADLINE_CHECK_INTERVAL {
                        since_check = 0;
                        if Instant::now() >= timed_start {
                            break;
                        }
                    }
                }

                // Entry-lateness guard (bench shape): under the
                // published-window protocol the only way to reach here
                // late is being descheduled on the path from the barrier
                // rendezvous through warm-up to window entry, which would
                // silently shorten this thread's count while the shared
                // denominator still covers the full window. Aborts the
                // process via `die()` instead of `panic!`: a panicked
                // worker never arrives at `barrier_done`, and a Barrier
                // has no poison mechanism — every other participant would
                // hang forever.
                let entered = Instant::now();
                let entry_lateness = entered.duration_since(timed_start);
                if entry_lateness > MAX_WINDOW_ENTRY_LATENESS {
                    die(format!(
                        "worker entered the counted window {entry_lateness:?} after it opened \
                         (allowed up to {MAX_WINDOW_ENTRY_LATENESS:?}) -- the thread was stalled \
                         on its way from the barrier rendezvous to the window opening, so part \
                         of the shared window would silently be missing from its count while \
                         the elapsed denominator still covers the full window; aborting loudly \
                         instead of reporting a plausible-looking number"
                    ));
                }

                // Counted window: pop-then-repush; only COMPLETED repushes
                // count (a None pop under transient drain counts nothing).
                // Bounded overshoot, NOT an exact `[timed_start, deadline)`
                // cut: the deadline below is checked once per
                // DEADLINE_CHECK_INTERVAL iterations AFTER that batch of
                // work, so up to DEADLINE_CHECK_INTERVAL - 1 repushes per
                // worker can complete past `deadline` and are still
                // counted. Elapsed runs from the shared anchor to the last
                // worker's `barrier_done`, so the overshoot lands in
                // numerator and denominator alike instead of being hidden;
                // per-worker finish times differ, so early workers stop
                // adding to the numerator before the shared denominator
                // closes — accepted as direction-neutral noise for
                // symmetric A/B arms, not papered over as an exact window.
                let mut ops = 0u64;
                since_check = 0;
                loop {
                    if let Some(idx) = stack.pop() {
                        // SAFETY: same argument as the warm-up loop above,
                        // all three clauses — `idx` just came out of this
                        // stack's own `pop()`, so it is domain-valid;
                        // because `pop()` returned `Some`, it is not
                        // currently live anywhere else; and that successful
                        // pop transferred exclusive publish/recycle
                        // authority for `idx` to this thread, which
                        // re-pushes it synchronously without sharing it
                        // (push_index clause 3).
                        #[allow(unsafe_code)]
                        unsafe {
                            stack.push(idx)
                        }
                        .expect("bounded measurement run never reaches TAG_MAX");
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

        // Coordinator: after every worker has announced readiness, the
        // rendezvous itself provides the happens-before edge — the value
        // set here after `barrier_ready.wait()` is visible to every worker
        // after their `barrier_window.wait()`. The anchor is a FUTURE
        // instant (`now + WARMUP`, bench shape): everyone released by
        // `barrier_window` warms up uncounted until it arrives, instead of
        // the window already being open at the workers' first clock check.
        barrier_ready.wait();
        let timed_start = Instant::now() + WARMUP;
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
