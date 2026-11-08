//! J13b control invalidation through the real AllocCore allocation lifecycle.
#![cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals",
    not(feature = "numa-aware"),
    not(feature = "exact-span-large")
))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode};

const MIB: usize = 1024 * 1024;
const BLOCK: usize = 8 * MIB;
const COHORT: usize = 8;
const GROUP_8_TO_32_MIB: usize = 2;

#[test]
fn realloc_and_core_teardown_do_not_observe_stale_live_controls() {
    let config = LargeCacheConfig::new().huge_page_mode(LargeThpMode::Auto);
    let mut core = AllocCore::new_with_config(config).expect("primordial bootstrap");
    // Release freed mappings rather than letting a cache hit disguise a fresh life.
    core.dbg_set_large_cache_budget(Some(0));

    let layout = Layout::from_size_align(BLOCK, 8).expect("valid 8 MiB layout");
    let page = aligned_vmem::page_size();
    assert!(page > 0 && page.is_power_of_two());
    let grown_size = BLOCK.checked_add(page).expect("grown size fits usize");
    let grown_layout = Layout::from_size_align(grown_size, 8).expect("valid grown layout");

    let mut realloc_cohort = [core::ptr::null_mut(); COHORT];
    for block in &mut realloc_cohort {
        *block = core.alloc(layout);
        assert!(!block.is_null(), "fresh 8 MiB allocation must succeed");
    }
    let selected = AllocCore::dbg_thp_control_telemetry();
    assert_eq!(
        selected[GROUP_8_TO_32_MIB],
        [1, 0, 0, 0, 0, 0, 0, 0],
        "one selection, no premise failures, no observations yet"
    );
    assert!(selected
        .iter()
        .enumerate()
        .all(|(group, counters)| group == GROUP_8_TO_32_MIB || *counters == [0; 8]));

    for block in &mut realloc_cohort {
        let old_ptr = *block;
        // SAFETY: `old_ptr` is a live allocation from `core` with exactly `layout`;
        // a successful realloc consumes it, and the returned pointer is retained.
        let grown_ptr = unsafe { core.realloc(old_ptr, layout, grown_size) };
        assert!(!grown_ptr.is_null(), "in-place growth must succeed");
        assert_eq!(
            grown_ptr, old_ptr,
            "growth must stay within the rounded span"
        );
        *block = grown_ptr;
    }
    // The in-place grow arm records the selected control as realloc-invalidated
    // exactly once, without selecting a replacement or observing the old one.
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB],
        [1, 0, 0, 0, 1, 0, 0, 0],
        "in-place reallocations must invalidate, not select or observe"
    );

    for block in realloc_cohort.iter().rev() {
        // SAFETY: each pointer is live with the matching post-realloc layout.
        unsafe { core.dealloc(*block, grown_layout) };
    }
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB],
        [1, 0, 0, 0, 1, 0, 0, 0],
        "freeing realloc-invalidated controls must not report observations"
    );

    // A second full fresh cohort selects its own control; leave it live for core teardown.
    let mut live_at_teardown = [core::ptr::null_mut(); COHORT];
    for block in &mut live_at_teardown {
        *block = core.alloc(layout);
        assert!(
            !block.is_null(),
            "fresh teardown-cohort allocation must succeed"
        );
    }
    let before_teardown = AllocCore::dbg_thp_control_telemetry();
    assert_eq!(
        before_teardown[GROUP_8_TO_32_MIB],
        [2, 0, 0, 0, 1, 0, 0, 0],
        "the second cohort selects one more control"
    );
    assert!(before_teardown
        .iter()
        .enumerate()
        .all(|(group, counters)| group == GROUP_8_TO_32_MIB || *counters == [0; 8]));

    // The live raw allocations are no longer accessible after their owning core is dropped.
    drop(core);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry(),
        before_teardown,
        "teardown of live controls must not produce residency observations"
    );
}
