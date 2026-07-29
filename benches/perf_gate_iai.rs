//! Task #127 — CI perf-gate: instruction-count regression guard.
//!
//! Why `iai-callgrind` and not the existing `criterion` benches: criterion
//! measures wall-clock time, which is noisy on shared GitHub Actions
//! runners (neighbour VMs, thermal throttling, scheduler jitter). A ±15-20%
//! threshold would be needed to avoid false positives on wall-clock, which
//! is wide enough to have missed the exact regression class this gate exists
//! to catch: the task #114 const-builder change that cost 22-31% on
//! `db_handler`-shaped workloads (per-call align/size dispatch, not gross
//! algorithmic change). `iai-callgrind` instead counts CPU *instructions*
//! retired under Valgrind/Callgrind emulation, which is deterministic
//! run-to-run on the same binary+input regardless of host contention — a
//! tight (~5-10%) threshold is viable without flaking.
//!
//! Scope: four microbenchmarks chosen to cover the hot paths touched by
//! recent fixes/regressions:
//!
//! - `small_churn_16b` — alloc+dealloc of the smallest size class (magazine/
//!   tcache fast path).
//! - `aligned_churn_640b_a128` — 640 B @ align(128): the tokio-shaped
//!   over-alignment case central to the #114 regression (align>16 no longer
//!   burns a 4 MiB segment per allocation).
//! - `large_alloc_free_cycle` — 4 MiB single-shot alloc+free: the
//!   dedicated-segment / OS-round-trip path (D1 large_cache territory).
//! - `realloc_grow` — geometric realloc growth 64 B → 4 MiB (16 doublings):
//!   the C2 realloc-grow path.
//!
//! Platform note: `iai-callgrind` benchmarks require Valgrind to actually
//! *run* (they compile a normal binary, then iai-callgrind's runner drives
//! it under `valgrind --tool=callgrind`). Valgrind is Linux-only, and the
//! `iai-callgrind` dev-dependency itself is scoped to
//! `[target.'cfg(target_os = "linux")'.dev-dependencies]` in Cargo.toml. All
//! items below (imports, benchmark functions, the `main!` invocation) are
//! `#[cfg(target_os = "linux")]`-gated except for the non-Linux `fn main`
//! fallback: Cargo still needs a `main` for this `harness = false` bench
//! target to link on every platform it resolves the target for
//! (Windows/macOS included), so the fallback compiles everywhere while the
//! real Callgrind body only exists — and only ever runs — on Linux CI.
//!
//! First-run / enforcing behavior (task #128): the perf-gate workflow now
//! PERSISTS a `main` baseline across runs (via `actions/cache`) and, on a
//! labelled PR, compares against it with `--baseline=main` plus an
//! `IAI_CALLGRIND_REGRESSION='Ir=10'` limit — so a >10% instruction-count
//! regression FAILS the (non-blocking) job. The first main-branch run merely
//! records the baseline (nothing to regress against yet). The exact numbers,
//! and that the limit actually trips, are only observable on real Linux CI
//! hardware (Valgrind is Linux-only); the threshold may be tuned once those
//! first numbers are in.

#![allow(clippy::missing_safety_doc)]

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
use std::alloc::{GlobalAlloc, Layout};
#[cfg(target_os = "linux")]
use std::hint::black_box;

#[cfg(target_os = "linux")]
use iai_callgrind::{library_benchmark, library_benchmark_group, main};
#[cfg(target_os = "linux")]
use mimalloc::MiMalloc;
#[cfg(target_os = "linux")]
use sefer_alloc::SeferAlloc;

// R22-17 (task #368): dealloc-only isolation arms need `HeapCore`/
// `HeapRegistry` directly (the `#[doc(hidden)]` test-only export surface --
// same one `tests/heap_core_tcache.rs` and friends already use), rather than
// going through `SeferAlloc`'s `GlobalAlloc` facade, so the pre-allocation
// pass can be excluded from the timed region (only the free loop is timed).
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
use sefer_alloc::registry::{bootstrap, HeapRegistry};

/// Number of alloc/dealloc pairs per churn iteration. Kept small relative to
/// the criterion benches (which use 1024) — callgrind emulation is far
/// slower than native execution; the instruction *count* is what we compare,
/// not wall-clock, so a smaller fixed op-count is enough to get a stable
/// signal without inflating CI job time.
#[cfg(target_os = "linux")]
const CHURN_OPS: usize = 64;

/// Batch size for the *cold* first-touch benches (front A). Unlike `CHURN_OPS`
/// (which reuses one block via alloc→dealloc back-to-back, hitting the hot
/// magazine path), the cold benches allocate a whole batch of DISTINCT blocks
/// before freeing any — so the magazine drains and the carve/refill path (fresh
/// segment) is exercised, not the magazine-hit path. 256 is chosen to force
/// carve well past the first magazine fill while keeping callgrind job time
/// bounded (4× `CHURN_OPS`, same order of magnitude). The bench names encode
/// this actual op-count (`..._256x..`), not the historical criterion "1024".
#[cfg(target_os = "linux")]
const COLD_BATCH: usize = 256;

// R23-2 (task #371) — warm N/2N matched-workload op counts, added to cancel
// the one-time process bootstrap constant `B` ALGEBRAICALLY instead of
// subtracting an external bootstrap-proxy bench's raw Ir (R22-15's
// `large_alloc_free_cycle` / `mimalloc_bootstrap_proxy` approach, corrected
// here per `docs/reviews/2026-07-26-r22-readonly-review.md` P1's asymmetry
// finding — see `docs/perf/R23_2_WARM_N_2N_MIMALLOC_GATE.md` §1 for why the
// two proxies are differently-sized fractions of their own arm's raw churn
// Ir, corrupting the cross-allocator ratio).
//
// Each `#[library_benchmark]` fn runs in its OWN fresh process under
// Callgrind (see `dealloc_prealloc_only_16b`'s doc comment above, and R22-17
// §2's module note) — there is no cross-fn memoization of `SeferAlloc::new()`
// or mimalloc's lazy static init between benches, so whatever one-time
// bootstrap cost `B` exists is baked into EVERY bench's raw Ir already,
// including these new N/2N arms; no separate untimed "warm-up" pre-loop is
// needed to make these arms "start warm" — the existing single-timed-loop
// pattern already IS the correct shape. Given `Ir(N) = B + N*c` and
// `Ir(2N) = B + 2N*c` for the SAME workload shape at two op counts, `c =
// (Ir(2N) - Ir(N)) / N` cancels `B` without needing to measure it via any
// proxy bench at all.
//
// `CHURN_OPS_2N` doubles `CHURN_OPS` (64 -> 128); `COLD_BATCH_2N` doubles
// `COLD_BATCH` (256 -> 512). Both stay well inside the primordial segment's
// single 4 MiB payload region (512 x 64 B = 32,768 B, a small fraction of one
// `SEGMENT`; see `src/alloc_core/os.rs::SEGMENT = 1 << 22` and
// `src/alloc_core/segment_header.rs`'s `primordial_meta_end()`/
// `small_meta_end()` asserts), so doubling does NOT cross a segment-capacity
// boundary the N-sized workload didn't already cross — the correctness
// caveat the task brief flagged does not apply to these two op counts (see
// the report's linearity sanity-check for the empirical confirmation).
#[cfg(target_os = "linux")]
const CHURN_OPS_2N: usize = CHURN_OPS * 2;
#[cfg(target_os = "linux")]
const COLD_BATCH_2N: usize = COLD_BATCH * 2;

// R23-2 (task #371) — a THIRD cold-carve op count (`COLD_BATCH_4N` = 4 x
// `COLD_BATCH` = 1,024), added ONLY for the cold-carve pair, to empirically
// test the linearity assumption `Ir(k*N) = B + k*N*c` the N/2N trick relies
// on: with a third point, `c` computed from (N, 2N) can be cross-checked
// against `c` computed from (2N, 4N) — if the workload is genuinely linear
// (no segment-boundary/geometry effect that N and 2N didn't already share),
// the two independently-derived `c` values should closely agree. 1,024 x
// 64 B = 65,536 B, still a small fraction of one 4 MiB `SEGMENT` — no
// segment-crossing risk (same margin argument as `COLD_BATCH_2N` above).
#[cfg(target_os = "linux")]
const COLD_BATCH_4N: usize = COLD_BATCH * 4;

// Small-block (16 B) alloc+dealloc churn — the magazine/tcache fast path
// exercised by every allocator-heavy workload (db_handler-shaped included).
#[cfg(target_os = "linux")]
#[library_benchmark]
fn small_churn_16b() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    for _ in 0..CHURN_OPS {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { sefer.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

// R23-2 (task #371) — the `2N` sibling of `small_churn_16b`, BYTE-IDENTICAL
// except for the op count (`CHURN_OPS_2N` = 2 x `CHURN_OPS`). Paired with
// `small_churn_16b`'s raw Ir to derive `c = (Ir(2N) - Ir(N)) / N`, the
// per-op cost with the one-time process bootstrap `B` cancelled
// algebraically — see the `CHURN_OPS_2N` doc comment above and
// `docs/perf/R23_2_WARM_N_2N_MIMALLOC_GATE.md`.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn small_churn_16b_2n() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    for _ in 0..CHURN_OPS_2N {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { sefer.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

// ---------------------------------------------------------------------------
// R22-17 (task #368) — dealloc-only isolation arms, to measure what fraction
// of a free's total Ir the `contains_base` own-thread ownership probe
// (`HeapCore::dealloc_routing` -> `AllocCore::contains_base` ->
// `SegmentTable::contains_base`, see `src/registry/heap_core_xthread.rs` and
// `src/alloc_core/segment_table.rs`) accounts for.
//
// `small_churn_16b` above measures alloc+dealloc TOGETHER; these arms isolate
// JUST the free half. IMPORTANT CORRECTION vs an earlier draft of this note:
// iai-callgrind's `#[library_benchmark]` times the ENTIRE annotated function
// body under Callgrind (there is no "setup phase excluded from
// measurement" — unlike criterion's `iter()` closures). So the pre-allocation
// pass below IS included in each arm's raw Ir. All three arms
// (`dealloc_prealloc_only_16b`, `dealloc_free_only_16b`,
// `dealloc_contains_base_probe_only_16b`) share the BYTE-IDENTICAL
// pre-allocation pass (same `CHURN_OPS`, same layout, same
// `bootstrap::ensure` + `HeapRegistry::claim` + alloc loop), so that shared
// prefix's Ir is a constant common term across all three raw numbers.
// `dealloc_prealloc_only_16b` measures that shared prefix ALONE (no loop body
// after it), so subtracting its Ir from the other two isolates each one's
// OWN loop-only cost:
//   real_free_loop_ir   = dealloc_free_only_16b_ir            - prealloc_only_ir
//   probe_loop_ir        = dealloc_contains_base_probe_only_16b_ir - prealloc_only_ir
//   contains_base share  = probe_loop_ir / real_free_loop_ir
//
// `dealloc_free_only_16b` frees the pre-allocated pointers in a tight loop
// through the SAME production `HeapCore::dealloc` -> `dealloc_routing` ->
// `contains_base` path `small_churn_16b` exercises -- nothing here is a
// bypass or an alternate implementation.
//
// `dealloc_contains_base_probe_only_16b` isolates `contains_base` ITSELF
// (via the `#[doc(hidden)]` `dbg_contains_base` measurement hook added in
// `src/registry/heap_core_diag.rs`), called directly against the same table
// state (one primordial segment, already registered by the pre-allocation
// pass) -- giving the probe's own per-call Ir with NO surrounding free
// bookkeeping (bitmap/magazine/stamp work) mixed in.
//
// All three arms require `alloc-xthread` (the feature that compiles in
// `dealloc_routing` and gates the `dbg_contains_base` hook) -- under plain
// `production` (which includes `alloc-xthread`) they compile and run
// normally; under a hypothetical feature set with `alloc-global` but without
// `alloc-xthread`, `HeapCore::dealloc` takes the `dealloc_own_thread` branch
// directly (no `contains_base` call at all), so there would be nothing to
// isolate.
//
// R23-1 (task #370) CORRECTION: an independent read-only review
// (`docs/reviews/2026-07-26-r22-readonly-review.md` P1) found that
// `dealloc_contains_base_probe_only_16b`'s timed loop calls BOTH
// `dbg_segment_base_of_ptr(ptr)` AND `dbg_contains_base(base)` per iteration
// -- so its Ir bundles `segment_base_of_ptr`'s own arithmetic, the two
// separate non-inlined call/return boundaries, and `contains_base`'s real
// work together under one "contains_base probe" label. The 18.6% headline
// (`docs/perf/R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE.md`) is therefore a
// routing-prefix upper envelope, not an isolated `contains_base`-only cost.
// `dealloc_segment_base_of_ptr_probe_only_16b` below adds the missing
// counterfactual: the SAME loop shape, calling ONLY `dbg_segment_base_of_ptr`
// (never `dbg_contains_base`) -- so its loop-only Ir isolates
// `segment_base_of_ptr` alone. Given both loop-only figures:
//   base_only_loop_ir     = dealloc_segment_base_of_ptr_probe_only_16b_ir - prealloc_only_ir
//   contains_base_only_ir = probe_loop_ir - base_only_loop_ir
// isolates `contains_base`'s OWN cost (the composite probe loop minus the
// base-only loop), leaving the two calls' call/return-boundary overhead
// bundled into the composite delta (not separately isolable without changing
// this file's established "isolate via subtraction of shared-prefix arms"
// pattern into per-instruction Callgrind attribution, which is out of scope
// here -- see the report's correction section for that residual).
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_prealloc_only_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);
    // Deliberately leaked (never freed): this arm exists ONLY to measure the
    // shared pre-allocation prefix's own Ir, common to both sibling arms
    // below. Each `#[library_benchmark]` runs in its own fresh process under
    // callgrind, so leaking here has no effect on any other bench.
}

// Same shared pre-allocation prefix as `dealloc_prealloc_only_16b` (see the
// module note above), PLUS a real free loop -- so
// `dealloc_free_only_16b`'s Ir minus `dealloc_prealloc_only_16b`'s Ir isolates
// the free loop's own cost.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_free_only_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: free-only, through the real `HeapCore::dealloc` ->
    // `dealloc_routing` -> `contains_base` path.
    for &ptr in &ptrs {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the pre-allocation pass above with
            // the same layout, and is freed exactly once.
            unsafe { (*heap).dealloc(ptr, layout) };
        }
    }
}

// Isolates JUST the `contains_base` probe (see the module note above): same
// pre-allocation shape as `dealloc_free_only_16b`, but the "timed" loop calls
// `dbg_contains_base` directly instead of a real `dealloc` -- so the blocks
// are never actually freed (they leak for the duration of the process, which
// is fine: each `#[library_benchmark]` runs in its own fresh process under
// callgrind). This measures the SAME production probe
// (`AllocCore::contains_base` -> `SegmentTable::contains_base`), just without
// the rest of `dealloc_routing`/`dealloc_own_thread_with_base`'s bookkeeping
// around it -- not an alternate/bypass implementation, see `dbg_contains_base`'s
// own doc comment in `src/registry/heap_core_diag.rs`.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_contains_base_probe_only_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: probe-only. `dbg_segment_base_of_ptr` + `dbg_contains_base`
    // together mirror exactly what `dealloc_routing` computes and checks
    // before its own-thread/foreign branch (see that function's doc comment):
    // `let base = os::segment_base_of_ptr(ptr); self.core.contains_base(base)`.
    for &ptr in &ptrs {
        if !ptr.is_null() {
            let base = unsafe { (*heap).dbg_segment_base_of_ptr(ptr) };
            let hit = unsafe { (*heap).dbg_contains_base(base) };
            black_box(hit);
        }
    }
}

// R23-1 (task #370) — isolates JUST `segment_base_of_ptr`, the missing
// counterfactual `docs/reviews/2026-07-26-r22-readonly-review.md` (P1)
// flagged: `dealloc_contains_base_probe_only_16b` above calls
// `dbg_segment_base_of_ptr` THEN `dbg_contains_base` in its timed loop, so its
// Ir bundles both functions' cost (plus two call/return boundaries) under one
// "contains_base probe" label. This arm is BYTE-IDENTICAL to
// `dealloc_contains_base_probe_only_16b` except the timed loop calls ONLY
// `dbg_segment_base_of_ptr` -- never `dbg_contains_base` -- so its loop-only
// Ir (this arm's raw Ir minus `dealloc_prealloc_only_16b`'s) isolates
// `segment_base_of_ptr`'s own cost alone. Subtracting THIS arm's loop-only Ir
// from `dealloc_contains_base_probe_only_16b`'s loop-only Ir then isolates
// `contains_base`'s own cost (the composite minus the base-only piece; the
// two calls' non-inlined call/return-boundary overhead remains bundled into
// that difference -- seeing it separately would need per-instruction
// Callgrind attribution, out of scope for this subtraction-based harness).
// The pointers are never freed (same harmless per-process leak as the sibling
// probe arm). Requires `alloc-xthread`, though `dbg_segment_base_of_ptr` itself
// only needs `alloc-global`: kept under the same `alloc-xthread` gate as its
// sibling arms so the three probe arms compile under the identical feature
// predicate and stay directly comparable.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_segment_base_of_ptr_probe_only_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: `segment_base_of_ptr` ALONE -- deliberately does NOT call
    // `dbg_contains_base`, unlike the composite probe arm above. Isolates the
    // base-computation half of the routing prefix.
    for &ptr in &ptrs {
        if !ptr.is_null() {
            let base = unsafe { (*heap).dbg_segment_base_of_ptr(ptr) };
            black_box(base);
        }
    }
}

// ---------------------------------------------------------------------------
// R23-3 (task #372) -- full orthogonal hot-path attribution, following up on
// the read-only review's P0 recommendation
// (`docs/reviews/2026-07-26-r22-readonly-review.md` §4.1): R22-17/R23-1
// isolated `contains_base` (8.8%) and `segment_base_of_ptr` (9.8%) as point
// components of a real free's Ir; this block isolates the REMAINING pieces
// of the hot alloc/free path as far as they can be cleanly separated without
// perturbing the very thing being measured (the Heisenberg risk the task
// brief warned about). See `docs/perf/R23_3_HOT_PATH_ATTRIBUTION_GATE.md`
// for the full decomposition, including what could NOT be cleanly isolated
// and why.
// ---------------------------------------------------------------------------

// R23-3 -- hot ALLOC, magazine-HIT isolation. `HeapCore::alloc`'s magazine
// fast path (`src/registry/heap_core_alloc.rs`) is: array pop (decrement
// `count`, read `slots[new_cnt]`) + (under `production`, no `hardened`) one
// `clear_magazine` bitmap write. To isolate JUST this pop cost (no carve, no
// refill, no free intermixed), the magazine must already hold resident
// blocks when the timed hit-drain runs. `TCACHE_CAP` (16, `src/registry/
// tcache.rs`) bounds how many blocks one class's magazine can hold at once.
//
// **A first draft of this pair used the N/2N technique directly on a
// repeated fill/drain LOOP (double the CYCLE count for 2N) and got a
// nonsensical result: 136.6 Ir/op for a "hit-only" pop, MORE than
// `small_churn_16b`'s own 69.0 Ir/op for a full alloc+free PAIR.** Root
// cause, found by treating that as the red flag it was rather than
// reporting it: doubling the cycle count doubles BOTH the fill work (carve
// 16 + free 16) AND the hit-drain work (pop 16) in lockstep -- since fill and
// hit are 1:1 (every pop drains a block the SAME cycle just pushed), `c =
// (Ir(2N)-Ir(N))/N` computed a per-CYCLE marginal cost (carve+free+hit,
// ~48 ops) divided by the wrong op count, not a hit-only marginal cost. This
// is disclosed here rather than silently fixed, per this project's
// zero-trust convention.
//
// **The corrected design** uses SHARED-PREFIX subtraction (R22-17/R23-1's
// established technique) instead: `alloc_magazine_prefill_only_16b` runs the
// fill (carve+free, populating the magazine to `MAGAZINE_FILL`=16 resident
// blocks) `PREFILL_CYCLES` times with NO hit-drain at all after the last
// fill. `alloc_magazine_hit_only_16b` is BYTE-IDENTICAL except it adds ONE
// final hit-drain (16 pops, all magazine HITS -- `count` was just left at 16
// by the last fill) after the same `PREFILL_CYCLES` fills. Subtracting the
// prefill arm's Ir from the hit arm's Ir isolates exactly `MAGAZINE_FILL`
// (16) hits' cost, with the (byte-identical, `PREFILL_CYCLES`-fold) fill
// cost cancelled exactly -- the same shared-prefix pattern
// `dealloc_prealloc_only_16b` already established for the free side, applied
// here because the interleaved-cycle N/2N shape does not validly isolate
// this component (see the paragraph above).
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
const MAGAZINE_FILL: usize = 16; // TCACHE_CAP, duplicated here (bench-local,
                                 // registry::tcache::TCACHE_CAP is not `pub`).
                                 // Number of fill (carve+free) cycles BEFORE the timed hit-drain. Matches
                                 // `CHURN_OPS / MAGAZINE_FILL` purely so the shared prefix's total op count is
                                 // the same order of magnitude as this file's other CHURN_OPS-scale arms --
                                 // not load-bearing for correctness (any positive cycle count would do; both
                                 // arms below share the identical prefill loop).
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
const PREFILL_CYCLES: usize = CHURN_OPS / MAGAZINE_FILL;

#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn alloc_magazine_prefill_only_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; MAGAZINE_FILL] = [core::ptr::null_mut(); MAGAZINE_FILL];
    for _ in 0..PREFILL_CYCLES {
        // Carve 16 fresh blocks (never magazine-resident before -- these are
        // carve/refill misses, not hits).
        for slot in ptrs.iter_mut() {
            // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
            *slot = unsafe { (*heap).alloc(layout) };
        }
        black_box(&ptrs);
        // Populate the magazine: free all 16 -- each push lands in the
        // magazine (count 0 -> 16), through the real
        // `dealloc_own_thread_with_base` push path. After the LAST cycle,
        // the magazine is left holding 16 resident blocks and this arm does
        // NOT drain them -- that is exactly the shared prefix
        // `alloc_magazine_hit_only_16b` below adds ONE more step onto.
        for &ptr in &ptrs {
            if !ptr.is_null() {
                // SAFETY: ptr was returned by the alloc loop above with the
                // same layout, freed exactly once.
                unsafe { (*heap).dealloc(ptr, layout) };
            }
        }
    }
}

// R23-3 -- BYTE-IDENTICAL to `alloc_magazine_prefill_only_16b` except for ONE
// added step after the shared prefill loop: drain the 16 blocks the last
// fill cycle just pushed. `count` is 16 at that point (just populated,
// never touched since), so every one of these 16 `alloc` calls is a
// magazine HIT (`cnt > 0` every time, no refill). Subtracting the prefill
// arm's Ir from this arm's Ir isolates exactly 16 hits' cost -- see the
// module note above `MAGAZINE_FILL` for why this replaced an N/2N attempt
// that did not validly isolate this component.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn alloc_magazine_hit_only_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; MAGAZINE_FILL] = [core::ptr::null_mut(); MAGAZINE_FILL];
    for _ in 0..PREFILL_CYCLES {
        for slot in ptrs.iter_mut() {
            // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
            *slot = unsafe { (*heap).alloc(layout) };
        }
        black_box(&ptrs);
        for &ptr in &ptrs {
            if !ptr.is_null() {
                // SAFETY: ptr was returned by the alloc loop above with the
                // same layout, freed exactly once.
                unsafe { (*heap).dealloc(ptr, layout) };
            }
        }
    }
    // Timed-in-spirit region: drain the magazine ONE more time. `count`
    // starts at 16 (just populated by the last prefill cycle) and every one
    // of these 16 `alloc` calls pops from it -- all 16 are magazine HITS,
    // never a miss/refill.
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);
}

// R23-3 -- free routing, Tier-2 (8192-slot open-addressing hash probe)
// isolation. R22-17/R23-1's `contains_base` isolation measures Tier-1's
// cache-HIT cost (this crate's benched workloads never span more than
// `OWN_CACHE_SIZE` (4) concurrently-hot segments, so Tier-2 never fires
// there). `dbg_hash_contains_only` (`src/alloc_core/segment_table.rs`,
// exposed via `HeapCore::dbg_hash_contains_only`) calls the SAME production
// `hash_contains` routine directly, unconditionally skipping the Tier-1
// `own_cache` check -- deterministically isolating Tier-2's cost regardless
// of OS-assigned segment addresses (see that hook's doc comment for why a
// >4-distinct-segment WORKLOAD cannot portably force a Tier-2 hit the way a
// direct call can). Same shared pre-allocation prefix shape as the sibling
// probe arms above.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_hash_contains_only_probe_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: `segment_base_of_ptr` + Tier-2-only probe. Mirrors the
    // composite `dealloc_contains_base_probe_only_16b` arm's shape exactly,
    // swapping `dbg_contains_base` for `dbg_hash_contains_only` -- so
    // subtracting `dealloc_segment_base_of_ptr_probe_only_16b`'s loop-only Ir
    // from this arm's loop-only Ir isolates Tier-2's own cost, the same
    // subtraction R23-1 used to isolate Tier-1's `contains_base`.
    for &ptr in &ptrs {
        if !ptr.is_null() {
            let base = unsafe { (*heap).dbg_segment_base_of_ptr(ptr) };
            let hit = unsafe { (*heap).dbg_hash_contains_only(base) };
            black_box(hit);
        }
    }
}

// R23-3 -- free's POST-ROUTING body isolation: the M2 double-free oracle
// checks (in-magazine bitmap probe + flushed/alloc-bitmap probe) and the
// magazine push itself, i.e. everything `dealloc_own_thread_with_base`
// (`src/registry/heap_core_free.rs`) does once ownership is already
// established. Investigated first (per the task brief): reading
// `dealloc_own_thread_with_base`'s body shows the oracle checks and the
// magazine push share the SAME `base`/`off`/`meta` locals in one straight-
// line block with no branch boundary between "check" and "push" for the
// common (non-double-free, non-overflow) case -- they are NOT two separable
// mechanisms the way `segment_base_of_ptr`/`contains_base` were; splitting
// them further would need a new hook that changes what a real free actually
// does (reading the bitmap without ever writing the magazine slot is not a
// thing the production path does), the exact Heisenberg risk the task brief
// warned about. So this arm isolates BOTH together, as the smallest honestly
// separable unit past the routing prefix.
//
// `dbg_dealloc_own_thread_with_base` (`src/registry/heap_core_diag.rs`) is
// the real `dealloc_own_thread_with_base` body, called with a
// pre-computed base exactly as `dealloc_routing` calls it once
// `contains_base` returns true -- so this arm's loop is
// `segment_base_of_ptr` + the REAL own-thread free body, deliberately
// skipping `contains_base`. Subtracting
// `dealloc_segment_base_of_ptr_probe_only_16b`'s loop-only Ir from this
// arm's isolates the post-routing body alone; it should also equal
// `dealloc_free_only_16b`'s loop-only Ir minus BOTH `contains_base`'s (R23-1)
// AND `segment_base_of_ptr`'s own isolated shares -- a cross-check performed
// in the report, not assumed.
// R24-6 (task #384): `bench-internals` added because this arm calls
// `HeapCore::dbg_dealloc_own_thread_with_base`, now gated on that feature —
// see `Cargo.toml`'s `bench-internals` doc and this bench target's
// `required-features` (which already requires it for the whole target; this
// per-arm `#[cfg]` additionally keeps `--all-features` builds honest).
#[cfg(all(
    target_os = "linux",
    feature = "alloc-xthread",
    feature = "fastbin",
    feature = "bench-internals"
))]
#[library_benchmark]
fn dealloc_own_thread_body_only_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: `segment_base_of_ptr` + the real own-thread free body,
    // deliberately skipping `contains_base` (already isolated by R23-1).
    // Actually frees every block through the production push/oracle path --
    // no bypass, no alternate implementation.
    for &ptr in &ptrs {
        if !ptr.is_null() {
            let base = unsafe { (*heap).dbg_segment_base_of_ptr(ptr) };
            // SAFETY: `ptr` was returned by the alloc loop above with `layout`
            // and is freed exactly once here; `base` is `ptr`'s true segment
            // base (`dealloc_routing`'s own `contains_base` check already
            // proved this same relationship in the sibling production path).
            unsafe { (*heap).dbg_dealloc_own_thread_with_base(ptr, layout, base) };
        }
    }
}

// ---------------------------------------------------------------------------
// R24-2 (task #380) -- decompose the free path by magazine state. R24-1
// (task #379) corrected R23-3's "74.70 Ir/free = M2 oracles + magazine push"
// headline: the bench arms free 64 DISTINCT pointers in one sequential pass,
// hitting the magazine overflow arm (`cnt == TCACHE_CAP`) six times -- so
// 74.70 Ir/free is an average over 58 non-overflow pushes AND 6 overflow
// events, NOT an isolated push cost. This block isolates the two magazine
// states (cheap non-overflow push vs. one overflow event) and sweeps the
// batch size N to show how the overflow ratio amortizes. See
// `docs/perf/R24_2_FREE_BY_MAGAZINE_STATE_GATE.md` for the full decomposition.
//
// WHY THE SWEEP USES AN alloc-64 PREFIX (count -> 0), NOT alloc-N-free-N:
// `refill_n_for_class` for the 16 B class is `TCACHE_CAP` (16), so `alloc
// k*16` leaves the magazine at exactly `count == 0`. `alloc N` for N not a
// multiple of 16 leaves `count == 16 - (N mod 16)`, so the free loop would
// NOT start at count 0 -- e.g. `alloc 8 free 8` starts the frees at count 8
// and overflows on the 8th free, and `alloc 17 free 17` gives TWO overflows,
// not one. To make every sweep point's free loop start at count 0 (so
// overflow fires predictably: 0 overflows for N<=16, exactly 1 at N=17, 2 at
// N=32, 6 at N=64), every arm below allocs a FIXED 64 (= 4*16, count -> 0)
// then frees only the first N. The shared alloc-64 prefix is byte-identical
// to `dealloc_prealloc_only_16b`, so `free_cost(N) = Ir(arm_N) - Ir(prefix)`
// cancels it exactly. This is disclosed here rather than silently chosen, per
// this file's "measured, not spun" convention (same disclosure norm as
// R23-3's §2 self-caught methodology bugs).
// ---------------------------------------------------------------------------

// R24-2 -- sweep point N=1. Shared prefix `dealloc_prealloc_only_16b` (alloc
// 64, free 0); this arm frees the first 1. One cheap push at cnt==0.
// `Ir(n1) - Ir(prefix)` isolates one non-overflow push at count 0.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_free_only_16b_n1() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: free the first 1 of the 64 pre-allocated pointers.
    for &ptr in &ptrs[..1] {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the alloc loop above with the same
            // layout, and is freed exactly once.
            unsafe { (*heap).dealloc(ptr, layout) };
        }
    }
}

// R24-2 -- sweep point N=8 + the shared prefix for the cheap-push isolation
// pair (n9 - n8 = one cheap push at cnt==8). Freeing the first 8 from count 0
// is 8 cheap pushes (cnt 0..7), no overflow.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_free_only_16b_n8() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: free the first 8 of the 64 pre-allocated pointers.
    for &ptr in &ptrs[..8] {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the alloc loop above with the same
            // layout, and is freed exactly once.
            unsafe { (*heap).dealloc(ptr, layout) };
        }
    }
}

// R24-2 -- measurement-1 dedicated pair partner: byte-identical to n8 PLUS one
// more free (the 9th), a cheap push at cnt==8 (count 8 -> 9). `Ir(n9) - Ir(n8)`
// isolates exactly one non-overflow push at count 8 -- the "pre-fill to
// count=8, plus one timed free" shared-prefix pair the task specifies.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_free_only_16b_n9() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: free the first 9 of the 64 pre-allocated pointers.
    for &ptr in &ptrs[..9] {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the alloc loop above with the same
            // layout, and is freed exactly once.
            unsafe { (*heap).dealloc(ptr, layout) };
        }
    }
}

// R24-2 -- sweep point N=16 + the shared prefix for the overflow isolation
// pair (n17 - n16 = one overflow event). Freeing the first 16 from count 0 is
// 16 cheap pushes (cnt 0..15, count -> 16), no overflow.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_free_only_16b_n16() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: free the first 16 of the 64 pre-allocated pointers.
    for &ptr in &ptrs[..16] {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the alloc loop above with the same
            // layout, and is freed exactly once.
            unsafe { (*heap).dealloc(ptr, layout) };
        }
    }
}

// R24-2 -- sweep point N=17 + measurement-2 isolation: byte-identical to n16
// PLUS one more free (the 17th), which hits cnt==TCACHE_CAP=16 -> the OVERFLOW
// arm (8-block bitmap-clear pass + flush_class on 8 blocks + 8-pointer
// compaction + final push). `Ir(n17) - Ir(n16)` isolates exactly ONE overflow
// event's extra cost over a cheap push.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_free_only_16b_n17() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: free the first 17 of the 64 pre-allocated pointers.
    for &ptr in &ptrs[..17] {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the alloc loop above with the same
            // layout, and is freed exactly once.
            unsafe { (*heap).dealloc(ptr, layout) };
        }
    }
}

// R24-2 -- sweep point N=32. Freeing the first 32 from count 0 hits overflow
// at frees #17 and #25 (2 overflow events; 30 cheap pushes), showing how the
// overflow cost amortizes as the batch grows.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_free_only_16b_n32() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; CHURN_OPS] = [core::ptr::null_mut(); CHURN_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: free the first 32 of the 64 pre-allocated pointers.
    for &ptr in &ptrs[..32] {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the alloc loop above with the same
            // layout, and is freed exactly once.
            unsafe { (*heap).dealloc(ptr, layout) };
        }
    }
}

// ---------------------------------------------------------------------------
// R25-3 (task #397) -- FLUSH_N sweep, gate 1: in-context Ir for bulk free at
// N = 17, 32, 64, 256, 1024. `FLUSH_N` (currently 8, `src/registry/tcache.rs`)
// is the compile-time constant swept by hand-editing that file between
// `npm run iai` runs (4, 8, 12, 16) -- these arms themselves do NOT encode
// FLUSH_N; they measure whatever FLUSH_N the tree is currently built with, so
// the SAME arm bodies serve every sweep point. `TCACHE_CAP` stays fixed at 16
// per the task brief.
//
// Same shared-prefix-subtraction technique as the R24-2 arms above, widened
// to reach N=1024: `PREFIX_OPS` (1024 + 64 = 1088, a multiple of 16) allocs
// 68 full magazine-fills of distinct 16 B blocks (count -> 0 after the
// prefix, same "why the sweep uses an alloc-N-not-alloc-then-free-then-
// realloc prefix" reasoning R24-2 §1.2 established), then each arm frees
// only the first N of those. `free_cost(N) = Ir(arm_N) - Ir(dealloc_prealloc_
// only_1088_16b)` isolates the N-block free loop's cost at whatever FLUSH_N
// is compiled in, cancelling the shared alloc prefix exactly.
#[cfg(target_os = "linux")]
const PREFIX_OPS: usize = 1088;

// Shared prefix for the R25-3 N-sweep (alloc PREFIX_OPS, free 0). Mirrors
// `dealloc_prealloc_only_16b` at a larger op count so N can reach 1024.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_prealloc_only_1088_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; PREFIX_OPS] = [core::ptr::null_mut(); PREFIX_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);
}

// R25-3 sweep point N=17: exactly one overflow event (matches R24-2's n17,
// re-measured here under the wider PREFIX_OPS prefix so it is directly
// comparable to the N=32/64/256/1024 points below at every FLUSH_N value).
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_free_only_1088_16b_n17() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; PREFIX_OPS] = [core::ptr::null_mut(); PREFIX_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    for &ptr in &ptrs[..17] {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the alloc loop above with the same
            // layout, and is freed exactly once.
            unsafe { (*heap).dealloc(ptr, layout) };
        }
    }
}

// R25-3 sweep point N=32.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_free_only_1088_16b_n32() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; PREFIX_OPS] = [core::ptr::null_mut(); PREFIX_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    for &ptr in &ptrs[..32] {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the alloc loop above with the same
            // layout, and is freed exactly once.
            unsafe { (*heap).dealloc(ptr, layout) };
        }
    }
}

// R25-3 sweep point N=64.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_free_only_1088_16b_n64() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; PREFIX_OPS] = [core::ptr::null_mut(); PREFIX_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    for &ptr in &ptrs[..64] {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the alloc loop above with the same
            // layout, and is freed exactly once.
            unsafe { (*heap).dealloc(ptr, layout) };
        }
    }
}

// R25-3 sweep point N=256.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_free_only_1088_16b_n256() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; PREFIX_OPS] = [core::ptr::null_mut(); PREFIX_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    for &ptr in &ptrs[..256] {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the alloc loop above with the same
            // layout, and is freed exactly once.
            unsafe { (*heap).dealloc(ptr, layout) };
        }
    }
}

// R25-3 sweep point N=1024 (the widest point; PREFIX_OPS=1088 leaves 64
// blocks unfreed so the prefix itself never fully drains during the free
// loop -- matching the "prefix allocs strictly more than every N" invariant
// R24-2 established).
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_free_only_1088_16b_n1024() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; PREFIX_OPS] = [core::ptr::null_mut(); PREFIX_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    for &ptr in &ptrs[..1024] {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the alloc loop above with the same
            // layout, and is freed exactly once.
            unsafe { (*heap).dealloc(ptr, layout) };
        }
    }
}

// R25-3 gate 2: free-then-immediate-reallocate burst. Frees the first 17
// pre-allocated blocks (one overflow event under FLUSH_N=8: blocks[0..8] are
// flushed to the substrate, blocks[8..17] remain magazine-resident), THEN
// immediately re-allocates 17 blocks of the same class and records, for each
// re-alloc, whether it returned one of the just-freed pointers (a magazine
// HIT / warm-retained block) or a fresh/different pointer (a MISS / re-carved
// or free-list block). The timed region covers BOTH the 17 frees and the 17
// re-allocs, so its Ir reflects the full free-then-realloc round trip at
// whatever FLUSH_N is compiled in -- a smaller FLUSH_N retains more
// just-freed blocks in the magazine (more LIFO hits on realloc), a larger
// FLUSH_N (up to 16, emptying the magazine completely) retains fewer/none.
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn dealloc_realloc_burst_1088_16b_n17() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; PREFIX_OPS] = [core::ptr::null_mut(); PREFIX_OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: free 17, then immediately re-allocate 17 of the same
    // class -- the exact free-then-realloc burst shape gate 2 specifies.
    for &ptr in &ptrs[..17] {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the alloc loop above with the same
            // layout, and is freed exactly once.
            unsafe { (*heap).dealloc(ptr, layout) };
        }
    }
    let mut reallocs: [*mut u8; 17] = [core::ptr::null_mut(); 17];
    for slot in reallocs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&reallocs);
}

// R25-3 gate 3: oscillating live-set size around the TCACHE_CAP=16 boundary
// (8..24 blocks), repeatedly alloc/free to cross it from both directions --
// stresses the "fits in the reduced/full magazine" vs. "needs a refill"
// boundary a FLUSH_N change could shift. One OSC_ROUNDS iteration: allocate
// up to 24 live blocks (crossing 16 from below), then free back down to 8
// (crossing 16 from above), repeated OSC_ROUNDS times. All blocks are the
// same 16 B class; `live` is a fixed-capacity ring buffer sized for the peak
// live count (24).
#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
const OSC_ROUNDS: usize = 20;

#[cfg(all(target_os = "linux", feature = "alloc-xthread"))]
#[library_benchmark]
fn oscillating_live_set_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut live: [*mut u8; 24] = [core::ptr::null_mut(); 24];
    let mut live_n: usize = 0;
    // Untimed warm-up: bring the live set to the low end (8) before the timed
    // oscillation begins, so the FIRST timed round starts from a known state.
    for slot in live.iter_mut().take(8) {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
        live_n += 1;
    }
    black_box(&live);

    // Timed region: OSC_ROUNDS oscillations, each growing live_n from 8 to 24
    // (crossing TCACHE_CAP=16 from below via allocs) then shrinking back to 8
    // (crossing 16 from above via frees).
    for _ in 0..OSC_ROUNDS {
        while live_n < 24 {
            // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
            live[live_n] = unsafe { (*heap).alloc(layout) };
            live_n += 1;
        }
        while live_n > 8 {
            live_n -= 1;
            let ptr = live[live_n];
            if !ptr.is_null() {
                // SAFETY: ptr was returned by the alloc loop above with the
                // same layout, and is freed exactly once (freed here, then
                // never read again -- `live[live_n]` is overwritten by the
                // next growth phase before any reuse).
                unsafe { (*heap).dealloc(ptr, layout) };
            }
        }
    }
    black_box(&live);
}

// R23-3 -- cold CARVE isolation: `AllocCore::carve_batch`
// (`src/alloc_core/alloc_core_small.rs`), the batched sibling of
// `carve_block` that `carve_block_with_refill`'s refill loop calls
// one-block-at-a-time. `dbg_carve_batch` (pre-existing test hook, task W4)
// drives it DIRECTLY against a bare `AllocCore` -- no magazine, no
// `HeapRegistry`/`HeapCore` plumbing, no BinTable refill push-back -- so
// this arm isolates the pure bump-cursor-advance + (lazy-commit builds only)
// commit-frontier-grow cost, without `carve_block_with_refill`'s per-extra-
// block `dealloc_small` push into the BinTable (`cold_alloc_free_256x16b`
// exercises that fuller path already). Freshly-constructed `AllocCore`, so
// every carved block is genuinely virgin bump-cursor advance, never a
// freelist pop -- see the recycle-pop isolation note below (near
// `recycle_alloc_free_256x16b`) for the recycle-pop counterfactual, derived
// via shared-prefix subtraction against `cold_alloc_free_256x16b`'s existing
// row rather than a new bench arm.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn carve_batch_only_16b() {
    let mut core = sefer_alloc::AllocCore::new().expect("primordial reservation");
    let layout = Layout::from_size_align(16, 8).unwrap();
    let class_idx = core
        .dbg_layout_class_for(layout)
        .expect("16 B/align 8 must resolve to a small class");
    let mut out: [*mut u8; COLD_BATCH] = [core::ptr::null_mut(); COLD_BATCH];
    let n = core.dbg_carve_batch(class_idx, &mut out);
    black_box(&out[..n]);
}

// R23-3 -- the `2N` sibling, byte-identical except for the batch size
// (`COLD_BATCH_2N`), for the N/2N marginal cost derivation. Uses a fresh
// `AllocCore` (its own one-time bootstrap `B`), exactly as every other N/2N
// pair in this file uses a fresh process/allocator per arm.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn carve_batch_only_16b_2n() {
    let mut core = sefer_alloc::AllocCore::new().expect("primordial reservation");
    let layout = Layout::from_size_align(16, 8).unwrap();
    let class_idx = core
        .dbg_layout_class_for(layout)
        .expect("16 B/align 8 must resolve to a small class");
    let mut out: [*mut u8; COLD_BATCH_2N] = [core::ptr::null_mut(); COLD_BATCH_2N];
    let n = core.dbg_carve_batch(class_idx, &mut out);
    black_box(&out[..n]);
}

// R24-8 (Investigation 1 + 2) -- `HeapCore::dealloc_batch` on a fresh-carve
// SAME-SEGMENT batch. Fresh heap, allocate N consecutive 16 B blocks (all land
// in ONE freshly-carved segment -> N same-base `contains_base` calls, the
// workload the proposed `last_base` ownership cache targets), then free them all
// in ONE `dealloc_batch` call. N=16 stays within `TCACHE_CAP` (no magazine
// overflow -> isolates `contains_base` caching + magazine fill, the cheap path);
// N=64 overflows the magazine (48 staged -> `flush_class`, the overflow path).
// Both sizes share ONE segment, so every `contains_base` after the first is a
// Tier-1 `own_cache` hit AND a would-be `last_base` cache hit.
#[cfg(all(target_os = "linux", feature = "batch-api"))]
#[library_benchmark]
fn dealloc_batch_fresh_16_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; 16] = [core::ptr::null_mut(); 16];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: one batched free of 16 same-segment blocks.
    // SAFETY: every entry was returned by the pre-allocation pass above with the
    // same layout, and is freed exactly once here.
    unsafe { (*heap).dealloc_batch(layout, &ptrs) };
}

#[cfg(all(target_os = "linux", feature = "batch-api"))]
#[library_benchmark]
fn dealloc_batch_fresh_64_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; 64] = [core::ptr::null_mut(); 64];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: one batched free of 64 same-segment blocks (magazine
    // overflow: first 16 fill the magazine, remaining 48 staged -> flush_class).
    // SAFETY: every entry was returned by the pre-allocation pass above with the
    // same layout, and is freed exactly once here.
    unsafe { (*heap).dealloc_batch(layout, &ptrs) };
}

// R25-7 (task #401) -- `STAGE_CAP` boundary sweep. These six arms extend the
// R24-8 `dealloc_batch_fresh_{16,64}_16b` pair to batch sizes that EXERCISE THE
// NEW MULTI-FLUSH PATH introduced by R24-8's STAGE_CAP 512->64 reduction. With
// STAGE_CAP=64, a batch does: first TCACHE_CAP(16) -> magazine, remaining N-16
// -> staged, flushed in STAGE_CAP(64)-sized chunks via intermediate flush_class
// calls. So N > STAGE_CAP + TCACHE_CAP = 80 triggers >=1 intermediate flush.
//   N=80:   the LARGEST batch that still fits in ONE flush (64 staged + 16
//           magazine = exactly 80, zero intermediate flushes).
//   N=81:   the SMALLEST batch that triggers exactly ONE intermediate flush.
//   N=128:  one intermediate (64) + one final (48) flush.
//   N=200:  two intermediate (64+64) + one final (56) flush.
//   N=512:  seven intermediate (64x7=448) + one final (48) flush.
//   N=1024: fifteen intermediate (64x15=960) + one final (48) flush.
// These arms measure whatever STAGE_CAP the tree is currently built with (they
// do NOT hardcode a value), so they double as reusable regression infrastructure
// for future STAGE_CAP changes -- same precedent as R24-8/R24-2/R25-3's retained
// bench arms.
#[cfg(all(target_os = "linux", feature = "batch-api"))]
#[library_benchmark]
fn dealloc_batch_fresh_80_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; 80] = [core::ptr::null_mut(); 80];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: one batched free of 80 same-segment blocks (exactly fills
    // magazine(16) + stage(64); zero intermediate flushes at STAGE_CAP=64).
    // SAFETY: every entry was returned by the pre-allocation pass above with the
    // same layout, and is freed exactly once here.
    unsafe { (*heap).dealloc_batch(layout, &ptrs) };
}

#[cfg(all(target_os = "linux", feature = "batch-api"))]
#[library_benchmark]
fn dealloc_batch_fresh_81_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; 81] = [core::ptr::null_mut(); 81];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: one batched free of 81 same-segment blocks (magazine(16) +
    // stage fills to 64 -> ONE intermediate flush, then 1 final). The smallest N
    // that triggers the mid-loop multi-flush path at STAGE_CAP=64.
    // SAFETY: every entry was returned by the pre-allocation pass above with the
    // same layout, and is freed exactly once here.
    unsafe { (*heap).dealloc_batch(layout, &ptrs) };
}

#[cfg(all(target_os = "linux", feature = "batch-api"))]
#[library_benchmark]
fn dealloc_batch_fresh_128_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; 128] = [core::ptr::null_mut(); 128];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: one batched free of 128 same-segment blocks (magazine(16) +
    // one intermediate flush(64) + one final flush(48)).
    // SAFETY: every entry was returned by the pre-allocation pass above with the
    // same layout, and is freed exactly once here.
    unsafe { (*heap).dealloc_batch(layout, &ptrs) };
}

#[cfg(all(target_os = "linux", feature = "batch-api"))]
#[library_benchmark]
fn dealloc_batch_fresh_200_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; 200] = [core::ptr::null_mut(); 200];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: one batched free of 200 same-segment blocks (magazine(16) +
    // two intermediate flushes(64+64) + one final flush(56)).
    // SAFETY: every entry was returned by the pre-allocation pass above with the
    // same layout, and is freed exactly once here.
    unsafe { (*heap).dealloc_batch(layout, &ptrs) };
}

#[cfg(all(target_os = "linux", feature = "batch-api"))]
#[library_benchmark]
fn dealloc_batch_fresh_512_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; 512] = [core::ptr::null_mut(); 512];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: one batched free of 512 same-segment blocks (magazine(16) +
    // seven intermediate flushes(64x7=448) + one final flush(48)).
    // SAFETY: every entry was returned by the pre-allocation pass above with the
    // same layout, and is freed exactly once here.
    unsafe { (*heap).dealloc_batch(layout, &ptrs) };
}

#[cfg(all(target_os = "linux", feature = "batch-api"))]
#[library_benchmark]
fn dealloc_batch_fresh_1024_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; 1024] = [core::ptr::null_mut(); 1024];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: one batched free of 1024 same-segment blocks (magazine(16) +
    // fifteen intermediate flushes(64x15=960) + one final flush(48)).
    // SAFETY: every entry was returned by the pre-allocation pass above with the
    // same layout, and is freed exactly once here.
    unsafe { (*heap).dealloc_batch(layout, &ptrs) };
}

// R26-7 (task #416) -- small-N grid coverage. These 4 eager arms extend the
// `dealloc_batch` N-grid below N=16, which R24-8/R25-7's existing arms
// (16/64/80/81/128/200/512/1024) never covered: N=0/1/8 are sub-magazine
// (within TCACHE_CAP=16, never overflow the magazine), N=17 is the first-
// overflow crossover (magazine(16) + 1 staged). They measure the SHIPPING
// `dealloc_batch` (the lazy `Option<[..]>` A/B variant this task also measured
// was NO-GO and removed in R27-6/task #424; see
// `docs/perf/R26_7_LAZY_STAGE_ARRAY_GATE.md`).

// ── Small-N grid arms (0/1/8/17) — extend R24-8/R25-7's grid below N=16 ──
#[cfg(all(target_os = "linux", feature = "batch-api"))]
#[library_benchmark]
fn dealloc_batch_fresh_0_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let ptrs: [*mut u8; 0] = [];
    black_box(&ptrs);

    // Timed region: one batched free of 0 blocks. The 512-byte stage array is
    // allocated/zeroed unconditionally even though no block is ever staged.
    // SAFETY: empty slice; every (zero) entry trivially satisfies the contract.
    unsafe { (*heap).dealloc_batch(layout, &ptrs) };
}

#[cfg(all(target_os = "linux", feature = "batch-api"))]
#[library_benchmark]
fn dealloc_batch_fresh_1_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; 1] = [core::ptr::null_mut(); 1];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: one batched free of 1 same-segment block (within TCACHE_CAP;
    // never overflows the magazine, so stage is never written).
    // SAFETY: every entry was returned by the pre-allocation pass above with the
    // same layout, and is freed exactly once here.
    unsafe { (*heap).dealloc_batch(layout, &ptrs) };
}

#[cfg(all(target_os = "linux", feature = "batch-api"))]
#[library_benchmark]
fn dealloc_batch_fresh_8_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; 8] = [core::ptr::null_mut(); 8];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: one batched free of 8 same-segment blocks (within TCACHE_CAP;
    // never overflows the magazine, so stage is never written).
    // SAFETY: every entry was returned by the pre-allocation pass above with the
    // same layout, and is freed exactly once here.
    unsafe { (*heap).dealloc_batch(layout, &ptrs) };
}

#[cfg(all(target_os = "linux", feature = "batch-api"))]
#[library_benchmark]
fn dealloc_batch_fresh_17_16b() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut ptrs: [*mut u8; 17] = [core::ptr::null_mut(); 17];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { (*heap).alloc(layout) };
    }
    black_box(&ptrs);

    // Timed region: one batched free of 17 same-segment blocks (magazine(16) +
    // 1 staged; the SMALLEST N that overflows into the stage array -- 1 final
    // flush, zero intermediate).
    // SAFETY: every entry was returned by the pre-allocation pass above with the
    // same layout, and is freed exactly once here.
    unsafe { (*heap).dealloc_batch(layout, &ptrs) };
}

// R23-3 -- recycle FREELIST-POP isolation.
//
// **A first draft here added `recycle_alloc_free_256x16b_2n` (an N/2N
// sibling doubling `COLD_BATCH`) and got a nonsensical result: 399.4 Ir/op,
// roughly DOUBLE virgin-carve's own marginal cost, for a mechanism (freelist
// pop) that should be cheaper or comparable, never a strict multiple more
// expensive than carving.** Root cause, again found by treating the
// surprising number as a red flag rather than reporting it: doubling
// `COLD_BATCH` in a TWO-ROUND bench doubles BOTH round 1 (256 extra virgin
// carve+frees) AND round 2 (256 extra recycle-pop+frees) in lockstep, so
// `c = (Ir(2N)-Ir(N))/COLD_BATCH` measured the marginal cost of one
// COMBINED (carve-round + recycle-round) unit, not recycle alone -- the
// exact same category of mistake the alloc-magazine-hit arms above hit
// first (see `MAGAZINE_FILL`'s doc comment) and fixed the same way: replace
// the invalid N/2N pair with shared-prefix subtraction.
//
// **The fix needs NO new bench arm.** `cold_alloc_free_256x16b` (this
// file, above) already IS round 1 of `recycle_alloc_free_256x16b` in
// isolation: both are `SeferAlloc::new()` + `COLD_BATCH` (256) virgin
// alloc-then-free-all, byte-for-byte identical bootstrap and workload
// shape. So `recycle_alloc_free_256x16b`'s raw Ir minus
// `cold_alloc_free_256x16b`'s raw Ir isolates round 2 (the freelist-pop
// round) alone, with round 1's (byte-identical) cost cancelled exactly --
// the report performs this subtraction directly from the two existing rows;
// see `docs/perf/R23_3_HOT_PATH_ATTRIBUTION_GATE.md`.

// R18-3 (task #330) — the FIRST instruction-count baseline for the dealloc
// hot path under `production,medium-classes`, the configuration where R17-4's
// Large-segment `kind_at` routing check (`dealloc_own_thread_with_base`,
// `src/registry/heap_core_free.rs` branch A) actually COMPILES IN. R17-4's
// "zero hot-path cost" claim was measured only under plain `production` (where
// branch A compiles out entirely) — this bench closes that proof gap by
// tracking the path WITH the check present. The R18-3 runtime size gate
// (`layout.size() >= MEDIUM_REALLOC_PROMOTION_THRESHOLD`) short-circuits the
// `kind_at` field load for every 16 B free below the 256 KiB threshold, so this
// bench measures exactly the cost of that gate on the common small-free path.
//
// Structurally identical to `small_churn_16b` (same 16 B alloc→dealloc churn)
// but named and documented for the medium-classes config: under plain
// `production` both produce identical Ir (branch A absent); under
// `production,medium-classes` THIS row is the tracked baseline. Run via:
//   node scripts/iai.mjs --features 'production medium-classes' medium_class_dealloc_churn_16b
#[cfg(target_os = "linux")]
#[library_benchmark]
fn medium_class_dealloc_churn_16b() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    for _ in 0..CHURN_OPS {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { sefer.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

// 640 B @ align(128) alloc+dealloc churn — the tokio-shaped over-alignment
// case at the center of the task #114 regression (align>16 previously
// burned a full 4 MiB segment per allocation instead of routing through
// the size-class path).
#[cfg(target_os = "linux")]
#[library_benchmark]
fn aligned_churn_640b_a128() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(640, 128).unwrap();
    for _ in 0..CHURN_OPS {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { sefer.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

// Single-shot 4 MiB alloc+free — the dedicated-segment / OS-round-trip path
// (D1 large_cache territory: `mmap`/`VirtualAlloc` cost per large block).
#[cfg(target_os = "linux")]
#[library_benchmark]
fn large_alloc_free_cycle() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(4 * 1024 * 1024, 8).unwrap();
    // SAFETY: layout has non-zero size and valid alignment.
    let ptr = unsafe { sefer.alloc(layout) };
    black_box(ptr);
    if !ptr.is_null() {
        // SAFETY: ptr was returned by the `alloc` call directly above with
        // the same layout.
        unsafe { sefer.dealloc(ptr, layout) };
    }
}

// Geometric realloc growth: 64 B doubled 16x up to 4 MiB via
// `GlobalAlloc::realloc` (the C2 realloc-grow path; no `Vec` amortization
// hiding the per-step cost).
#[cfg(target_os = "linux")]
#[library_benchmark]
fn realloc_grow() {
    let sefer = SeferAlloc::new();
    let align = 8_usize;
    let start = 64_usize;
    let doublings = 16_u32;

    let init_layout = Layout::from_size_align(start, align).unwrap();
    // SAFETY: init_layout has non-zero size and valid alignment.
    let mut ptr = unsafe { sefer.alloc(init_layout) };
    if ptr.is_null() {
        return;
    }
    let mut current_size = start;

    for _ in 0..doublings {
        let new_size = current_size * 2;
        let old_layout = Layout::from_size_align(current_size, align).unwrap();
        // SAFETY: ptr was returned by a prior alloc/realloc call with
        // `old_layout`; `new_size` is non-zero.
        let new_ptr = unsafe { sefer.realloc(ptr, old_layout, new_size) };
        if new_ptr.is_null() {
            // SAFETY: ptr is still valid for `old_layout` (realloc did not
            // free on OOM).
            unsafe { sefer.dealloc(ptr, old_layout) };
            return;
        }
        ptr = new_ptr;
        current_size = new_size;
    }

    black_box(ptr);
    let final_layout = Layout::from_size_align(current_size, align).unwrap();
    // SAFETY: ptr is the result of the last successful alloc/realloc call
    // with `final_layout`.
    unsafe { sefer.dealloc(ptr, final_layout) };
}

// Front A — cold first-touch of tiny 16 B blocks. Allocate a whole batch of
// `COLD_BATCH` distinct blocks (no alloc↔dealloc reuse), THEN free them all in
// a second pass. This drains the per-thread magazine and forces the
// carve/refill path (magazine empty, fresh segment) rather than the hot
// magazine-hit path that `small_churn_16b` measures. Op-count is encoded in
// the name (256×16 B) per §F semantic conformance.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn cold_alloc_free_256x16b() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH] = [core::ptr::null_mut(); COLD_BATCH];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { sefer.alloc(layout) };
    }
    black_box(&ptrs);
    for &ptr in &ptrs {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by an `alloc` call above with the same
            // layout, and is freed exactly once.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

// R23-2 (task #371) — the `2N` sibling of `cold_alloc_free_256x16b`,
// BYTE-IDENTICAL except for the batch size (`COLD_BATCH_2N` = 2 x
// `COLD_BATCH`, still well within one primordial segment's payload capacity
// — see the `COLD_BATCH_2N` doc comment above). Paired with
// `cold_alloc_free_256x16b`'s raw Ir to derive the bootstrap-cancelled
// per-op cost `c = (Ir(2N) - Ir(N)) / N` on the cold-carve path — see
// `docs/perf/R23_2_WARM_N_2N_MIMALLOC_GATE.md`.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn cold_alloc_free_256x16b_2n() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH_2N] = [core::ptr::null_mut(); COLD_BATCH_2N];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { sefer.alloc(layout) };
    }
    black_box(&ptrs);
    for &ptr in &ptrs {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by an `alloc` call above with the same
            // layout, and is freed exactly once.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

// R23-2 (task #371) — the `4N` sibling, added ONLY for this cold-carve
// pair, purely as a linearity sanity-check (see `COLD_BATCH_4N`'s doc
// comment above): with three op counts (N, 2N, 4N), `c` derived from
// (N, 2N) can be cross-checked against `c` derived from (2N, 4N).
#[cfg(target_os = "linux")]
#[library_benchmark]
fn cold_alloc_free_256x16b_4n() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH_4N] = [core::ptr::null_mut(); COLD_BATCH_4N];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { sefer.alloc(layout) };
    }
    black_box(&ptrs);
    for &ptr in &ptrs {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by an `alloc` call above with the same
            // layout, and is freed exactly once.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

// R24-5 (task #383) -- the ALLOC-ONLY shared prefix for the cold alloc/free
// split. Byte-identical to `cold_alloc_free_256x16b` EXCEPT the free loop is
// removed (COLD_BATCH pointers deliberately leaked -- each
// `#[library_benchmark]` runs in its own fresh process under callgrind, so
// leaking here has no effect on any other bench; same rationale as
// `dealloc_prealloc_only_16b`'s doc comment). `free_cost(N) =
// Ir(cold_alloc_free) - Ir(cold_alloc_only)` isolates JUST the free work
// (R24-2's shared-prefix technique scaled from CHURN_OPS=64 to COLD_BATCH=256),
// and the N/2N/4N trio yields the bootstrap-cancelled per-op ALLOC cost
// `c_alloc = (Ir(_2n) - Ir(_N)) / N` (R23-2's technique). See
// `docs/perf/R24_5_COLD_ALLOC_FREE_SPLIT_GATE.md`.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn cold_alloc_only_256x16b() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH] = [core::ptr::null_mut(); COLD_BATCH];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { sefer.alloc(layout) };
    }
    black_box(&ptrs);
    // Deliberately leaked (never freed): this arm exists ONLY to measure the
    // shared alloc prefix's own Ir -- see the doc comment above.
}

// R24-5 (task #383) -- the `2N` sibling of `cold_alloc_only_256x16b`,
// byte-identical except for the batch size (`COLD_BATCH_2N`), for the
// bootstrap-cancelled per-op ALLOC cost `c = (Ir(2N) - Ir(N)) / N`.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn cold_alloc_only_256x16b_2n() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH_2N] = [core::ptr::null_mut(); COLD_BATCH_2N];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { sefer.alloc(layout) };
    }
    black_box(&ptrs);
    // Deliberately leaked (never freed): alloc-only shared prefix.
}

// R24-5 (task #383) -- the `4N` sibling (`COLD_BATCH_4N`), added for the same
// linearity cross-check rationale as `cold_alloc_free_256x16b_4n`
// (`c` derived from (N,2N) cross-checked against `c` from (2N,4N)).
#[cfg(target_os = "linux")]
#[library_benchmark]
fn cold_alloc_only_256x16b_4n() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH_4N] = [core::ptr::null_mut(); COLD_BATCH_4N];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { sefer.alloc(layout) };
    }
    black_box(&ptrs);
    // Deliberately leaked (never freed): alloc-only shared prefix.
}

// Front A — same cold first-touch shape as `cold_alloc_free_256x16b`, but with
// 64 B blocks (align 8). Second tiny size class on the carve/refill path.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn cold_alloc_free_256x64b() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(64, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH] = [core::ptr::null_mut(); COLD_BATCH];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { sefer.alloc(layout) };
    }
    black_box(&ptrs);
    for &ptr in &ptrs {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by an `alloc` call above with the same
            // layout, and is freed exactly once.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

// P7 Front — steady-state cold recycle of tiny 16 B blocks. Unlike the cold
// benches above (which measure the VIRGIN bump/carve path — a fresh process, one
// round, blind to what happens once blocks have been freed once), this bench runs
// TWO rounds: allocate `COLD_BATCH` distinct blocks, free them ALL, then allocate
// `COLD_BATCH` again + free them all again. Round 1's frees flush the drained
// magazine's overflow into the BinTable freelist; round 2's allocs then DRAIN that
// freelist (`pop_free` per block) instead of bump-carving virgin memory. That
// freelist-refill round-trip — a dependent `read_next` load + `mark_alloc` bitmap
// RMW + `inc_live` per block — is exactly the steady-state cold path P7's Э7/Э8/Э10
// batch-drain optimizations target, and which the single-round `cold_*` benches and
// the criterion steady-state numbers cannot isolate. Only round 2 is the signal;
// round 1 exists solely to populate the freelist round 2 drains. `COLD_BATCH` (256)
// is reused unchanged so the recycle op-count matches the virgin cold benches — the
// virgin-vs-recycle instruction delta is then a clean apples-to-apples comparison.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn recycle_alloc_free_256x16b() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH] = [core::ptr::null_mut(); COLD_BATCH];
    for _round in 0..2 {
        for slot in ptrs.iter_mut() {
            // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
            *slot = unsafe { sefer.alloc(layout) };
        }
        black_box(&ptrs);
        for &ptr in &ptrs {
            if !ptr.is_null() {
                // SAFETY: ptr was returned by an `alloc` call above with the same
                // layout, and is freed exactly once per round.
                unsafe { sefer.dealloc(ptr, layout) };
            }
        }
    }
}

// R24-5 (task #383) -- recycle round-2 ALLOC-ONLY isolation. Round 1 here is
// byte-identical to `cold_alloc_free_256x16b`'s whole body (alloc COLD_BATCH +
// free COLD_BATCH, populating the BinTable freelist); round 2 then allocates
// COLD_BATCH again -- draining that recycled freelist -- but does NOT free. So:
//   round2_alloc_only = Ir(recycle_alloc_only) - Ir(cold_alloc_free)
//   round2_free_only  = Ir(recycle_alloc_free) - Ir(recycle_alloc_only)
// (the two sum to the round-2 total R23-3 measured via
// `recycle_alloc_free - cold_alloc_free`). This splits round 2 the same way
// the alloc-only/free-only pair above split round 1, isolating the
// recycled-refill alloc cost on the SAME SeferAlloc/HeapCore face as the
// virgin alloc cost. Round-2 pointers are deliberately leaked (own fresh
// process per bench). See `docs/perf/R24_5_COLD_ALLOC_FREE_SPLIT_GATE.md`.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn recycle_alloc_only_256x16b() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH] = [core::ptr::null_mut(); COLD_BATCH];

    // Round 1: alloc + free (populate the freelist round 2 drains).
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { sefer.alloc(layout) };
    }
    black_box(&ptrs);
    for &ptr in &ptrs {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by an alloc call above with the same
            // layout, and is freed exactly once.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }

    // Round 2: alloc-only (drain the recycled freelist) -- NO free.
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { sefer.alloc(layout) };
    }
    black_box(&ptrs);
    // Round-2 pointers deliberately leaked (own fresh process per bench).
}

// P7 Front — same two-round steady-state recycle shape as
// `recycle_alloc_free_256x16b`, but with 64 B blocks (align 8). Second tiny size
// class on the freelist-drain path; round 2 drains what round 1 freed.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn recycle_alloc_free_256x64b() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(64, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH] = [core::ptr::null_mut(); COLD_BATCH];
    for _round in 0..2 {
        for slot in ptrs.iter_mut() {
            // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
            *slot = unsafe { sefer.alloc(layout) };
        }
        black_box(&ptrs);
        for &ptr in &ptrs {
            if !ptr.is_null() {
                // SAFETY: ptr was returned by an `alloc` call above with the same
                // layout, and is freed exactly once per round.
                unsafe { sefer.dealloc(ptr, layout) };
            }
        }
    }
}

// Front B — 256 B @ align(8) alloc+dealloc churn: the working-set reuse shape
// of `small_churn_16b` (immediate alloc→dealloc, hitting the magazine), at the
// size where mimalloc leads even on reuse. This is the hot-path counterpart to
// the cold benches above.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn churn_256b() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(256, 8).unwrap();
    for _ in 0..CHURN_OPS {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { sefer.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

// Writing-churn counterpart of `small_churn_16b` but at 256 B: after each
// non-null alloc, write the first 16 bytes (two u64 words) of the block before
// freeing it. This dirties word1 (bytes 8..16 — the magazine M2 double-free
// guard key slot), reproducing the realistic write-to-what-you-allocate
// pattern instead of leaving a stale key that forces a slow-path scan on free.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn churn_write_256b() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(256, 8).unwrap();
    for _ in 0..CHURN_OPS {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { sefer.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr is a freshly allocated 256 B block; the first 16
            // bytes are in bounds and writable. `write_volatile` prevents the
            // stores being elided.
            unsafe { core::ptr::write_volatile(ptr.cast::<u64>(), 0xA5A5_A5A5_A5A5_A5A5) };
            unsafe { core::ptr::write_volatile(ptr.cast::<u64>().add(1), 0xA5A5_A5A5_A5A5_A5A5) };
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

// X5 judge seed — multi-segment cold alloc/free. The future X5 work targets
// per-class segment queues; this bench forces the allocator to REGISTER
// MULTIPLE small segments for ONE size class and then scan them on the second
// round's allocs (`find_segment_with_free` walks every registered segment of
// the class when the magazine + primordial freelist are drained). Geometry:
// the largest small size class is 258,752 B (≈253 KiB), and one 4 MiB segment
// holds 15 usable such blocks (16 fit in 4 MiB but the primordial reserves one
// block's worth for its registry and each fresh segment loses one to per-segment
// metadata); so `MULTISEG_BATCH` (34) allocations span 3 segments (15 + 15 + 4).
// Round 1 allocates 34 distinct blocks — draining the magazine,
// carving segment 1, then registering + carving segments 2 and 3 — and frees
// them all. Round 2 allocates 34 again: with the magazine drained and every
// block back on the segment freelists, each alloc's magazine-refill calls
// `find_segment_with_free`, which must walk the 3 registered segments. This is
// the exact path X5's segment-queue reordering will speed up; the cold
// first-segment carve (round 1) is the floor X5 cannot beat. The block size
// (258,752 B ≈ 253 KiB requests, served by the largest small class — NOT literal 16 B
// blocks) is chosen so a handful of allocations span multiple segments; 16 B
// blocks would need ~260k allocations to fill one segment, far too many for
// callgrind's <1M-Ir budget. Kept FAST: 2 × 34 = 68 allocs + 68 frees of a
// cache-cold large-small class — total work comparable to the existing cold
// benches (well under 1M Ir; the per-alloc cost is dominated by the segment
// scan, not a 253 KiB memcpy, since these are fresh freelist pops).
// SMALL_MAX (258,752 B ≈ 253 KiB) EXACTLY — the largest small size class, so
// this request routes to the Small (per-segment freelist) path, NOT the Large
// dedicated-segment path. A literal 256 KiB (262,144 B) exceeds SMALL_MAX and
// would give ONE Large segment per block (1 block/segment), breaking the
// "16 blocks per 4 MiB segment, 34 blocks span 3 segments" geometry this bench
// relies on to exercise `find_segment_with_free`'s multi-segment scan.
#[cfg(target_os = "linux")]
const MULTISEG_BLOCK: usize = 258_752;
#[cfg(target_os = "linux")]
const MULTISEG_BATCH: usize = 34;
#[cfg(target_os = "linux")]
#[library_benchmark]
fn multiseg_cold_256k() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(MULTISEG_BLOCK, 8).unwrap();
    let mut ptrs: [*mut u8; MULTISEG_BATCH] = [core::ptr::null_mut(); MULTISEG_BATCH];
    for _round in 0..2 {
        for slot in ptrs.iter_mut() {
            // SAFETY: layout has non-zero size and valid (power-of-two)
            // alignment.
            *slot = unsafe { sefer.alloc(layout) };
        }
        black_box(&ptrs);
        for &ptr in &ptrs {
            if !ptr.is_null() {
                // SAFETY: ptr was returned by an `alloc` call above with the
                // same layout, and is freed exactly once per round.
                unsafe { sefer.dealloc(ptr, layout) };
            }
        }
    }
}

// PERF-4 (task #14) — decommit→recycle segment-churn regression guard.
//
// The `cold_*` / `recycle_*` / `churn_*` benches above all live inside the
// PRIMORDIAL segment (small op-counts never span past it), and the primordial
// segment is explicitly EXCLUDED from decommit (`dec_live_and_maybe_decommit`
// bails on `kind != Small`) — so NONE of them ever exercise
// `decommit_empty_segment` → `table.recycle`. `multiseg_cold_256k` touches it
// only twice (2 rounds). This bench is the dedicated regression target for the
// decommit-churn path the shamir-db sweep flagged (0.3.0 vs 0.2.1): it drives a
// NON-primordial small segment through empty→decommit→recycle→re-reserve on
// EVERY round, which is precisely where PERF-4's "dead metadata reset before
// release" tax was paid, and where the fix (release-follows fast path in
// `decommit_empty_segment_for_release`) removes it.
//
// Geometry: the largest small size class is SMALL_MAX = 258,752 B (≈253 KiB);
// one 4 MiB segment holds 15 usable such blocks (16 fit in 4 MiB, but the
// primordial reserves one block's worth for its self-hosted registry and every
// fresh small segment loses a block to per-segment metadata → 15 usable).
// `SEGCYCLE_BATCH` (34) fills the primordial (15) + a SECOND small segment (15)
// + opens a THIRD (4) each round, so the SECOND segment is NON-current when the
// batch is freed → it empties while not the carve target → `decommit_empty_segment`
// fires and `recycle` returns it to the OS. The next round re-reserves it.
// CRITICAL: a batch that only just spills into the second segment (e.g. 18) does
// NOT decommit — that second segment is still the CURRENT carve target, which is
// excluded from decommit; the batch MUST reach a THIRD segment (≥ 31 blocks) to
// leave the second one non-current. `SEGCYCLE_ROUNDS` (6) repeats the full
// reserve→fill→empty→decommit→recycle cycle so the per-round decommit/recycle
// cost dominates the signal (measured: 6 decommits per run, 1 per round). Kept
// within callgrind's <1M-Ir budget: 6 × 34 = 204 allocs + 204 frees of a
// large-small class (freelist pops, not 253 KiB memcpys).
//
// The block size MUST be `<= SMALL_MAX` (258,752 B), NOT a literal 256 KiB
// (262,144 B): 262,144 > 258,752 routes to the dedicated-segment Large path,
// where `dec_live_and_maybe_decommit` bails on `kind != Small` and
// `decommit_empty_segment_for_release` (the very path this bench guards) is
// NEVER reached — the pre-fix bench silently measured the Large path and its
// decommit counter never moved.
#[cfg(target_os = "linux")]
const SEGCYCLE_BLOCK: usize = 258_752;
#[cfg(target_os = "linux")]
const SEGCYCLE_BATCH: usize = 34;
#[cfg(target_os = "linux")]
const SEGCYCLE_ROUNDS: usize = 6;
#[cfg(target_os = "linux")]
#[library_benchmark]
fn seg_cycle_decommit_256k() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(SEGCYCLE_BLOCK, 8).unwrap();
    let mut ptrs: [*mut u8; SEGCYCLE_BATCH] = [core::ptr::null_mut(); SEGCYCLE_BATCH];
    for _round in 0..SEGCYCLE_ROUNDS {
        for slot in ptrs.iter_mut() {
            // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
            *slot = unsafe { sefer.alloc(layout) };
        }
        black_box(&ptrs);
        for &ptr in &ptrs {
            if !ptr.is_null() {
                // SAFETY: ptr was returned by an `alloc` call above with the same
                // layout, and is freed exactly once per round. Freeing the whole
                // batch empties the non-primordial second segment → decommit →
                // recycle.
                unsafe { sefer.dealloc(ptr, layout) };
            }
        }
    }
}

// ---------------------------------------------------------------------------
// R22-15 (task #366) — mimalloc comparison arms.
//
// Per R20-4's (task #349) feasibility finding (docs/perf/
// R20_4_MIMALLOC_IR_ARM_FEASIBILITY.md, §8 implementation sketch): mimalloc's
// C core is statically linked into this same binary (libmimalloc-sys's
// build.rs "we only ever build a static lib"), so Callgrind's instruction
// count for it is exactly as attributable as SeferAlloc's own Rust code --
// no dynamic-link/JIT attribution gap. Per benches/global_alloc.rs's already-
// established pattern (module doc there, lines 12-19), mimalloc is called
// DIRECTLY through its `GlobalAlloc` impl on a locally-constructed
// `mimalloc::MiMalloc` value -- NEVER installed as `#[global_allocator]` --
// so it can live in the SAME bench binary/file as the SeferAlloc arms above,
// with no new bench target and no `Cargo.toml`/CI change required.
//
// Every mimalloc bench below is a byte-for-byte mirror of its SeferAlloc
// sibling (same op counts, same sizes, same alignment, same alloc/dealloc
// shape) so the comparison is apples-to-apples -- only the allocator value
// differs (`mi.alloc(layout)` / `mi.dealloc(ptr, layout)` in place of
// `sefer.alloc(layout)` / `sefer.dealloc(ptr, layout)`).
//
// `mimalloc_bootstrap_proxy` mirrors `large_alloc_free_cycle`'s role as the
// SeferAlloc bootstrap proxy, per R20-4 §8's flagged nuance: mimalloc's own
// one-time init cost (its first-call thread-local heap setup) is a
// DIFFERENT constant from SeferAlloc's (different allocator, different
// internal bookkeeping) -- subtracting SeferAlloc's `large_alloc_free_cycle`
// constant from a mimalloc row would silently corrupt the marginal-Ir/op
// decomposition. `scripts/iai.mjs` is taught (see BOOTSTRAP_BENCH_BY_PREFIX
// there) to apply THIS proxy's Ir only to `mimalloc_*` rows.

// Small-block (16 B) alloc+dealloc churn via mimalloc -- mirrors
// `small_churn_16b` exactly (same CHURN_OPS, same layout).
#[cfg(target_os = "linux")]
#[library_benchmark]
fn mimalloc_small_churn_16b() {
    let mi = MiMalloc;
    let layout = Layout::from_size_align(16, 8).unwrap();
    for _ in 0..CHURN_OPS {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { mi.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { mi.dealloc(ptr, layout) };
        }
    }
}

// R23-2 (task #371) -- the `2N` sibling of `mimalloc_small_churn_16b`,
// BYTE-IDENTICAL except for the op count (`CHURN_OPS_2N`). Paired with
// `mimalloc_small_churn_16b`'s raw Ir to derive mimalloc's own
// bootstrap-cancelled per-op cost `c = (Ir(2N) - Ir(N)) / N` on the hot
// churn path -- see `docs/perf/R23_2_WARM_N_2N_MIMALLOC_GATE.md`.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn mimalloc_small_churn_16b_2n() {
    let mi = MiMalloc;
    let layout = Layout::from_size_align(16, 8).unwrap();
    for _ in 0..CHURN_OPS_2N {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { mi.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { mi.dealloc(ptr, layout) };
        }
    }
}

// 256 B @ align(8) alloc+dealloc churn via mimalloc -- mirrors `churn_256b`
// exactly.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn mimalloc_churn_256b() {
    let mi = MiMalloc;
    let layout = Layout::from_size_align(256, 8).unwrap();
    for _ in 0..CHURN_OPS {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { mi.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { mi.dealloc(ptr, layout) };
        }
    }
}

// Cold first-touch of tiny 16 B blocks via mimalloc -- mirrors
// `cold_alloc_free_256x16b` exactly (COLD_BATCH distinct blocks allocated,
// then all freed).
#[cfg(target_os = "linux")]
#[library_benchmark]
fn mimalloc_cold_alloc_free_256x16b() {
    let mi = MiMalloc;
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH] = [core::ptr::null_mut(); COLD_BATCH];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { mi.alloc(layout) };
    }
    black_box(&ptrs);
    for &ptr in &ptrs {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by an `alloc` call above with the same
            // layout, and is freed exactly once.
            unsafe { mi.dealloc(ptr, layout) };
        }
    }
}

// R23-2 (task #371) -- the `2N` sibling of `mimalloc_cold_alloc_free_256x16b`,
// BYTE-IDENTICAL except for the batch size (`COLD_BATCH_2N`). Paired with
// `mimalloc_cold_alloc_free_256x16b`'s raw Ir to derive mimalloc's own
// bootstrap-cancelled per-op cost `c = (Ir(2N) - Ir(N)) / N` on the cold-carve
// path -- see `docs/perf/R23_2_WARM_N_2N_MIMALLOC_GATE.md`.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn mimalloc_cold_alloc_free_256x16b_2n() {
    let mi = MiMalloc;
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH_2N] = [core::ptr::null_mut(); COLD_BATCH_2N];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { mi.alloc(layout) };
    }
    black_box(&ptrs);
    for &ptr in &ptrs {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by an `alloc` call above with the same
            // layout, and is freed exactly once.
            unsafe { mi.dealloc(ptr, layout) };
        }
    }
}

// R23-2 (task #371) -- the `4N` sibling, added ONLY for this cold-carve pair,
// purely as a linearity sanity-check -- mirrors `cold_alloc_free_256x16b_4n`
// exactly, via mimalloc.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn mimalloc_cold_alloc_free_256x16b_4n() {
    let mi = MiMalloc;
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH_4N] = [core::ptr::null_mut(); COLD_BATCH_4N];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { mi.alloc(layout) };
    }
    black_box(&ptrs);
    for &ptr in &ptrs {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by an `alloc` call above with the same
            // layout, and is freed exactly once.
            unsafe { mi.dealloc(ptr, layout) };
        }
    }
}

// R24-5 (task #383) -- mimalloc's alloc-only shared prefix, giving mimalloc's
// OWN alloc/free split so the cross-allocator comparison is per-half (alloc
// vs alloc, free vs free), not full-round-only. `free_mi(N) =
// Ir(mimalloc_cold_alloc_free) - Ir(mimalloc_cold_alloc_only)`.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn mimalloc_cold_alloc_only_256x16b() {
    let mi = MiMalloc;
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH] = [core::ptr::null_mut(); COLD_BATCH];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { mi.alloc(layout) };
    }
    black_box(&ptrs);
    // Deliberately leaked (never freed): mimalloc alloc-only shared prefix.
}

// R24-5 (task #383) -- the `2N` sibling of `mimalloc_cold_alloc_only_256x16b`
// (`COLD_BATCH_2N`), for mimalloc's bootstrap-cancelled per-op ALLOC cost.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn mimalloc_cold_alloc_only_256x16b_2n() {
    let mi = MiMalloc;
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH_2N] = [core::ptr::null_mut(); COLD_BATCH_2N];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { mi.alloc(layout) };
    }
    black_box(&ptrs);
    // Deliberately leaked (never freed): mimalloc alloc-only shared prefix.
}

// R24-5 (task #383) -- the `4N` sibling (`COLD_BATCH_4N`), mimalloc's own
// linearity cross-check counterpart to `cold_alloc_only_256x16b_4n`.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn mimalloc_cold_alloc_only_256x16b_4n() {
    let mi = MiMalloc;
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH_4N] = [core::ptr::null_mut(); COLD_BATCH_4N];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { mi.alloc(layout) };
    }
    black_box(&ptrs);
    // Deliberately leaked (never freed): mimalloc alloc-only shared prefix.
}

// Cold first-touch of 64 B blocks via mimalloc -- mirrors
// `cold_alloc_free_256x64b` exactly.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn mimalloc_cold_alloc_free_256x64b() {
    let mi = MiMalloc;
    let layout = Layout::from_size_align(64, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH] = [core::ptr::null_mut(); COLD_BATCH];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { mi.alloc(layout) };
    }
    black_box(&ptrs);
    for &ptr in &ptrs {
        if !ptr.is_null() {
            // SAFETY: ptr was returned by an `alloc` call above with the same
            // layout, and is freed exactly once.
            unsafe { mi.dealloc(ptr, layout) };
        }
    }
}

// Steady-state cold recycle of tiny 16 B blocks via mimalloc -- mirrors
// `recycle_alloc_free_256x16b` exactly (2 rounds: round 1 populates whatever
// free-list mimalloc maintains, round 2 drains it).
#[cfg(target_os = "linux")]
#[library_benchmark]
fn mimalloc_recycle_alloc_free_256x16b() {
    let mi = MiMalloc;
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH] = [core::ptr::null_mut(); COLD_BATCH];
    for _round in 0..2 {
        for slot in ptrs.iter_mut() {
            // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
            *slot = unsafe { mi.alloc(layout) };
        }
        black_box(&ptrs);
        for &ptr in &ptrs {
            if !ptr.is_null() {
                // SAFETY: ptr was returned by an `alloc` call above with the same
                // layout, and is freed exactly once per round.
                unsafe { mi.dealloc(ptr, layout) };
            }
        }
    }
}

// Steady-state cold recycle of 64 B blocks via mimalloc -- mirrors
// `recycle_alloc_free_256x64b` exactly.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn mimalloc_recycle_alloc_free_256x64b() {
    let mi = MiMalloc;
    let layout = Layout::from_size_align(64, 8).unwrap();
    let mut ptrs: [*mut u8; COLD_BATCH] = [core::ptr::null_mut(); COLD_BATCH];
    for _round in 0..2 {
        for slot in ptrs.iter_mut() {
            // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
            *slot = unsafe { mi.alloc(layout) };
        }
        black_box(&ptrs);
        for &ptr in &ptrs {
            if !ptr.is_null() {
                // SAFETY: ptr was returned by an `alloc` call above with the same
                // layout, and is freed exactly once per round.
                unsafe { mi.dealloc(ptr, layout) };
            }
        }
    }
}

// mimalloc bootstrap proxy -- mirrors `large_alloc_free_cycle`'s role
// exactly: a single-shot 4 MiB alloc+free via mimalloc, touching no small-
// class/magazine-equivalent path, so its Ir is (mimalloc's one-time process
// init + one large alloc+free) -- the cleanest mimalloc-specific bootstrap
// constant this bench set can offer (see the R22-15 module note above and
// `scripts/iai.mjs`'s per-prefix bootstrap map).
#[cfg(target_os = "linux")]
#[library_benchmark]
fn mimalloc_bootstrap_proxy() {
    let mi = MiMalloc;
    let layout = Layout::from_size_align(4 * 1024 * 1024, 8).unwrap();
    // SAFETY: layout has non-zero size and valid alignment.
    let ptr = unsafe { mi.alloc(layout) };
    black_box(ptr);
    if !ptr.is_null() {
        // SAFETY: ptr was returned by the `alloc` call directly above with
        // the same layout.
        unsafe { mi.dealloc(ptr, layout) };
    }
}

// R24-8: no-op stubs so `library_benchmark_group!` resolves when `batch-api`
// is absent (`dealloc_batch` does not exist without that feature). These
// produce ~0 Ir and are never the subject of a real measurement run.
#[cfg(all(target_os = "linux", not(feature = "batch-api")))]
#[library_benchmark]
fn dealloc_batch_fresh_16_16b() {
    black_box(0u8);
}

#[cfg(all(target_os = "linux", not(feature = "batch-api")))]
#[library_benchmark]
fn dealloc_batch_fresh_64_16b() {
    black_box(0u8);
}

// R25-7: no-op stubs so `library_benchmark_group!` resolves when `batch-api`
// is absent (same pattern as the 16/64 stubs above).
#[cfg(all(target_os = "linux", not(feature = "batch-api")))]
#[library_benchmark]
fn dealloc_batch_fresh_80_16b() {
    black_box(0u8);
}

#[cfg(all(target_os = "linux", not(feature = "batch-api")))]
#[library_benchmark]
fn dealloc_batch_fresh_81_16b() {
    black_box(0u8);
}

#[cfg(all(target_os = "linux", not(feature = "batch-api")))]
#[library_benchmark]
fn dealloc_batch_fresh_128_16b() {
    black_box(0u8);
}

#[cfg(all(target_os = "linux", not(feature = "batch-api")))]
#[library_benchmark]
fn dealloc_batch_fresh_200_16b() {
    black_box(0u8);
}

#[cfg(all(target_os = "linux", not(feature = "batch-api")))]
#[library_benchmark]
fn dealloc_batch_fresh_512_16b() {
    black_box(0u8);
}

#[cfg(all(target_os = "linux", not(feature = "batch-api")))]
#[library_benchmark]
fn dealloc_batch_fresh_1024_16b() {
    black_box(0u8);
}

// R26-7: no-op stubs for the 4 eager N's (0/1/8/17), same pattern as the stubs
// above (`library_benchmark_group!` must resolve when `batch-api` is absent).
#[cfg(all(target_os = "linux", not(feature = "batch-api")))]
#[library_benchmark]
fn dealloc_batch_fresh_0_16b() {
    black_box(0u8);
}

#[cfg(all(target_os = "linux", not(feature = "batch-api")))]
#[library_benchmark]
fn dealloc_batch_fresh_1_16b() {
    black_box(0u8);
}

#[cfg(all(target_os = "linux", not(feature = "batch-api")))]
#[library_benchmark]
fn dealloc_batch_fresh_8_16b() {
    black_box(0u8);
}

#[cfg(all(target_os = "linux", not(feature = "batch-api")))]
#[library_benchmark]
fn dealloc_batch_fresh_17_16b() {
    black_box(0u8);
}

#[cfg(target_os = "linux")]
library_benchmark_group!(
    name = perf_gate;
    benchmarks =
        small_churn_16b,
        small_churn_16b_2n,
        dealloc_prealloc_only_16b,
        dealloc_free_only_16b,
        dealloc_contains_base_probe_only_16b,
        dealloc_segment_base_of_ptr_probe_only_16b,
        alloc_magazine_prefill_only_16b,
        alloc_magazine_hit_only_16b,
        dealloc_hash_contains_only_probe_16b,
        dealloc_own_thread_body_only_16b,
        dealloc_free_only_16b_n1,
        dealloc_free_only_16b_n8,
        dealloc_free_only_16b_n9,
        dealloc_free_only_16b_n16,
        dealloc_free_only_16b_n17,
        dealloc_free_only_16b_n32,
        dealloc_prealloc_only_1088_16b,
        dealloc_free_only_1088_16b_n17,
        dealloc_free_only_1088_16b_n32,
        dealloc_free_only_1088_16b_n64,
        dealloc_free_only_1088_16b_n256,
        dealloc_free_only_1088_16b_n1024,
        dealloc_realloc_burst_1088_16b_n17,
        oscillating_live_set_16b,
        carve_batch_only_16b,
        carve_batch_only_16b_2n,
        dealloc_batch_fresh_16_16b,
        dealloc_batch_fresh_64_16b,
        dealloc_batch_fresh_80_16b,
        dealloc_batch_fresh_81_16b,
        dealloc_batch_fresh_128_16b,
        dealloc_batch_fresh_200_16b,
        dealloc_batch_fresh_512_16b,
        dealloc_batch_fresh_1024_16b,
        dealloc_batch_fresh_0_16b,
        dealloc_batch_fresh_1_16b,
        dealloc_batch_fresh_8_16b,
        dealloc_batch_fresh_17_16b,
        medium_class_dealloc_churn_16b,
        aligned_churn_640b_a128,
        large_alloc_free_cycle,
        realloc_grow,
        cold_alloc_free_256x16b,
        cold_alloc_free_256x16b_2n,
        cold_alloc_free_256x16b_4n,
        cold_alloc_only_256x16b,
        cold_alloc_only_256x16b_2n,
        cold_alloc_only_256x16b_4n,
        cold_alloc_free_256x64b,
        recycle_alloc_free_256x16b,
        recycle_alloc_only_256x16b,
        recycle_alloc_free_256x64b,
        churn_256b,
        churn_write_256b,
        multiseg_cold_256k,
        seg_cycle_decommit_256k,
        mimalloc_small_churn_16b,
        mimalloc_small_churn_16b_2n,
        mimalloc_churn_256b,
        mimalloc_cold_alloc_free_256x16b,
        mimalloc_cold_alloc_free_256x16b_2n,
        mimalloc_cold_alloc_free_256x16b_4n,
        mimalloc_cold_alloc_only_256x16b,
        mimalloc_cold_alloc_only_256x16b_2n,
        mimalloc_cold_alloc_only_256x16b_4n,
        mimalloc_cold_alloc_free_256x64b,
        mimalloc_recycle_alloc_free_256x16b,
        mimalloc_recycle_alloc_free_256x64b,
        mimalloc_bootstrap_proxy,
);

#[cfg(target_os = "linux")]
main!(library_benchmark_groups = perf_gate);
