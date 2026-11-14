//! The budgeted-advice charge in the Large header survives a cache deposit
//! and a cache-hit reuse of the same mapping, and is returned exactly once
//! at terminal release — exercised through the real advice mechanism (Always
//! mode with a budget), never through a test-only field stamp.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use std::alloc::Layout;

use sefer_alloc::alloc_core::policy::large_thp_runtime::ThpAdvice;
use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, LargeThpShare};

const MIB: usize = 1024 * 1024;
const BUDGET: usize = 8 * MIB;
// Requested size; its span is rounded up to 8 MiB but advice ends at its own end.
const SPAN: usize = 4 * MIB;

/// Budgeted advice for the `SPAN` block: the span cap is the observed payload
/// end (header offset plus request), not the rounded-up usable span.
fn trimmed_extent(core: &AllocCore, p: *mut u8) -> usize {
    let (_, _, base, offset, _) = core.large_geometry_for_test(p).expect("large block");
    assert_eq!(p.addr(), base + offset);
    let end = (offset + SPAN).min(core.dbg_span_usable_of(p));
    ThpAdvice::extent(
        end,
        LargeThpShare::ONE,
        Some(BUDGET),
        aligned_vmem::page_size(),
    )
}

#[test]
fn charge_carries_across_deposit_hit_and_is_returned_exactly_once() {
    // Baseline the process-global counters before anything is charged; the
    // second test in this file allocates no Large block, so it never moves
    // them and this binary's totals are ours alone.
    let charged_before = AllocCore::dbg_thp_charged_total();
    let returned_before = AllocCore::dbg_thp_returned_total();
    let advised_before = AllocCore::dbg_thp_advised_bytes_total();

    let cfg = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Always)
        .huge_page_block_min_bytes(0)
        .huge_page_budget_bytes(BUDGET);
    let mut core = AllocCore::new_with_config(cfg).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(None);
    // Park decay far away so only the explicit steps below evict.
    core.dbg_set_decay_config(1000, 3_600_000, 1 << 40);

    let layout = Layout::from_size_align(SPAN, 8).unwrap();
    let first = core.alloc(layout);
    assert!(!first.is_null());
    // Linux/Android exercise real advice; other targets check only the zero-charge lifecycle.
    let expected = if cfg!(any(target_os = "linux", target_os = "android")) {
        trimmed_extent(&core, first)
    } else {
        0
    };
    assert_eq!(core.large_thp_charge_for_test(first), Some(expected));
    assert_eq!(AllocCore::dbg_thp_ledger_used(), expected);
    assert_eq!(
        AllocCore::dbg_thp_charged_total() - charged_before,
        expected as u64
    );
    assert_eq!(
        AllocCore::dbg_thp_advised_bytes_total(),
        advised_before + expected as u64
    );

    // Dealloc to a cache deposit: the charge is preserved, nothing returned.
    // SAFETY: `first` is live with its original layout.
    unsafe { core.dealloc(first, layout) };
    assert!(core
        .dbg_large_cache_slot_sizes()
        .iter()
        .any(Option::is_some));
    assert_eq!(core.large_thp_charge_for_test(first), None);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), expected);
    assert_eq!(AllocCore::dbg_thp_returned_total(), returned_before);

    // Alloc again: the same mapping is reused (cache hit) with no re-advise
    // and no re-charge.
    let again = core.alloc(layout);
    assert!(!again.is_null());
    assert_eq!(again, first, "the hit must reuse the same base pointer");
    assert_eq!(core.large_thp_charge_for_test(again), Some(expected));
    assert_eq!(AllocCore::dbg_thp_ledger_used(), expected);
    assert_eq!(
        AllocCore::dbg_thp_charged_total() - charged_before,
        expected as u64
    );
    assert_eq!(
        AllocCore::dbg_thp_advised_bytes_total(),
        advised_before + expected as u64
    );

    // SAFETY: `again` is live with its original layout.
    unsafe { core.dealloc(again, layout) };
    assert!(core.dbg_large_cache_used() > 0);
    assert_eq!(AllocCore::dbg_thp_returned_total(), returned_before);
    // Forced decay eviction unmaps the cache and returns the charge once.
    core.dbg_set_decay_config(10_000, 0, 0);
    core.dbg_force_decay_tick();
    assert_eq!(core.dbg_large_cache_used(), 0);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    assert_eq!(
        AllocCore::dbg_thp_returned_total() - returned_before,
        expected as u64
    );
    assert_eq!(core.large_thp_charge_for_test(first), None);
    core.dbg_set_decay_config(1000, 3_600_000, 1 << 40);

    // A fresh block charges again; dropping the core releases it.
    let fresh = core.alloc(layout);
    assert!(!fresh.is_null());
    let fresh_expected = if cfg!(any(target_os = "linux", target_os = "android")) {
        trimmed_extent(&core, fresh)
    } else {
        0
    };
    assert_eq!(core.large_thp_charge_for_test(fresh), Some(fresh_expected));
    assert_eq!(AllocCore::dbg_thp_ledger_used(), fresh_expected);
    drop(core);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    assert_eq!(
        AllocCore::dbg_thp_charged_total() - charged_before,
        AllocCore::dbg_thp_returned_total() - returned_before
    );
    // On Linux the mechanism is live: the charge must be the real budgeted
    // extent, not the vacuous zero.
    if cfg!(target_os = "linux") {
        assert!(expected > 0, "Linux must produce a nonzero charge");
        assert_eq!(expected, fresh_expected);
        assert_eq!(expected, SPAN, "the charge ends at the requested size");
    }
}

#[test]
fn observer_rejects_non_large_pointer() {
    let mut core = AllocCore::new().expect("primordial bootstrap");
    let small = Layout::from_size_align(64, 8).unwrap();
    let p = core.alloc(small);
    assert!(!p.is_null());
    assert_eq!(core.large_thp_charge_for_test(p), None);
    // SAFETY: `p` is live with its original layout.
    unsafe { core.dealloc(p, small) };
}
