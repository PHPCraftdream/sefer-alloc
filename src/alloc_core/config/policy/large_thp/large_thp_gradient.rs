//! `LargeThpGradient` — the normalized parameter set driving the large-THP
//! gradient estimator ([`LargeThpEstimator`](super::large_thp_estimator::LargeThpEstimator)).
//!
//! ## Knob semantics
//!
//! The estimator keeps a fast state `e` and a slow mean `m` (both Q31
//! shares in `[0, 1]`). A new observation `o` updates:
//! - **`down_shift`** — EMA rate `2^-down_shift`, applied to `e` when the
//!   observation is BELOW the current `e` (the falling direction).
//! - **`up_shift`** — EMA rate `2^-up_shift`, applied to `e` when the
//!   observation is at or above the current `e` (the rising direction).
//!   The asymmetric pair lets a preset fall slowly (small step) and recover
//!   quickly (large step) or vice versa.
//! - **`mean_shift`** — EMA rate `2^-mean_shift` applied to `m`, in both
//!   directions (a symmetric slow mean over observations).
//! - **`anchor_raw`** — Q31 anchor weight in `[0, 1]`; the intermediate
//!   estimate is `est = e + anchor * (m - e)`, i.e. a point between the
//!   fast state and the slow mean (`anchor_raw = 0` → pure `e`,
//!   `SCALE` → pure `m`).
//! - **`floor_raw`** — Q31 share floor: if `est` is strictly below the
//!   floor, the reported share is `0`.
//! - **`gain_num` / `gain_den`** — the multiplicative gain as an exact
//!   rational `gain_num / gain_den` (floor division after the multiply).
//!   A Q31 fixed-point constant cannot represent values greater than
//!   `1.0`, so a gain of `1.25` would be unrepresentable — hence the exact
//!   rational pair (`5/4`). Both are stored pre-normalized; `gain_den` is
//!   never `0` after construction.
//! - **`curve`** — the [`LargeThpCurve`](super::large_thp_curve::LargeThpCurve)
//!   shape applied to `est` before the gain.
//!
//! The reported share is `0` if `est < floor`, else
//! `min(1, gain * curve(est))`.
//!
//! ## Normalization rules (const, total, no panic)
//!
//! - Shifts (`down_shift`, `up_shift`, `mean_shift`) clamp to
//!   `[0, MAX_SHIFT]`.
//! - `anchor_raw` and `floor_raw` clamp to `[0, SCALE]`.
//! - `gain_den == 0` normalizes to `1`.
//! - `gain_num` is passed through UNclamped: any `u32` is safe because the
//!   downstream product bound is `u32::MAX * 2^31 < 2^63`, which fits `u64`
//!   exactly — no intermediate anywhere can overflow.
//!
//! This module remains pure fixed-point math; the allocator uses it through
//! the separate `large_thp_runtime` adapter.

use super::large_thp_curve::LargeThpCurve;
use super::large_thp_share::LargeThpShare;

/// Normalized large-THP gradient parameter set.
///
/// Construct via one of the three presets ([`CAUTIOUS`](Self::CAUTIOUS),
/// [`EAGER`](Self::EAGER), [`STEADY`](Self::STEADY)) or [`custom`](Self::custom)
/// (which normalizes every argument; no constructor can produce an
/// out-of-range field).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LargeThpGradient {
    down_shift: u32,
    up_shift: u32,
    mean_shift: u32,
    anchor_raw: u32,
    floor_raw: u32,
    gain_num: u32,
    gain_den: u32,
    curve: LargeThpCurve,
}

impl LargeThpGradient {
    /// Protocol constant: the maximum legal EMA shift (`2^-31` is the
    /// slowest representable rate in Q31 — half a raw unit per step).
    pub const MAX_SHIFT: u32 = 31;

    /// Fall fast (`2^-2` = 1/4), recover slowly (`2^-5` = 1/32), mean at
    /// `2^-5`, anchor 0 (pure fast state), floor 0.10 (Q31, rounded to
    /// nearest), gain 5/4, linear curve.
    pub const CAUTIOUS: Self = Self::custom(2, 5, 5, 0, 214_748_365, 5, 4, LargeThpCurve::Linear);

    /// Same as [`CAUTIOUS`](Self::CAUTIOUS) but down_shift 5 / up_shift 2 —
    /// slower fall (1/32), faster recovery (1/4).
    pub const EAGER: Self = Self::custom(5, 2, 5, 0, 214_748_365, 5, 4, LargeThpCurve::Linear);

    /// Balanced shifts (3/3), slower mean (`2^-6`), anchor 0.5
    /// (`1_073_741_824 = 2^30`), floor 0.10, gain 5/4, linear curve.
    pub const STEADY: Self = Self::custom(
        3,
        3,
        6,
        1_073_741_824,
        214_748_365,
        5,
        4,
        LargeThpCurve::Linear,
    );

    /// Const-normalizing constructor. Total: clamps shifts to
    /// `[0, MAX_SHIFT]`, clamps `anchor_raw`/`floor_raw` to `[0, SCALE]`,
    /// maps `gain_den == 0` to `1`, and passes `gain_num` through (see the
    /// module doc for the overflow bound justifying no clamp). No panic for
    /// any input.
    // `allow(clippy::too_many_arguments)`: the 8 arguments ARE the full
    // normalized parameter set (see module doc); a builder would add
    // indirection without removing any argument.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn custom(
        down_shift: u32,
        up_shift: u32,
        mean_shift: u32,
        anchor_raw: u32,
        floor_raw: u32,
        gain_num: u32,
        gain_den: u32,
        curve: LargeThpCurve,
    ) -> Self {
        Self {
            down_shift: Self::clamp_shift(down_shift),
            up_shift: Self::clamp_shift(up_shift),
            mean_shift: Self::clamp_shift(mean_shift),
            anchor_raw: Self::clamp_share(anchor_raw),
            floor_raw: Self::clamp_share(floor_raw),
            gain_num,
            gain_den: if gain_den == 0 { 1 } else { gain_den },
            curve,
        }
    }

    /// Clamp a shift to `[0, MAX_SHIFT]` (const, total).
    const fn clamp_shift(s: u32) -> u32 {
        if s > Self::MAX_SHIFT {
            Self::MAX_SHIFT
        } else {
            s
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

    /// EMA shift applied to the fast state when the observation is below it.
    #[must_use]
    pub const fn down_shift(self) -> u32 {
        self.down_shift
    }

    /// EMA shift applied to the fast state when the observation is at or
    /// above it.
    #[must_use]
    pub const fn up_shift(self) -> u32 {
        self.up_shift
    }

    /// EMA shift of the slow mean `m`.
    #[must_use]
    pub const fn mean_shift(self) -> u32 {
        self.mean_shift
    }

    /// Raw Q31 anchor weight in `[0, SCALE]`.
    #[must_use]
    pub const fn anchor_raw(self) -> u32 {
        self.anchor_raw
    }

    /// Raw Q31 share floor in `[0, SCALE]`.
    #[must_use]
    pub const fn floor_raw(self) -> u32 {
        self.floor_raw
    }

    /// Gain numerator (unclamped by design; see module doc).
    #[must_use]
    pub const fn gain_num(self) -> u32 {
        self.gain_num
    }

    /// Gain denominator (guaranteed `>= 1` after normalization).
    #[must_use]
    pub const fn gain_den(self) -> u32 {
        self.gain_den
    }

    /// The output shaping curve.
    #[must_use]
    pub const fn curve(self) -> LargeThpCurve {
        self.curve
    }
}
