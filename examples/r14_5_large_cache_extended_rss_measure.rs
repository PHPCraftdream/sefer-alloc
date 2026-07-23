//! R14-5 (task #290, item 3) THROWAWAY measurement harness — NOT a shipping
//! artifact. Companion to `r13_7_large_cache_extended_hit_rate_measure.rs`
//! (which measures hit-rate/wall-clock on a batch-cycling workload); this
//! file measures RETAINED COMMIT/RSS on an ADVERSARIAL, LONG-HOLDING,
//! WIDE-DIVERSITY workload — the scenario Round 13 review flagged as the
//! extension's real RSS-retention risk: "may temporarily increase retained
//! committed memory by roughly 5x relative to the base 8 slots" (`8` slots
//! → `40` once materialised).
//!
//! ## Workload shape
//!
//! 1. Allocate `N_DISTINCT` (default 40 — enough to fill every slot the
//!    extension provides) distinct, pairwise-`>2x`-apart Large sizes.
//! 2. Free them ALL (each deposits into its own cache slot — proven pattern
//!    from `large_cache_extended_materializes_on_overflow.rs`).
//! 3. HOLD (do not touch the cache further) for the remainder of the run —
//!    this is the "long-holding" adversarial part: with `budget_bytes: None`
//!    (pre-R14-5) or the new finite default (post-R14-5), the decay-based
//!    headroom mechanism is the ONLY thing that can shrink retained commit
//!    over time, and decay only fires on a subsequent large alloc/free (see
//!    `maybe_decay_large_cache`'s doc) — so a workload that stops touching
//!    the Large path after filling the cache retains its full committed
//!    footprint indefinitely until the NEXT large op. This probe reports the
//!    RSS/commit snapshot at exactly that "parked" moment, the worst case
//!    for retained-but-idle committed memory.
//! 4. Print RSS/commit snapshots at each checkpoint (baseline, post-fill,
//!    post-hold) via the same `proc-probe` wrappers
//!    `examples/_shared/paired_ab_large_cache_workload.rs` uses.
//!
//! ## Run once per arm
//!
//!   BASE (8 slots only, no extension):
//!     cargo run --release --example r14_5_large_cache_extended_rss_measure --features "alloc-core alloc-decommit alloc-stats"
//!   EXTENDED (8+32=40 slots):
//!     cargo run --release --example r14_5_large_cache_extended_rss_measure --features "alloc-core alloc-decommit alloc-stats large-cache-extended"
//!
//! `alloc-stats` is required for a non-zero `large_cache_hits` readout (same
//! note as the R13-7 sibling harness); RSS/commit numbers are read
//! regardless of `alloc-stats`.

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

fn rss_kib() -> u64 {
    proc_probe::snapshot().rss / 1024
}

fn commit_kib() -> u64 {
    proc_probe::snapshot().commit / 1024
}

fn layout(bytes: usize) -> Layout {
    Layout::from_size_align(bytes, 8).unwrap()
}

/// `N_DISTINCT` pairwise `>2x`-apart Large sizes — enough to fill every slot
/// the extension provides (8 base + 32 extension = 40) when the feature is
/// on, and to exercise the base-cache's own FIFO eviction ceiling (capped at
/// 8 resident) when it is off.
const N_DISTINCT: usize = 40;

/// `n` distinct Large sizes, LINEARLY spaced one SEGMENT apart starting at
/// the safely-Large floor. Deliberately NOT the geometric-doubling
/// `>2x`-apart derivation the sibling `large_cache_extended_*` correctness
/// tests use: doubling 40 times explodes past any realistic/addressable
/// range (confirmed empirically: it starts OOM-ing around the 13th step on
/// a normal dev host, well before reaching the interesting "cache is full"
/// regime this RSS probe needs to actually reach). Deposit (via `dealloc`)
/// never runs best-fit matching — only `alloc` does — so N linearly-spaced,
/// even CLOSE-together, sizes still land in N distinct slots as long as each
/// is allocated FRESH (never served from an existing cache entry); this
/// probe's fill phase allocates every size fresh in one pass before freeing
/// any of them, so linear spacing is safe here even though the correctness
/// tests elsewhere need the stricter `>2x` derivation for their DIFFERENT
/// purpose (guaranteeing a SUBSEQUENT `alloc` cannot best-fit-match a
/// smaller size against a larger resident entry).
fn large_test_sizes(n: usize) -> Vec<usize> {
    let segment = SegmentLayout::SEGMENT;
    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    let floor = (2 * small_max).div_ceil(segment).max(1) * segment;
    (0..n).map(|i| floor + i * segment).collect()
}

fn main() {
    let extended = cfg!(feature = "large-cache-extended");
    println!("=== R14-5 large-cache-extended RSS/commit retention judge ===");
    println!(
        "feature large-cache-extended: {}",
        if extended {
            "ON (8+32=40 slots)"
        } else {
            "OFF (8 slots only)"
        }
    );
    println!("N_DISTINCT: {N_DISTINCT} distinct Large sizes (adversarial, long-holding)");
    println!();

    // argv[1] = "unbounded" (default; isolates slot-count-driven retention
    // from any budget policy — the number Round 13 review's "~5x" claim is
    // about) or "default-config" (leaves `LargeCacheConfig::DEFAULT` as-is,
    // exercising R14-5 item 2's new finite `large-cache-extended` default
    // budget, `DEFAULT_EXTENDED_BUDGET_BYTES` = 1280 MiB, showing the
    // mitigation this task adds in the SAME harness/workload shape).
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "unbounded".into());
    println!("mode: {mode}");

    let baseline_rss = rss_kib();
    let baseline_commit = commit_kib();
    println!(
        "baseline (before any Large alloc):  RSS={baseline_rss} KiB  commit={baseline_commit} KiB"
    );

    let mut ac = AllocCore::new().expect("primordial");
    #[cfg(feature = "alloc-decommit")]
    if mode == "unbounded" {
        ac.dbg_set_large_cache_budget(None); // isolate: measure slot-count-driven retention alone
    }
    // else: leave `AllocCore::new()`'s resolved default config as-is —
    // exercises whatever `LargeCacheConfig::DEFAULT` resolves to (unbounded
    // pre-R14-5 without `large-cache-extended`; the finite
    // `DEFAULT_EXTENDED_BUDGET_BYTES` post-R14-5 WITH `large-cache-extended`).
    #[cfg(feature = "alloc-decommit")]
    println!(
        "resolved large-cache budget: {:?}",
        ac.dbg_large_cache_budget()
    );

    let sizes = large_test_sizes(N_DISTINCT);
    let requested_total_bytes: usize = sizes.iter().sum();
    println!(
        "requesting {} distinct sizes, {:.1} MiB raw total request size",
        sizes.len(),
        requested_total_bytes as f64 / (1024.0 * 1024.0)
    );

    // Allocate all N_DISTINCT, then free all N_DISTINCT — each dealloc
    // deposits into its own cache slot (best-fit only runs on alloc, so N
    // pairwise->2x-apart sizes each land in a distinct slot; proven pattern
    // from `large_cache_extended_materializes_on_overflow.rs`).
    let mut ptrs = Vec::with_capacity(sizes.len());
    for &bytes in &sizes {
        let l = layout(bytes);
        let p = ac.alloc(l);
        if p.is_null() {
            eprintln!("OOM at {bytes} bytes -- stopping early (host memory pressure)");
            break;
        }
        ptrs.push((p, l));
    }
    let filled_count = ptrs.len();
    for &(p, l) in &ptrs {
        // SAFETY (R6-MS-1/2): pointer from the alloc loop immediately above,
        // live, freed exactly once here (deposits into the cache).
        unsafe { ac.dealloc(p, l) };
    }

    let post_fill_rss = rss_kib();
    let post_fill_commit = commit_kib();
    println!(
        "post-fill ({filled_count} deposits, all freed to cache): RSS={post_fill_rss} KiB  commit={post_fill_commit} KiB"
    );
    println!(
        "  delta vs baseline: RSS +{} KiB  commit +{} KiB",
        post_fill_rss.saturating_sub(baseline_rss),
        post_fill_commit.saturating_sub(baseline_commit)
    );

    #[cfg(feature = "alloc-decommit")]
    {
        println!(
            "  large_cache_used bytes (accounting): {} ({:.1} MiB)",
            ac.dbg_large_cache_used(),
            ac.dbg_large_cache_used() as f64 / (1024.0 * 1024.0)
        );
        let base_occupied = ac
            .dbg_large_cache_slot_sizes()
            .iter()
            .filter(|s| s.is_some())
            .count();
        println!("  base slot occupancy (8):     {base_occupied}");
        #[cfg(feature = "large-cache-extended")]
        {
            println!(
                "  extension materialised:      {}",
                ac.dbg_large_cache_extension_materialised()
            );
            let ext_occupied = ac
                .dbg_large_cache_extended_slot_sizes()
                .iter()
                .filter(|s| s.is_some())
                .count();
            println!("  extension slot occupancy (32): {ext_occupied}");
            println!(
                "  total slots available now:   {}",
                ac.dbg_large_cache_total_slots()
            );
        }
    }

    // "HOLD" phase: do NOT touch the Large path further. Read RSS/commit one
    // more time after a brief no-op window to represent the "parked" steady
    // state a long-idle-but-still-cached workload would present to an
    // external RSS monitor. (No sleep needed for a meaningful signal here —
    // the number of interest is what the ALLOCATOR retained after the fill
    // phase, which is already fully reflected in the post-fill snapshot; this
    // final read is a sanity check that nothing changes without a further
    // large-path touch, i.e. no background decay thread silently shrinks it.)
    let post_hold_rss = rss_kib();
    let post_hold_commit = commit_kib();
    println!(
        "post-hold (no further Large-path activity): RSS={post_hold_rss} KiB  commit={post_hold_commit} KiB"
    );
    assert_eq!(
        post_fill_commit, post_hold_commit,
        "commit must not silently drift while the cache is idle and untouched \
         (no background decay thread exists in this crate's design -- decay \
         only fires inline on a subsequent large alloc/free)"
    );

    println!();
    println!(
        "SUMMARY: retained commit growth over baseline = {} KiB ({:.1} MiB) for {} resident distinct sizes",
        post_fill_commit.saturating_sub(baseline_commit),
        post_fill_commit.saturating_sub(baseline_commit) as f64 / 1024.0,
        filled_count.min(if extended { 40 } else { 8 }),
    );
}
