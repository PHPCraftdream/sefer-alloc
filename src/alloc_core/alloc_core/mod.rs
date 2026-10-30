//! [`AllocCore`] — the single-threaded allocator over the self-hosted segment
//! substrate (Phase 8, `alloc-core` feature).
//!
//! This is the **Cartographer** of the segment substrate: all placement logic
//! (which size class, which page, free-list pop/push, large/huge routing) is
//! **pure safe integer arithmetic** over segment-relative offsets and
//! size-class indices. Every raw memory touch is delegated to the [`node`](crate::alloc_core::platform::node)
//! seam; every OS reservation to the [`os`](crate::alloc_core::platform::os) seam. `AllocCore` itself
//! contains NO `unsafe` and NO `Vec`/`Box`/`HashSet`/`std::alloc` — the alloc
//! path is therefore **reentrancy-free (M5)**: it cannot recurse into the
//! global allocator because it allocates no metadata through it.
//!
//! ## API
//!
//! - [`AllocCore::new`] — bootstrap the primordial segment (the ONLY place
//!   that hand-carves self-hosted metadata; see [`bootstrap`]).
//! - [`alloc`](AllocCore::alloc) / [`dealloc`](AllocCore::dealloc) /
//!   [`realloc`](AllocCore::realloc) / [`alloc_zeroed`](AllocCore::alloc_zeroed)
//!   — the single-threaded allocator entry points. `alloc`/`alloc_zeroed`
//!   are **safe** `pub fn`s; `dealloc`/`realloc` are **`unsafe fn`s** (R6-MS-1/2)
//!   carrying a `# Safety` contract that mirrors `GlobalAlloc`'s — a
//!   well-behaved caller passes a valid prior pointer/layout (see the current
//!   method's `# Safety` contract).
//!   The crate's former posture was a safe `pub fn` with a defensive **M2
//!   contract at runtime** (a foreign or already-freed pointer degraded to a
//!   no-op); that was reversed after the round5 `memory_safety_review` produced
//!   concrete safe-Rust counterexamples proving the defensive checks
//!   insufficient — see [`dealloc`](AllocCore::dealloc)'s `# Safety` section
//!   for the full rationale and exploit catalogue, and `CHANGELOG.md`
//!   (R6-MS-1/2) for the migration. The M2 defensive paths are RETAINED as
//!   defence-in-depth. None of the entry points panic or recurse.
//!
//! ## Single-threaded
//!
//! `AllocCore` is deliberately NEITHER `Send` NOR `Sync` (raw pointers, no
//! `unsafe impl` of either). In `production` the `AllocCore` inside a registry
//! slot stays in place, but ownership can transfer to a later claimant via the
//! slot's ordered lease protocol; soundness does not depend on `Send`.

mod alloc_core_core_diag;

mod bootstrap;

/// Group module: the always-compiled bench/diagnostic statics
/// (`DECOMMIT_CALLS`, the decay/Tier-1 routing oracles, the zero-pass and
/// realloc/promotion counters, `FOREIGN_OR_UNROUTABLE_FREES`),
/// `promotion_byte_bucket`, and the `LargeCacheHitCounter` alias.
mod counters;
#[cfg(feature = "alloc-global")]
pub(crate) use counters::LARGE_REMOTE_RETIREMENTS;

/// M-C oracle (Ph4c, receipt open question M-C) numeric observers of the
/// strict trim-path sidecar ingress drain — see `sidecar_drain.rs` and
/// `SeferAlloc::dbg_sidecar_ingress_stats`. Not public API; never read by
/// production logic.
#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "bench-internals"
))]
pub(crate) use sidecar_drain::{
    BACKGROUND_INGRESS_STEP_CALLS, SIDECAR_INGRESS_DRAIN_CALLS, SIDECAR_INGRESS_RECORDS_CONSUMED,
};

/// Shared Small-drain finalization result and the discovery drain's U31
/// bench observer, re-exported for `alloc_core::small`'s discovery drain
/// (see `sidecar_drain.rs`; `DiscoverySummaryObserver` stays private to
/// `sidecar_drain` — its only external consumer, the discovery scan, now
/// uses the wrapping `U31NoPendingObserver`).
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
pub(crate) use sidecar_drain::{DrainEnd, U31NoPendingObserver};

/// Group module: object lifecycle — construction config resolution, bootstrap
/// invocation, and teardown.
mod lifecycle;

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
mod sidecar_drain;

#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]
#[doc(hidden)]
mod sidecar_test_hooks;

/// Group module: the GlobalAlloc-face entry points (`alloc`, `alloc_zeroed`,
/// `dealloc`, `realloc`) and the in-place realloc fast-path family.
mod mem;
// R6-T16: consumed by the fastbin-gated own-free/realloc paths and the
// `medium-classes` promotion path via the `alloc_core::OwnedBlock` re-export.
#[cfg(all(
    feature = "alloc-global",
    any(feature = "fastbin", feature = "medium-classes")
))]
#[cfg_attr(not(feature = "fastbin"), allow(unused_imports))]
pub(crate) use mem::OwnedBlock;

/// Group module: NUMA-node caching, the registry-walk/membership accessors
/// (`segment_bases`/`contains_base`/`small_cur`), and `(size, align)`
/// classification.
mod state;

mod alloc_core_impl;
pub use alloc_core_impl::*;
