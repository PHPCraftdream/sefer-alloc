//! J13b: integration tests for the large-THP gradient math core
//! (`LargeThpShare`, `LargeThpCurve`, `LargeThpGradient`,
//! `LargeThpEstimator`, `LargeThpRequestGroup`).
//!
//! Coverage: request-group bucket boundaries, gradient parameter
//! normalization at extremes, share clamping, exact curve values, the
//! exact floor-sqrt property of `Generous` (plus monotonicity of all
//! three curves), one-step EMA rounding (half-ties away from zero),
//! totality at every extreme input, gain-product saturation, the floor
//! gate, the anchor estimate, interleaving independence/determinism, and
//! the update-then-report contract.
//!
//! Oracle discipline: every expected integer is justified by an inline
//! arithmetic comment, and oracles are stated as independent closed-form
//! mathematical laws (round-half-away via `(2*|d| + 2^shift) /
//! 2^(shift+1)`, the exact floor-sqrt property `y*y <= arg <
//! (y+1)*(y+1)`), never by copying the implementation's helper code.
#![cfg(feature = "alloc-decommit")]

use sefer_alloc::{
    LargeThpCurve, LargeThpEstimator, LargeThpGradient, LargeThpRequestGroup, LargeThpShare,
};

/// Shorthand for the Q31 scale (2^31 raw units == 1.0).
const SCALE: u32 = LargeThpShare::SCALE;

/// Closed-form round-half-away-from-zero of `diff / 2^shift`, computed as
/// `(2*|d| + 2^shift) / 2^(shift+1)` (sign applied). Independent of the
/// implementation: it is the standard definition of the rounding rule,
/// not a copy of its code. For `shift == 0` the formula degenerates to
/// `diff` exactly (rate 1.0 moves the state onto the observation).
fn rhalf(diff: i64, shift: u32) -> i64 {
    if diff >= 0 {
        (2 * diff + (1i64 << shift)) / (1i64 << (shift + 1))
    } else {
        -((2 * (-diff) + (1i64 << shift)) / (1i64 << (shift + 1)))
    }
}

#[test]
fn request_group_boundaries_exact() {
    // Every boundary is classified with its lower neighbor bucket; each
    // expectation follows directly from `bytes / MIB` against the bucket
    // ranges [0,1), [1,8), [8,32), [32,128), [128,512), [512, inf) MiB.
    assert_eq!(
        LargeThpRequestGroup::of_bytes(0),
        LargeThpRequestGroup::Under1MiB
    );
    assert_eq!(
        LargeThpRequestGroup::of_bytes(LargeThpRequestGroup::MIB - 1),
        LargeThpRequestGroup::Under1MiB
    );
    assert_eq!(
        LargeThpRequestGroup::of_bytes(LargeThpRequestGroup::B1_MIB),
        LargeThpRequestGroup::MiB1To8
    );
    assert_eq!(
        LargeThpRequestGroup::of_bytes(LargeThpRequestGroup::B8_MIB - 1),
        LargeThpRequestGroup::MiB1To8
    );
    assert_eq!(
        LargeThpRequestGroup::of_bytes(LargeThpRequestGroup::B8_MIB),
        LargeThpRequestGroup::MiB8To32
    );
    assert_eq!(
        LargeThpRequestGroup::of_bytes(LargeThpRequestGroup::B32_MIB - 1),
        LargeThpRequestGroup::MiB8To32
    );
    assert_eq!(
        LargeThpRequestGroup::of_bytes(LargeThpRequestGroup::B32_MIB),
        LargeThpRequestGroup::MiB32To128
    );
    assert_eq!(
        LargeThpRequestGroup::of_bytes(LargeThpRequestGroup::B128_MIB - 1),
        LargeThpRequestGroup::MiB32To128
    );
    assert_eq!(
        LargeThpRequestGroup::of_bytes(LargeThpRequestGroup::B128_MIB),
        LargeThpRequestGroup::MiB128To512
    );
    assert_eq!(
        LargeThpRequestGroup::of_bytes(LargeThpRequestGroup::B512_MIB - 1),
        LargeThpRequestGroup::MiB128To512
    );
    assert_eq!(
        LargeThpRequestGroup::of_bytes(LargeThpRequestGroup::B512_MIB),
        LargeThpRequestGroup::Over512MiB
    );
    assert_eq!(
        LargeThpRequestGroup::of_bytes(u64::MAX),
        LargeThpRequestGroup::Over512MiB
    );
}

#[test]
fn custom_params_normalize_extremes() {
    // All-max constructor: shifts clamp to MAX_SHIFT == 31, anchor/floor
    // clamp to [0, SCALE] (so both u32::MAX become SCALE == 2^31),
    // gain_den 0 normalizes to 1, gain_num passes through unclamped.
    let g = LargeThpGradient::custom(
        u32::MAX,
        u32::MAX,
        u32::MAX,
        u32::MAX,
        u32::MAX,
        u32::MAX,
        0,
        LargeThpCurve::Strict,
    );
    assert_eq!(g.down_shift(), 31);
    assert_eq!(g.up_shift(), 31);
    assert_eq!(g.mean_shift(), 31);
    assert_eq!(g.anchor_raw(), LargeThpShare::SCALE);
    assert_eq!(g.floor_raw(), LargeThpShare::SCALE);
    assert_eq!(g.gain_num(), u32::MAX);
    assert_eq!(g.gain_den(), 1);
    assert_eq!(g.curve(), LargeThpCurve::Strict);

    // All-zero constructor stays all-zero (no accidental re-normalization
    // towards a default; gain_den 0 rule only fires on a zero denominator).
    let z = LargeThpGradient::custom(0, 0, 0, 0, 0, 0, 1, LargeThpCurve::Linear);
    assert_eq!(z.anchor_raw(), 0);
    assert_eq!(z.floor_raw(), 0);
    assert_eq!(z.gain_num(), 0);
}

#[test]
fn share_from_raw_clamps() {
    // u32::MAX = 2^32 - 1 > SCALE = 2^31, so it saturates to exactly ONE.
    assert_eq!(
        LargeThpShare::from_raw(u32::MAX).raw(),
        LargeThpShare::SCALE
    );
    assert_eq!(
        LargeThpShare::from_raw(LargeThpShare::SCALE),
        LargeThpShare::ONE
    );
    assert_eq!(LargeThpShare::from_raw(0), LargeThpShare::ZERO);
    // SCALE + 7 is above the representable range, so it saturates to SCALE.
    assert_eq!(
        LargeThpShare::from_raw(LargeThpShare::SCALE + 7).raw(),
        LargeThpShare::SCALE
    );
}

#[test]
fn curve_exact_values() {
    // Linear is the identity on the clamped input: eval(x) == x.
    for &x in &[
        0u32,
        1,
        1 << 30,
        LargeThpShare::SCALE - 1,
        LargeThpShare::SCALE,
    ] {
        assert_eq!(LargeThpCurve::Linear.eval(x), x);
    }
    // Input clamp: SCALE + 123 > SCALE saturates to SCALE before shaping.
    assert_eq!(
        LargeThpCurve::Linear.eval(LargeThpShare::SCALE + 123),
        LargeThpShare::SCALE
    );

    // Strict = (x*x) >> 31, exact truncation:
    //   0*0 >> 31 = 0; 1 >> 31 = 0; (2^30)^2 = 2^60, 2^60 >> 31 = 2^29;
    //   (2^31)^2 = 2^62, 2^62 >> 31 = 2^31 = SCALE.
    assert_eq!(LargeThpCurve::Strict.eval(0), 0);
    assert_eq!(LargeThpCurve::Strict.eval(1), 0);
    assert_eq!(LargeThpCurve::Strict.eval(1 << 30), 1 << 29);
    assert_eq!(
        LargeThpCurve::Strict.eval(LargeThpShare::SCALE),
        LargeThpShare::SCALE
    );

    // Generous endpoints are exact by definition:
    //   eval(0) = isqrt(0) = 0; eval(SCALE) = isqrt(2^31 << 31) = isqrt(2^62) = 2^31.
    assert_eq!(LargeThpCurve::Generous.eval(0), 0);
    assert_eq!(
        LargeThpCurve::Generous.eval(LargeThpShare::SCALE),
        LargeThpShare::SCALE
    );
}

#[test]
fn curve_generous_exact_floor_sqrt_property() {
    // Deterministic sweep, no RNG: every power of two 2^k, k = 0..=31,
    // plus k * 2^24 for k = 1..=128 (note: k up to 128, because
    // 128 * 2^24 == 2^31 == SCALE and inputs above SCALE are clamped,
    // which would break the floor-sqrt property statement for the raw
    // argument; 128 multiples tile [0, SCALE] evenly).
    let mut sweep: Vec<u64> = Vec::new();
    for k in 0..=31u32 {
        sweep.push(1u64 << k);
    }
    for k in 1..=128u64 {
        sweep.push(k << 24);
    }
    sweep.sort_unstable();

    for &x in &sweep {
        // x <= SCALE = 2^31 so x << 31 <= 2^62 fits u64 exactly.
        let arg: u64 = x << 31;
        let y = LargeThpCurve::Generous.eval(x as u32) as u64;
        // Exact floor-sqrt property, no float involved:
        // y = floor(sqrt(arg)) iff y*y <= arg AND (y+1)*(y+1) > arg.
        assert!(
            y * y <= arg,
            "y^2 <= arg violated at x = {x}: {} > {arg}",
            y * y
        );
        assert!(
            (y + 1) * (y + 1) > arg,
            "(y+1)^2 > arg violated at x = {x}: {} <= {arg}",
            (y + 1) * (y + 1)
        );
    }

    // Monotone non-decreasing across the sorted sweep for ALL variants:
    // Generous is sqrt (increasing), Linear identity, Strict square.
    for curve in [
        LargeThpCurve::Generous,
        LargeThpCurve::Linear,
        LargeThpCurve::Strict,
    ] {
        for w in sweep.windows(2) {
            let a = curve.eval(w[0] as u32);
            let b = curve.eval(w[1] as u32);
            assert!(
                a <= b,
                "{curve:?} not monotone at {0} -> {1}: {a} > {b}",
                w[0],
                w[1]
            );
        }
    }
}

#[test]
fn one_step_ema_rounding_exact() {
    // Rate 1.0 jump down (down_shift 0): e moves onto the observation
    // exactly. with_state(5, 0), o = 0: diff = -5, rhalf(-5, 0) = -5
    // (shift 0 degenerates to diff), so e = 5 - 5 = 0.
    // m: diff = -5, shift 5, rhalf(-5, 5) = -((10 + 32) / 64) = -(42/64) = 0.
    let mut st = LargeThpEstimator::with_state(5, 0);
    st.observe(
        &LargeThpGradient::custom(0, 7, 5, 0, 0, 1, 1, LargeThpCurve::Linear),
        LargeThpShare::ZERO,
    );
    assert_eq!(st.e_raw(), 0);
    assert_eq!(st.m_raw(), 0);

    // Half tie UP: diff = 102 - 100 = 2, rate 2^-2, 2/4 = 0.5 rounds AWAY
    // from zero -> step +1, e = 101. (Oracle: rhalf(2, 2) = (4 + 4) / 8 = 1.)
    let g = LargeThpGradient::custom(2, 2, 2, 0, 0, 1, 1, LargeThpCurve::Linear);
    let mut st = LargeThpEstimator::with_state(100, 100);
    st.observe(&g, LargeThpShare::from_raw(102));
    assert_eq!(st.e_raw(), 101);
    assert_eq!(st.m_raw(), 101);

    // Half tie DOWN: diff = -2, -0.5 rounds away from zero -> step -1.
    // (Oracle: rhalf(-2, 2) = -((4 + 4) / 8) = -1.)
    let mut st = LargeThpEstimator::with_state(102, 102);
    st.observe(&g, LargeThpShare::from_raw(100));
    assert_eq!(st.e_raw(), 101);
    assert_eq!(st.m_raw(), 101);

    // Below half-quantum: diff = -1, |step| = (2*1 + 4) / 8 = 0 (3/8 < 1/2),
    // no move.
    let mut st = LargeThpEstimator::with_state(101, 101);
    st.observe(&g, LargeThpShare::from_raw(100));
    assert_eq!(st.e_raw(), 101);
    assert_eq!(st.m_raw(), 101);

    // Quarter up: diff = 3, step = (6 + 4) / 8 = 1 (0.75 rounds to 1),
    // e = 101.
    let mut st = LargeThpEstimator::with_state(100, 100);
    st.observe(&g, LargeThpShare::from_raw(103));
    assert_eq!(st.e_raw(), 101);
    assert_eq!(st.m_raw(), 101);

    // Full-scale fall with Cautious, from new() (both at SCALE = 2^31),
    // o = 0. e: diff = -SCALE, down rate 2^-2 -> step -(SCALE/4) =
    // -536_870_912 (exact), e = SCALE - SCALE/4 = 1_610_612_736.
    // m: diff = -SCALE, mean rate 2^-5 -> step -(SCALE/32) = -67_108_864
    // (exact: 2^31/2^5 = 2^26), m = 2_147_483_648 - 67_108_864 =
    // 2_080_374_784. e != m after a single observe is the point: it asserts
    // the down-vs-mean rate DIVERGENCE (anti-mutation guard for mean_shift).
    let mut st = LargeThpEstimator::new();
    st.observe(&LargeThpGradient::CAUTIOUS, LargeThpShare::ZERO);
    assert_eq!(st.e_raw(), SCALE - rhalf(SCALE as i64, 2) as u32);
    assert_eq!(st.e_raw(), 1_610_612_736);
    assert_eq!(st.m_raw(), SCALE - rhalf(SCALE as i64, 5) as u32);
    assert_eq!(st.m_raw(), 2_080_374_784);

    // Max-shift one-ulp crawl: shifts 31 => rate 2^-31, one raw unit per
    // step. Step 1: diff = -2^31, step = -((2^31 + 2^30) >> 31) = -1
    // (exactly 1.5 -> 1), e = SCALE - 1. Step 2: diff = -(SCALE - 1),
    // step = -((SCALE - 1 + 2^30) >> 31) = -1 (1.4999 -> 1), e = SCALE - 2.
    let g31 = LargeThpGradient::custom(31, 31, 31, 0, 0, 1, 1, LargeThpCurve::Linear);
    let mut st = LargeThpEstimator::with_state(LargeThpShare::SCALE, LargeThpShare::SCALE);
    st.observe(&g31, LargeThpShare::ZERO);
    assert_eq!(st.e_raw(), LargeThpShare::SCALE - 1);
    st.observe(&g31, LargeThpShare::ZERO);
    assert_eq!(st.e_raw(), LargeThpShare::SCALE - 2);

    // Small delta: diff = (SCALE-32) - (SCALE-16) = -16 for BOTH state
    // components. e: rhalf(-16, 2) = -((32 + 4) / 8) = -4, e = SCALE - 20.
    // m: rhalf(-16, 5) = -((32 + 32) / 64) = -1 (16/32 is exactly 0.5,
    // round-half-away from zero -> 1), m = SCALE - 17: this pins the
    // half-quantum rounding of the mean rate on the same observation.
    let mut st = LargeThpEstimator::with_state(SCALE - 16, SCALE - 16);
    st.observe(
        &LargeThpGradient::CAUTIOUS,
        LargeThpShare::from_raw(SCALE - 32),
    );
    assert_eq!(st.e_raw(), SCALE - 20);
    assert_eq!(st.m_raw(), SCALE - 17);
}

#[test]
fn bounds_all_extremes_no_panic() {
    // Totality probe: every (gradient, initial state, observation)
    // combination must produce in-range state and shares without panic.
    let gradients = [
        LargeThpGradient::CAUTIOUS,
        LargeThpGradient::EAGER,
        LargeThpGradient::STEADY,
        LargeThpGradient::custom(
            u32::MAX,
            u32::MAX,
            u32::MAX,
            u32::MAX,
            u32::MAX,
            u32::MAX,
            0,
            LargeThpCurve::Strict,
        ),
        LargeThpGradient::custom(0, 0, 0, 0, 0, 0, 1, LargeThpCurve::Linear),
        LargeThpGradient::custom(
            0,
            0,
            0,
            LargeThpShare::SCALE,
            LargeThpShare::SCALE,
            u32::MAX,
            u32::MAX,
            LargeThpCurve::Generous,
        ),
    ];
    let states = [
        LargeThpEstimator::new(),
        LargeThpEstimator::with_state(0, 0),
        LargeThpEstimator::with_state(LargeThpShare::SCALE, LargeThpShare::SCALE),
        LargeThpEstimator::with_state(u32::MAX, u32::MAX),
    ];
    let obs = [
        LargeThpShare::ZERO,
        LargeThpShare::ONE,
        LargeThpShare::from_raw(u32::MAX),
    ];
    let scale = LargeThpShare::SCALE;
    for g in &gradients {
        for st0 in &states {
            for o in &obs {
                let mut st = *st0;
                let share = st.observe(g, *o);
                assert!(share.raw() <= scale);
                assert!(st.e_raw() <= scale);
                assert!(st.m_raw() <= scale);
                assert!(st.bounded_share(g).raw() <= scale);
                assert!(st.estimate(g).raw() <= scale);
            }
        }
    }
}

#[test]
fn gain_overflow_saturates_exact() {
    // Gain u32::MAX / 1: product u32::MAX * SCALE = (2^32 - 1) * 2^31 < 2^63,
    // fits u64 exactly; divided by 1 it vastly exceeds SCALE so it clamps
    // to ONE.
    let g = LargeThpGradient::custom(2, 2, 2, 0, 0, u32::MAX, 1, LargeThpCurve::Linear);
    let st = LargeThpEstimator::with_state(LargeThpShare::SCALE, LargeThpShare::SCALE);
    assert_eq!(st.bounded_share(&g), LargeThpShare::ONE);

    // Gain u32::MAX / u32::MAX == 1 exactly: with e = m = SCALE the share
    // is (MAX * SCALE) / MAX == SCALE exactly.
    let g = LargeThpGradient::custom(2, 2, 2, 0, 0, u32::MAX, u32::MAX, LargeThpCurve::Linear);
    let st = LargeThpEstimator::with_state(LargeThpShare::SCALE, LargeThpShare::SCALE);
    assert_eq!(st.bounded_share(&g).raw(), LargeThpShare::SCALE);

    // With e = m = SCALE / 2 = 2^30: (MAX * 2^30) / MAX == 2^30 == SCALE/2
    // exactly ((2^32 - 1) * 2^30 < 2^62, fits u64).
    let st = LargeThpEstimator::with_state(LargeThpShare::SCALE / 2, LargeThpShare::SCALE / 2);
    assert_eq!(st.bounded_share(&g).raw(), LargeThpShare::SCALE / 2);
}

#[test]
fn floor_gate_exact() {
    // Cautious (floor 214_748_365 = 0.10 rounded to nearest Q31, anchor 0,
    // gain 5/4, Linear). est == e == 214_748_365 == floor, so the gate
    // (strict `<`) does NOT fire: share = (5 * 214_748_365) / 4 =
    // 1_073_741_825 / 4 = 268_435_456 (floor division, .25 dropped).
    let st = LargeThpEstimator::with_state(214_748_365, 999);
    assert_eq!(
        st.bounded_share(&LargeThpGradient::CAUTIOUS).raw(),
        268_435_456
    );

    // One unit below the floor: est = 214_748_364 < floor -> share 0.
    let st = LargeThpEstimator::with_state(214_748_364, 999);
    assert_eq!(st.bounded_share(&LargeThpGradient::CAUTIOUS).raw(), 0);

    // Custom floor at SCALE: est = SCALE - 1 < SCALE -> 0; est = SCALE
    // passes and min(SCALE, 5 * SCALE / 4 = 2_684_354_560) == SCALE.
    let g = LargeThpGradient::custom(
        2,
        2,
        2,
        0,
        LargeThpShare::SCALE,
        5,
        4,
        LargeThpCurve::Linear,
    );
    let st = LargeThpEstimator::with_state(LargeThpShare::SCALE - 1, 0);
    assert_eq!(st.bounded_share(&g).raw(), 0);
    let st = LargeThpEstimator::with_state(LargeThpShare::SCALE, 0);
    assert_eq!(st.bounded_share(&g).raw(), LargeThpShare::SCALE);
}

#[test]
fn anchor_estimate_exact() {
    // STEADY anchor = 2^30, est = e + round_half_up(|m - e| * 2^30 / SCALE)
    // with the sign of (m - e).
    // e = SCALE, m = 0: d = -SCALE, mag = 2^31 * 2^30 = 2^61,
    //   q = (2^61 + 2^30) >> 31 = 2^30, signed -2^30, est = 2^31 - 2^30 = 2^30.
    let st = LargeThpEstimator::with_state(LargeThpShare::SCALE, 0);
    assert_eq!(st.estimate(&LargeThpGradient::STEADY).raw(), 1 << 30);

    // e = 0, m = SCALE: q = +2^30, est = 0 + 2^30 = 2^30.
    let st = LargeThpEstimator::with_state(0, LargeThpShare::SCALE);
    assert_eq!(st.estimate(&LargeThpGradient::STEADY).raw(), 1 << 30);

    // e == m: d = 0, q = 0, est = SCALE.
    let st = LargeThpEstimator::with_state(LargeThpShare::SCALE, LargeThpShare::SCALE);
    assert_eq!(
        st.estimate(&LargeThpGradient::STEADY).raw(),
        LargeThpShare::SCALE
    );

    // All three estimates are in-range shares by construction; re-assert.
    for st in [
        LargeThpEstimator::with_state(LargeThpShare::SCALE, 0),
        LargeThpEstimator::with_state(0, LargeThpShare::SCALE),
        LargeThpEstimator::with_state(LargeThpShare::SCALE, LargeThpShare::SCALE),
    ] {
        assert!(st.estimate(&LargeThpGradient::STEADY).raw() <= LargeThpShare::SCALE);
    }
}

#[test]
fn independence_determinism_exact() {
    let scale = LargeThpShare::SCALE;
    let s1: [LargeThpShare; 24] = [
        LargeThpShare::ONE,
        LargeThpShare::ONE,
        LargeThpShare::ONE,
        LargeThpShare::ONE,
        LargeThpShare::ONE,
        LargeThpShare::ONE,
        LargeThpShare::ONE,
        LargeThpShare::ONE,
        LargeThpShare::ZERO,
        LargeThpShare::ZERO,
        LargeThpShare::ZERO,
        LargeThpShare::ZERO,
        LargeThpShare::ZERO,
        LargeThpShare::ZERO,
        LargeThpShare::ZERO,
        LargeThpShare::ZERO,
        LargeThpShare::from_raw(scale / 2),
        LargeThpShare::from_raw(scale / 2),
        LargeThpShare::from_raw(scale / 2),
        LargeThpShare::from_raw(scale / 2),
        LargeThpShare::from_raw(scale / 2),
        LargeThpShare::from_raw(scale / 2),
        LargeThpShare::from_raw(scale / 2),
        LargeThpShare::from_raw(scale / 2),
    ];
    let mut s2 = [LargeThpShare::ZERO; 24];
    for slot in s2.iter_mut().skip(4).take(20) {
        *slot = LargeThpShare::ONE;
    }

    let mut a = LargeThpEstimator::with_state(scale, scale);
    for &o in &s1 {
        a.observe(&LargeThpGradient::CAUTIOUS, o);
    }

    let mut b = LargeThpEstimator::with_state(scale, scale);
    let mut c = LargeThpEstimator::with_state(scale, scale);
    let mut d = LargeThpEstimator::with_state(scale, scale);
    for i in 0..s1.len() {
        b.observe(&LargeThpGradient::CAUTIOUS, s1[i]);
        c.observe(&LargeThpGradient::STEADY, s2[i]);
        d.observe(&LargeThpGradient::CAUTIOUS, s1[i]);
        // B's state must equal D's (same stream, no interference from C's
        // updates) exactly, after EVERY step.
        assert_eq!(b.e_raw(), d.e_raw(), "step {i}: e diverged from solo twin");
        assert_eq!(b.m_raw(), d.m_raw(), "step {i}: m diverged from solo twin");
    }

    assert_eq!(a.e_raw(), b.e_raw());
    assert_eq!(a.m_raw(), b.m_raw());

    // Determinism: re-run the whole sequence into fresh estimators and
    // require bit-identical finals.
    let mut a2 = LargeThpEstimator::with_state(scale, scale);
    for &o in &s1 {
        a2.observe(&LargeThpGradient::CAUTIOUS, o);
    }
    let mut b2 = LargeThpEstimator::with_state(scale, scale);
    let mut c2 = LargeThpEstimator::with_state(scale, scale);
    for i in 0..s1.len() {
        b2.observe(&LargeThpGradient::CAUTIOUS, s1[i]);
        c2.observe(&LargeThpGradient::STEADY, s2[i]);
    }
    assert_eq!(a.e_raw(), a2.e_raw());
    assert_eq!(a.m_raw(), a2.m_raw());
    assert_eq!(b.e_raw(), b2.e_raw());
    assert_eq!(b.m_raw(), b2.m_raw());
}

#[test]
fn update_then_report_order() {
    // Contract: observe() UPDATEs e/m first, THEN reports the share from
    // the updated state. Therefore the returned share must equal
    // bounded_share() probed immediately after the observe.
    let mut st = LargeThpEstimator::new();
    let share_before = st.bounded_share(&LargeThpGradient::CAUTIOUS);
    let s1 = st.observe(&LargeThpGradient::CAUTIOUS, LargeThpShare::ONE);
    let share_after = st.bounded_share(&LargeThpGradient::CAUTIOUS);
    assert_eq!(s1, share_after);
    // The reported share reflects the POST-update state, not the
    // pre-update one (a fresh estimator sits at SCALE, so the pre-update
    // share would also be ONE here — the discriminating case is below).
    let _ = share_before;

    // Discriminating case: from with_state(0, 0) observing ZERO leaves the
    // state at (0, 0); the returned share must equal the share computed
    // from that post-update state.
    let mut st = LargeThpEstimator::with_state(0, 0);
    let s = st.observe(&LargeThpGradient::CAUTIOUS, LargeThpShare::from_raw(0));
    assert_eq!(s, st.bounded_share(&LargeThpGradient::CAUTIOUS));
    assert_eq!(st.e_raw(), 0);
    assert_eq!(st.m_raw(), 0);

    // Discriminating order case: from e = m = 171_798_692 (0.08) the
    // PRE-update share is 0 (est 0.08 < Cautious floor 0.10), but
    // observing ONE lifts e: diff = 2_147_483_648 - 171_798_692 =
    // 1_975_684_956, rate 1/32, step = round-half-away(61_740_154.875) =
    // 61_740_155 (via the rhalf oracle above: (2*1_975_684_956 + 32)/64),
    // so e = 233_538_847 ≈ 0.109 — est above the floor — and the
    // POST-update share is (5 * 233_538_847) / 4 = 291_923_558 > 0.
    // Update-then-report must return the positive post-update share;
    // report-then-update would return 0.
    let mut st = LargeThpEstimator::with_state(171_798_692, 171_798_692);
    assert_eq!(
        st.bounded_share(&LargeThpGradient::CAUTIOUS).raw(),
        0,
        "pre-update share must be floored to 0"
    );
    let s = st.observe(&LargeThpGradient::CAUTIOUS, LargeThpShare::ONE);
    assert!(
        s.raw() > 0,
        "post-update share must reflect the lifted state"
    );
    assert_eq!(s, st.bounded_share(&LargeThpGradient::CAUTIOUS));
}
