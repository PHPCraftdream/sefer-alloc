//! Real pure estimator dependencies mounted for actual-source atomic tests.
#![allow(dead_code)]

#[path = "../../src/alloc_core/config/policy/large_thp/large_thp_curve.rs"]
pub(crate) mod large_thp_curve;
#[path = "../../src/alloc_core/config/policy/large_thp/large_thp_estimator.rs"]
mod large_thp_estimator;
#[path = "../../src/alloc_core/config/policy/large_thp/large_thp_gradient.rs"]
mod large_thp_gradient;
#[path = "../../src/alloc_core/config/policy/large_thp/large_thp_share.rs"]
mod large_thp_share;

pub(crate) use large_thp_estimator::LargeThpEstimator;
pub(crate) use large_thp_gradient::LargeThpGradient;
pub(crate) use large_thp_share::LargeThpShare;

pub(crate) mod alloc_core {
    pub(crate) mod config {
        pub(crate) mod policy {
            pub(crate) mod large_thp {
                pub(crate) use super::super::super::super::{
                    LargeThpEstimator, LargeThpGradient, LargeThpShare,
                };
            }
        }
    }
}
