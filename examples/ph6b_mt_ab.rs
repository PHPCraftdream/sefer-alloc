//! Ph6b (#2097) A/B cost harness: multi-thread wall-clock under the real
//! installed `#[global_allocator]` — MEASUREMENT-ONLY.
//!
//! Installs `SeferAlloc` as the process-wide allocator (`Profile::default()`,
//! no env overrides):
//!
//! ```text
//! #[global_allocator]
//! static GLOBAL: SeferAlloc = SeferAlloc::new();
//! ```
//!
//! # What is honestly measured
//!
//! Every `alloc`/`dealloc` call below goes through `std::alloc` into the ONE
//! installed static `GLOBAL` instance shared by all threads (inside, the
//! per-thread TLS heaps of `SeferAlloc`). This is the *installed
//! `#[global_allocator]`* shape (ADR axis 3), with a T = 1, 2, 4, 8 sweep.
//! `malloc-bench-rs` is deliberately NOT used here: it constructs a fresh
//! allocator instance per thread, which is not the installed-`#[global_allocator]`
//! form this axis must measure.
//!
//! Two deterministic workloads, identical op budget per thread:
//!   - **larson**  — server churn: 768 live slots per thread; each step frees a
//!     random own slot (xorshift64) and allocates a new block with a
//!     log-uniform size from the small class set {16,32,48,64,80,112,144,192,
//!     240,256,304,384,512} plus 5% {1024,4096}.
//!   - **mstress** — rounds of "fill a vector of 512 mixed blocks → free a
//!     random half → refill → free all"; exactly `MSTRESS_ROUNDS` rounds so
//!     total ops per thread ≈ 400_000.
//!
//! Deterministic cross-thread free fraction 1/16: a block due to be freed is
//! handed via `std::sync::mpsc` to the right neighbour in a ring; the
//! RECEIVING thread frees it (exercising the remote-free path). Fully
//! deterministic: fixed xorshift seeds per thread index, no time-based
//! decisions. At T = 1 the ring degenerates and all frees are local.
//!
//! Threads synchronize on a `Barrier` BEFORE the measured phase; timing is
//! taken around the whole work phase (after the barrier), one cell per
//! (workload × T), no averaging. The cell's wall time is the maximum of the
//! per-thread windows (the critical path — all threads start inside the same
//! barrier-release window).
//!
//! Output protocol: only `RESULT key=value` lines
//! (`RESULT mt_ns workload=<w> T=<t> ns=<u64>`,
//! `RESULT mt_ops workload=<w> T=<t> ops=<u64>`, plus the sanity counters
//! `RESULT segments_reserved_total=<u64>` and `RESULT config_conflicts=<u64>`
//! from `GLOBAL.stats()` — proving the real allocator was actually engaged
//! and no profile config conflicts occurred).
//!
//! **Build:** `cargo build --release --example ph6b_mt_ab --features "production internals"`

#![allow(clippy::cast_precision_loss)]

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

use std::sync::mpsc::Sender;
use std::sync::{mpsc, Arc, Barrier};

/// Live slots per thread (larson working set).
const WORKING_SET: usize = 768;
/// larson steps per thread (one step = one free + one alloc).
const STEPS_PER_THREAD: usize = 400_000;
/// mstress: 260 rounds × 1536 ops/round (512 fill + 256 half-free + 256 refill
/// + 512 free-all) = 399_360 ops ≈ 400_000 per thread.
const MSTRESS_ROUNDS: usize = 260;
/// Blocks per mstress round.
const MSTRESS_BLOCKS: usize = 512;
/// Thread sweep (ADR axis 3).
const THREAD_SWEEP: &[usize] = &[1, 2, 4, 8];
/// Every 16th free goes cross-thread.
const XTHREAD_EVERY: u64 = 16;

/// Small size classes (log-uniform draw set).
const SIZES: &[usize] = &[16, 32, 48, 64, 80, 112, 144, 192, 240, 256, 304, 384, 512];
/// Rare large sizes: 1-in-20 draws (5%).
const LARGE_SIZES: &[usize] = &[1024, 4096];

// --- deterministic xorshift64 (one stream per thread, fixed seed) -----------
struct Xorshift64(u64);

impl Xorshift64 {
    fn new(thread_index: usize) -> Self {
        Self(0x9E37_79B9_7F4A_7C15 ^ (thread_index as u64).wrapping_mul(0xFF51_AFD7_ED55_8CC1))
    }
    #[inline]
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    #[inline]
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    /// Log-uniform draw: mostly small classes, 5% large.
    #[inline]
    fn size(&mut self) -> usize {
        if self.below(20) == 0 {
            LARGE_SIZES[self.below(LARGE_SIZES.len() as u64) as usize]
        } else {
            SIZES[self.below(SIZES.len() as u64) as usize]
        }
    }
}

// --- alloc/free primitives (route through the installed global allocator) ----

fn alloc_block(size: usize) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size, 16).unwrap();
    // SAFETY: non-zero size, power-of-two alignment <= 16.
    let p = unsafe { std::alloc::alloc(layout) };
    assert!(!p.is_null(), "alloc({size}) failed — harness is invalid");
    // SAFETY: `p` covers at least `size >= 1` bytes.
    unsafe { std::ptr::write_volatile(p, 0xA5u8) };
    p
}

fn free_block(p: *mut u8, size: usize) {
    let layout = std::alloc::Layout::from_size_align(size, 16).unwrap();
    // SAFETY: `p` was allocated with exactly this layout and is freed once.
    unsafe { std::alloc::dealloc(p, layout) };
}

/// Remote-free queue item: the block's address exposed as `usize` (strict
/// provenance: reconstituted with `ptr::from_exposed_addr_mut`, which is safe
/// code — keeps `*mut u8` out of the channel so no `unsafe impl Send` needed).
type RemoteBlk = (usize, usize);

/// Per-thread context: deterministic RNG, op counter, inbound remote-free
/// queue and outbound senders (ring: thread i sends to thread (i+1) % T).
struct Worker {
    rng: Xorshift64,
    ops: u64,
    free_tick: u64,
    rx: mpsc::Receiver<RemoteBlk>,
    senders: Vec<Sender<RemoteBlk>>,
}

impl Worker {
    /// Free `p`, either locally or (every `XTHREAD_EVERY`-th free) handed to
    /// the right neighbour, which frees it. Ops are counted where the free
    /// physically happens (local free here, remote free by the recipient).
    fn release(&mut self, p: *mut u8, size: usize, me: usize) {
        self.free_tick += 1;
        if self.senders.len() > 1 && self.free_tick % XTHREAD_EVERY == 0 {
            let to = (me + 1) % self.senders.len();
            // Unbounded channel: send cannot fail while the receiver lives;
            // all receivers live in joined threads of this cell's scope.
            let _ = self.senders[to].send((p as usize, size));
        } else {
            free_block(p, size);
            self.ops += 1;
        }
    }
    /// Drain any blocks queued for remote free by the left neighbour.
    fn drain_remote(&mut self) {
        while let Ok((addr, size)) = self.rx.try_recv() {
            free_block(std::ptr::with_exposed_provenance_mut::<u8>(addr), size);
            self.ops += 1;
        }
    }
}

/// larson-shaped churn: `WORKING_SET` live slots, `STEPS_PER_THREAD` steps.
fn run_larson(w: &mut Worker, me: usize) {
    let mut slots: Vec<(*mut u8, usize)> = Vec::with_capacity(WORKING_SET);
    for _ in 0..WORKING_SET {
        let size = w.rng.size();
        slots.push((alloc_block(size), size));
        w.ops += 1;
    }
    for step in 0..STEPS_PER_THREAD {
        let idx = w.rng.below(WORKING_SET as u64) as usize;
        let (p, size) = slots[idx];
        w.release(p, size, me);
        let size = w.rng.size();
        slots[idx] = (alloc_block(size), size);
        w.ops += 1;
        if step % 64 == 0 {
            w.drain_remote();
        }
    }
    w.drain_remote();
    // Free the remaining working set so the `segments_reserved_total` sanity
    // line is taken from a quiesced allocator.
    for (p, size) in slots.drain(..) {
        free_block(p, size);
        w.ops += 1;
    }
    w.drain_remote();
}

/// mstress-shaped rounds: fill 512 → free random half → refill → free all.
fn run_mstress(w: &mut Worker, me: usize) {
    for round in 0..MSTRESS_ROUNDS {
        let mut blocks: Vec<(*mut u8, usize)> = Vec::with_capacity(MSTRESS_BLOCKS);
        for _ in 0..MSTRESS_BLOCKS {
            let size = w.rng.size();
            blocks.push((alloc_block(size), size));
            w.ops += 1;
        }
        // Free a random half: every other slot starting at a deterministic
        // rng-chosen offset.
        let off = w.rng.below(2) as usize;
        let mut i = off;
        for _ in 0..(MSTRESS_BLOCKS / 2) {
            let (p, size) = blocks[i];
            w.release(p, size, me);
            blocks[i] = (std::ptr::null_mut(), 0);
            i = (i + 2) % MSTRESS_BLOCKS;
        }
        blocks.retain(|(p, _)| !p.is_null());
        // Refill to 512.
        while blocks.len() < MSTRESS_BLOCKS {
            let size = w.rng.size();
            blocks.push((alloc_block(size), size));
            w.ops += 1;
        }
        // Free all.
        for (p, size) in blocks.drain(..) {
            w.release(p, size, me);
        }
        if round % 8 == 0 {
            w.drain_remote();
        }
    }
    w.drain_remote();
}

/// One (workload × T) cell: spawn T workers, barrier, then time the whole
/// work phase; the cell result is max(per-thread ns) × sum(per-thread ops).
fn measure(workload: &str, threads: usize) -> (u64, u64) {
    let workload = workload.to_string();
    let barrier = Arc::new(Barrier::new(threads));
    let mut senders: Vec<mpsc::Sender<RemoteBlk>> = Vec::with_capacity(threads);
    let mut rxs: Vec<mpsc::Receiver<RemoteBlk>> = Vec::with_capacity(threads);
    for _ in 0..threads {
        let (tx, rx) = mpsc::channel();
        senders.push(tx);
        rxs.push(rx);
    }
    let mut handles = Vec::with_capacity(threads);
    for (me, rx) in rxs.into_iter().enumerate() {
        let barrier = Arc::clone(&barrier);
        let senders = senders.clone();
        let workload = workload.clone();
        handles.push(std::thread::spawn(move || {
            let mut w = Worker {
                rng: Xorshift64::new(me),
                ops: 0,
                free_tick: 0,
                rx,
                senders,
            };
            barrier.wait();
            let t0 = std::time::Instant::now();
            if workload == "larson" {
                run_larson(&mut w, me);
            } else {
                run_mstress(&mut w, me);
            }
            let ns = t0.elapsed().as_nanos() as u64;
            w.drain_remote();
            (ns, w.ops)
        }));
    }
    let mut max_ns = 0u64;
    let mut total_ops = 0u64;
    for h in handles {
        let (ns, ops) = h.join().unwrap();
        if ns > max_ns {
            max_ns = ns;
        }
        total_ops += ops;
    }
    (max_ns, total_ops)
}

fn main() {
    eprintln!("== ph6b_mt_ab: installed #[global_allocator] MT A/B harness (measurement-only) ==");
    for &t in THREAD_SWEEP {
        for workload in ["larson", "mstress"] {
            let (ns, ops) = measure(workload, t);
            println!("RESULT mt_ns workload={workload} T={t} ns={ns}");
            println!("RESULT mt_ops workload={workload} T={t} ops={ops}");
        }
    }
    let stats = GLOBAL.stats();
    println!(
        "RESULT segments_reserved_total={}",
        stats.segments_reserved_total
    );
    println!("RESULT config_conflicts={}", stats.config_conflicts);
}
