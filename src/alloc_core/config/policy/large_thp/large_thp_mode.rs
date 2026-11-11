//! `LargeThpMode` — how a fresh Large mapping is offered the transparent-huge-page hint.

/// Selects the large-allocation THP advice policy of a
/// [`LargeCacheConfig`](crate::alloc_core::config::large_cache_config::LargeCacheConfig).
///
/// Process-wide (the first explicit config published wins whole). Effective on
/// Linux/Android only; a no-op elsewhere and forced to a no-op under
/// `numa-aware`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LargeThpMode {
    /// Never hints a Large range; the default outside supported Linux.
    Off,
    /// Hints every eligible fresh Large range; the default on Linux with
    /// `alloc-decommit` and without `numa-aware` (minimum requested 8 MiB).
    Always,
    /// Hints a learned share of each request group from clean owner controls.
    /// Explicit opt-in, not the default.
    Auto,
}
