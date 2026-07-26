//! R21-2 (task #351) / R22-14 (task #365) — Stage-1 OPT-H diagnostic
//! hit-rate probe, promoted from a throwaway reconstruction recipe
//! (`docs/perf/R21_2_OPT_H_STAGE1_HIT_RATE.md` §3's inline `cat > ... <<'RS'`
//! instructions) to a small, permanent, committed example so the report's
//! 0/320 and 0/20 numbers are reproducible from something checked into the
//! repo, not just from prose reconstruction instructions.
//!
//! **Observation-only, per this task's explicit scope:** this binary adds NO
//! new counters and changes NO allocator decision logic. It only calls the
//! two EXISTING `#[doc(hidden)]` debug accessors
//! (`AllocCore::dbg_opt_h_attempts()` / `dbg_opt_h_hits()`, added by R21-2
//! itself) after running each of the two EXISTING, unmodified shared
//! workload files R21-2's report already cites as its harnesses.
//!
//! **Build:**
//! `cargo run --release --example r21_2_opt_h_stage1_probe --features "production,medium-classes,alloc-stats"`
//!
//! Expected output (matching R21_2_OPT_H_STAGE1_HIT_RATE.md §3.1/§3.2 and
//! `docs/perf/_raw_r21_2_stage1_measurement.log`):
//! ```text
//! harness=r10_2_medium_workload opt_h_attempts=320 opt_h_hits=0
//! harness=r21_1_hot_buffer_workload opt_h_attempts=20 opt_h_hits=0
//! ```

use sefer_alloc::{AllocCore, SeferAlloc};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Two EXISTING shared workload files, `include!`d verbatim — byte-identical
// to what R10-2's / R21-1's own `paired_ab_*` example binaries compile.
// Each is wrapped in its own module so their (differently-shaped) helper
// items don't collide. `#[allow(dead_code)]`: each shared file also defines
// a private `rss_kib`/`commit_kib` memory-snapshot probe that the normal
// `paired_ab_*_{off,on}.rs` wrappers call but this attempts/hits-only probe
// does not — those files are unmodified per this task's own scope (not
// touched to add `pub`), so the unused-private-fn warning is silenced here
// instead, at the include site, rather than editing the shared source.
#[allow(dead_code)]
mod r10_2_harness {
    include!("_shared/paired_ab_medium_workload.rs");
}

#[allow(dead_code)]
mod r21_1_harness {
    include!("_shared/paired_ab_hot_buffer_workload.rs");
}

fn main() {
    // Harness 1: R10-2's N=16-simultaneous-object adversarial workload.
    let _ = r10_2_harness::run_phased_workload();
    println!(
        "harness=r10_2_medium_workload opt_h_attempts={} opt_h_hits={}",
        AllocCore::dbg_opt_h_attempts(),
        AllocCore::dbg_opt_h_hits()
    );

    // Harness 2: R21-1's single-hot-buffer workload. Counters are
    // process-global and cumulative, so this harness's attempts/hits are
    // printed as a DELTA over harness 1's cumulative totals above, matching
    // what running each harness in its own fresh process (as the original
    // report's reconstruction recipe did) would have measured independently.
    let attempts_before = AllocCore::dbg_opt_h_attempts();
    let hits_before = AllocCore::dbg_opt_h_hits();

    let _ = r21_1_harness::run_hot_buffer_workload();

    println!(
        "harness=r21_1_hot_buffer_workload opt_h_attempts={} opt_h_hits={}",
        AllocCore::dbg_opt_h_attempts() - attempts_before,
        AllocCore::dbg_opt_h_hits() - hits_before
    );
}
