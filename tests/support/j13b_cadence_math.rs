//! Real pure cadence dependencies mounted for actual-source tests: the Q31
//! share, the estimator, the gradient presets and the observation family
//! (bounded mix and fixed control selector).
#![allow(dead_code)]

#[path = "../../crates/core/sefer-policy/src/policy/large_thp/large_thp_curve.rs"]
pub(crate) mod large_thp_curve;
#[path = "../../crates/core/sefer-policy/src/policy/large_thp/large_thp_estimator.rs"]
mod large_thp_estimator;
#[path = "../../crates/core/sefer-policy/src/policy/large_thp/large_thp_gradient.rs"]
mod large_thp_gradient;
#[path = "../../crates/core/sefer-policy/src/policy/large_thp_observation/mod.rs"]
pub(crate) mod large_thp_observation;
#[path = "../../crates/core/sefer-policy/src/policy/large_thp/large_thp_share.rs"]
mod large_thp_share;

#[allow(unused_imports)]
pub(crate) use large_thp_estimator::LargeThpEstimator;
#[allow(unused_imports)]
pub(crate) use large_thp_gradient::LargeThpGradient;
pub(crate) use large_thp_share::LargeThpShare;

pub(crate) mod alloc_core {
    pub(crate) mod config {
        pub(crate) mod policy {
            pub(crate) mod large_thp {
                pub(crate) use super::super::super::super::LargeThpShare;
            }
        }
    }
}
