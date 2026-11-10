//! J13b (Linux): a pre-syscall advice failure (injected) certifies nothing was
//! touched, so the reserved charge is returned at once: the ledger and header
//! charge are zero, nothing counts as advised, charged == returned, and the
//! allocation still succeeds. One scenario per binary.

#![cfg(all(
    target_os = "linux",
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    feature = "lazy-commit-fault-injection",
    not(feature = "numa-aware")
))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode};

const MIB: usize = 1024 * 1024;

#[test]
fn injected_pre_syscall_failure_returns_the_charge() {
    let cfg = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Always)
        .huge_page_block_min_bytes(0)
        .huge_page_budget_bytes(8 * MIB);
    let mut core = AllocCore::new_with_config(cfg).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(None);
    let l = Layout::from_size_align(16 * MIB, 8).unwrap();

    aligned_vmem::fault_injection::arm_fail_next_advise_range(1);
    let p = core.alloc(l);
    aligned_vmem::fault_injection::arm_fail_next_advise_range(0);
    assert!(!p.is_null());
    assert_eq!(core.large_thp_charge_for_test(p), Some(0));
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), 0);
    assert_eq!(AllocCore::dbg_thp_charged_total(), (8 * MIB) as u64);
    assert_eq!(AllocCore::dbg_thp_returned_total(), (8 * MIB) as u64);

    // SAFETY: `p` is live with its original layout.
    unsafe { core.dealloc(p, l) };
    drop(core);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    assert_eq!(
        AllocCore::dbg_thp_charged_total(),
        AllocCore::dbg_thp_returned_total()
    );
}
