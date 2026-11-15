//! Legacy/research concurrent tier (Phase 3b / 7a / 7b / 7c), extracted from
//! sefer-alloc (workspace-restructure plan step P1.2,
//! `docs/plans/2026-11-15-workspace-restructure-plan.md`).
//!
//! # Status: legacy research-tier
//!
//! The concurrent region family ([`EpochRegion`], [`LockFreeRegion`],
//! [`ShardedRegion`]) is legacy: superseded by the production `alloc-xthread`
//! cross-thread free path for the allocator face. The types are kept for
//! backward compatibility and as a research baseline, but no new development
//! is planned on them. [`PinnedRunner`] (under the `pinning` feature) remains
//! useful on its own as a thin safe wrapper over `core_affinity` for
//! thread-per-core dispatch and is NOT deprecated.
//!
//! sefer-alloc re-exports the seven public types under its `experimental`
//! feature (via the `src/concurrent` shim module), so the public paths
//! `sefer_alloc::{EpochHandle, EpochRegion, LockFreeHandle, LockFreeRegion,
//! ShardedHandle, ShardedRegion}` and `sefer_alloc::PinnedRunner` keep
//! resolving unchanged.

#![warn(missing_docs)]
#![deny(unsafe_code)]

// The concurrent tier uses AtomicU64; pointer-width atomics alone do not
// suffice. Mirrors the root crate's own guard for its `experimental` feature.
#[cfg(not(target_has_atomic = "64"))]
compile_error!(
    "sefer-concurrent requires a target with 64-bit atomics \
     (target_has_atomic = \"64\"): the epoch, lock-free and sharded tiers \
     store generations and lengths in AtomicU64. Choose a target with this \
     capability."
);

pub(crate) mod epoch;
pub(crate) mod lock_free;
pub(crate) mod sharded;

#[cfg(feature = "pinning")]
mod pinning;

// `#[cfg(kani)]` proof harnesses in `src/` are the established pattern for
// proofs that need `pub(crate)` internals (here: `crate::epoch::hand::AtomicSlot`)
// invisible from `tests/` — moved here from sefer-alloc's `src/kani_proofs.rs`
// in P1.2 because this crate owns the internals the harness needs. loom covers
// a handwritten shadow publication model, not the real `AtomicSlot` or its
// reclamation; these two proofs check vacant-slot facts only.
#[cfg(kani)]
mod kani_proofs;

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
