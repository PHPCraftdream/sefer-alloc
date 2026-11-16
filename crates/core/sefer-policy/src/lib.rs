//! # sefer-policy — the sefer allocator's policy layer
//!
//! The const-buildable configuration types, the configuration-policy
//! builder types, the medium-promotion reachability predicate and the
//! Large-THP (transparent huge page) policy runtime of the
//! [`sefer-alloc`](https://crates.io/crates/sefer-alloc) allocator,
//! extracted verbatim from the root crate (`src/alloc_core/config`) as a
//! `#![forbid(unsafe_code)]`, `no_std` crate with a single dependency, `aligned-vmem`,
//! only for the `ThpAdvice::classify` error type.
//!
//! ## What lives here
//!
//! - [`large_cache_config`] — [`large_cache_config::LargeCacheConfig`], the builder type every
//!   allocator entry point that takes a configuration consumes
//!   (feature `alloc-decommit`).
//! - [`large_cache_mode`] — [`LargeCacheMode`](large_cache_mode::LargeCacheMode),
//!   the Large-cache decommit mode enum (feature `alloc-decommit`).
//! - [`small_segment_pool_config`] —
//!   [`SmallSegmentPoolConfig`](small_segment_pool_config::SmallSegmentPoolConfig),
//!   the small-segment pool pair (feature `alloc-decommit`).
//! - [`policy`] — the three configuration-policy types
//!   [`Profile`](policy::Profile), [`SmallPoolPolicy`](policy::SmallPoolPolicy)
//!   and [`LargeCachePolicy`](policy::LargeCachePolicy): small measured
//!   builders composing named axes on top of the config types.
//! - [`medium_promotion`] — the single source of truth for the
//!   medium→Large promotion-reachability predicate (a `cfg!` const plus the
//!   two `#[cfg]`-generator macros).
//! - [`policy::large_thp`], [`policy::large_thp_observation`],
//!   [`policy::large_thp_runtime`] — the Large-THP policy runtime: the
//!   packed five-word [`ThpTuple`](policy::large_thp_runtime::ThpTuple)
//!   codec, share/gradient/curve estimators, the observation control
//!   selector and residency planner, and the publication cell, cadence and
//!   budget ledger of the adaptive Auto mechanism.
//!
//! The segment geometry, the OS aperture and the allocator itself stay in
//! the root `sefer-alloc` crate; this crate is policy only — pure `core`
//! arithmetic, constants and `core::sync::atomic` state; its single
//! dependency is the already-published `aligned-vmem` aperture crate, used
//! only for the `ThpAdvice::classify` error type.
//!
//! ## Feature mesh
//!
//! The moved sources gate on the root crate's feature names
//! (`alloc-decommit`, `alloc-global`, `bench-internals`, `internals`,
//! `large-cache-extended`, `medium-classes`, `exact-span-large`,
//! `large-reserved-capacity`, `numa-aware`). sefer-policy mirrors each of
//! them 1:1 (all default-off, none implying another); the root crate
//! forwards them so every `#[cfg]` evaluates exactly as before. Standalone
//! consumers enable the same names directly on this crate.
//!
//! ## Zero own unsafe
//!
//! `#![forbid(unsafe_code)]` at the top of this crate. No raw pointers, no
//! allocator invariants, no C/C++ libraries; the single dependency is `aligned-vmem` (error type only).

#![no_std]
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

// The four config-bearing modules keep their `alloc-decommit` gate HERE
// (mirroring the root crate's former `alloc_core/config/mod.rs` gating)
// because the FILES themselves are not feature-independent: their principal
// types are `#[cfg(feature = "alloc-decommit")]`-gated inside the files, so
// ungated compilation breaks `alloc-decommit`-less builds (profile.rs's
// unconditional imports would not resolve). Gating the declarations keeps
// their compiled-file set identical to when they lived directly in the root
// crate's `alloc_core::config`.
// `medium_promotion` is deliberately UNGATED: the file holds only the `cfg!`
// const and the two `macro_rules!` definitions — no imports — and must exist
// in every configuration.
#[cfg(feature = "alloc-decommit")]
pub mod large_cache_config;
#[cfg(feature = "alloc-decommit")]
pub mod large_cache_mode;
pub mod medium_promotion;
#[cfg(feature = "alloc-decommit")]
pub mod policy;
#[cfg(feature = "alloc-decommit")]
pub mod small_segment_pool_config;
