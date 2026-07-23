//! R13-7 (task #277) THROWAWAY measurement harness — NOT a shipping artifact.
//!
//! Exact-count judge for the `large-cache-extended` sidecar
//! (`src/alloc_core/large_cache_extended.rs`): a FIXED, KNOWN number of
//! BATCH alloc-all/dealloc-all/realloc-all cycles over 9 distinct Large
//! sizes, exact `hits / total_deallocs` percentage via
//! `AllocCore::dbg_large_cache_hits()`.
//!
//! ## Why BATCH, not round-robin (self-caught measurement bug)
//!
//! An earlier version of this file used a round-robin pattern (alloc one
//! size, dealloc it immediately, next size, repeat) over 24 sizes chosen to
//! be "pairwise > 2x apart". That claim was WRONG — with 24 sizes packed
//! into a realistic sub-4MiB-to-100MiB range, many non-adjacent pairs still
//! land within `LARGE_CACHE_SIZE_FACTOR`(2x) of each other (e.g. 150 KiB and
//! 190 KiB, both present in that list, satisfy `190 <= 150 * 2`). Best-fit
//! (`alloc_core_large.rs`'s hit-scan picks the SMALLEST satisfying
//! `usable_size`) means an exact self-match always wins once an entry is
//! resident, but round-robin's single-flight order let the 8-slot BASE
//! array's FIFO eviction collapse many of the 24 nominal sizes onto shared
//! effective bands before their next round — measured result: identical
//! 91.54% hit rate and only 6 occupied slots in BOTH the extension-off and
//! extension-on arms, i.e. the extension sidecar never even materialised.
//! That is a flaw in the harness's request ORDERING, not in the cache
//! mechanism (which the two `tests/large_cache_extended_*.rs` files
//! independently verify is correct via a real overflow).
//!
//! The fix: BATCH the workload — allocate all N sizes first (they cannot
//! steal each other's cache entries while nothing has been deposited yet),
//! deallocate all N (every one gets its OWN cache deposit, since best-fit
//! matching only runs on `alloc`, never on `dealloc`/deposit — matches
//! `tests/large_cache_extended_materializes_on_overflow.rs`'s proven
//! pattern), THEN re-allocate all N (each request's own EXACT prior deposit
//! is always the tightest best-fit match, so self-hits are guaranteed
//! regardless of what else is resident), then deallocate all N again to
//! close the cycle. Repeated `CYCLES` times for a wall-clock signal.
//!
//! Sizes: the SAME runtime-computed list
//! `large_cache_extended_materializes_on_overflow.rs::large_test_sizes`
//! derives (see that file's module doc for the full derivation and its
//! two-fold correction history: an original up-to-1-TiB fixed list silently
//! OOM'd, and a first fixed-but-realistic list broke under `--all-features`
//! because `medium-classes-wide` raises the Small/Large boundary well past
//! several of its values) — duplicated here (not shared; no test-support
//! crate exists for `tests/`+`examples/` common code) rather than inventing
//! a third fixed list.
//!
//! Run once per arm (same discipline as `r13_6_large_cache_hit_rate_measure.rs`):
//!   BEFORE (8 slots only):  cargo run --release --example r13_7_large_cache_extended_hit_rate_measure --features "alloc-core alloc-decommit alloc-stats"
//!   AFTER  (8+32 slots):    cargo run --release --example r13_7_large_cache_extended_hit_rate_measure --features "alloc-core alloc-decommit alloc-stats large-cache-extended"
//!
//! `alloc-stats` is required for a non-zero readout — see
//! `r13_6_large_cache_hit_rate_measure.rs`'s identical note.

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};
use std::time::Instant;

/// Verbatim copy of
/// `large_cache_extended_materializes_on_overflow.rs::large_test_sizes` —
/// see that file's module doc for the full derivation rationale.
fn large_test_sizes() -> Vec<usize> {
    let segment = SegmentLayout::SEGMENT;
    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    let mut n = (2 * small_max).div_ceil(segment).max(1) * segment;
    let mut sizes = Vec::with_capacity(9);
    for _ in 0..9 {
        sizes.push(n);
        n = (2 * n + 1).div_ceil(segment) * segment;
    }
    sizes
}

/// Batch alloc-all/dealloc-all/realloc-all/dealloc-all cycles — large enough
/// that the first cycle's unavoidable cold misses are a small fraction of
/// the total.
const CYCLES: usize = 200;

fn main() {
    let extended = cfg!(feature = "large-cache-extended");
    let alloc_stats = cfg!(feature = "alloc-stats");
    println!("=== R13-7 large-cache-extended hit-rate + wall-clock judge ===");
    println!(
        "feature large-cache-extended: {}",
        if extended {
            "ON (8+32=40 slots)"
        } else {
            "OFF (8 slots only)"
        }
    );
    println!(
        "feature alloc-stats:          {}",
        if alloc_stats { "ON" } else { "OFF" }
    );
    if !alloc_stats {
        println!(
            "WARNING: alloc-stats is OFF -- dbg_large_cache_hits() will read 0 \
             regardless of actual cache behaviour. Re-run with alloc-stats added."
        );
    }
    let sizes = large_test_sizes();
    println!(
        "sizes: {} distinct Large sizes, {CYCLES} batch alloc-all/dealloc-all cycles ({} total ops)",
        sizes.len(),
        sizes.len() * CYCLES
    );
    println!();

    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None); // isolate slot-count effect from byte-budget

    let layouts: Vec<Layout> = sizes
        .iter()
        .map(|&bytes| Layout::from_size_align(bytes, 8).unwrap())
        .collect();

    // Warm-up batch (outside the timed region): populate the cache with all
    // 9 sizes once, so the timed loop's very first cycle is already
    // measuring hits, not the unavoidable cold-carve misses.
    {
        let mut ptrs = Vec::with_capacity(layouts.len());
        for &l in &layouts {
            let p = ac.alloc(l);
            assert!(
                !p.is_null(),
                "OOM during warm-up -- this list totals well under 1 GiB \
                 simultaneous, an OOM here indicates a genuinely memory-starved host"
            );
            ptrs.push(p);
        }
        for (i, &p) in ptrs.iter().enumerate() {
            // SAFETY (R6-MS-1/2): `p` is a live allocation from the warm-up
            // loop immediately above, freed exactly once here.
            unsafe { ac.dealloc(p, layouts[i]) };
        }
    }

    let hits_before_measured_loop = ac.dbg_large_cache_hits();
    let start = Instant::now();
    let mut total_deallocs: u64 = 0;
    for _ in 0..CYCLES {
        // Batch re-allocate all 9 -- each request's own EXACT prior deposit
        // is always the tightest best-fit match (see module doc), so this is
        // a guaranteed self-hit regardless of allocation order within the
        // batch.
        let mut ptrs = Vec::with_capacity(layouts.len());
        for &l in &layouts {
            let p = ac.alloc(l);
            assert!(!p.is_null(), "OOM re-allocating during measured cycle");
            std::hint::black_box(p);
            ptrs.push(p);
        }
        // Batch deallocate all 9 -- deposits each back into the cache
        // (deposit never consults best-fit, so no cross-size interference).
        for (i, &p) in ptrs.iter().enumerate() {
            // SAFETY (R6-MS-1/2): `p` is a live allocation from the batch
            // alloc loop immediately above, freed exactly once here.
            unsafe { ac.dealloc(p, layouts[i]) };
            total_deallocs += 1;
        }
    }
    let elapsed = start.elapsed();

    let hits = ac.dbg_large_cache_hits() - hits_before_measured_loop;
    let hit_rate_pct = if total_deallocs > 0 {
        100.0 * hits as f64 / total_deallocs as f64
    } else {
        0.0
    };
    let ns_per_op = elapsed.as_nanos() as f64 / total_deallocs as f64;

    println!("total alloc+dealloc cycles: {total_deallocs}");
    println!("large_cache_hits (exact):   {hits}");
    println!("hit rate:                   {hit_rate_pct:.2}%");
    println!("wall-clock total:           {elapsed:?}");
    println!("wall-clock per op (alloc+dealloc pair): {ns_per_op:.1} ns");
    println!(
        "final large_cache_used bytes: {}",
        ac.dbg_large_cache_used()
    );
    println!(
        "base slot occupancy (8):     {:?}",
        ac.dbg_large_cache_slot_sizes()
            .iter()
            .filter(|s| s.is_some())
            .count()
    );
    #[cfg(feature = "large-cache-extended")]
    {
        println!(
            "extension materialised:      {}",
            ac.dbg_large_cache_extension_materialised()
        );
        println!(
            "extension slot occupancy (32): {}",
            ac.dbg_large_cache_extended_slot_sizes()
                .iter()
                .filter(|s| s.is_some())
                .count()
        );
        println!(
            "total slots available now:   {}",
            ac.dbg_large_cache_total_slots()
        );
    }
}
