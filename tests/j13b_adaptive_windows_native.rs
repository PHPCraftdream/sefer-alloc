//! Same executor at explicitly reduced geometry; never registered campaign data.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]
#[path = "../examples/_shared/j13b_adaptive_windows.rs"]
mod adaptive_windows;
#[path = "../examples/_shared/j13b_control_interval.rs"]
#[allow(dead_code)]
mod control_interval;
#[path = "../examples/_shared/j13b_control_snapshot.rs"]
mod control_snapshot;
#[path = "../examples/_shared/j13b_measured_window.rs"]
#[cfg_attr(any(miri, not(target_os = "linux")), allow(dead_code))]
mod measured_window;
#[path = "../examples/_shared/j13b_os_probe.rs"]
#[allow(dead_code)]
mod os_probe;
#[path = "../examples/_shared/j13b_os_sample.rs"]
#[cfg_attr(any(miri, not(target_os = "linux")), allow(dead_code))]
mod os_sample;
#[path = "../examples/_shared/j13b_owned_block.rs"]
mod owned;
#[path = "../examples/_shared/j13b_owner_training.rs"]
mod owner_training;
#[path = "../examples/_shared/j13b_payload_actors.rs"]
mod payload_actors;
#[path = "../examples/_shared/j13b_run_stats.rs"]
mod run_stats;
#[path = "../examples/_shared/j13b_training_observation.rs"]
#[allow(dead_code)]
mod training_observation;
#[path = "../examples/_shared/j13b_training_run.rs"]
#[allow(dead_code)]
mod training_run;
#[path = "../examples/_shared/j13b_window_execution.rs"]
#[cfg_attr(any(miri, not(target_os = "linux")), allow(dead_code))]
mod window_execution;
#[path = "../examples/_shared/j13b_window_kernel.rs"]
#[allow(dead_code)]
mod window_kernel;
#[path = "../examples/_shared/j13b_workload.rs"]
#[allow(dead_code)]
mod workload;
use adaptive_windows::AdaptiveWindows;
use sefer_alloc::{LargeCacheConfig, SeferAlloc};

#[cfg(any(miri, not(target_os = "linux")))]
#[test]
fn adaptive_windows_reject_model_or_unsupported_os_before_payloads() {
    let allocator = SeferAlloc::with_config(LargeCacheConfig::new());
    let before = control_snapshot::ControlSnapshot::capture();
    for scenario in ["phased", "mixed"] {
        let failure = AdaptiveWindows::run(&allocator, scenario, 4096).unwrap_err();
        assert_eq!(
            failure.to_string(),
            "adaptive windows require native Linux OS epochs"
        );
    }
    assert_eq!(control_snapshot::ControlSnapshot::capture(), before);
}

#[cfg(all(not(miri), target_os = "linux"))]
#[test]
fn actual_phased_and_mixed_windows_preserve_training_and_live_os_boundaries() {
    use control_snapshot::ControlSnapshot;
    use os_probe::OsProbe;
    use sefer_alloc::{AllocCore, LargeThpMode};
    use window_execution::WindowExecution;
    use workload::{Bucket, Cell, Phase, TrainingState, TRAINING_TARGET};
    const MIB: usize = 1024 * 1024;
    let page = aligned_vmem::page_size();
    let allocator = SeferAlloc::with_config(
        LargeCacheConfig::new()
            .budget_bytes(0)
            .huge_page_mode(LargeThpMode::Auto)
            .huge_page_block_min_bytes(MIB)
            .huge_page_budget_bytes(0),
    );
    fn smoke(scenario: &str) -> Vec<Cell> {
        let mut cells = workload::plan(scenario).unwrap();
        for cell in &mut cells {
            let sparse_mixed = scenario == "mixed"
                && matches!(cell.phase, Phase::TrainSparse | Phase::TrainedSparse);
            cell.bytes = if sparse_mixed { 8 * MIB } else { MIB };
            cell.blocks = 1;
            cell.bucket = if sparse_mixed {
                Bucket::EightTo32
            } else {
                Bucket::OneTo8
            };
            cell.minimum_request = MIB;
            if let Some((shape, _, _)) = cell.co_resident {
                cell.co_resident = Some((shape, 8 * MIB, 1));
            }
        }
        cells
    }
    let phased = smoke("phased");
    // Rejection must precede claims and resets; not merely fail at a later probe.
    let before = ControlSnapshot::capture();
    assert_eq!(
        AdaptiveWindows::run(&allocator, "cycle", page)
            .unwrap_err()
            .to_string(),
        "unsupported adaptive scenario"
    );
    let mut reordered = phased.clone();
    reordered.swap(1, 2);
    assert!(AdaptiveWindows::execute(&allocator, &reordered, page).is_err());
    assert_eq!(ControlSnapshot::capture(), before);
    assert!(AdaptiveWindows::execute(&allocator, &phased[..4], page).is_err());
    assert_eq!(ControlSnapshot::capture(), before);
    let mut mismatched = phased.clone();
    mismatched[2].bytes = 8 * MIB;
    assert!(AdaptiveWindows::execute(&allocator, &mismatched, page).is_err());
    assert_eq!(ControlSnapshot::capture(), before);
    let mut same_group_mixed = smoke("mixed");
    for cell in &mut same_group_mixed {
        cell.bytes = MIB;
        cell.bucket = Bucket::OneTo8;
        if let Some((shape, _, blocks)) = cell.co_resident {
            cell.co_resident = Some((shape, MIB, blocks));
        }
    }
    assert!(AdaptiveWindows::execute(&allocator, &same_group_mixed, page).is_err());
    assert_eq!(ControlSnapshot::capture(), before);
    let lease = OsProbe::begin().expect("own-process epoch lease");
    let overlap = AdaptiveWindows::execute(&allocator, &phased, page).unwrap_err();
    assert_eq!(overlap.kind(), std::io::ErrorKind::WouldBlock);
    assert_eq!(ControlSnapshot::capture(), before);
    drop(lease);

    for scenario in ["phased", "mixed"] {
        let cells = smoke(scenario);
        let windows =
            AdaptiveWindows::execute(&allocator, &cells, page).expect("real measured windows");
        assert_eq!(windows.len(), cells.len());
        let mut previous = None;
        let mut training_count = 0;
        for (window, cell) in windows.iter().zip(&cells) {
            assert_eq!(window.cell, *cell);
            assert!(
                !window.registered_geometry,
                "CI geometry must never claim campaign identity"
            );
            assert!(window.os.rss_start_bytes > 0 && window.os.rss_end_bytes > 0);
            assert!(window.os.rss_peak_bytes > 0);
            std::hint::black_box((
                window.os.anon_huge_pages_bytes,
                window.os.minor_faults,
                window.os.major_faults,
            ));
            assert_eq!(
                window.os.peak_scope,
                "OS-reported approximate VmHWM since clear_refs=5, including probes"
            );
            assert_eq!(
                window.os.fault_scope,
                "own-process stat 10/12 delta before final RSS read; excludes children"
            );
            let (start, end) = match &window.execution {
                WindowExecution::Training(run) => {
                    assert!(
                        window.live.is_none(),
                        "no invented live training observation"
                    );
                    training_count += 1;
                    assert!(matches!(
                        run.state,
                        TrainingState::Complete {
                            valid: TRAINING_TARGET,
                            ..
                        }
                    ));
                    assert_eq!(run.requested_bytes, cell.bytes);
                    assert_eq!(run.shape, cell.shape);
                    assert_eq!(run.cache_budgets, [0, 0]);
                    assert_eq!(run.resolved_tuple_words, AllocCore::dbg_thp_tuple().pack());
                    assert_eq!(run.observations.len(), TRAINING_TARGET);
                    assert!(run.observations.iter().all(|observation| observation
                        .interval
                        .class_valid(cell.phase == Phase::TrainSparse)));
                    assert!(window.full_window_ns >= run.wall_ns);
                    (run.before, run.after)
                }
                WindowExecution::Batch {
                    setup_stats,
                    payload_stats,
                    executor_wall_ns,
                    resolved_tuple_words,
                    cache_budgets,
                    actor_threads,
                    before,
                    after,
                    live_rss_bytes,
                    live_anon_huge_pages_bytes,
                } => {
                    let extra = cell.co_resident.map_or(0, |(_, _, blocks)| blocks);
                    let blocks = u64::try_from(cell.blocks + extra).unwrap();
                    let bytes = cell.bytes * cell.blocks
                        + cell
                            .co_resident
                            .map_or(0, |(_, bytes, blocks)| bytes * blocks);
                    let written = match cell.phase {
                        Phase::TrainedSparse => {
                            cell.bytes / if scenario == "mixed" { 50 } else { 20 }
                        }
                        Phase::CoResident => cell.bytes + 8 * MIB / 50,
                        Phase::Measure | Phase::TrainedDense => cell.bytes,
                        Phase::TrainSparse | Phase::TrainDense => {
                            panic!("training must retain its own evidence")
                        }
                    };
                    assert_eq!(setup_stats.allocations, 2);
                    assert_eq!(setup_stats.frees, 2);
                    assert_eq!(setup_stats.requested_bytes_issued, 64);
                    assert_eq!(payload_stats.allocations, blocks);
                    assert_eq!(payload_stats.frees, blocks);
                    assert_eq!(payload_stats.reallocations, 0);
                    assert_eq!(
                        payload_stats.requested_bytes_issued,
                        u64::try_from(bytes).unwrap()
                    );
                    assert_eq!(payload_stats.bytes_written, u64::try_from(written).unwrap());
                    assert_eq!(after.ordinal - before.ordinal, blocks);
                    let live = window.live.expect("batch live diagnostic boundary");
                    assert_eq!(live.ordinal, after.ordinal);
                    assert_eq!(live.learning, before.learning);
                    for group in 0..6 {
                        assert_eq!(live.controls[group][1..], before.controls[group][1..]);
                    }
                    assert_eq!(*cache_budgets, [0, 0]);
                    assert_eq!(*resolved_tuple_words, AllocCore::dbg_thp_tuple().pack());
                    assert_ne!(actor_threads[0], actor_threads[1]);
                    assert!(*live_rss_bytes > 0);
                    // A real zero is valid; no promise that Linux granted huge pages.
                    std::hint::black_box(live_anon_huge_pages_bytes);
                    assert!(window.full_window_ns >= *executor_wall_ns);
                    (*before, *after)
                }
            };
            if let Some(prior) = previous {
                assert_eq!(start, prior, "no hidden eligible inter-window traffic");
            }
            previous = Some(end);
        }
        assert_eq!(training_count, 2);
        assert_eq!(ControlSnapshot::capture(), previous.unwrap());
    }
    // A failed setup must join/clean actors and release its OS lease before return.
    let mut wrong_minimum = phased.clone();
    for cell in &mut wrong_minimum {
        cell.minimum_request = 0;
    }
    let before = ControlSnapshot::capture();
    let failed = AdaptiveWindows::execute(&allocator, &wrong_minimum, page).unwrap_err();
    assert_eq!(failed.to_string(), "payload actor session failed");
    assert_eq!(ControlSnapshot::capture(), before);
    drop(OsProbe::begin().expect("failed window released epoch lease"));
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
    assert_eq!(allocator.stats().config_conflicts, 0);
}
