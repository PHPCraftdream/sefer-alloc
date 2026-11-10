//! `ThpTuple` — the resolved large-THP configuration (mode, minimum block,
//! advised-byte budget, gradient) and its exact 5-word packed form.
//!
//! Packed layout (`pack`/`try_unpack`):
//! - `w0` = `FORMAT_TAG` (bits 56..64, never zero) | mode (bits 0..2) |
//!   curve (2..4) | down shift (4..9) | up shift (9..14) | mean shift (14..19)
//! - `w1` = anchor (high 32) | floor (low 32)
//! - `w2` = gain numerator (high 32) | gain denominator (low 32)
//! - `w3` = minimum block bytes, `w4` = budget bytes (0 = unlimited)
//!
//! Decoding rebuilds the gradient through the normalising
//! [`LargeThpGradient::custom`], so a decoded tuple is always valid.

use crate::alloc_core::config::policy::large_thp::{LargeThpCurve, LargeThpGradient, LargeThpMode};

/// Resolved large-THP tuple; see the module doc for the packed layout.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ThpTuple {
    /// Advice policy.
    pub mode: LargeThpMode,
    /// Smallest fresh Large block (bytes) eligible for advice; `0` makes every
    /// fresh Large block eligible.
    pub min_bytes: usize,
    /// Bound on ADVISED bytes (not RSS); `0` = unlimited.
    pub budget_bytes: usize,
    /// Learning parameters used by `Auto`.
    pub gradient: LargeThpGradient,
}

// Live under the decommit non-NUMA mechanism gate (publication cell, config
// resolvers, fresh-path budget) and behind the internals dbg_resolved_thp_tuple;
// the attribute keeps every other feature row warning-clean.
#[cfg_attr(
    not(any(feature = "alloc-decommit", feature = "internals")),
    allow(dead_code)
)]
impl ThpTuple {
    /// Format tag stored in the top byte of word 0; nonzero so a packed tuple
    /// is never an all-zero blank.
    pub const FORMAT_TAG: u64 = 0xA7;

    /// Default minimum block: 8 MiB.
    pub const DEFAULT_MIN_BYTES: usize = 8 * 1024 * 1024;

    /// `(Off, 8 MiB, 0 = unlimited, CAUTIOUS)`: today's behaviour.
    pub const DEFAULT: Self = Self {
        mode: LargeThpMode::Off,
        min_bytes: Self::DEFAULT_MIN_BYTES,
        budget_bytes: 0,
        gradient: LargeThpGradient::CAUTIOUS,
    };

    const TAG_SHIFT: u32 = 56;

    const fn mode_code(mode: LargeThpMode) -> u64 {
        match mode {
            LargeThpMode::Off => 0,
            LargeThpMode::Always => 1,
            LargeThpMode::Auto => 2,
        }
    }

    const fn curve_code(curve: LargeThpCurve) -> u64 {
        match curve {
            LargeThpCurve::Generous => 0,
            LargeThpCurve::Linear => 1,
            LargeThpCurve::Strict => 2,
        }
    }

    const fn mode_from(code: u64) -> Option<LargeThpMode> {
        match code {
            0 => Some(LargeThpMode::Off),
            1 => Some(LargeThpMode::Always),
            2 => Some(LargeThpMode::Auto),
            _ => None,
        }
    }

    const fn curve_from(code: u64) -> Option<LargeThpCurve> {
        match code {
            0 => Some(LargeThpCurve::Generous),
            1 => Some(LargeThpCurve::Linear),
            2 => Some(LargeThpCurve::Strict),
            _ => None,
        }
    }

    /// Pack into the five-word form (exact for every valid tuple).
    #[must_use]
    pub const fn pack(&self) -> [u64; 5] {
        let g = self.gradient;
        let w0 = (Self::FORMAT_TAG << Self::TAG_SHIFT)
            | Self::mode_code(self.mode)
            | (Self::curve_code(g.curve()) << 2)
            | ((g.down_shift() as u64) << 4)
            | ((g.up_shift() as u64) << 9)
            | ((g.mean_shift() as u64) << 14);
        let w1 = ((g.anchor_raw() as u64) << 32) | g.floor_raw() as u64;
        let w2 = ((g.gain_num() as u64) << 32) | g.gain_den() as u64;
        [w0, w1, w2, self.min_bytes as u64, self.budget_bytes as u64]
    }

    /// Decode a packed tuple; `None` for an unknown format tag, mode or curve.
    /// Min/budget saturate to `usize::MAX` on targets narrower than 64 bits.
    #[must_use]
    pub const fn try_unpack(w: [u64; 5]) -> Option<Self> {
        let w0 = w[0];
        if (w0 >> Self::TAG_SHIFT) != Self::FORMAT_TAG {
            return None;
        }
        let mode = match Self::mode_from(w0 & 0b11) {
            Some(m) => m,
            None => return None,
        };
        let curve = match Self::curve_from((w0 >> 2) & 0b11) {
            Some(c) => c,
            None => return None,
        };
        let gradient = LargeThpGradient::custom(
            ((w0 >> 4) & 0x1F) as u32,
            ((w0 >> 9) & 0x1F) as u32,
            ((w0 >> 14) & 0x1F) as u32,
            (w[1] >> 32) as u32,
            w[1] as u32,
            (w[2] >> 32) as u32,
            w[2] as u32,
            curve,
        );
        Some(Self {
            mode,
            min_bytes: Self::to_usize(w[3]),
            budget_bytes: Self::to_usize(w[4]),
            gradient,
        })
    }

    /// Decode a packed tuple, returning [`DEFAULT`](Self::DEFAULT) when
    /// [`try_unpack`](Self::try_unpack) rejects it.
    #[must_use]
    pub const fn unpack(w: [u64; 5]) -> Self {
        match Self::try_unpack(w) {
            Some(t) => t,
            None => Self::DEFAULT,
        }
    }

    const fn to_usize(v: u64) -> usize {
        if v > usize::MAX as u64 {
            usize::MAX
        } else {
            v as usize
        }
    }
}
