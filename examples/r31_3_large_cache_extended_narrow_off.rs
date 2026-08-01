//! R31-3 (task #466, step 3) — N=1/2/4 narrow-working-set-after-burst TIMING
//! regression judge, **baseline arm** (`large-cache-extended` OFF — base
//! 8-slot cache).
//!
//! Companion to `r31_3_large_cache_extended_narrow_on.rs`. Both binaries
//! `include!` the identical shared workload
//! (`examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`) —
//! the ONLY difference between them is whether this binary is built WITH or
//! WITHOUT the `large-cache-extended` feature. Fills the gap
//! `docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` item 4 and
//! `docs/perf/OPEN_ITEMS.md` item 7 (`[L]` tier) named as deferred: does the
//! widened O(40) scan bound cost anything on a narrow (N=1/2/4)
//! working-set-after-materialisation-burst shape?
//!
//! **ALLOCATOR LAYER UNDER TEST** (CLAUDE.md's R30-8 rule): the real
//! `#[global_allocator]` — `SeferAlloc::new()` — via plain `std::alloc::{alloc,
//! dealloc}`, exactly like the sibling turnover A/B
//! (`paired_ab_large_cache_extended_{off,on}.rs`).
//!
//! `argv[1]` selects the narrow working-set size: `1`, `2`, or `4` (default
//! `1` if omitted).
//!
//! **RESULT lines:**
//! - `RESULT narrow_n=<n>` — the working-set size this run measured.
//! - `RESULT elapsed_ns=<n>` — the TIMED narrow-phase wall-clock only
//!   (excludes the untimed materialisation burst and warm-up — the metric
//!   `scripts/paired-ab-runner.mjs` pairs by default).
//! - `RESULT large_cache_hits=<n>` — cache hits during the TIMED narrow
//!   phase. Expected 100% of `total_deallocs` in BOTH arms (path-activation
//!   oracle — see the shared workload's module doc).
//! - `RESULT total_deallocs=<n>` — sanity: `n * ROUNDS`.
//! - `RESULT rss_after_kib=<n>` / `RESULT commit_after_kib=<n>` — memory
//!   snapshot at the end of the timed region.
//! - `RESULT segments_reserved_total=<n>` — installed-allocator sanity
//!   counter (nonzero in both arms).
//!
//! **Build:** `cargo build --release --example r31_3_large_cache_extended_narrow_off --features "production alloc-stats bench-internals"`
//!
//! **CORRECTED 2026-08-01 (task #487):** `bench-internals` added to the
//! required feature set — the shared workload's in-run materialisation
//! oracle (see `examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`'s
//! module doc) reads `SeferAlloc::dbg_current_large_cache_total_slots`,
//! which is `bench-internals`-gated. This binary's own config is unaffected
//! (`SeferAlloc::new()` unchanged) — only the ON arm's config needed to
//! change; see `r31_3_large_cache_extended_narrow_on.rs`'s own correction
//! note for why.

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Shared narrow-AB workload body — see
// `examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`'s
// module doc for the full workload shape and why `include!` (not a shared
// crate module) is used.
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

    let result = run_narrow_ab_workload(&GLOBAL, n);

    let stats = GLOBAL.stats();

    proc_probe::emit("arm", "large_cache_extended_narrow_off");
    proc_probe::emit_u64("narrow_n", n as u64);
    proc_probe::emit_ns("elapsed_ns", result.narrow_elapsed_ns);
    proc_probe::emit_u64("large_cache_hits", result.narrow_hits);
    proc_probe::emit_u64("total_deallocs", result.narrow_total_deallocs);
    proc_probe::emit_u64("rss_after_kib", result.rss_after_kib);
    proc_probe::emit_u64("commit_after_kib", result.commit_after_kib);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
    proc_probe::emit_u64("segments_reserved_baseline", result.reserved_baseline);
    proc_probe::emit_u64("segments_reserved_pre_timing", result.reserved_pre_timing);
    proc_probe::emit_u64(
        "large_cache_used_bytes_pre_timing",
        result.used_bytes_pre_timing,
    );
    #[cfg(feature = "bench-internals")]
    proc_probe::emit_u64(
        "oracle_total_slots",
        GLOBAL.dbg_current_large_cache_total_slots() as u64,
    );
}
