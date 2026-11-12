//! J13b C2: with only knobless configs the process tuple stays the default.
//! One test: process-global.
#![cfg(all(
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals",
    not(feature = "numa-aware")
))]

use sefer_alloc::alloc_core::policy::large_thp_runtime::ThpTuple;
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::{LargeCacheConfig, LargeThpGradient, LargeThpMode};

#[test]
fn knobless_configs_leave_the_default_tuple() {
    for cfg in [
        LargeCacheConfig::new(),
        LargeCacheConfig::new().huge_page_after_segments(2),
        LargeCacheConfig::new().budget_bytes(64 * 1024 * 1024),
    ] {
        let core = AllocCore::new_with_config(cfg).expect("primordial");
        drop(core);
    }
    let t = AllocCore::dbg_thp_tuple();
    assert_eq!(t, ThpTuple::DEFAULT);
    assert_eq!(
        t.mode,
        if cfg!(target_os = "linux") {
            LargeThpMode::Always
        } else {
            LargeThpMode::Off
        }
    );
    assert_eq!(t.min_bytes, 8 * 1024 * 1024);
    assert_eq!(t.budget_bytes, 0);
    assert_eq!(t.gradient, LargeThpGradient::CAUTIOUS);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
}
