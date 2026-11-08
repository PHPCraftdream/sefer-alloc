//! J13b control telemetry: the AllocCore move-leg realloc path selects exactly
//! one fresh control among the eight move destinations, records the old
//! control as realloc-invalidated, observes the copied-into control exactly
//! once at free, and a failed (OOM-injected) realloc leaves telemetry
//! byte-identical. One scenario per binary.

#![cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals",
    not(feature = "numa-aware"),
    not(feature = "exact-span-large"),
    not(feature = "large-reserved-capacity")
))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode};

const MIB: usize = 1024 * 1024;
const BLOCK: usize = 8 * MIB;
const MOVED: usize = 16 * MIB;
const COHORT: usize = 8;
const GROUP_8_TO_32_MIB: usize = 2;

#[test]
fn move_leg_realloc_selects_invalidates_and_observes_once() {
    let config = LargeCacheConfig::new().huge_page_mode(LargeThpMode::Auto);
    let mut core = AllocCore::new_with_config(config).expect("primordial bootstrap");
    // A zero cache budget forces every free to release its mapping and keeps
    // every request fresh for the ordinal-based control selection.
    core.dbg_set_large_cache_budget(Some(0));

    let layout = Layout::from_size_align(BLOCK, 8).expect("valid 8 MiB layout");
    let moved_layout = Layout::from_size_align(MOVED, 8).expect("valid 16 MiB layout");

    let mut blocks = [core::ptr::null_mut(); COHORT];
    for block in &mut blocks {
        *block = core.alloc(layout);
        assert!(!block.is_null(), "8 MiB allocation must succeed");
    }
    let control = AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB];
    assert_eq!(control, [1, 0, 0, 0, 0, 0, 0, 0]);
    assert!(AllocCore::dbg_thp_control_telemetry()
        .iter()
        .enumerate()
        .all(|(group, counters)| group == GROUP_8_TO_32_MIB || *counters == [0; 8]));

    // Move leg: grow each block to 16 MiB. A 16 MiB request does not fit the
    // usable span of an 8 MiB request, so every realloc must move.
    for block in &mut blocks {
        let old_ptr = *block;
        // SAFETY: `old_ptr` is a live owned pointer with its exact original
        // `layout`; a successful realloc consumes it and the returned pointer
        // is retained in its place.
        let moved_ptr = unsafe { core.realloc(old_ptr, layout, MOVED) };
        assert!(!moved_ptr.is_null(), "16 MiB move realloc must succeed");
        assert_ne!(
            moved_ptr, old_ptr,
            "16 MiB does not fit the 8 MiB usable span, so the realloc must move"
        );
        *block = moved_ptr;
    }
    let after_moves = AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB];
    assert_eq!(
        after_moves,
        [2, 0, 0, 0, 1, 0, 0, 0],
        "one fresh move destination selected a new control; the old control was invalidated-realloc"
    );

    // Allocation-failure boundary: the hook forces `alloc_large` to return
    // null while set, so the next move realloc must fail without touching
    // telemetry, and the old pointer must stay valid.
    let before_failure = AllocCore::dbg_thp_control_telemetry();
    AllocCore::dbg_set_inject_large_alloc_oom(true);
    // SAFETY: `blocks[0]` is a live owned pointer with its exact `moved_layout`
    // layout; a failed realloc leaves it valid and owned.
    let failed = unsafe { core.realloc(blocks[0], moved_layout, 24 * MIB) };
    assert!(failed.is_null(), "injected OOM must fail the realloc");
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry(),
        before_failure,
        "failed realloc must not select, invalidate, or observe anything"
    );
    AllocCore::dbg_set_inject_large_alloc_oom(false);

    for block in blocks.iter().rev() {
        // SAFETY: each pointer is the live moved allocation with its exact
        // 16 MiB layout; freed exactly once.
        unsafe { core.dealloc(*block, moved_layout) };
    }

    let final_counts = AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB];
    assert_eq!(
        &final_counts[..6],
        &[2, 0, 1, 0, 1, 0],
        "one fresh move destination observed validly; one old control invalidated-realloc"
    );
    assert!(
        final_counts[6] > 0,
        "the copied-into control reports resident payload pages"
    );
    assert!(
        final_counts[7] > 0,
        "the copied-into control reports sampled payload pages"
    );
    drop(core);
}
