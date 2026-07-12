//! [`LargeCacheMode`] — the operating-mode enum for the per-shard large-segment
//! free-cache (feature = `alloc-decommit`).
//!
//! Extracted verbatim from `alloc_core.rs` (task #27, one-export-per-file
//! maintainability split). This is a self-contained data enum — no methods, no
//! private helpers — referenced by [`LargeCacheConfig`](super::large_cache_config::LargeCacheConfig)
//! (its `mode` field/setter), by the `AllocCore` shard's `large_cache_mode`
//! field, and by the `dbg_large_cache_mode` test seam. Re-exported at the crate
//! root as `sefer_alloc::LargeCacheMode` via `alloc_core::mod.rs` and `lib.rs`.

/// The large-cache operating modes.
///
/// `Lazy` is the default and currently the only mode with implemented
/// behaviour. `Background` and `Both` are reserved for a future background
/// scavenger thread that is **not yet implemented**: they are accepted by
/// [`LargeCacheConfig::mode`](super::large_cache_config::LargeCacheConfig::mode)
/// for API stability, but materialising a heap whose config resolves to one of
/// them (in [`AllocCore::new_with_config`](super::AllocCore::new_with_config))
/// **panics** with a clear message rather than silently degrading to `Lazy`.
/// Set via [`LargeCacheConfig::mode`].
///
/// [`LargeCacheConfig::mode`]: super::large_cache_config::LargeCacheConfig::mode
#[cfg(feature = "alloc-decommit")]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum LargeCacheMode {
    /// Default: Phase 2 lazy decay only. No background thread. Identical to
    /// pre-Phase-3 behaviour; all existing tests continue to pass unchanged.
    Lazy,
    /// Reserved for a future background scavenger thread that visits idle
    /// shards and calls `run_decay_step()` on their large-caches.
    /// **Not yet implemented** — materialising a heap with this mode panics
    /// (see the type-level doc); use [`Lazy`](Self::Lazy).
    Background,
    /// Alias for `Background`. Reserved for the future distinction "lazy hooks
    /// AND background thread active" vs "background thread only".
    /// **Not yet implemented** — materialising a heap with this mode panics
    /// (see the type-level doc); use [`Lazy`](Self::Lazy).
    Both,
}
