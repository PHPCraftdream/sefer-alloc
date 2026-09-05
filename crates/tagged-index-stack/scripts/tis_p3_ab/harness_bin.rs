//! Wall-clock harness for the tagged-index-stack link-ordering/CAS study.
//!
//! The production binary is built without `tagged_index_stack_test` and is
//! the only binary whose samples enter the timing CSV. A separate cfg-enabled
//! binary observes retry activation after warm-up; its result is never mixed
//! with timing samples.

#![deny(unsafe_code)]

use std::env;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Barrier, OnceLock};
use std::time::{Duration, Instant};

use {{CRATE_NAME}}::ArrayIndexStack;

type Stack = ArrayIndexStack<16, LINKS>;
const LINKS: usize = 256;
const PREFILL: u32 = 64;
const DEADLINE_CHECK_INTERVAL: u32 = 64;
const WARMUP: Duration = Duration::from_millis(200);
const MAX_WINDOW_ENTRY_LATENESS: Duration = Duration::from_millis(100);
const MAX_WINDOW_MS: u64 = 60_000;

fn die(msg: String) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(2);
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> T {
    match env::var(key) {
        Err(env::VarError::NotPresent) => default,
        Err(env::VarError::NotUnicode(_)) => die(format!("{key}: value is not valid Unicode")),
        Ok(raw) => raw.trim().parse::<T>().unwrap_or_else(|_| {
            die(format!("{key}: invalid value {raw:?} (expected {})", std::any::type_name::<T>()))
        }),
    }
}

#[cfg(tagged_index_stack_test)]
fn retry_counts() -> (usize, usize) {
    {{CRATE_NAME}}::retry_counts_for_test()
}

#[cfg(not(tagged_index_stack_test))]
fn retry_counts() -> (usize, usize) {
    (0, 0)
}

fn cycle(stack: &Stack) -> bool {
    let Some(index) = stack.pop() else { return false };
    // SAFETY: `index` was removed by this pop, so it is in-domain,
    // unreachable, and exclusively owned by this thread for the repush.
    #[allow(unsafe_code)]
    unsafe { stack.push(index) }
        .expect("bounded measurement run never reaches TAG_MAX");
    true
}

fn main() {
    let threads: usize = parse_env("TIS_AB_THREADS", 4);
    let window_ms: u64 = parse_env("TIS_AB_WINDOW_MS", 1_000);
    let smoke = env::var("TIS_AB_SMOKE").as_deref() == Ok("1");
    let variant = env::var("TIS_AB_VARIANT").unwrap_or_else(|_| String::from("unlabeled"));
    if !(1..=256).contains(&threads) {
        die(format!("TIS_AB_THREADS: value {threads} out of range (valid: 1..=256)"));
    }
    if !(50..=MAX_WINDOW_MS).contains(&window_ms) {
        die(format!("TIS_AB_WINDOW_MS: value {window_ms} out of range (valid: 50..={MAX_WINDOW_MS} ms)"));
    }
    let window = Duration::from_millis(window_ms);
    if Instant::now().checked_add(WARMUP).and_then(|t| t.checked_add(window)).is_none() {
        die(format!("TIS_AB_WINDOW_MS: value {window_ms} cannot form a representable deadline"));
    }

    let stack = Stack::new();
    for index in 0..PREFILL {
        // SAFETY: each fresh in-domain index is pushed once before sharing.
        #[allow(unsafe_code)]
        unsafe { stack.push(index) }
            .expect("bounded measurement run never reaches TAG_MAX");
    }

    let retry_before_cell: OnceLock<(usize, usize)> = OnceLock::new();
    let warmup_deadline_cell: OnceLock<Instant> = OnceLock::new();
    let observed_start_cell: OnceLock<Instant> = OnceLock::new();
    let barrier_ready = Barrier::new(threads + 1);
    let barrier_start = Barrier::new(threads + 1);
    let barrier_warmup = Barrier::new(threads + 1);
    let barrier_observed = Barrier::new(threads + 1);
    let barrier_done = Barrier::new(threads + 1);
    let ops_total_cell = AtomicU64::new(0);

    let elapsed_ms = std::thread::scope(|scope| {
        for _ in 0..threads {
            let stack = &stack;
            let warmup_deadline_cell = &warmup_deadline_cell;
            let observed_start_cell = &observed_start_cell;
            let barrier_ready = &barrier_ready;
            let barrier_start = &barrier_start;
            let barrier_warmup = &barrier_warmup;
            let barrier_observed = &barrier_observed;
            let barrier_done = &barrier_done;
            let ops_total_cell = &ops_total_cell;
            scope.spawn(move || {
                barrier_ready.wait();
                barrier_start.wait();
                let warmup_deadline = *warmup_deadline_cell
                    .get()
                    .expect("warm-up deadline published before release");
                while Instant::now() < warmup_deadline {
                    let _ = cycle(stack);
                }
                barrier_warmup.wait();
                barrier_observed.wait();
                let observed_start = *observed_start_cell
                    .get()
                    .expect("observed window published before release");
                let deadline = observed_start.checked_add(window).unwrap_or_else(|| {
                    die(String::from("observed_start + window overflows this platform's Instant range"))
                });
                let entered = Instant::now();
                if entered.duration_since(observed_start) > MAX_WINDOW_ENTRY_LATENESS {
                    die(format!("worker entered observed window late: {:?}", entered.duration_since(observed_start)));
                }
                let mut ops = 0u64;
                let mut since_check = 0u32;
                loop {
                    if cycle(stack) { ops += 1; }
                    since_check += 1;
                    if since_check >= DEADLINE_CHECK_INTERVAL {
                        since_check = 0;
                        if Instant::now() >= deadline { break; }
                    }
                }
                ops_total_cell.fetch_add(ops, Ordering::Relaxed);
                barrier_done.wait();
            });
        }

        barrier_ready.wait();
        let warmup_deadline = Instant::now().checked_add(WARMUP).unwrap_or_else(|| {
            die(String::from("now + WARMUP overflows this platform's Instant range"))
        });
        warmup_deadline_cell.set(warmup_deadline).expect("warm-up deadline published once");
        barrier_start.wait();
        barrier_warmup.wait();
        retry_before_cell.set(retry_counts()).expect("retry baseline published once");
        let observed_start = Instant::now();
        observed_start_cell.set(observed_start).expect("observed window published once");
        barrier_observed.wait();
        barrier_done.wait();
        u64::try_from(observed_start.elapsed().as_millis().min(u128::from(u64::MAX)))
            .unwrap_or(u64::MAX)
    });

    let ops_total = ops_total_cell.load(Ordering::Relaxed);
    let elapsed_ms_f = elapsed_ms.max(1) as f64;
    let ops_per_sec = ops_total as f64 / (elapsed_ms_f / 1000.0);
    let (pop_before, push_before) = retry_before_cell.get().copied().expect("retry baseline exists");
    let (pop_after, push_after) = retry_counts();
    println!(
        "{{\"variant\":\"{variant}\",\"threads\":{threads},\"window_ms\":{window_ms},\"elapsed_ms\":{elapsed_ms},\"ops_total\":{ops_total},\"ops_per_sec\":{ops_per_sec:.2},\"push_retries\":{},\"pop_retries\":{},\"activation\":{},\"smoke\":{smoke}}}",
        push_after.saturating_sub(push_before),
        pop_after.saturating_sub(pop_before),
        cfg!(tagged_index_stack_test),
    );
}
