//! R13-6 (task #276) THROWAWAY measurement harness — NOT a shipping artifact.
//!
//! Exact-count companion to `benches/r13_6_exact_span_reserved_capacity_wallclock.rs`'s
//! `r13_6_large_cache_cycle` group: criterion's `Bencher::iter` runs an
//! UNKNOWN, black-box-chosen number of iterations per sample (warm-up +
//! measured), so `AllocCore::dbg_large_cache_hits()` read after a criterion
//! group only gives an approximate lower bound on the true hit rate, not an
//! exact percentage. This harness runs a FIXED, KNOWN number of round-robin
//! alloc+dealloc cycles across the same four sub-4-MiB control sizes
//! `r12_3_exact_span_measure.rs` uses (260 KiB, 512 KiB, 1 MiB, 1.75 MiB) and
//! reports an EXACT hit rate: `hits / total_dealloc_deposits`.
//!
//! Run once per feature combination (same discipline as
//! `r12_4_reserved_capacity_measure.rs`):
//!   cargo run --release --example r13_6_large_cache_hit_rate_measure --features "production alloc-stats"
//!   cargo run --release --example r13_6_large_cache_hit_rate_measure --features "production alloc-stats exact-span-large large-reserved-capacity"
//!
//! `alloc-stats` is required for a non-zero readout: `dbg_large_cache_hits()`'s
//! INCREMENT (not the accessor) is gated behind `#[cfg(feature =
//! "alloc-stats")]` (`alloc_core_large.rs`'s cache-hit branch) — without it
//! this harness always prints 0 hits regardless of actual cache behaviour.

use core::alloc::Layout;
use sefer_alloc::AllocCore;

const KIB: usize = 1024;
const MIB: usize = 1024 * 1024;

/// Mirrors `r12_3_exact_span_measure.rs`'s four sub-4-MiB control sizes.
const SIZES: [usize; 4] = [260 * KIB, 512 * KIB, MIB, (7 * MIB) / 4];

/// Round-robin passes over `SIZES` — large enough that the first pass's
/// unavoidable cold misses are a small fraction of the total.
const PASSES: usize = 2000;

fn main() {
    let exact_span = cfg!(feature = "exact-span-large");
    let reserved_cap = cfg!(feature = "large-reserved-capacity");
    let alloc_stats = cfg!(feature = "alloc-stats");
    println!("=== R13-6 large-cache exact hit-rate measurement ===");
    println!(
        "feature exact-span-large:        {}",
        if exact_span { "ON" } else { "OFF" }
    );
    println!(
        "feature large-reserved-capacity: {}",
        if reserved_cap { "ON" } else { "OFF" }
    );
    println!(
        "feature alloc-stats:             {}",
        if alloc_stats { "ON" } else { "OFF" }
    );
    if !alloc_stats {
        println!(
            "WARNING: alloc-stats is OFF -- dbg_large_cache_hits() will read 0 \
             regardless of actual cache behaviour (the increment is compiled \
             out, not \"no hits occurred\"). Re-run with alloc-stats added."
        );
    }
    println!("sizes: {SIZES:?} bytes, {PASSES} round-robin passes");
    println!();

    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    let mut total_deallocs: u64 = 0;
    for _ in 0..PASSES {
        for &size in &SIZES {
            let layout = Layout::from_size_align(size, 8).unwrap();
            let ptr = ac.alloc(layout);
            assert!(!ptr.is_null(), "OOM allocating {size} bytes");
            std::hint::black_box(ptr);
            // SAFETY (R6-MS-1/2): `ptr` is a live allocation from this
            // AllocCore made with `layout`, freed exactly once here.
            unsafe { ac.dealloc(ptr, layout) };
            total_deallocs += 1;
        }
    }

    let hits = ac.dbg_large_cache_hits();
    let hit_rate_pct = if total_deallocs > 0 {
        100.0 * hits as f64 / total_deallocs as f64
    } else {
        0.0
    };
    println!("total alloc+dealloc cycles: {total_deallocs}");
    println!("large_cache_hits (exact):   {hits}");
    println!("hit rate:                   {hit_rate_pct:.2}%");
    println!(
        "final large_cache_used bytes: {} (slot occupancy: {:?})",
        ac.dbg_large_cache_used(),
        ac.dbg_large_cache_slot_sizes()
    );
}
