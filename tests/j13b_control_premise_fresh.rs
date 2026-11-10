//! J13b control telemetry: a never-touched fresh mapping observes exactly zero
//! resident payload pages, proving nothing was faulted before the
//! MADV_NOHUGEPAGE premise and that sampling excludes the header. One
//! scenario per binary.

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
fn fresh_unwritten_mapping_observes_zero_resident_pages() {
    let config = LargeCacheConfig::new().huge_page_mode(LargeThpMode::Auto);
    let mut core = AllocCore::new_with_config(config).expect("primordial bootstrap");
    // A zero cache budget forces every free to release its mapping and keeps
    // every request fresh for the ordinal-based control selection.
    core.dbg_set_large_cache_budget(Some(0));

    let layout = Layout::from_size_align(BLOCK, 8).expect("valid block layout");
    let mut blocks = [core::ptr::null_mut(); COHORT];
    for block in &mut blocks {
        *block = core.alloc(layout);
        assert!(!block.is_null(), "8 MiB allocation must succeed");
        // Deliberately no payload writes: the mapping must stay fresh
        // (nothing faulted, base or huge) before the premise check.
    }

    // Free in reverse without ever touching a payload byte.
    for block in blocks.iter().rev() {
        // SAFETY: each pointer is live with the original layout; freed
        // exactly once.
        unsafe { core.dealloc(*block, layout) };
    }

    let control = AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB];
    assert_eq!(
        &control[..6],
        &[1, 0, 1, 0, 0, 0],
        "the fresh control is observed validly with no errors or invalidations"
    );
    assert_eq!(
        control[6], 0,
        "a never-touched fresh mapping must report zero resident payload pages"
    );
    assert!(
        control[7] > 0,
        "sampling must still cover payload pages, excluding the header"
    );
    drop(core);
}
