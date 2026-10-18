//! The three public configuration-policy types ([`Profile`](crate::Profile),
//! [`SmallPoolPolicy`](crate::SmallPoolPolicy), [`LargeCachePolicy`](crate::LargeCachePolicy)),
//! moved verbatim from the former single `profile.rs` per the one-file-one-export rule.

pub mod large_cache_policy;
pub mod profile;
pub mod small_pool_policy;

pub use large_cache_policy::LargeCachePolicy;
pub use profile::Profile;
pub use small_pool_policy::SmallPoolPolicy;
