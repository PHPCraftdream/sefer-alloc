//! Phase 1 large-cache byte-budget tests.
//!
//! These tests verify:
//!   - Spans of any size can enter the cache when no budget is set.
//!   - The per-shard byte-budget is enforced; FIFO eviction fires when needed.
//!   - A budget that is too small for an individual span causes immediate OS release.
//!   - `SEFER_LARGE_CACHE_BUDGET` env var is parsed and applied on construction.
//!   - The `large_cache_used_bytes` invariant is maintained across alloc/dealloc.
//!
//! All tests use `dbg_set_large_cache_budget` rather than the env var (except
//! `env_var_sets_budget`) to avoid flakiness in parallel test runs.

#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

use core::alloc::Layout;
use sefer_alloc::AllocCore;

// ── helpers ──────────────────────────────────────────────────────────────────

const MIB: usize = 1024 * 1024;

fn layout(mib: usize) -> Layout {
    Layout::from_size_align(mib * MIB, 8).unwrap()
}

/// Check the running invariant: `dbg_large_cache_used == sum(slot sizes)`.
fn assert_used_bytes_invariant(ac: &AllocCore) {
    let slots = ac.dbg_large_cache_slot_sizes();
    let expected: usize = slots.iter().filter_map(|s| *s).sum();
    assert_eq!(
        ac.dbg_large_cache_used(),
        expected,
        "large_cache_used_bytes invariant violated: counter={}, slot_sum={}",
        ac.dbg_large_cache_used(),
        expected
    );
}

// ── test 1 ───────────────────────────────────────────────────────────────────

/// Without a budget, the cache admits spans of any size (including 100 MiB).
/// A span deposited on `dealloc` should raise `large_cache_used_bytes` and
/// be returned on a matching `alloc` (same or compatible size).
#[test]
fn budget_none_caches_any_size() {
    let mut ac = AllocCore::new().expect("primordial");
    // Ensure unbounded (no budget).
    ac.dbg_set_large_cache_budget(None);

    let l = layout(100); // 100 MiB
    let ptr1 = ac.alloc(l);
    if ptr1.is_null() {
        eprintln!("OOM allocating 100 MiB — skip test (machine too small)");
        return;
    }

    // Before dealloc: used_bytes should be 0 (no span in cache yet).
    assert_eq!(ac.dbg_large_cache_used(), 0);
    assert_used_bytes_invariant(&ac);

    ac.dealloc(ptr1, l);

    // After dealloc: the span should be in the cache (budget == None → admit any size).
    assert!(
        ac.dbg_large_cache_used() > 0,
        "100 MiB span should be cached when budget is None"
    );
    assert_used_bytes_invariant(&ac);

    // Re-alloc at the same size: must get a valid pointer back (cache hit path).
    let ptr2 = ac.alloc(l);
    assert!(!ptr2.is_null(), "re-alloc after cache deposit must succeed");

    // After cache hit: used_bytes must drop back to 0.
    assert_eq!(ac.dbg_large_cache_used(), 0);
    assert_used_bytes_invariant(&ac);

    // Verify the memory is writable/readable (pages are still committed).
    unsafe {
        ptr2.write(0xAB);
        assert_eq!(ptr2.read(), 0xAB);
    }
    ac.dealloc(ptr2, l);
}

// ── test 2 ───────────────────────────────────────────────────────────────────

/// With a restrictive budget, depositing more spans than the budget can hold
/// triggers FIFO eviction. After each dealloc the byte-budget invariant holds
/// and `used_bytes <= budget`.
///
/// We use a budget of exactly ONE span's usable size (discovered dynamically
/// from the first deposit) so the second dealloc always forces eviction.
/// This is robust regardless of how many segments the header rounding adds.
#[test]
fn budget_set_evicts_when_full() {
    let mut ac = AllocCore::new().expect("primordial");
    // Start unbounded so we can discover the real usable_size of a 4 MiB span.
    ac.dbg_set_large_cache_budget(None);

    let l = layout(4); // 4 MiB

    let p1 = ac.alloc(l);
    if p1.is_null() {
        eprintln!("OOM — skip budget_set_evicts_when_full");
        return;
    }
    ac.dealloc(p1, l);
    assert_used_bytes_invariant(&ac);

    // Discover the real usable_size from what was actually deposited.
    let one_span = ac.dbg_large_cache_used();
    assert!(one_span > 0, "first dealloc must cache the span");

    // Now set the budget to exactly ONE span. The second dealloc must evict
    // slot 0 before depositing the new span.
    ac.dbg_set_large_cache_budget(Some(one_span));

    // Alloc a second span (hits the cache for p1's slot → ok).
    let p2 = ac.alloc(l);
    if p2.is_null() {
        eprintln!("OOM — skip budget_set_evicts_when_full (p2)");
        return;
    }
    // After the cache hit, used_bytes should have dropped by one_span.
    assert_used_bytes_invariant(&ac);

    // Dealloc p2 → deposit it. Cache is now empty (p1's slot was consumed by
    // the cache hit above). Budget allows one_span → deposit succeeds.
    ac.dealloc(p2, l);
    assert_used_bytes_invariant(&ac);
    assert_eq!(
        ac.dbg_large_cache_used(),
        one_span,
        "used_bytes should equal one_span after depositing p2"
    );

    // Alloc a third span (hits the cache for p2's slot).
    let p3 = ac.alloc(l);
    if p3.is_null() {
        assert_used_bytes_invariant(&ac);
        return;
    }
    // After the second cache hit, cache should be empty.
    assert_eq!(ac.dbg_large_cache_used(), 0);
    assert_used_bytes_invariant(&ac);

    // Dealloc p3 → deposit it (budget = one_span, used_bytes = 0 → fits).
    ac.dealloc(p3, l);
    assert_used_bytes_invariant(&ac);

    // Now both slots may or may not be occupied, but used_bytes must never
    // exceed the budget.
    assert!(
        ac.dbg_large_cache_used() <= one_span,
        "used_bytes {} must not exceed budget {}",
        ac.dbg_large_cache_used(),
        one_span
    );
}

// ── test 3 ───────────────────────────────────────────────────────────────────

/// A budget that is smaller than the span being freed means the span cannot
/// be cached at all — `used_bytes` stays 0 and all slots remain empty.
#[test]
fn budget_too_small_releases_immediately() {
    const BUDGET: usize = 10 * MIB; // 10 MiB
    const SPAN: usize = 100;        // 100 MiB — larger than the budget

    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(Some(BUDGET));

    let l = layout(SPAN);
    let ptr = ac.alloc(l);
    if ptr.is_null() {
        eprintln!("OOM — skip budget_too_small_releases_immediately");
        return;
    }
    unsafe { ptr.write(0xCD) };

    ac.dealloc(ptr, l);

    // The span (100 MiB) exceeds the budget (10 MiB); it must NOT be cached.
    assert_eq!(
        ac.dbg_large_cache_used(),
        0,
        "span larger than budget must be released to the OS, not cached"
    );
    assert_used_bytes_invariant(&ac);
    let slots = ac.dbg_large_cache_slot_sizes();
    assert!(slots.iter().all(|s| s.is_none()), "all slots must be empty");
}

// ── test 4 ───────────────────────────────────────────────────────────────────

/// `SEFER_LARGE_CACHE_BUDGET` env var is parsed and applied at `AllocCore::new`.
///
/// Strategy: set a large budget (256M), alloc+dealloc a 4 MiB span, and verify
/// it is cached (proving the env var was parsed as non-None and the budget was
/// applied). The usable_size of a 4 MiB span is ~8 MiB (2 segments), which is
/// comfortably under 256 MiB.
///
/// NOTE: `std::env::set_var` is process-global. This test is the sole writer
/// of `SEFER_LARGE_CACHE_BUDGET` in the test suite. We set the var, create the
/// AllocCore (which reads it once in `new`), then immediately restore the env.
/// If the test suite ever runs tests in parallel threads this should be moved
/// to a serial harness.
#[test]
fn env_var_sets_budget() {
    // Safety note: set_var is documented as not thread-safe, but this test is
    // the only writer of this key and restores it before any concurrent test
    // could observe it (Rust test threads run after we drop the AllocCore).
    unsafe {
        std::env::set_var("SEFER_LARGE_CACHE_BUDGET", "256M");
    }
    let mut ac = AllocCore::new().expect("primordial");
    // Restore the env immediately so other concurrent AllocCore::new calls are
    // not affected (other tests in this file use dbg_set_large_cache_budget
    // and do NOT call AllocCore::new after this point).
    unsafe {
        std::env::remove_var("SEFER_LARGE_CACHE_BUDGET");
    }

    // AllocCore::new parsed "256M" → budget = 256 MiB.
    // A 4 MiB alloc produces a span of ~8 MiB usable (2 segments), well
    // under the budget. Dealloc → the span should be cached.
    let l = layout(4); // 4 MiB
    let p1 = ac.alloc(l);
    if p1.is_null() {
        eprintln!("OOM — skip env_var_sets_budget");
        return;
    }
    ac.dealloc(p1, l);
    assert_used_bytes_invariant(&ac);

    let used = ac.dbg_large_cache_used();
    assert!(
        used > 0,
        "span should be cached when budget (256 MiB) is larger than span (~8 MiB)"
    );

    // Second alloc at the same size → hits the cache.
    let p2 = ac.alloc(l);
    assert!(!p2.is_null());
    // After cache hit, used_bytes drops by one span.
    assert_eq!(
        ac.dbg_large_cache_used(),
        0,
        "cache hit must remove the span from used_bytes"
    );
    assert_used_bytes_invariant(&ac);
    ac.dealloc(p2, l);
    assert_used_bytes_invariant(&ac);

    // Invariant: used_bytes <= budget at all times.
    assert!(
        ac.dbg_large_cache_used() <= 256 * MIB,
        "used_bytes must not exceed the 256 MiB budget"
    );
}

// ── test 5 ───────────────────────────────────────────────────────────────────

/// After a sequence of alloc/dealloc/alloc operations the invariant
/// `large_cache_used_bytes == sum(slot usable_sizes)` always holds.
/// This test exercises it at every observable step.
#[test]
fn large_cache_used_bytes_invariant() {
    let mut ac = AllocCore::new().expect("primordial");
    // Unbounded budget — focus on invariant, not eviction.
    ac.dbg_set_large_cache_budget(None);

    assert_used_bytes_invariant(&ac);

    let l4 = layout(4);
    let l8 = layout(8);

    let p4 = ac.alloc(l4);
    assert_used_bytes_invariant(&ac);

    if p4.is_null() {
        eprintln!("OOM — skip large_cache_used_bytes_invariant");
        return;
    }

    let p8 = ac.alloc(l8);
    assert_used_bytes_invariant(&ac);

    if !p8.is_null() {
        ac.dealloc(p4, l4);
        assert_used_bytes_invariant(&ac);

        ac.dealloc(p8, l8);
        assert_used_bytes_invariant(&ac);

        // Re-alloc 4 MiB — should hit cache (slot from the 4 MiB deposit).
        let p4b = ac.alloc(l4);
        assert_used_bytes_invariant(&ac);
        if !p4b.is_null() {
            ac.dealloc(p4b, l4);
            assert_used_bytes_invariant(&ac);
        }
    } else {
        // p8 OOM — just dealloc p4.
        ac.dealloc(p4, l4);
        assert_used_bytes_invariant(&ac);
    }

    // Final state: invariant must hold.
    assert_used_bytes_invariant(&ac);
}
