// R14-3 (task #288) shared fixed-work round for the
// `paired_ab_class_aware_dirty_off` / `paired_ab_class_aware_dirty_on`
// process-level A/B/B/A judge binaries.
//
// ## Why this file exists
//
// Three independent Round-13 reviews of `docs/perf/
// R13_9_CLASS_AWARE_DIRTY_PRODUCTION_GATE.md` found the same methodology gap
// in its headline "21.71x at N=8" figure: that number comes from
// `benches/r12_7_class_aware_dirty_wallclock.rs`'s `run_round`'s own manual
// `ns/owner_alloc` SUB-WINDOW timer (`start = Instant::now()` AFTER
// pre-allocating `BLOCKS_PER_CLASS * n` producer blocks, stopped BEFORE
// `HeapRegistry::recycle`), not the FULL round Criterion's own `iter()`
// closure measures (pre-alloc + timed window + recycle, all of it). Reading
// the raw logs that headline was measured from
// (`docs/perf/_raw_r13_9_wallclock_{baseline_off,treatment_on}.log`) at N=8:
// criterion's own full-round mean moved ~20.6ms -> ~18.4ms (~11% faster),
// while the sub-window `ns/owner_alloc` moved 18.8ms -> 1.35ms (the 21.71x-ish
// figure lives here). The ~17ms of drain work the window metric "removes"
// did not vanish — it moved into the unmeasured pre-alloc/recycle portion of
// the SAME round (deferred reclaim work happens there instead), so the
// end-to-end wall-clock improvement for a fixed amount of work is honestly
// closer to a low double-digit percentage than an order of magnitude, at
// least on this workload shape and host.
//
// This file is the FIXED-WORK, PROCESS-LEVEL counterpart that makes both
// axes independently reproducible outside criterion's own `iter()` machinery,
// following this project's established `scripts/paired-ab-runner.mjs`
// protocol (fresh process per sample, `RESULT key=value` stdout lines,
// `--config` two-arbitrary-arms mode) instead of inventing a third runner —
// see `docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB.md` for the full
// report and `scripts/_r14_3_class_aware_dirty_ab.json` for the config this
// workload is driven through.
//
// `include!` (not a shared crate module) is used for the same reason every
// other `paired_ab_*_workload.rs` file in this directory documents: Cargo
// examples are independent compilation units with no shared `examples/`-
// support crate in this project, so `include!`ing the identical source into
// both wrapper binaries is what guarantees the workload body is byte-for-byte
// identical in both arms — the ONLY difference between
// `paired_ab_class_aware_dirty_off.rs` and `paired_ab_class_aware_dirty_on.rs`
// is the Cargo feature set (`class-aware-dirty` off vs on) each is built
// with.
//
// ## Workload shape (deliberately identical to the criterion bench)
//
// Same shape as `benches/r12_7_class_aware_dirty_wallclock.rs::run_round`
// at the bench's own worst-case N (`N_PRODUCER_CLASSES = 8`, the point where
// the R13-9 gate's ratio is largest): `N_PRODUCER_CLASSES` concurrent
// producer threads each remote-free `BLOCKS_PER_CLASS` pre-allocated blocks
// of their own distinct size class into a shared owner heap, while the owner
// thread continuously allocates+frees the target class in batches of
// `OWNER_BATCH`, forcing `find_segment_with_free_impl` ->
// `drain_dirty_segments` on every magazine miss. `BLOCKS_PER_CLASS` and
// `MIN_OWNER_ITERS` are copied verbatim from the bench file so this is the
// SAME fixed amount of work, not a rescaled approximation.
//
// ## Fixed work, not "until done"
//
// Per this task's own brief: the round runs a FIXED number of owner
// allocations (`MIN_OWNER_ITERS`, same floor the bench enforces) and a FIXED
// number of remote frees (`BLOCKS_PER_CLASS` per producer, `N_PRODUCER_CLASSES`
// producers) — not "producer runs until it decides it's done" with the owner
// racing an unbounded target. The owner loop's exit condition
// (`i >= MIN_OWNER_ITERS && producers_done`) means the owner may do a little
// more than the floor while producers are still finishing their fixed
// `BLOCKS_PER_CLASS` batch, but the producer side's total remote-free count
// is exactly fixed regardless of arm, and the owner's floor is identical
// across arms too — so both arms do materially the same amount of NOMINAL
// work; the entire point of measuring is whether that fixed work takes more
// or less wall-clock time.
//
// ## Two axes emitted (mirrors the bench's own now-dual-axis output)
//
// - `elapsed_ns` — the FULL round, one outer `Instant` pair wrapping
//   pre-alloc + the timed window + recycle, i.e. everything
//   `run_fixed_work_round` does. This is the metric `paired-ab-runner.mjs`
//   pairs by default (`metric: "elapsed_ns"`) and the one comparable to
//   criterion's own reported "time:" — the full-round axis the R13-9 report
//   did not cite next to its window headline.
// - `window_ns` — the SAME sub-window `run_round` in the bench measures
//   (post-pre-alloc, pre-recycle), reported alongside so a reader can see
//   both axes from ONE process launch instead of cross-referencing two
//   separate documents.
// - `owner_allocs` — the fixed owner-alloc count actually completed (for
//   sanity: this should read >= MIN_OWNER_ITERS in every arm/run).

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

/// Blocks of each producer class pre-allocated (and then remotely freed) per
/// round — copied verbatim from `benches/r12_7_class_aware_dirty_wallclock.rs`
/// so this is the SAME fixed amount of remote-free work, not a rescaled
/// approximation.
const BLOCKS_PER_CLASS: usize = 800;

/// Worst-case producer-class count from the bench's own N in `[1, 2, 4, 8]`
/// sweep — the point where the R13-9 gate's window-metric ratio was largest,
/// so this fixed-work judge measures the same adversarial shape the headline
/// number was drawn from.
const N_PRODUCER_CLASSES: usize = 8;

/// Same producer-class index set as the bench/judge test.
const PRODUCER_CLASS_INDICES: &[usize] = &[40, 41, 42, 43, 44, 45, 46, 47];

/// The class the owner allocates continuously — the first producer class, so
/// its drains are useful while the other N-1 producer segments' drains are
/// wasted (identical rationale to the bench's own `TARGET_CLASS`).
const TARGET_CLASS: usize = 40;

/// Owner-thread self-free batch size — identical to the bench's `OWNER_BATCH`.
const OWNER_BATCH: usize = 512;

/// Fixed owner-alloc floor — identical to the bench's `MIN_OWNER_ITERS`.
const MIN_OWNER_ITERS: usize = 800;

fn materialise_directory(heap: *mut HeapCore) -> Vec<*mut u8> {
    let threshold = AllocCore::dbg_directory_materialize_threshold() as usize;
    let class_count = AllocCore::dbg_small_class_count();
    let carve_ceiling = PRODUCER_CLASS_INDICES[0];
    let target = (threshold + 8).min(carve_ceiling);
    assert!(
        target > threshold,
        "size-class table too small for materialisation carve (need > {threshold} classes below producer range, have {carve_ceiling})"
    );
    assert!(
        *PRODUCER_CLASS_INDICES.last().unwrap() < class_count,
        "producer class indices exceed SMALL_CLASS_COUNT ({class_count})"
    );
    let mut keep_alive: Vec<*mut u8> = Vec::with_capacity(target);
    for cls in 0..target {
        let block_size = AllocCore::dbg_block_size(cls);
        let layout =
            Layout::from_size_align(block_size, 8).expect("class block size is a valid layout");
        let p = unsafe { (*heap).alloc(layout) };
        assert!(
            !p.is_null(),
            "materialise alloc for class {cls} returned null"
        );
        keep_alive.push(p);
    }
    keep_alive
}

/// One fixed-work round: `N_PRODUCER_CLASSES` producers each remote-free
/// exactly `BLOCKS_PER_CLASS` blocks; the owner completes at least
/// `MIN_OWNER_ITERS` allocs. Returns `(full_round_ns, window_ns, owner_allocs)`.
///
/// Structurally identical to
/// `benches/r12_7_class_aware_dirty_wallclock.rs::run_round`, with an added
/// OUTER `Instant` pair spanning the entire function body (pre-alloc through
/// recycle) so the full-round axis is measured directly rather than inferred
/// from a separate criterion run.
pub fn run_fixed_work_round() -> (u128, u128, usize) {
    let full_round_start = Instant::now();

    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let heap_addr = heap as usize;

    let _keep_alive = materialise_directory(heap);

    let producer_classes = &PRODUCER_CLASS_INDICES[..N_PRODUCER_CLASSES];
    let mut blocks_per_producer: Vec<Vec<usize>> = Vec::with_capacity(N_PRODUCER_CLASSES);
    for &cls in producer_classes {
        let block_size = AllocCore::dbg_block_size(cls);
        let layout = Layout::from_size_align(block_size, 8)
            .expect("producer class block size is a valid layout");
        let mut v: Vec<usize> = Vec::with_capacity(BLOCKS_PER_CLASS);
        for _ in 0..BLOCKS_PER_CLASS {
            let p = unsafe { (*heap).alloc(layout) };
            assert!(!p.is_null(), "producer-class pre-alloc returned null");
            v.push(p as usize);
        }
        blocks_per_producer.push(v);
    }

    let producers_done = Arc::new(AtomicBool::new(false));
    let producers_done_owner = Arc::clone(&producers_done);

    // The window timer — same sub-window the criterion bench's `run_round`
    // measures (post-pre-alloc, pre-recycle) — kept as a companion metric,
    // NOT the headline axis of this judge.
    let window_start = Instant::now();

    let mut handles = Vec::with_capacity(N_PRODUCER_CLASSES);
    for (i, &cls) in producer_classes.iter().enumerate() {
        let addrs = std::mem::take(&mut blocks_per_producer[i]);
        let block_size = AllocCore::dbg_block_size(cls);
        let layout = Layout::from_size_align(block_size, 8)
            .expect("producer class block size is a valid layout");
        handles.push(thread::spawn(move || {
            let _ = bootstrap::ensure();
            let remote_heap = HeapRegistry::claim();
            assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
            for (i, addr) in addrs.iter().enumerate() {
                let p = *addr as *mut u8;
                unsafe { (*remote_heap).dealloc(p, layout) };
                if i & 0x3F == 0 {
                    std::thread::yield_now();
                }
            }
            unsafe { HeapRegistry::recycle(remote_heap) };
        }));
    }

    let target_block_size = AllocCore::dbg_block_size(TARGET_CLASS);
    let target_layout = Layout::from_size_align(target_block_size, 8)
        .expect("TARGET_CLASS block size is a valid layout");
    let target_heap_addr = heap_addr;
    let owner_handle: thread::JoinHandle<usize> = thread::spawn(move || {
        let target_heap = target_heap_addr as *mut HeapCore;
        let mut batch: Vec<*mut u8> = Vec::with_capacity(OWNER_BATCH);
        let mut i = 0usize;
        loop {
            let p = unsafe { (*target_heap).alloc(target_layout) };
            if !p.is_null() {
                batch.push(p);
            }
            i += 1;
            if batch.len() >= OWNER_BATCH {
                for p in batch.drain(..) {
                    unsafe { (*target_heap).dealloc(p, target_layout) };
                }
            }
            if i >= MIN_OWNER_ITERS && producers_done_owner.load(Ordering::Acquire) {
                break;
            }
        }
        for p in batch.drain(..) {
            unsafe { (*target_heap).dealloc(p, target_layout) };
        }
        i
    });

    for h in handles {
        h.join().expect("producer thread must not panic");
    }
    producers_done.store(true, Ordering::Release);
    let owner_allocs = owner_handle.join().expect("owner thread must not panic");

    let window_ns = window_start.elapsed().as_nanos();

    unsafe { HeapRegistry::recycle(heap) };

    let full_round_ns = full_round_start.elapsed().as_nanos();

    (full_round_ns, window_ns, owner_allocs)
}
