//! J13b: when the fresh Large registration fails after advice reserved a
//! charge, the mapping is unmapped and the charge goes back through the
//! release funnel: the ledger is empty and charged == returned, and the
//! allocator keeps working once the injection is cleared. One scenario per
//! binary.

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
const BUDGET: usize = 8 * MIB;

struct Disarm;

impl Drop for Disarm {
    fn drop(&mut self) {
        AllocCore::dbg_set_inject_large_register_fail(false);
    }
}

#[test]
fn registration_failure_returns_the_reserved_charge() {
    let cfg = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Always)
        .huge_page_block_min_bytes(0)
        .huge_page_budget_bytes(BUDGET);
    let mut core = AllocCore::new_with_config(cfg).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(None);
    let l = Layout::from_size_align(16 * MIB, 8).unwrap();
    let reserved = if cfg!(target_os = "linux") {
        BUDGET as u64
    } else {
        0
    };

    let guard = Disarm;
    AllocCore::dbg_set_inject_large_register_fail(true);
    let p = core.alloc(l);
    assert!(p.is_null(), "an injected registration failure yields null");
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    assert_eq!(AllocCore::dbg_thp_charged_total(), reserved);
    assert_eq!(AllocCore::dbg_thp_returned_total(), reserved);

    AllocCore::dbg_set_inject_large_register_fail(false);
    drop(guard);
    let q = core.alloc(l);
    assert!(!q.is_null());
    assert_eq!(core.large_thp_charge_for_test(q), Some(reserved as usize));
    assert_eq!(AllocCore::dbg_thp_ledger_used() as u64, reserved);
    // SAFETY: `q` is live with its original layout.
    unsafe { core.dealloc(q, l) };
    drop(core);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    assert_eq!(
        AllocCore::dbg_thp_charged_total(),
        AllocCore::dbg_thp_returned_total()
    );
    assert_eq!(AllocCore::dbg_thp_charged_total(), 2 * reserved);
}
