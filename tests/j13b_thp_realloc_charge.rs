//! J13b: growing an advised Large block never advises more or changes its
//! charge when it grows in place; when it moves, the old mapping's charge returns exactly
//! once through the normal free path (the cache is closed so the old mapping
//! is released at once). The new mapping is fresh but finds the budget taken
//! by the still-live old one. One scenario per binary.

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

#[test]
fn realloc_returns_old_charge_once_and_never_widens() {
    let cfg = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Always)
        .huge_page_block_min_bytes(0)
        .huge_page_budget_bytes(BUDGET);
    let mut core = AllocCore::new_with_config(cfg).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(Some(0));

    let l = Layout::from_size_align(16 * MIB, 8).unwrap();
    let p = core.alloc(l);
    assert!(!p.is_null());
    let charge = if cfg!(target_os = "linux") {
        // The span cap is the observed payload end (header offset plus request).
        let (_, _, base, offset, _) = core.large_geometry_for_test(p).expect("large block");
        assert_eq!(p.addr(), base + offset);
        ThpAdvice::extent(
            (offset + 16 * MIB).min(core.dbg_span_usable_of(p)),
            LargeThpShare::ONE,
            Some(BUDGET),
            aligned_vmem::page_size(),
        )
    } else {
        0
    };
    assert_eq!(core.large_thp_charge_for_test(p), Some(charge));
    assert_eq!(AllocCore::dbg_thp_ledger_used(), charge);
    let advised = AllocCore::dbg_thp_advised_bytes_total();
    assert_eq!(advised, charge as u64);

    grow_free_and_check(&mut core, p, l, charge, advised);
    drop(core);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    assert_eq!(
        AllocCore::dbg_thp_charged_total(),
        AllocCore::dbg_thp_returned_total()
    );
}

fn grow_free_and_check(core: &mut AllocCore, p: *mut u8, l: Layout, charge: usize, advised: u64) {
    let charged_before = AllocCore::dbg_thp_charged_total();
    // SAFETY: `p` is live with layout `l`.
    let q = unsafe { core.realloc(p, l, 64 * MIB) };
    assert!(!q.is_null());
    if q == p {
        // In-place growth keeps the one mapping, its charge and its advice.
        assert_eq!(core.large_thp_charge_for_test(q), Some(charge));
        assert_eq!(AllocCore::dbg_thp_ledger_used(), charge);
        assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), advised);
    } else {
        // Moved: the budget was still held by the old mapping when the new
        // one was reserved (new charge 0), then the old one was released.
        assert_eq!(core.large_thp_charge_for_test(q), Some(0));
        assert_eq!(AllocCore::dbg_thp_charged_total(), charged_before);
        assert_eq!(AllocCore::dbg_thp_returned_total(), charge as u64);
        assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
        assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), advised);
    }
    let l64 = Layout::from_size_align(64 * MIB, 8).unwrap();
    // SAFETY: `q` is live with layout `l64`.
    unsafe { core.dealloc(q, l64) };
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    assert_eq!(AllocCore::dbg_thp_returned_total(), charge as u64);
}
