//! The public const-buildable configuration types (`Profile`,
//! `LargeCacheConfig`, `LargeCacheMode`, `SmallSegmentPoolConfig`) and the
//! configuration-policy types now live in the `sefer-policy` crate
//! (`crates/core/sefer-policy`); this module re-exports them under their
//! historic paths so every `crate::alloc_core::config::…` reference in the
//! root crate (and the public API surface re-exported through
//! `alloc_core`/`lib.rs`) keeps resolving unchanged.
//
// The re-exports keep the `alloc-decommit` gate HERE (mirroring the root
// re-exports in `alloc_core/mod.rs`) and the per-module gate inside
// sefer-policy's own `lib.rs`, so the compiled-file set is identical to when
// the files lived directly in `alloc_core`.
//
// `continuation_bounds` is NOT policy — it is segment geometry for the
// owner-only free-list walk (it reads `segment_header`, `os::SEGMENT`,
// `fatal::fatal`, `size_classes::MIN_BLOCK`) — so it stays in the root crate
// at this exact path. (It moves to the `sefer-segment` crate in plan step
// P2.1.) `platform/node.rs` holds the only platform→config back-edge to it.
// `medium_promotion`'s macros expand `#[cfg]`s that reference root features
// at their use sites, and its `cfg!` const reads the same feature set; the
// file moved to sefer-policy, which mirrors those feature names 1:1 (the
// root manifest forwards them), and the module is re-exported here ungated
// exactly as before.

#[cfg(not(feature = "hardened"))]
pub(crate) mod continuation_bounds;

#[cfg(feature = "alloc-decommit")]
pub use sefer_policy::{large_cache_config, large_cache_mode, policy, small_segment_pool_config};

/// R6-T20: the medium-promotion reachability predicate's single source of
/// truth (`cfg!` const + the two `#[cfg]` generator macros). Ungated — the
/// file is feature-independent (only `cfg!` and `macro_rules!`, no imports)
/// and must exist in every configuration: its consumers span the whole
/// `alloc_core` tree (always) and the `registry` tree (`alloc-global`).
pub(crate) use sefer_policy::medium_promotion;
