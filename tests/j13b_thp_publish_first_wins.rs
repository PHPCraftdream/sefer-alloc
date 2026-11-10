//! J13b C2: the first explicit large-THP tuple wins whole; later different
//! tuples count exactly one conflict each and never leak a single knob.
//! Process-global state: one test in this binary.
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

fn full(mode: LargeThpMode, min: usize, budget: usize, g: LargeThpGradient) -> LargeCacheConfig {
    LargeCacheConfig::new()
        .huge_page_mode(mode)
        .huge_page_block_min_bytes(min)
        .huge_page_budget_bytes(budget)
        .huge_page_gradient(g)
}

fn materialise(cfg: LargeCacheConfig) {
    let core = AllocCore::new_with_config(cfg).expect("primordial");
    drop(core);
}

#[test]
fn first_tuple_wins_whole_and_each_loser_counts_once() {
    assert_eq!(AllocCore::dbg_thp_tuple(), ThpTuple::DEFAULT);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);

    let a_cfg = full(
        LargeThpMode::Always,
        16 * MIB,
        1024 * MIB,
        LargeThpGradient::EAGER,
    );
    let a = a_cfg.dbg_resolved_thp_tuple();
    assert_ne!(a, ThpTuple::DEFAULT);

    materialise(a_cfg);
    assert_eq!(AllocCore::dbg_thp_tuple(), a);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);

    let others = [
        full(
            LargeThpMode::Always,
            16 * MIB,
            1024 * MIB + 1,
            LargeThpGradient::EAGER,
        ),
        full(
            LargeThpMode::Always,
            16 * MIB + 1,
            1024 * MIB,
            LargeThpGradient::EAGER,
        ),
        full(
            LargeThpMode::Auto,
            16 * MIB,
            1024 * MIB,
            LargeThpGradient::EAGER,
        ),
        full(
            LargeThpMode::Always,
            16 * MIB,
            1024 * MIB,
            LargeThpGradient::STEADY,
        ),
        full(LargeThpMode::Auto, 4 * MIB, 0, LargeThpGradient::CAUTIOUS),
        full(LargeThpMode::Off, 16 * MIB, 0, LargeThpGradient::EAGER),
        LargeCacheConfig::new().huge_page_mode(LargeThpMode::Off),
    ];
    for (i, cfg) in others.into_iter().enumerate() {
        let b = cfg.dbg_resolved_thp_tuple();
        assert_ne!(b, a, "candidate {i} must differ from the winner");
        let before = AllocCore::dbg_thp_tuple_conflicts();
        materialise(cfg);
        assert_eq!(
            AllocCore::dbg_thp_tuple_conflicts(),
            before + 1,
            "candidate {i}: exactly one conflict"
        );
        assert_eq!(
            AllocCore::dbg_thp_tuple(),
            a,
            "candidate {i}: no knob leaks"
        );
    }

    let before = AllocCore::dbg_thp_tuple_conflicts();
    materialise(full(
        LargeThpMode::Always,
        16 * MIB,
        1024 * MIB,
        LargeThpGradient::EAGER,
    ));
    assert_eq!(
        AllocCore::dbg_thp_tuple_conflicts(),
        before,
        "identical tuple adds none"
    );
    assert_eq!(AllocCore::dbg_thp_tuple(), a);
}
