//! Actual small native operations/cache/failure windows; never campaign admission.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "alloc-stats",
    feature = "internals",
    feature = "bench-internals",
    feature = "lazy-commit-fault-injection",
    not(feature = "numa-aware")
))]
#[cfg(all(not(miri), any(target_os = "linux", target_os = "android")))]
#[path = "_shared/j13b_advice_assertions.rs"]
mod advice_assertions;
#[path = "../examples/_shared/j13b_advice_sample.rs"]
#[cfg_attr(
    any(miri, not(any(target_os = "linux", target_os = "android"))),
    allow(dead_code)
)]
mod advice_sample;
#[path = "../examples/_shared/j13b_advice_windows.rs"]
#[cfg_attr(
    any(miri, not(any(target_os = "linux", target_os = "android"))),
    allow(dead_code)
)]
mod advice_windows;
#[path = "../examples/_shared/j13b_owned_block.rs"]
#[allow(dead_code)]
mod owned;
#[path = "../examples/_shared/j13b_run_stats.rs"]
#[allow(dead_code)]
mod run_stats;
#[path = "../examples/_shared/j13b_window_kernel.rs"]
#[allow(dead_code)]
mod window_kernel;
#[path = "../examples/_shared/j13b_workload.rs"]
#[allow(dead_code)]
mod workload;

#[cfg(any(miri, not(any(target_os = "linux", target_os = "android"))))]
#[test]
fn advice_windows_reject_models_before_clocks_capture_or_payload() {
    let before = sefer_alloc::AllocCore::dbg_thp_advised_bytes_total();
    let mut windows = advice_windows::AdviceWindows::new();
    assert_eq!(
        windows.begin().unwrap_err().to_string(),
        "advice windows require native Linux/Android capture"
    );
    let allocator = sefer_alloc::SeferAlloc::with_config(sefer_alloc::LargeCacheConfig::new());
    let plan = workload::TouchPlan {
        bytes: 1024 * 1024,
        shape: workload::Shape::Dense,
    };
    assert!(windows.batch(&allocator, &[plan], 4096).is_err());
    assert!(windows
        .grow(&allocator, 1024 * 1024, 8 * 1024 * 1024, 4096)
        .is_err());
    assert!(windows
        .churn(&allocator, plan, 1, 1, 4096, |_, _| panic!(
            "model guard reached a native worker"
        ))
        .is_err());
    assert_eq!(
        sefer_alloc::AllocCore::dbg_thp_advised_bytes_total(),
        before
    );
}

#[cfg(all(not(miri), any(target_os = "linux", target_os = "android")))]
#[test]
fn actual_windows_reconcile_operations_cached_ownership_and_unknown_release() {
    const NAME: &str = "actual_windows_reconcile_operations_cached_ownership_and_unknown_release";
    const ENV: &str = "J13B_ADVICE_WINDOWS_CASE";
    if !aligned_vmem::AdviceCapture::native_available() {
        assert!(advice_windows::AdviceWindows::new().begin().is_err());
        return;
    }
    let case = match std::env::var(ENV) {
        Ok(case) => case,
        Err(std::env::VarError::NotPresent) => {
            for case in ["operations", "cache", "unknown", "off"] {
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", NAME, "--nocapture"])
                    .env(ENV, case)
                    .status()
                    .unwrap();
                assert!(
                    status.success(),
                    "native advice window child failed: {case}"
                );
                match case {
                    "operations" => println!("[j13b-advice-windows] operations PASS"),
                    "cache" => println!("[j13b-advice-windows] cache PASS"),
                    "unknown" => println!("[j13b-advice-windows] unknown PASS"),
                    "off" => println!("[j13b-advice-windows] off PASS"),
                    _ => unreachable!("fixed child case list"),
                }
            }
            return;
        }
        Err(std::env::VarError::NotUnicode(_)) => panic!("window case is not UTF-8"),
    };
    run_case(&case);
}

#[cfg(all(not(miri), any(target_os = "linux", target_os = "android")))]
fn run_case(case: &str) {
    use advice_sample::AdviceSample;
    use advice_windows::AdviceWindows;
    use aligned_vmem::AdviceLifetimeSnapshot as Totals;
    use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, SeferAlloc};
    use workload::{Shape, TouchPlan};
    const MIB: usize = 1024 * 1024;
    fn config(
        actual: &SeferAlloc,
        cache: usize,
        budget: usize,
        mode: LargeThpMode,
    ) -> std::io::Result<()> {
        if actual.dbg_current_large_cache_budget() != Some(cache)
            || AllocCore::dbg_thp_tuple().mode != mode
            || AllocCore::dbg_thp_tuple().min_bytes != MIB
            || AllocCore::dbg_thp_tuple().budget_bytes != budget
            || AllocCore::dbg_thp_tuple_conflicts() != 0
            || actual.stats().config_conflicts != 0
        {
            return Err(std::io::Error::other(
                "native window configuration mismatch",
            ));
        }
        Ok(())
    }
    fn observed(sample: &AdviceSample) {
        advice_assertions::assert_sample(sample);
        assert_eq!(
            sample.window.after.advice.process[Totals::TOTAL],
            AllocCore::dbg_thp_advised_bytes_total()
        );
    }

    let mode = if case == "off" {
        LargeThpMode::Off
    } else {
        LargeThpMode::Always
    };
    let cache = if matches!(case, "operations" | "off") {
        0
    } else {
        8 * MIB
    };
    let budget = if case == "unknown" { 8 * MIB } else { 0 };
    let allocator = SeferAlloc::with_config(
        LargeCacheConfig::new()
            .budget_bytes(cache)
            .huge_page_mode(mode)
            .huge_page_block_min_bytes(MIB)
            .huge_page_budget_bytes(budget),
    );
    let mut windows = AdviceWindows::new();
    let page = aligned_vmem::page_size();
    let plan = TouchPlan {
        bytes: MIB,
        shape: Shape::Dense,
    };
    if case == "off" {
        let (_, sample) = windows.batch(&allocator, &[plan], page).unwrap();
        observed(&sample);
        config(&allocator, cache, budget, mode).unwrap();
        assert!(sample.window.end > sample.window.begin);
        assert_eq!(
            sample.window.after.requests[1][0] - sample.window.before.requests[1][0],
            1
        );
        assert_eq!(sample.window.after.advice.process, [0; 4]);
        assert_eq!(sample.window.peak_process, 0);
        assert_eq!(sample.window.after.outstanding_fresh, 0);
        return;
    }
    if case == "operations" {
        let wall = std::time::Instant::now();
        let (stats, first) = windows
            .batch(
                &allocator,
                &[
                    plan,
                    TouchPlan {
                        bytes: 8 * MIB,
                        shape: Shape::Quarter,
                    },
                ],
                page,
            )
            .unwrap();
        observed(&first);
        config(&allocator, cache, budget, mode).unwrap();
        assert_eq!((stats.allocations, stats.frees), (2, 2));
        for group in [1, 2] {
            assert_eq!(
                first.window.after.requests[group][0]
                    .checked_sub(first.window.before.requests[group][0]),
                Some(1)
            );
        }
        assert!(
            first.observed_interval_ns
                >= stats.wall_ns + first.observer_wall_ns.iter().sum::<u64>()
        );
        assert_eq!(first.window.after.outstanding_fresh, 0);
        assert_eq!(first.window.after.advice.process[Totals::NOW], 0);
        assert!(first.window.peak_process > 0);
        assert!(
            u64::try_from(wall.elapsed().as_nanos()).unwrap()
                >= stats.wall_ns + first.observer_wall_ns.iter().sum::<u64>()
        );
        let (grown, second) = windows.grow(&allocator, MIB, 8 * MIB, page).unwrap();
        observed(&second);
        assert_eq!(second.window.before, first.window.after);
        assert_eq!(grown.reallocations, 3);
        assert_eq!(grown.bytes_written, u64::try_from(8 * MIB).unwrap());
        assert!(second.window.peak_process > 0);
        assert!(
            second.window.after.advice.process[Totals::TOTAL]
                > second.window.before.advice.process[Totals::TOTAL]
        );
        assert!(
            second.observed_interval_ns
                >= grown.wall_ns + second.observer_wall_ns.iter().sum::<u64>()
        );
        assert_eq!(second.window.after.advice.process[Totals::NOW], 0);
        let (churn, third) = windows
            .churn(
                &allocator,
                TouchPlan {
                    bytes: MIB,
                    shape: Shape::Quarter,
                },
                4,
                4,
                page,
                |_, actual| config(actual, cache, budget, mode),
            )
            .unwrap();
        observed(&third);
        assert_eq!(third.window.before, second.window.after);
        assert_eq!((churn.allocations, churn.frees), (16, 16));
        assert_eq!(
            third.window.after.requests[1][0].checked_sub(third.window.before.requests[1][0]),
            Some(16)
        );
        assert!(third.window.peak_process > 0);
        assert!(
            third.window.after.advice.process[Totals::TOTAL]
                > third.window.before.advice.process[Totals::TOTAL]
        );
        assert!(
            third.observed_interval_ns
                >= churn.wall_ns + third.observer_wall_ns.iter().sum::<u64>()
        );
        assert_eq!(third.window.after.outstanding_fresh, 0);
        assert_eq!(third.window.after.advice.process[Totals::NOW], 0);
        let boundary = aligned_vmem::AdviceCapture::snapshot().unwrap().boundary;
        assert!(windows.batch(&allocator, &[], page).is_err());
        assert!(
            windows.finish().is_err(),
            "failed execution cannot become an empty supported row"
        );
        assert!(windows.begin().is_err());
        assert_eq!(
            aligned_vmem::AdviceCapture::snapshot().unwrap().boundary,
            boundary
        );
        return;
    }
    assert!(matches!(case, "cache" | "unknown"));
    let (_, first) = windows.batch(&allocator, &[plan], page).unwrap();
    observed(&first);
    config(&allocator, cache, budget, mode).unwrap();
    let incoming = first.window.after.advice.process[Totals::NOW];
    assert!(incoming > 0);
    if case == "unknown" {
        let charged = AllocCore::dbg_thp_ledger_used();
        assert!(charged > 0);
        windows.begin().unwrap();
        aligned_vmem::fault_injection::arm_fail_next_release(1);
        allocator.dbg_trim_current_thread();
        assert!(windows.finish().is_err());
        assert_eq!(AllocCore::dbg_thp_ledger_used(), charged);
        let records = aligned_vmem::AdviceCapture::snapshot().unwrap().boundary;
        assert!(windows.begin().is_err());
        assert_eq!(
            aligned_vmem::AdviceCapture::snapshot().unwrap().boundary,
            records
        );
        aligned_vmem::fault_injection::arm_fail_next_release(0);
        return;
    }
    let hits = allocator.stats().large_cache_hits;
    let (_, reused) = windows.batch(&allocator, &[plan], page).unwrap();
    observed(&reused);
    assert!(allocator.stats().large_cache_hits > hits);
    assert_eq!(reused.window.before, first.window.after);
    assert_eq!(reused.window.before, reused.window.after);
    assert_eq!(reused.window.begin, reused.window.end);
    assert_eq!(reused.window.peak_process, incoming);
    windows.begin().unwrap();
    allocator.dbg_trim_current_thread();
    let released = windows.finish().unwrap();
    observed(&released);
    assert_eq!(released.window.before, reused.window.after);
    assert_eq!(released.window.peak_process, incoming);
    assert_eq!(released.window.after.advice.process[Totals::NOW], 0);
    assert_eq!(released.window.after.outstanding_fresh, 0);
}
