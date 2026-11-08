//! `LargeThpCurve` — the closed-form shape applied to the gradient
//! estimator's intermediate estimate before it is scaled into a share.
//!
//! Q31 → Q31: each variant maps an input in `[0, SCALE]` (raw Q31) to an
//! output in the same range, integers only, total for every input (the
//! input is clamped to `[0, SCALE]` first).
//!
//! ## Design limitation (deliberate, stated as contract)
//!
//! The curve family is EXACTLY these three closed forms — floor-sqrt
//! (`Generous`), identity (`Linear`), floor-square (`Strict`) — at Q31. It
//! is deliberately NOT an arbitrary real exponent curve (a gamma-shaped
//! `pow(k, r)` family): real-exponent curves need floating-point `pow` /
//! `log`, which this `core`-only module bans. Any new curve shape must be
//! added as an explicit new enum variant with its own bounded integer
//! implementation and its own documented error bound.
//!
//! This module remains pure fixed-point math; the allocator uses it through
//! the separate `large_thp_runtime` adapter.

use super::large_thp_share::LargeThpShare;

/// The estimator's output shaping curve (Q31 → Q31, integers only).
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum LargeThpCurve {
    /// Concave, sqrt-shaped: `isqrt(x << 31)` (floor integer square root).
    /// Boosts small estimates (a small estimate maps to a comparatively
    /// larger share) — the generous end of the family.
    Generous,
    /// The identity: output equals the (clamped) input exactly, error 0.
    Linear,
    /// Convex, square-shaped: `(x * x) >> 31` (exact truncation). Suppresses
    /// small estimates — the strict end of the family.
    Strict,
}

impl LargeThpCurve {
    /// Floor integer square root of a `u64`, const-compatible halving
    /// binary search (no floats). Total for every input.
    const fn isqrt_u64(v: u64) -> u64 {
        // Search floor(sqrt(v)) over [0, 2^32]. Any u64 v < 2^64 has
        // floor(sqrt(v)) < 2^32, so this bracket is exact. `mid` rounds
        // UP within the remaining range; each step either raises `lo` to
        // a proven-feasible value or lowers `hi` just below a proven-
        // infeasible one, and the loop terminates because the gap halves.
        let mut lo: u64 = 0;
        let mut hi: u64 = if v < 2 { v } else { 1 << 32 };
        while lo < hi {
            let mid: u64 = lo + ((hi - lo + 1) >> 1);
            if mid <= v / mid {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        lo
    }

    /// Evaluate the curve at `est_raw` (raw Q31), returning raw Q31.
    ///
    /// Total: the input is clamped to `[0, SCALE]` first, so every `u32`
    /// input is accepted without panic. Error per variant:
    /// - `Linear`: exact, error 0.
    /// - `Generous`: floor-sqrt of an exact scaled value — truncation error
    ///   strictly less than 1 ulp (`2^-31` of full scale). `eval(0) == 0`
    ///   and `eval(SCALE) == SCALE` exactly.
    /// - `Strict`: `(x*x) >> 31` exact truncation — error strictly less
    ///   than 1 ulp. `eval(SCALE) == SCALE` exactly.
    #[must_use]
    pub const fn eval(&self, est_raw: u32) -> u32 {
        // Clamp (total, defensive): inputs above SCALE saturate to SCALE.
        let x: u64 = if est_raw > LargeThpShare::SCALE {
            LargeThpShare::SCALE as u64
        } else {
            est_raw as u64
        };
        let y: u64 = match *self {
            LargeThpCurve::Linear => x,
            // Max input x = 2^31, so x << 31 <= 2^62 — fits u64 exactly.
            LargeThpCurve::Generous => Self::isqrt_u64(x << 31),
            // x <= 2^31, so x*x <= 2^62 — fits u64; exact truncation.
            LargeThpCurve::Strict => (x * x) >> 31,
        };
        // y <= SCALE for every variant: Linear is already clamped to
        // [0, SCALE]; Generous: isqrt(x << 31) <= isqrt(SCALE << 31) =
        // SCALE; Strict: (x*x) >> 31 <= (SCALE*SCALE) >> 31 = SCALE —
        // so this cast is in range.
        if y > LargeThpShare::SCALE as u64 {
            LargeThpShare::SCALE
        } else {
            y as u32
        }
    }
}
