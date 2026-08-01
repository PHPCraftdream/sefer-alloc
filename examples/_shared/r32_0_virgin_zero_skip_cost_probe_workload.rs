// R32-0 (task #490) — shared workload for the `paired-ab-runner.mjs`
// wall-clock A/B companion to the `benches/r32_0_virgin_zero_skip_cost_side_gate.rs`
// deterministic gate. `include!`d by both
// `examples/r32_0_cost_probe_alloc_recycled_{off,on}.rs`.
//
// **ALLOCATOR LAYER UNDER TEST:** the real `HeapCore` production chain (via
// `HeapRegistry::claim`), matching this task's own bench gate and R31-0's
// established entry-point-layer precedent — NOT bare `AllocCore`.
//
// **Scenario:** plain-`alloc` RECYCLED (the worst-case cost arm this task
// exists to measure — a block that gets NO benefit from `virgin-zero-skip`
// but still pays the feature's per-hit/per-push mask bookkeeping on every
// iteration). One fixed size (4 KiB, `refill_n = 16` — the class with the
// MOST magazine slots and therefore the class where a per-slot-bit cost, if
// any were real, would be most likely to show up per relative call). A
// single process launch runs `ROUNDS` iterations of prime-once +
// alloc+dealloc loop and reports one `elapsed_ns` total — the metric
// `scripts/paired-ab-runner.mjs` pairs by default.

use sefer_alloc::registry::{bootstrap, HeapRegistry};
use std::alloc::Layout;
use std::time::Instant;

const SIZE: usize = 4096;
const ROUNDS: u64 = 200_000;

fn run(arm_label: &str) {
    let _ = bootstrap::ensure();
    let p = HeapRegistry::claim();
    assert!(!p.is_null(), "HeapRegistry::claim returned null");
    // SAFETY: `p` was just returned by `claim`; this process owns it for the
    // remainder of this single-shot binary.
    let heap = unsafe { &mut *p };

    let layout = Layout::from_size_align(SIZE, 8).unwrap();
    let prime = heap.alloc(layout);
    assert!(!prime.is_null(), "prime alloc returned null");
    // SAFETY: `prime` is non-null and valid for `SIZE` bytes.
    unsafe { core::ptr::write_bytes(prime, 0xAA, SIZE) };
    // SAFETY: `prime` was returned by `heap.alloc(layout)` above, freed once.
    unsafe { heap.dealloc(prime, layout) };

    let hits_before = heap.tcache_hits();

    let t0 = Instant::now();
    for _ in 0..ROUNDS {
        let p = heap.alloc(layout);
        std::hint::black_box(p);
        // SAFETY: `p` was returned by the `alloc` call immediately above,
        // freed exactly once per loop iteration (LIFO re-serving `prime`).
        unsafe { heap.dealloc(p, layout) };
    }
    let elapsed_ns = t0.elapsed().as_nanos();

    let hits_after = heap.tcache_hits();
    let hits_delta = hits_after - hits_before;

    // Path-activation oracle (CLAUDE.md R30-8): every one of `ROUNDS`
    // iterations must be a magazine HIT (the same physical block re-served
    // in a tight LIFO loop) — a miss-starved run would silently measure
    // refill cost instead of the steady-state hit-path cost this probe
    // claims to isolate.
    assert_eq!(
        hits_delta, ROUNDS,
        "R32-0 cost probe ({arm_label}): expected every one of {ROUNDS} \
         rounds to be a magazine HIT (tight LIFO re-serve of the primed \
         block), got {hits_delta} hits — the probe did not exercise the \
         steady-state recycled path it claims to measure"
    );

    proc_probe::emit("arm", arm_label);
    proc_probe::emit_u64("size", SIZE as u64);
    proc_probe::emit_u64("rounds", ROUNDS);
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_ns("ns_per_round", elapsed_ns / ROUNDS as u128);
    proc_probe::emit_u64("oracle_hits_delta", hits_delta);
    // No installed-allocator sanity gate: both binaries link `sefer_alloc`
    // as an ordinary library (never installed as `#[global_allocator]`), so
    // there is no "which allocator is globally active" question to check —
    // unlike the `paired_ab_{sefer,mimalloc,system}` built-in profile.
}
