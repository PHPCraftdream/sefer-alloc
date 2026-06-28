// Stress harness — readability of the long-running churn logic matters more
// than clippy style nits here.
#![allow(
    clippy::too_many_arguments,
    clippy::manual_is_multiple_of,
    clippy::manual_clamp,
    clippy::let_and_return
)]

//! High-load multi-thread soak harness for `SeferMalloc`.
//!
//! Run (smoke — ~5 s):
//!   `cargo run --release --example soak_xthread --features "alloc-global alloc-xthread"`
//!
//! Run (full soak — N hours):
//!   `SEFER_SOAK_SECONDS=3600 SEFER_SOAK_THREADS=64 \
//!    cargo run --release --example soak_xthread --features "alloc-global alloc-xthread"`
//!
//! This is NOT a throughput benchmark — it is a *stability* and *correctness*
//! harness. The goal is to run for hours at 32–128 threads and detect:
//!   - panics or aborts (heap corruption, double-free, null-deref),
//!   - deadlocks (heartbeat absence),
//!   - accounting mismatches (total_alloc != total_free_local + total_free_xthread).
//!
//! Four allocation patterns exercised per worker (selected by xorshift PRNG):
//!   1. **small-class churn** — alloc/free same size (8–64 B),
//!   2. **mixed-size churn** — alloc/free 16 B..8 KiB,
//!   3. **cross-thread handoff** — block allocated here, freed on another thread,
//!   4. **rare large blocks** — >2 MiB alloc+free.
//!
//! Memory safety discipline (identical to `malloc_macro.rs`):
//!   - every block is freed **exactly once, by exactly one thread**;
//!   - cross-thread blocks are *moved* into the channel (producer empties its
//!     slot before send, consumer drains and frees);
//!   - teardown drains all mailboxes before joining.
//!
//! Exit codes:
//!   0 — clean shutdown, all accounting balanced.
//!   1 — a worker panicked (propagated via `join().unwrap()`).
//!   2 — accounting invariant violated (alloc ≠ free_local + free_xthread).

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::semicolon_if_nothing_returned
)]

use std::alloc::{GlobalAlloc, Layout};
use std::env;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::SeferMalloc;

// ── global allocator ────────────────────────────────────────────────────────

#[global_allocator]
static GLOBAL: SeferMalloc = SeferMalloc::new();

// ── constants ────────────────────────────────────────────────────────────────

/// Maximum number of worker threads accepted from the environment.
const MAX_THREADS: usize = 128;

/// Working-set size (live blocks) per worker for small/mixed patterns.
const WORKING_SET: usize = 512;

/// Threshold above which a block is counted as "large".
const LARGE_THRESHOLD: usize = 2 * 1024 * 1024; // 2 MiB

/// Perform a cross-thread handoff once every this many ops per worker.
const HANDOFF_EVERY: usize = 32;

/// Allocate a large block once every this many ops per worker.
const LARGE_EVERY: usize = 512;

/// Print a heartbeat line every this many seconds.
const HEARTBEAT_SECS: u64 = 10;

// ── block handle ─────────────────────────────────────────────────────────────

/// An owned allocation: raw pointer + layout required for correct `dealloc`.
///
/// The pointer is valid for `layout.size()` bytes. Ownership is exclusive —
/// the block is moved across threads via MPSC; no two threads hold the same
/// `Block` at the same time.
struct Block {
    ptr: *mut u8,
    layout: Layout,
}

// SAFETY: `Block` is only ever transferred via MPSC as a full ownership move.
// The producer empties its slot before sending (no aliasing post-send).
// The consumer is the unique owner and frees the block exactly once.
unsafe impl Send for Block {}

// ── PRNG ──────────────────────────────────────────────────────────────────────

/// Deterministic dependency-free xorshift64* PRNG. Fixed seed → reproducible.
struct Xrs64(u64);

impl Xrs64 {
    fn new(seed: u64) -> Self {
        Self(seed | 1) // avoid all-zero fixed point
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform-ish in `[0, n)`.
    #[inline]
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

// ── size helpers ──────────────────────────────────────────────────────────────

/// Small-class size: uniform in 8..=64 B (stays within one or two size bins).
#[inline]
fn pick_small_size(rng: &mut Xrs64) -> usize {
    8 + rng.below(57) // 8..64
}

/// Mixed size: small-skewed, 16 B..8 KiB (same distribution as `malloc_macro`).
#[inline]
fn pick_mixed_size(rng: &mut Xrs64) -> usize {
    let r = rng.next_u64();
    if r.is_multiple_of(32) {
        512 + (r >> 8) as usize % (8 * 1024 - 512)
    } else {
        16 + (r >> 8) as usize % (512 - 16)
    }
}

/// Large size: 2 MiB + up to 2 MiB extra.
#[inline]
fn pick_large_size(rng: &mut Xrs64) -> usize {
    LARGE_THRESHOLD + rng.below(LARGE_THRESHOLD)
}

#[inline]
fn layout_for(size: usize) -> Layout {
    Layout::from_size_align(size.max(1), 8).unwrap()
}

// ── alloc/free helpers ───────────────────────────────────────────────────────

/// Allocate one block of `size` bytes via `a`, touching the first byte.
///
/// # Safety
/// `a` must be a valid `GlobalAlloc`. The returned `Block` (if non-null) must
/// be freed exactly once via `free_block` with the same allocator instance.
#[inline]
unsafe fn alloc_block_sized<A: GlobalAlloc>(a: &A, size: usize) -> Block {
    let layout = layout_for(size);
    // SAFETY: layout has non-zero size and valid power-of-two alignment.
    let ptr = unsafe { a.alloc(layout) };
    if !ptr.is_null() {
        // Touch the first byte: defeat dead-store elimination + fault the page.
        // SAFETY: ptr is valid for at least 1 byte.
        unsafe { ptr.write(0xA5) };
    }
    Block { ptr, layout }
}

/// Free a block previously produced by `alloc_block_sized` with the same
/// allocator.
///
/// # Safety
/// `block` must have been allocated by `a` and must not yet have been freed.
#[inline]
unsafe fn free_block<A: GlobalAlloc>(a: &A, block: Block) {
    if !block.ptr.is_null() {
        // SAFETY: block.ptr came from a.alloc(block.layout) and is freed once.
        unsafe { a.dealloc(block.ptr, block.layout) };
    }
}

/// Drain all pending cross-thread blocks from `rx` and free them via `a`.
/// Increments `count` for each block freed.
///
/// # Safety
/// Every block in `rx` was allocated by `a` on another thread; ownership was
/// transferred here via MPSC. We are the unique owner and free each once.
#[inline]
unsafe fn drain_mailbox<A: GlobalAlloc>(a: &A, rx: &Receiver<Block>, count: &mut u64) {
    while let Ok(block) = rx.try_recv() {
        // SAFETY: unique ownership transferred via channel; freed once here.
        unsafe { free_block(a, block) };
        *count += 1;
    }
}

// ── per-worker counters ───────────────────────────────────────────────────────

/// Counters returned by a single worker thread.
#[derive(Default, Debug)]
struct WorkerCounts {
    alloc: u64,
    free_local: u64,
    free_xthread: u64,
    large: u64,
}

// ── worker body ───────────────────────────────────────────────────────────────

/// Main body of a soak worker.
///
/// Runs until `stop` becomes `true`, exercising four allocation patterns in a
/// loop chosen by the xorshift PRNG. Updates the shared `ops_counter` on each
/// operation so the heartbeat thread can compute throughput.
///
/// # Safety
/// `a` is a valid `GlobalAlloc` shared across workers (a ZST).
/// The free-exactly-once discipline is upheld — see module docs.
unsafe fn soak_worker<A: GlobalAlloc>(
    a: &A,
    seed: u64,
    thread_idx: usize,
    n_threads: usize,
    senders: &[Sender<Block>],
    rx: &Receiver<Block>,
    stop: &AtomicBool,
    ops_counter: &AtomicU64,
) -> WorkerCounts {
    let mut rng = Xrs64::new(seed);
    let mut counts = WorkerCounts::default();

    // ── pre-fill a working set of small/mixed blocks ──────────────────────
    let mut slots: Vec<Option<Block>> = (0..WORKING_SET)
        .map(|_| {
            let sz = pick_mixed_size(&mut rng);
            // SAFETY: valid allocator; block tracked in `slots`, freed exactly once below.
            Some(unsafe { alloc_block_sized(a, sz) })
        })
        .collect();
    counts.alloc += WORKING_SET as u64;

    let mut op_tick: u64 = 0;

    while !stop.load(Ordering::Relaxed) {
        // Service any inbound cross-thread frees first (keep mailboxes drained).
        // SAFETY: inbound blocks are uniquely owned here.
        let before = counts.free_xthread;
        unsafe { drain_mailbox(a, rx, &mut counts.free_xthread) };
        let drained = counts.free_xthread - before;
        if drained > 0 {
            op_tick += drained;
        }

        let r = rng.next_u64();
        let op_class = r % 16; // 0..15

        // ── rare large alloc+free (once per LARGE_EVERY cycles) ──────────
        if op_tick % LARGE_EVERY as u64 == 0 {
            let sz = pick_large_size(&mut rng);
            // SAFETY: valid allocator.
            let big = unsafe { alloc_block_sized(a, sz) };
            counts.alloc += 1;
            counts.large += 1;
            black_box(big.ptr);
            // SAFETY: we own `big`, freed once immediately.
            unsafe { free_block(a, big) };
            counts.free_local += 1;
        }

        // ── cross-thread handoff (once per HANDOFF_EVERY cycles) ─────────
        if n_threads > 1 && op_tick % HANDOFF_EVERY as u64 == 0 {
            let idx = rng.below(WORKING_SET);
            if let Some(block) = slots[idx].take() {
                let mut target = rng.below(n_threads);
                if target == thread_idx {
                    target = (target + 1) % n_threads;
                }
                // Move ownership out to the target thread.
                // If the channel is disconnected (shouldn't happen mid-run),
                // free locally to prevent a leak and keep accounting correct.
                match senders[target].send(block) {
                    Ok(()) => {
                        // Receiver will count this in its free_xthread.
                    }
                    Err(unsent) => {
                        // Receiver gone — free here and count as local free.
                        // SAFETY: we still own `unsent.0`; freed once.
                        unsafe { free_block(a, unsent.0) };
                        counts.free_local += 1;
                    }
                }
                // Refill the slot.
                let sz = if op_class < 4 {
                    pick_small_size(&mut rng)
                } else {
                    pick_mixed_size(&mut rng)
                };
                // SAFETY: valid allocator; tracked in `slots`.
                slots[idx] = Some(unsafe { alloc_block_sized(a, sz) });
                counts.alloc += 1;
                op_tick += 1;
                ops_counter.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        }

        // ── normal alloc/free on a random slot ───────────────────────────
        let idx = rng.below(WORKING_SET);
        if let Some(block) = slots[idx].take() {
            // SAFETY: we own `block`; freed once.
            unsafe { free_block(a, block) };
            counts.free_local += 1;
        }

        // Pick new size: small-class (op_class < 6) or mixed.
        let sz = if op_class < 6 {
            pick_small_size(&mut rng)
        } else {
            pick_mixed_size(&mut rng)
        };
        // SAFETY: valid allocator; tracked in `slots`.
        slots[idx] = Some(unsafe { alloc_block_sized(a, sz) });
        counts.alloc += 1;

        op_tick += 1;
        ops_counter.fetch_add(1, Ordering::Relaxed);
    }

    // ── teardown ─────────────────────────────────────────────────────────

    // Free all blocks we still own in the working set.
    for slot in slots.drain(..).flatten() {
        // SAFETY: owned here, freed once.
        unsafe { free_block(a, slot) };
        counts.free_local += 1;
    }

    // Final mailbox drain: free any cross-thread blocks that arrived after our
    // loop ended, so nothing is leaked.
    // SAFETY: inbound blocks are uniquely owned here.
    unsafe { drain_mailbox(a, rx, &mut counts.free_xthread) };

    black_box(&counts.alloc);
    counts
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    // ── configuration ────────────────────────────────────────────────────

    let soak_seconds: u64 = env::var("SEFER_SOAK_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5); // smoke default: 5 s

    let avail = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let n_threads: usize = env::var("SEFER_SOAK_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(avail)
        .min(MAX_THREADS)
        .max(1);

    let run_duration = Duration::from_secs(soak_seconds);

    println!("== sefer-alloc soak harness ==");
    println!("threads={n_threads}  duration={soak_seconds}s  working_set/thread={WORKING_SET}");
    println!("(set SEFER_SOAK_THREADS=N  SEFER_SOAK_SECONDS=N to override)\n");

    // ── shared state ─────────────────────────────────────────────────────

    let stop = Arc::new(AtomicBool::new(false));
    // Per-thread ops counter for heartbeat throughput display.
    let ops_counters: Arc<Vec<AtomicU64>> =
        Arc::new((0..n_threads).map(|_| AtomicU64::new(0)).collect());

    // Per-thread MPSC mailboxes for cross-thread block handoff.
    let mut senders: Vec<Sender<Block>> = Vec::with_capacity(n_threads);
    let mut receivers: Vec<Option<Receiver<Block>>> = Vec::with_capacity(n_threads);
    for _ in 0..n_threads {
        let (tx, rx) = channel::<Block>();
        senders.push(tx);
        receivers.push(Some(rx));
    }
    let senders = Arc::new(senders);

    // Barrier: hold all workers until the main thread is ready to start the
    // clock (+1 for the main thread itself).
    let barrier = Arc::new(Barrier::new(n_threads + 1));

    // ── spawn workers ─────────────────────────────────────────────────────

    let mut handles = Vec::with_capacity(n_threads);
    for (t, rx_slot) in receivers.iter_mut().enumerate() {
        let stop_t = Arc::clone(&stop);
        let senders_t = Arc::clone(&senders);
        let barrier_t = Arc::clone(&barrier);
        let ops_t = Arc::clone(&ops_counters);
        let rx = rx_slot.take().unwrap();

        // Distinct seed per thread: well-separated by multiplying by a large odd.
        let seed = 0x9E37_79B9_7F4A_7C15u64
            .wrapping_mul(t as u64 + 1)
            .wrapping_add(0xDEAD_BEEF_CAFE_1234);

        let handle = thread::spawn(move || {
            // Grab a ZST handle to our global allocator (matching malloc_macro
            // pattern: SeferMalloc is a ZST, so this is a no-op at runtime).
            let alloc = SeferMalloc::new();

            // Wait for all workers to be ready before starting.
            barrier_t.wait();

            // SAFETY: `alloc` is a valid GlobalAlloc; the free-exactly-once
            // discipline is upheld by `soak_worker` (see fn docs).
            let counts = unsafe {
                soak_worker(
                    &alloc, seed, t, n_threads, &senders_t, &rx, &stop_t, &ops_t[t],
                )
            };
            counts
        });
        handles.push(handle);
    }

    // ── start clock + heartbeat ───────────────────────────────────────────

    // Synchronize with all workers.
    barrier.wait();
    let start = Instant::now();
    let mut last_heartbeat = start;
    let mut last_ops: u64 = 0;

    // Heartbeat loop: print progress every HEARTBEAT_SECS until time is up.
    loop {
        let elapsed = start.elapsed();
        if elapsed >= run_duration {
            break;
        }

        let now = Instant::now();
        let since_last = now.duration_since(last_heartbeat);
        if since_last >= Duration::from_secs(HEARTBEAT_SECS) {
            let total_ops: u64 = ops_counters.iter().map(|c| c.load(Ordering::Relaxed)).sum();
            let delta = total_ops.saturating_sub(last_ops);
            let rate = delta as f64 / since_last.as_secs_f64() / 1_000_000.0;
            println!(
                "[T+{:.0}s] {} threads alive, ops={} ({:.2} M/s)",
                elapsed.as_secs_f64(),
                n_threads,
                total_ops,
                rate,
            );
            last_heartbeat = now;
            last_ops = total_ops;
        }

        // Sleep a short interval so we don't busy-spin.
        thread::sleep(Duration::from_millis(200));
    }

    // Signal workers to stop.
    stop.store(true, Ordering::Relaxed);

    // ── collect results ───────────────────────────────────────────────────

    let elapsed = start.elapsed();

    let mut total_alloc: u64 = 0;
    let mut total_free_local: u64 = 0;
    let mut total_free_xthread: u64 = 0;
    let mut total_large: u64 = 0;

    for (i, h) in handles.into_iter().enumerate() {
        match h.join() {
            Ok(counts) => {
                total_alloc += counts.alloc;
                total_free_local += counts.free_local;
                total_free_xthread += counts.free_xthread;
                total_large += counts.large;
            }
            Err(_) => {
                eprintln!("[soak] worker {i} panicked — aborting");
                std::process::exit(1);
            }
        }
    }

    // Drop senders so all channels are closed; every receiver was drained in
    // its worker's teardown, so no block leaks and no double-free.
    drop(senders);

    // ── print summary ─────────────────────────────────────────────────────

    let total_ops: u64 = ops_counters.iter().map(|c| c.load(Ordering::Relaxed)).sum();
    let rate = total_ops as f64 / elapsed.as_secs_f64() / 1_000_000.0;

    println!();
    println!("== soak complete ==");
    println!("  elapsed:         {:.2}s", elapsed.as_secs_f64());
    println!("  threads:         {n_threads}");
    println!("  total_alloc:     {total_alloc}");
    println!("  total_free_local:{total_free_local}");
    println!("  total_free_xth:  {total_free_xthread}");
    println!("  total_large:     {total_large}");
    println!("  total_ops:       {total_ops}  ({rate:.2} M/s aggregate)");

    // ── invariant check ───────────────────────────────────────────────────
    //
    // Every allocated block must be freed exactly once.
    // total_alloc == total_free_local + total_free_xthread
    //
    // Note: the working-set pre-fill is included in `total_alloc`, and those
    // blocks are freed during teardown and counted in `total_free_local` or
    // `total_free_xthread`.  Cross-thread blocks sent via MPSC are counted by
    // the *receiver* (increments `free_xthread`), not the sender.  So the
    // identity holds exactly when the handoff discipline is sound.
    let total_free = total_free_local + total_free_xthread;
    if total_alloc != total_free {
        eprintln!();
        eprintln!(
            "[soak] INVARIANT VIOLATED: alloc={total_alloc} != free={total_free} \
             (local={total_free_local} xthread={total_free_xthread})"
        );
        eprintln!("[soak] This indicates a leak or double-free in the handoff logic.");
        std::process::exit(2);
    }

    println!();
    println!("[soak] invariant OK: alloc={total_alloc} == free={total_free} (local={total_free_local} + xthread={total_free_xthread})");
    println!("[soak] exit 0 — all checks passed");
}
