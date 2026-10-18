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
// R6-T16: read by `stats()` under `alloc-decommit`; external diagnostics
// reach it under `internals`. Suppress the unused-import lint elsewhere.
#[cfg_attr(
    not(any(feature = "alloc-decommit", feature = "internals")),
    allow(unused_imports)
)]
#[doc(hidden)]
pub use heap_registry::config_conflicts_total;
#[doc(hidden)]
pub use heap_registry::heaps_claimed_high_water;
// R6-T16: the lib-side consumer reads this only in the
// `alloc-decommit ∧ ¬fastbin` branch of `cache_hit_totals`; suppress the
// unused-import lint exactly there instead of narrowing the re-export's cfg.
#[cfg_attr(
    not(all(feature = "alloc-decommit", not(feature = "fastbin"))),
    allow(unused_imports)
)]
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
// R6-T16: the lib-side consumer reads this only in the
// `fastbin ∧ ¬alloc-decommit` branch of `cache_hit_totals`; suppress the
// unused-import lint exactly there instead of narrowing the re-export's cfg.
#[cfg_attr(
    not(all(feature = "fastbin", not(feature = "alloc-decommit"))),
    allow(unused_imports)
)]
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
#[doc(hidden)]
pub use heap_registry::tcache_hits_total;
#[cfg(feature = "internals")]
#[doc(hidden)]
pub use heap_registry::HeapLease;
#[doc(hidden)]
pub use heap_registry::HeapRegistry;
#[doc(hidden)]
#[cfg_attr(not(feature = "internals"), allow(unused_imports))] // tests/registry_basic.rs
pub use heap_slot::HeapSlot;
