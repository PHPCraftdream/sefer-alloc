//! [`LargeCacheMode`] — the operating-mode enum for the per-shard large-segment
//! free-cache (feature = `alloc-decommit`).
//!
//! Extracted verbatim from `alloc_core.rs` (task #27, one-export-per-file
//! maintainability split). This is a self-contained data enum — no methods, no
//! private helpers — referenced by [`LargeCacheConfig`](super::large_cache_config::LargeCacheConfig)
//! (its `mode` field/setter), by the `AllocCore` shard's `large_cache_mode`
//! field, and by the `dbg_large_cache_mode` test seam. Re-exported at the crate
//! root as `sefer_alloc::LargeCacheMode` via `alloc_core::mod.rs` and `lib.rs`.

/// The large-cache operating mode.
///
/// `Lazy` is the default and currently the **only** variant: event-driven
/// decay with no background thread (a tick fires inline on the next large
/// alloc/free after the interval has elapsed). It is the sole mode with
/// implemented behaviour.
///
/// The enum is marked `#[non_exhaustive]` as a deliberate forward-compatibility
/// seam: a real future background-scavenger implementation can add the
/// corresponding variant(s) (e.g. a `Background` mode that visits idle shards
/// on a timer) as a *non-breaking* addition rather than another breaking
/// change. Earlier pre-1.0 revisions of this enum carried `Background`/`Both`
/// variants that were never implemented — they silently degraded to `Lazy` and
/// were later briefly made to panic, which itself conflicted with the crate's
/// never-panics entry-point guarantee (the panic was reachable lazily through
/// `GlobalAlloc::alloc`). Removing them outright ("make invalid states
/// unrepresentable"; see `docs/reviews/2026-07-12-round3-remediation-plan.md`,
/// решение №2) closes that gap at the type level. Set via
/// [`LargeCacheConfig::mode`].
///
/// [`LargeCacheConfig::mode`]: super::large_cache_config::LargeCacheConfig::mode
#[cfg(feature = "alloc-decommit")]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum LargeCacheMode {
    /// Default: Phase 2 lazy decay only. No background thread. Identical to
    /// pre-Phase-3 behaviour; all existing tests continue to pass unchanged.
    Lazy,
}
