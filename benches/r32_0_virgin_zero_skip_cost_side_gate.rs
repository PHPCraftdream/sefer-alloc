//! R32-0 (task #490) -- the LOST-front (cost-side) production-layer gate for
//! `virgin-zero-skip`, completing what `docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md`
//! (task #471) deliberately left unmeasured: R31-0 proved the WON front (the
//! `notouch` virgin `alloc_zeroed` win, -89% to -98.6%). It never measured
//! what turning the feature ON costs on paths that collect NO benefit:
//! plain `alloc` (never zeroes anything, virgin or not) and RECYCLED
//! `alloc_zeroed` (the virgin check correctly says "no", so the block is
//! explicit-zeroed either way -- only the bookkeeping differs). CLAUDE.md's
//! cost/benefit rule (added specifically after a prior round paired
//! mismatched regimes) requires both sides measured in the SAME regime
//! before any promotion verdict -- this gate is that missing cost side.
//!
//! ## Entry-point layer (CLAUDE.md's entry-point-honesty rule)
//!
//! **`HeapCore::alloc` / `HeapCore::alloc_zeroed` / `HeapCore::realloc`**
//! (`src/registry/heap_core_alloc.rs`, `src/registry/heap_core_free.rs`) --
//! the SAME layer R31-0 used (a fresh `HeapCore` claimed per rep via
//! `HeapRegistry::claim`, magazine path included), which is the exact chain
//! `SeferAlloc`'s real `#[global_allocator]` (`src/global/sefer_alloc.rs`)
//! calls. This is deliberately NOT bare `AllocCore` -- R30-3 shipped a wrong
//! NO-GO once by measuring that magazine-bypass substrate (see CLAUDE.md's
//! own account and R31-0 §0's defect writeup). This gate reuses R31-0's own
//! harness shape (fresh-heap-per-rep, same sweep sizes, same REPS/BURST) so
//! its numbers sit in the SAME regime as R31-0's already-published WON-front
//! numbers, satisfying CLAUDE.md's same-workload-regime cost/benefit rule.
//!
//! ## What is (and is NOT) measured here
//!
//! 1. **Plain `alloc`, virgin scenario** (fresh heap, same-class burst, no
//!    zeroing requested at all). Source-confirmed
//!    (`heap_core_alloc.rs:207-221`) that `virgin-zero-skip` adds exactly
//!    ONE extra unconditional `u16` AND-with-inverted-bit per magazine-HIT
//!    pop on this path (`self.tcache.classes[c].virgin_mask &= !(1u16 <<
//!    new_cnt)`) -- a defensive mask-invariant maintenance write, not a
//!    read/branch. This is the smallest possible per-hit cost the feature
//!    could add to the allocator's single hottest path (per CLAUDE.md).
//! 2. **Plain `alloc`, recycled scenario** (primed dirty block, tight LIFO
//!    `alloc`+`dealloc` loop) -- same single-AND cost on the hit path, plus
//!    the free-path masks-shift/clear sites in `heap_core_free.rs`
//!    (`dealloc_own_thread_with_base`'s two push arms, lines ~738 and
//!    ~792/812) and `heap_core_dealloc_batch.rs` (line ~350) -- all
//!    unconditional, feature-gated `u16` bit ops, never a branch on content.
//! 3. **`alloc_zeroed`, recycled scenario** -- NOT re-measured fresh here.
//!    R31-0 §3.2 ALREADY measured this exact arm (recycled `alloc_zeroed`,
//!    same 4 sizes x 3 touches, same `HeapCore` layer, same REPS/BURST) and
//!    found "no material, consistent regression" (small, sign-inconsistent
//!    deltas across all 12 cells). This report cites that existing result
//!    rather than duplicating it -- re-running the identical arm would not
//!    add evidence, only noise. See this report's own §3 for the citation
//!    and why it still counts as "the same regime" for this report's
//!    purposes.
//! 4. **`realloc`** -- source-confirmed (`heap_core_free.rs`'s `realloc`,
//!    lines ~911-1000+) that `realloc` contains ZERO direct
//!    `#[cfg(feature = "virgin-zero-skip")]` code of its own. Its move leg
//!    is explicitly documented as funnelling through `HeapCore::alloc`/
//!    `dealloc` ("MUST-1" doc comment, `heap_core_free.rs:926`), so any cost
//!    `realloc` pays is 100% inherited from arms 1-2 above, not a separate
//!    code path. This gate includes ONE confirmatory wall-clock cell (not a
//!    full sweep) per the task brief's own permission ("if it doesn't [touch
//!    realloc], say so and skip this arm rather than inventing one").
//!
//! ## A known confound this gate does NOT reproduce (filed separately, task #495)
//!
//! `alloc_small_zeroed_via_magazine`'s magazine-HIT arm
//! (`heap_core_alloc.rs:373`) pays an extra `self.stamp_segment_owner(issued)`
//! call that plain `alloc`'s equivalent hit arm explicitly documents it does
//! NOT pay ("P4: NO stamp here", `heap_core_alloc.rs:~160`). This gate's
//! plain-`alloc` arms therefore measure a CHEAPER baseline than
//! `alloc_zeroed`'s hit arm on that one dimension, for a reason unrelated to
//! `virgin-zero-skip` itself (the asymmetry exists on BOTH the OFF and ON
//! binary of THIS gate, and existed before `virgin-zero-skip` was invented).
//! This is a caveat on how to READ this report's numbers, not a defect in
//! this gate's design -- flagged explicitly so a reader does not mistake
//! "the cost measured today" for "the cost inherent to `virgin-zero-skip`
//! itself." Filed as task #495, out of scope for this task to fix.
//!
//! ## Path-activation oracle (CLAUDE.md R30-8)
//!
//! Plain `alloc` never zeroes, so `dbg_small_zero_pass_count` cannot serve
//! as its activation oracle (unlike R31-0's `alloc_zeroed` arms). Instead:
//!   1. **`HeapCore::tcache_hits()` before/after** -- same magazine-hit-count
//!      proof R31-0 used, confirming the production magazine path actually
//!      ran (not bypassed).
//!   2. **`HeapCore::dbg_tcache_virgin_mask(c)` sampled immediately after the
//!      FIRST burst call of each virgin rep.** Source-confirmed (not
//!      assumed): plain `alloc`'s OWN magazine-miss path
//!      (`refill_magazine_slow`, `heap_core_alloc.rs:665`) never writes
//!      `virgin_mask` at all -- only `alloc_zeroed`'s sibling miss path
//!      (`refill_magazine_slow_virgin`) sets bits. So on an `alloc`-only
//!      workload the mask for this class must read EXACTLY 0 after every
//!      refill, proving the per-hit AND this gate measures
//!      (`heap_core_alloc.rs:218-220`) is unconditionally clearing an
//!      ALREADY-ZERO bit here -- a real, but always-a-no-op, per-hit cost on
//!      a plain-`alloc` workload specifically (never a benefit; the mask is
//!      never populated on this path to begin with). `NA` on the OFF binary
//!      (the field does not exist there at all -- compiled out, not merely
//!      unread).
//!
//! ## Run
//!
//! ```text
//! cargo bench --bench r32_0_virgin_zero_skip_cost_side_gate --features "production alloc-stats"
//! cargo bench --bench r32_0_virgin_zero_skip_cost_side_gate --features "production alloc-stats virgin-zero-skip"
//! ```

#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "alloc-decommit",
    feature = "alloc-stats"
))]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::alloc::Layout;
use std::time::{Duration, Instant};

use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

/// Same sweep as R31-0 (same regime): all Small-classified under plain
/// `production` (`SMALL_MAX` ~253 KiB).
const SIZES: &[(&str, usize)] = &[
    ("4k", 4096),
    ("16k", 16384),
    ("64k", 65536),
    ("128k", 131072),
];

/// Same burst size as R31-0.
const BURST: usize = 16;

/// Same rep count as R31-0 ("short scenario by default").
const REPS: usize = 15;

fn claim_fresh() -> &'static mut HeapCore {
    let p = HeapRegistry::claim();
    assert!(
        !p.is_null(),
        "HeapRegistry::claim returned null (registry exhaustion?)"
    );
    // SAFETY: `p` was just returned by `claim`; this thread owns it for the
    // rep. The slot lives in the `'static` registry array, so the exclusive
    // borrow is `'static`. Each rep claims a DISTINCT slot, so no aliasing.
    unsafe { &mut *p }
}

fn ns_per_op(elapsed: Duration, n: usize) -> f64 {
    elapsed.as_secs_f64() * 1e9 / n as f64
}

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * pct / 100.0).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn ceil_div(a: usize, b: usize) -> usize {
    a.div_ceil(b)
}

struct RepResult {
    elapsed: Duration,
    hits_delta: u64,
    refill_n: usize,
    /// Sampled virgin mask right after the FIRST burst call (virgin scenario
    /// only; 0 for recycled). `None` when the feature is off (field absent).
    first_call_mask: Option<u16>,
}

/// Plain-`alloc`, "virgin" scenario: fresh heap (empty magazine, empty free
/// list), same-class burst of `BURST` plain `alloc` calls -- first call is a
/// carve-miss (refills + retains virgin blocks under ON), remaining calls are
/// magazine hits popping those retained-virgin blocks. No zeroing is ever
/// requested; this isolates the per-hit virgin-mask-maintenance AND alone.
fn rep_alloc_virgin(layout: Layout, size: usize) -> RepResult {
    let heap = claim_fresh();
    let c = heap
        .dbg_class_for(layout)
        .expect("swept size must be Small-classified");
    let refill_n = heap.dbg_refill_n_for_class(c);
    assert_eq!(
        heap.dbg_tcache_count(c),
        0,
        "fresh heap: class {c} magazine must be empty"
    );

    let hits_before = heap.tcache_hits();

    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(BURST);
    #[cfg_attr(not(feature = "virgin-zero-skip"), allow(unused_mut))]
    let mut first_call_mask: Option<u16> = None;
    let t0 = Instant::now();
    for i in 0..BURST {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "virgin alloc returned null");
        std::hint::black_box(p);
        ptrs.push(p);
        if i == 0 {
            #[cfg(feature = "virgin-zero-skip")]
            {
                first_call_mask = Some(heap.dbg_tcache_virgin_mask(c));
            }
            let _ = size; // size only used for the alloc_zeroed sibling arm
        }
    }
    let elapsed = t0.elapsed();

    let hits_delta = heap.tcache_hits().saturating_sub(hits_before);

    for p in ptrs {
        // SAFETY: `p` was returned by `heap.alloc(layout)` above and is
        // freed exactly once, through the same heap, before the rep ends.
        unsafe { heap.dealloc(p, layout) };
    }

    RepResult {
        elapsed,
        hits_delta,
        refill_n,
        first_call_mask,
    }
}

/// Plain-`alloc`, "recycled" scenario: fresh heap, prime + dirty ONE block
/// (untimed), then a tight LIFO `alloc`+`dealloc` burst re-serving that same
/// physical block every time -- exercises BOTH the hit-path AND (on pop) and
/// the free-path mask-clear sites (on push) every iteration.
fn rep_alloc_recycled(layout: Layout, size: usize) -> RepResult {
    let heap = claim_fresh();
    let c = heap
        .dbg_class_for(layout)
        .expect("swept size must be Small-classified");
    let refill_n = heap.dbg_refill_n_for_class(c);

    let prime = heap.alloc(layout);
    assert!(!prime.is_null(), "recycled prime alloc returned null");
    // SAFETY: `prime` is non-null and valid for `size` bytes (the layout used).
    unsafe { core::ptr::write_bytes(prime, 0xAA, size) };
    // SAFETY: `prime` was returned by `heap.alloc(layout)` above, freed once.
    unsafe { heap.dealloc(prime, layout) };

    let hits_before = heap.tcache_hits();

    let t0 = Instant::now();
    for _ in 0..BURST {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "recycled alloc returned null");
        std::hint::black_box(p);
        // SAFETY: `p` was returned by the `alloc` call immediately above,
        // freed exactly once per loop iteration (LIFO re-serving prime).
        unsafe { heap.dealloc(p, layout) };
    }
    let elapsed = t0.elapsed();

    let hits_delta = heap.tcache_hits().saturating_sub(hits_before);

    RepResult {
        elapsed,
        hits_delta,
        refill_n,
        first_call_mask: None,
    }
}

fn expected_hits_alloc(scenario: &str, burst: usize, refill_n: usize) -> usize {
    match scenario {
        "virgin" => burst - ceil_div(burst, refill_n),
        "recycled" => burst,
        _ => 0,
    }
}

#[allow(clippy::too_many_arguments)]
fn report_cell(
    scenario: &str,
    size_label: &str,
    ns: &mut [f64],
    hits: &[u64],
    refill_n: usize,
    first_call_masks: &[Option<u16>],
) {
    let reps = ns.len();
    ns.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean_ns: f64 = ns.iter().sum::<f64>() / reps as f64;
    let p50_ns = percentile(ns, 50.0);
    let min_ns = ns.first().copied().unwrap_or(0.0);
    let max_ns = ns.last().copied().unwrap_or(0.0);

    let mean_hits: f64 = hits.iter().sum::<u64>() as f64 / reps as f64;
    let min_hits = hits.iter().min().copied().unwrap_or(0);
    let exp_hits = expected_hits_alloc(scenario, BURST, refill_n);

    // Retention-mask oracle: plain `alloc`'s OWN miss path
    // (`refill_magazine_slow`, `heap_core_alloc.rs:665`) never writes
    // `PerClass::virgin_mask` -- ONLY `alloc_zeroed`'s sibling miss path
    // (`refill_magazine_slow_virgin`) sets bits. Confirmed in source before
    // writing this oracle (not assumed): `refill_magazine_slow`'s body has
    // no `virgin_mask` reference at all. So on a workload built ENTIRELY
    // from plain `alloc` (this arm), the mask for this class must read
    // EXACTLY 0 after every miss-triggered refill, regardless of refill_n --
    // proving the per-hit AND this gate measures (`heap_core_alloc.rs:218-220`)
    // is unconditionally clearing an already-zero bit here: a real, but
    // always-a-no-op, per-hit cost on an `alloc`-only workload. `NA` when
    // the feature is off (field does not exist) or scenario is recycled
    // (not sampled).
    let on = cfg!(feature = "virgin-zero-skip");
    let mask_oracle = if !on || scenario != "virgin" {
        "NA".to_string()
    } else {
        let all_zero = first_call_masks.iter().all(|m| matches!(m, Some(0)));
        if all_zero {
            "PASS".to_string()
        } else {
            "FAIL".to_string()
        }
    };

    println!(
        "R32_0_ALLOC_{scenario},{size_label},{reps},{refill_n},{mean_hits:.2},{min_hits},{exp_hits},{mean_ns:.1},{p50_ns:.1},{min_ns:.1},{max_ns:.1},{mask_oracle}"
    );
}

fn run_alloc_virgin() {
    for &(label, size) in SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();
        let mut ns: Vec<f64> = Vec::with_capacity(REPS);
        let mut hits: Vec<u64> = Vec::with_capacity(REPS);
        let mut masks: Vec<Option<u16>> = Vec::with_capacity(REPS);
        let mut refill_n = 0usize;
        for _ in 0..REPS {
            let r = rep_alloc_virgin(layout, size);
            ns.push(ns_per_op(r.elapsed, BURST));
            hits.push(r.hits_delta);
            masks.push(r.first_call_mask);
            refill_n = r.refill_n;
        }
        report_cell("virgin", label, &mut ns, &hits, refill_n, &masks);
    }
}

fn run_alloc_recycled() {
    for &(label, size) in SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();
        let mut ns: Vec<f64> = Vec::with_capacity(REPS);
        let mut hits: Vec<u64> = Vec::with_capacity(REPS);
        let mut refill_n = 0usize;
        for _ in 0..REPS {
            let r = rep_alloc_recycled(layout, size);
            ns.push(ns_per_op(r.elapsed, BURST));
            hits.push(r.hits_delta);
            refill_n = r.refill_n;
        }
        report_cell("recycled", label, &mut ns, &hits, refill_n, &[]);
    }
}

/// One confirmatory `realloc` cell (grow, same-class-to-same-class Large
/// promotion avoided by staying inside Small): NOT a full sweep, per the
/// task brief's own permission -- `realloc` has zero direct
/// `virgin-zero-skip` code, so this is a sanity check that the inherited
/// cost from `alloc`/`dealloc` is not somehow amplified by the realloc
/// wrapper itself, not a new isolated mechanism.
fn run_realloc_confirmation() {
    const OLD_SIZE: usize = 4096;
    const NEW_SIZE: usize = 8192;
    const REALLOC_REPS: usize = 200;
    let old_layout = Layout::from_size_align(OLD_SIZE, 8).unwrap();

    let heap = claim_fresh();
    let mut ns: Vec<f64> = Vec::with_capacity(REALLOC_REPS);
    for _ in 0..REALLOC_REPS {
        let p = heap.alloc(old_layout);
        assert!(!p.is_null(), "realloc-confirmation alloc returned null");
        let t0 = Instant::now();
        // SAFETY: `p` was returned by `heap.alloc(old_layout)` immediately
        // above, matches `old_layout` exactly, and is used/re-freed exactly
        // once via the returned pointer.
        let p2 = unsafe { heap.realloc(p, old_layout, NEW_SIZE) };
        let elapsed = t0.elapsed();
        assert!(!p2.is_null(), "realloc-confirmation realloc returned null");
        ns.push(elapsed.as_secs_f64() * 1e9);
        let new_layout = Layout::from_size_align(NEW_SIZE, 8).unwrap();
        // SAFETY: `p2` was returned by the `realloc` call above with
        // `new_layout`, freed exactly once.
        unsafe { heap.dealloc(p2, new_layout) };
    }
    ns.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean_ns: f64 = ns.iter().sum::<f64>() / ns.len() as f64;
    let p50_ns = percentile(&ns, 50.0);
    println!("R32_0_REALLOC,4k_to_8k,{REALLOC_REPS},{mean_ns:.1},{p50_ns:.1}");
}

fn main() {
    let _ = bootstrap::ensure();
    println!("# R32-0 virgin-zero-skip COST-SIDE (LOST-front) gate");
    println!("# CSV rows: R32_0_ALLOC_{{virgin|recycled}},size,reps,refill_n,mean_hits,min_hits,expected_hits,mean_ns,p50_ns,min_ns,max_ns,mask_oracle");
    println!("# CSV rows: R32_0_REALLOC,label,reps,mean_ns,p50_ns");
    println!(
        "# cfg: virgin-zero-skip={} BURST={BURST} REPS={REPS}",
        cfg!(feature = "virgin-zero-skip"),
    );

    println!("# --- plain alloc, virgin scenario (same-class burst on fresh heap) ---");
    run_alloc_virgin();
    println!("# --- plain alloc, recycled scenario (primed dirty block, tight LIFO loop) ---");
    run_alloc_recycled();
    println!("# --- realloc confirmation (one representative grow, 4k->8k) ---");
    run_realloc_confirmation();
}
