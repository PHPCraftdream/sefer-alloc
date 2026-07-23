//! R14-5 (task #290, item 6) process-level A/B/B/A judge binary, **treatment
//! arm** (`large-cache-extended` ON — 8+32=40-slot cache).
//!
//! Companion to `paired_ab_large_cache_extended_off.rs` — see that file's
//! module doc for the full rationale, the turnover-workload shape, and why
//! this specific shape (not a static live-set) is required for the
//! extension to show any effect at all (R13-8 §0's own caveat). The ONLY
//! difference between the two binaries is this one is built WITH
//! `large-cache-extended`; both `include!` the identical workload source
//! (`examples/_shared/paired_ab_large_cache_extended_turnover_workload.rs`).
//!
//! Expected result direction (per R13-8 Part C's in-process finding): a HIGH
//! `large_cache_hits` count in this arm (up to 100%, since all 24 distinct
//! sizes fit within the 40-slot extension) versus the baseline arm's LOW
//! count (base 8 slots can only hold 8 of the 24 sizes at once).
//!
//! **Build:** `cargo build --release --example paired_ab_large_cache_extended_on --features "production alloc-stats large-cache-extended"`

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Shared turnover workload body — identical include to
// `paired_ab_large_cache_extended_off.rs`; see that file / the shared
// workload's own module doc.
include!("_shared/paired_ab_large_cache_extended_turnover_workload.rs");

fn main() {
    let (elapsed_ns, hits, total_deallocs, rss_after_kib, commit_after_kib) =
        run_turnover_workload(&GLOBAL);

    let stats = GLOBAL.stats();

    proc_probe::emit("arm", "large_cache_extended_on");
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_u64("large_cache_hits", hits);
    proc_probe::emit_u64("total_deallocs", total_deallocs);
    proc_probe::emit_u64("rss_after_kib", rss_after_kib);
    proc_probe::emit_u64("commit_after_kib", commit_after_kib);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
}
