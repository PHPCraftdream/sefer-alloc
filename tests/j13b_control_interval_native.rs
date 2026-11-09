//! Bind the decoder to actual quiescent facade cohorts and captured tickets.
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
#[path = "../examples/_shared/j13b_control_interval.rs"]
mod control_interval;
#[path = "../examples/_shared/j13b_control_snapshot.rs"]
mod control_snapshot;
#[path = "../examples/_shared/j13b_owned_block.rs"]
mod owned;
#[path = "../examples/_shared/j13b_run_stats.rs"]
mod run_stats;
#[path = "../examples/_shared/j13b_window_kernel.rs"]
#[allow(dead_code)]
mod window_kernel;
#[path = "../examples/_shared/j13b_workload.rs"]
#[allow(dead_code)]
mod workload;
use control_interval::ControlInterval;
use control_snapshot::ControlSnapshot;
use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, SeferAlloc};
use window_kernel::WindowKernel;
use workload::{Shape, TouchPlan};

#[test]
fn actual_same_size_owner_cohorts_bind_single_sample_and_ticket_capture() {
    const MIB: usize = 1024 * 1024;
    const GROUP: usize = 1;
    let page = aligned_vmem::page_size();
    assert!(page.is_power_of_two() && page < MIB);
    let bytes = 8 * MIB - page;
    // Exactly one payload allocator; process-global bookkeeping stays on System.
    let allocator = SeferAlloc::with_config(
        LargeCacheConfig::new()
            .budget_bytes(0)
            .huge_page_mode(LargeThpMode::Auto)
            .huge_page_block_min_bytes(bytes)
            .huge_page_budget_bytes(0),
    );
    for shape in [Shape::Dense, Shape::Sparse] {
        // No workers or payload operations are live at either boundary.
        AllocCore::dbg_thp_control_ticket_log_reset();
        let before = ControlSnapshot::capture();
        let stats = WindowKernel::batch(&allocator, &[TouchPlan { bytes, shape }; 8], page)
            .expect("actual same-size complete cohort");
        let after = ControlSnapshot::capture();
        let (tickets, cursor) = AllocCore::dbg_thp_control_ticket_log()
            .expect("supported actual capture, no dropped or invalid entries");
        assert_eq!(cursor, 8);
        for &entry in &tickets[..8] {
            assert_eq!(
                (entry >> 1) & 1 != 0,
                AllocCore::dbg_thp_control_decision_at(entry >> 2)
            );
        }
        assert_eq!(stats.allocations, 8);
        assert_eq!(stats.frees, 8);
        assert_eq!(
            stats.requested_bytes_issued,
            u64::try_from(bytes * 8).unwrap()
        );
        assert_eq!(allocator.dbg_current_large_cache_budget(), Some(0));
        assert_eq!(allocator.stats().config_conflicts, 0);
        let interval = ControlInterval::close(GROUP, page, &before, &after, &tickets[..8])
            .expect("one actual owner-terminal control observation");
        assert_eq!(interval.eligible_fresh, 8);
        assert_eq!(interval.reasons, [0, 1, 0, 0, 0]);
        assert_eq!(interval.applied, 1);
        assert_eq!(interval.dropped, 0);
        assert!(interval.sampled_pages > 0);
        let sparse = shape == Shape::Sparse;
        assert!(interval.class_valid(sparse));
        assert!(!interval.class_valid(!sparse));
    }
    let tuple = AllocCore::dbg_thp_tuple();
    assert_eq!(tuple.mode, LargeThpMode::Auto);
    assert_eq!(tuple.min_bytes, bytes);
    assert_eq!(tuple.budget_bytes, 0);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
}
