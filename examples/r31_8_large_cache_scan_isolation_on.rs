//! R31-8 (task #488) — scan-cost isolation microjudge, **treatment arm**
//! (extended 8+32=40-slot large cache, `large-cache-extended` ON).
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
//! **Budget override:** same "recover unbounded" idiom as the sibling
//! narrow-AB ON binary (`r31_3_large_cache_extended_narrow_on.rs`) — the
//! decoy population needs 39 distinct deposits up to 40 segments (160 MiB)
//! each, individually far above the 256 MiB default `large-cache-extended`
//! budget for the largest few; `budget_bytes(usize::MAX)` avoids the same
//! budget-rejection-before-materialisation trap that file's own 2026-08-01
//! correction note documents in detail.
//!
//! **Build:** `cargo build --release --example r31_8_large_cache_scan_isolation_on --features "production alloc-stats bench-internals large-cache-extended"`

use sefer_alloc::{AllocCore, LargeCacheConfig};

include!("_shared/r31_8_large_cache_scan_isolation_workload.rs");

/// Extended cache: `LARGE_CACHE_SLOTS + LARGE_CACHE_EXTENDED_SLOTS` (8 + 32
/// = 40) — both constituent constants are `pub(super)`/`pub(crate)`, not
/// nameable from `examples/` — hardcoded here as the well-known, documented
/// total (matches the sibling narrow-AB ON binary's own oracle assertion of
/// `40`, `r31_3_large_cache_extended_narrow_on.rs`).
const EXTENDED_SLOTS: usize = 40;
const ROUNDS: usize = 200_000;

fn main() {
    let mut core = AllocCore::new_with_config(LargeCacheConfig::new().budget_bytes(usize::MAX))
        .expect("primordial bootstrap");

    let target_size = populate_worst_case(&mut core, EXTENDED_SLOTS - 1);

    // Oracle: the extension sidecar must actually have materialised by this
    // point (39 decoys + 1 target = 40 deposits, overflowing the base 8 by
    // 32 -- exactly filling the sidecar) -- CLAUDE.md's R30-8 path-
    // activation rule, mirroring the sibling narrow-AB workload's own oracle.
    assert!(
        core.dbg_large_cache_extension_materialised(),
        "R31-8 scan-isolation ON arm: large-cache extension sidecar did not \
         materialise after populating {EXTENDED_SLOTS} entries -- the \
         widened-scan-bound premise this microjudge measures never actually \
         activated"
    );
    assert_eq!(
        core.dbg_large_cache_total_slots(),
        EXTENDED_SLOTS,
        "R31-8 scan-isolation ON arm: resolved total large-cache slot count \
         must be {EXTENDED_SLOTS} once materialised"
    );

    // Sanity/oracle: the fitting entry the population helper deposited must
    // be recoverable via a real `alloc()` cache HIT before we start timing
    // -- same discipline as the sibling OFF binary, checked via
    // `AllocCore::dbg_large_cache_hits`'s own before/after delta.
    let hits_before_probe = core.dbg_large_cache_hits();
    let probe = alloc_one(&mut core, target_size);
    let hits_after_probe = core.dbg_large_cache_hits();
    assert_eq!(
        hits_after_probe - hits_before_probe,
        1,
        "R31-8 scan-isolation ON arm: the worst-case-populated fitting \
         entry was NOT a cache hit on the sanity probe -- the timed loop \
         below would measure something other than a scan-bound hit"
    );
    dealloc_one(&mut core, probe, target_size);

    let elapsed_ns = run_scan_isolation(&mut core, target_size, ROUNDS);

    proc_probe::emit("arm", "large_cache_scan_isolation_on");
    proc_probe::emit_u64("scan_bound", EXTENDED_SLOTS as u64);
    proc_probe::emit_u64("rounds", ROUNDS as u64);
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_ns("ns_per_round", elapsed_ns / ROUNDS as u128);
    proc_probe::emit_u64("oracle_materialised", 1);
    proc_probe::emit_u64("oracle_probe_hit", 1);
}
