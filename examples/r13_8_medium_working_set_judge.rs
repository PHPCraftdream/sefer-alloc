//! R13-8 (task #278) THROWAWAY measurement harness — NOT a shipping
//! artifact. **MEASUREMENT ONLY, not a design/implementation task.**
//!
//! ## What this judges
//!
//! Two independent questions about a "256-2048 SIMULTANEOUSLY LIVE objects,
//! 260 KiB - 2 MiB each" working set — the scenario named directly in the
//! task brief, which is a distinct scenario from R13-6/R13-7's "few distinct
//! sizes, high TURNOVER" cache workload (see module doc note below, and
//! `docs/perf/R13_8_MEDIUM_WORKING_SET_JUDGE.md` for the full verdict):
//!
//! 1. **Does the `large-cache-extended` sidecar (R13-7, task #277) help
//!    here?** The cache holds only FREED/reusable segments; a purely
//!    "allocate N, hold all N live, then free" workload never deposits
//!    anything into the cache until teardown, so a priori the cache should
//!    be near-irrelevant to a *static* live-set judge — this harness
//!    verifies that expectation empirically (`dbg_large_cache_hits()` read
//!    at the point all N objects are live) rather than asserting it.
//! 2. **Is `MAX_SEGMENTS` (`segment_table.rs`, currently 1024) a real
//!    ceiling for this scenario, and does RSS/commit/wall-clock degrade
//!    non-linearly approaching it?** Every Large allocation — exact-span or
//!    not — consumes exactly one `SegmentTable` slot
//!    (`R12_13_PAGE_RUN_LAYER_DEFERRED.md` §2.1's finding, unchanged since).
//!    2048 simultaneously live Large objects would need 2048 slots against
//!    a 1024 cap — this harness finds the EXACT live count at which
//!    `alloc()` starts returning null due to table exhaustion (not physical
//!    OOM), via `AllocCore::dbg_max_segments()` / `dbg_table_count()`.
//!
//! ## Why sizes are computed, not hardcoded (lesson from task #277)
//!
//! Hardcoding "260 KiB" would silently stop being Large under
//! `--all-features` (`medium-classes-wide` raises `SMALL_MAX` to ~1.75 MiB
//! -- confirmed by direct run: `dbg_small_class_count()`/`dbg_block_size()`
//! give small_max ≈ 252.69 KiB under `production` alone, ≈ 1792.00 KiB
//! under `--all-features`). This harness reads the CURRENT build's
//! `small_max` at runtime and picks its size ladder strictly above it, so
//! "these are Large objects" is true in whichever feature combination the
//! binary was actually built with, never assumed from a comment. The upper
//! bound (2 MiB) is always Large in every shipped feature combination
//! (`SEGMENT` = 4 MiB is the ultimate ceiling on any class table).
//!
//! ## Memory-budget honesty (lesson from task #277's brief itself)
//!
//! 2048 live objects x up to 2 MiB = up to ~4 GiB of REQUESTED bytes, but
//! actual COMMITTED bytes depend on the feature set: WITHOUT
//! `exact-span-large`, every Large object rounds its `usable` span up to a
//! whole `SEGMENT` (4 MiB) regardless of request size, so 2048 live objects
//! commit up to ~8 GiB of VA/RSS -- checked against this host's available
//! physical memory (~17 GiB free at harness-authoring time on this session's
//! host) before running; see `MAX_LIVE_BUDGET_MIB` below and the honest bail
//! path in `alloc_n_live` (an explicit panic message, per the task brief's
//! "no silent `eprintln!`+return" instruction -- NOT a quiet early return).
//! In practice `MAX_SEGMENTS = 1024` is hit (see finding above) well before
//! the physical memory budget would be, so the memory bail is a defensive
//! backstop, not the expected exit path.
//!
//! ## Run (per arm; `alloc-stats` needed for non-zero cache-hit/segment
//! ## counters, same requirement as R13-6/R13-7's harnesses):
//!
//! ```text
//! cargo run --release --example r13_8_medium_working_set_judge --features "production alloc-stats"
//! cargo run --release --example r13_8_medium_working_set_judge --features "production alloc-stats exact-span-large"
//! cargo run --release --example r13_8_medium_working_set_judge --features "production alloc-stats exact-span-large large-cache-extended"
//! ```

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};
use std::time::Instant;

/// Upper bound on how much committed memory this harness will attempt to
/// drive, in MiB. 2048 live objects at up to 4 MiB committed span each
/// (whole-SEGMENT rounding, the worst case without `exact-span-large`) is
/// 8192 MiB. This host has tens of GiB free; the constant is a defensive
/// backstop against running on a memory-starved host, not an expected limit
/// (see module doc).
const MAX_LIVE_BUDGET_MIB: usize = 12_288; // 12 GiB

/// The live-object counts the task brief names explicitly.
const LIVE_COUNTS: [usize; 4] = [256, 512, 1024, 2048];

/// Build a size ladder of `n` distinct Large sizes, strictly above the
/// CURRENT build's Small/Large boundary, spanning up to ~2 MiB. Computed at
/// runtime (not hardcoded) so it stays correct under any feature
/// combination that shifts `SMALL_MAX` (see module doc).
fn large_size_ladder(n: usize) -> Vec<usize> {
    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    // Start comfortably above small_max (the task brief's own "260 KiB"
    // lower bound only makes sense verbatim under `production`; under
    // `--all-features` we must start above ~1.75 MiB instead -- computed,
    // not assumed).
    let lo = (small_max + small_max / 16).max(small_max + 4096);
    let hi = 2 * 1024 * 1024usize; // 2 MiB, the task brief's stated upper bound
    let hi = hi.max(lo + n * 64); // guarantee room for n distinct steps
    let mut sizes = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64 / (n.max(2) - 1) as f64;
        let size = lo + ((hi - lo) as f64 * t) as usize;
        sizes.push(size);
    }
    sizes
}

fn rss_kib() -> u64 {
    proc_probe::snapshot().rss / 1024
}
fn commit_kib() -> u64 {
    proc_probe::snapshot().commit / 1024
}

/// Result of attempting to bring `target` objects simultaneously live.
struct LiveRunResult {
    /// How many objects were ACTUALLY brought live before either reaching
    /// `target` or hitting a graceful OOM (table-full or physical).
    achieved: usize,
    /// `true` if the run stopped early because `alloc()` returned null
    /// (either `SegmentTable` exhaustion or a real OS reservation failure
    /// -- distinguished via `dbg_table_count()` vs `dbg_max_segments()`).
    stopped_by_null_alloc: bool,
    rss_kib_at_peak: u64,
    commit_kib_at_peak: u64,
    segments_reserved_delta: u64,
    segments_released_delta: u64,
    alloc_wall_clock: std::time::Duration,
}

/// Allocate up to `target` simultaneously live Large objects (round-robin
/// over `sizes`), stopping early on a graceful null (table-full or OS OOM).
/// Returns the pointers/layouts actually allocated (for later dealloc) plus
/// a `LiveRunResult` summary. Honest bail: if `target * max(sizes)` would
/// exceed `MAX_LIVE_BUDGET_MIB`, this panics with a clear message rather
/// than silently truncating (per the task brief's explicit instruction).
fn alloc_n_live(
    ac: &mut AllocCore,
    target: usize,
    sizes: &[usize],
) -> (Vec<(*mut u8, Layout)>, LiveRunResult) {
    let worst_case_mib = (target.saturating_mul(*sizes.iter().max().unwrap_or(&0))) / (1024 * 1024);
    assert!(
        worst_case_mib <= MAX_LIVE_BUDGET_MIB,
        "r13_8 honest bail: target={target} objects x max size {} B would need up to \
         {worst_case_mib} MiB, exceeding this harness's MAX_LIVE_BUDGET_MIB={MAX_LIVE_BUDGET_MIB} \
         safety backstop -- refusing to run rather than risk host OOM. Lower LIVE_COUNTS \
         or raise the budget deliberately if this host has the headroom.",
        sizes.iter().max().unwrap_or(&0)
    );

    let segments_reserved_before = AllocCore::dbg_segments_reserved_total();
    let segments_released_before = AllocCore::dbg_segments_released_total();

    let mut ptrs = Vec::with_capacity(target);
    let start = Instant::now();
    let mut stopped_by_null_alloc = false;
    for i in 0..target {
        let size = sizes[i % sizes.len()];
        let layout = Layout::from_size_align(size, 8).unwrap();
        let p = ac.alloc(layout);
        if p.is_null() {
            stopped_by_null_alloc = true;
            break;
        }
        // Touch first and last byte so the page is genuinely resident (not
        // just reserved) -- a fair RSS/commit measurement.
        // SAFETY: `p` is a freshly returned, non-null allocation of `size`
        // bytes from the `alloc` call immediately above; writing within
        // `[0, size)` is in-bounds.
        unsafe {
            p.write_bytes(0xA5, 1);
            p.add(size - 1).write_bytes(0xA5, 1);
        }
        ptrs.push((p, layout));
    }
    let alloc_wall_clock = start.elapsed();

    let rss_kib_at_peak = rss_kib();
    let commit_kib_at_peak = commit_kib();
    let segments_reserved_delta =
        AllocCore::dbg_segments_reserved_total() - segments_reserved_before;
    let segments_released_delta =
        AllocCore::dbg_segments_released_total() - segments_released_before;

    let result = LiveRunResult {
        achieved: ptrs.len(),
        stopped_by_null_alloc,
        rss_kib_at_peak,
        commit_kib_at_peak,
        segments_reserved_delta,
        segments_released_delta,
        alloc_wall_clock,
    };
    (ptrs, result)
}

fn dealloc_all(ac: &mut AllocCore, ptrs: Vec<(*mut u8, Layout)>) -> std::time::Duration {
    let start = Instant::now();
    for (p, layout) in ptrs {
        // SAFETY: `p`/`layout` are exactly the pair returned by the matching
        // `alloc` call in `alloc_n_live`, freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
    start.elapsed()
}

fn main() {
    let exact_span = cfg!(feature = "exact-span-large");
    let cache_extended = cfg!(feature = "large-cache-extended");
    let alloc_stats = cfg!(feature = "alloc-stats");

    println!("=== R13-8 medium (260 KiB - 2 MiB) working-set judge ===");
    println!("feature exact-span-large:    {exact_span}");
    println!("feature large-cache-extended: {cache_extended}");
    println!("feature alloc-stats:         {alloc_stats}");
    if !alloc_stats {
        println!(
            "WARNING: alloc-stats is OFF -- dbg_large_cache_hits()/dbg_segments_*_total() \
             cache-hit-specific increments may read 0. Segment reserve/release counters are \
             NOT gated on alloc-stats (see dbg_segments_reserved_total's doc) and remain valid."
        );
    }

    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    let max_segments = AllocCore::dbg_max_segments();
    println!(
        "current build: small_max = {small_max} B ({:.2} KiB), SEGMENT = {} B ({} MiB), \
         MAX_SEGMENTS = {max_segments}",
        small_max as f64 / 1024.0,
        SegmentLayout::SEGMENT,
        SegmentLayout::SEGMENT / (1024 * 1024)
    );
    println!();

    // ------------------------------------------------------------------
    // Part A: scale sweep at 256/512/1024/2048 live objects.
    // ------------------------------------------------------------------
    println!("--- Part A: live-object scale sweep ---");
    let rss_idle = rss_kib();
    let commit_idle = commit_kib();
    println!("idle rss_kib={rss_idle} commit_kib={commit_idle}");
    println!();

    for &target in &LIVE_COUNTS {
        let sizes = large_size_ladder(32.min(target));
        let mut ac = AllocCore::new().expect("primordial");
        ac.dbg_set_large_cache_budget(None);

        let (ptrs, r) = alloc_n_live(&mut ac, target, &sizes);
        let cache_hits_at_peak = ac.dbg_large_cache_hits();

        println!(
            "target={target:5} achieved={:5} stopped_by_null_alloc={} \
             alloc_wall_clock={:?} ({:.1} us/op)",
            r.achieved,
            r.stopped_by_null_alloc,
            r.alloc_wall_clock,
            r.alloc_wall_clock.as_micros() as f64 / r.achieved.max(1) as f64
        );
        println!(
            "  rss_kib={} (delta_from_idle={}) commit_kib={} (delta_from_idle={})",
            r.rss_kib_at_peak,
            r.rss_kib_at_peak.saturating_sub(rss_idle),
            r.commit_kib_at_peak,
            r.commit_kib_at_peak.saturating_sub(commit_idle)
        );
        println!(
            "  segments_reserved_delta={} segments_released_delta={} \
             (expect reserved≈achieved, released≈0 -- nothing freed yet)",
            r.segments_reserved_delta, r.segments_released_delta
        );
        println!(
            "  large_cache_hits at peak-live (expect 0 -- static live set never \
             deposits/reuses): {cache_hits_at_peak}"
        );
        println!(
            "  table_count={} of MAX_SEGMENTS={}",
            ac.dbg_table_count(),
            max_segments
        );

        let dealloc_elapsed = dealloc_all(&mut ac, ptrs);
        println!(
            "  dealloc_wall_clock={:?} ({:.1} us/op)",
            dealloc_elapsed,
            dealloc_elapsed.as_micros() as f64 / r.achieved.max(1) as f64
        );
        println!();
    }

    // ------------------------------------------------------------------
    // Part B: EXACT MAX_SEGMENTS ceiling probe -- push past 1024 live
    // objects and find the precise count where alloc() starts returning
    // null due to table exhaustion (not physical OOM).
    // ------------------------------------------------------------------
    println!("--- Part B: exact SegmentTable-capacity ceiling probe ---");
    {
        let probe_target = max_segments + 64; // deliberately push past the cap
        let sizes = large_size_ladder(32);
        let mut ac = AllocCore::new().expect("primordial");
        ac.dbg_set_large_cache_budget(None);
        let (ptrs, r) = alloc_n_live(&mut ac, probe_target, &sizes);
        println!(
            "probe_target={probe_target} (MAX_SEGMENTS+64) achieved={} stopped_by_null_alloc={}",
            r.achieved, r.stopped_by_null_alloc
        );
        println!(
            "  table_count at stop = {} (MAX_SEGMENTS = {})",
            ac.dbg_table_count(),
            max_segments
        );
        assert!(
            r.achieved <= max_segments,
            "r13_8: achieved {} live objects, MORE than MAX_SEGMENTS={} -- either the table \
             grew, or a cache/recycle path let a live object NOT consume a slot; this would \
             contradict R12_13_PAGE_RUN_LAYER_DEFERRED.md section 2.1's finding and needs \
             investigation, not silent accept.",
            r.achieved,
            max_segments
        );
        if r.stopped_by_null_alloc && r.achieved == max_segments {
            println!(
                "  CONFIRMED: alloc() returns null EXACTLY at MAX_SEGMENTS={max_segments} live \
                 Large objects in this size range -- a real, demonstrated ceiling for this \
                 workload shape."
            );
        }
        dealloc_all(&mut ac, ptrs);
    }
    println!();

    // ------------------------------------------------------------------
    // Part C: TURNOVER judge -- the shape the large-cache/large-cache-
    // extended sidecar actually targets (repeated alloc/free of a working
    // set of DISTINCT sizes), run in THIS task's 260 KiB - 2 MiB range, to
    // give Question 1 concrete numbers instead of only the a-priori
    // argument that a static live set can never benefit. 24 distinct sizes
    // deliberately exceeds the base LARGE_CACHE_SLOTS=8, so a working set
    // this wide is exactly where R13-7's extension is designed to help --
    // if it helps anywhere, it should show up here.
    // ------------------------------------------------------------------
    println!("--- Part C: turnover judge (24 distinct sizes, batch cycles) ---");
    {
        let sizes = large_size_ladder(24);
        let layouts: Vec<Layout> = sizes
            .iter()
            .map(|&b| Layout::from_size_align(b, 8).unwrap())
            .collect();
        let mut ac = AllocCore::new().expect("primordial");
        ac.dbg_set_large_cache_budget(None);

        // Warm-up: populate the cache with all sizes once (batched, per the
        // R13-7 lesson: round-robin single-flight lets FIFO eviction
        // collapse distinct sizes before the extension ever materialises).
        {
            let mut ptrs = Vec::with_capacity(layouts.len());
            for &l in &layouts {
                let p = ac.alloc(l);
                assert!(!p.is_null(), "OOM during Part C warm-up");
                ptrs.push(p);
            }
            for (i, &p) in ptrs.iter().enumerate() {
                // SAFETY: `p` is a live allocation from the warm-up loop
                // immediately above, freed exactly once here.
                unsafe { ac.dealloc(p, layouts[i]) };
            }
        }

        const CYCLES: usize = 200;
        let hits_before = ac.dbg_large_cache_hits();
        let start = Instant::now();
        let mut total_deallocs: u64 = 0;
        for _ in 0..CYCLES {
            let mut ptrs = Vec::with_capacity(layouts.len());
            for &l in &layouts {
                let p = ac.alloc(l);
                assert!(!p.is_null(), "OOM during Part C measured cycle");
                ptrs.push(p);
            }
            for (i, &p) in ptrs.iter().enumerate() {
                // SAFETY: `p` is a live allocation from the batch alloc loop
                // immediately above, freed exactly once here.
                unsafe { ac.dealloc(p, layouts[i]) };
                total_deallocs += 1;
            }
        }
        let elapsed = start.elapsed();
        let hits = ac.dbg_large_cache_hits() - hits_before;
        let hit_rate_pct = 100.0 * hits as f64 / total_deallocs.max(1) as f64;

        println!(
            "sizes={} cycles={CYCLES} total_alloc_dealloc_pairs={total_deallocs}",
            sizes.len()
        );
        println!(
            "hits={hits} hit_rate={hit_rate_pct:.2}% wall_clock={elapsed:?} \
             ({:.1} ns/op)",
            elapsed.as_nanos() as f64 / total_deallocs.max(1) as f64
        );
        println!(
            "base slot occupancy (8): {}",
            ac.dbg_large_cache_slot_sizes()
                .iter()
                .filter(|s| s.is_some())
                .count()
        );
        #[cfg(feature = "large-cache-extended")]
        {
            println!(
                "extension materialised: {} extension slots used: {} total slots: {}",
                ac.dbg_large_cache_extension_materialised(),
                ac.dbg_large_cache_extended_slot_sizes()
                    .iter()
                    .filter(|s| s.is_some())
                    .count(),
                ac.dbg_large_cache_total_slots()
            );
        }
        #[cfg(not(feature = "large-cache-extended"))]
        {
            println!(
                "extension not compiled in this arm -- base 8 slots only for {} distinct sizes",
                sizes.len()
            );
        }
    }
    println!();

    println!("=== done ===");
}
