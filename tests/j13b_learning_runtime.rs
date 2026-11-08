//! Real fresh controls drive shrink, zero-share recovery and group isolation.
#![cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use sefer_alloc::{
    AllocCore, LargeCacheConfig, LargeThpCurve, LargeThpEstimator, LargeThpGradient, LargeThpMode,
    LargeThpShare,
};
use std::alloc::Layout;

const MIB: usize = 1024 * 1024;

fn cohort(core: &mut AllocCore, bytes: usize, dense: bool, group: usize) -> LargeThpShare {
    let ordinal_before = AllocCore::dbg_thp_control_ordinal();
    assert_eq!(
        ordinal_before % 8,
        0,
        "scenario must start on a complete global cohort boundary"
    );
    let before = AllocCore::dbg_thp_control_telemetry()[group];
    let layout = Layout::from_size_align(bytes, 8).expect("valid cohort layout");
    let mut blocks = [core::ptr::null_mut(); 8];
    let page = aligned_vmem::page_size();
    assert!(
        page != 0 && page.is_power_of_two(),
        "valid runtime page size"
    );
    for block in &mut blocks {
        *block = core.alloc(layout);
        assert!(!block.is_null());
        if dense {
            for offset in (0..bytes).step_by(page) {
                // SAFETY: this is a live allocation; offset is strictly inside
                // its requested payload, never allocator metadata.
                unsafe { block.add(offset).write_volatile(0x5a) };
            }
        }
    }
    for block in blocks {
        // SAFETY: each retained allocation is freed once with its exact layout.
        unsafe { core.dealloc(block, layout) };
    }
    assert_eq!(
        AllocCore::dbg_thp_control_ordinal() - ordinal_before,
        8,
        "every cohort request must take the fresh eligible path, not cache reuse"
    );
    let after = AllocCore::dbg_thp_control_telemetry()[group];
    assert_eq!(after[0] - before[0], 1, "one fresh control per cohort");
    assert_eq!(after[2] - before[2], 1, "one valid first-life observation");
    assert_eq!(
        &after[3..6],
        &before[3..6],
        "no invalidation or sample error"
    );
    let resident = after[6] - before[6];
    let sampled = after[7] - before[7];
    assert!(sampled > 0 && resident <= sampled);
    if dense {
        assert!(
            u128::from(resident) * 100 >= u128::from(sampled) * 95,
            "dense mechanism was not activated"
        );
    } else {
        assert!(
            u128::from(resident) * 100 < u128::from(sampled),
            "untouched control must be sparse"
        );
    }
    LargeThpShare::from_raw(
        ((u128::from(resident) * u128::from(LargeThpShare::SCALE)) / u128::from(sampled)) as u32,
    )
}

fn check_expected(
    group: usize,
    expected: LargeThpEstimator,
    gradient: &LargeThpGradient,
    applied: u64,
) {
    assert_eq!(
        AllocCore::dbg_thp_learning_telemetry()[group],
        [
            u64::from(expected.e_raw()),
            u64::from(expected.m_raw()),
            u64::from(expected.bounded_share(gradient).raw()),
            applied,
            0,
        ],
        "runtime must use the resolved custom gradient exactly once per valid control"
    );
}

#[test]
fn clean_controls_drive_custom_gradient_through_zero_and_recovery_without_cross_group_state() {
    let gradient = LargeThpGradient::custom(
        1,
        0,
        4,
        LargeThpShare::SCALE / 8,
        LargeThpShare::SCALE / 8,
        3,
        2,
        LargeThpCurve::Strict,
    );
    let config = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Auto)
        .huge_page_block_min_bytes(0)
        .huge_page_gradient(gradient);
    let mut core = AllocCore::new_with_config(config).expect("primordial bootstrap");
    assert_eq!(
        AllocCore::dbg_thp_tuple().gradient,
        gradient,
        "resolved gradient must match the labelled custom configuration"
    );
    assert_eq!(AllocCore::dbg_thp_tuple().mode, LargeThpMode::Auto);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
    core.dbg_set_large_cache_budget(Some(0));
    let initial = AllocCore::dbg_thp_learning_telemetry();
    let mut expected = LargeThpEstimator::new();
    check_expected(2, expected, &gradient, 0);
    for applied in 1..=12 {
        let observation = cohort(&mut core, 8 * MIB, false, 2);
        expected.observe(&gradient, observation);
        check_expected(2, expected, &gradient, applied);
    }
    assert!(
        AllocCore::dbg_thp_advised_bytes_total() > 0,
        "Auto must activate advice before learning shrinks its share"
    );
    assert_eq!(expected.bounded_share(&gradient), LargeThpShare::ZERO);
    let before_zero = AllocCore::dbg_thp_advised_bytes_total();
    expected.observe(&gradient, cohort(&mut core, 8 * MIB, false, 2));
    check_expected(2, expected, &gradient, 13);
    assert_eq!(
        AllocCore::dbg_thp_advised_bytes_total(),
        before_zero,
        "zero share must suppress actual advice, not only report zero"
    );

    expected.observe(&gradient, cohort(&mut core, 8 * MIB, true, 2));
    check_expected(2, expected, &gradient, 14);
    assert_eq!(
        AllocCore::dbg_thp_advised_bytes_total(),
        before_zero,
        "the recovering cohort was allocated under zero share"
    );
    assert_eq!(
        expected.bounded_share(&gradient),
        LargeThpShare::ONE,
        "custom up_shift zero must recover immediately"
    );
    expected.observe(&gradient, cohort(&mut core, 8 * MIB, true, 2));
    check_expected(2, expected, &gradient, 15);
    assert!(
        AllocCore::dbg_thp_advised_bytes_total() > before_zero,
        "recovered share must reactivate real advice"
    );

    let group2 = AllocCore::dbg_thp_learning_telemetry()[2];
    let mut other = LargeThpEstimator::new();
    other.observe(&gradient, cohort(&mut core, 4 * MIB, false, 1));
    check_expected(1, other, &gradient, 1);
    let after = AllocCore::dbg_thp_learning_telemetry();
    assert_eq!(
        after[2], group2,
        "a different request group must not alter the learned group"
    );
    for group in [0, 3, 4, 5] {
        assert_eq!(
            after[group], initial[group],
            "unused group must retain its own prior"
        );
    }
}
