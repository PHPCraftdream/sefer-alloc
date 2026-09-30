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
//! The registry module is `pub` only so integration tests in `tests/` can
//! exercise it before 12.3 wires it into `SeferAlloc`. It is NOT part of the
//! crate's supported public surface; every item is `#[doc(hidden)]` and may
//! change in any Phase 12.x sub-commit. Once 12.3 caches the registry pointer
//! inside the TLS binding, the test-only pub surface here shrinks (or moves
//! behind a `registry-test` dev-feature).
//!
//! ## Re-exports only
//!
//! Per the one-export-per-file rule, no logic lives here. The files:
//!
//! - [`heap_core`] — the thin, slot-resident heap value (`HeapCore`).
//! - [`heap_slot`] — one slot (`HeapSlot`): state / generation / heap / link.
//! - `bootstrap::chunk` — R6-OPT-P0-2 (round 1): a lazily-materialised,
//!   fixed-size shard of the slot array (`RegistryChunk`), so a process only
//!   ever pays the OS commit cost for the chunks it actually touches.
//! - [`bootstrap`] — the process-global `Registry` + per-chunk atomic
//!   state-machine.
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
// Unconnected Stage 3 substrate; exposed only through the existing
// doc-hidden `internals` test surface.
#[doc(hidden)]
#[allow(dead_code)]
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
#[doc(hidden)]
pub use heap_registry::HeapRegistry;
#[doc(hidden)]
pub use heap_slot::HeapSlot;
