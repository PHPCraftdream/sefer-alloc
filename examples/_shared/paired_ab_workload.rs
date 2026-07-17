// Shared workload for the three `paired_ab_*` process-level judge binaries
// (task R6-OPT-A6, `radical_optimization_review` §5.5 item 1 / §5.6 / §6
// Stage A.1-2).
//
// ## Why this file exists, and why it is `include!`d rather than a real module
//
// `benches/global_alloc.rs` already compares SeferAlloc / mimalloc / System
// head-to-head, but it does so by calling each allocator's `GlobalAlloc`
// impl **directly** (`bench_direct_alloc(sefer_ref, layout)` etc.) inside
// ONE process — none of the three is ever actually installed as that
// process's `#[global_allocator]`. That is a legitimate, honest, apples-to-
// apples comparison of the raw alloc/dealloc hot path, but it is NOT the
// same codegen shape as a real production binary, where `#[global_allocator]`
// routes *every* `Vec`/`Box`/etc. allocation in the whole program through one
// indirection (a different inlining/linker shape than a directly-called
// generic function). This task builds that OTHER kind of judge: three
// separate, minimal, standalone binaries, each *actually* installing its own
// `#[global_allocator]`, driven by `scripts/paired-ab-runner.mjs` as
// alternating separate OS processes so wall-clock differences are measured
// at full process-level fidelity (page faults, real linker layout, real
// first-touch cost — not just the hot loop).
//
// For the comparison to be honest, the ONLY difference between the three
// binaries must be which allocator is installed — so this file holds the
// literal workload body, `include!`d verbatim into
// `examples/paired_ab_sefer.rs`, `examples/paired_ab_mimalloc.rs`, and
// `examples/paired_ab_system.rs`. `include!` (not a shared library module)
// is used because Cargo examples are independent compilation units with no
// shared `examples/`-support crate in this project (the same reasoning
// `dealloc_only_unbound_thread.rs`'s module doc gives for duplicating the
// RSS/commit probe rather than factoring it into a shared crate) — but
// duplicating the WORKLOAD itself (as opposed to a small stable OS probe)
// would risk exactly the three-binaries-silently-drift-apart failure mode
// the task explicitly warns against, so here we go one step further than
// A5's "duplicate a small stable probe" precedent and share the workload
// body via `include!`, guaranteeing byte-for-byte identical workload code
// compiled into all three binaries.
//
// ## The workload itself — lifted from `benches/global_alloc.rs`, not invented
//
// Two operations, matching `benches/global_alloc.rs`'s `churn_step`/
// `churn_step_write` and its `Vec_push` closures exactly in shape (same
// sizes, same working-set, same PRNG, same growth sequence):
//
// 1. Churn: a `CHURN_WORKING_SET`-sized pool of live blocks; each of
//    `CHURN_OPS` steps frees a pseudo-random slot (fixed xorshift64* seed
//    `0xCAFE`, identical algorithm to the bench file) and reallocates a
//    replacement, writing the first 16 bytes of the fresh block (the bench
//    file's "write" variant — chosen as the default here because it is the
//    more realistic pattern: real code almost always writes into what it
//    just allocated).
// 2. `Vec<i64>` push/grow churn: geometric growth (capacity doubles
//    4, 8, 16, ... exactly as `Vec` does) over `VEC_PUSHES` elements,
//    calling the real global `alloc`/`dealloc` faces (`Vec::new` +
//    `Vec::push`, which is how a real program would write this — unlike the
//    bench file's manual `Layout`-driven replica, this binary can use `Vec`
//    directly BECAUSE the allocator under test is the process's actual
//    `#[global_allocator]`).
//
// Both operations run for `ROUNDS` rounds so the timed region is large
// enough for `Instant`-based wall-clock measurement to be meaningful (a
// single pass is sub-microsecond; this file's caller times the whole
// `run_workload()` call as one unit, matching a process-level judge's
// granularity — see `scripts/paired-ab-runner.mjs`'s module doc for the
// statistical treatment of the resulting per-process timings).

use std::hint::black_box;

// ---------------------------------------------------------------------------
// RSS / commit-charge probes — thin KiB wrappers over the `proc-probe` crate's
// re-export of `proc-memstat`'s same-instant `snapshot()` (bytes). Defined ONCE
// here (this file is `include!`d verbatim into all three `paired_ab_*`
// binaries), so the OS FFI that used to be copy-pasted into each of the three
// binaries now lives in ONE place (`crates/proc-memstat`, reached via
// `proc-probe`'s "measure + report" re-export). Printed line names/units are
// unchanged. The `RESULT` lines themselves are emitted via `proc_probe::emit_*`
// (see each binary's `main`), so the stdout protocol the runner parses can
// never drift from a hand-rolled `println!`.
// ---------------------------------------------------------------------------

/// Resident set size in KiB (bytes / 1024 from `proc_probe::snapshot`).
fn rss_kib() -> u64 {
    proc_probe::snapshot().rss / 1024
}

/// Commit charge in KiB (bytes / 1024 from `proc_probe::snapshot`).
fn commit_kib() -> u64 {
    proc_probe::snapshot().commit / 1024
}

/// Same sizes `benches/global_alloc.rs::SIZES` sweeps.
const SIZES: &[usize] = &[16, 64, 256, 1024];

/// Same op-pair count per size as the bench file's `OPS`.
const CHURN_OPS: usize = 1024;

/// Same working-set size as the bench file's `CHURN_WORKING_SET`.
const CHURN_WORKING_SET: usize = 256;

/// Number of `Vec` push/grow cycles per round, matching the bench file's
/// `VEC_PUSHES`.
const VEC_PUSHES: usize = 512;

/// How many times the whole workload (all 4 churn sizes + the Vec-push
/// pattern) repeats within one timed `run_workload()` call. Chosen so a
/// single process's timed region is comfortably multi-millisecond even on a
/// fast host, giving `Instant` enough resolution headroom without inflating
/// per-process wall-clock past what a `--quick` iteration cycle wants.
const ROUNDS: usize = 8;

/// Deterministic, dependency-free PRNG (xorshift64*) — copied verbatim from
/// `benches/global_alloc.rs::XorShift64` (fixed seed, same algorithm) so the
/// exact same pseudo-random free/alloc index sequence drives every binary.
struct XorShift64(u64);

impl XorShift64 {
    const fn new(seed: u64) -> Self {
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

/// Churn (write variant): pre-fill `CHURN_WORKING_SET` blocks of `size`
/// bytes, then run `CHURN_OPS` free+realloc+write steps, matching
/// `benches/global_alloc.rs::churn_prefill_write` + `churn_step_write`
/// exactly (same PRNG seed `0xCAFE`, same 16-byte write pattern). Uses
/// `std::alloc::{alloc, dealloc}` directly against the GLOBAL allocator
/// (there is no `Layout`-taking `GlobalAlloc` reference to call through
/// here — the whole point of this harness is that the installed
/// `#[global_allocator]` handles these calls, whichever of the three
/// binaries this is compiled into).
fn churn_write(size: usize) {
    let layout = std::alloc::Layout::from_size_align(size, 8).unwrap();

    let mut live: Vec<*mut u8> = Vec::with_capacity(CHURN_WORKING_SET);
    for _ in 0..CHURN_WORKING_SET {
        // SAFETY: `layout` has non-zero size and valid (power-of-two, <=
        // usize::MAX/2) alignment (8), satisfying `GlobalAlloc::alloc`'s
        // preconditions.
        let p = unsafe { std::alloc::alloc(layout) };
        if !p.is_null() {
            // SAFETY: `p` is a freshly allocated block of at least 16 bytes
            // (every SIZES entry is >= 16), so the first two u64 words are
            // in bounds and writable; `write_volatile` prevents the store
            // being optimized away.
            unsafe { std::ptr::write_volatile(p.cast::<u64>(), 0xA5A5_A5A5_A5A5_A5A5) };
            unsafe { std::ptr::write_volatile(p.cast::<u64>().add(1), 0xA5A5_A5A5_A5A5_A5A5) };
        }
        live.push(p);
    }

    let mut rng = XorShift64::new(0xCAFE);
    for _ in 0..CHURN_OPS {
        let idx = rng.next_usize() % CHURN_WORKING_SET;
        let old = live[idx];
        if !old.is_null() {
            // SAFETY: `old` was allocated above with this same `layout` and
            // is freed exactly once here before being overwritten below.
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

    for &p in &live {
        if !p.is_null() {
            // SAFETY: `p` is still live (every slot was freed-and-replaced,
            // never freed twice) and was allocated with this same `layout`.
            unsafe { std::alloc::dealloc(p, layout) };
        }
    }
}

/// `Vec<i64>` push/grow churn: geometric growth exactly mirroring
/// `benches/global_alloc.rs`'s `Vec_push` closures, but via the REAL `Vec`
/// API (not a manual `Layout`-driven replica) — legitimate here specifically
/// because the allocator under test is genuinely installed as
/// `#[global_allocator]`, so `Vec::push`'s internal `alloc`/`realloc` calls
/// route through it exactly as a real program's would.
fn vec_push_churn() {
    let mut v: Vec<i64> = Vec::new();
    for i in 0..VEC_PUSHES {
        v.push(i as i64);
    }
    black_box(&v);
    drop(v);
}

/// Run the full shared workload once: all 4 `SIZES` through `churn_write`,
/// then one `vec_push_churn` pass, repeated `ROUNDS` times. This is the
/// single function all three `paired_ab_*` binaries call identically — the
/// ONLY difference between the three binaries is which `#[global_allocator]`
/// is installed when this function's `alloc`/`dealloc`/`Vec` calls resolve.
fn run_workload() {
    for _ in 0..ROUNDS {
        for &size in SIZES {
            churn_write(size);
        }
        vec_push_churn();
    }
}
