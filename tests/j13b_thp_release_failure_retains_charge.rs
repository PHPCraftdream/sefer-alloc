//! A simulated backend release failure must not recycle advised-byte budget.
//! This proves root error propagation, not an actual kernel refusal. The
//! failed mapping and its charge are deliberately retained until process exit.
#![cfg(all(
    target_os = "linux",
    not(miri),
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    feature = "lazy-commit-fault-injection",
    not(feature = "numa-aware")
))]

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode};
use std::alloc::Layout;

const MIB: usize = 1024 * 1024;
const BUDGET: usize = 8 * MIB;

#[test]
fn failed_release_retains_charge_and_prevents_readmission() {
    let config = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Always)
        .huge_page_block_min_bytes(0)
        .huge_page_budget_bytes(BUDGET);
    let mut core = AllocCore::new_with_config(config).expect("bootstrap");
    core.dbg_set_large_cache_budget(Some(0));
    let layout = Layout::from_size_align(16 * MIB, 8).unwrap();
    let first = core.alloc(layout);
    assert!(!first.is_null());
    assert_eq!(core.large_thp_charge_for_test(first), Some(BUDGET));
    assert_eq!(AllocCore::dbg_thp_ledger_used(), BUDGET);

    aligned_vmem::fault_injection::arm_fail_next_release_backend_result(1);
    // SAFETY: first is live with its original layout and is freed once. The
    // terminal failure consumes ownership; no later access or retry uses it.
    unsafe { core.dealloc(first, layout) };
    aligned_vmem::fault_injection::arm_fail_next_release_backend_result(0);
    assert_eq!(AllocCore::dbg_thp_returned_total(), 0);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), BUDGET);

    // A new reservation must not spend budget still held by the failed release.
    let second = core.alloc(layout);
    assert!(!second.is_null());
    assert_eq!(core.large_thp_charge_for_test(second), Some(0));
    assert_eq!(AllocCore::dbg_thp_charged_total(), BUDGET as u64);
    assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), BUDGET as u64);
    // SAFETY: second is live with its original layout and is freed once.
    unsafe { core.dealloc(second, layout) };
    drop(core);

    // Teardown must not retry an ownership-consumed failed mapping.
    assert_eq!(AllocCore::dbg_thp_returned_total(), 0);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), BUDGET);
}
