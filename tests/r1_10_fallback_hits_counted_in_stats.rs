//! R1-10 (src review round 1, `docs/reviews/2026-09-28-005939-src-review-oxx-round-1.md`):
//! a magazine (tcache) hit or large-cache hit served by the FALLBACK heap
//! must be folded into `SeferAlloc::stats()`'s process-wide
//! `tcache_hits`/`large_cache_hits` totals, not silently dropped.
//!
//! ## Background
//!
//! Before this task, fallback init (`global::fallback::heap_ptr`) bound only
//! `thread_free`/`overflow` (`alloc-xthread`-only concerns); it never called
//! `HeapCore::bind_tcache_hits`/`bind_large_cache_hits` the way
//! `HeapRegistry::claim`'s `bind_slot_counters` does for a real registry
//! slot. So the fallback's `tcache_hits`/`large_cache_hits_sink` fields
//! stayed `None` forever, the per-hit increment in `HeapCore::alloc`
//! (`#[cfg(feature = "alloc-stats")] if let Some(hits) = self.tcache_hits`)
//! was always skipped for a fallback-served hit, and the process-wide
//! aggregators (`registry::heap_registry::counters`) only ever walked
//! registry SLOTS — the fallback has none — so a fallback hit was invisible
//! end to end: not counted locally, and (even if it had been) not summed
//! into the process-wide total either.
//!
//! ## This test
//!
//! Drives ONE guaranteed magazine hit and ONE guaranteed large-cache hit
//! through the FALLBACK heap specifically (via the `#[doc(hidden)]` test
//! hook `HeapCore::dbg_with_fallback_for_test`, which runs its closure
//! through the SAME production `with_heap`/`LockGuard` path `SeferAlloc`'s
//! own fallback arms use — no test-only shortcut around the lock or the
//! bind sequence), and asserts:
//!
//! 1. The fallback `HeapCore`'s OWN local counters (`tcache_hits()` /
//!    `dbg_large_cache_hits()`) advance — proving the BIND half of the fix
//!    (the counters are no longer `None`/unbound).
//! 2. `SeferAlloc::stats()`'s process-wide totals advance by AT LEAST that
//!    much — proving the AGGREGATION half of the fix (the fallback's
//!    contribution is folded in, not just locally counted and then dropped
//!    by the registry-slots-only walk).
//!
//! **Counterfactual:** reverting either half alone (leaving `bind_*` calls
//! out of fallback init, OR leaving them in but not folding the fallback
//! statics into the three aggregator functions) makes this test fail —
//! verified by hand for this task (see the task report): with `bind_*`
//! reverted, the fallback's OWN local counters read 0 after the workload
//! (assertion 1 fails); with only the aggregator fold-in reverted, the
//! fallback's local counters advance but `stats()`'s totals do not move
//! (assertion 2 fails).
//!
//! Mirrors `tests/regression_percounter_perheap_aggregation.rs`'s
//! local-delta-vs-global-delta shape, applied to the fallback heap instead
//! of a second registry-claimed heap.

#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "alloc-decommit",
    feature = "alloc-xthread",
    feature = "alloc-stats",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::HeapCore;
use sefer_alloc::SeferAlloc;

/// Serialise against other test FILES that also touch the process-wide
/// fallback/registry statics — mirrors
/// `regression_percounter_perheap_aggregation.rs`'s `SerialGuard`. Only
/// serialises within this file; the `>=`-not-`==` assertions below tolerate
/// concurrent activity from other test binaries' threads (the default
/// `cargo test` runner may run other test FILES' processes concurrently, but
/// each integration test file is its own process, so no cross-FILE
/// interleaving is possible here — this guard exists for this file's own two
/// `#[test]`s, of which there is only one, kept for structural parity with
/// the sibling regression test).
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

#[test]
fn fallback_hits_are_folded_into_process_wide_stats() {
    let _serial = SerialGuard::acquire();

    let alloc = SeferAlloc::new();
    let before = alloc.stats();

    // Small, align>16 so the fastbin magazine path is taken (mirrors
    // `regression_percounter_perheap_aggregation.rs`'s shapes). Large: 4 MiB,
    // comfortably above `SMALL_MAX` in every feature combination this file
    // requires, so it always classifies Large and round-trips through the
    // large-object cache.
    let small_layout = Layout::from_size_align(384, 128).unwrap();
    let large_layout = Layout::from_size_align(4 * 1024 * 1024, 8).unwrap();

    let (local_tcache_delta, local_large_delta) = HeapCore::dbg_with_fallback_for_test(|h| {
        let tcache_before = h.tcache_hits();
        let large_before = h.dbg_large_cache_hits();

        // Magazine hit: alloc, dealloc (into the magazine), alloc again
        // (popped from the magazine — a hit).
        let p1 = h.alloc(small_layout);
        assert!(!p1.is_null(), "fallback small alloc #1 returned null");
        unsafe { h.dealloc(p1, small_layout) };
        let p2 = h.alloc(small_layout);
        assert!(!p2.is_null(), "fallback small alloc #2 returned null");
        unsafe { h.dealloc(p2, small_layout) };

        // Large-cache hit: alloc, dealloc (deposited into the large cache),
        // alloc the SAME size again (served from the cache — a hit).
        let lp1 = h.alloc(large_layout);
        assert!(!lp1.is_null(), "fallback large alloc #1 returned null");
        unsafe { h.dealloc(lp1, large_layout) };
        let lp2 = h.alloc(large_layout);
        assert!(!lp2.is_null(), "fallback large alloc #2 returned null");
        unsafe { h.dealloc(lp2, large_layout) };

        let tcache_after = h.tcache_hits();
        let large_after = h.dbg_large_cache_hits();
        (
            tcache_after.saturating_sub(tcache_before),
            large_after.saturating_sub(large_before),
        )
    })
    .expect("fallback with_heap reported true OOM — unexpected on a fresh process");

    assert!(
        local_tcache_delta > 0,
        "the fallback HeapCore's OWN local tcache_hits() did not advance — the R1-10 \
         bind half of the fix is missing (fallback init never called \
         HeapCore::bind_tcache_hits, so self.tcache_hits stayed None and the per-hit \
         increment in HeapCore::alloc was skipped)"
    );
    assert!(
        local_large_delta > 0,
        "the fallback HeapCore's OWN local dbg_large_cache_hits() did not advance — the \
         R1-10 bind half of the fix is missing for the large-cache counter \
         (fallback init never called HeapCore::bind_large_cache_hits)"
    );

    let after = alloc.stats();
    assert!(
        after.tcache_hits >= before.tcache_hits + local_tcache_delta,
        "SeferAlloc::stats().tcache_hits did not advance by at least the fallback's own \
         observed local delta (before={}, after={}, local_delta={local_tcache_delta}) — \
         the R1-10 aggregation half of the fix is missing (the fallback's counter is \
         bound and advancing locally, but `tcache_hits_total()`'s registry-slot walk \
         does not fold it in)",
        before.tcache_hits,
        after.tcache_hits
    );
    assert!(
        after.large_cache_hits >= before.large_cache_hits + local_large_delta,
        "SeferAlloc::stats().large_cache_hits did not advance by at least the fallback's \
         own observed local delta (before={}, after={}, local_delta={local_large_delta}) \
         — the R1-10 aggregation half of the fix is missing for the large-cache counter",
        before.large_cache_hits,
        after.large_cache_hits
    );
}
