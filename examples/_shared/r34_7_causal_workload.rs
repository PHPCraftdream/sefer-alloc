// R34-7 (task #526): shared workload body for the three causal-harness worker
// binaries (`r34_7_causal_worker_{sefer,mimalloc,system}.rs`).
//
// ## Why this file exists
//
// `benches/global_alloc.rs` compares SeferAlloc / mimalloc / System by calling
// each allocator's `GlobalAlloc` impl directly in ONE process — none is ever
// installed as `#[global_allocator]`, and only SeferAlloc's state is reset
// between groups. The R32/R33 review
// (`docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md` §"P1 —
// `global_alloc` нельзя использовать для причинного run-over-run вердикт")
// proved this is non-causal: in one run, the control arms (mimalloc/System,
// whose code never changed) "regressed" by +59%/+71% — more than SeferAlloc's
// own +53% — proving the entire signal was host-state drift, not code.
//
// This file is the shared workload body for the causal replacement: three
// separate binaries, each ACTUALLY installing its own `#[global_allocator]`,
// driven by `scripts/r34_7_causal_harness.mjs` as fresh subprocesses so
// wall-clock differences are measured at full process-level fidelity. The
// `include!` pattern (same as `examples/_shared/paired_ab_workload.rs`) guarantees
// byte-for-byte identical workload code across all three binaries — the ONLY
// difference is the `#[global_allocator]` static in each wrapper file.
//
// ## The workload — churn-write (the exact pattern the old bench measured)
//
// One "round" of `churn_write_round(size)`:
//   1. Pre-fill `CHURN_WORKING_SET` (256) live blocks of `size` bytes (untimed
//      warmup — first-touch page faults, carve).
//   2. `CHURN_OPS` (1024) free+realloc+write steps: free a pseudo-random slot,
//      allocate a replacement, write the first 16 bytes. This is the steady-
//      state churn pattern a per-thread magazine (tcache) wins on (identical
//      algorithm to `benches/global_alloc.rs::churn_step_write`).
//   3. Teardown: free every remaining live block.
//
// Each worker binary calls `churn_write_round` `iterations` times inside a
// timed region, preceded by one untimed warmup round. The reported
// `ns_per_op = elapsed_ns / (iterations × CHURN_OPS)` is nanoseconds per
// free+alloc pair — directly comparable to the old bench's churn-write column.

use std::hint::black_box;

/// Same op-pair count per round as `benches/global_alloc.rs::OPS`.
const CHURN_OPS: usize = 1024;

/// Same working-set size as `benches/global_alloc.rs::CHURN_WORKING_SET`.
const CHURN_WORKING_SET: usize = 256;

/// Deterministic, dependency-free PRNG (xorshift64*) — identical algorithm to
/// `benches/global_alloc.rs::XorShift64` (fixed seed, same constants) so the
/// exact same pseudo-random free/alloc index sequence drives every binary and
/// every run.
struct XorShift64(u64);

impl XorShift64 {
    const fn new(seed: u64) -> Self {
        // Avoid the all-zero fixed point.
        Self(seed | 1)
    }

    #[inline]
    fn next_usize(&mut self) -> usize {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D) as usize
    }
}

/// One complete churn-write round: pre-fill `CHURN_WORKING_SET` blocks of
/// `size` bytes, run `CHURN_OPS` free+realloc+write steps (same PRNG seed
/// `0xCAFE` as `benches/global_alloc.rs`), then free all remaining live
/// blocks. Uses `std::alloc::{alloc, dealloc}` directly against the process's
/// installed `#[global_allocator]` — whichever of the three binaries this code
/// is compiled into determines which allocator handles these calls.
fn churn_write_round(size: usize) {
    let layout = std::alloc::Layout::from_size_align(size, 8).unwrap();

    // ── Pre-fill (untimed warmup) ─────────────────────────────────────────
    let mut live: Vec<*mut u8> = Vec::with_capacity(CHURN_WORKING_SET);
    for _ in 0..CHURN_WORKING_SET {
        // SAFETY: `layout` has non-zero size and valid alignment (8), satisfying
        // `GlobalAlloc::alloc`'s preconditions.
        let p = unsafe { std::alloc::alloc(layout) };
        if !p.is_null() {
            // SAFETY: `p` is a freshly allocated block of at least 16 bytes
            // (the worker rejects sizes < 16 in `main`), so the first two u64
            // words are in bounds and writable; `write_volatile` prevents the
            // store being optimized away.
            unsafe { std::ptr::write_volatile(p.cast::<u64>(), 0xA5A5_A5A5_A5A5_A5A5) };
            unsafe { std::ptr::write_volatile(p.cast::<u64>().add(1), 0xA5A5_A5A5_A5A5_A5A5) };
        }
        live.push(p);
    }

    // ── Steady-state churn (this is what gets timed across iterations) ─────
    let mut rng = XorShift64::new(0xCAFE);
    for _ in 0..CHURN_OPS {
        let idx = rng.next_usize() % CHURN_WORKING_SET;
        let old = live[idx];
        if !old.is_null() {
            // SAFETY: `old` was allocated above with this same `layout` and is
            // freed exactly once here before being overwritten below.
            unsafe { std::alloc::dealloc(old, layout) };
        }
        // SAFETY: same layout preconditions as the prefill alloc above.
        let p = unsafe { std::alloc::alloc(layout) };
        if !p.is_null() {
            // SAFETY: same bounds/volatility reasoning as the prefill write.
            unsafe { std::ptr::write_volatile(p.cast::<u64>(), 0xA5A5_A5A5_A5A5_A5A5) };
            unsafe { std::ptr::write_volatile(p.cast::<u64>().add(1), 0xA5A5_A5A5_A5A5_A5A5) };
        }
        live[idx] = p;
    }

    black_box(&live);

    // ── Teardown ──────────────────────────────────────────────────────────
    for &p in &live {
        if !p.is_null() {
            // SAFETY: `p` is still live (every slot was freed-and-replaced,
            // never freed twice) and was allocated with this same `layout`.
            unsafe { std::alloc::dealloc(p, layout) };
        }
    }
}

/// Run the timed measurement: one untimed warmup round, then `iterations`
/// timed rounds of `churn_write_round(size)`. Returns total elapsed
/// nanoseconds for the timed region only.
fn run_timed(size: usize, iterations: usize) -> u128 {
    // Warmup (untimed) — fills caches, commits pages.
    churn_write_round(size);

    let t0 = std::time::Instant::now();
    for _ in 0..iterations {
        churn_write_round(size);
    }
    t0.elapsed().as_nanos()
}

/// Parse `--size <N> --iterations <N>` from the process's command line. Both
/// are required. `size` must be >= 16 (the churn-write workload writes two u64
/// words = 16 bytes into every fresh block). On any parse/validation error the
/// process prints usage to stderr and exits non-zero — a worker that fails to
/// parse must NOT emit a `RESULT ns_per_op=` line, or the orchestrator would
/// pair a garbage value.
fn parse_size_and_iterations() -> (usize, usize) {
    let args: Vec<String> = std::env::args().collect();
    let mut size: Option<usize> = None;
    let mut iterations: Option<usize> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--size" => {
                i += 1;
                size = args.get(i).and_then(|s| s.parse().ok());
            }
            "--iterations" => {
                i += 1;
                iterations = args.get(i).and_then(|s| s.parse().ok());
            }
            _ => {}
        }
        i += 1;
    }
    let size = match size {
        Some(s) if s >= 16 => s,
        _ => {
            eprintln!(
                "usage: {} --size <N>=16.. --iterations <N>=1..\n(got args: {:?})",
                args.first().map(String::as_str).unwrap_or("worker"),
                &args[1..],
            );
            std::process::exit(2);
        }
    };
    let iterations = match iterations {
        Some(n) if n >= 1 => n,
        _ => {
            eprintln!(
                "usage: {} --size <N>=16.. --iterations <N>=1..\n(got args: {:?})",
                args.first().map(String::as_str).unwrap_or("worker"),
                &args[1..],
            );
            std::process::exit(2);
        }
    };
    (size, iterations)
}
