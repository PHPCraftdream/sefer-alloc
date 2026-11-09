//! Real training protocol with smaller requests, not registered campaign volumes.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    feature = "lazy-commit-fault-injection",
    not(feature = "numa-aware")
))]
#[path = "../examples/_shared/j13b_control_interval.rs"]
#[allow(dead_code)]
mod control_interval;
#[path = "../examples/_shared/j13b_control_snapshot.rs"]
mod control_snapshot;
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
#[path = "../examples/_shared/j13b_window_kernel.rs"]
#[allow(dead_code)]
mod window_kernel;
#[path = "../examples/_shared/j13b_workload.rs"]
#[allow(dead_code)]
mod workload;
use owner_training::OwnerTraining;
use sefer_alloc::{LargeCacheConfig, SeferAlloc};
use workload::Shape;

#[cfg(any(miri, not(any(target_os = "linux", target_os = "android"))))]
#[test]
fn unsupported_training_is_rejected_before_payload_claims() {
    let allocator = SeferAlloc::with_config(LargeCacheConfig::new());
    let before = control_snapshot::ControlSnapshot::capture();
    let error = OwnerTraining::run(&allocator, 1024 * 1024, Shape::Dense, 4096).unwrap_err();
    assert_eq!(
        error.to_string(),
        "owner training requires native Linux residency"
    );
    assert_eq!(control_snapshot::ControlSnapshot::capture(), before);
}

#[cfg(all(not(miri), any(target_os = "linux", target_os = "android")))]
#[test]
fn actual_training_reaches_both_targets_and_caps_sampler_failure_without_padding() {
    use sefer_alloc::{AllocCore, LargeThpMode};
    use training_run::TrainingRun;
    use window_kernel::WindowKernel;
    use workload::{TouchPlan, TrainingState, TRAINING_CAP, TRAINING_TARGET};
    const MIB: usize = 1024 * 1024;
    const GROUP: usize = 1;
    const ONE: u64 = 1 << 31;
    let page = aligned_vmem::page_size();
    let allocator = SeferAlloc::with_config(
        LargeCacheConfig::new()
            .budget_bytes(0)
            .huge_page_mode(LargeThpMode::Auto)
            .huge_page_block_min_bytes(MIB)
            .huge_page_budget_bytes(0),
    );
    let initial = WindowKernel::batch(
        &allocator,
        &[TouchPlan {
            bytes: MIB,
            shape: Shape::Dense,
        }],
        page,
    )
    .expect("counted real initial/cold request");
    assert_eq!(initial.allocations, 1);
    assert_eq!(initial.frees, 1);
    assert_eq!(
        AllocCore::dbg_thp_control_ordinal(),
        1,
        "nonaligned training start"
    );

    fn inspect(run: &TrainingRun, bytes: usize) -> usize {
        let eligible = match run.state {
            TrainingState::Complete {
                eligible,
                valid,
                attained_share,
            }
            | TrainingState::Insufficient {
                eligible,
                valid,
                attained_share,
            } => {
                assert_eq!(
                    attained_share,
                    Some(
                        f64::from(u32::try_from(run.after.learning[run.group][2]).unwrap())
                            / f64::from(1_u32 << 31)
                    )
                );
                assert!(valid <= TRAINING_TARGET && eligible <= TRAINING_CAP);
                eligible
            }
            _ => panic!("real training must terminate explicitly"),
        };
        assert_eq!(run.requested_bytes, bytes);
        assert_eq!(run.group, GROUP);
        assert!(matches!(run.shape, Shape::Dense | Shape::SparseFivePercent));
        assert_eq!(run.cache_budgets, [0, 0]);
        assert_eq!(run.resolved_tuple_words, AllocCore::dbg_thp_tuple().pack());
        assert_eq!(run.setup_stats.allocations, 2);
        assert_eq!(run.setup_stats.frees, 2);
        assert_eq!(run.setup_stats.requested_bytes_issued, 64);
        assert_eq!(
            run.payload_stats.allocations,
            u64::try_from(eligible).unwrap()
        );
        assert_eq!(run.payload_stats.frees, u64::try_from(eligible).unwrap());
        assert_eq!(run.payload_stats.reallocations, 0);
        assert_eq!(
            run.payload_stats.requested_bytes_issued,
            u64::try_from(bytes * eligible).unwrap()
        );
        assert_eq!(
            run.payload_stats.bytes_written,
            u64::try_from(
                eligible
                    * if run.shape == Shape::Dense {
                        bytes
                    } else {
                        bytes / 20
                    }
            )
            .unwrap()
        );
        assert_eq!(
            run.after.ordinal - run.before.ordinal,
            u64::try_from(eligible).unwrap()
        );
        let _wall = std::hint::black_box(run.wall_ns);
        let mut previous_position = 0;
        for row in &run.observations {
            assert!(row.eligible_position > previous_position && row.eligible_position <= eligible);
            assert_eq!(row.ticket & 3, 3);
            assert_eq!(
                (row.ticket >> 2).checked_sub(run.before.ordinal).unwrap() + 1,
                u64::try_from(row.eligible_position).unwrap()
            );
            assert!(AllocCore::dbg_thp_control_decision_at(row.ticket >> 2));
            assert_ne!(row.owner_thread, row.touch_thread);
            assert!(row.e_q31 <= ONE && row.m_q31 <= ONE);
            assert_eq!(row.interval.eligible_fresh, 1);
            previous_position = row.eligible_position;
        }
        eligible
    }

    let sparse = OwnerTraining::run(&allocator, MIB, Shape::SparseFivePercent, page)
        .expect("actual sparse training");
    assert!(matches!(
        sparse.state,
        TrainingState::Complete {
            valid: TRAINING_TARGET,
            ..
        }
    ));
    assert_eq!(
        sparse.observations.last().unwrap().eligible_position,
        inspect(&sparse, MIB),
        "stop at the actual 96th class observation, without cohort padding"
    );
    assert_eq!(sparse.observations.len(), TRAINING_TARGET);
    assert!(sparse.after.learning[GROUP][0] < sparse.before.learning[GROUP][0]);
    assert!(sparse
        .observations
        .iter()
        .all(|row| row.interval.class_valid(true)));
    let dense =
        OwnerTraining::run(&allocator, MIB, Shape::Dense, page).expect("actual dense training");
    assert_eq!(dense.before, sparse.after);
    assert!(matches!(
        dense.state,
        TrainingState::Complete {
            valid: TRAINING_TARGET,
            ..
        }
    ));
    assert_eq!(
        dense.observations.last().unwrap().eligible_position,
        inspect(&dense, MIB),
        "stop at the actual 96th class observation, without cohort padding"
    );
    assert_eq!(dense.observations.len(), TRAINING_TARGET);
    assert!(dense.after.learning[GROUP][0] > dense.before.learning[GROUP][0]);
    assert!(dense
        .observations
        .iter()
        .all(|row| row.interval.class_valid(false)));

    struct ResetSampler;
    impl Drop for ResetSampler {
        fn drop(&mut self) {
            aligned_vmem::fault_injection::arm_fail_next_residency_sample(0);
        }
    }
    let reset = ResetSampler;
    aligned_vmem::fault_injection::arm_fail_next_residency_sample(
        u32::try_from(TRAINING_CAP).unwrap(),
    );
    let failed = OwnerTraining::run(&allocator, MIB, Shape::SparseFivePercent, page)
        .expect("cap failure observations, never fabricate samples");
    drop(reset);
    assert_eq!(failed.before, dense.after);
    assert!(matches!(
        failed.state,
        TrainingState::Insufficient {
            eligible: TRAINING_CAP,
            valid: 0,
            ..
        }
    ));
    assert_eq!(inspect(&failed, MIB), TRAINING_CAP);
    assert!(!failed.observations.is_empty());
    assert!(failed
        .observations
        .iter()
        .all(|row| row.interval.reasons == [0, 0, 0, 0, 1]));
    assert_eq!(failed.after.learning, failed.before.learning);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
    assert_eq!(allocator.stats().config_conflicts, 0);
}
