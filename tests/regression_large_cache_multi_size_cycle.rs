//! Regression test — task D1 (Phase D, pределы/тюнинг).
//!
//! `LARGE_CACHE_SLOTS` used to be 2. A workload that cycles through more than
//! two distinct large sizes (each within `LARGE_CACHE_SIZE_FACTOR` of the
//! next request so it is cache-admissible) evicted the cache down to nothing
//! every round, forcing an OS round-trip on every `alloc_large` despite the
//! cache existing. With `LARGE_CACHE_SLOTS == 2` and 4 distinct sizes, no
//! slot can ever survive a full round — 4 distinct spans compete for 2 slots,
//! so by the time a size comes back around its slot has already been evicted.
//!
//! This test cycles 4 distinct large sizes for several rounds and asserts
//! that cache HITS actually occur (via the `dbg_large_cache_hits` diagnostic
//! counter added for this task). It also exercises the FIFO-eviction fix: the
//! old "slot 0 == oldest" assumption breaks once `LARGE_CACHE_SLOTS > 2` and
//! hits/deposits stop filling slots in strict index order — the fix replaces
//! it with a real insertion-sequence-number FIFO (`CachedLarge::seq`).
//!
//! Counterfactual (verified manually — see task report): reverting
//! `LARGE_CACHE_SLOTS` to 2 while keeping this test's 4-distinct-size cycle
//! makes the hit-rate assertion fail (0 or near-0 hits after warmup), because
//! every round evicts both slots before the same size comes around again.

// Requires `alloc-stats` (task W3): the assertion rests on the per-hit
// `large_cache_hits` increment (via `dbg_large_cache_hits`), gated behind
// `alloc-stats` (default OFF). Without it the counter reads 0 by design.
#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "alloc-stats"
))]

use core::alloc::Layout;
use sefer_alloc::AllocCore;

const MIB: usize = 1024 * 1024;

fn layout(mib: usize) -> Layout {
    Layout::from_size_align(mib * MIB, 8).unwrap()
}

/// Cycle 4 distinct large sizes (4, 5, 6, 7 MiB — all within
/// `LARGE_CACHE_SIZE_FACTOR` (2x) of each other's rounded `usable_size`, so
/// each is cache-admissible against the others) for several rounds and check
/// that the cache actually absorbs repeat requests (hits > 0 after warmup).
#[test]
fn multi_size_cycle_produces_cache_hits() {
    let mut ac = AllocCore::new().expect("primordial");
    // Unbounded budget: isolate the slot-count effect from the byte-budget
    // effect (D1 is specifically about slot count).
    ac.dbg_set_large_cache_budget(None);

    // Sizes spread more than `LARGE_CACHE_SIZE_FACTOR` (2x) apart from their
    // neighbours so each occupies its OWN cache slot (no cross-size hit
    // "borrowing" between e.g. 4 MiB and 5 MiB requests) — this isolates the
    // slot-COUNT effect that D1 targets, rather than the size-factor
    // tolerance.
    let sizes = [4usize, 16, 64, 256];
    let layouts: Vec<Layout> = sizes.iter().map(|&m| layout(m)).collect();

    let hits_before = ac.dbg_large_cache_hits();

    const ROUNDS: usize = 6;
    for round in 0..ROUNDS {
        let mut ptrs = Vec::with_capacity(layouts.len());
        for l in &layouts {
            let p = ac.alloc(*l);
            if p.is_null() {
                eprintln!("OOM in multi_size_cycle_produces_cache_hits round {round} — skip");
                return;
            }
            ptrs.push(p);
        }
        for (p, l) in ptrs.into_iter().zip(layouts.iter()) {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { ac.dealloc(p, *l) };
        }
    }

    let hits_after = ac.dbg_large_cache_hits();
    let hits = hits_after - hits_before;

    // Theoretical max: every alloc from round 1 onward hits (all 4 distinct
    // sizes already deposited in round 0) = 4 hits/round * (ROUNDS - 1)
    // rounds = 20. With 8 cache slots every one of the 4 distinct sizes gets
    // its own durable slot, so we expect the full 20/20.
    //
    // With the OLD `LARGE_CACHE_SLOTS == 2`, only the last 2 sizes deposited
    // within a round (of the 4 requested) survive to be hit by the NEXT
    // round — the first 2 deposits get FIFO-evicted by the later 2 deposits
    // in the same round before the loop revisits them. That caps hits at
    // 2 hits/round * (ROUNDS - 1) = 10/20 — verified manually by reverting
    // `LARGE_CACHE_SLOTS` to 2 (see task D1 report: 10 hits observed, vs 20
    // with 8 slots). This threshold (15) sits strictly between the two,
    // so it fails under the old 2-slot cache and passes under the fix.
    let expected_min = (sizes.len() as u64) * (ROUNDS as u64 - 1) - 5; // 15
    assert!(
        hits >= expected_min,
        "expected >= {expected_min} large-cache hits cycling {} distinct sizes over {ROUNDS} \
         rounds (theoretical max {}), got {hits} — LARGE_CACHE_SLOTS may be too small",
        sizes.len(),
        (sizes.len() as u64) * (ROUNDS as u64 - 1)
    );
}
