//! J13b: integration tests for the large-THP gradient recovery dynamics —
//! multi-step ascent/descent trajectories of `LargeThpEstimator` checked
//! against independent analytic geometric-decay laws.
//!
//! ## Analytic table (integer/ratio form; no floats anywhere)
//!
//! Ascent law: `e_n = 1 - (1 - e_0) * (1 - 2^-up_shift)^n` (rate `2^-up_shift`).
//! Descent law: `e_n = e_0 * (1 - 2^-down_shift)^n` — both ignoring the
//! `<= 0.5` ulp per-step round-half-away quantization.
//!
//! - CAUTIOUS (up rate 1/32) from `e_0 = 0.08` (raw 171_798_692):
//!   n = 12 gives `e_12 = 1 - 0.92 * (31/32)^12 ≈ 0.371` — NOT 0.97.
//!   `e >= 0.97` needs `n = ceil(ln(0.03/0.92) / ln(32/31)) = 108`;
//!   `e >= 0.80` (where the gain-5/4 linear share saturates) needs 49.
//! - EAGER (up rate 1/4) from 0.08: `e_12 = 1 - 0.92 * (3/4)^12 ≈ 0.9709`,
//!   so `e >= 0.97` needs exactly 12; `e >= 0.80` needs 6.
//! - STEADY (up rate 1/8) from 0.08 (e only; anchor 2^30 additionally
//!   mixes the slower mean): `e >= 0.97` needs 26; `e >= 0.80` needs 12.
//! - Mean EMA, mean_shift 5 (rate 1/32) mirrors the Cautious-ascent
//!   counts; mean_shift 6 (rate 1/64): `e >= 0.97` needs 218, `>= 0.80`
//!   needs 97.
//! - Descent, CAUTIOUS (down rate 1/4) from 1.0:
//!   `e <= 0.08` needs `n = 9` because `(3/4)^9 ≈ 0.0751 <= 0.08`
//!   (n = 8 gives (3/4)^8 ≈ 0.1001 > 0.08).
#![cfg(feature = "alloc-decommit")]

use sefer_alloc::{LargeThpCurve, LargeThpEstimator, LargeThpGradient, LargeThpShare};

/// Shorthand for the Q31 scale (2^31 raw units == 1.0).
const SCALE: u32 = LargeThpShare::SCALE;

/// `floor(0.97 * 2^31) = floor(2_083_037_138.56) = 2_083_037_138`.
const P_097: u32 = 2_083_037_138;

/// Cautious start `e_0 = 0.08` in Q31: `0.08 * 2^31 = 171_798_691.84`,
/// rounded to nearest = 171_798_692.
const E0_RAW: u32 = 171_798_692;

#[test]
fn cautious_12_obs_matches_analytic_not_097() {
    // Reference from the closed-form ascent law, in exact integer form:
    // e_12 = SCALE - (SCALE - e_0) * 31^12 / 32^12 (truncating division).
    // 32^12 = (2^5)^12 = 2^60 and 31^12 < 32^12, so the product
    // (SCALE - e_0) * 31^12 < 2^31 * 2^60 = 2^91 — fits u128 comfortably.
    let pow31_12: u128 = 31u128.pow(12);
    let pow32_12: u128 = 32u128.pow(12);
    let ref_raw = SCALE - (((SCALE - E0_RAW) as u128 * pow31_12) / pow32_12) as u32;

    let mut st = LargeThpEstimator::with_state(E0_RAW, E0_RAW);
    for _ in 0..12 {
        st.observe(&LargeThpGradient::CAUTIOUS, LargeThpShare::ONE);
    }
    let e = st.e_raw();
    // Quantization band: each step rounds half-away with error <= 0.5 ulp;
    // accumulated error <= 0.5 * (1 - (31/32)^12) * 32 ≈ 5.1 ulp (each
    // early error decays geometrically), plus <= 1 ulp from the reference
    // truncation -> 16 units is a safe margin.
    let diff = i64::from(e) - i64::from(ref_raw);
    assert!(diff.abs() <= 16, "e {e} vs analytic {ref_raw}");

    // Anti-myth: 12 observations at rate 1/32 recover only to ≈0.37
    // (0.371 * SCALE ≈ 794_668_910 << 0.9 * SCALE). A "12 obs -> 0.97"
    // claim is only valid at Eager's 1/4 rate — see the table above.
    // Widened to u64: 9*SCALE and 35*SCALE overflow u32 (2^31 scale).
    assert!(
        u64::from(e) < 9 * u64::from(SCALE) / 10,
        "12 Cautious observations must NOT reach 0.9, got {e}"
    );
}

#[test]
fn cautious_ascends_to_0_97_within_120_obs() {
    // Analytic requirement: 108 observations (see the table); cap 120 =
    // 108 + 12 margin. No stall risk: at the target SCALE - e ≈ 64M units,
    // far above the ~16-unit quantization stall bound.
    let mut st = LargeThpEstimator::with_state(E0_RAW, E0_RAW);
    let mut reached = false;
    for i in 1..=120u32 {
        let prev = st.e_raw();
        st.observe(&LargeThpGradient::CAUTIOUS, LargeThpShare::ONE);
        // Non-decreasing: diff >= 0 => round-half-away step >= 0.
        assert!(st.e_raw() >= prev, "step {i}: e decreased");
        assert!(st.e_raw() <= SCALE);
        if st.e_raw() >= P_097 {
            reached = true;
            break;
        }
    }
    assert!(reached, "e never reached {P_097} within 120 observations");
    // The slow mean (mean_shift 5, same 1/32 rate) must have kept pace.
    assert!(st.m_raw() >= P_097);
}

#[test]
fn eager_12_obs_reaches_0_97() {
    // Analytic: e_12 = 1 - 0.92 * (3/4)^12 ≈ 0.9709 >= 0.97. The 12-obs
    // claim is TRUE at rate 1/4 and FALSE at Cautious's 1/32 (see
    // `cautious_12_obs_matches_analytic_not_097`).
    let mut st = LargeThpEstimator::with_state(E0_RAW, E0_RAW);
    for _ in 0..12 {
        st.observe(&LargeThpGradient::EAGER, LargeThpShare::ONE);
    }
    assert!(st.e_raw() >= P_097, "e_12 = {}", st.e_raw());
    // Share saturates once e >= 0.8 (gain 5/4, linear), which happens by
    // observation 6 per the table — so certainly after 12.
    assert_eq!(
        st.bounded_share(&LargeThpGradient::EAGER),
        LargeThpShare::ONE
    );
}

#[test]
fn cautious_falls_to_0_08_in_9_obs_rises_slower() {
    // (a) Descent from 1.0 at rate 1/4: reference from the closed-form
    // descent law e_n = SCALE * (1 - 2^-down_shift)^n; down_shift 2 gives
    // (3/4)^9 = 19683/262144 exactly (3^9 = 19683, 4^9 = 2^18), so
    // ref ≈ 161_218_176 = 0.0751 * SCALE (truncating division; SCALE *
    // 19683 < 2^63 — fits u128).
    let ref_down = ((SCALE as u128 * 19683u128) / 262_144u128) as u32;
    let mut st = LargeThpEstimator::new();
    for _ in 0..9 {
        st.observe(&LargeThpGradient::CAUTIOUS, LargeThpShare::ZERO);
    }
    // Analytic descent law: e_9 = (3/4)^9 ≈ 0.0751 <= 0.08, so the
    // E0_RAW bound holds.
    assert!(st.e_raw() <= E0_RAW, "e_9 = {}", st.e_raw());
    let diff = i64::from(st.e_raw()) - i64::from(ref_down);
    // 16-unit band: quantization at rate 1/4 is <= 0.5 ulp *
    // (1 - (3/4)^9) / (1/4) ≈ 1.85 ulp, plus <= 1 ulp reference
    // truncation — 16 is a safe margin.
    assert!(
        diff.abs() <= 16,
        "e_9 {} vs analytic {ref_down}",
        st.e_raw()
    );

    // (b) Asymmetry: rising from 0.08 for the SAME 9 observations at the
    // Cautious UP rate 1/32 reaches only 1 - 0.92 * (31/32)^9 ≈ 0.309,
    // strictly below 0.35.
    let mut st = LargeThpEstimator::with_state(E0_RAW, E0_RAW);
    for _ in 0..9 {
        st.observe(&LargeThpGradient::CAUTIOUS, LargeThpShare::ONE);
    }
    // Widened to u64: 9*SCALE and 35*SCALE overflow u32 (2^31 scale).
    assert!(
        u64::from(st.e_raw()) < 35 * u64::from(SCALE) / 100,
        "9 rising observations must stay below 0.35, got {}",
        st.e_raw()
    );
}

#[test]
fn dominance_eager_ge_cautious_identical_stream() {
    // Dominance precondition set (proven for exactly these two presets):
    // identical mean/anchor/floor/gain/curve means the share is monotone
    // in e alone; Eager's up rate 1/4 >= Cautious's 1/32 and Eager's down
    // rate 1/32 <= Cautious's 1/4 give e_Eager >= e_Cautious at every
    // step by induction, since round-half-away is monotone in |diff|.
    // Do NOT generalize to arbitrary custom params (anchor > 0 or other
    // shifts can break pointwise dominance).
    assert_eq!(LargeThpGradient::CAUTIOUS.anchor_raw(), 0);
    assert_eq!(LargeThpGradient::EAGER.anchor_raw(), 0);
    assert_eq!(
        LargeThpGradient::CAUTIOUS.curve(),
        LargeThpGradient::EAGER.curve()
    );
    assert_eq!(
        LargeThpGradient::CAUTIOUS.gain_num(),
        LargeThpGradient::EAGER.gain_num()
    );
    assert_eq!(
        LargeThpGradient::CAUTIOUS.gain_den(),
        LargeThpGradient::EAGER.gain_den()
    );
    assert_eq!(
        LargeThpGradient::CAUTIOUS.floor_raw(),
        LargeThpGradient::EAGER.floor_raw()
    );
    assert_eq!(
        LargeThpGradient::CAUTIOUS.mean_shift(),
        LargeThpGradient::EAGER.mean_shift()
    );

    // Deterministic 272-step stream: rise, fall, rise, hold, fall, rise.
    let mut stream: Vec<LargeThpShare> = Vec::new();
    for _ in 0..64 {
        stream.push(LargeThpShare::ONE);
    }
    for _ in 0..32 {
        stream.push(LargeThpShare::ZERO);
    }
    for _ in 0..64 {
        stream.push(LargeThpShare::ONE);
    }
    for _ in 0..16 {
        stream.push(LargeThpShare::from_raw(SCALE / 2));
    }
    for _ in 0..32 {
        stream.push(LargeThpShare::ZERO);
    }
    for _ in 0..64 {
        stream.push(LargeThpShare::ONE);
    }

    let mut eager = LargeThpEstimator::with_state(SCALE, SCALE);
    let mut cautious = LargeThpEstimator::with_state(SCALE, SCALE);
    for (i, &o) in stream.iter().enumerate() {
        let se = eager.observe(&LargeThpGradient::EAGER, o);
        let sc = cautious.observe(&LargeThpGradient::CAUTIOUS, o);
        assert!(
            se.raw() >= sc.raw(),
            "step {i}: eager share {} < cautious share {}",
            se.raw(),
            sc.raw()
        );
        assert!(
            eager.e_raw() >= cautious.e_raw(),
            "step {i}: eager e {} < cautious e {}",
            eager.e_raw(),
            cautious.e_raw()
        );
        assert_eq!(eager.m_raw(), cautious.m_raw(), "step {i}: means diverged");
    }
}

#[test]
fn mean_ema_isolated_matches_geometric_law() {
    // Shifts 31/31 on e move it at most 1 ulp per step, isolating the mean
    // trajectory for direct comparison with the ascent law.
    let g = LargeThpGradient::custom(31, 31, 5, 0, 0, 1, 1, LargeThpCurve::Linear);
    let mut st = LargeThpEstimator::with_state(0, E0_RAW);
    for _ in 0..12 {
        st.observe(&g, LargeThpShare::ONE);
    }
    // Side-guard: rate 2^-31 of any diff <= 2^31 rounds to <= 1 ulp/step,
    // so 12 steps move e by at most 12 raw units.
    assert!(st.e_raw() <= 12, "e crawled to {}", st.e_raw());

    // Same closed-form reference as test 1: m_12 = SCALE - (SCALE - e_0) *
    // 31^12 / 32^12; products fit u128 (bounded by 2^31 * 2^60 = 2^91).
    let ref_raw = SCALE - (((SCALE - E0_RAW) as u128 * 31u128.pow(12)) / 32u128.pow(12)) as u32;
    let diff = i64::from(st.m_raw()) - i64::from(ref_raw);
    assert!(diff.abs() <= 16, "m {} vs analytic {ref_raw}", st.m_raw());
}
