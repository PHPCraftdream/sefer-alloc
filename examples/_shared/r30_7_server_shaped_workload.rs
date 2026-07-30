// Shared workload for the R30-7 (task #456) Deliverable 4 application-shaped
// server A/B pair (`r30_7_throughput_profile_server_ab_default.rs` /
// `r30_7_throughput_profile_server_ab_throughput.rs`). `include!`d verbatim
// into both binaries — see `examples/_shared/paired_ab_workload.rs`'s module
// doc for why `include!` (not a shared crate module) is this project's
// established pattern for exactly this need (Cargo examples are independent
// compilation units; the ONLY difference between the two binaries must be
// which `SeferAlloc` config is installed, so the workload body must be
// byte-identical).
//
// ## Why this workload, not the original single-thread teardown loop
//
// The `(8, 32 MiB)` throughput recipe's ~22% win (README, R27-4) was
// measured on a SINGLE-THREADED, single-shot prefill/churn/teardown loop at
// one fixed size (1024 B, batch 120). This workload instead simulates
// `THREADS` concurrent "request handlers": each thread repeatedly (a)
// allocates a burst of objects at a MIX of realistic sizes (headers, small
// structs, medium buffers, page buffers), (b) touches every object (first +
// last byte) so the allocation is real, (c) churns a fraction of the
// working set before freeing the round, and (d) loops this for `ROUNDS`
// rounds — a continuous cycle for the whole timed region, not one
// burst-then-teardown. This is a materially different shape: multiple
// threads' allocation traffic genuinely overlaps (contending on the shared
// registry/large-cache infra where applicable), and no single object size
// dominates the segment-carve pattern the way the original 1024B-only probe
// does.

use std::alloc::{alloc, dealloc, Layout};
use std::thread;
use std::time::Instant;

use sefer_alloc::SeferAlloc;

/// Realistic small-object size mix: a request header, a small struct, a
/// medium buffer, and a page-sized buffer. All strictly "Small" under plain
/// `production` (well under the 16 KiB `SMALL_MAX` boundary).
const SIZE_CLASSES: [usize; 4] = [64, 256, 1024, 4096];

/// Concurrent "request handler" threads.
const THREADS: usize = 8;

/// Objects allocated per round, per thread (spread across the 4 size
/// classes in round-robin order). Calibrated so a round's PEAK concurrently-
/// live bytes per thread (`OBJS_PER_ROUND / 4 * sum(SIZE_CLASSES)` =
/// `18504 / 4 * 5440` ≈ 24.1 MiB) genuinely EXCEEDS the small-pool's default
/// 16 MiB (cap 4) byte ceiling — mirroring R27-4's own calibration principle
/// (its batch-120 @ 256-working-set shape was chosen specifically to
/// overflow a 4-segment pool; a per-round working set that fits inside the
/// default pool would never activate the mechanism this profile targets —
/// the same R26-4/"path-activation oracle" concern this project's other
/// gates apply; see `segments_reserved_total`/`decommit_calls_total` in
/// this file's own emitted RESULT lines, the activation check this probe's
/// caller (the gate report) must read before trusting the latency number).
const OBJS_PER_ROUND: usize = 18_504;

/// Rounds per thread — the continuous-cycle axis (vs. the original
/// micro-benchmark's single 8-batch timed window). Chosen so the full
/// `THREADS`-way run completes in low single-digit seconds per process
/// launch (comfortably inside the paired-ab-runner's per-arm budget for a
/// 20-pair real-claim comparison), after `OBJS_PER_ROUND` was raised to
/// genuinely activate the pool-overflow mechanism.
const ROUNDS: usize = 6;

/// Fraction (out of `OBJS_PER_ROUND`) of the previous round's objects that
/// are churned (freed + replaced) mid-round before the round's remaining
/// objects are all freed at round end — reproduces a connection-pool-style
/// mix of short- and slightly-longer-lived objects instead of every object
/// having identical lifetime.
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
/// (joined before the timer stops) — the metric an application actually
/// cares about, not a per-thread average.
fn run_arm(arm_name: &str, global: &'static SeferAlloc) {
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
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
    proc_probe::emit_u64("decommit_calls_total", stats.decommit_calls);
    proc_probe::emit_u64("large_cache_hits", stats.large_cache_hits);
    proc_probe::emit_u64("rss_after_kib", snap.rss / 1024);
    proc_probe::emit_u64("commit_after_kib", snap.commit / 1024);
}
