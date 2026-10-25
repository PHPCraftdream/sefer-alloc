// dp_depot_probe.rs — B4 D0 depot-provenance chunk 2 (task b4-provenance):
// malloc-bench-rs larson/mstress at T in {1,2,4}, one allocator (sefer), with
// the D0 DEPOT_* counters printed. The workload part below the marker is
// vendored VERBATIM from .rush/wsl/ut_u31mt_probe.rs (itself a verbatim
// vendoring of crates/malloc-bench-rs/src/lib.rs worker loops) so the
// allocator-call stream is identical to the judge/callgrind runs; only the
// driver tail differs (DEPOT_* counters instead of B4_*/U31 ones) and the
// wall-clock timing is dropped (counters are Relaxed atomics, not
// wall-clock).
// Usage: dp_depot_probe <larson|mstress> <threads> <steps>
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

// ── Probe driver (DEPOT_* counters; process-wide statics, Relaxed) ──────────

#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn depot_counters() -> [u64; 17] {
    use sefer_alloc::registry::HeapCore;
    let (ov_e, ov_b) = HeapCore::dbg_depot_bin_ingress_own_overflow();
    let (sw_e, sw_b) = HeapCore::dbg_depot_flush_swap_top();
    let (ba_e, ba_b) = HeapCore::dbg_depot_bin_ingress_own_batch();
    let (rm_e, rm_b) = HeapCore::dbg_depot_bin_ingress_remote();
    let (td_e, td_b) = HeapCore::dbg_depot_flush_teardown();
    let (rf_e, rf_tot, rf_dr, rf_cv, rf_ce) = HeapCore::dbg_depot_refill_stats();
    let pushes = HeapCore::dbg_depot_bin_ingress_substrate_pushes();
    [
        ov_e, ov_b, sw_e, sw_b, ba_e, ba_b, rm_e, rm_b, td_e, td_b, rf_e, rf_tot, rf_dr, rf_cv,
        rf_ce, pushes, 0,
    ]
}

#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
fn depot_counters() -> [u64; 17] {
    [0; 17]
}

#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn shadow_aggregates() -> (
    [u64; 4],
    [u64; 4],
    [u64; 4],
    [u64; 4],
    [u64; 4],
    [u64; 4],
    u64,
) {
    use sefer_alloc::registry::HeapCore;
    HeapCore::dbg_depot_shadow_aggregates()
}

#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
fn shadow_aggregates() -> (
    [u64; 4],
    [u64; 4],
    [u64; 4],
    [u64; 4],
    [u64; 4],
    [u64; 4],
    u64,
) {
    ([0; 4], [0; 4], [0; 4], [0; 4], [0; 4], [0; 4], 0)
}

#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn unmapped() -> u64 {
    use sefer_alloc::registry::HeapCore;
    HeapCore::dbg_depot_shadow_unmapped()
}

#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
fn unmapped() -> u64 {
    0
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let wl = match a.get(1).map(String::as_str).unwrap_or("larson") {
        "mstress" => Workload::Mstress,
        _ => Workload::Larson,
    };
    let threads: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let steps: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let working_set = 768_usize;
    let mstress_blocks = 512_usize;
    let before = depot_counters();
    let sh_before = shadow_aggregates();
    let ops_actual = run_counting(
        wl,
        threads.max(1),
        steps,
        working_set,
        mstress_blocks,
        sefer_alloc::SeferAlloc::new,
    );
    let c = depot_counters();
    let sh_after = shadow_aggregates();
    let (sh_abs, sh_real, sh_fully, sh_rfd, sh_rdr, sh_rfl, sh_inv) = sh_after;
    let ds = |a: [u64; 4], b: [u64; 4]| [b[0] - a[0], b[1] - a[1], b[2] - a[2], b[3] - a[3]];
    let abs_k = ds(sh_before.0, sh_abs);
    let real_k = ds(sh_before.1, sh_real);
    let fully_k = ds(sh_before.2, sh_fully);
    let rfd_k = ds(sh_before.3, sh_rfd);
    let rdr_k = ds(sh_before.4, sh_rdr);
    let rfl_k = ds(sh_before.5, sh_rfl);
    let inv = sh_inv - sh_before.6;
    let d = |x: u64, y: u64| y - x;
    // B4 (R4): provenance-completeness oracle — OTHER (unattributed Small
    // bin ingress) must be ZERO on every workload this example runs.
    let attributed = d(before[1], c[1])
        + d(before[3], c[3])
        + d(before[5], c[5])
        + d(before[7], c[7])
        + d(before[9], c[9]);
    let other = d(before[15], c[15]).saturating_sub(attributed);
    assert_eq!(
        other,
        0,
        "depot_bin_ingress_other_blocks != 0: an unattributed Small \
         bin-ingress path exists (pushes {} < attributed {} + other {})",
        d(before[15], c[15]),
        attributed,
        other
    );
    let wl_name = a.get(1).map(String::as_str).unwrap_or("larson");
    println!("{wl_name} ops_actual={ops_actual}");
    println!(
        "{{\"workload\":\"{wl_name}\",\"threads\":{threads},\"steps\":{steps},\"ops_actual\":{ops_actual},\"working_set\":{working_set},\"mstress_blocks\":{mstress_blocks},\"depot_bin_ingress_own_overflow_events\":{},\"depot_bin_ingress_own_overflow_blocks\":{},\"depot_flush_swap_top_events\":{},\"depot_flush_swap_top_blocks\":{},\"depot_bin_ingress_own_batch_events\":{},\"depot_bin_ingress_own_batch_blocks\":{},\"depot_bin_ingress_remote_events\":{},\"depot_bin_ingress_remote_blocks\":{},\"depot_flush_teardown_events\":{},\"depot_flush_teardown_blocks\":{},\"depot_refill_events\":{},\"depot_refill_blocks_total\":{},\"depot_refill_blocks_drained\":{},\"depot_refill_blocks_carved\":{},\"depot_refill_carve_events\":{},\"depot_bin_ingress_pushes\":{},\"depot_bin_ingress_other_blocks\":{}}}",
        d(before[0], c[0]),
        d(before[1], c[1]),
        d(before[2], c[2]),
        d(before[3], c[3]),
        d(before[4], c[4]),
        d(before[5], c[5]),
        d(before[6], c[6]),
        d(before[7], c[7]),
        d(before[8], c[8]),
        d(before[9], c[9]),
        d(before[10], c[10]),
        d(before[11], c[11]),
        d(before[12], c[12]),
        d(before[13], c[13]),
        d(before[14], c[14]),
        d(before[15], c[15]),
        d(before[16], c[16]),
    );
    println!(
        "{{\"absorbed_k\":[{}, {}, {}, {}],\"real_k\":[{}, {}, {}, {}],\"fully_absorbed_ev_k\":[{}, {}, {}, {}],\"refill_from_depot_k\":[{}, {}, {}, {}],\"refill_drain_k\":[{}, {}, {}, {}],\"refill_fully_k\":[{}, {}, {}, {}],\"invalidations\":{},\"unmapped\":{}}}",
        abs_k[0], abs_k[1], abs_k[2], abs_k[3],
        real_k[0], real_k[1], real_k[2], real_k[3],
        fully_k[0], fully_k[1], fully_k[2], fully_k[3],
        rfd_k[0], rfd_k[1], rfd_k[2], rfd_k[3],
        rdr_k[0], rdr_k[1], rdr_k[2], rdr_k[3],
        rfl_k[0], rfl_k[1], rfl_k[2], rfl_k[3],
        inv,
        unmapped(),
    );
}
