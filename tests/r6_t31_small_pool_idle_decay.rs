//! R6-T31 (R6-CR-5) — an idle-but-live owner must drain the small-segment
//! hysteresis pool via the large-path cold edges, not only via
//! `reserve_small_segment`.
//!
//! Counterfactual: before the fix the ONLY `maybe_decay_small_pool` caller was
//! `reserve_small_segment_impl`, so a burst → free → idle workload whose later
//! activity is large-only never drains the pool and `dbg_pooled_count()` stays
//! positive indefinitely — the final assertion is red on the old code.
#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals"
))]

use core::alloc::Layout;
use sefer_alloc::AllocCore;

/// Fill the small-segment hysteresis pool: churn a batch of small 64-byte
/// blocks spanning several segments, then free them all (empty segments are
/// admitted to the pool instead of being released). Returns `Ok(pooled)` if
/// all allocations succeeded, or `Err(())` if an alloc returned null (OOM in
/// a constrained environment — the only permitted skip).
fn fill_small_pool(ac: &mut AllocCore) -> Result<usize, ()> {
    let layout = Layout::from_size_align(64, 8).expect("valid layout");
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(200_000);
    for _ in 0..200_000 {
        let p = ac.alloc(layout);
        if p.is_null() {
            // OOM in a constrained environment — clean up and skip.
            for &q in &ptrs {
                // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the
                // pointer was returned by a prior matching alloc in this test,
                // is live, and is freed exactly once here.
                unsafe { ac.dealloc(q, layout) };
            }
            return Err(());
        }
        ptrs.push(p);
    }
    for &p in &ptrs {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer
        // was returned by a prior matching alloc in this test, is live, and is
        // freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
    Ok(ac.dbg_pooled_count())
}

#[test]
fn idle_owner_drains_small_pool_via_large_path_cold_edges() {
    let mut ac = AllocCore::new().expect("primordial");

    // Small churn to fill the pool. The ONLY permitted skip is allocation OOM;
    // if all allocations succeeded but the pool stayed empty, that is a pool-
    // admission FAILURE, not an environment limit.
    match fill_small_pool(&mut ac) {
        Err(()) => {
            eprintln!("OOM during small churn (constrained env) — skipping");
            return;
        }
        Ok(pooled) => {
            assert!(
                pooled > 0,
                "small churn freed every block but the pool stayed empty — \
                 pool admission broken (R6-T31)"
            );
        }
    }
    assert!(ac.dbg_pooled_count() > 0);

    // Arm instant decay: interval 0 → every eligible tick decays. The large
    // cache draining is irrelevant to this test.
    ac.dbg_set_decay_config(5000, 0, 0);

    // Drive ONLY large-path cold events (no small reserve): alloc + free a
    // 4 MiB block per iteration; both edges tick `maybe_decay_small_pool`.
    let large = Layout::from_size_align(4 * 1024 * 1024, 8).expect("valid layout");
    for _ in 0..256 {
        let p = ac.alloc(large);
        if !p.is_null() {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the
            // pointer was returned by the matching alloc immediately above,
            // is live, and is freed exactly once here.
            unsafe { ac.dealloc(p, large) };
        }
        if ac.dbg_pooled_count() == 0 {
            break;
        }
    }

    assert_eq!(
        ac.dbg_pooled_count(),
        0,
        "idle-owner large-path cold events must drain the pool (R6-T31)"
    );
}
