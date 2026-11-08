//! J13b C1: `LargeCacheConfig` large-THP knobs resolve into one tuple
//! (defaults, each knob alone, partial mixes, const builders).
#![cfg(all(feature = "internals", feature = "alloc-decommit"))]

use sefer_alloc::alloc_core::policy::large_thp_runtime::ThpTuple;
use sefer_alloc::{LargeCacheConfig, LargeThpGradient, LargeThpMode};

const MIB: usize = 1024 * 1024;

const CONST_CFG: LargeCacheConfig = LargeCacheConfig::new()
    .huge_page_mode(LargeThpMode::Always)
    .huge_page_block_min_bytes(0)
    .huge_page_budget_bytes(64 * MIB)
    .huge_page_gradient(LargeThpGradient::EAGER);

#[test]
fn plain_config_is_the_default_tuple() {
    let t = LargeCacheConfig::new().dbg_resolved_thp_tuple();
    assert_eq!(t.mode, LargeThpMode::Off);
    assert_eq!(t.min_bytes, 8 * MIB);
    assert_eq!(t.budget_bytes, 0);
    assert_eq!(t.gradient, LargeThpGradient::CAUTIOUS);
    assert_eq!(t, ThpTuple::DEFAULT);
    assert_eq!(LargeCacheConfig::DEFAULT.dbg_resolved_thp_tuple(), t);
}

#[test]
fn mode_alone() {
    let t = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Auto)
        .dbg_resolved_thp_tuple();
    assert_eq!(t.mode, LargeThpMode::Auto);
    assert_eq!(t.min_bytes, 8 * MIB);
    assert_eq!(t.budget_bytes, 0);
    assert_eq!(t.gradient, LargeThpGradient::CAUTIOUS);
}

#[test]
fn min_alone_zero_is_not_off() {
    let t = LargeCacheConfig::new()
        .huge_page_block_min_bytes(0)
        .dbg_resolved_thp_tuple();
    assert_eq!(t.min_bytes, 0);
    assert_eq!(t.mode, LargeThpMode::Off);
    let t = LargeCacheConfig::new()
        .huge_page_block_min_bytes(3 * MIB)
        .dbg_resolved_thp_tuple();
    assert_eq!(t.min_bytes, 3 * MIB);
}

#[test]
fn budget_alone_zero_is_unlimited() {
    let t = LargeCacheConfig::new()
        .huge_page_budget_bytes(0)
        .dbg_resolved_thp_tuple();
    assert_eq!(t.budget_bytes, 0);
    assert_eq!(t.min_bytes, 8 * MIB);
    let t = LargeCacheConfig::new()
        .huge_page_budget_bytes(10 * MIB)
        .dbg_resolved_thp_tuple();
    assert_eq!(t.budget_bytes, 10 * MIB);
}

#[test]
fn gradient_alone() {
    let t = LargeCacheConfig::new()
        .huge_page_gradient(LargeThpGradient::STEADY)
        .dbg_resolved_thp_tuple();
    assert_eq!(t.gradient, LargeThpGradient::STEADY);
    assert_eq!(t.mode, LargeThpMode::Off);
    assert_eq!(t.min_bytes, 8 * MIB);
    assert_eq!(t.budget_bytes, 0);
}

#[test]
fn partial_specification_mixes_defaults() {
    let t = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Always)
        .huge_page_budget_bytes(5 * MIB)
        .dbg_resolved_thp_tuple();
    assert_eq!(t.mode, LargeThpMode::Always);
    assert_eq!(t.min_bytes, 8 * MIB);
    assert_eq!(t.budget_bytes, 5 * MIB);
    assert_eq!(t.gradient, LargeThpGradient::CAUTIOUS);
}

#[test]
fn builders_are_const() {
    let t = CONST_CFG.dbg_resolved_thp_tuple();
    assert_eq!(t.mode, LargeThpMode::Always);
    assert_eq!(t.min_bytes, 0);
    assert_eq!(t.budget_bytes, 64 * MIB);
    assert_eq!(t.gradient, LargeThpGradient::EAGER);
}

#[test]
fn cache_budget_is_independent_of_thp_budget() {
    let t = LargeCacheConfig::new()
        .budget_bytes(512 * MIB)
        .dbg_resolved_thp_tuple();
    assert_eq!(t.budget_bytes, 0);
    let t = LargeCacheConfig::new()
        .huge_page_budget_bytes(7 * MIB)
        .dbg_resolved_thp_tuple();
    assert_eq!(t.budget_bytes, 7 * MIB);
    let both = LargeCacheConfig::new()
        .budget_bytes(512 * MIB)
        .huge_page_budget_bytes(7 * MIB);
    assert_eq!(both.dbg_resolved_thp_tuple().budget_bytes, 7 * MIB);
}
