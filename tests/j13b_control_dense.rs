//! J13b control telemetry: one dense Auto-control cohort, isolated per process.
#![cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals",
    not(feature = "numa-aware")
))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode};

const MIB: usize = 1024 * 1024;
const BLOCK: usize = 8 * MIB;
const COHORT: usize = 8;
const GROUP_8_TO_32_MIB: usize = 2;

#[test]
fn dense_control_reports_high_residency() {
    let config = LargeCacheConfig::new().huge_page_mode(LargeThpMode::Auto);
    let mut core = AllocCore::new_with_config(config).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(Some(0));

    let layout = Layout::from_size_align(BLOCK, 8).expect("valid block layout");
    let mut blocks = [core::ptr::null_mut(); COHORT];
    for block in &mut blocks {
        *block = core.alloc(layout);
        assert!(!block.is_null(), "8 MiB allocation must succeed");
    }
    let page = aligned_vmem::page_size();
    assert!(page > 0);
    for block in blocks {
        for offset in (0..BLOCK).step_by(page) {
            // SAFETY: every offset is within the live payload allocation.
            unsafe { block.add(offset).write_volatile(0x5a) };
        }
    }

    let before_free = AllocCore::dbg_thp_control_telemetry();
    assert_eq!(
        before_free[GROUP_8_TO_32_MIB],
        [1, 0, 0, 0, 0, 0, 0, 0],
        "one selection, no premise failures, no observations yet"
    );
    assert!(before_free
        .iter()
        .enumerate()
        .all(|(group, counters)| group == GROUP_8_TO_32_MIB || *counters == [0; 8]));

    for block in blocks.iter().rev() {
        // SAFETY: each pointer is live with the original layout.
        unsafe { core.dealloc(*block, layout) };
    }

    let control = AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB];
    assert_eq!(
        &control[..6],
        &[1, 0, 1, 0, 0, 0],
        "no errors and no invalidations are expected"
    );
    let resident = control[6] as u128;
    let sampled = control[7] as u128;
    assert!(
        sampled > 0,
        "valid sampling must report sampled payload pages"
    );
    // Residency is an environmental snapshot and can be lowered by reclaim;
    // this threshold remains the explicit dense-control oracle, not a claim
    // that the kernel guarantees exact residency.
    assert!(
        resident * 100 > sampled * 90,
        "dense control residency {resident}/{sampled} must exceed 90%"
    );
    drop(core);
}
