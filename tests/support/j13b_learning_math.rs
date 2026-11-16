//! Real pure estimator dependencies mounted for actual-source atomic tests.
//! (P1.3: the legacy `alloc_core::config::policy` path tree is gone — the moved
//! sources resolve imports at the sefer-policy crate root; the consuming tests
//! alias them under `crate::policy`.)
#![allow(dead_code)]

#[path = "../../crates/core/sefer-policy/src/policy/large_thp/large_thp_curve.rs"]
pub(crate) mod large_thp_curve;
#[path = "../../crates/core/sefer-policy/src/policy/large_thp/large_thp_estimator.rs"]
mod large_thp_estimator;
#[path = "../../crates/core/sefer-policy/src/policy/large_thp/large_thp_gradient.rs"]
mod large_thp_gradient;
#[path = "../../crates/core/sefer-policy/src/policy/large_thp/large_thp_share.rs"]
mod large_thp_share;

pub(crate) use large_thp_estimator::LargeThpEstimator;
pub(crate) use large_thp_gradient::LargeThpGradient;
pub(crate) use large_thp_share::LargeThpShare;
