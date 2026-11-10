//! `LargeThpMode` — how a fresh Large mapping is offered the transparent-huge-page hint.

/// Selects the large-allocation THP advice policy of a
/// [`LargeCacheConfig`](crate::alloc_core::config::large_cache_config::LargeCacheConfig).
///
/// Process-wide (the first explicit config published wins whole). Effective on
/// Linux/Android only; a no-op elsewhere and forced to a no-op under
/// `numa-aware`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LargeThpMode {
    /// Never hints a Large range (the default).
    Off,
    /// Hints every eligible fresh Large range.
    Always,
    /// Hints a learned share of each request group. Learning is NOT wired in
    /// this stage: `Auto` currently advises nothing.
    Auto,
}
