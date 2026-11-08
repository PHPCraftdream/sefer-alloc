//! J13b Auto runtime admission: optimistic group share obeys the same budget.
//! Unsupported backends still advise and learn nothing. One scenario per binary.
#![cfg(all(
    not(miri),
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, LargeThpShare};
use std::alloc::Layout;

const MIB: usize = 1024 * 1024;

#[test]
fn auto_runtime_share_obeys_budget_and_only_controls_feed_learning() {
    let config = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Auto)
        .huge_page_block_min_bytes(0)
        .huge_page_budget_bytes(8 * MIB);
    let mut core = AllocCore::new_with_config(config).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(Some(0));
    let tuple = AllocCore::dbg_thp_tuple();
    assert_eq!(tuple.mode, LargeThpMode::Auto);
    assert_eq!(tuple.min_bytes, 0);
    assert_eq!(tuple.budget_bytes, 8 * MIB);
    let ordinal_before = AllocCore::dbg_thp_control_ordinal();
    assert_eq!(
        ordinal_before % 8,
        0,
        "two complete global cohorts are required"
    );

    let layout = Layout::from_size_align(16 * MIB, 8).expect("valid layout");
    let mut pointers = [core::ptr::null_mut(); 16];
    let mut charge = 0;
    for (index, pointer) in pointers.iter_mut().enumerate() {
        if index == 8 && cfg!(any(target_os = "linux", target_os = "android")) {
            assert_eq!(
                AllocCore::dbg_thp_ledger_used(),
                8 * MIB,
                "the second cohort starts with the entire budget already held"
            );
        }
        *pointer = core.alloc(layout);
        assert!(!pointer.is_null());
        charge += core
            .large_thp_charge_for_test(*pointer)
            .expect("live Large header");
    }
    assert_eq!(AllocCore::dbg_thp_learning_telemetry()[2][3..], [0, 0]);
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        assert_eq!(
            charge,
            8 * MIB,
            "Auto's non-controls actually activate budgeted advice"
        );
        assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), (8 * MIB) as u64);
        assert_eq!(AllocCore::dbg_thp_ledger_used(), 8 * MIB);
        assert_eq!(
            AllocCore::dbg_thp_control_ordinal() - ordinal_before,
            16,
            "all sixteen allocations must be fresh and eligible"
        );
        assert_eq!(
            AllocCore::dbg_thp_control_telemetry()[2][0],
            2,
            "budget rejection must not suppress the second cohort's clean control"
        );
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        assert_eq!(charge, 0);
        assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), 0);
        assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    }
    for pointer in pointers {
        // SAFETY: every pointer is live with the unchanged allocation layout.
        unsafe { core.dealloc(pointer, layout) };
    }
    drop(core);
    assert_eq!(AllocCore::dbg_thp_returned_total(), charge as u64);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    let learned = AllocCore::dbg_thp_learning_telemetry();
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        assert_eq!(
            learned[2][3..],
            [2, 0],
            "only two clean controls train; the fourteen non-controls never do"
        );
        assert!(learned[2][0] < u64::from(LargeThpShare::SCALE));
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    assert_eq!(
        learned[2],
        [
            u64::from(LargeThpShare::SCALE),
            u64::from(LargeThpShare::SCALE),
            u64::from(LargeThpShare::SCALE),
            0,
            0
        ]
    );
}
