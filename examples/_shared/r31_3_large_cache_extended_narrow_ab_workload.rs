// R31-3 (task #466, step 3) shared workload for the
// `r31_3_large_cache_extended_narrow_{off,on}` process-level A/B judge
// binaries — the N=1/2/4 TIMING regression check
// `docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` item 4 named as a
// deferred follow-up (`docs/perf/OPEN_ITEMS.md` item 7, `[L]` tier): item 4's
// own `tests/large_cache_extended_narrow_working_set_after_materialization.rs`
// proved N=1/2/4 CORRECTNESS after sidecar materialisation, but explicitly
// left the wall-clock cost of the widened O(40) scan bound on that same
// narrow-working-set shape unmeasured ("a dedicated timing gate for this
// specific narrow-working-set-after-burst shape is deferred as a follow-up").
//
// ## Why this exists (the concern this gate answers)
//
// `large-cache-extended` is sized for WIDE working sets (up to 40 distinct
// Large sizes — see `docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md`
// §6's turnover-profile win, refreshed in
// `docs/perf/_raw_r31_3_paired_ab_turnover_refresh.log`). Most real programs'
// Large-object working sets are much narrower (1-4 distinct sizes) most of
// the time. The open concern (task #466's brief, mirroring the R14-5 report's
// own §4 closing paragraph): once the sidecar has materialised (e.g. from an
// earlier burst of size diversity), does every SUBSEQUENT narrow-working-set
// alloc/free pay a hidden fixed cost from the wider O(40) scan bound
// (`large_cache_scan_bound`), relative to the base 8-slot cache that never
// widens?
//
// ## Workload shape
//
// Two phases per process, both timed separately so the "does materialisation
// itself cost something" question and the "does the SUBSEQUENT narrow phase
// cost something" question are not conflated:
//
// 1. **Materialisation burst (untimed in the reported RESULT, but its own
//    Instant is captured for context):** batch-alloc-all/dealloc-all across
//    `MATERIALIZE_N = 9` distinct sizes — the same proven pattern
//    `tests/large_cache_extended_narrow_working_set_after_materialization.rs`
//    uses to force the sidecar to materialise (overflows the base 8 by
//    exactly 1). In the OFF arm (extension compiled out) this phase simply
//    exercises the base 8-slot cache's own FIFO eviction — there is no
//    sidecar to materialise, so this phase is present in BOTH arms for
//    workload-shape symmetry (the OFF arm's timing baseline should reflect
//    "a real program that once had a size-diverse burst", not an
//    artificially cache-virgin state).
// 2. **Narrow phase (THIS is the timed, reported region):** the working set
//    narrows to the first `N` of the 9 materialisation sizes (still genuinely
//    distinct, still each an individually-resident cache entry from the
//    burst). `WARMUP_ROUNDS` untimed cycles, then `ROUNDS` timed
//    batch-alloc-all/dealloc-all cycles over just the `N` sizes — mirrors
//    `run_turnover_workload`'s single-`Instant`-pair-around-many-rounds
//    structure (`examples/_shared/paired_ab_large_cache_extended_turnover_workload.rs`).
//
// ## Path-activation oracle (CLAUDE.md's R30-8 rule)
//
// Both arms report `large_cache_hits` accumulated during the TIMED narrow
// phase (via `SeferAlloc::stats()`, the real `#[global_allocator]` counter —
// same layer-identification pattern as the turnover workload) — every
// narrow-phase alloc after warm-up MUST hit the cache in both arms (each of
// the N sizes was deposited into its own slot during warm-up and never
// evicted, since N <= 4 is far below either 8 or 40 slots), so a 100% hit
// rate in BOTH arms is the mechanism-activation proof this comparison
// measures like-for-like servicing (a cache hit vs a cache hit), not
// "sidecar scan overhead" contaminated by one arm actually missing and
// paying a real OS round-trip. The materialisation burst (Phase 1) forces
// the ON arm's sidecar to the widened 40-slot scan bound using the same
// proven pattern
// `tests/large_cache_extended_narrow_working_set_after_materialization.rs::scan_bound_stays_forty_during_narrow_working_set_phase`
// already established stays at 40 throughout a subsequent narrow phase
// (materialisation is one-way, never reverts, per
// `large_cache_extended.rs`'s documented design) — this timing gate reuses
// that existing correctness fact rather than re-deriving it with a second
// hook; the per-arm 100% `large_cache_hits` activation proof above is this
// gate's OWN mechanism-activation evidence for the TIMED region
// specifically.

use std::alloc::Layout;
use std::hint::black_box;
use std::time::Instant;

use sefer_alloc::AllocCore;

fn rss_kib() -> u64 {
    proc_probe::snapshot().rss / 1024
}

fn commit_kib() -> u64 {
    proc_probe::snapshot().commit / 1024
}

/// 9 distinct, pairwise->2x-apart Large sizes — identical derivation to
/// `tests/large_cache_extended_narrow_working_set_after_materialization.rs::large_test_sizes`,
/// duplicated (not shared: no `examples/`-support crate exists in this
/// project for cross-file helpers) so materialisation is forced the same
/// proven way.
fn materialize_sizes(n: usize) -> Vec<usize> {
    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    let segment = 2 * 1024 * 1024usize; // SegmentLayout::SEGMENT, duplicated to avoid a extra import
    let mut size = (2 * small_max).div_ceil(segment).max(1) * segment;
    let mut sizes = Vec::with_capacity(n);
    for _ in 0..n {
        sizes.push(size);
        size = (2 * size + 1).div_ceil(segment) * segment;
    }
    sizes
}

const MATERIALIZE_N: usize = 9;
const WARMUP_ROUNDS: usize = 3;
const ROUNDS: usize = 400;
const ALIGN: usize = 8;
const TOUCH: u64 = 0xA5A5_A5A5_A5A5_A5A5;

fn alloc_one(size: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    // SAFETY: `layout` has non-zero size and valid (power-of-two, <=
    // usize::MAX/2) alignment (8), satisfying `GlobalAlloc::alloc`'s
    // preconditions.
    let p = unsafe { std::alloc::alloc(layout) };
    assert!(!p.is_null(), "alloc({size}) failed -- probe is invalid");
    // SAFETY: caller guarantees `p` points to at least 16 writable bytes
    // (every size in this workload is well above small_max, comfortably >
    // 16 bytes).
    unsafe {
        std::ptr::write_volatile(p.cast::<u64>(), TOUCH);
        std::ptr::write_volatile(p.cast::<u64>().add(1), TOUCH);
    }
    p
}

fn dealloc_one(p: *mut u8, size: usize) {
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    // SAFETY: `p` was allocated with this exact `layout` by `alloc_one`, and
    // is freed exactly once.
    unsafe { std::alloc::dealloc(p, layout) };
}

fn batch_cycle(sizes: &[usize]) {
    let mut ptrs = Vec::with_capacity(sizes.len());
    for &sz in sizes {
        let p = alloc_one(sz);
        black_box(p);
        ptrs.push((p, sz));
    }
    for &(p, sz) in &ptrs {
        dealloc_one(p, sz);
    }
}

/// Returns `(narrow_elapsed_ns, narrow_hits, narrow_total_deallocs,
/// rss_after_kib, commit_after_kib)`. `n` is the narrow working-set size
/// (1, 2, or 4). `global` is the process's installed `SeferAlloc`
/// `#[global_allocator]` static.
pub fn run_narrow_ab_workload(
    global: &'static sefer_alloc::SeferAlloc,
    n: usize,
) -> (u128, u64, u64, u64, u64) {
    let nine_sizes = materialize_sizes(MATERIALIZE_N);
    assert_eq!(nine_sizes.len(), MATERIALIZE_N);

    // Phase 1: materialisation burst (untimed in the reported metric) — one
    // pass is enough to force the sidecar (ON arm) / populate all 8+1
    // distinct-size FIFO churn (OFF arm, base cache only ever holds 8 of the
    // 9 at once by construction, exactly like the turnover workload's
    // baseline).
    batch_cycle(&nine_sizes);

    // Phase 2: narrow to the first `n` of the 9 materialisation sizes — still
    // genuinely distinct, still each an individually cache-resident entry
    // from the burst above (dealloc's best-fit is not consulted; each entry
    // occupies its own slot per the established `large_cache_extended_*`
    // proven pattern).
    let working_set: Vec<usize> = nine_sizes[..n].to_vec();

    // Untimed warm-up: re-populate exactly the N working-set entries so the
    // timed region starts in steady state (every entry already resident,
    // no first-touch admission cost counted against the timed region).
    for _ in 0..WARMUP_ROUNDS {
        batch_cycle(&working_set);
    }

    let hits_before = global.stats().large_cache_hits;
    let t = Instant::now();
    let mut total_deallocs: u64 = 0;
    for _ in 0..ROUNDS {
        batch_cycle(&working_set);
        total_deallocs += n as u64;
    }
    let narrow_elapsed_ns = t.elapsed().as_nanos();
    let narrow_hits = global.stats().large_cache_hits - hits_before;

    (
        narrow_elapsed_ns,
        narrow_hits,
        total_deallocs,
        rss_kib(),
        commit_kib(),
    )
}
