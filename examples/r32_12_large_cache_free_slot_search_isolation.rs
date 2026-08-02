//! R32-12 (task #503, F8 sub-change (2)) — free-slot-search isolation
//! microjudge. Measures `large_cache_find_free_slot`'s admission-path cost
//! at a fixed, worst-case-for-a-linear-scan occupancy shape (base cache: 7
//! permanent decoys occupying slots `0..7`, one genuinely free slot at index
//! 7, repeatedly taken/redeposited by the timed loop).
//!
//! Built and run TWICE against two different source trees (via
//! `git worktree add`, the established pattern for this project's
//! before/after perf gates — see e.g.
//! `docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE.md`): BEFORE = the linear
//! `.iter().position(|s| s.is_none())` scan (pre-R32-12), AFTER = the
//! `large_cache_occupied.trailing_ones()` bitmask lookup (this task). This
//! file itself is IDENTICAL in both trees (copied, not edited) — only the
//! `large_cache_find_free_slot` implementation it calls (transitively,
//! through `AllocCore::alloc`/`dealloc`) differs.
//!
//! **ALLOCATOR LAYER UNDER TEST:** a bare `AllocCore` (feature `alloc-core`)
//! — the same microjudge-layer choice R31-8 documents and justifies (this
//! isolates the scan itself, not the full `#[global_allocator]` dispatch
//! chain).
//!
//! **Build:** `cargo build --release --example r32_12_large_cache_free_slot_search_isolation --features "alloc-core alloc-decommit alloc-stats"`

use sefer_alloc::AllocCore;

include!("_shared/r32_12_large_cache_free_slot_search_workload.rs");

/// Base cache: `LARGE_CACHE_SLOTS` (8) is `pub(super)`, not nameable from
/// `examples/` — hardcoded here, matching R31-8's own precedent
/// (`examples/r31_8_large_cache_scan_isolation_off.rs`).
const BASE_SLOTS: usize = 8;
const DECOY_COUNT: usize = BASE_SLOTS - 1; // 7 permanent decoys, 1 free slot (worst case)
const ROUNDS: usize = 200_000;

fn main() {
    let mut core = AllocCore::new().expect("primordial bootstrap");

    let cycle_size = populate_decoys(&mut core, DECOY_COUNT);

    // Prime the one cycling slot once, outside the timed region, so the
    // FIRST timed iteration's alloc(S) is already a hit (matching every
    // subsequent iteration's shape) rather than a one-off miss.
    let prime = alloc_one(&mut core, cycle_size);
    dealloc_one(&mut core, prime, cycle_size);

    // Path-activation oracle (CLAUDE.md R30-8): every alloc(cycle_size) in
    // the timed loop below must be a genuine cache HIT (against the entry
    // the prior iteration's dealloc deposited at the single free slot) --
    // not a miss (fresh OS reservation), which would silently measure
    // something else entirely.
    let hits_before = core.dbg_large_cache_hits();
    let elapsed_ns = run_free_slot_search_isolation(&mut core, cycle_size, ROUNDS);
    let hits_after = core.dbg_large_cache_hits();
    let hit_delta = hits_after - hits_before;
    assert_eq!(
        hit_delta, ROUNDS as u64,
        "R32-12 free-slot-search isolation: expected every one of {ROUNDS} timed \
         alloc(cycle_size) calls to be a cache HIT (delta == rounds), got hit_delta={hit_delta} \
         -- the worst-case-populated shape was not maintained across the timed loop"
    );

    proc_probe::emit("arm", "large_cache_free_slot_search_isolation");
    proc_probe::emit_u64("decoy_count", DECOY_COUNT as u64);
    proc_probe::emit_u64("scan_bound", BASE_SLOTS as u64);
    proc_probe::emit_u64("rounds", ROUNDS as u64);
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_ns("ns_per_round", elapsed_ns / ROUNDS as u128);
    proc_probe::emit_u64("oracle_hit_delta", hit_delta);
}
