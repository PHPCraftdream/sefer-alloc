//! `LargeThpShare` — the Q31 fixed-point share value exchanged between the
//! large-THP gradient-estimator components.
//!
//! ## Scale definition
//!
//! A `LargeThpShare` stores a rational in `[0, 1]` as a `u32` raw value in
//! Q31 fixed-point: `SCALE == 1 << 31 == 2_147_483_648` raw units represent
//! exactly `1.0`, `0` represents exactly `0.0`, and the resolution is
//! `2^-31` (one raw unit). The stored field is private and never observable
//! outside the clamp-at-construction contract: every constructor normalizes
//! its input by clamping to `[0, SCALE]`, so an out-of-range raw value can
//! never be smuggled in. Clamping (saturating semantics, not wrapping or
//! trapping) is the deliberate total-function choice — this type must not
//! panic for ANY input value.
//!
//! This module remains pure fixed-point math; the allocator uses it through
//! the separate `large_thp_runtime` adapter.

/// A share in `[0, 1]` as Q31 fixed-point (`SCALE` raw units == `1.0`).
///
/// Total by construction: any raw value is clamped into `[0, SCALE]` before
/// being stored, and every accessor is `const`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LargeThpShare(u32);

impl LargeThpShare {
    /// Raw units per `1.0` (`2^31`, Q31 scale).
    pub const SCALE: u32 = 1 << 31;
    /// The exact value `0.0`.
    pub const ZERO: Self = Self(0);
    /// The exact value `1.0` (`SCALE` raw units).
    pub const ONE: Self = Self(Self::SCALE);

    /// Construct from a raw Q31 value, clamping to `[0, SCALE]`.
    ///
    /// Total: `raw > SCALE` saturates to `ONE`; no panic for any input.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        if raw > Self::SCALE {
            Self(Self::SCALE)
        } else {
            Self(raw)
        }
    }

    /// The raw Q31 representation (guaranteed `<= SCALE`).
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}
