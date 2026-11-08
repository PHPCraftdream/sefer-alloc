//! J13b: `Auto` is stored and readable, but its learned share is a seam that
//! returns zero until the learning scopes land, so nothing is advised and the
//! ledger is untouched even with min 0 and a budget. One scenario per binary.

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
fn auto_advises_nothing_until_learning_is_wired() {
    let cfg = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Auto)
        .huge_page_block_min_bytes(0)
        .huge_page_budget_bytes(8 * MIB);
    let mut core = AllocCore::new_with_config(cfg).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(None);

    let tuple = AllocCore::dbg_thp_tuple();
    assert_eq!(tuple.mode, LargeThpMode::Auto);
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
