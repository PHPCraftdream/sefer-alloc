//! `malloc-bench-rs` — portable, generic-over-`GlobalAlloc` benchmark harness.
//!
//! Run **larson** (server churn) + **mstress** (alloc-then-free batches)
//! workloads against ANY [`std::alloc::GlobalAlloc`], get aggregate ops/sec.
//! Pre-spawns worker threads, runs a fixed op budget per worker, times the
//! steady-state region (criterion-style per-iter timing mis-measures MT
//! workloads — thread spawn inside the timed closure dominates; we avoid that).
//!
//! # Design
//!
//! Two workloads, both reporting **aggregate ops/sec** (an op = one
//! alloc+free pair) over a fixed operation budget measured with
//! `Instant::elapsed`:
//!
//! - **larson** — server-churn: each thread keeps a working set of live
//!   slots; each step frees a random slot and allocates a new random-size
//!   block into it. Periodically a block is handed off cross-thread.
//! - **mstress** — rounds of "fill a vector of mixed blocks → free half in
//!   random order → refill → free all"; a fraction freed cross-thread.
//!
//! Cross-thread handoff is leak/UAF-free by construction: every allocated
//! block is freed **exactly once, by exactly one thread**. A handed-off block
//! is moved out of the producer's bookkeeping (its slot is set empty) before
//! being sent; the consumer drains its mailbox and frees each received block
//! once. At the end every thread frees its own remaining live blocks, then
//! drains any final mailbox contents — so nothing is dropped on the floor and
//! nothing is freed twice.
//!
//! # Determinism
//!
//! A dependency-free xorshift64 PRNG with a fixed per-thread seed, so runs
//! are reproducible. No external `rand` crate is required.
//!
//! # No `#[global_allocator]`
//!
//! The harness calls `GlobalAlloc::alloc`/`dealloc` trait methods directly —
//! it never sets `#[global_allocator]`. Your allocator does not need to be
//! registered globally; you can benchmark several allocators in one binary.
//!
//! # Example
//!
//! ```rust
//! use malloc_bench_rs::{run, Config, Workload};
//! use std::alloc::System;
//!
//! let cfg = Config {
//!     threads: 2,
//!     steps_per_thread: 10_000,
//!     working_set: 128,
//!     mstress_blocks: 64,
//! };
//! let ops = run(Workload::Larson, &cfg, || System);
//! assert!(ops > 0.0, "expected non-zero ops/sec");
//! ```

#![allow(unsafe_code)]
// Confined to alloc_block / free_block / drain_mailbox helpers.
// Each unsafe call is individually justified with a // SAFETY: comment.
#![deny(missing_docs)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::semicolon_if_nothing_returned
)]

use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

// ─── Internal primitives ──────────────────────────────────────────────────────

/// A live allocation: raw pointer plus the exact layout it was allocated with
/// (needed for a correct `dealloc`).
///
/// `Send` is asserted explicitly below — the block is logically *moved* to
/// the receiving thread, which becomes its sole owner; the producer no longer
/// touches it after sending.
struct Block {
    ptr: *mut u8,
    layout: Layout,
}

// SAFETY: a `Block` is only ever sent across threads as an ownership transfer
// (the producer empties its slot before sending; the consumer is the unique
// owner thereafter). No two threads ever hold the same `Block`, so there is no
// aliasing of the underlying allocation across threads.
unsafe impl Send for Block {}

/// Deterministic, dependency-free PRNG (xorshift64*). Fixed seed → reproducible.
struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        // Avoid the all-zero state (xorshift fixed point).
        Self(seed | 1)
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

    /// Uniform-ish in `[0, n)` (n > 0).
    #[inline]
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// Pick a small-skewed allocation size: mostly 16..512 B, rarely up to ~8 KiB.
#[inline]
fn pick_size(rng: &mut XorShift64) -> usize {
    let r = rng.next_u64();
    if r.is_multiple_of(32) {
        // ~3% large: 512 B .. 8 KiB.
        512 + (r >> 8) as usize % (8 * 1024 - 512)
    } else {
        // ~97% small: 16 .. 512 B.
        16 + (r >> 8) as usize % (512 - 16)
    }
}

#[inline]
fn layout_for(size: usize) -> Layout {
    // 8-byte alignment — matches the typical allocator test convention.
    Layout::from_size_align(size.max(1), 8).unwrap()
}

/// Allocate one block of a random small-skewed size, touching the first byte
/// so the allocation is not optimized away and the page is actually faulted in.
///
/// # Safety
///
/// `a` must be a valid `GlobalAlloc`. The returned `Block` (if non-null) must
/// be freed exactly once via `free_block` with the same allocator.
#[inline]
unsafe fn alloc_block<A: GlobalAlloc>(a: &A, rng: &mut XorShift64) -> Block {
    let layout = layout_for(pick_size(rng));
    // SAFETY: layout has non-zero size and valid alignment.
    let ptr = unsafe { a.alloc(layout) };
    if !ptr.is_null() {
        // Touch first byte to fault the page and defeat dead-store elimination.
        // SAFETY: ptr is valid for `layout.size() >= 1` bytes.
        unsafe { ptr.write(0xA5) };
    }
    Block { ptr, layout }
}

/// Free a block previously produced by `alloc_block` with the same allocator.
///
/// # Safety
///
/// `block` must have been allocated by `a` and not yet freed.
#[inline]
unsafe fn free_block<A: GlobalAlloc>(a: &A, block: Block) {
    if !block.ptr.is_null() {
        // SAFETY: block.ptr came from `a.alloc(block.layout)` and is freed once.
        unsafe { a.dealloc(block.ptr, block.layout) };
    }
}

/// Drain any blocks waiting in this thread's cross-thread mailbox and free them.
///
/// # Safety
///
/// Every received block was allocated by `a` on some thread and ownership was
/// transferred here; we are its unique owner and free it once.
#[inline]
unsafe fn drain_mailbox<A: GlobalAlloc>(a: &A, rx: &Receiver<Block>, count: &mut u64) {
    while let Ok(block) = rx.try_recv() {
        // SAFETY: unique ownership, freed once (see fn docs).
        unsafe { free_block(a, block) };
        *count += 1;
    }
}

// ─── Worker implementations ───────────────────────────────────────────────────

/// One larson worker. Returns the number of alloc+free *ops* it performed.
///
/// # Safety
///
/// `a` is a valid `GlobalAlloc` shared by all workers (a ZST or `Copy`
/// handle). The body upholds the per-block free-exactly-once discipline.
unsafe fn larson_worker<A: GlobalAlloc>(
    a: &A,
    seed: u64,
    steps: usize,
    working_set: usize,
    senders: &[Sender<Block>],
    rx: &Receiver<Block>,
    self_idx: usize,
) -> u64 {
    let mut rng = XorShift64::new(seed);
    let mut ops: u64 = 0;

    // Pre-fill the working set.
    let mut slots: Vec<Option<Block>> = Vec::with_capacity(working_set);
    for _ in 0..working_set {
        // SAFETY: valid allocator; block tracked in `slots`, freed once below.
        slots.push(Some(unsafe { alloc_block(a, &mut rng) }));
    }

    // Every K steps, hand a block to another thread for cross-thread free.
    const HANDOFF_EVERY: usize = 16;
    let n_threads = senders.len();

    for step in 0..steps {
        // Service any inbound cross-thread frees first (keeps mailboxes drained).
        // SAFETY: received blocks are uniquely owned here.
        unsafe { drain_mailbox(a, rx, &mut ops) };

        let idx = rng.below(working_set);

        if n_threads > 1 && step % HANDOFF_EVERY == 0 {
            // Hand this slot's block to another thread (move ownership out).
            if let Some(block) = slots[idx].take() {
                let mut target = rng.below(n_threads);
                if target == self_idx {
                    target = (target + 1) % n_threads;
                }
                // The producer no longer owns `block` after send; the slot is
                // now empty and will be refilled below.
                if senders[target].send(block).is_err() {
                    // Receiver gone (shouldn't happen during the run) — we
                    // can't send, so the block would be leaked. Free locally
                    // to stay UAF/leak-free. (Unreachable in practice.)
                }
            }
        } else if let Some(block) = slots[idx].take() {
            // Normal path: free the old block locally.
            // SAFETY: block was allocated by `a`, owned here, freed once.
            unsafe { free_block(a, block) };
        }

        // Refill the slot with a fresh allocation.
        // SAFETY: valid allocator; tracked in `slots`.
        slots[idx] = Some(unsafe { alloc_block(a, &mut rng) });
        ops += 1;
    }

    // Teardown: free every block we still own locally.
    for block in slots.drain(..).flatten() {
        // SAFETY: owned here, freed once.
        unsafe { free_block(a, block) };
    }
    black_box(&ops);
    ops
}

/// One mstress worker. Returns the number of alloc+free *ops* it performed.
///
/// # Safety
///
/// As `larson_worker`.
unsafe fn mstress_worker<A: GlobalAlloc>(
    a: &A,
    seed: u64,
    rounds: usize,
    block_count: usize,
    senders: &[Sender<Block>],
    rx: &Receiver<Block>,
    self_idx: usize,
) -> u64 {
    let mut rng = XorShift64::new(seed);
    let mut ops: u64 = 0;
    let n_threads = senders.len();

    for _ in 0..rounds {
        // SAFETY: inbound blocks uniquely owned here.
        unsafe { drain_mailbox(a, rx, &mut ops) };

        // Fill a vector with `block_count` mixed-size blocks.
        let mut blocks: Vec<Option<Block>> = Vec::with_capacity(block_count);
        for _ in 0..block_count {
            // SAFETY: valid allocator; tracked in `blocks`.
            blocks.push(Some(unsafe { alloc_block(a, &mut rng) }));
        }

        // Free half in random order; ~1 in 8 of those go cross-thread.
        let half = block_count / 2;
        for _ in 0..half {
            let idx = rng.below(block_count);
            if let Some(block) = blocks[idx].take() {
                if n_threads > 1 && rng.below(8) == 0 {
                    let mut target = rng.below(n_threads);
                    if target == self_idx {
                        target = (target + 1) % n_threads;
                    }
                    let _ = senders[target].send(block);
                } else {
                    // SAFETY: owned here, freed once.
                    unsafe { free_block(a, block) };
                }
                ops += 1;
            }
        }

        // Refill the now-empty slots.
        for slot in blocks.iter_mut() {
            if slot.is_none() {
                // SAFETY: valid allocator; tracked in `blocks`.
                *slot = Some(unsafe { alloc_block(a, &mut rng) });
            }
        }

        // Free everything remaining locally.
        for block in blocks.drain(..).flatten() {
            // SAFETY: owned here, freed once.
            unsafe { free_block(a, block) };
            ops += 1;
        }
    }

    black_box(&ops);
    ops
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Workload selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Workload {
    /// Server-churn pattern: each thread keeps a live working set; each step
    /// frees a random slot and allocates a new random-size block. Periodically
    /// a block is handed off cross-thread. Models long-running server heaps.
    Larson,
    /// Batch-stress pattern: rounds of "fill N blocks → free half in random
    /// order → refill → free all". A fraction freed cross-thread. Models
    /// request-scoped allocators and scripting-engine GCs.
    Mstress,
}

/// Configuration for a single benchmark run.
///
/// All fields have sensible defaults via [`Default`].
#[derive(Clone, Debug)]
pub struct Config {
    /// Number of worker threads to spawn. Defaults to
    /// `std::thread::available_parallelism()` (or 4 if unavailable).
    pub threads: usize,
    /// Number of alloc+free steps (ops) each thread performs. Larger values
    /// give more stable measurements; smaller values run faster.
    /// Default: 200 000.
    pub steps_per_thread: usize,
    /// **Larson only.** Number of live blocks each thread keeps in its working
    /// set. Default: 512.
    pub working_set: usize,
    /// **Mstress only.** Number of blocks per mstress round.
    /// Default: 256.
    pub mstress_blocks: usize,
}

impl Default for Config {
    fn default() -> Self {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Self {
            threads,
            steps_per_thread: 200_000,
            working_set: 512,
            mstress_blocks: 256,
        }
    }
}

/// Run one workload × allocator × thread-count and return aggregate ops/sec.
///
/// `A` is your allocator — typically a ZST (`System`, `mimalloc::MiMalloc`,
/// etc.). A fresh instance is constructed **per thread** via `make_alloc`.
///
/// # Contract on `A`: stateless facade over shared global state
///
/// Because a block allocated on one thread may be handed off (via mpsc) and
/// freed on ANOTHER thread — and each thread holds its OWN `A` instance —
/// `A` MUST be a stateless facade over shared/global allocator state (e.g.
/// [`std::alloc::System`], or `SeferAlloc`'s `#[global_allocator]` instance,
/// which routes all instances to the same process-global heap registry).
/// Allocating via one `A` instance and freeing via a DIFFERENT `A` instance is
/// sound only when both instances route to the SAME underlying allocator; a
/// genuinely per-instance-stateful `A` (each instance owning a private arena)
/// would see a cross-thread free reach the wrong arena and is NOT supported by
/// this harness. This is the same constraint the `GlobalAlloc` trait already
/// implies for any allocator installed as `#[global_allocator]`.
///
/// The harness:
/// 1. Spawns `config.threads` workers, each with its own mailbox channel for
///    cross-thread block handoff.
/// 2. Uses a [`Barrier`] so all workers start the timed region together
///    (eliminates thread-spawn skew from the measurement).
/// 3. Sums total ops across all workers and divides by wall-clock elapsed.
///
/// # Safety contract upheld by the harness
///
/// Every block allocated by `A::alloc` is freed exactly once by exactly one
/// thread via `A::dealloc`. The harness manages ownership bookkeeping (slot
/// `Option` discipline + mpsc transfer) so callers do not need to.
///
/// The harness itself calls `A::alloc` / `A::dealloc` (the `GlobalAlloc`
/// trait is `unsafe`); those calls follow the safety contracts of
/// [`GlobalAlloc`].
///
/// # Example
///
/// ```rust
/// use malloc_bench_rs::{run, Config, Workload};
/// use std::alloc::System;
///
/// let cfg = Config { threads: 1, steps_per_thread: 1_000, working_set: 64, mstress_blocks: 32 };
/// let ops = run(Workload::Larson, &cfg, || System);
/// assert!(ops > 0.0);
/// ```
pub fn run<A>(workload: Workload, config: &Config, make_alloc: fn() -> A) -> f64
where
    A: GlobalAlloc + Send + 'static,
{
    let threads = config.threads.max(1);
    let steps = config.steps_per_thread;
    let working_set = config.working_set.max(1);
    let mstress_blocks = config.mstress_blocks.max(1);

    // Per-thread cross-thread mailboxes.
    let mut senders: Vec<Sender<Block>> = Vec::with_capacity(threads);
    let mut receivers: Vec<Option<Receiver<Block>>> = Vec::with_capacity(threads);
    for _ in 0..threads {
        let (tx, rx) = channel::<Block>();
        senders.push(tx);
        receivers.push(Some(rx));
    }
    let senders = Arc::new(senders);

    // Barrier: align all workers so the timed region is the steady state, not
    // thread-spawn skew. +1 for the main thread that starts/stops the clock.
    let barrier = Arc::new(Barrier::new(threads + 1));

    let mut handles = Vec::with_capacity(threads);
    for (t, rx_slot) in receivers.iter_mut().enumerate() {
        let senders = Arc::clone(&senders);
        let barrier = Arc::clone(&barrier);
        let rx = rx_slot.take().unwrap();
        // Per-thread seed derived from the thread index — fixed, reproducible.
        let seed = 0x9E37_79B9_7F4A_7C15u64
            .wrapping_mul(t as u64 + 1)
            .wrapping_add(0xDEAD_BEEF);
        let alloc = make_alloc();

        let handle = thread::spawn(move || {
            // Each worker waits at the barrier so all start together.
            barrier.wait();

            // SAFETY: `alloc` is a valid GlobalAlloc; workers uphold the
            // free-exactly-once invariant (see module docs).
            let ops = unsafe {
                match workload {
                    Workload::Larson => {
                        larson_worker(&alloc, seed, steps, working_set, &senders, &rx, t)
                    }
                    Workload::Mstress => mstress_worker(
                        &alloc,
                        seed,
                        steps / mstress_blocks.max(1) + 1,
                        mstress_blocks,
                        &senders,
                        &rx,
                        t,
                    ),
                }
            };

            // Final drain: free any cross-thread blocks that arrived after
            // our loop ended so nothing is leaked.
            let mut extra = 0u64;
            // SAFETY: uniquely owned inbound blocks.
            unsafe { drain_mailbox(&alloc, &rx, &mut extra) };
            ops + extra
        });
        handles.push(handle);
    }

    // Start the clock once every worker is at the barrier (steady state).
    barrier.wait();
    let start = Instant::now();

    let mut total_ops: u64 = 0;
    for h in handles {
        total_ops += h.join().expect("worker panicked");
    }
    let elapsed = start.elapsed();

    // After all workers joined, every sender is dropped and every receiver was
    // drained in the worker's final step: no block is leaked or double-freed.
    drop(senders);

    total_ops as f64 / elapsed.as_secs_f64()
}

/// Run a thread-count sweep for one workload × allocator.
///
/// For each `T` in `threads_sweep`, runs [`run`] with `config.threads = T`
/// and returns a `Vec<(threads, ops_per_sec)>`.
///
/// This is the primary entry point for scalability tables: you call `sweep`
/// once per allocator, zip the results, and print a comparison table.
///
/// # Example
///
/// ```rust
/// use malloc_bench_rs::{sweep, Config, Workload};
/// use std::alloc::System;
///
/// let cfg = Config { threads: 1, steps_per_thread: 1_000, working_set: 64, mstress_blocks: 32 };
/// let results = sweep(Workload::Larson, &cfg, &[1, 2], || System);
/// assert_eq!(results.len(), 2);
/// assert!(results[0].1 > 0.0);
/// ```
pub fn sweep<A>(
    workload: Workload,
    config: &Config,
    threads_sweep: &[usize],
    make_alloc: fn() -> A,
) -> Vec<(usize, f64)>
where
    A: GlobalAlloc + Send + 'static,
{
    threads_sweep
        .iter()
        .map(|&t| {
            let mut cfg = config.clone();
            cfg.threads = t;
            let ops = run(workload, &cfg, make_alloc);
            (t, ops)
        })
        .collect()
}
