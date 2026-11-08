//! J13b control telemetry: residency-sampler failure then recovery.
#![cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals",
    feature = "lazy-commit-fault-injection",
    not(feature = "numa-aware")
))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode};

const MIB: usize = 1024 * 1024;
const BLOCK: usize = 8 * MIB;
const COHORT: usize = 8;
const GROUP_8_TO_32_MIB: usize = 2;

fn allocate_cohort(core: &mut AllocCore, layout: Layout) -> [*mut u8; COHORT] {
    let mut blocks = [core::ptr::null_mut(); COHORT];
    for block in &mut blocks {
        *block = core.alloc(layout);
        assert!(!block.is_null(), "8 MiB allocation must succeed");
        // SAFETY: `block` is the live payload pointer returned for `layout`.
        unsafe { block.write_volatile(0x5a) };
    }
    blocks
}

fn free_cohort(core: &mut AllocCore, blocks: &[*mut u8; COHORT], layout: Layout) {
    for block in blocks.iter().rev() {
        // SAFETY: each pointer is live with the original layout.
        unsafe { core.dealloc(*block, layout) };
    }
}

#[test]
fn failed_residency_sample_is_counted_and_next_cohort_recovers() {
    let config = LargeCacheConfig::new().huge_page_mode(LargeThpMode::Auto);
    let mut core = AllocCore::new_with_config(config).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(Some(0));
    let layout = Layout::from_size_align(BLOCK, 8).expect("valid block layout");

    let learning_before = AllocCore::dbg_thp_learning_telemetry();
    let failed = allocate_cohort(&mut core, layout);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB],
        [1, 0, 0, 0, 0, 0, 0, 0]
    );
    aligned_vmem::fault_injection::arm_fail_next_residency_sample(1);
    free_cohort(&mut core, &failed, layout);
    aligned_vmem::fault_injection::arm_fail_next_residency_sample(0);
    let after_failure = AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB];
    assert_eq!(
        &after_failure[..6],
        &[1, 0, 0, 0, 0, 1],
        "failed sample is counted as an error"
    );
    assert!(
        after_failure[6] == 0 && after_failure[7] == 0,
        "failed sample contributes no page totals"
    );

    assert_eq!(
        AllocCore::dbg_thp_learning_telemetry(),
        learning_before,
        "a failed sample must not train or count as applied/dropped"
    );
    let recovered = allocate_cohort(&mut core, layout);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB][0],
        2
    );
    free_cohort(&mut core, &recovered, layout);
    let final_counts = AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB];
    assert_eq!(&final_counts[..6], &[2, 0, 1, 0, 0, 1]);
    assert!(
        final_counts[7] > 0,
        "recovered valid observation samples payload pages"
    );
    assert_eq!(
        &AllocCore::dbg_thp_learning_telemetry()[GROUP_8_TO_32_MIB][3..],
        &[1, 0]
    );
    drop(core);
}
