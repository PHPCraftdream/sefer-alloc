//! J13b C1: `ThpTuple` five-word pack/unpack codec.
#![cfg(all(feature = "internals", feature = "alloc-core"))]

use sefer_alloc::alloc_core::policy::large_thp_runtime::ThpTuple;
use sefer_alloc::{LargeThpCurve, LargeThpGradient, LargeThpMode};

const MIB: usize = 1024 * 1024;

fn roundtrip(t: ThpTuple) {
    let w = t.pack();
    assert_ne!(w[0], 0);
    assert_eq!(ThpTuple::try_unpack(w), Some(t));
    assert_eq!(ThpTuple::unpack(w), t);
}

fn tuple(mode: LargeThpMode, min: usize, budget: usize, g: LargeThpGradient) -> ThpTuple {
    ThpTuple {
        mode,
        min_bytes: min,
        budget_bytes: budget,
        gradient: g,
    }
}

#[test]
fn default_tuple_roundtrips() {
    let d = ThpTuple::DEFAULT;
    assert_eq!(
        d.mode,
        if cfg!(all(
            target_os = "linux",
            feature = "alloc-decommit",
            not(feature = "numa-aware")
        )) {
            LargeThpMode::Always
        } else {
            LargeThpMode::Off
        }
    );
    assert_eq!(d.min_bytes, 8 * MIB);
    assert_eq!(d.budget_bytes, 0);
    assert_eq!(d.gradient, LargeThpGradient::CAUTIOUS);
    roundtrip(d);
}

#[test]
fn always_min_zero_with_budget_roundtrips() {
    let t = tuple(
        LargeThpMode::Always,
        0,
        96 * MIB,
        LargeThpGradient::CAUTIOUS,
    );
    roundtrip(t);
    let w = t.pack();
    assert_eq!(w[3], 0);
    assert_eq!(w[4], (96 * MIB) as u64);
}

#[test]
fn auto_with_each_named_gradient_roundtrips() {
    for g in [
        LargeThpGradient::CAUTIOUS,
        LargeThpGradient::EAGER,
        LargeThpGradient::STEADY,
    ] {
        roundtrip(tuple(LargeThpMode::Auto, 8 * MIB, 0, g));
    }
}

#[test]
fn extreme_custom_gradients_roundtrip() {
    for curve in [
        LargeThpCurve::Generous,
        LargeThpCurve::Linear,
        LargeThpCurve::Strict,
    ] {
        for g in [
            LargeThpGradient::custom(0, 0, 0, 0, 0, 0, 1, curve),
            LargeThpGradient::custom(
                u32::MAX,
                u32::MAX,
                u32::MAX,
                u32::MAX,
                u32::MAX,
                u32::MAX,
                u32::MAX,
                curve,
            ),
            LargeThpGradient::custom(31, 0, 31, 1 << 31, 1, u32::MAX, 0, curve),
        ] {
            for mode in [LargeThpMode::Off, LargeThpMode::Always, LargeThpMode::Auto] {
                roundtrip(tuple(mode, usize::MAX, usize::MAX, g));
                roundtrip(tuple(mode, 0, 0, g));
            }
        }
    }
}

#[test]
fn corrupted_tag_mode_curve_decode_to_default() {
    let t = tuple(LargeThpMode::Always, MIB, 4 * MIB, LargeThpGradient::EAGER);
    let w = t.pack();
    assert_ne!(t, ThpTuple::DEFAULT);

    let mut bad = w;
    bad[0] ^= 1 << 56;
    assert_eq!(ThpTuple::try_unpack(bad), None);
    assert_eq!(ThpTuple::unpack(bad), ThpTuple::DEFAULT);

    let mut bad = w;
    bad[0] |= 0b11;
    assert_eq!(ThpTuple::try_unpack(bad), None);
    assert_eq!(ThpTuple::unpack(bad), ThpTuple::DEFAULT);

    let mut bad = w;
    bad[0] |= 0b11 << 2;
    assert_eq!(ThpTuple::try_unpack(bad), None);
    assert_eq!(ThpTuple::unpack(bad), ThpTuple::DEFAULT);

    assert_eq!(ThpTuple::unpack([0; 5]), ThpTuple::DEFAULT);
    assert_eq!(ThpTuple::try_unpack([0; 5]), None);
}

#[test]
fn word_zero_is_never_zero() {
    for mode in [LargeThpMode::Off, LargeThpMode::Always, LargeThpMode::Auto] {
        for g in [
            LargeThpGradient::custom(0, 0, 0, 0, 0, 0, 1, LargeThpCurve::Generous),
            LargeThpGradient::CAUTIOUS,
        ] {
            assert_ne!(tuple(mode, 0, 0, g).pack()[0], 0);
        }
    }
}
