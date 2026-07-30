//! R31-3 (task #466, step 3) — N=1/2/4 narrow-working-set-after-burst TIMING
//! regression judge, **treatment arm** (`large-cache-extended` ON —
//! 8+32=40-slot cache).
//!
//! Companion to `r31_3_large_cache_extended_narrow_off.rs` — see that file's
//! module doc for the full rationale and workload shape. The ONLY difference
//! between the two binaries is this one is built WITH `large-cache-extended`;
//! both `include!` the identical workload source
//! (`examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`).
//!
//! **ALLOCATOR LAYER UNDER TEST** (CLAUDE.md's R30-8 rule): the real
//! `#[global_allocator]` — `SeferAlloc::new()` — via plain `std::alloc::{alloc,
//! dealloc}`. `SeferAlloc::new()` uses `LargeCacheConfig::DEFAULT`, which
//! under `large-cache-extended` resolves `budget_bytes: None` to
//! `Some(DEFAULT_EXTENDED_BUDGET_BYTES)` (256 MiB, R17-9) — i.e. this arm
//! measures the FINITE-budget shipped default, not an unbounded override
//! (per the task brief's explicit instruction to use a finite budget for the
//! extended arm).
//!
//! **Build:** `cargo build --release --example r31_3_large_cache_extended_narrow_on --features "production alloc-stats large-cache-extended"`

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Shared narrow-AB workload body — identical include to
// `r31_3_large_cache_extended_narrow_off.rs`; see that file / the shared
// workload's own module doc.
include!("_shared/r31_3_large_cache_extended_narrow_ab_workload.rs");

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    assert!(
        matches!(n, 1 | 2 | 4),
        "narrow working-set size must be 1, 2, or 4 (got {n})"
    );

    let (elapsed_ns, hits, total_deallocs, rss_after_kib, commit_after_kib) =
        run_narrow_ab_workload(&GLOBAL, n);

    let stats = GLOBAL.stats();

    proc_probe::emit("arm", "large_cache_extended_narrow_on");
    proc_probe::emit_u64("narrow_n", n as u64);
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_u64("large_cache_hits", hits);
    proc_probe::emit_u64("total_deallocs", total_deallocs);
    proc_probe::emit_u64("rss_after_kib", rss_after_kib);
    proc_probe::emit_u64("commit_after_kib", commit_after_kib);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
}
