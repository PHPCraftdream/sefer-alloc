// R34-23 (task #542): shared workload body for the three real-Vec worker
// binaries (`r34_23_vec_worker_{sefer,mimalloc,system}.rs`).
//
// ## Why this exists
//
// Deliverable #2 of R34-23: a REAL `Vec<u8>` (not manual alloc/realloc)
// driving std's own growth-factor / amortized-realloc logic through the
// process's installed `#[global_allocator]`. `benches/large_realloc.rs` and
// the R34-23 direct gate (`r34_23_realloc_direct_gate.rs`) call
// `GlobalAlloc::realloc` directly — useful for controlled per-pattern
// measurement, but they bypass `Vec`'s own capacity-doubling policy and the
// magazine-aware `HeapCore::alloc`/`realloc` chain a real `Vec.push()` goes
// through. This workload closes that gap: a genuine `Vec<u8>` that grows via
// `.push()`, `.reserve()`, and `.shrink_to_fit()`, timing the growth phase so
// the cross-allocator ratio reflects what a real program pays.
//
// Each worker binary installs its OWN `#[global_allocator]` (SeferAlloc /
// mimalloc / System) and is launched as a FRESH subprocess by
// `scripts/r34_23_vec_harness.mjs` — so cross-allocator contamination is
// impossible by construction (the R32/R33 §"P1" non-causality critique). The
// `include!` pattern guarantees byte-for-byte identical workload code across
// all three binaries; the ONLY difference is the `#[global_allocator]` static.
//
// ## Workload shapes
//
// 1. **`growth`** — `Vec::new()` then `.push(byte)` to `target` bytes. Each
//    capacity doubling is one `GlobalAlloc::realloc` through the installed
//    allocator. We count capacity changes (= realloc count) so the harness
//    can compute ns/realloc alongside ns/push. The push loop writes each byte
//    so the preserved prefix is genuinely touched (would catch in-place
//    realloc corruption — though the canary check in the direct gate is the
//    primary corruption detector; here we write real data).
// 2. **`shrink_grow`** — push to `target`, `.shrink_to_fit()` (one realloc —
//    always a move, since no allocator shrinks in place here), then push
//    to `2*target`. Tests the shrink realloc path interleaved with regrowth.
// 3. **`reserve_exact`** — `.reserve_exact(n)` in a geometric sequence (no
//    pushing between, just capacity reservations). This is the closest a real
//    `Vec` API gets to a "pure realloc chain" without push overhead.

use std::hint::black_box;
use std::time::Instant;

/// One `growth` round: push `target` bytes into a fresh `Vec<u8>`, counting
/// capacity changes. Returns `(elapsed_ns, realloc_count, final_capacity)`.
fn growth_round(target: usize) -> (u128, usize, usize) {
    let mut v: Vec<u8> = Vec::new();
    let mut reallocs = 0;
    let mut last_cap = 0usize;
    let t0 = Instant::now();
    for i in 0..target {
        v.push((i & 0xFF) as u8);
        let cap = v.capacity();
        if cap != last_cap {
            reallocs += 1;
            last_cap = cap;
        }
    }
    let elapsed = t0.elapsed().as_nanos();
    let final_cap = v.capacity();
    black_box(&v);
    (elapsed, reallocs, final_cap)
}

/// One `shrink_grow` round: push to `target`, shrink_to_fit, push to
/// `2*target`. Returns `(elapsed_ns, realloc_count)`.
fn shrink_grow_round(target: usize) -> (u128, usize) {
    let mut v: Vec<u8> = Vec::new();
    let mut reallocs = 0;
    let mut last_cap = 0usize;
    let t0 = Instant::now();
    for i in 0..target {
        v.push((i & 0xFF) as u8);
        let cap = v.capacity();
        if cap != last_cap { reallocs += 1; last_cap = cap; }
    }
    // shrink_to_fit forces a realloc to exact len (always a move here).
    v.shrink_to_fit();
    reallocs += 1;
    let half = target;
    for i in 0..target {
        v.push(((half + i) & 0xFF) as u8);
        let cap = v.capacity();
        if cap != last_cap { reallocs += 1; last_cap = cap; }
    }
    let elapsed = t0.elapsed().as_nanos();
    black_box(&v);
    (elapsed, reallocs)
}

/// One `reserve_exact` round: build a fresh `Vec<u8>` to `target`, then call
/// `.reserve_exact(target)` (doubling), pushing into the new region, then
/// `.reserve_exact(2*target)` — each step FORCES a realloc because the
/// requested total exceeds current capacity. This is the closest a real Vec
/// API gets to a controlled geometric realloc chain. Returns `(elapsed_ns,
/// realloc_count)`.
fn reserve_exact_round(target: usize) -> (u128, usize) {
    let mut v: Vec<u8> = Vec::new();
    let mut reallocs = 0;
    let t0 = Instant::now();
    // Initial fill to target (push-driven growth — several reallocs).
    let mut last_cap = 0usize;
    for i in 0..target {
        v.push((i & 0xFF) as u8);
        let cap = v.capacity();
        if cap != last_cap { reallocs += 1; last_cap = cap; }
    }
    // Now force two explicit large reallocs via reserve_exact, writing into
    // each new region so the next reserve has real data to preserve.
    for &mult in &[2usize, 4] {
        let want = target * mult;
        let cap_before = v.capacity();
        v.reserve_exact(want - v.len());
        if v.capacity() != cap_before { reallocs += 1; }
        while v.len() < want { v.push(0xA5); }
    }
    let elapsed = t0.elapsed().as_nanos();
    black_box(&v);
    (elapsed, reallocs)
}

/// Run the timed measurement for one shape: one untimed warmup round, then
/// `iterations` timed rounds. Emits `RESULT` lines (one summary per shape per
/// iteration) in the proc-probe protocol the harness parses.
fn run_shape(shape: &str, iterations: usize, body: impl Fn() -> (u128, usize)) {
    // Warmup (untimed).
    let _ = body();

    for rep in 0..iterations {
        let (elapsed_ns, reallocs) = body();
        proc_probe::emit("shape", shape);
        proc_probe::emit_u64("rep", rep as u64);
        proc_probe::emit_ns("elapsed_ns", elapsed_ns);
        proc_probe::emit_u64("realloc_count", reallocs as u64);
    }
}

/// Run ALL shapes for `iterations` timed rounds each, then emit the sefer-only
/// realloc path-activation oracle deltas (read once before all shapes and once
/// after, so the delta covers every shape combined — the per-shape breakdown
/// comes from the direct gate, not here).
#[cfg(feature = "alloc-stats")]
fn run_all_shapes(iterations: usize) {
    let il0 = sefer_alloc::AllocCore::dbg_reloc_inplace_large_count();
    let is0 = sefer_alloc::AllocCore::dbg_reloc_inplace_small_count();
    let d0 = sefer_alloc::AllocCore::dbg_reloc_fastpath_decline_count();

    run_shape("growth_4mib", iterations, || {
        let (e, r, _) = growth_round(4 * 1024 * 1024);
        (e, r)
    });
    run_shape("growth_1mib", iterations, || {
        let (e, r, _) = growth_round(1024 * 1024);
        (e, r)
    });
    run_shape("shrink_grow_1mib", iterations, || shrink_grow_round(1024 * 1024));
    run_shape("reserve_exact_geom", iterations, || reserve_exact_round(1024 * 1024));

    let il1 = sefer_alloc::AllocCore::dbg_reloc_inplace_large_count();
    let is1 = sefer_alloc::AllocCore::dbg_reloc_inplace_small_count();
    let d1 = sefer_alloc::AllocCore::dbg_reloc_fastpath_decline_count();
    proc_probe::emit_u64("oracle_inplace_large_delta", il1 - il0);
    proc_probe::emit_u64("oracle_inplace_small_delta", is1 - is0);
    proc_probe::emit_u64("oracle_decline_delta", d1 - d0);
}

/// Non-`alloc-stats` build: same shapes, no oracle (counters read 0 anyway).
#[cfg(not(feature = "alloc-stats"))]
fn run_all_shapes(iterations: usize) {
    run_shape("growth_4mib", iterations, || {
        let (e, r, _) = growth_round(4 * 1024 * 1024);
        (e, r)
    });
    run_shape("growth_1mib", iterations, || {
        let (e, r, _) = growth_round(1024 * 1024);
        (e, r)
    });
    run_shape("shrink_grow_1mib", iterations, || shrink_grow_round(1024 * 1024));
    run_shape("reserve_exact_geom", iterations, || reserve_exact_round(1024 * 1024));
    proc_probe::emit_u64("oracle_inplace_large_delta", 0);
    proc_probe::emit_u64("oracle_inplace_small_delta", 0);
    proc_probe::emit_u64("oracle_decline_delta", 0);
}

/// Parse `--iterations <N>` from the command line (required, >= 1).
fn parse_iterations() -> usize {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i].as_str() == "--iterations" {
            i += 1;
            if let Some(n) = args.get(i).and_then(|s| s.parse().ok()) {
                if n >= 1 {
                    return n;
                }
            }
        }
        i += 1;
    }
    eprintln!(
        "usage: {} --iterations <N>=1..\n(got args: {:?})",
        args.first().map(String::as_str).unwrap_or("worker"),
        &args[1..],
    );
    std::process::exit(2);
}
