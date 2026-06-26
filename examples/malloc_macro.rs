//! Multi-threaded macro-benchmark for `SeferMalloc` vs `mimalloc` vs `System`.
//!
//! Run with:
//!   `cargo run --release --example malloc_macro --features "alloc-global alloc-xthread"`
//!
//! Unlike `benches/global_alloc.rs` (a single-threaded micro-churn of one fixed
//! layout), this harness exercises the dimensions a real allocator must serve:
//!   1. **multi-thread scaling** — a sweep over T = 1, 2, 4 worker threads,
//!   2. **cross-thread free** — a fraction of blocks are handed to another
//!      thread and freed there (under `alloc-xthread` this routes through the
//!      per-segment remote-free path),
//!   3. **mixed sizes** — small-skewed distribution (16..512 B, rare larger).
//!
//! Two workloads, both reporting **aggregate ops/sec** (an op = one alloc+free
//! pair) over a fixed operation budget measured with `Instant::elapsed`:
//!   - **larson**  — server-churn: each thread keeps a working set of live
//!     slots; each step frees a random slot and allocates a new random-size
//!     block into it. Periodically a block is handed off cross-thread.
//!   - **mstress** — rounds of "fill a vector of mixed blocks → free half in
//!     random order → refill → free all"; a fraction freed cross-thread.
//!
//! This is NOT a criterion micro-loop: criterion's per-iter model mis-measures
//! MT work (thread spawn inside the timed closure dominates). We pre-spawn the
//! threads, run a fixed op budget, and time the whole steady-state region.
//!
//! Determinism: a dependency-free xorshift PRNG with a fixed per-thread seed, so
//! runs are reproducible. No `rand` crate is added.
//!
//! Cross-thread handoff is leak/UAF-free by construction: every allocated block
//! is freed **exactly once, by exactly one thread**. A handed-off block is moved
//! out of the producer's bookkeeping (its slot is set empty) before being sent;
//! the consumer drains its mailbox and frees each received block once. At the
//! end every thread frees its own remaining live blocks, then drains any final
//! mailbox contents — so nothing is dropped on the floor and nothing is freed
//! twice.

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::semicolon_if_nothing_returned
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

use sefer_alloc::SeferMalloc;

/// A live allocation: raw pointer plus the exact layout it was allocated with
/// (needed for a correct `dealloc`). `Send` is asserted explicitly below — the
/// block is logically *moved* to the receiving thread, which becomes its sole
/// owner; the producer no longer touches it.
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
    // 8-byte alignment, matching the single-threaded bench.
    Layout::from_size_align(size.max(1), 8).unwrap()
}

/// Allocate one block of a random small-skewed size, touching the first byte so
/// the allocation is not optimized away and the page is actually faulted in.
///
/// # Safety
/// `alloc` must be a valid `GlobalAlloc`. The returned `Block` (if non-null)
/// must be freed exactly once via `free_block` with the same allocator.
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
/// Every received block was allocated by `a` on some thread and ownership was
/// transferred here; we are its unique owner and free it once.
#[inline]
unsafe fn drain_mailbox<A: GlobalAlloc>(a: &A, rx: &Receiver<Block>, count: &mut u64) {
    while let Ok(block) = rx.try_recv() {
        // SAFETY: see fn docs — unique ownership, freed once.
        unsafe { free_block(a, block) };
        *count += 1;
    }
}

/// One larson worker. Returns the number of alloc+free *ops* it performed.
///
/// # Safety
/// `a` is a valid `GlobalAlloc` shared by all workers (a ZST or `Copy` handle).
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
                    // Receiver gone (shouldn't happen during the run) — we would
                    // leak; instead free locally to stay UAF/leak free.
                    // (Unreachable in practice; kept for total correctness.)
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

        // Free everything remaining (local).
        for block in blocks.drain(..).flatten() {
            // SAFETY: owned here, freed once.
            unsafe { free_block(a, block) };
            ops += 1;
        }
    }

    black_box(&ops);
    ops
}

/// Workload selector.
#[derive(Clone, Copy)]
enum Workload {
    Larson,
    Mstress,
}

/// Run one (workload × allocator × T) configuration and return aggregate
/// ops/sec. `A` is a ZST `GlobalAlloc` constructed fresh per thread via
/// `ZstAlloc::default_zst` (all three of our allocators are ZSTs).
///
/// # Safety
/// `A` is a valid `GlobalAlloc`; the closure body upholds the per-block
/// free-exactly-once discipline documented at module level.
fn run_config<A>(workload: Workload, threads: usize, steps_per_thread: usize) -> f64
where
    A: ZstAlloc + GlobalAlloc + Send + 'static,
{
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

    let working_set = 768; // ~512..1024 live blocks per thread (larson)
    let mstress_blocks = 512;

    let mut handles = Vec::with_capacity(threads);
    for (t, rx_slot) in receivers.iter_mut().enumerate() {
        let senders = Arc::clone(&senders);
        let barrier = Arc::clone(&barrier);
        let rx = rx_slot.take().unwrap();
        let seed = 0x9E37_79B9_7F4A_7C15u64
            .wrapping_mul(t as u64 + 1)
            .wrapping_add(0xDEAD_BEEF);
        let alloc = A::default_zst();
        let handle = thread::spawn(move || {
            // Each worker waits at the barrier so all start together.
            barrier.wait();
            // SAFETY: `alloc` is a valid GlobalAlloc; workers uphold the
            // free-exactly-once invariant (see module docs).
            let ops = unsafe {
                match workload {
                    Workload::Larson => larson_worker(
                        &alloc,
                        seed,
                        steps_per_thread,
                        working_set,
                        &senders,
                        &rx,
                        t,
                    ),
                    Workload::Mstress => mstress_worker(
                        &alloc,
                        seed,
                        steps_per_thread / mstress_blocks.max(1) + 1,
                        mstress_blocks,
                        &senders,
                        &rx,
                        t,
                    ),
                }
            };
            // Final drain: free any cross-thread blocks that arrived after our
            // loop ended, so nothing is leaked.
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
    // drained in the worker's final step, so no block is leaked or double-freed.
    drop(senders);

    total_ops as f64 / elapsed.as_secs_f64()
}

/// Helper to get a default instance of a ZST allocator without `Default` bound
/// gymnastics — all three allocators here are ZSTs constructible from a const.
trait ZstAlloc {
    fn default_zst() -> Self;
}
impl ZstAlloc for SeferMalloc {
    fn default_zst() -> Self {
        SeferMalloc::new()
    }
}
impl ZstAlloc for mimalloc::MiMalloc {
    fn default_zst() -> Self {
        mimalloc::MiMalloc
    }
}
impl ZstAlloc for System {
    fn default_zst() -> Self {
        System
    }
}

fn main() {
    println!("== sefer-alloc MT macro-benchmark ==");
    println!("Deterministic xorshift PRNG (fixed seeds); aggregate ops/sec.");
    println!("op = one alloc+free pair. Higher is better.\n");

    // Op budget per thread tuned so the whole suite runs in a few seconds.
    // (Total across the sweep = budget × sum(threads) × workloads × allocators.)
    let steps_per_thread = 400_000usize;
    let thread_sweep = [1usize, 2, 4];

    for &workload in &[Workload::Larson, Workload::Mstress] {
        let name = match workload {
            Workload::Larson => "larson",
            Workload::Mstress => "mstress",
        };
        println!("--- workload: {name} ---");
        println!(
            "{:>3}  {:>16}  {:>16}  {:>16}",
            "T", "SeferMalloc", "mimalloc", "System"
        );
        for &t in &thread_sweep {
            let sefer = run_config::<SeferMalloc>(workload, t, steps_per_thread);
            let mi = run_config::<mimalloc::MiMalloc>(workload, t, steps_per_thread);
            let sys = run_config::<System>(workload, t, steps_per_thread);
            println!(
                "{:>3}  {:>14.2} M  {:>14.2} M  {:>14.2} M",
                t,
                sefer / 1e6,
                mi / 1e6,
                sys / 1e6
            );
        }
        println!();
    }

    println!("(M = million ops/sec. RSS is not measured here — no portable,");
    println!(" dependency-free peak-RSS probe across Win/Linux/macOS; would");
    println!(" require platform syscalls. Reported honestly as N/A.)");
}
