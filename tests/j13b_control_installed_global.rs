//! J13b control telemetry: the installed GlobalAlloc boundary selects and
//! observes controls per group through the public allocator surface. One
//! scenario per binary.

#![cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, SeferAlloc};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const MIB: usize = 1024 * 1024;
const BLOCK: usize = 16 * MIB;
const COHORT: usize = 8;
const GROUP_8_TO_32_MIB: usize = 2;

#[test]
fn installed_global_allocator_selects_and_observes_controls() {
    // Bootstrap the process tuple through an explicit config, then drop it
    // (matches the established primordial-bootstrap pattern).
    drop(
        AllocCore::new_with_config(LargeCacheConfig::new().huge_page_mode(LargeThpMode::Auto))
            .expect("primordial bootstrap"),
    );

    let layout = Layout::from_size_align(BLOCK, 8).expect("valid 16 MiB layout");
    let mut blocks = [core::ptr::null_mut(); COHORT];
    for block in &mut blocks {
        // SAFETY: valid non-zero layout; the returned pointer is retained.
        *block = unsafe { GLOBAL.alloc(layout) };
        assert!(!block.is_null(), "16 MiB allocation must succeed");
        // Sparse payload touch: one volatile byte per block, deliberately not
        // touching the header or any other payload base page.
        // SAFETY: `block` is the live payload pointer returned for `layout`.
        unsafe { block.write_volatile(0x5a) };
    }

    let before_free = AllocCore::dbg_thp_control_telemetry();
    assert_eq!(
        before_free[GROUP_8_TO_32_MIB],
        [1, 0, 0, 0, 0, 0, 0, 0],
        "the installed GlobalAlloc boundary selects one control for the cohort"
    );
    assert!(before_free
        .iter()
        .enumerate()
        .all(|(group, counters)| group == GROUP_8_TO_32_MIB || *counters == [0; 8]));

    for block in blocks.iter().rev() {
        // SAFETY: each pointer is live with the original layout; freed
        // exactly once.
        unsafe { GLOBAL.dealloc(*block, layout) };
    }

    let control = AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB];
    assert_eq!(
        &control[..6],
        &[1, 0, 1, 0, 0, 0],
        "the control is observed validly with no errors or invalidations"
    );
    let resident = control[6] as u128;
    let sampled = control[7] as u128;
    assert!(
        sampled > 0,
        "valid sampling must report sampled payload pages"
    );
    assert!(
        resident * 100 < sampled,
        "sparse control residency {resident}/{sampled} must be below 1%; header pages are excluded"
    );
    let learning = AllocCore::dbg_thp_learning_telemetry()[GROUP_8_TO_32_MIB];
    assert_eq!(
        &learning[3..],
        &[1, 0],
        "only the clean control trains the installed allocator"
    );
    assert!(learning[0] < u64::from(sefer_alloc::LargeThpShare::SCALE));
    assert!(learning[2] < u64::from(sefer_alloc::LargeThpShare::SCALE));
    assert!(
        AllocCore::dbg_thp_advised_bytes_total() > 0,
        "non-control Auto allocations must actually activate advice"
    );
}
