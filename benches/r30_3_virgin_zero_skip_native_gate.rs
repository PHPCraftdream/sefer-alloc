//! R30-3 (task #452) -- the activation-proven native wall-clock judge for
//! `virgin-zero-skip`, superseding the CONFIRMED-BROKEN
//! `benches/r29_16_virgin_zero_skip_calloc_wallclock.rs` bench.
//!
//! ## Why the old bench was replaced, not patched
//!
//! `r29_16_virgin_zero_skip_calloc_wallclock.rs`'s `bench_virgin` allocated a
//! batch of blocks, then freed the WHOLE batch inside the SAME `b.iter()`
//! closure. Criterion calls that closure thousands of times per sample; from
//! the 2nd call onward, `alloc_small_with_virgin`'s dispatch
//! (`src/alloc_core/alloc_core_small.rs:274-297`) found a populated free list
//! (step 1, `self.pop_free(...)`) and never reached the bump-carve path (step
//! 3) where `virgin-zero-skip` actually fires. The "virgin" and "recycled"
//! arms ended up measuring nearly the same code path. This was documented
//! (not fixed) by follow-up commit `68e2019` (see
//! `docs/perf/R29_16_VIRGIN_ZERO_SKIP_CALLOC_GATE.md` §8) and confirmed again
//! by the independent `docs/reviews/2026-07-30-r29-followup-readonly-review.md`
//! §3, whose 8-point design this harness implements.
//!
//! ## Why a custom timing-loop binary, not Criterion
//!
//! The matrix here is 4 sizes x 3 consumer behaviors x 2 scenarios
//! (virgin/recycled) = 24 cells per built binary, and EVERY cell must print
//! its own path-activation-oracle percentage (point 4 of the design) next to
//! its own timing distribution -- Criterion's `iter_batched` has no
//! first-class channel back to the harness for a custom per-sample counter
//! delta alongside the timing (its `Bencher` closure return value becomes
//! the NEXT untimed-teardown input, not extra reported data). This project
//! already has an established precedent for exactly this shape --
//! `benches/heap_fanin_persistent.rs`'s module doc (see its "Harness shape:
//! custom timing loop, not Criterion" section) and
//! `benches/directory_threshold_probe.rs` both use a plain `fn main()` +
//! `Instant`-based timing loop with `eprintln!` diagnostics instead of
//! `criterion_main!`, for the same underlying reason (a matrix too wide for
//! Criterion's per-`bench_function` summary model, needing custom per-cell
//! data reported alongside the timing). This file follows that established
//! pattern: `harness = false`, `fn main()`, manual `Instant` timing +
//! `println!` CSV-shaped rows (one row per matrix cell) so the raw log IS
//! the machine-parseable evidence CLAUDE.md's summary-CSV rule asks for.
//!
//! ## The 8-point design (source: the readonly review's §3), and how each
//! point is met here
//!
//! 1. **Feature OFF vs ON in separately built, immutable binaries.** This
//!    bench is gated on `alloc-global + alloc-decommit + alloc-stats` --
//!    deliberately NOT `virgin-zero-skip` -- so the identical bench binary is
//!    built twice: once without `virgin-zero-skip` (OFF / baseline) and once
//!    with it (ON / treatment). See `docs/perf/R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md`
//!    §1 for the exact `cargo bench` invocations and their reproducible
//!    source identity.
//! 2. **A genuinely fresh, empty-free-list heap per timed batch, built in
//!    UNTIMED setup.** Deliberately uses `AllocCore::new()` directly, NOT
//!    `HeapRegistry::claim()`/`recycle()` -- `HeapRegistry`'s slot lifecycle
//!    is first-claim-wins for a slot's process lifetime (`claim`'s re-claim
//!    of an already-materialised slot reuses the SAME `HeapCore`, segments
//!    and free lists included; `recycle` only flips the slot back to
//!    `FREE`, it does not tear the segments down -- see
//!    `src/registry/heap_registry.rs:118-181` and the CLAUDE.md R26-4
//!    "config-sweep same-process-reuse" rule, the SAME hazard class one
//!    level higher, generalized from config values to free-list state).
//!    `AllocCore::new()` performs a real fresh OS primordial reservation
//!    every call and its `Drop` impl releases it -- a genuinely empty free
//!    list every iteration, by construction, not by convention.
//! 3. **Timed region allocates genuinely never-served blocks and does not
//!    free them until after the batch.** The virgin scenario's timed loop
//!    calls `alloc_zeroed` (+ the consumer-behavior touch) `VIRGIN_BATCH`
//!    times and pushes every pointer into a `Vec` that is freed AFTER
//!    `Instant::elapsed()` is read.
//! 4. **Path-activation oracle.** `AllocCore::dbg_small_zero_pass_count()`
//!    (`src/alloc_core/alloc_core_core_diag.rs:516`, requires `alloc-stats`)
//!    already counts, process-wide, every Small `alloc_zeroed` call that took
//!    the explicit-zero (non-virgin) path -- it is bumped in EXACTLY the
//!    branch `virgin-zero-skip` bypasses (`alloc_core.rs:1310-1319`,
//!    `heap_core_alloc.rs:546-584`). No new hook was needed: reading this
//!    counter before and after a batch gives `explicit_zero_calls`, so
//!    `virgin_activation_pct = 100 - 100 * explicit_zero_calls / batch_len`.
//!    A cell whose intended-path activation falls under `MIN_ACTIVATION_PCT`
//!    (95%) is printed with an explicit `oracle=FAIL` marker and excluded
//!    from the report's headline table. **This oracle caught a real design
//!    bug during development of this very file**, not just a hypothetical:
//!    see `VIRGIN_BATCH`'s doc comment for the `carve_block_with_refill`
//!    interaction it found (a first attempt at `VIRGIN_BATCH = 16` measured
//!    only 6.25% virgin-path activation and was rejected by this gate).
//! 5. **Size sweep: 4, 16, 64, 128 KiB.** `SIZES` below.
//! 6. **Three consumer behaviors.** `Touch::None` (return without touching),
//!    `Touch::OneBytePerPage` (read one byte per 4 KiB page -- probes
//!    whether pages are actually committed/faulted), `Touch::Full` (read +
//!    write every byte).
//! 7. **Cross with `small-segment-lazy-commit`.** This bench is gated so it
//!    builds identically with or without `small-segment-lazy-commit`; the
//!    report runs the full sweep under both. `numa-aware` is deliberately
//!    NOT included in either arm of this sweep -- the README's R29-15 Trap
//!    note documents that `numa-aware` silently bypasses
//!    `small-segment-lazy-commit` entirely, which would make a
//!    "lazy-commit ON" row secretly not be lazy at all; that is a different
//!    experiment than the one this task asks for (eager vs lazy commit
//!    changing WHERE the zeroing saving appears), so it is out of scope here
//!    by deliberate choice, not oversight.
//! 8. **Paired process-level sampling; native time/distribution, not an IAI
//!    ratio.** Each (size, touch, scenario) cell reports `REPS` independent
//!    per-batch elapsed-time samples (mean, p50, min, max ns/op) from real
//!    `Instant::now()` deltas -- no Ir number appears in this file or is
//!    treated as a wall-clock claim anywhere in the accompanying report.
//!
//! ## Harness shape
//!
//! Recycled scenario: `REPS = 10` independent (fresh-heap) samples per cell,
//! `RECYCLED_BATCH = 16` blocks per sample. Virgin scenario: `VIRGIN_REPS =
//! 50` independent (fresh-heap) samples per cell, `VIRGIN_BATCH = 1` block
//! per sample (forced to 1 by the `carve_block_with_refill` interaction --
//! see that const's doc; `VIRGIN_REPS` is raised to 50, vs. the recycled
//! scenario's 10, to compensate with more independent samples since each
//! virgin rep's timed region is a single call rather than a 16-call batch).
//! CLAUDE.md "Speed: short scenario by default": the full matrix (4 sizes x
//! 3 touch behaviors x (50 virgin reps + 10 recycled reps) = 1,440 fresh-heap
//! samples) completes in low single-digit seconds per binary on this
//! project's dev host. Even at the largest swept size (128 KiB) and the
//! recycled scenario's larger batch, `16 * 131072 = 2 MiB` fits comfortably
//! inside one 4 MiB primordial segment's small-payload budget, so the timed
//! region never pays a mid-batch segment-reservation cost that would
//! confound the per-call cost.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "alloc-stats"
))]

use std::alloc::Layout;
use std::time::{Duration, Instant};

use sefer_alloc::AllocCore;

/// Sweep sizes -- point 5 of the design: 4, 16, 64, 128 KiB. All four are
/// comfortably Small-classified (`SegmentLayout::SMALL_MAX` is ~253 KiB under
/// plain `production`; 128 KiB is still well under that ceiling), so every
/// arm exercises the SAME `virgin-zero-skip`-gated Small `alloc_zeroed` path
/// (`src/alloc_core/alloc_core.rs:1305-1338`), not the separately-gated Large
/// freshness skip.
const SIZES: &[(&str, usize)] = &[
    ("4k", 4096),
    ("16k", 16384),
    ("64k", 65536),
    ("128k", 131072),
];

/// Blocks per timed batch for the RECYCLED scenario -- see module doc for
/// the segment-capacity budget check at the largest swept size. NOT used by
/// the virgin scenario -- see `VIRGIN_BATCH`'s doc for why.
const RECYCLED_BATCH: usize = 16;

/// Blocks per timed "batch" for the VIRGIN scenario -- **forced to 1 by a
/// real structural property of the allocator discovered while building this
/// judge's oracle, not a bench-design choice.** `alloc_small_with_virgin`'s
/// bump-carve path (step 3, `src/alloc_core/alloc_core_small.rs:294-297`)
/// calls `carve_block_with_refill`
/// (`src/alloc_core/alloc_core_small.rs:346-376`), which carves the caller's
/// block AND ALSO proactively carves `REFILL_BATCH = 31` more blocks of the
/// SAME class and pushes every one of them onto the free list (Phase 9
/// amortisation, unconditional, not gated on `virgin-zero-skip`). Those 31
/// blocks are OS-fresh/zero-filled and have never been handed to any
/// caller, but the very next `alloc_zeroed` call of the same class pops one
/// off the free list at `alloc_small_with_virgin` step 1 -- and ANY
/// free-list pop is `is_virgin = false` by the dispatch conjunct
/// (`alloc_small_with_virgin`'s own doc,
/// `src/alloc_core/alloc_core_small.rs:255-263`), regardless of whether the
/// popped block was ever actually dirtied. A first measurement at
/// `BATCH = 16` (this file's original design) confirmed this empirically:
/// virgin-path activation measured 6.25% = 1/16 -- exactly "only the first
/// call of the batch is virgin," matching this mechanism exactly, not
/// sampling noise (see `docs/perf/_raw_r30_3_batch16_oracle_fail.log`, kept
/// as evidence of the design iteration). Any `VIRGIN_BATCH > 1` on a SINGLE
/// class/heap therefore structurally caps virgin activation at
/// `1 / (1 + 31) ≈ 3.1%` in steady state -- WELL under `MIN_ACTIVATION_PCT`
/// -- so `VIRGIN_BATCH = 1` (one genuinely virgin call per fresh `AllocCore`,
/// the same single-shot-process shape the existing `perf_gate_iai.rs` IAI
/// arms already use) is the only batch size that can pass the oracle gate
/// for this scenario. This is itself a real finding about the feature's
/// achievable hit rate on ANY same-class multi-block burst -- see the gate
/// report's promotion-decision section.
const VIRGIN_BATCH: usize = 1;

/// Independent fresh-heap repetitions per (size, touch, scenario) cell --
/// point 8 of the design ("paired process-level sampling; report native
/// time/distribution"). Each rep gets its OWN fresh `AllocCore` (point 2),
/// so these are independent samples, not sub-slices of one shared run.
const REPS: usize = 10;

/// Repetitions for the virgin scenario specifically -- higher than `REPS`
/// because `VIRGIN_BATCH = 1` means each rep's timed region is a single
/// call (much shorter than the recycled scenario's 16-call batch), so more
/// reps are needed for a stable mean/percentile under this project's fast
/// short-scenario profile (CLAUDE.md "Speed: short scenario by default").
const VIRGIN_REPS: usize = 50;

/// Minimum fraction of a batch's `alloc_zeroed` calls that must go through
/// the intended path for a cell to be trusted -- point 4 of the design
/// (reject any sample where the intended path does not dominate, e.g. under
/// 95% virgin-path calls). Applied symmetrically: a "virgin" batch needs at
/// least 95% of its calls to be genuine bump-carves (explicit-zero delta at
/// most 5% of BATCH), a "recycled" batch needs at least 95% of its calls to
/// be free-list pops (explicit-zero delta at least 95% of BATCH).
const MIN_ACTIVATION_PCT: f64 = 95.0;

/// Consumer behavior applied to each freshly `alloc_zeroed`-returned block
/// before it is retained for the rest of the timed batch -- point 6 of the
/// design. These interact differently with whether zeroing was actually
/// skipped (not touching never re-dirties/refaults) vs merely deferred
/// (touching may trigger the OS's own first-touch page-fault path
/// regardless of whether our software skipped `Node::zero`).
#[derive(Clone, Copy)]
enum Touch {
    /// Return the pointer without reading or writing any byte of it.
    None,
    /// Read exactly one byte per 4 KiB page -- probes whether pages are
    /// actually committed/faulted without paying a full-payload memory
    /// bandwidth cost.
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

/// One rep's result: elapsed time for the timed region, plus the
/// path-activation-oracle readout for that SAME batch (as a percentage of
/// `batch_len` calls -- see the module doc's point 4).
struct RepResult {
    elapsed: Duration,
    explicit_zero_pct: f64,
}

/// "Virgin" scenario, one rep: fresh `AllocCore` (untimed), then
/// `VIRGIN_BATCH` (= 1, see that const's doc for why) `alloc_zeroed` call(s)
/// on a segment that has NEVER served this class before -- a genuine
/// bump-carve. Consumer `touch` is applied inside the timed region (it is
/// part of what a real caller does with the pointer); freeing + `AllocCore`
/// teardown happens AFTER the timer stops.
fn rep_virgin(layout: Layout, size: usize, touch: Touch) -> RepResult {
    // UNTIMED setup (point 2): a genuinely fresh `AllocCore` -- real OS
    // primordial reservation, empty free lists, `payload_virgin == true` on
    // its small segment.
    let mut core = AllocCore::new().expect("primordial reservation");
    let before = AllocCore::dbg_small_zero_pass_count();

    let mut ptrs = Vec::with_capacity(VIRGIN_BATCH);
    let t0 = Instant::now();
    for _ in 0..VIRGIN_BATCH {
        let p = core.alloc_zeroed(layout);
        assert!(!p.is_null(), "virgin alloc_zeroed returned null");
        // SAFETY: `p` is non-null, valid for `size` bytes (the layout just
        // used), and was just returned by `alloc_zeroed`.
        unsafe { touch.apply(p, size) };
        ptrs.push(p);
    }
    let elapsed = t0.elapsed();

    let after = AllocCore::dbg_small_zero_pass_count();

    // UNTIMED teardown: free everything, then let `core` drop (releases the
    // OS reservation) -- both outside the timed region.
    for p in ptrs {
        // SAFETY: `p` was returned by `core.alloc_zeroed(layout)` above and
        // is freed exactly once, through the SAME `AllocCore` that produced
        // it, before `core` itself is dropped.
        unsafe { core.dealloc(p, layout) };
    }

    RepResult {
        elapsed,
        explicit_zero_pct: explicit_zero_pct(before, after, VIRGIN_BATCH),
    }
}

/// "Recycled" scenario, one rep: fresh `AllocCore`, prime + dirty ONE block
/// of this class (untimed) so the free list has a genuinely dirty entry to
/// pop on the FIRST timed call too, then `RECYCLED_BATCH`
/// `alloc_zeroed`+`dealloc` cycles of the SAME class -- each pops the
/// just-freed block back (LIFO single-block free list at this class): never
/// virgin by `alloc_small_with_virgin`'s dispatch conjunct, so `Node::zero`
/// MUST run regardless of `virgin-zero-skip`.
fn rep_recycled(layout: Layout, size: usize, touch: Touch) -> RepResult {
    let mut core = AllocCore::new().expect("primordial reservation");
    let prime = core.alloc(layout);
    assert!(!prime.is_null(), "recycled prime alloc returned null");
    // SAFETY: `prime` is non-null and valid for `size` bytes (the layout
    // just used).
    unsafe { core::ptr::write_bytes(prime, 0xAA, size) };
    // SAFETY: `prime` was returned by `core.alloc` above with the same
    // layout, freed exactly once.
    unsafe { core.dealloc(prime, layout) };

    let before = AllocCore::dbg_small_zero_pass_count();

    let t0 = Instant::now();
    for _ in 0..RECYCLED_BATCH {
        let p = core.alloc_zeroed(layout);
        assert!(!p.is_null(), "recycled alloc_zeroed returned null");
        // SAFETY: `p` is non-null, valid for `size` bytes, just returned by
        // `alloc_zeroed`.
        unsafe { touch.apply(p, size) };
        // SAFETY: `p` was returned by the `alloc_zeroed` call immediately
        // above with the same layout, freed exactly once per loop
        // iteration.
        unsafe { core.dealloc(p, layout) };
    }
    let elapsed = t0.elapsed();

    let after = AllocCore::dbg_small_zero_pass_count();

    RepResult {
        elapsed,
        explicit_zero_pct: explicit_zero_pct(before, after, RECYCLED_BATCH),
    }
}

fn explicit_zero_pct(before: u64, after: u64, batch_len: usize) -> f64 {
    let delta = after.saturating_sub(before);
    100.0 * (delta as f64) / (batch_len as f64)
}

fn ns_per_op(elapsed: Duration, batch_len: usize) -> f64 {
    elapsed.as_secs_f64() * 1e9 / (batch_len as f64)
}

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * pct / 100.0).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn main() {
    println!(
        "# R30-3 virgin-zero-skip native gate -- CSV header:\n\
         # R30_3_ROW,scenario,size,touch,reps,mean_ns_per_op,p50_ns_per_op,min_ns_per_op,max_ns_per_op,mean_activation_pct,min_activation_pct,oracle"
    );
    println!(
        "# cfg: virgin-zero-skip={} small-segment-lazy-commit={} VIRGIN_BATCH={VIRGIN_BATCH} \
         RECYCLED_BATCH={RECYCLED_BATCH} VIRGIN_REPS={VIRGIN_REPS} REPS={REPS} \
         MIN_ACTIVATION_PCT={MIN_ACTIVATION_PCT}",
        cfg!(feature = "virgin-zero-skip"),
        cfg!(feature = "small-segment-lazy-commit"),
    );

    // Each scenario prints its OWN intended-path activation pct directly
    // (virgin: 100 - explicit_zero_pct; recycled: explicit_zero_pct as-is)
    // so `min_activation_pct >= MIN_ACTIVATION_PCT` always means "the
    // intended path dominated this cell's batches", symmetrically for both
    // scenarios.
    run_virgin();
    run_recycled();
}

fn run_virgin() {
    for &(size_label, size) in SIZES {
        for &touch in &[Touch::None, Touch::OneBytePerPage, Touch::Full] {
            let layout = Layout::from_size_align(size, 8).expect("valid layout");

            let mut ns: Vec<f64> = Vec::with_capacity(VIRGIN_REPS);
            let mut virgin_pct: Vec<f64> = Vec::with_capacity(VIRGIN_REPS);
            for _ in 0..VIRGIN_REPS {
                let r = rep_virgin(layout, size, touch);
                ns.push(ns_per_op(r.elapsed, VIRGIN_BATCH));
                virgin_pct.push(100.0 - r.explicit_zero_pct);
            }

            report_row("virgin", size_label, touch.label(), &mut ns, &virgin_pct);
        }
    }
}

fn run_recycled() {
    for &(size_label, size) in SIZES {
        for &touch in &[Touch::None, Touch::OneBytePerPage, Touch::Full] {
            let layout = Layout::from_size_align(size, 8).expect("valid layout");

            let mut ns: Vec<f64> = Vec::with_capacity(REPS);
            let mut recycled_pct: Vec<f64> = Vec::with_capacity(REPS);
            for _ in 0..REPS {
                let r = rep_recycled(layout, size, touch);
                ns.push(ns_per_op(r.elapsed, RECYCLED_BATCH));
                recycled_pct.push(r.explicit_zero_pct);
            }

            report_row(
                "recycled",
                size_label,
                touch.label(),
                &mut ns,
                &recycled_pct,
            );
        }
    }
}

fn report_row(
    scenario: &str,
    size_label: &str,
    touch_label: &str,
    ns: &mut [f64],
    activation_pct: &[f64],
) {
    let reps = ns.len();
    ns.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean_ns: f64 = ns.iter().sum::<f64>() / (ns.len() as f64);
    let p50_ns = percentile(ns, 50.0);
    let min_ns = ns.first().copied().unwrap_or(0.0);
    let max_ns = ns.last().copied().unwrap_or(0.0);

    let mean_activation: f64 = activation_pct.iter().sum::<f64>() / (activation_pct.len() as f64);
    let min_activation = activation_pct.iter().cloned().fold(f64::INFINITY, f64::min);
    let oracle_pass = min_activation >= MIN_ACTIVATION_PCT;

    println!(
        "R30_3_ROW,{scenario},{size_label},{touch_label},{reps},{mean_ns:.1},{p50_ns:.1},{min_ns:.1},{max_ns:.1},{mean_activation:.2},{min_activation:.2},{}",
        if oracle_pass { "PASS" } else { "FAIL" },
    );
}
