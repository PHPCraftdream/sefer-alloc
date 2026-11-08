//! J13b C2: an equal tuple published again is no conflict, and a config with
//! no large-THP knob never publishes or conflicts. One test: process-global.
#![cfg(all(
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals",
    not(feature = "numa-aware")
))]

use sefer_alloc::alloc_core::policy::large_thp_runtime::ThpTuple;
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::{LargeCacheConfig, LargeThpGradient, LargeThpMode};

const MIB: usize = 1024 * 1024;

fn materialise(cfg: LargeCacheConfig) {
    let core = AllocCore::new_with_config(cfg).expect("primordial");
    drop(core);
}

#[test]
fn equal_tuples_do_not_conflict_and_knobless_configs_stay_out() {
    let a_cfg = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Always)
        .huge_page_gradient(LargeThpGradient::STEADY);
    let a = a_cfg.dbg_resolved_thp_tuple();
    assert_eq!(a.mode, LargeThpMode::Always);
    assert_eq!(a.min_bytes, 8 * MIB);
    assert_eq!(a.budget_bytes, 0);

    materialise(LargeCacheConfig::new().huge_page_after_segments(3));
    assert_eq!(AllocCore::dbg_thp_tuple(), ThpTuple::DEFAULT);

    materialise(a_cfg);
    assert_eq!(AllocCore::dbg_thp_tuple(), a);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);

    materialise(a_cfg);
    assert_eq!(AllocCore::dbg_thp_tuple(), a);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);

    materialise(
        a_cfg
            .huge_page_block_min_bytes(8 * MIB)
            .huge_page_budget_bytes(0),
    );
    assert_eq!(AllocCore::dbg_thp_tuple(), a);
    assert_eq!(
        AllocCore::dbg_thp_tuple_conflicts(),
        0,
        "explicit defaults resolve equal"
    );

    materialise(LargeCacheConfig::new());
    materialise(LargeCacheConfig::new().huge_page_after_segments(5));
    assert_eq!(
        AllocCore::dbg_thp_tuple(),
        a,
        "knobless configs do not publish"
    );
    assert_eq!(
        AllocCore::dbg_thp_tuple_conflicts(),
        0,
        "knobless configs do not conflict"
    );
}
