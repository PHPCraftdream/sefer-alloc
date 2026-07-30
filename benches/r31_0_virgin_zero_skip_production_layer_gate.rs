//! R31-0 (task #471) -- the PRODUCTION-LAYER wall-clock + mechanism-activation
//! gate for `virgin-zero-skip`, CORRECTING R30-3's (task #452) NO-GO verdict.
//!
//! ## The defect this gate fixes (CONFIRMED in source before any measurement)
//!
//! R30-3's judge (`benches/r30_3_virgin_zero_skip_native_gate.rs`) constructs a
//! BARE `AllocCore` (`AllocCore::new()`, its line 278) and calls
//! `core.alloc_zeroed(layout)` directly. That path is the magazine-BYPASS
//! substrate: a bump-carve goes through `carve_block_with_refill`
//! (`src/alloc_core/alloc_core_small.rs:346`), which carves the caller's block
//! AND proactively refills `REFILL_BATCH = 31` more blocks onto the FREE LIST.
//! Every subsequent same-class `alloc_zeroed` then pops a free-list block, and
//! a free-list pop is `is_virgin = false` by `alloc_small_with_virgin`'s
//! dispatch conjunct -- so virgin-path activation is structurally capped at
//! ~1-in-32. R30-3 measured exactly that (~3% at BATCH=16) and concluded the
//! feature is structurally useless for any same-class burst.
//!
//! That conclusion is TRUE for bare `AllocCore` and FALSE for the actual
//! `production + virgin-zero-skip` configuration. The PRODUCTION call chain is:
//!   `HeapCore::alloc_zeroed` (`src/registry/heap_core_alloc.rs:524`)
//!     -> `alloc_small_zeroed_via_magazine` (`:337`)
//!       -> on a magazine MISS: `refill_magazine_slow_virgin` (`:413`)
//!         -> `AllocCore::refill_class_bump_virgin_checked`
//! This refill carves `refill_n_for_class(block_size)` blocks (clamped to
//! `TCACHE_CAP = 16` by a 64 KiB byte budget), issues ONE to the caller, and
//! STORES the retained `refill_n-1` blocks' virgin bits into
//! `PerClass::virgin_mask` (`heap_core_alloc.rs:472`). Later magazine HITS read
//! and clear their bit (`:354-356`) and STILL skip the explicit zero pass --
//! i.e. virginity is preserved across an ENTIRE freshly-carved refill, not
//! just the first block. `tests/r13_3_magazine_virgin_hit_skips_zero.rs`
//! already asserts this exact mechanism; this gate measures it on the wall
//! clock.
//!
//! ## Why this judge reaches a different answer
//!
//! Same-class BURSTS of `BURST = 16` `alloc_zeroed` calls on a FRESH
//! `HeapCore` (claimed per rep via `HeapRegistry::claim`, never recycled -- a
//! fresh slot gives a fresh, empty magazine + empty free list every rep):
//!   - 4 KiB  (refill_n = 16): 1 carve-miss + 15 retained-virgin HITS, all skip.
//!   - 16 KiB (refill_n = 4):  4 carve-misses + 12 retained-virgin HITS, all skip.
//!   - 64/128 KiB (refill_n = 1): 16 carve-misses, 0 retained -- every call
//!     carves+issues its OWN single virgin block (still 100% skip, but with no
//!     multi-block retention benefit; the report explains this size-dependent
//!     shape).
//!
//! So the production magazine path achieves ~100% virgin activation on ANY
//! same-class burst -- the OPPOSITE of R30-3's ~3% ceiling.
//!
//! ## Path-activation oracle (CLAUDE.md R30-8)
//!
//! Per (size, touch) arm, these mechanism signals:
//!   1. `HeapCore::tcache_hits()` before/after the burst = magazine-HIT count.
//!      Equals `BURST - ceil(BURST/refill_n)` on BOTH OFF and ON (the magazine
//!      retains blocks regardless of the feature flag) -- the cross-binary
//!      proof that the production magazine refill+retain path RAN, the exact
//!      layer R30-3's bare-`AllocCore` gate excluded.
//!   2. `AllocCore::dbg_small_zero_pass_count()` (`alloc-stats`) before/after =
//!      explicit-zero calls. Virgin scenario: ~0 on ON (every block virgin ->
//!      skip); vacuous on OFF (the counter lives inside the `virgin-zero-skip`
//!      branch and never moves with the feature off -- same limitation R30-3's
//!      report section 4 documented). Recycled scenario: ~BURST on ON.
//!   3. A separate RETENTION PROBE (ON only, `dbg_tcache_virgin_mask`): after
//!      ONE `alloc_zeroed` on a fresh heap, asserts the magazine holds
//!      `refill_n-1` retained blocks whose `virgin_mask` bits are ALL set --
//!      the smoking-gun proof the magazine retains virginity across a refill.
//!
//! ## Run
//!
//! ```text
//! # OFF (baseline) and ON (treatment) are the SAME source compiled two ways:
//! cargo bench --bench r31_0_virgin_zero_skip_production_layer_gate --features "production alloc-stats"
//! cargo bench --bench r31_0_virgin_zero_skip_production_layer_gate --features "production alloc-stats virgin-zero-skip"
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

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

/// Sweep sizes: 4, 16, 64, 128 KiB. All are Small-classified (`SMALL_MAX` is
/// ~253 KiB under plain `production`), so every arm exercises the
/// `virgin-zero-skip`-gated Small `alloc_zeroed` magazine path, not the
/// separately-gated Large freshness skip.
const SIZES: &[(&str, usize)] = &[
    ("4k", 4096),
    ("16k", 16384),
    ("64k", 65536),
    ("128k", 131072),
];

/// Blocks per timed same-class burst. 16 divides every swept size's refill_n
/// (16/4/1/1) exactly, giving a clean miss/hit split. This is the SAME-CLASS
/// BURST the production magazine layer is designed to serve virgin -- the
/// exact workload shape R30-3's bare-`AllocCore` gate could not represent
/// (its free-list refill always marked retained blocks non-virgin).
const BURST: usize = 16;

/// Independent fresh-heap samples per (size, touch, scenario) cell. Each rep
/// claims a FRESH `HeapCore` (empty magazine + empty free list), so these are
/// independent samples. Each rep's timed region is a 16-call burst (far more
/// work than R30-3's single-call virgin rep), so 15 reps give a stable
/// mean/percentile under CLAUDE.md's "short scenario by default" profile.
const REPS: usize = 15;

/// Minimum fraction of a burst's calls that must take the intended path for a
/// cell to be trusted (R30-8 path-activation oracle). Virgin cell: at least
/// 95% of calls must SKIP zeroing (explicit-zero delta <= 5% of BURST).
/// Recycled cell: at least 95% must explicit-zero. Applied only on the ON
/// binary -- on OFF the `SMALL_ZERO_PASS_CALLS` counter never moves, so the
/// oracle is structurally vacuous there and reported as "NA".
const MIN_ACTIVATION_PCT: f64 = 95.0;

/// Consumer behavior applied to each freshly `alloc_zeroed`-returned block
/// before it is retained for the rest of the timed burst -- the same three
/// categories R30-3's judge used.
#[derive(Clone, Copy)]
enum Touch {
    /// Return the pointer without reading or writing any byte.
    None,
    /// Read exactly one byte per 4 KiB page -- probes whether pages are
    /// actually committed/faulted without paying a full-payload bandwidth cost.
    OneBytePerPage,
    /// Read then write every byte of the allocation.
    Full,
}

impl Touch {
    fn label(self) -> &'static str {
        match self {
            Touch::None => "notouch",
            Touch::OneBytePerPage => "onebyte",
            Touch::Full => "full",
        }
    }

    /// Apply this behavior to a freshly allocated `size`-byte block.
    /// SAFETY (caller contract): `ptr` is non-null, valid for `size` bytes,
    /// and was returned by `alloc_zeroed` with a layout of exactly `size`.
    #[inline]
    unsafe fn apply(self, ptr: *mut u8, size: usize) {
        const PAGE: usize = 4096;
        match self {
            Touch::None => {}
            Touch::OneBytePerPage => {
                let mut off = 0usize;
                let mut acc: u8 = 0;
                while off < size {
                    acc ^= core::ptr::read_volatile(ptr.add(off));
                    off += PAGE;
                }
                std::hint::black_box(acc);
            }
            Touch::Full => {
                for off in 0..size {
                    let p = ptr.add(off);
                    let v = core::ptr::read_volatile(p);
                    core::ptr::write_volatile(p, v.wrapping_add(1));
                }
            }
        }
    }
}

const TOUCHES: &[Touch] = &[Touch::None, Touch::OneBytePerPage, Touch::Full];

struct RepResult {
    elapsed: Duration,
    hits_delta: u64,
    zp_delta: u64,
    refill_n: usize,
}

/// Claim a FRESH heap slot (empty magazine + empty free list) for one rep.
/// Never recycled: a fresh slot is claimed every rep (recycling would hand the
/// next rep the SAME slot's polluted free list; `MAX_HEAPS = 4096` is far
/// larger than the ~360 fresh-heap total this binary uses, so no exhaustion).
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

/// "Virgin" scenario, one rep: fresh heap (untimed), then a `BURST`-call
/// same-class `alloc_zeroed` burst on a class that has NEVER been served --
/// a genuine carve + retained-virgin magazine hits. Consumer `touch` is applied
/// inside the timed region; freeing happens AFTER the timer stops.
fn rep_virgin(layout: Layout, size: usize, touch: Touch) -> RepResult {
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
    let zp_before = AllocCore::dbg_small_zero_pass_count();

    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(BURST);
    let t0 = Instant::now();
    for _ in 0..BURST {
        let p = heap.alloc_zeroed(layout);
        assert!(!p.is_null(), "virgin alloc_zeroed returned null");
        // SAFETY: `p` is non-null, valid for `size` bytes (the layout just used).
        unsafe { touch.apply(p, size) };
        ptrs.push(p);
    }
    let elapsed = t0.elapsed();

    let hits_delta = heap.tcache_hits().saturating_sub(hits_before);
    let zp_delta = AllocCore::dbg_small_zero_pass_count().saturating_sub(zp_before);

    // UNTIMED teardown: free every block through the same heap that produced
    // it, then leave the slot LIVE (never recycled -- see `claim_fresh`).
    for p in ptrs {
        // SAFETY: `p` was returned by `heap.alloc_zeroed(layout)` above and is
        // freed exactly once, through the same heap, before the rep ends.
        unsafe { heap.dealloc(p, layout) };
    }

    RepResult {
        elapsed,
        hits_delta,
        zp_delta,
        refill_n,
    }
}

/// "Recycled" scenario, one rep: fresh heap, prime + dirty ONE block of this
/// class (untimed) so a non-virgin block is available, then a `BURST`-call
/// tight LIFO loop of `alloc_zeroed`+`dealloc` that re-serves that same dirty
/// block -- every pop is non-virgin and MUST run the explicit zero pass
/// regardless of the feature flag.
fn rep_recycled(layout: Layout, size: usize, touch: Touch) -> RepResult {
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
    let zp_before = AllocCore::dbg_small_zero_pass_count();

    let t0 = Instant::now();
    for _ in 0..BURST {
        let p = heap.alloc_zeroed(layout);
        assert!(!p.is_null(), "recycled alloc_zeroed returned null");
        // SAFETY: `p` is non-null, valid for `size` bytes, just returned.
        unsafe { touch.apply(p, size) };
        // SAFETY: `p` was returned by the `alloc_zeroed` call immediately above,
        // freed exactly once per loop iteration (LIFO re-serving prime).
        unsafe { heap.dealloc(p, layout) };
    }
    let elapsed = t0.elapsed();

    let hits_delta = heap.tcache_hits().saturating_sub(hits_before);
    let zp_delta = AllocCore::dbg_small_zero_pass_count().saturating_sub(zp_before);

    RepResult {
        elapsed,
        hits_delta,
        zp_delta,
        refill_n,
    }
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

fn expected_hits(scenario: &str, burst: usize, refill_n: usize) -> usize {
    match scenario {
        "virgin" => burst - ceil_div(burst, refill_n),
        "recycled" => burst,
        _ => 0,
    }
}

fn run_virgin() {
    for &(label, size) in SIZES {
        for &touch in TOUCHES {
            let layout = Layout::from_size_align(size, 8).unwrap();
            let mut ns: Vec<f64> = Vec::with_capacity(REPS);
            let mut hits: Vec<u64> = Vec::with_capacity(REPS);
            let mut zp: Vec<u64> = Vec::with_capacity(REPS);
            let mut refill_n = 0usize;
            for _ in 0..REPS {
                let r = rep_virgin(layout, size, touch);
                ns.push(ns_per_op(r.elapsed, BURST));
                hits.push(r.hits_delta);
                zp.push(r.zp_delta);
                refill_n = r.refill_n;
            }
            // virgin activation = fraction that SKIPPED zeroing.
            let act: Vec<f64> = zp
                .iter()
                .map(|&z| 100.0 * (BURST as f64 - z as f64) / BURST as f64)
                .collect();
            report_cell(
                "virgin",
                label,
                touch.label(),
                &mut ns,
                &hits,
                &zp,
                &act,
                refill_n,
            );
        }
    }
}

fn run_recycled() {
    for &(label, size) in SIZES {
        for &touch in TOUCHES {
            let layout = Layout::from_size_align(size, 8).unwrap();
            let mut ns: Vec<f64> = Vec::with_capacity(REPS);
            let mut hits: Vec<u64> = Vec::with_capacity(REPS);
            let mut zp: Vec<u64> = Vec::with_capacity(REPS);
            let mut refill_n = 0usize;
            for _ in 0..REPS {
                let r = rep_recycled(layout, size, touch);
                ns.push(ns_per_op(r.elapsed, BURST));
                hits.push(r.hits_delta);
                zp.push(r.zp_delta);
                refill_n = r.refill_n;
            }
            // recycled activation = fraction that EXPLICIT-zeroed.
            let act: Vec<f64> = zp
                .iter()
                .map(|&z| 100.0 * z as f64 / BURST as f64)
                .collect();
            report_cell(
                "recycled",
                label,
                touch.label(),
                &mut ns,
                &hits,
                &zp,
                &act,
                refill_n,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn report_cell(
    scenario: &str,
    size_label: &str,
    touch_label: &str,
    ns: &mut [f64],
    hits: &[u64],
    zp: &[u64],
    act: &[f64],
    refill_n: usize,
) {
    let reps = ns.len();
    ns.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean_ns: f64 = ns.iter().sum::<f64>() / reps as f64;
    let p50_ns = percentile(ns, 50.0);
    let min_ns = ns.first().copied().unwrap_or(0.0);
    let max_ns = ns.last().copied().unwrap_or(0.0);

    let mean_hits: f64 = hits.iter().sum::<u64>() as f64 / reps as f64;
    let min_hits = hits.iter().min().copied().unwrap_or(0);
    let mean_zp: f64 = zp.iter().sum::<u64>() as f64 / reps as f64;
    let min_zp = zp.iter().min().copied().unwrap_or(0);

    let mean_act: f64 = act.iter().sum::<f64>() / reps as f64;
    let min_act = act.iter().cloned().fold(f64::INFINITY, f64::min);

    let exp_hits = expected_hits(scenario, BURST, refill_n);
    let on = cfg!(feature = "virgin-zero-skip");
    let oracle = if on {
        if min_act >= MIN_ACTIVATION_PCT {
            "PASS"
        } else {
            "FAIL"
        }
    } else {
        "NA"
    };

    println!(
        "R31_0_{scenario},{size_label},{touch_label},{reps},{refill_n},{mean_hits:.2},{min_hits},{exp_hits},{mean_zp:.2},{min_zp},{mean_ns:.1},{p50_ns:.1},{min_ns:.1},{max_ns:.1},{mean_act:.2},{min_act:.2},{oracle}"
    );
}

#[cfg(feature = "virgin-zero-skip")]
fn run_retention_probes() {
    for &(label, size) in SIZES {
        retention_probe(label, size);
    }
}

/// ON-binary-only smoking-gun probe: after ONE `alloc_zeroed` on a fresh heap,
/// the magazine must hold `refill_n-1` retained blocks whose `virgin_mask` bits
/// are ALL set -- direct proof the magazine RETAINS virginity across a freshly
/// carved refill (R13-3), the mechanism R30-3's bare-`AllocCore` path lacked.
#[cfg(feature = "virgin-zero-skip")]
fn retention_probe(size_label: &str, size: usize) {
    let heap = claim_fresh();
    let layout = Layout::from_size_align(size, 8).unwrap();
    let c = heap
        .dbg_class_for(layout)
        .expect("swept size must be Small-classified");
    let refill_n = heap.dbg_refill_n_for_class(c);
    assert_eq!(
        heap.dbg_tcache_count(c),
        0,
        "fresh heap: class {c} magazine must be empty"
    );

    let p = heap.alloc_zeroed(layout);
    assert!(!p.is_null(), "retention-probe alloc_zeroed returned null");

    let retained = heap.dbg_tcache_count(c);
    let mask = heap.dbg_tcache_virgin_mask(c);
    let expected_retained = refill_n.saturating_sub(1);
    let expected_mask: u16 = match expected_retained {
        0 => 0,
        n if n >= 16 => u16::MAX,
        n => (1u16 << n) - 1,
    };
    let retention_ok = retained as usize == expected_retained && mask == expected_mask;

    // Correctness check: the issued block is all-zero (OS-fresh, skip is safe).
    let bytes = unsafe { core::slice::from_raw_parts(p, size) };
    let all_zero = bytes.iter().all(|&b| b == 0);

    // SAFETY: `p` was returned by `heap.alloc_zeroed(layout)` above, freed once.
    unsafe { heap.dealloc(p, layout) };

    println!(
        "R31_0_RETENTION,{size_label},{refill_n},{retained},{expected_retained},{mask},{expected_mask},{all_zero},{}",
        if retention_ok { "PASS" } else { "FAIL" }
    );
}

fn main() {
    let _ = bootstrap::ensure();
    println!("# R31-0 virgin-zero-skip PRODUCTION-LAYER gate");
    println!("# CSV rows: R31_0_{{VIRGIN|RECYCLED}},size,touch,reps,refill_n,mean_hits,min_hits,expected_hits,mean_zp,min_zp,mean_ns,p50_ns,min_ns,max_ns,mean_act_pct,min_act_pct,oracle");
    println!("# CSV rows: R31_0_RETENTION,size,refill_n,retained,expected_retained,mask,expected_mask,all_zero,oracle");
    println!(
        "# cfg: virgin-zero-skip={} BURST={BURST} REPS={REPS} MIN_ACTIVATION_PCT={MIN_ACTIVATION_PCT}",
        cfg!(feature = "virgin-zero-skip"),
    );

    #[cfg(feature = "virgin-zero-skip")]
    {
        println!("# --- retention probes (magazine retains virginity across a refill) ---");
        run_retention_probes();
    }

    println!("# --- virgin scenario (same-class burst on fresh heap) ---");
    run_virgin();
    println!("# --- recycled scenario (primed dirty block, tight LIFO loop) ---");
    run_recycled();
}
