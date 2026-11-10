//! J13b control telemetry: cache reuse must not select or observe a second life.
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
fn cached_second_life_is_not_selected_or_observed() {
    let config = LargeCacheConfig::new().huge_page_mode(LargeThpMode::Auto);
    let mut core = AllocCore::new_with_config(config).expect("primordial bootstrap");
    // Leave the cache available, with enough room for all eight mappings.
    core.dbg_set_large_cache_budget(Some(usize::MAX));
    core.dbg_set_decay_config(1000, 3_600_000, 1 << 40);

    let layout = Layout::from_size_align(BLOCK, 8).expect("valid block layout");
    let mut first_life = [core::ptr::null_mut(); COHORT];
    for block in &mut first_life {
        *block = core.alloc(layout);
        assert!(!block.is_null(), "8 MiB allocation must succeed");
        // SAFETY: `block` is the live payload pointer returned for `layout`.
        unsafe { block.write_volatile(0x5a) };
    }
    let before_first_free = AllocCore::dbg_thp_control_telemetry();
    assert_eq!(
        before_first_free[GROUP_8_TO_32_MIB],
        [1, 0, 0, 0, 0, 0, 0, 0]
    );

    for block in first_life.iter().rev() {
        // SAFETY: each pointer is live with the original layout.
        unsafe { core.dealloc(*block, layout) };
    }
    let after_first_life = AllocCore::dbg_thp_control_telemetry();
    assert_eq!(
        &after_first_life[GROUP_8_TO_32_MIB][..6],
        &[1, 0, 1, 0, 0, 0]
    );

    let learned_after_first_life = AllocCore::dbg_thp_learning_telemetry();
    assert_eq!(&learned_after_first_life[GROUP_8_TO_32_MIB][3..], &[1, 0]);
    let mut second_life = [core::ptr::null_mut(); COHORT];
    for block in &mut second_life {
        *block = core.alloc(layout);
        assert!(!block.is_null(), "cached 8 MiB allocation must succeed");
    }
    for (index, block) in second_life.iter().enumerate() {
        assert!(
            first_life.contains(block),
            "second life must reuse a cached mapping"
        );
        assert!(
            !second_life[..index].contains(block),
            "each cached mapping is reused once"
        );
    }
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry(),
        after_first_life,
        "cache hits must not select fresh controls or sample second lives"
    );

    for block in second_life.iter().rev() {
        // SAFETY: each pointer is live with the original layout.
        unsafe { core.dealloc(*block, layout) };
    }
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry(),
        after_first_life,
        "cached second-life frees must not create residency observations"
    );
    assert_eq!(
        AllocCore::dbg_thp_learning_telemetry(),
        learned_after_first_life,
        "cache reuse must not train again"
    );
    drop(core);
    assert_eq!(
        AllocCore::dbg_thp_learning_telemetry(),
        learned_after_first_life,
        "cache terminal release must not train again"
    );
}
