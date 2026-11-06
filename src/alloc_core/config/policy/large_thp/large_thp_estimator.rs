//! `LargeThpEstimator` — the two-state (fast `e`, slow mean `m`)
//! asymmetric-EMA gradient estimator over Q31 shares, driven by a
//! [`LargeThpGradient`](super::large_thp_gradient::LargeThpGradient)
//! parameter set.
//!
//! Both state components start at `SCALE` (i.e. `1.0` — the estimator is
//! born fully optimistic). The estimate is `est = e + anchor * (m - e)`,
//! and the reported share applies the floor, curve and gain on top of it.
//!
//! ## Update-then-report order
//!
//! [`observe`](LargeThpEstimator::observe) UPDATEs both state components from the
//! observation FIRST, then computes and returns the share from the UPDATED
//! state. [`bounded_share`](LargeThpEstimator::bounded_share) reports from the current
//! state without any update.
//!
//! ## Error bounds
//!
//! - Per-step quantization error of the EMA step (round-half-away-from-
//!   zero at `2^shift`) is at most `0.5` raw unit = `0.5` ulp.
//! - Accumulated EMA error bound: at most `2^(shift-32)` of full scale —
//!   i.e. `<= 2^-26` of full scale for all three presets, whose maximum
//!   shift is `6`.
//! - The intermediate `est` rounding is round-half-up at `SCALE`, so its
//!   quantization error is `<= 0.5` ulp.
//!
//! All arithmetic uses widened `u64`/`i64` intermediates with documented
//! bounds and saturating clamps; every function is total (no panic for any
//! input value).
//!
//! This module is a standalone fixed-point math component; runtime
//! integration into the allocator is a separate future task.

use super::large_thp_gradient::LargeThpGradient;
use super::large_thp_share::LargeThpShare;

/// Two-state asymmetric-EMA gradient estimator over Q31 shares.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LargeThpEstimator {
    e: u32,
    m: u32,
}

impl LargeThpEstimator {
    /// Fresh estimator: both `e` and `m` at `SCALE` (1.0).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            e: LargeThpShare::SCALE,
            m: LargeThpShare::SCALE,
        }
    }

    /// Restore from raw state, clamping each component to `[0, SCALE]`
    /// (deterministic tests and future state restore). Total.
    #[must_use]
    pub const fn with_state(e_raw: u32, m_raw: u32) -> Self {
        Self {
            e: Self::clamp_share(e_raw),
            m: Self::clamp_share(m_raw),
        }
    }

    /// Raw Q31 fast state `e` (guaranteed `<= SCALE`).
    #[must_use]
    pub const fn e_raw(&self) -> u32 {
        self.e
    }

    /// Raw Q31 slow mean `m` (guaranteed `<= SCALE`).
    #[must_use]
    pub const fn m_raw(&self) -> u32 {
        self.m
    }

    /// UPDATE-THEN-REPORT: first update `e` and `m` from the observation
    /// `o`, then compute and return the share from the UPDATED state.
    pub fn observe(&mut self, g: &LargeThpGradient, o: LargeThpShare) -> LargeThpShare {
        let o_raw = o.raw();
        // EMA step on e: diff_e in i64; |o_raw - e| <= SCALE = 2^31.
        let diff_e: i64 = o_raw as i64 - self.e as i64;
        let shift_e: u32 = if diff_e < 0 {
            g.down_shift()
        } else {
            g.up_shift()
        };
        // Step never overshoots past the observation
        // (|step| <= |diff|), so the clamp below is defensive only.
        self.e = Self::clamp_u32(self.e as i64 + Self::ema_step(diff_e, shift_e));
        // EMA step on m, symmetric rate.
        let diff_m: i64 = o_raw as i64 - self.m as i64;
        self.m = Self::clamp_u32(self.m as i64 + Self::ema_step(diff_m, g.mean_shift()));
        self.bounded_share(g)
    }

    /// The share from the CURRENT state, without any state update.
    #[must_use]
    pub fn bounded_share(&self, g: &LargeThpGradient) -> LargeThpShare {
        let est_raw = self.estimate_raw(g);
        if est_raw < g.floor_raw() {
            return LargeThpShare::ZERO;
        }
        let shaped = g.curve().eval(est_raw);
        // Product bound: gain_num <= u32::MAX and shaped <= 2^31, so
        // gain_num * shaped < 2^63 — fits u64 exactly, no overflow.
        // gain_den >= 1 by gradient normalization — no div-by-zero.
        // Floor division; truncation error < 1 ulp. Clamp to SCALE in u64
        // BEFORE narrowing (raw division result can exceed u32 range).
        let share = (g.gain_num() as u64 * shaped as u64) / g.gain_den() as u64;
        let share = if share > LargeThpShare::SCALE as u64 {
            LargeThpShare::SCALE
        } else {
            share as u32
        };
        LargeThpShare::from_raw(share)
    }

    /// The intermediate estimate `est = e + anchor * (m - e)`, clamped to
    /// `[0, SCALE]`, without any state update.
    #[must_use]
    pub fn estimate(&self, g: &LargeThpGradient) -> LargeThpShare {
        LargeThpShare::from_raw(self.estimate_raw(g))
    }

    /// Raw Q31 estimate (same arithmetic as [`estimate`](Self::estimate),
    /// already clamped, without re-clamping on the way out).
    fn estimate_raw(&self, g: &LargeThpGradient) -> u32 {
        // d in i64: |d| <= 2^31 (both operands in [0, 2^31]).
        let d: i64 = self.m as i64 - self.e as i64;
        // mag bound: |d| <= 2^31 and anchor_raw <= 2^31, so
        // |d| * anchor_raw <= 2^62 — fits u64 exactly.
        let mag: u64 = (d.unsigned_abs()) * (g.anchor_raw() as u64);
        // Round-half-up of mag / SCALE: mag + 2^30 <= 2^62 + 2^30 — fits
        // u64. Quantization error <= 0.5 ulp.
        let q: i64 = ((mag + (LargeThpShare::SCALE as u64 / 2)) >> 31) as i64;
        let q = if d < 0 { -q } else { q };
        Self::clamp_u32(self.e as i64 + q)
    }

    /// One EMA step: the round-half-away-from-zero quotient
    /// `diff / 2^shift`, as i64. Bounds: `|diff| <= 2^31`; defensively
    /// `shift = min(param, 31)` so `half = 2^(shift-1) <= 2^30` and
    /// `|diff| + half <= 2^31 + 2^30` — fits i64. `|diff| == 0` returns 0;
    /// `shift == 0` returns `diff` exactly (rate 1.0). The step never
    /// overshoots past the observation:
    /// `round(|diff| / 2^shift) <= |diff|` for every `shift >= 0`.
    const fn ema_step(diff: i64, shift: u32) -> i64 {
        let shift: u32 = if shift > 31 { 31 } else { shift };
        if diff == 0 {
            return 0;
        }
        if shift == 0 {
            return diff;
        }
        let half: i64 = 1i64 << (shift - 1);
        if diff > 0 {
            (diff + half) >> shift
        } else {
            -(((-diff) + half) >> shift)
        }
    }

    /// Saturating clamp of an i64 state value into `[0, SCALE]`.
    ///
    /// Defensive only: the EMA step cannot overshoot past the observation,
    /// so a value already in `[0, SCALE]` stays there — this guard exists
    /// so totalness does not depend on that argument.
    const fn clamp_u32(v: i64) -> u32 {
        if v < 0 {
            0
        } else if v > LargeThpShare::SCALE as i64 {
            LargeThpShare::SCALE
        } else {
            v as u32
        }
    }

    /// Clamp a raw Q31 value to `[0, SCALE]` (const, total).
    const fn clamp_share(v: u32) -> u32 {
        if v > LargeThpShare::SCALE {
            LargeThpShare::SCALE
        } else {
            v
        }
    }
}

impl Default for LargeThpEstimator {
    fn default() -> Self {
        Self::new()
    }
}
