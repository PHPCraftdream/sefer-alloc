//! `LargeThpCadence` — pure arithmetic of the adaptive Auto control cadence
//! (`docs/perf/J13_AUTO_ADAPTIVE_CADENCE_DESIGN.md` §3): a per-group level
//! `l` in `0..=5` thins the fixed 1-in-8 candidate selector to
//! `1/(8 * 2^l)`. Safe code, no atomics, total over every input.

use crate::alloc_core::config::policy::large_thp::LargeThpShare;
use crate::alloc_core::config::policy::large_thp_observation::bounded_mix::bounded_mix;
use crate::alloc_core::config::policy::large_thp_observation::large_thp_control_selector::LargeThpControlSelector;

/// Namespace of the cadence constants and pure functions.
pub(crate) struct LargeThpCadence;

// Consumers are the runtime (under the mechanism gate) and the actual-source
// tests that mount this file; the remaining rows see only part of the surface.
#[allow(dead_code)]
impl LargeThpCadence {
    /// Highest level: rate `1/(8 * 2^5) = 1/256`.
    pub(crate) const LEVEL_MAX: u32 = 5;
    /// Consecutive agreements that raise the level by one step.
    pub(crate) const RUNS_PER_LEVEL: u32 = 4;
    /// Saturating agreement run: `LEVEL_MAX * RUNS_PER_LEVEL`.
    pub(crate) const RUN_MAX: u32 = 20;
    /// Inclusive agreement band `1/8` in Q31 raw units.
    pub(crate) const TOLERANCE_RAW: u32 = LargeThpShare::SCALE / 8;
    /// Domain separator: the live-cohort mix differs from the candidate mix.
    const THIN_DOMAIN: u64 = 0xC2B2_AE3D_27D4_EB4F;

    /// Level for an agreement run; total over `u32`.
    pub(crate) const fn level_of(run: u32) -> u32 {
        let run = if run > Self::RUN_MAX {
            Self::RUN_MAX
        } else {
            run
        };
        run / Self::RUNS_PER_LEVEL
    }

    /// Run after one valid observation: saturating increment on agreement,
    /// reset to zero on disagreement.
    pub(crate) const fn next_run(run: u32, agreed: bool) -> u32 {
        if !agreed {
            return 0;
        }
        if run >= Self::RUN_MAX {
            Self::RUN_MAX
        } else {
            run + 1
        }
    }

    /// Whether the observation `obs` agrees with the estimate `est` in force
    /// when the control was issued: `|obs - est| <= 1/8`, inclusive.
    pub(crate) const fn agrees(est: LargeThpShare, obs: LargeThpShare) -> bool {
        est.raw().abs_diff(obs.raw()) <= Self::TOLERANCE_RAW
    }

    /// Whether the candidate's cohort is the single live cohort of its
    /// aligned super-cohort of `2^level` cohorts. Level 0 is always live;
    /// levels above `LEVEL_MAX` clamp. Total over the whole `u64` ordinal
    /// domain.
    pub(crate) const fn live(seed: u64, ordinal: u64, level: u32) -> bool {
        let level = if level > Self::LEVEL_MAX {
            Self::LEVEL_MAX
        } else {
            level
        };
        if level == 0 {
            return true;
        }
        let cohort = ordinal / LargeThpControlSelector::COHORT_LEN;
        let mask = (1u64 << level) - 1;
        let domain = seed ^ Self::THIN_DOMAIN.wrapping_add(level as u64);
        (cohort & mask) == (bounded_mix(domain, cohort >> level) & mask)
    }

    /// The oracle decision: a candidate of the fixed selector that is also
    /// live at `level`. At level 0 this is exactly `decision_at`.
    pub(crate) const fn select(seed: u64, ordinal: u64, level: u32) -> bool {
        LargeThpControlSelector::decision_at(seed, ordinal) && Self::live(seed, ordinal, level)
    }
}
