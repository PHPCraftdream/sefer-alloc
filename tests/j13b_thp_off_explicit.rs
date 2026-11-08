//! J13b: an explicit `Off` (with a budget set) publishes a tuple but still
//! advises nothing and leaves the ledger untouched. One scenario per binary.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode};

const MIB: usize = 1024 * 1024;

#[test]
fn explicit_off_with_budget_never_advises_or_charges() {
    let cfg = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Off)
        .huge_page_block_min_bytes(0)
        .huge_page_budget_bytes(8 * MIB);
    let mut core = AllocCore::new_with_config(cfg).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(None);

    let tuple = AllocCore::dbg_thp_tuple();
    assert_eq!(tuple.mode, LargeThpMode::Off);
    assert_eq!(tuple.min_bytes, 0);
    assert_eq!(tuple.budget_bytes, 8 * MIB);

    let layout = Layout::from_size_align(16 * MIB, 8).unwrap();
    let p = core.alloc(layout);
    assert!(!p.is_null());
    assert_eq!(core.large_thp_charge_for_test(p), Some(0));
    assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), 0);
    assert_eq!(AllocCore::dbg_thp_charged_total(), 0);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);

    // SAFETY: `p` is live with its original layout.
    unsafe { core.dealloc(p, layout) };
    drop(core);
    assert_eq!(AllocCore::dbg_thp_returned_total(), 0);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
}
