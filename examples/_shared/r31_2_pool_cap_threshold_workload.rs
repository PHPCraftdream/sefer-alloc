// Shared workload for R31-2 (task #465)'s multi-threaded small-pool-cap
// THRESHOLD sweep (`r31_2_pool_cap_threshold_ab_cap4/8/16/32.rs`). `include!`d
// verbatim into all four binaries — see `examples/_shared/r30_7_server_shaped_workload.rs`'s
// module doc for why `include!` (not a shared crate module) is this
// project's established pattern for exactly this need (Cargo examples are
// independent compilation units; the ONLY difference between the four
// binaries must be which `SeferAlloc` small-pool cap is installed, so the
// workload body must be byte-identical across all four).
//
// ## Why this workload
//
// R30-7 (task #456, `docs/perf/R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md`)
// found `decommit_calls_total = 40` in BOTH the `default` (cap 4) arm and the
// `Profile::Throughput` (cap 8) arm of its 8-thread/4-size-mix/continuous-churn
// workload — a mechanism delta of ZERO between cap 4 and cap 8 at that
// concurrency. That result does NOT prove pooling has no value at this
// concurrency; it may only mean cap 8 was not large enough to change the
// mechanism for THIS SPECIFIC workload shape. This file is BYTE-IDENTICAL to
// `r30_7_server_shaped_workload.rs`'s workload body (same 8 threads, same
// 4-size mix, same per-round working set, same churn fraction, same round
// count) so the cap-4 arm here is the SAME comparison point R30-7 already
// measured — the sweep adds cap 16 and cap 32 as two more points on the same
// axis, searching for the actual threshold where (if anywhere) the mechanism
// delta moves off zero.

use std::alloc::{alloc, dealloc, Layout};
use std::thread;
use std::time::Instant;

use sefer_alloc::SeferAlloc;

/// Realistic small-object size mix: a request header, a small struct, a
/// medium buffer, and a page-sized buffer. All strictly "Small" under plain
/// `production` (well under the 16 KiB `SMALL_MAX` boundary).
const SIZE_CLASSES: [usize; 4] = [64, 256, 1024, 4096];

/// Concurrent "request handler" threads. Identical to R30-7's shape.
const THREADS: usize = 8;

/// Objects allocated per round, per thread (spread across the 4 size
/// classes in round-robin order). Identical to R30-7's calibration: a
/// round's PEAK concurrently-live bytes per thread
/// (`18504 / 4 * 5440` = 24.00 MiB exactly, since 18504 divides evenly by
/// 4) genuinely EXCEEDS the small pool's default 16 MiB (cap 4) byte
/// ceiling — the same calibration this sweep reuses so the cap-4 arm here
/// is the same comparison point R30-7 measured.
const OBJS_PER_ROUND: usize = 18_504;

/// Rounds per thread — the continuous-cycle axis. Identical to R30-7.
const ROUNDS: usize = 6;

/// Fraction (out of `OBJS_PER_ROUND`) of the previous round's objects that
/// are churned (freed + replaced) mid-round. Identical to R30-7.
const CHURN_FRACTION_NUM: usize = 1;
const CHURN_FRACTION_DEN: usize = 2;

struct XorShift64(u64);
impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self(seed)
    }
    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    #[inline]
    fn next_usize(&mut self) -> usize {
        self.next_u64() as usize
    }
}

/// One request handler's full run: `ROUNDS` rounds of mixed-size
/// allocate/touch/churn/free. Returns nothing — all memory is freed by the
/// time this returns (peak-bounded per thread: at most `OBJS_PER_ROUND`
/// objects live at any instant).
fn run_handler(seed: u64) {
    let layouts: Vec<Layout> = SIZE_CLASSES
        .iter()
        .map(|&sz| Layout::from_size_align(sz, 8).unwrap())
        .collect();
    let mut rng = XorShift64::new(seed);

    for _round in 0..ROUNDS {
        // (a) Allocate a round's worth of objects, sizes round-robin across
        // the mix, each genuinely touched.
        let mut live: Vec<(*mut u8, Layout)> = Vec::with_capacity(OBJS_PER_ROUND);
        for i in 0..OBJS_PER_ROUND {
            let layout = layouts[i % layouts.len()];
            // SAFETY: layout has non-zero size and valid (8-byte) alignment.
            let p = unsafe { alloc(layout) };
            assert!(!p.is_null(), "alloc failed (OOM?) — size={}", layout.size());
            // SAFETY: `p` is a fresh, live allocation of `layout.size()`
            // bytes; writing the first and last byte touches the page(s).
            unsafe {
                p.write_volatile(0xAB);
                p.add(layout.size() - 1).write_volatile(0xCD);
            }
            live.push((p, layout));
        }

        // (b) Churn a fraction of the round's working set: free + replace
        // (mimicking some objects being short-lived relative to the round).
        let churn_count = OBJS_PER_ROUND * CHURN_FRACTION_NUM / CHURN_FRACTION_DEN;
        for _ in 0..churn_count {
            let idx = rng.next_usize() % live.len();
            let (old_p, old_layout) = live[idx];
            // SAFETY: `old_p` is currently live, allocated with `old_layout`.
            unsafe { dealloc(old_p, old_layout) };
            let new_layout = layouts[rng.next_usize() % layouts.len()];
            // SAFETY: layout has non-zero size and valid alignment.
            let new_p = unsafe { alloc(new_layout) };
            assert!(!new_p.is_null(), "alloc failed (OOM?) during churn");
            // SAFETY: `new_p` is a fresh, live allocation.
            unsafe {
                new_p.write_volatile(0xEF);
                new_p.add(new_layout.size() - 1).write_volatile(0x12);
            }
            live[idx] = (new_p, new_layout);
        }

        // (c) Free everything remaining from this round before starting the
        // next — bounds peak memory per thread while still repeating the
        // full alloc/touch/churn/free cycle continuously for `ROUNDS` rounds.
        for (p, layout) in live {
            // SAFETY: `p` is still live, allocated with `layout`, freed once.
            unsafe { dealloc(p, layout) };
        }
    }
}

/// Spawn `THREADS` concurrent handler threads, run them all to completion,
/// and report end-to-end wall-clock time for the WHOLE concurrent run
/// (joined before the timer stops), plus the REQUESTED pool config (R26-4
/// contract point 1 — the compile-time const this binary was built with) and
/// the mechanism/RSS counters needed to judge the arm.
fn run_arm(
    arm_name: &str,
    global: &'static SeferAlloc,
    requested_pool_segments: u64,
    requested_pool_byte_cap: u64,
) {
    // Untimed warm-up round on the main thread (absorbs primordial-segment
    // bootstrap for the main thread specifically — R27-4's warm-up-placement
    // fix, applied here too), matching the sibling probes' established
    // pattern of an untimed warm-up before `t0`.
    run_handler(0xCAFE);

    let t0 = Instant::now();
    let handles: Vec<_> = (0..THREADS)
        .map(|i| {
            let seed = 0xCAFE_u64.wrapping_add((i as u64).wrapping_mul(0x9E37_79B9));
            thread::spawn(move || run_handler(seed))
        })
        .collect();
    for h in handles {
        h.join().expect("handler thread panicked");
    }
    let elapsed_ns = t0.elapsed().as_nanos();

    let stats = global.stats();
    let snap = proc_probe::snapshot();

    proc_probe::emit("arm", arm_name);
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_u64("threads", THREADS as u64);
    proc_probe::emit_u64("rounds_per_thread", ROUNDS as u64);
    proc_probe::emit_u64("pool_segments_requested", requested_pool_segments);
    proc_probe::emit_u64("pool_byte_cap_requested", requested_pool_byte_cap);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
    proc_probe::emit_u64("segments_released_total", stats.segments_released_total);
    proc_probe::emit_u64("decommit_calls_total", stats.decommit_calls);
    proc_probe::emit_u64("large_cache_hits", stats.large_cache_hits);
    proc_probe::emit_u64("rss_after_kib", snap.rss / 1024);
    proc_probe::emit_u64("commit_after_kib", snap.commit / 1024);
}
