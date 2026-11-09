//! Real smaller-request VM/payload evidence, never registered campaign volumes.
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
#[path = "../examples/_shared/j13b_owned_block.rs"]
#[allow(dead_code)]
mod owned;
#[path = "../examples/_shared/j13b_payload_actors.rs"]
#[allow(dead_code)]
mod payload_actors;
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
fn native_advice_capture_rejects_unsupported_hosts_before_payload_claims() {
    let before = sefer_alloc::AllocCore::dbg_thp_advised_bytes_total();
    assert_eq!(
        aligned_vmem::AdviceCapture::begin_native().unwrap_err(),
        "advice lifetime capture requires native Linux/Android"
    );
    assert_eq!(
        sefer_alloc::AllocCore::dbg_thp_advised_bytes_total(),
        before
    );
}

#[cfg(all(not(miri), any(target_os = "linux", target_os = "android")))]
#[test]
fn actual_native_capture_closes_modes_cache_realloc_remote_and_release_failures() {
    const NAME: &str =
        "actual_native_capture_closes_modes_cache_realloc_remote_and_release_failures";
    const ENV: &str = "J13B_ADVICE_CAPTURE_CASE";
    if !aligned_vmem::AdviceCapture::native_available() {
        assert_eq!(
            aligned_vmem::AdviceCapture::begin_native().unwrap_err(),
            "advice lifetime capture requires native Linux/Android"
        );
        return;
    }
    let case = match std::env::var(ENV) {
        Ok(case) => case,
        Err(std::env::VarError::NotPresent) => {
            for case in [
                "concurrent",
                "always",
                "budget",
                "auto",
                "off",
                "advice-pre",
                "release-pre",
                "release-backend",
                "off-legacy",
            ] {
                let status = std::process::Command::new(
                    std::env::current_exe().expect("own test executable"),
                )
                .args(["--exact", NAME, "--nocapture"])
                .env(ENV, case)
                .status()
                .expect("isolated configuration child");
                assert!(status.success(), "native child failed: {case}");
                match case {
                    "concurrent" => println!("[j13b-advice-child] concurrent PASS"),
                    "always" => println!("[j13b-advice-child] always PASS"),
                    "budget" => println!("[j13b-advice-child] budget PASS"),
                    "auto" => println!("[j13b-advice-child] auto PASS"),
                    "off" => println!("[j13b-advice-child] off PASS"),
                    "advice-pre" => println!("[j13b-advice-child] advice-pre PASS"),
                    "release-pre" => println!("[j13b-advice-child] release-pre PASS"),
                    "release-backend" => println!("[j13b-advice-child] release-backend PASS"),
                    "off-legacy" => println!("[j13b-advice-child] off-legacy PASS"),
                    _ => unreachable!("fixed child case list"),
                }
            }
            return;
        }
        Err(std::env::VarError::NotUnicode(_)) => panic!("native fixture case is not UTF-8"),
    };
    run_native_case(&case);
}

#[cfg(all(not(miri), any(target_os = "linux", target_os = "android")))]
fn run_native_case(case: &str) {
    use aligned_vmem::{
        AdviceCapture as Capture, AdviceCaptureSnapshot, AdviceEvent as Event,
        AdviceLifetimeSnapshot as Totals, AdviceReplay, AdviceReplaySnapshot,
    };
    use owned::OwnedBlock;
    use payload_actors::PayloadActors;
    use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, SeferAlloc};
    use workload::{Shape, TouchPlan};
    const MIB: usize = 1024 * 1024;

    fn raw() -> AdviceCaptureSnapshot {
        Capture::snapshot().expect("closed native capture")
    }
    fn inspect() -> AdviceReplaySnapshot {
        let capture = raw();
        assert_eq!(capture.encoding_version, Event::ENCODING_VERSION);
        assert_eq!(capture.slot_storage_bytes, 4 * MIB);
        assert!(capture.total_storage_bytes > capture.slot_storage_bytes);
        assert_eq!(
            capture.tls_context_bytes_per_thread,
            2 * core::mem::size_of::<usize>()
        );
        assert_eq!(
            u64::try_from(capture.records.len()).unwrap(),
            capture.boundary
        );
        let replay = AdviceReplay::from_capture(&capture).expect("native ordered lifecycle");
        let snapshot = replay
            .snapshot()
            .expect("readable native ownership evidence");
        assert_eq!(
            snapshot.advice.process[Totals::TOTAL],
            AllocCore::dbg_thp_advised_bytes_total()
        );
        snapshot
    }
    fn touch(block: &mut OwnedBlock<'_, SeferAlloc>, page: usize) {
        for byte in block.slots().iter_mut().step_by(page) {
            byte.write(0x5a);
        }
    }
    struct ResetFaults;
    impl Drop for ResetFaults {
        fn drop(&mut self) {
            aligned_vmem::fault_injection::arm_fail_next_release(0);
            aligned_vmem::fault_injection::arm_fail_next_release_backend_result(0);
            aligned_vmem::fault_injection::arm_fail_next_advise_range(0);
        }
    }
    let _reset = ResetFaults;
    let (mode, budget) = match case {
        "always" | "static" | "concurrent" => (LargeThpMode::Always, 0),
        "auto" => (LargeThpMode::Auto, 0),
        "off" | "off-legacy" => (LargeThpMode::Off, 0),
        "budget" | "advice-pre" | "release-pre" | "release-backend" => {
            (LargeThpMode::Always, 8 * MIB)
        }
        _ => panic!("unknown native capture fixture case"),
    };
    assert_eq!(
        AllocCore::dbg_thp_advised_bytes_total(),
        0,
        "arm capture before any payload advice"
    );
    Capture::begin_native().expect("one native process-wide capture");
    let cache_budget = if matches!(case, "static" | "concurrent") {
        0
    } else {
        8 * MIB
    };
    let allocator = SeferAlloc::with_config(
        LargeCacheConfig::new()
            .budget_bytes(cache_budget)
            .huge_page_mode(mode)
            .huge_page_block_min_bytes(MIB)
            .huge_page_budget_bytes(budget),
    );
    let page = aligned_vmem::page_size();

    if case == "concurrent" {
        let stats = window_kernel::WindowKernel::churn(
            &allocator,
            TouchPlan {
                bytes: MIB,
                shape: Shape::Quarter,
            },
            4,
            16,
            page,
            |_, actual| {
                if actual.dbg_current_large_cache_budget() != Some(0)
                    || AllocCore::dbg_thp_tuple().mode != LargeThpMode::Always
                    || AllocCore::dbg_thp_tuple().min_bytes != MIB
                    || AllocCore::dbg_thp_tuple().budget_bytes != 0
                    || AllocCore::dbg_thp_tuple_conflicts() != 0
                    || actual.stats().config_conflicts != 0
                {
                    return Err(std::io::Error::other(
                        "concurrent capture worker config mismatch",
                    ));
                }
                Ok(())
            },
        )
        .expect("actual four-worker native capture");
        assert_eq!(stats.allocations, 64);
        assert_eq!(stats.frees, 64);
        assert_eq!(stats.bytes_written, 64 * u64::try_from(MIB / 4).unwrap());
        let end = inspect();
        assert_eq!(end.requests[1], [64, 64, 64]);
        assert_eq!(end.outstanding_fresh, 0);
        assert!(end.advice.process[Totals::TOTAL] > 0);
        assert_eq!(end.advice.process[Totals::NOW], 0);
        assert_eq!(
            end.advice.process[Totals::TOTAL],
            end.advice.process[Totals::RELEASED]
        );
        assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
        assert_eq!(allocator.stats().config_conflicts, 0);
        eprintln!(
            "[j13b-advice-capture] case=concurrent joined-lifetimes PASS canonical={}",
            end.canonical_releases
        );
        return;
    }

    if case == "static" {
        let learning = AllocCore::dbg_thp_learning_telemetry();
        let controls = AllocCore::dbg_thp_control_telemetry();
        let ordinal = AllocCore::dbg_thp_control_ordinal();
        assert!(ordinal.is_multiple_of(8));
        let mut blocks = Vec::with_capacity(16);
        for _ in 0..16 {
            let mut block =
                OwnedBlock::allocate(&allocator, MIB).expect("static-reference payload");
            block.slots()[0].write(0x5a);
            blocks.push(block);
        }
        assert_eq!(allocator.dbg_current_large_cache_budget(), Some(0));
        assert_eq!(AllocCore::dbg_thp_tuple().mode, LargeThpMode::Always);
        assert_eq!(AllocCore::dbg_thp_tuple().min_bytes, MIB);
        assert_eq!(AllocCore::dbg_thp_tuple().budget_bytes, 0);
        assert_eq!(AllocCore::dbg_thp_control_ordinal() - ordinal, 16);
        let live = inspect();
        assert_eq!(live.requests[1], [16, 16, 16]);
        let capture = raw();
        let advice: Vec<_> = capture
            .records
            .iter()
            .filter(|event| event.kind() == Event::ACCEPTED)
            .collect();
        assert_eq!(advice.len(), 14);
        for event in advice {
            assert_eq!(event.mode(), Some(1));
            assert_eq!(
                event.words()[Event::VALUE],
                event.words()[Event::USABLE_LEN],
                "fixed-one non-control advice"
            );
        }
        assert_eq!(
            live.advice.process[Totals::NOW],
            live.advice.process[Totals::TOTAL]
        );
        drop(blocks);
        let end = inspect();
        assert_eq!(end.outstanding_fresh, 0);
        assert_eq!(end.advice.process[Totals::NOW], 0);
        assert_eq!(
            end.advice.process[Totals::TOTAL],
            end.advice.process[Totals::RELEASED]
        );
        assert_eq!(
            AllocCore::dbg_thp_control_telemetry()[1][2] - controls[1][2],
            2
        );
        assert_eq!(
            AllocCore::dbg_thp_learning_telemetry(),
            learning,
            "static sampling never admits learner updates"
        );
        assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
        assert_eq!(allocator.stats().config_conflicts, 0);
        eprintln!("[j13b-advice-capture] case=static matched-controls-and-lifetime PASS");
        return;
    }

    if case == "advice-pre" {
        aligned_vmem::fault_injection::arm_fail_next_advise_range(1);
    }
    let mut first = OwnedBlock::allocate(&allocator, MIB).expect("actual fresh Large payload");
    touch(&mut first, page);
    assert_eq!(allocator.dbg_current_large_cache_budget(), Some(8 * MIB));
    assert_eq!(AllocCore::dbg_thp_tuple().mode, mode);
    assert_eq!(AllocCore::dbg_thp_tuple().min_bytes, MIB);
    assert_eq!(AllocCore::dbg_thp_tuple().budget_bytes, budget);
    let live = inspect();
    assert_eq!(
        live.requests[1],
        [1, 1, u64::from(mode != LargeThpMode::Off)]
    );
    assert_eq!(live.outstanding_fresh, 1);

    if case == "advice-pre" {
        assert_eq!(live.advice.process, [0; 4]);
        assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
        assert!(AllocCore::dbg_thp_charged_total() > 0);
        assert_eq!(
            AllocCore::dbg_thp_charged_total(),
            AllocCore::dbg_thp_returned_total()
        );
        drop(first);
        allocator.dbg_trim_current_thread();
        assert_eq!(inspect().outstanding_fresh, 0);
        eprintln!("[j13b-advice-capture] case={case} pre-syscall-advice-zero PASS");
        return;
    }

    if case == "release-pre" || case == "release-backend" {
        assert!(live.advice.process[Totals::NOW] > 0);
        drop(first);
        allocator.dbg_trim_current_thread();
        assert_eq!(inspect().advice.process[Totals::NOW], 0);
        let (charged, before) = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let mut owned = OwnedBlock::allocate(&allocator, MIB)
                        .expect("worker mapping before release FI");
                    touch(&mut owned, page);
                    assert_eq!(allocator.dbg_current_large_cache_budget(), Some(8 * MIB));
                    let active = inspect();
                    let charged = AllocCore::dbg_thp_ledger_used();
                    assert!(charged > 0);
                    assert_eq!(
                        u64::try_from(charged).unwrap(),
                        active.advice.process[Totals::NOW]
                    );
                    drop(owned);
                    assert_eq!(
                        inspect().advice.process[Totals::NOW],
                        active.advice.process[Totals::NOW]
                    );
                    let before = raw().boundary;
                    if case == "release-pre" {
                        aligned_vmem::fault_injection::arm_fail_next_release(1);
                    } else {
                        aligned_vmem::fault_injection::arm_fail_next_release_backend_result(1);
                    }
                    allocator.dbg_trim_current_thread();
                    (charged, before)
                })
                .join()
                .expect("failed-release owner exits before final capture")
        });
        assert_eq!(
            AllocCore::dbg_thp_ledger_used(),
            charged,
            "unproven release retains charge"
        );
        let capture = raw();
        let refused: Vec<_> = capture
            .records
            .iter()
            .enumerate()
            .filter(|(index, event)| {
                u64::try_from(*index).unwrap() >= before
                    && event.kind() == Event::RELEASED
                    && !event.confirmed_release()
            })
            .collect();
        assert_eq!(refused.len(), 1);
        let key = refused[0].1.words()[Event::RESERVATION];
        drop(refused);
        assert!(
            AdviceReplay::from_capture(&capture).is_err(),
            "unknown outcomes cannot be canonicalized into success"
        );
        assert_eq!(
            capture
                .records
                .iter()
                .enumerate()
                .filter(|(index, event)| u64::try_from(*index).unwrap() >= before
                    && event.kind() == Event::RELEASED
                    && event.words()[Event::RESERVATION] == key)
                .count(),
            1,
            "no retry after owner-thread teardown"
        );
        let mut replay = AdviceReplay::new();
        let mut failed = false;
        for (index, event) in capture.records.into_iter().enumerate() {
            if replay.push(u64::try_from(index).unwrap(), event).is_err() {
                failed = true;
                break;
            }
        }
        assert!(failed);
        assert!(
            replay.snapshot().is_err(),
            "unproven native outcome cannot become zero evidence"
        );
        assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
        assert_eq!(allocator.stats().config_conflicts, 0);
        eprintln!("[j13b-advice-capture] case={case} unproven-no-retry PASS");
        return;
    }

    if mode == LargeThpMode::Off {
        assert_eq!(live.advice.process, [0; 4]);
    } else if mode == LargeThpMode::Always {
        assert!(live.advice.process[Totals::NOW] > 0);
        if budget == 0 {
            assert_eq!(
                AllocCore::dbg_thp_ledger_used(),
                0,
                "unlimited accepted advice has zero charge"
            );
        }
    }
    let initial_records = raw();
    let original = initial_records
        .records
        .iter()
        .find(|event| {
            event.kind() == Event::FRESH
                && event.words()[Event::REQUESTED] == u64::try_from(MIB).unwrap()
        })
        .expect("actual original reservation")
        .words();
    drop(first);
    let cached = inspect();
    assert_eq!(cached.advice, live.advice, "deposit does not retire advice");
    assert_eq!(cached.outstanding_fresh, 1);
    let hits = allocator.stats().large_cache_hits;
    let mut reused = OwnedBlock::allocate(&allocator, MIB).expect("actual cache reuse");
    touch(&mut reused, page);
    assert!(
        allocator.stats().large_cache_hits > hits,
        "actual cache-hit activation"
    );
    assert_eq!(
        inspect().requests,
        cached.requests,
        "cache hit is not a fresh event"
    );
    assert_eq!(
        inspect().advice,
        cached.advice,
        "cache hit neither re-advises nor re-charges"
    );
    let target = usize::try_from(original[Event::RESERVATION_LEN])
        .unwrap()
        .checked_mul(2)
        .unwrap();
    assert!(target <= 128 * MIB);
    reused
        .reallocate(target)
        .expect("grow beyond original reservation must move");
    touch(&mut reused, page);
    let grown = inspect();
    assert!(
        grown.requests.iter().map(|row| row[0]).sum::<u64>()
            > cached.requests.iter().map(|row| row[0]).sum::<u64>()
    );
    drop(reused);
    allocator.dbg_trim_current_thread();
    assert_eq!(inspect().outstanding_fresh, 0);
    assert_eq!(inspect().advice.process[Totals::NOW], 0);

    PayloadActors::with(
        &allocator,
        2,
        page,
        |_, actual| {
            if actual.dbg_current_large_cache_budget() != Some(8 * MIB) {
                return Err(std::io::Error::other("remote actor cache mismatch"));
            }
            actual.dbg_trim_current_thread();
            Ok(())
        },
        |actors| {
            for actor in 0..2 {
                actors.allocate(
                    actor,
                    &[TouchPlan {
                        bytes: 32,
                        shape: Shape::Dense,
                    }],
                )?;
                actors.free(actor)?;
            }
            actors.allocate(
                0,
                &[TouchPlan {
                    bytes: MIB,
                    shape: Shape::Dense,
                }],
            )?;
            actors.transfer(0, 1)?;
            actors.touch(1)?;
            actors.free(1)?;
            actors.allocate(
                0,
                &[TouchPlan {
                    bytes: 32,
                    shape: Shape::Dense,
                }],
            )?;
            actors.free(0)?;
            Ok(())
        },
    )
    .expect("real remote retirement then owner drain and joined teardown");
    allocator.dbg_trim_current_thread();
    let end = inspect();
    assert_eq!(end.outstanding_fresh, 0);
    assert_eq!(end.advice.process[Totals::NOW], 0);
    assert_eq!(
        end.advice.process[Totals::TOTAL],
        end.advice.process[Totals::RELEASED]
    );
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    if mode == LargeThpMode::Off {
        assert_eq!(end.advice.process, [0; 4]);
    }

    if case == "off-legacy" {
        let block = OwnedBlock::allocate(&allocator, MIB).expect("legacy uncharged mapping");
        drop(block);
        aligned_vmem::fault_injection::arm_fail_next_release(1);
        allocator.dbg_trim_current_thread();
        assert_eq!(
            inspect().outstanding_fresh,
            0,
            "legacy path must retain its original native call"
        );
        let probe =
            aligned_vmem::try_reserve_aligned(page, page).expect("separate checked FI witness");
        assert!(
            probe.try_release().is_err(),
            "legacy release must not consume checked-release FI"
        );
        assert_eq!(inspect().advice.process, [0; 4]);
    }
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
    assert_eq!(allocator.stats().config_conflicts, 0);
    eprintln!("[j13b-advice-capture] case={case} lifecycle PASS events={} accepted={} peak={} released={}", end.events,
        end.advice.process[Totals::TOTAL], end.advice.process[Totals::PEAK], end.advice.process[Totals::RELEASED]);
}
