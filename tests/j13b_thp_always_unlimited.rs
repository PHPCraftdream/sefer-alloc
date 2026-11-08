//! J13b: `Always`, min 8 MiB, budget 0 (unlimited) advises exactly the
//! 2 MiB-floored prefix of a fresh Large span, takes no charge, skips requests
//! below the minimum, and never re-advises on a cache hit. Real advice is
//! asserted on Linux only (elsewhere nothing is advised); physical huge-page
//! backing is never asserted. One scenario per binary (process-global state).

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

fn expected(span: usize) -> u64 {
    if cfg!(target_os = "linux") {
        ThpAdvice::extent(span, LargeThpShare::ONE, None, aligned_vmem::page_size()) as u64
    } else {
        0
    }
}

#[test]
fn always_unlimited_advises_floored_prefix_without_charge() {
    let cfg = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Always)
        .huge_page_block_min_bytes(8 * MIB)
        .huge_page_budget_bytes(0);
    let mut core = AllocCore::new_with_config(cfg).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(None);

    let big = Layout::from_size_align(16 * MIB, 8).unwrap();
    let small = Layout::from_size_align(4 * MIB, 8).unwrap();

    let a = core.alloc(big);
    assert!(!a.is_null());
    let span_a = core.dbg_span_usable_of(a);
    let want = expected(span_a);
    if cfg!(target_os = "linux") {
        assert!(
            want >= 16 * MIB as u64,
            "a 16 MiB request spans at least 16 MiB"
        );
    }
    assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), want);
    assert_eq!(core.large_thp_charge_for_test(a), Some(0));
    assert_eq!(AllocCore::dbg_thp_charged_total(), 0);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);

    // Below the minimum: not advised.
    let b = core.alloc(small);
    assert!(!b.is_null());
    assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), want);
    assert_eq!(core.large_thp_charge_for_test(b), Some(0));

    // SAFETY: both are live with their original layouts.
    unsafe {
        core.dealloc(a, big);
        core.dealloc(b, small);
    }
    // The cache hit reuses the mapping and must not advise again.
    let a2 = core.alloc(big);
    assert_eq!(a2, a, "the cached mapping must be reused");
    assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), want);
    assert_eq!(AllocCore::dbg_thp_charged_total(), 0);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);

    // SAFETY: `a2` is live with its original layout.
    unsafe { core.dealloc(a2, big) };
    drop(core);
    assert_eq!(AllocCore::dbg_thp_returned_total(), 0);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
}
