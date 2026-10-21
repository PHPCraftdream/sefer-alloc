// ut_u31mt_probe.rs — B4-29/U31 chunk-3 MT probe v2 (counter-normalization fix):
// malloc-bench-rs larson/mstress at T in {1,2,4}, one allocator, B4_* + U31
// scan observer deltas AND the ACTUAL operation count (ops_actual), counted
// with EXACTLY the definition of crates/malloc-bench-rs/src/lib.rs:
// larson: ops += 1 per step loop iteration PLUS one per mailbox free
// (drain_mailbox `*count += 1` per freed block — including the per-worker
// final drain and the post-join sweep over leftover receivers);
// mstress: rounds = steps/blocks + 1, ops += 1 per freed block of the random
// half, PLUS one per local free of the tail, PLUS one per mailbox free.
// `run()` does not return total_ops, so the worker loops are vendored here
// verbatim (same PRNG, same seeds, same free-exactly-once discipline); only
// the wall-clock timing is dropped. The workload's allocator-call stream is
// identical to `malloc_bench_rs::run`.
// Usage: ut_u31mt_probe <sefer|mimalloc> <larson|mstress> <threads> <steps> [--counters]
#![allow(unsafe_code)]

use malloc_bench_rs::Workload;
use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Barrier};
use std::thread;

// ── Vendored verbatim from crates/malloc-bench-rs/src/lib.rs ────────────────

struct Block {
    ptr: *mut u8,
    layout: Layout,
}
// SAFETY: ownership transfer only (see lib.rs).
unsafe impl Send for Block {}

struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
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
    #[inline]
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

#[inline]
fn pick_size(rng: &mut XorShift64) -> usize {
    let r = rng.next_u64();
    if r.is_multiple_of(32) {
        512 + (r >> 8) as usize % (8 * 1024 - 512)
    } else {
        16 + (r >> 8) as usize % (512 - 16)
    }
}

#[inline]
fn layout_for(size: usize) -> Layout {
    Layout::from_size_align(size.max(1), 8).unwrap()
}

#[inline]
unsafe fn alloc_block<A: GlobalAlloc>(a: &A, rng: &mut XorShift64) -> Block {
    let layout = layout_for(pick_size(rng));
    // SAFETY: layout has non-zero size and valid alignment.
    let ptr = unsafe { a.alloc(layout) };
    if !ptr.is_null() {
        // SAFETY: ptr is valid for layout.size() >= 1 bytes.
        unsafe { ptr.write(0xA5) };
    }
    Block { ptr, layout }
}

#[inline]
unsafe fn free_block<A: GlobalAlloc>(a: &A, block: Block) {
    if !block.ptr.is_null() {
        // SAFETY: block.ptr came from a.alloc and is freed once.
        unsafe { a.dealloc(block.ptr, block.layout) };
    }
}

#[inline]
unsafe fn drain_mailbox<A: GlobalAlloc>(a: &A, rx: &Receiver<Block>, count: &mut u64) {
    while let Ok(block) = rx.try_recv() {
        // SAFETY: unique ownership, freed once.
        unsafe { free_block(a, block) };
        *count += 1;
    }
}

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
    let mut slots: Vec<Option<Block>> = Vec::with_capacity(working_set);
    for _ in 0..working_set {
        // SAFETY: valid allocator; tracked in slots.
        slots.push(Some(unsafe { alloc_block(a, &mut rng) }));
    }
    const HANDOFF_EVERY: usize = 16;
    let n_threads = senders.len();
    for step in 0..steps {
        // SAFETY: received blocks uniquely owned here.
        unsafe { drain_mailbox(a, rx, &mut ops) };
        let idx = rng.below(working_set);
        if n_threads > 1 && step % HANDOFF_EVERY == 0 {
            if let Some(block) = slots[idx].take() {
                let mut target = rng.below(n_threads);
                if target == self_idx {
                    target = (target + 1) % n_threads;
                }
                if let Err(e) = senders[target].send(block) {
                    // SAFETY: never observed by another thread; unique owner.
                    unsafe { free_block(a, e.0) };
                }
            }
        } else if let Some(block) = slots[idx].take() {
            // SAFETY: owned here, freed once.
            unsafe { free_block(a, block) };
        }
        // SAFETY: valid allocator; tracked in slots.
        slots[idx] = Some(unsafe { alloc_block(a, &mut rng) });
        ops += 1;
    }
    for block in slots.drain(..).flatten() {
        // SAFETY: owned here, freed once.
        unsafe { free_block(a, block) };
    }
    black_box(&ops);
    ops
}

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
        let mut blocks: Vec<Option<Block>> = Vec::with_capacity(block_count);
        for _ in 0..block_count {
            // SAFETY: valid allocator; tracked in blocks.
            blocks.push(Some(unsafe { alloc_block(a, &mut rng) }));
        }
        let half = block_count / 2;
        for _ in 0..half {
            let idx = rng.below(block_count);
            if let Some(block) = blocks[idx].take() {
                if n_threads > 1 && rng.below(8) == 0 {
                    let mut target = rng.below(n_threads);
                    if target == self_idx {
                        target = (target + 1) % n_threads;
                    }
                    if let Err(e) = senders[target].send(block) {
                        // SAFETY: never observed by another thread.
                        unsafe { free_block(a, e.0) };
                    }
                } else {
                    // SAFETY: owned here, freed once.
                    unsafe { free_block(a, block) };
                }
                ops += 1;
            }
        }
        for slot in blocks.iter_mut() {
            if slot.is_none() {
                // SAFETY: valid allocator; tracked in blocks.
                *slot = Some(unsafe { alloc_block(a, &mut rng) });
            }
        }
        for block in blocks.drain(..).flatten() {
            // SAFETY: owned here, freed once.
            unsafe { free_block(a, block) };
            ops += 1;
        }
    }
    black_box(&ops);
    ops
}

/// Mirror of `malloc_bench_rs::run_with` minus the wall-clock timing; returns
/// total_ops (worker ops + per-worker final-drain frees + post-join sweep).
fn run_counting<A: GlobalAlloc + Send + 'static>(
    workload: Workload,
    threads: usize,
    steps: usize,
    working_set: usize,
    mstress_blocks: usize,
    make_alloc: fn() -> A,
) -> u64 {
    let mut senders: Vec<Sender<Block>> = Vec::with_capacity(threads);
    let mut receivers: Vec<Option<Receiver<Block>>> = Vec::with_capacity(threads);
    for _ in 0..threads {
        let (tx, rx) = channel::<Block>();
        senders.push(tx);
        receivers.push(Some(rx));
    }
    let senders = Arc::new(senders);
    let barrier = Arc::new(Barrier::new(threads + 1));
    let mut handles = Vec::with_capacity(threads);
    for (t, rx_slot) in receivers.iter_mut().enumerate() {
        let senders = Arc::clone(&senders);
        let barrier = Arc::clone(&barrier);
        let rx = rx_slot.take().unwrap();
        let seed = 0x9E37_79B9_7F4A_7C15u64
            .wrapping_mul(t as u64 + 1)
            .wrapping_add(0xDEAD_BEEF);
        let alloc = make_alloc();
        let handle = thread::spawn(move || {
            barrier.wait();
            // SAFETY: valid GlobalAlloc; free-exactly-once upheld (see lib.rs).
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
            let mut extra = 0u64;
            // SAFETY: uniquely owned inbound blocks.
            unsafe { drain_mailbox(&alloc, &rx, &mut extra) };
            (ops + extra, rx)
        });
        handles.push(handle);
    }
    barrier.wait();
    let mut total_ops: u64 = 0;
    let mut leftover_receivers = Vec::with_capacity(threads);
    for h in handles {
        let (ops, rx) = h.join().expect("worker panicked");
        total_ops += ops;
        leftover_receivers.push(rx);
    }
    drop(senders);
    let final_alloc = make_alloc();
    let mut final_freed = 0u64;
    for rx in &leftover_receivers {
        // SAFETY: all workers joined, senders dropped; uniquely owned here.
        unsafe { drain_mailbox(&final_alloc, rx, &mut final_freed) };
    }
    total_ops + final_freed
}

// ── Probe driver ─────────────────────────────────────────────────────────────

#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn counters() -> ([u64; 7], [u64; 4]) {
    (
        sefer_alloc::SeferAlloc::dbg_b4_collapse_counters(),
        sefer_alloc::SeferAlloc::dbg_b4_u31_scan_counters(),
    )
}

#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
fn counters() -> ([u64; 7], [u64; 4]) {
    ([0; 7], [0; 4])
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let alloc = a.get(1).map(String::as_str).unwrap_or("sefer");
    let wl = match a.get(2).map(String::as_str).unwrap_or("larson") {
        "mstress" => Workload::Mstress,
        _ => Workload::Larson,
    };
    let threads: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(2);
    let steps: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let want_counters = a.iter().any(|x| x == "--counters");
    assert!(alloc == "sefer" || alloc == "mimalloc");
    let working_set = 768_usize;
    let mstress_blocks = 512_usize;
    let before = if want_counters && alloc == "sefer" {
        Some(counters())
    } else {
        None
    };
    let ops_actual = match alloc {
        "mimalloc" => run_counting(wl, threads.max(1), steps, working_set, mstress_blocks, || {
            mimalloc::MiMalloc
        }),
        _ => run_counting(
            wl,
            threads.max(1),
            steps,
            working_set,
            mstress_blocks,
            sefer_alloc::SeferAlloc::new,
        ),
    };
    println!("{alloc} ops_actual={ops_actual}");
    if let Some((before, before_u31)) = before {
        let (after, after_u31) = counters();
        let delta = |i: usize| after[i] - before[i];
        let wl_name = a.get(2).map(String::as_str).unwrap_or("larson");
        println!(
            "{{\"workload\":\"{wl_name}\",\"threads\":{threads},\"steps\":{steps},\"find_segment_calls\":{},\"scan_calls\":{},\"scan_words\":{},\"foreign_publish_ok\":{},\"directory_lookups\":{},\"shard_lock_spins\":{},\"shard_lock_yields\":{}}}",
            delta(0),
            delta(1),
            delta(2),
            delta(3),
            delta(4),
            delta(5),
            delta(6),
        );
        let scan_words = delta(2);
        let u31_seg = after_u31[0] - before_u31[0];
        let u31_base = after_u31[1] - before_u31[1];
        let u31_nopending = after_u31[2] - before_u31[2];
        let u31_disc = after_u31[3] - before_u31[3];
        println!(
            "{{\"workload\":\"{wl_name}\",\"threads\":{threads},\"steps\":{steps},\"ops_actual\":{ops_actual},\"scan_words\":{scan_words},\"scan_words_per_op_scaled_1e3\":{},\"u31_scan_segments\":{u31_seg},\"u31_holders_base\":{u31_base},\"u31_scan_no_pending\":{u31_nopending},\"u31_discovery_scans\":{u31_disc},\"holders_base_num\":{u31_base},\"holders_base_den\":{u31_seg}}}",
            if ops_actual > 0 { scan_words * 1000 / ops_actual as u64 } else { 0 },
        );
    }
}
