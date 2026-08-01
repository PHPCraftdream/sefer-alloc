//! R31-8 (task #488) — scan-cost isolation microjudge, **baseline arm**
//! (base 8-slot large cache, `large-cache-extended` OFF).
//!
//! See `examples/_shared/r31_8_large_cache_scan_isolation_workload.rs`'s
//! module doc for the full design rationale (worst-case scan position,
//! why `AllocCore` directly rather than `SeferAlloc`).
//!
//! **ALLOCATOR LAYER UNDER TEST:** a bare `AllocCore` (feature `alloc-core`)
//! — explicitly NOT the real `#[global_allocator]` chain (that is the
//! sibling `r31_3_large_cache_extended_narrow_{off,on}` real-process A/B,
//! which this microjudge complements, not replaces — see this task's report
//! for what each does and does not prove).
//!
//! **Build:** `cargo build --release --example r31_8_large_cache_scan_isolation_off --features "production alloc-stats bench-internals"`

use sefer_alloc::AllocCore;

include!("_shared/r31_8_large_cache_scan_isolation_workload.rs");

/// Base cache: `LARGE_CACHE_SLOTS` (8) is `pub(super)`, not nameable from
/// `examples/` — hardcoded here as the base cache's well-known, long-stable
/// size (matches `HeapCore::dbg_large_cache_slot_sizes`'s own hardcoded `8`
/// return-array length, `src/registry/heap_core_diag.rs`, same rationale).
const BASE_SLOTS: usize = 8;
const ROUNDS: usize = 200_000;

fn main() {
    let mut core = AllocCore::new().expect("primordial bootstrap");

    let target_size = populate_worst_case(&mut core, BASE_SLOTS - 1);

    // Sanity/oracle: the fitting entry the population helper deposited must
    // be recoverable via a real `alloc()` cache HIT before we start timing
    // -- confirms the worst-case shape is actually a HIT-servicing scan, not
    // an accidental miss (which would silently measure something else
    // entirely: a fresh OS reservation, not a scan). Checked via
    // `AllocCore::dbg_large_cache_hits`'s own before/after delta (gated only
    // on `alloc-decommit`, which `alloc-core`'s `alloc_large` path already
    // requires -- not `bench-internals`), not just inferred from "should be
    // a hit" reasoning.
    let hits_before_probe = core.dbg_large_cache_hits();
    let probe = alloc_one(&mut core, target_size);
    let hits_after_probe = core.dbg_large_cache_hits();
    assert_eq!(
        hits_after_probe - hits_before_probe,
        1,
        "R31-8 scan-isolation OFF arm: the worst-case-populated fitting \
         entry was NOT a cache hit on the sanity probe -- the timed loop \
         below would measure something other than a scan-bound hit"
    );
    dealloc_one(&mut core, probe, target_size);

    let elapsed_ns = run_scan_isolation(&mut core, target_size, ROUNDS);

    proc_probe::emit("arm", "large_cache_scan_isolation_off");
    proc_probe::emit_u64("scan_bound", BASE_SLOTS as u64);
    proc_probe::emit_u64("rounds", ROUNDS as u64);
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_ns("ns_per_round", elapsed_ns / ROUNDS as u128);
    proc_probe::emit_u64("oracle_probe_hit", 1);
}
