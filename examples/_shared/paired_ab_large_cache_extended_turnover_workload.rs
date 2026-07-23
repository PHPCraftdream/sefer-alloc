// R14-5 (task #290, item 6) shared TURNOVER workload for the
// `paired_ab_large_cache_extended_{off,on}` process-level A/B/B/A judge
// binaries.
//
// ## Why this file exists (and why it is `include!`d, not a real module)
//
// R13-8 Part C ("turnover judge") measured, in-process, that a 24-distinct-
// size batch alloc/dealloc cycle goes from 33.33% cache hit rate (base 8
// slots) to 100% (40-slot extension) — a real, large win, but that
// measurement never went through this project's own paired A/B/B/A
// process-level protocol (`scripts/paired-ab-runner.mjs`), which is the
// standard this project's `docs/perf/R14_3_*`/`R14_4_*` gates established
// for a PRODUCTION promotion decision. This file is that missing piece for
// `large-cache-extended`: the SAME turnover shape R13-8 Part C used
// (batch-alloc-all / batch-dealloc-all over N distinct sizes, repeated),
// wired through the paired-ab-runner so the promotion recommendation in
// `docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` is backed by the
// same statistical protocol (paired t-test + sign test, A/B/B/A ordering)
// every other Round 14 production-gate decision cites.
//
// `include!` (not a shared crate module) mirrors every other
// `paired_ab_*_workload.rs` file in this directory: Cargo examples are
// independent compilation units with no shared `examples/`-support crate in
// this project, and duplicating the workload body across two wrappers would
// risk the two-binaries-silently-drift-apart failure mode. Both wrappers
// (`paired_ab_large_cache_extended_off.rs`,
// `paired_ab_large_cache_extended_on.rs`) `include!` this file verbatim —
// the ONLY difference between the two binaries is which Cargo feature set
// each is compiled with (`production alloc-stats` vs `production alloc-stats
// large-cache-extended`).
//
// ## R13-8's own honest caveat, inherited here (R13-8 §0, "Question 1")
//
// A STATIC live-set workload deposits nothing into the Large cache until
// teardown — `large_cache_hits` reads 0 regardless of the extension, so a
// paired A/B session built on a static-live-set workload would be
// methodologically empty (measuring noise, not the extension). This
// workload is explicitly the TURNOVER shape (batch alloc-all, batch
// dealloc-all, repeat) R13-8 itself identified as the scenario where the
// extension actually does something — NOT the peak-live-object scenario.
//
// ## Workload shape
//
// `N_DISTINCT = 24` distinct Large sizes (matches R13-8 Part C exactly —
// enough to overflow the base 8 slots by 16, deep into the extension's own
// range, without needing the full 40-slot ceiling). Sizes are computed at
// runtime relative to the CURRENT build's actual Small/Large boundary
// (`AllocCore::dbg_small_class_count`/`dbg_block_size`), density-agnostic
// under `medium-classes-wide` per the R12-14/R13-7 established convention —
// see `examples/r13_8_medium_working_set_judge.rs`'s `large_size_ladder` for
// the identical derivation this file's `turnover_size_ladder` mirrors.
//
// `WARMUP_ROUNDS` untimed batch cycles populate the cache to steady state,
// then ONE `Instant` pair times `ROUNDS` batch alloc-all/dealloc-all cycles
// — the SAME single-Instant-pair-around-many-rounds structure
// `paired_ab_large_cache_workload.rs` uses, so timing overhead stays
// negligible against the timed region.

use std::alloc::Layout;
use std::hint::black_box;
use std::time::Instant;

use sefer_alloc::AllocCore;
// `SeferAlloc` is imported by each `include!`ing wrapper binary already
// (`paired_ab_large_cache_extended_{off,on}.rs`) — not re-imported here to
// avoid an E0252 duplicate-import error under `include!`'s textual splice.

fn rss_kib() -> u64 {
    proc_probe::snapshot().rss / 1024
}

fn commit_kib() -> u64 {
    proc_probe::snapshot().commit / 1024
}

/// 24 distinct Large sizes spanning the CURRENT build's actual Small/Large
/// boundary up to 2 MiB — verbatim-equivalent derivation to
/// `examples/r13_8_medium_working_set_judge.rs::large_size_ladder`
/// (duplicated, not shared: no `examples/`-support crate exists in this
/// project for cross-file helpers).
fn turnover_size_ladder(n: usize) -> Vec<usize> {
    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    let lo = (small_max + small_max / 16).max(small_max + 4096);
    let hi = 2 * 1024 * 1024usize; // 2 MiB
    let hi = hi.max(lo + n * 64);
    let mut sizes = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64 / (n.max(2) - 1) as f64;
        let size = lo + ((hi - lo) as f64 * t) as usize;
        sizes.push(size);
    }
    sizes
}

const N_DISTINCT: usize = 24;
const WARMUP_ROUNDS: usize = 3;
const ROUNDS: usize = 200;
const ALIGN: usize = 8;
const TOUCH: u64 = 0xA5A5_A5A5_A5A5_A5A5;

fn alloc_one(size: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    // SAFETY: `layout` has non-zero size and valid (power-of-two, <=
    // usize::MAX/2) alignment (8), satisfying `GlobalAlloc::alloc`'s
    // preconditions.
    let p = unsafe { std::alloc::alloc(layout) };
    assert!(!p.is_null(), "alloc({size}) failed -- probe is invalid");
    // SAFETY: `p` points to at least ALIGN=8 <= size bytes (every ladder
    // entry is well above 8 bytes), so the first 16 bytes are writable for
    // every size this workload uses (all >= small_max, comfortably > 16).
    unsafe {
        std::ptr::write_volatile(p.cast::<u64>(), TOUCH);
        std::ptr::write_volatile(p.cast::<u64>().add(1), TOUCH);
    }
    p
}

fn dealloc_one(p: *mut u8, size: usize) {
    let layout = Layout::from_size_align(size, ALIGN).unwrap();
    // SAFETY: `p` was allocated with this exact `layout` (same size, same
    // align) by `alloc_one`, and is freed exactly once.
    unsafe { std::alloc::dealloc(p, layout) };
}

/// Returns `(elapsed_ns, hits, total_deallocs, rss_after_kib, commit_after_kib)`.
/// `global` is the process's installed `SeferAlloc` `#[global_allocator]`
/// static — passed in so this shared file does not need to know the
/// wrapper's static's name (each wrapper declares its own `GLOBAL`, mirroring
/// `paired_ab_large_cache_{off,on}.rs`'s existing pattern).
pub fn run_turnover_workload(global: &'static SeferAlloc) -> (u128, u64, u64, u64, u64) {
    let sizes = turnover_size_ladder(N_DISTINCT);

    // Warm-up (untimed): populate the cache with all N_DISTINCT sizes.
    for _ in 0..WARMUP_ROUNDS {
        let mut ptrs = Vec::with_capacity(sizes.len());
        for &sz in &sizes {
            ptrs.push((alloc_one(sz), sz));
        }
        for &(p, sz) in &ptrs {
            dealloc_one(p, sz);
        }
    }

    let hits_before = global.stats().large_cache_hits;
    let t = Instant::now();
    let mut total_deallocs: u64 = 0;
    for _ in 0..ROUNDS {
        let mut ptrs = Vec::with_capacity(sizes.len());
        for &sz in &sizes {
            let p = alloc_one(sz);
            black_box(p);
            ptrs.push((p, sz));
        }
        for &(p, sz) in &ptrs {
            dealloc_one(p, sz);
            total_deallocs += 1;
        }
    }
    let elapsed_ns = t.elapsed().as_nanos();
    let hits = global.stats().large_cache_hits - hits_before;

    (elapsed_ns, hits, total_deallocs, rss_kib(), commit_kib())
}
