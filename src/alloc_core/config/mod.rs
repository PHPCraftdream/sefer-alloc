//! The public const-buildable configuration types (`Profile`,
//! `LargeCacheConfig`, `LargeCacheMode`, `SmallSegmentPoolConfig`).
//
// The four children keep their `alloc-decommit` gate HERE (mirroring the root
// re-exports in `alloc_core/mod.rs`) because the FILES themselves are not
// feature-independent: their principal types are `#[cfg(feature =
// "alloc-decommit")]`-gated inside the files, so ungated compilation breaks
// `alloc-core`-only builds (profile.rs's unconditional imports would not
// resolve). Gating the declarations keeps their compiled-file set identical
// to when they lived directly in `alloc_core`.

#[cfg(feature = "alloc-decommit")]
pub mod large_cache_config;
#[cfg(feature = "alloc-decommit")]
pub mod large_cache_mode;
/// R30-7 (task #456), reworked R31-9 (task #473): [`Profile`] — a small
/// builder composing two independent, named, measured configuration axes
/// ([`profile::SmallPoolPolicy`] for `pool_segments`/`pool_byte_cap`,
/// [`profile::LargeCachePolicy`] for large-cache `headroom_bytes`), from
/// this project's own measured gate reports (R27-3/R27-4/R30-6/R31-1/R31-2).
/// See the module doc for the full rationale and exact numbers.
#[cfg(feature = "alloc-decommit")]
pub mod profile;
#[cfg(feature = "alloc-decommit")]
pub mod small_segment_pool_config;
