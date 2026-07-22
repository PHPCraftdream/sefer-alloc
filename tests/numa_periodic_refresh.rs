//! R12-5: bounded mid-claim NUMA-node cache refresh regression.
//!
//! R11-5's `current_node_cached()` cache is invalidated only at registry-slot
//! `claim()`/`recycle()` boundaries (`tests/numa_cache_invalidation.rs`
//! covers that). Within a single claim, a thread that migrates to a
//! different NUMA node (OS scheduler migration, not `claim`/`recycle`) was
//! previously invisible to the cache for the ENTIRE remaining lifetime of
//! that claim — unbounded in wall-clock time for a long-lived heap. R12-5
//! adds a periodic forced re-query every `AllocCore::NUMA_NODE_REFRESH_PERIOD`
//! calls to `current_node_cached()`, bounding that staleness. See
//! `docs/PHASE_NUMA_DESIGN.md` §4.1 "Bounded mid-claim refresh (R12-5)".
//!
//! This test proves the fix with a pre-fix/post-fix counterfactual: claim a
//! heap on mock node A, populate the cache, "migrate" by scripting the mock
//! to node B (simulating an OS migration `HeapRegistry::claim`/`recycle` is
//! NOT involved in), then drive `NUMA_NODE_REFRESH_PERIOD + 1` more calls
//! through the cached accessor. Without the fix the cache would report node A
//! forever (until the next `claim()`); with the fix it must report node B
//! once the refresh period elapses.
//!
//! Build/run:
//!   cargo test --features "numa-aware-mock alloc-global" --test numa_periodic_refresh

#![cfg(all(feature = "numa-aware-mock", feature = "alloc-global"))]

use std::sync::atomic::{AtomicBool, Ordering};

use numa_shim::mock;
use sefer_alloc::registry::{bootstrap, HeapRegistry};

/// Serialise all tests in this file — same discipline as
/// `tests/numa_cache_invalidation.rs`: the registry is a process-global
/// static and the mock's call log is per-thread, so parallel tests here
/// would race on claim/recycle and on the scripted mock node.
static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

fn script_node(node: u32) {
    mock::set_current_node(node);
    let _ = mock::drain();
}

/// The headline regression: after "migrating" mid-claim (scripting the mock
/// to a new node WITHOUT going through `claim`/`recycle`), the cached value
/// must catch up to the new node within `NUMA_NODE_REFRESH_PERIOD` calls to
/// the cached accessor — it must NOT stay pinned to the pre-migration node
/// for the rest of the claim.
#[test]
fn cached_node_refreshes_after_mid_claim_migration() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // ── Claim on node A, populate the cache ──
    script_node(1);
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "claim returned null");

    // SAFETY: live claimed slot, single-writer.
    unsafe { (*heap).dbg_populate_numa_cache_for_test() };
    assert_eq!(
        unsafe { (*heap).dbg_cached_numa_node() },
        Some(1),
        "cache must populate with the pre-migration mock node (1)"
    );

    // ── Simulate an OS-level migration: the mock now returns node 9, but we
    // do NOT claim/recycle — this is exactly the scenario R11-5's
    // claim-boundary-only invalidation could not see. ──
    script_node(9);

    // Re-populate ONE more time immediately after "migrating": with only a
    // single hit since the last real query, the cache must still be well
    // under `NUMA_NODE_REFRESH_PERIOD` and therefore still report the STALE
    // node (1), not the new one (9) yet. This is the counterfactual half: a
    // naive "always re-query" implementation would wrongly pass the
    // headline assertion below for the wrong reason (it would look correct
    // even if the periodic-refresh mechanism didn't exist, because the
    // bounded staleness property would be vacuous). Asserting staleness
    // HERE first proves the cache is genuinely caching, not just always
    // hitting the OS.
    unsafe { (*heap).dbg_populate_numa_cache_for_test() };
    assert_eq!(
        unsafe { (*heap).dbg_cached_numa_node() },
        Some(1),
        "immediately after migration, the cache must still report the STALE \
         node (1) — this proves the cache is genuinely caching (not \
         re-querying every call), which is the premise the refresh-bound \
         assertion below depends on"
    );

    // ── Drive enough more calls to exceed the refresh period. ──
    //
    // `dbg_populate_numa_cache_for_test` is a thin wrapper around
    // `current_node_cached()`; each call increments the hit counter until it
    // reaches `NUMA_NODE_REFRESH_PERIOD`, at which point the NEXT call forces
    // a real re-query. We already spent one hit above (bringing the count to
    // 1), so driving `NUMA_NODE_REFRESH_PERIOD` more calls is guaranteed to
    // cross the threshold and force a re-query within this loop.
    const REFRESH_PERIOD: u32 = 128; // mirrors AllocCore::NUMA_NODE_REFRESH_PERIOD
    for _ in 0..=REFRESH_PERIOD {
        // SAFETY: live claimed slot, single-writer.
        unsafe { (*heap).dbg_populate_numa_cache_for_test() };
    }

    // ── Post-fix: the cache must have refreshed to the NEW node (9). ──
    // Pre-fix (no periodic refresh — cache only ever re-queries at
    // claim()/recycle() boundaries), this would still read `Some(1)` no
    // matter how many more calls are driven, since we never recycled.
    assert_eq!(
        unsafe { (*heap).dbg_cached_numa_node() },
        Some(9),
        "after driving past NUMA_NODE_REFRESH_PERIOD calls post-migration, \
         the cache must have refreshed to the NEW mock node (9) — a value \
         still stuck at the stale node (1) here is the exact bug R12-5's \
         periodic refresh exists to bound"
    );

    // SAFETY: `heap` was returned by `claim` above and not yet recycled.
    unsafe { HeapRegistry::recycle(heap) };
}
