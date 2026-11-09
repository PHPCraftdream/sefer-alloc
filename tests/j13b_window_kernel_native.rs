//! Optimized real-payload GlobalAlloc facade activation, with System bookkeeping.
#![cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]
#[path = "../examples/_shared/j13b_owned_block.rs"]
mod owned;
#[path = "../examples/_shared/j13b_run_stats.rs"]
mod run_stats;
#[path = "../examples/_shared/j13b_window_kernel.rs"]
mod window_kernel;
#[path = "../examples/_shared/j13b_workload.rs"]
#[allow(dead_code)]
mod workload;
use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, SeferAlloc};
use window_kernel::WindowKernel;
use workload::{Shape, TouchPlan};

#[test]
fn optimized_facade_windows_keep_real_dense_sparse_growth_and_worker_activation() {
    const MIB: usize = 1024 * 1024;
    const GROUP: usize = 1;
    let page = aligned_vmem::page_size();
    assert!(page.is_power_of_two() && page < MIB);
    let bytes = 8 * MIB - page;
    // This object, and only this object, serves payload allocations. Rust test
    // and kernel bookkeeping use System; no competing config claims the slots.
    let allocator = SeferAlloc::with_config(
        LargeCacheConfig::new()
            .budget_bytes(0)
            .huge_page_mode(LargeThpMode::Auto)
            .huge_page_block_min_bytes(bytes)
            .huge_page_budget_bytes(0),
    );
    let dense = [TouchPlan {
        bytes,
        shape: Shape::Dense,
    }; 8];
    let dense_stats =
        WindowKernel::batch(&allocator, &dense, page).expect("optimized dense window");
    assert_eq!(dense_stats.allocations, 8);
    assert_eq!(dense_stats.frees, 8);
    assert_eq!(dense_stats.bytes_written, u64::try_from(bytes * 8).unwrap());
    assert_eq!(allocator.dbg_current_large_cache_budget(), Some(0));
    let tuple = AllocCore::dbg_thp_tuple();
    assert_eq!(tuple.mode, LargeThpMode::Auto);
    assert_eq!(tuple.min_bytes, bytes);
    assert_eq!(tuple.budget_bytes, 0);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
    assert_eq!(allocator.stats().config_conflicts, 0);
    let first = AllocCore::dbg_thp_control_telemetry()[GROUP];
    assert_eq!(&first[..6], &[1, 0, 1, 0, 0, 0]);
    assert!(
        first[7] > 0 && u128::from(first[6]) * 100 >= u128::from(first[7]) * 95,
        "release-optimized dense writes must really fault the control pages"
    );
    let before = AllocCore::dbg_thp_learning_telemetry();
    let sparse = [TouchPlan {
        bytes,
        shape: Shape::Sparse,
    }; 8];
    let sparse_stats =
        WindowKernel::batch(&allocator, &sparse, page).expect("optimized sparse window");
    assert_eq!(sparse_stats.bytes_written, 8);
    assert_eq!(
        AllocCore::dbg_thp_control_ordinal(),
        16,
        "cache disabled: all eligible payloads fresh"
    );
    let second = AllocCore::dbg_thp_control_telemetry()[GROUP];
    assert_eq!(second[0] - first[0], 1);
    assert_eq!(second[2] - first[2], 1);
    let resident = second[6] - first[6];
    let sampled = second[7] - first[7];
    assert!(sampled > 0 && u128::from(resident) * 100 < u128::from(sampled));
    let after = AllocCore::dbg_thp_learning_telemetry();
    assert!(after[GROUP][0] < before[GROUP][0]);
    assert_eq!(after[GROUP][3], 2);
    assert_eq!(after[GROUP][4], 0);

    let growth = WindowKernel::grow(&allocator, 1024, 64 * 1024, page)
        .expect("real installed-layer reallocs");
    assert_eq!(growth.reallocations, 6);
    assert_eq!(growth.bytes_written, 64 * 1024);
    let churn = WindowKernel::churn(
        &allocator,
        TouchPlan {
            bytes: 64,
            shape: Shape::Quarter,
        },
        4,
        2,
        page,
        |_, actor_allocator| {
            assert_eq!(actor_allocator.dbg_current_large_cache_budget(), Some(0));
            assert_eq!(actor_allocator.stats().config_conflicts, 0);
            Ok(())
        },
    )
    .expect("actual four-worker payload facade");
    assert_eq!(churn.allocations, 8);
    assert_eq!(churn.frees, 8);
    assert_eq!(churn.bytes_written, 128);
    assert_eq!(
        AllocCore::dbg_thp_control_ordinal(),
        16,
        "small probes do not create controls"
    );
    assert_eq!(AllocCore::dbg_thp_learning_telemetry(), after);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
    assert_eq!(allocator.stats().config_conflicts, 0);
}
