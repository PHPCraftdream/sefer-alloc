//! R14-5 (task #290, item 6) process-level A/B/B/A judge binary, **baseline
//! arm** (`large-cache-extended` OFF — base 8-slot cache).
//!
//! Companion to `paired_ab_large_cache_extended_on.rs`. Both binaries
//! `include!` the identical TURNOVER workload
//! (`examples/_shared/paired_ab_large_cache_extended_turnover_workload.rs`)
//! — the ONLY difference between them is whether this binary is built WITH
//! or WITHOUT the `large-cache-extended` feature. This is the process-level,
//! paired-A/B/B/A counterpart to R13-8 Part C's in-process turnover judge
//! (`examples/r13_8_medium_working_set_judge.rs`,
//! `docs/perf/R13_8_MEDIUM_WORKING_SET_JUDGE.md`), built specifically to
//! back the promotion recommendation in
//! `docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` with the same
//! paired t-test/sign-test protocol the other Round 14 production-gate
//! decisions cite (`scripts/paired-ab-runner.mjs`).
//!
//! **RESULT lines:**
//! - `RESULT elapsed_ns=<n>` — full timed-region wall-clock (the metric
//!   `scripts/paired-ab-runner.mjs` pairs by default).
//! - `RESULT large_cache_hits=<n>` — cache hits during the timed region.
//!   Expected LOW (base 8 slots can only hold 8 of the 24 distinct sizes at
//!   once) in this arm — the turnover-shape counterpart to R13-8 Part C's
//!   33.33% base hit-rate finding.
//! - `RESULT total_deallocs=<n>` — sanity: fixed at `24 * 200 = 4800`.
//! - `RESULT rss_after_kib=<n>` / `RESULT commit_after_kib=<n>` — memory
//!   snapshot at the end of the timed region.
//! - `RESULT segments_reserved_total=<n>` — installed-allocator sanity
//!   counter (nonzero in both arms; both genuinely exercise SeferAlloc).
//!
//! **Build:** `cargo build --release --example paired_ab_large_cache_extended_off --features "production alloc-stats"`

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Shared turnover workload body — see
// `examples/_shared/paired_ab_large_cache_extended_turnover_workload.rs`'s
// module doc for why `include!` (not a shared crate module) is used.
include!("_shared/paired_ab_large_cache_extended_turnover_workload.rs");

fn main() {
    let (elapsed_ns, hits, total_deallocs, rss_after_kib, commit_after_kib) =
        run_turnover_workload(&GLOBAL);

    let stats = GLOBAL.stats();

    proc_probe::emit("arm", "large_cache_extended_off");
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_u64("large_cache_hits", hits);
    proc_probe::emit_u64("total_deallocs", total_deallocs);
    proc_probe::emit_u64("rss_after_kib", rss_after_kib);
    proc_probe::emit_u64("commit_after_kib", commit_after_kib);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
}
