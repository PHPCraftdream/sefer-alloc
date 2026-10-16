//! Phase 12.2 — the global heap registry (§2.1 of
//! `ALLOC_PLAN_PHASE12-13.md`): a self-hosting slot table of heaps, gated
//! behind `alloc-global` (it becomes the substrate of `SeferAlloc` in 12.3).
//!
//! The registry is the keystone inversion of Phase 12: heaps become SLOTS in
//! a global, self-hosting table (the `Region` slot-table discipline, reflected
//! one level deeper — the heap pool itself becomes a slot table). A thread
//! does NOT own its heap; it caches a raw `*mut HeapCore` to a registry slot
//! in TLS (12.3). On thread exit, `AbandonGuard::drop` does NOT abandon or
//! walk segments — it recycles the slot (`LIVE → FREE`) with the `HeapCore`
//! and all its segments staying whole; a later thread that claims the recycled
//! slot reuses the same `HeapCore` as-is (whole-slot reuse, Phase 12.5).
//!
//! ## `#[doc(hidden)]` — not public API
//!
//! The registry module and its selected items are doc-hidden implementation
//! surfaces. Visibility is limited by each item's current Rust visibility and
//! cfg gates; this is not a stable public API.
//!
//! ## Re-exports only
//!
//! Per the one-export-per-file rule, no logic lives here. The files:
//!
//! - [`heap_core`] — the thin, slot-resident heap value (`HeapCore`).
//! - [`heap_slot`] — one slot (`HeapSlot`): state / generation / heap / link.
//! - `bootstrap::chunk` — lazily materialized fixed-size slot-array chunk
//!   (`RegistryChunk`), limiting committed registry metadata to used chunks.
//! - [`bootstrap`] — process-global registry initialization and per-chunk
//!   state management.
//! - [`heap_registry`] — the claim/recycle API (whole-slot reuse).
//!
//! [`heap_core`]: self::heap_core
//! [`heap_slot`]: self::heap_slot
//! [`bootstrap`]: self::bootstrap
//! [`heap_registry`]: self::heap_registry

#[doc(hidden)]
pub mod bootstrap;
#[doc(hidden)]
pub mod heap_core;
mod heap_core_xthread;
#[doc(hidden)]
pub mod heap_registry;
#[doc(hidden)]
pub mod heap_slot;
// Production segment registration and foreign-free terminal routing;
// external access uses the doc-hidden `internals` surface.
#[doc(hidden)]
pub mod segment_route;

#[doc(hidden)]
pub use heap_core::HeapCore;
#[doc(hidden)]
pub use heap_registry::config_conflicts_total;
#[doc(hidden)]
pub use heap_registry::heaps_claimed_high_water;
#[cfg(feature = "alloc-decommit")]
#[doc(hidden)]
pub use heap_registry::large_cache_hits_total;
#[cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "alloc-decommit"
))]
#[doc(hidden)]
pub use heap_registry::tcache_and_large_cache_hits_total;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
#[doc(hidden)]
pub use heap_registry::tcache_hits_total;
#[cfg(feature = "internals")]
#[doc(hidden)]
pub use heap_registry::HeapLease;
#[doc(hidden)]
pub use heap_registry::HeapRegistry;
