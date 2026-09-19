//! Concurrent tier (Phase 3b).
//!
//! Re-exports only — no logic lives here (per the one-export-per-file rule).
//!
//! # Status: legacy research-tier
//!
//! The concurrent region family ([`EpochRegion`], [`LockFreeRegion`],
//! [`ShardedRegion`]) is now legacy: it is superseded by the production
//! `alloc-xthread` cross-thread free path for the allocator face. These types
//! are kept under the existing `experimental` feature for backward compatibility
//! and as a research baseline, but no new development is planned on them.
//!
//! [`PinnedRunner`] (under the `pinning` feature) remains useful on its own as a
//! thin safe wrapper over `core_affinity` for thread-per-core dispatch and is
//! NOT deprecated.

pub(crate) mod epoch;
pub(crate) mod lock_free;
pub(crate) mod sharded;

#[cfg(feature = "pinning")]
mod pinning;

#[allow(deprecated)]
pub use epoch::epoch_handle::EpochHandle;
#[allow(deprecated)]
pub use epoch::epoch_region::EpochRegion;
#[allow(deprecated)]
pub use lock_free::lock_free_handle::LockFreeHandle;
#[allow(deprecated)]
pub use lock_free::lock_free_region::LockFreeRegion;
#[allow(deprecated)]
pub use sharded::sharded_handle::ShardedHandle;
#[allow(deprecated)]
pub use sharded::sharded_region::ShardedRegion;

#[cfg(feature = "pinning")]
pub use pinning::PinnedRunner;
