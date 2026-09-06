//! Wall-clock harness for the production-shaped registry storage study.

#![deny(unsafe_code)]

use std::env;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
#[cfg(tagged_index_stack_test)]
use std::sync::atomic::AtomicBool;
use std::sync::{Barrier, OnceLock};
use std::time::{Duration, Instant};

use {{CRATE_NAME}}::{StackHead, StackOps, StackStorage};

const LINKS: usize = 256;
const PREFILL: u32 = 64;
const DEADLINE_CHECK_INTERVAL: u32 = 64;
const WARMUP: Duration = Duration::from_millis(200);
const MAX_WINDOW_MS: u64 = 60_000;
const MAX_WINDOW_ENTRY_LATENESS: Duration = Duration::from_millis(100);
const MATERIALIZED_VARIANT: &str = "{{VARIANT_NAME}}";
#[cfg(tagged_index_stack_test)]
const EXPECTED_STORE_NEXT_CALLS: u64 = {{EXPECTED_STORE_NEXT_CALLS}};
#[cfg(tagged_index_stack_test)]
const ORACLE_A: u32 = 0;
#[cfg(tagged_index_stack_test)]
const ORACLE_X: u32 = 1;

#[repr(align(64))]
struct RegistrySlot {
    next_free: AtomicU32,
}

#[cfg(tagged_index_stack_test)]
struct ActivationProbe {
    x_first_store_entered: Barrier,
    allow_x_first_store: Barrier,
    armed: AtomicBool,
    x_first_store_seen: AtomicBool,
    store_next_calls: AtomicU64,
}

#[cfg(tagged_index_stack_test)]
impl ActivationProbe {
    fn new() -> Self {
        Self {
            x_first_store_entered: Barrier::new(2),
            allow_x_first_store: Barrier::new(2),
            armed: AtomicBool::new(false),
            x_first_store_seen: AtomicBool::new(false),
            store_next_calls: AtomicU64::new(0),
        }
    }
}

impl RegistrySlot {
    const fn new() -> Self {
        Self { next_free: AtomicU32::new(0) }
    }
}

struct RegistryShapedStorage {
    head: StackHead<16>,
    slots: [RegistrySlot; LINKS],
    #[cfg(tagged_index_stack_test)]
    activation_probe: ActivationProbe,
}

// This models Registry's ownership shape, not HeapSlot's byte layout.

impl RegistryShapedStorage {
    fn new() -> Self {
        Self {
            head: StackHead::new(),
            slots: [const { RegistrySlot::new() }; LINKS],
            #[cfg(tagged_index_stack_test)]
            activation_probe: ActivationProbe::new(),
        }
    }

    fn slot(&self, index: u32) -> &RegistrySlot {
        &self.slots[index as usize]
    }
}

// SAFETY:
// 1. `self.head` has one binding for this storage value's lifetime.
// 2. `slots[index]` is a stable mapping used by both link hooks.
// 3. Distinct values share neither heads, cells, nor populations.
// 4. Every valid index has one dedicated cell holding TAIL or a domain index.
// 5. `head()` always returns this value's same head.
// 6. The domain is exactly 0..256, within the 16-bit index domain.
// 7. AtomicU32 makes races atomic; placeholders select Acquire/Release for
//    base and the intentional Relaxed candidate.
#[allow(unsafe_code)]
unsafe impl StackStorage<16> for RegistryShapedStorage {
    /// # Safety
    ///
    /// The caller uses this head only with this storage's own link binding.
    unsafe fn head(&self) -> &StackHead<16> {
        &self.head
    }

    /// # Safety
    ///
    /// `index` is in 0..256 and was initialized through this same binding.
    unsafe fn load_next(&self, index: u32) -> u32 {
        self.slot(index).next_free.load({{LINK_LOAD_ORDERING}})
    }

    /// # Safety
    ///
    /// `index` is in-domain, non-live, and uniquely owned; `next` is TAIL or
    /// the observed in-domain head, and this store precedes publication.
    unsafe fn store_next(&self, index: u32, next: u32) {
        #[cfg(tagged_index_stack_test)]
        {
            self.activation_probe.store_next_calls.fetch_add(1, Ordering::Relaxed);
            if self.activation_probe.armed.load(Ordering::Acquire)
                && index == ORACLE_X
                && !self.activation_probe.x_first_store_seen.swap(true, Ordering::AcqRel)
            {
                self.activation_probe.x_first_store_entered.wait();
                self.activation_probe.allow_x_first_store.wait();
            }
        }
        self.slot(index).next_free.store(next, {{LINK_STORE_ORDERING}});
    }
}

type Stack = RegistryShapedStorage;

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
    let Some(index) = stack.pop_index() else { return false };
    // SAFETY: this binding returned an in-domain index; the successful pop
    // removed it from the live chain and gave this thread its unique recycle
    // authority, which this push consumes exactly once.
    #[allow(unsafe_code)]
    unsafe { stack.push_index(index) }
        .expect("bounded measurement run never reaches TAG_MAX");
    true
}

#[cfg(tagged_index_stack_test)]
fn run_activation_oracle() {
    let smoke = env::var("TIS_AB_SMOKE").as_deref() == Ok("1");
    let stack = Stack::new();
    // SAFETY: the fresh binding owns A's first publication authority and A is
    // in-domain; this establishes the initial top before the measured leg.
    if let Err(_) = {
        #[allow(unsafe_code)]
        unsafe { stack.push_index(ORACLE_A) }
    } {
        die(String::from("activation oracle: initial push of A unexpectedly sealed"));
    }
    stack.activation_probe.store_next_calls.store(0, Ordering::Relaxed);
    stack.activation_probe.armed.store(true, Ordering::Release);
    let (pop_before, push_before) = retry_counts();
    let x_result = std::thread::scope(|scope| {
        let x = scope.spawn(|| {
            // SAFETY: X is in-domain, non-live, and its unique publication
            // authority belongs to this worker for the whole push call.
            {
                #[allow(unsafe_code)]
                unsafe { stack.push_index(ORACLE_X) }
            }
                .map_err(|_| String::from("activation oracle: X push sealed"))
        });

        // X is blocked inside its first store_next. The coordinator now pops
        // A and republishes A, changing only its tag while keeping its index.
        stack.activation_probe.x_first_store_entered.wait();
        let coordinator_result = if stack.pop_index() != Some(ORACLE_A) {
            Err(String::from("activation oracle: coordinator did not pop A"))
        } else {
            // SAFETY: pop returned A to this coordinator, which owns its one
            // recycled publication authority; A is still in-domain and non-live.
            {
                #[allow(unsafe_code)]
                unsafe { stack.push_index(ORACLE_A) }
            }
                .map_err(|_| String::from("activation oracle: coordinator push of A sealed"))
        };
        stack.activation_probe.allow_x_first_store.wait();
        let worker_result = match x.join() {
            Ok(result) => result,
            Err(_) => Err(String::from("activation oracle: X worker panicked")),
        };
        match (coordinator_result, worker_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(message), _) | (_, Err(message)) => Err(message),
        }
    });
    if let Err(message) = x_result {
        die(message);
    }
    stack.activation_probe.armed.store(false, Ordering::Release);
    let (pop_after, push_after) = retry_counts();
    let push_retries = push_after.saturating_sub(push_before);
    let pop_retries = pop_after.saturating_sub(pop_before);
    let store_next_calls = stack.activation_probe.store_next_calls.load(Ordering::Relaxed);
    if push_retries != 1 || pop_retries != 0 || store_next_calls != EXPECTED_STORE_NEXT_CALLS {
        die(format!(
            "activation oracle mismatch for {MATERIALIZED_VARIANT}: push_retries={push_retries}, pop_retries={pop_retries}, store_next_calls={store_next_calls}, expected push=1 pop=0 store={EXPECTED_STORE_NEXT_CALLS}"
        ));
    }
    println!(
        "{{\"variant\":\"{MATERIALIZED_VARIANT}\",\"source_variant\":\"{MATERIALIZED_VARIANT}\",\"threads\":0,\"window_ms\":0,\"elapsed_ms\":0,\"ops_total\":0,\"ops_per_sec\":0.0,\"push_retries\":0,\"pop_retries\":0,\"activation_push_retries\":{push_retries},\"activation_pop_retries\":{pop_retries},\"activation_store_next_calls\":{store_next_calls},\"activation_probe\":\"tag_only_retry\",\"activation\":true,\"smoke\":{smoke}}}"
    );
}

fn main() {
    #[cfg(tagged_index_stack_test)]
    if env::var("TIS_AB_ACTIVATION_ORACLE").as_deref() == Ok("1") {
        run_activation_oracle();
        return;
    }
    let threads: usize = parse_env("TIS_AB_THREADS", 4);
    let window_ms: u64 = parse_env("TIS_AB_WINDOW_MS", 1_000);
    let smoke = env::var("TIS_AB_SMOKE").as_deref() == Ok("1");
    let variant = MATERIALIZED_VARIANT;
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
        // SAFETY: 0..PREFILL is inside this binding's 0..256 domain; the fresh
        // stack makes every index non-live, and this loop owns and consumes one
        // unique initial publish authority for each index exactly once.
        #[allow(unsafe_code)]
        unsafe { stack.push_index(index) }
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
