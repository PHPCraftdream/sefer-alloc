//! J13b: with a budget the charge is reserved before advice and conserved:
//! the sum of header charges equals the ledger and never exceeds the budget;
//! a cache deposit keeps the charge, a cache hit neither re-charges nor
//! re-advises; each terminal release (decay eviction, rejected deposit, core
//! drop) returns the charge exactly once. Real advice is asserted on Linux
//! only (elsewhere every charge is zero). One scenario per binary.

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

fn page() -> usize {
    aligned_vmem::page_size()
}

/// Charge a fresh 16 MiB block gets while `used` bytes are already charged;
/// the span cap is the observed payload end (header offset plus request).
fn expect_charge(core: &AllocCore, p: *mut u8, used: usize) -> usize {
    if cfg!(target_os = "linux") {
        let (_, _, base, offset, _) = core.large_geometry_for_test(p).expect("large block");
        assert_eq!(p.addr(), base + offset);
        let end = (offset + 16 * MIB).min(core.dbg_span_usable_of(p));
        ThpAdvice::extent(end, LargeThpShare::ONE, Some(BUDGET - used), page())
    } else {
        0
    }
}

#[test]
fn budget_is_conserved_across_deposit_hit_and_every_release() {
    let cfg = LargeCacheConfig::new()
        .huge_page_block_min_bytes(0)
        .huge_page_budget_bytes(BUDGET);
    assert_eq!(
        cfg.dbg_resolved_thp_tuple().mode,
        if cfg!(target_os = "linux") {
            LargeThpMode::Always
        } else {
            LargeThpMode::Off
        }
    );
    let mut core = AllocCore::new_with_config(cfg).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(None);
    // Park decay far away so only the explicit steps below evict.
    core.dbg_set_decay_config(1000, 3_600_000, 1 << 40);

    let l = Layout::from_size_align(16 * MIB, 8).unwrap();
    let a = core.alloc(l);
    let b = core.alloc(l);
    let c = core.alloc(l);
    assert!(!a.is_null() && !b.is_null() && !c.is_null());

    let ca = expect_charge(&core, a, 0);
    let cb = expect_charge(&core, b, ca);
    let cc = expect_charge(&core, c, ca + cb);
    if cfg!(target_os = "linux") {
        assert_eq!(ca, BUDGET, "the first block takes the whole budget");
        assert_eq!((cb, cc), (0, 0), "later blocks find the budget exhausted");
    }
    assert_eq!(core.large_thp_charge_for_test(a), Some(ca));
    assert_eq!(core.large_thp_charge_for_test(b), Some(cb));
    assert_eq!(core.large_thp_charge_for_test(c), Some(cc));
    let used = ca + cb + cc;
    assert!(used <= BUDGET);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), used);
    assert_eq!(AllocCore::dbg_thp_charged_total(), used as u64);
    assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), used as u64);
    assert_eq!(AllocCore::dbg_thp_returned_total(), 0);

    // A deposit keeps the charge.
    // SAFETY: `a` is live with its original layout.
    unsafe { core.dealloc(a, l) };
    assert!(core
        .dbg_large_cache_slot_sizes()
        .iter()
        .any(Option::is_some));
    assert_eq!(AllocCore::dbg_thp_ledger_used(), used);
    assert_eq!(AllocCore::dbg_thp_returned_total(), 0);

    // A hit reuses the mapping without re-charging or re-advising.
    let a2 = core.alloc(l);
    assert_eq!(a2, a);
    assert_eq!(core.large_thp_charge_for_test(a2), Some(ca));
    assert_eq!(AllocCore::dbg_thp_ledger_used(), used);
    assert_eq!(AllocCore::dbg_thp_charged_total(), used as u64);
    assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), used as u64);

    // Decay eviction (every cached mapping unmapped) returns the charge once.
    // SAFETY: `a2` is live with its original layout.
    unsafe { core.dealloc(a2, l) };
    assert_eq!(AllocCore::dbg_thp_ledger_used(), used);
    core.dbg_set_decay_config(10_000, 0, 0);
    core.dbg_force_decay_tick();
    assert_eq!(core.dbg_large_cache_used(), 0);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), cb + cc);
    assert_eq!(AllocCore::dbg_thp_returned_total(), ca as u64);
    core.dbg_set_decay_config(1000, 3_600_000, 1 << 40);

    // The budget is free again: a fresh block takes it.
    let d = core.alloc(l);
    assert!(!d.is_null());
    let cd = expect_charge(&core, d, cb + cc);
    assert_eq!(core.large_thp_charge_for_test(d), Some(cd));
    assert_eq!(AllocCore::dbg_thp_ledger_used(), cb + cc + cd);
    assert!(AllocCore::dbg_thp_ledger_used() <= BUDGET);

    // A rejected deposit releases at once and returns the charge.
    core.dbg_set_large_cache_budget(Some(0));
    // SAFETY: `d` is live with its original layout.
    unsafe { core.dealloc(d, l) };
    assert_eq!(AllocCore::dbg_thp_ledger_used(), cb + cc);
    assert_eq!(AllocCore::dbg_thp_returned_total(), (ca + cd) as u64);
    core.dbg_set_large_cache_budget(None);

    // One more charged block, left in the cache for the core drop to release.
    let f = core.alloc(l);
    let cf = expect_charge(&core, f, cb + cc);
    assert_eq!(core.large_thp_charge_for_test(f), Some(cf));
    // SAFETY: all three are live with their original layouts.
    unsafe {
        core.dealloc(f, l);
        core.dealloc(b, l);
        core.dealloc(c, l);
    }
    assert_eq!(AllocCore::dbg_thp_ledger_used(), cb + cc + cf);

    drop(core);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    assert_eq!(
        AllocCore::dbg_thp_charged_total(),
        AllocCore::dbg_thp_returned_total()
    );
    assert_eq!(
        AllocCore::dbg_thp_charged_total(),
        (ca + cb + cc + cd + cf) as u64
    );
}
