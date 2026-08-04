//! R34-23 (task #542) — direct `GlobalAlloc::realloc` geometric-chain gate.
//!
//! ## Why this exists
//!
//! `benches/large_realloc.rs` calls `GlobalAlloc::realloc` directly (good) but:
//! (a) only x2-doubling + one neighbour-pressure scenario, (b) never touches
//! the payload (`black_box(ptr)` only — cannot detect in-place realloc data
//! corruption), (c) no committed-bytes reporting, (d) criterion ns/op only,
//! no raw per-sample data for a derive-script pipeline. This binary closes
//! those gaps for the DIRECT-realloc axis (deliverable #1 of R34-23): growth
//! factors x1.25 / x1.5 / x2, a shrink/grow oscillation, and the README's two
//! headline patterns (`realloc_grow_geometric` 64 B→4 MiB and
//! `realloc_grow_neighbour_pressure`), each measured under a COPIED payload
//! (write+verify a canary through the pointer on every step) and an UNTOUCHED
//! payload (the existing `black_box`-only style, for "pure" cost comparison).
//!
//! ## Causality
//!
//! Launched as a FRESH subprocess per allocator by
//! `scripts/r34_23_realloc_direct_harness.mjs` (one process per `--allocator`
//! value) — NOT three allocators sharing one process. The R32/R33 review
//! (`docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md` §"P1")
//! proved in-process cross-allocator comparison is non-causal (control arms
//! "regressed" +59 %/+71 % from host drift alone). Direct `GlobalAlloc` trait
//! calls with fresh instances are cleaner than `#[global_allocator]`
//! installation, but we isolate per-allocator anyway so the comparison is
//! causal by construction: a fresh process has empty allocator state and no
//! cross-arm thermal/page-cache coupling.
//!
//! ## Path-activation oracle (CLAUDE.md R30-8)
//!
//! For the sefer arm only, this binary reads the three process-wide realloc
//! counters (`dbg_reloc_inplace_large_count` / `_small_count` /
//! `_fastpath_decline_count`) before and after each cell. The delta proves
//! which code path actually fired under each growth pattern — not just that
//! the config resolved. `inplace_large + inplace_small + decline == total`
//! reallocs that reached the fast-path detection. The mimalloc/System arms
//! emit 0 for all three (they never touch `AllocCore`).
//!
//! ## Output protocol
//!
//! Emits CSV-ish lines (comma-separated, no spaces) that
//! `scripts/r34_23_realloc_direct_harness.mjs` parses:
//!   `HEADER,...`   — column names (once, first line)
//!   `SAMPLE,...`   — one per timed grow-chain (raw per-sample data)
//!   `CELL,...`     — one per (pattern, payload) summary with oracle deltas
//!
//! USAGE (the harness drives this; manual):
//!   r34_23_realloc_direct_gate --allocator sefer --samples 30

#![cfg(feature = "alloc-global")]
#![allow(clippy::cast_possible_truncation)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::time::Instant;

use sefer_alloc::SeferAlloc;

// ── Growth patterns ──────────────────────────────────────────────────────────

/// One geometric grow chain: start at `start`, multiply by `factor` (rounded
/// up to the alignment) until `>= target`, capping at `max_steps` reallocs.
struct Pattern {
    name: &'static str,
    start: usize,
    factor_num: usize, // numerator; actual factor = factor_num / factor_den
    factor_den: usize,
    target: usize,
    max_steps: usize,
}

/// x2, 64 B → 4 MiB, 16 doublings — exactly the README `realloc_grow_geometric`.
const P_GEOMETRIC_X2_4MIB: Pattern = Pattern {
    name: "geometric_x2_4mib",
    start: 64,
    factor_num: 2,
    factor_den: 1,
    target: 4 * 1024 * 1024,
    max_steps: 16,
};

/// x2, 64 B → 1 MiB, 14 doublings — bounded target for cross-factor comparison.
const P_GEOMETRIC_X2_1MIB: Pattern = Pattern {
    name: "geometric_x2_1mib",
    start: 64,
    factor_num: 2,
    factor_den: 1,
    target: 1024 * 1024,
    max_steps: 16,
};

/// x1.5, 64 B → 1 MiB.
const P_GEOMETRIC_X1P5_1MIB: Pattern = Pattern {
    name: "geometric_x1p5_1mib",
    start: 64,
    factor_num: 3,
    factor_den: 2,
    target: 1024 * 1024,
    max_steps: 24,
};

/// x1.25, 64 B → 1 MiB.
const P_GEOMETRIC_X1P25_1MIB: Pattern = Pattern {
    name: "geometric_x1p25_1mib",
    start: 64,
    factor_num: 5,
    factor_den: 4,
    target: 1024 * 1024,
    max_steps: 32,
};

/// Shrink/grow oscillation: grow 64 B → 1 MiB (x2), then 3 cycles of
/// shrink-to-half + grow-back. Tests the shrink (always moves — OPT-G is
/// grow-or-same only) interleaved with re-grow (in-place in the fresh
/// segment's span). Returns the realloc step sequence as (old, new) pairs.
fn shrink_grow_steps() -> Vec<(usize, usize)> {
    let mut steps = Vec::new();
    let mut cur = 64;
    let target = 1024 * 1024;
    while cur < target {
        let next = (cur * 2).min(target);
        steps.push((cur, next));
        cur = next;
    }
    // 3 oscillation cycles: shrink to half, grow back to full.
    for _ in 0..3 {
        let half = target / 2;
        steps.push((target, half)); // shrink
        steps.push((half, target)); // grow back
    }
    steps
}

/// The neighbour-pressure pattern from `realloc_grow_neighbour_pressure`:
/// 512 KiB start, +256 KiB per step, 8 steps, 32 live 64 B neighbours. Returns
/// the grow step sizes (the neighbours are allocated separately inside the
/// runner).
fn neighbour_pressure_grow_sizes() -> Vec<usize> {
    let mut sizes = Vec::new();
    let mut cur = 512 * 1024;
    for _ in 0..8 {
        cur += 256 * 1024;
        sizes.push(cur);
    }
    sizes
}

const NEIGHBOUR_COUNT: usize = 32;
const NEIGHBOUR_SIZE: usize = 64;
const ALIGN: usize = 8;

/// Compute the geometric grow sizes for a `Pattern`.
fn geometric_sizes(p: &Pattern) -> Vec<usize> {
    let mut sizes = Vec::new();
    let mut cur = p.start;
    for _ in 0..p.max_steps {
        if cur >= p.target {
            break;
        }
        // next = ceil(cur * factor_num / factor_den), rounded up to ALIGN.
        let raw_num = cur.checked_mul(p.factor_num).expect("size overflow");
        let mut next = raw_num.div_ceil(p.factor_den);
        next = next.div_ceil(ALIGN) * ALIGN;
        if next <= cur {
            next = cur + ALIGN; // guarantee forward progress
        }
        sizes.push(next);
        cur = next;
    }
    sizes
}

// ── Canary helpers (copied-payload mode) ─────────────────────────────────────

/// 8-byte canary value written at the start and (if space) at the last 8 bytes
/// of the current allocation before each realloc, then verified after. A
/// mismatch means the realloc corrupted or lost the preserved prefix — the
/// exact failure mode in-place realloc bugs cause.
const CANARY: u64 = 0xC0DE_FEED_F00D_BA5E;

/// Write the canary at offset 0 and (if size >= 16) at offset (size - 8).
/// # Safety: `ptr` must be valid for a write of `size` bytes.
unsafe fn write_canary(ptr: *mut u8, size: usize) {
    unsafe { ptr.cast::<u64>().write_volatile(CANARY) };
    if size >= 16 {
        unsafe { ptr.add(size - 8).cast::<u64>().write_volatile(CANARY) };
    }
}

/// Verify the canary; returns `true` if intact. # Safety: `ptr` valid for
/// `size` bytes.
unsafe fn verify_canary(ptr: *mut u8, size: usize) -> bool {
    let head = unsafe { ptr.cast::<u64>().read_volatile() };
    if head != CANARY {
        return false;
    }
    if size >= 16 {
        let tail = unsafe { ptr.add(size - 8).cast::<u64>().read_volatile() };
        if tail != CANARY {
            return false;
        }
    }
    true
}

// ── Grow-chain runners ───────────────────────────────────────────────────────

/// Run one geometric grow chain, timing the whole realloc sequence. Returns
/// elapsed nanoseconds. `copied` selects payload mode.
fn run_geometric_chain<A: GlobalAlloc>(a: &A, sizes: &[usize], start: usize, copied: bool) -> u128 {
    let init = Layout::from_size_align(start, ALIGN).unwrap();
    // SAFETY: valid layout.
    let mut ptr = unsafe { a.alloc(init) };
    let mut cur_size = start;
    if ptr.is_null() {
        return 0;
    }
    if copied {
        // SAFETY: ptr valid for cur_size bytes.
        unsafe { write_canary(ptr, cur_size) };
    }

    let t0 = Instant::now();
    for &new_size in sizes {
        let old_layout = Layout::from_size_align(cur_size, ALIGN).unwrap();
        // SAFETY: ptr from prior alloc/realloc with old_layout.
        let new_ptr = unsafe { a.realloc(ptr, old_layout, new_size) };
        if new_ptr.is_null() {
            // SAFETY: free what we have.
            unsafe { a.dealloc(ptr, old_layout) };
            return 0;
        }
        ptr = new_ptr;
        if copied {
            // SAFETY: new_ptr valid for new_size; verify the preserved prefix
            // survived the (possibly in-place) realloc.
            assert!(
                unsafe { verify_canary(ptr, cur_size.min(new_size)) },
                "CANARY CORRUPTION after realloc {cur_size} -> {new_size}"
            );
            // Write the canary into the grown tail region too.
            // SAFETY: ptr valid for new_size.
            unsafe { write_canary(ptr, new_size) };
        }
        cur_size = new_size;
    }
    let elapsed = t0.elapsed().as_nanos();

    black_box(ptr);
    let final_layout = Layout::from_size_align(cur_size, ALIGN).unwrap();
    // SAFETY: ptr is the last successful realloc result.
    unsafe { a.dealloc(ptr, final_layout) };
    elapsed
}

/// Run one shrink/grow oscillation chain.
fn run_oscillation_chain<A: GlobalAlloc>(a: &A, steps: &[(usize, usize)], copied: bool) -> u128 {
    let start = steps[0].0;
    let init = Layout::from_size_align(start, ALIGN).unwrap();
    // SAFETY: valid layout.
    let mut ptr = unsafe { a.alloc(init) };
    let mut cur_size = start;
    if ptr.is_null() {
        return 0;
    }
    if copied {
        // SAFETY: ptr valid for cur_size.
        unsafe { write_canary(ptr, cur_size) };
    }

    let t0 = Instant::now();
    for &(old, new) in steps {
        debug_assert_eq!(old, cur_size, "oscillation step mismatch");
        let old_layout = Layout::from_size_align(cur_size, ALIGN).unwrap();
        // SAFETY: ptr from prior alloc/realloc with old_layout.
        let new_ptr = unsafe { a.realloc(ptr, old_layout, new) };
        if new_ptr.is_null() {
            unsafe { a.dealloc(ptr, old_layout) };
            return 0;
        }
        ptr = new_ptr;
        if copied {
            let check_size = cur_size.min(new);
            assert!(
                unsafe { verify_canary(ptr, check_size) },
                "CANARY CORRUPTION after oscillation realloc {cur_size} -> {new}"
            );
            // SAFETY: ptr valid for new bytes.
            unsafe { write_canary(ptr, new) };
        }
        cur_size = new;
    }
    let elapsed = t0.elapsed().as_nanos();

    black_box(ptr);
    let final_layout = Layout::from_size_align(cur_size, ALIGN).unwrap();
    // SAFETY: ptr is the last realloc result.
    unsafe { a.dealloc(ptr, final_layout) };
    elapsed
}

/// Run one neighbour-pressure chain (matches `realloc_grow_neighbour_pressure`).
fn run_neighbour_pressure<A: GlobalAlloc>(a: &A, grow_sizes: &[usize], copied: bool) -> u128 {
    let start = 512 * 1024;
    let init = Layout::from_size_align(start, ALIGN).unwrap();
    let noise_layout = Layout::from_size_align(NEIGHBOUR_SIZE, ALIGN).unwrap();
    // SAFETY: valid layout.
    let mut subject = unsafe { a.alloc(init) };
    if subject.is_null() {
        return 0;
    }
    let mut subject_size = start;
    if copied {
        // SAFETY: subject valid for start bytes.
        unsafe { write_canary(subject, start) };
    }

    let mut neighbours: Vec<*mut u8> = Vec::with_capacity(NEIGHBOUR_COUNT);
    for _ in 0..NEIGHBOUR_COUNT {
        // SAFETY: valid noise layout.
        let p = unsafe { a.alloc(noise_layout) };
        neighbours.push(p);
    }

    let t0 = Instant::now();
    for &new_size in grow_sizes {
        let old_layout = Layout::from_size_align(subject_size, ALIGN).unwrap();
        // SAFETY: subject from prior alloc/realloc.
        let new_ptr = unsafe { a.realloc(subject, old_layout, new_size) };
        if new_ptr.is_null() {
            unsafe { a.dealloc(subject, old_layout) };
            subject = std::ptr::null_mut();
            break;
        }
        subject = new_ptr;
        if copied {
            assert!(
                unsafe { verify_canary(subject, subject_size.min(new_size)) },
                "CANARY CORRUPTION in neighbour-pressure realloc {subject_size} -> {new_size}"
            );
            // SAFETY: subject valid for new_size.
            unsafe { write_canary(subject, new_size) };
        }
        subject_size = new_size;
    }
    let elapsed = t0.elapsed().as_nanos();

    black_box(subject);
    for &p in &neighbours {
        if !p.is_null() {
            // SAFETY: p from a.alloc(noise_layout).
            unsafe { a.dealloc(p, noise_layout) };
        }
    }
    if !subject.is_null() {
        let final_layout = Layout::from_size_align(subject_size, ALIGN).unwrap();
        // SAFETY: subject is the last realloc result.
        unsafe { a.dealloc(subject, final_layout) };
    }
    elapsed
}

// ── Oracle reads (sefer only) ────────────────────────────────────────────────

#[cfg(feature = "alloc-stats")]
fn oracle_snapshot() -> (u64, u64, u64) {
    (
        sefer_alloc::AllocCore::dbg_reloc_inplace_large_count(),
        sefer_alloc::AllocCore::dbg_reloc_inplace_small_count(),
        sefer_alloc::AllocCore::dbg_reloc_fastpath_decline_count(),
    )
}

#[cfg(not(feature = "alloc-stats"))]
fn oracle_snapshot() -> (u64, u64, u64) {
    (0, 0, 0)
}

// ── Cell runner: all samples for one (pattern, payload) ──────────────────────

fn run_cell(
    pattern: &str,
    payload: &str,
    samples: usize,
    body: impl Fn(usize) -> u128, // (sample_idx) -> ns; does one chain
) {
    let (il_before, is_before, dec_before) = oracle_snapshot();
    let mem_before = proc_probe::snapshot();

    let mut ns_samples: Vec<u128> = Vec::with_capacity(samples);
    for s in 0..samples {
        let ns = body(s);
        ns_samples.push(ns);
        // Emit raw per-sample line.
        println!(
            "SAMPLE,{pattern},{payload},{s},{ns},{rss},{commit}",
            rss = mem_before.rss,
            commit = mem_before.commit,
        );
    }

    let (il_after, is_after, dec_after) = oracle_snapshot();
    let mem_after = proc_probe::snapshot();
    ns_samples.sort_unstable();
    let median = ns_samples[ns_samples.len() / 2];
    let min = ns_samples[0];
    let max = *ns_samples.last().unwrap();
    println!(
        "CELL,{pattern},{payload},{samples},{median},{min},{max},{il_d},{is_d},{dec_d},{rss_b},{commit_b},{rss_a},{commit_a}",
        il_d = il_after - il_before,
        is_d = is_after - is_before,
        dec_d = dec_after - dec_before,
        rss_b = mem_before.rss,
        commit_b = mem_before.commit,
        rss_a = mem_after.rss,
        commit_a = mem_after.commit,
    );
}

// ── Per-allocator driver ─────────────────────────────────────────────────────

fn run_all<A: GlobalAlloc>(a: &A, samples: usize) {
    println!("HEADER,pattern,payload,rep,ns_per_chain,rss_bytes_before,commit_bytes_before");

    // Geometric patterns.
    for p in [
        &P_GEOMETRIC_X2_4MIB,
        &P_GEOMETRIC_X2_1MIB,
        &P_GEOMETRIC_X1P5_1MIB,
        &P_GEOMETRIC_X1P25_1MIB,
    ] {
        for payload in ["copied", "untouched"] {
            let copied = payload == "copied";
            let sizes = geometric_sizes(p);
            let start = p.start;
            run_cell(p.name, payload, samples, move |_s| {
                run_geometric_chain(a, &sizes, start, copied)
            });
        }
    }

    // Shrink/grow oscillation.
    for payload in ["copied", "untouched"] {
        let copied = payload == "copied";
        let osc = shrink_grow_steps();
        run_cell("shrink_grow_osc", payload, samples, move |_s| {
            run_oscillation_chain(a, &osc, copied)
        });
    }

    // Neighbour pressure (README realloc_grow_neighbour_pressure).
    for payload in ["copied", "untouched"] {
        let copied = payload == "copied";
        let np = neighbour_pressure_grow_sizes();
        run_cell("neighbour_pressure", payload, samples, move |_s| {
            run_neighbour_pressure(a, &np, copied)
        });
    }
}

// ── main ─────────────────────────────────────────────────────────────────────

fn parse_allocator() -> String {
    let args: Vec<String> = std::env::args().collect();
    let mut allocator = String::new();
    let mut i = 1;
    while i < args.len() {
        if args[i].as_str() == "--allocator" {
            i += 1;
            allocator = args.get(i).cloned().unwrap_or_default();
        }
        i += 1;
    }
    if allocator.is_empty() {
        eprintln!(
            "usage: {} --allocator <sefer|mimalloc|system> [--samples N]",
            args.first()
                .map(String::as_str)
                .unwrap_or("r34_23_realloc_direct_gate"),
        );
        std::process::exit(2);
    }
    allocator
}

fn parse_samples() -> usize {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i].as_str() == "--samples" {
            i += 1;
            if let Some(v) = args.get(i).and_then(|s| s.parse().ok()) {
                return v;
            }
        }
        i += 1;
    }
    30
}

fn main() {
    let allocator = parse_allocator();
    let samples = parse_samples();

    // One untimed warmup to fill caches / commit pages before timing.
    match allocator.as_str() {
        "sefer" => {
            let a = SeferAlloc::new();
            // Warmup: run each pattern once untimed.
            warmup(&a);
            run_all(&a, samples);
        }
        "mimalloc" => {
            let a = mimalloc::MiMalloc;
            warmup(&a);
            run_all(&a, samples);
        }
        "system" => {
            let a = System;
            warmup(&a);
            run_all(&a, samples);
        }
        other => {
            eprintln!("unknown allocator '{other}' (expected sefer|mimalloc|system)");
            std::process::exit(2);
        }
    }
}

/// One untimed warmup pass per pattern so first-touch page faults and cold
/// carve don't poison the first timed sample.
fn warmup<A: GlobalAlloc>(a: &A) {
    let _ = run_geometric_chain(a, &geometric_sizes(&P_GEOMETRIC_X2_1MIB), 64, false);
    let _ = run_oscillation_chain(a, &shrink_grow_steps(), false);
    let _ = run_neighbour_pressure(a, &neighbour_pressure_grow_sizes(), false);
}
