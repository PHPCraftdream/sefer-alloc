//! Phase 12.2 — the global heap registry (§2.1 of
//! `ALLOC_PLAN_PHASE12-13.md`): a self-hosting slot table of heaps, gated
//! behind `alloc-global` (it becomes the substrate of `SeferAlloc` in 12.3).
//!
//! The registry is the keystone inversion of Phase 12: heaps become SLOTS in
//! a global, self-hosting table (the `Region` slot-table discipline, reflected
//! one level deeper — the heap pool itself becomes a slot table). A thread
//! does NOT own its heap; it caches a raw `*mut HeapCore` to a registry slot
//! in TLS (12.3). Thread exit does not drop the heap; it abandons its
//! segments back to the registry (12.3/12.4) and recycles the slot.
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
//! - `tagged_ptr` — the packed `(value | tag)` ABA-defence word.
//! - [`heap_core`] — the thin, slot-resident heap value (`HeapCore`).
//! - [`heap_slot`] — one slot (`HeapSlot`): state / generation / heap / link.
//! - [`heap_overflow`] — RAD-4b: the slot-resident second-chance MPSC
//!   overflow ring that absorbs a cross-thread free once a segment's
//!   `RemoteFreeRing` AND its bounded retry are both exhausted.
//! - [`bootstrap`] — the process-global `Registry` + atomic state-machine.
//! - [`heap_registry`] — the claim/recycle/abandon API.
//!
//! [`heap_core`]: self::heap_core
//! [`heap_slot`]: self::heap_slot
//! [`heap_overflow`]: self::heap_overflow
//! [`bootstrap`]: self::bootstrap
//! [`heap_registry`]: self::heap_registry

#[doc(hidden)]
pub mod bootstrap;
#[doc(hidden)]
pub mod heap_core;
// `pub` (doc-hidden) only so a standalone miri UB-detection test
// (`tests/miri_heap_overflow_unit.rs`) can reach `HeapOverflow`'s
// `new_boxed_for_test`/`push`/`drain` test surface directly, without paying
// the full `bootstrap::ensure()` + `MAX_HEAPS`-slot registry cost that made
// exercising this protocol through the normal `remote_fanin` harnesses
// impractically slow under miri's interpreter — mirrors the existing
// `heap_core`/`heap_slot` doc-hidden test-only export pattern.
#[cfg(feature = "alloc-xthread")]
#[doc(hidden)]
pub mod heap_overflow;
#[doc(hidden)]
pub mod heap_registry;
#[doc(hidden)]
pub mod heap_slot;
// `pub` (doc-hidden) only so `tests/regression_counter_wrap.rs` can reach the
// `dbg_*` pack/unpack forwarders for the W7a tag-wrap counterfactual. The
// `TaggedPtr` type itself stays `pub(crate)`; only the thin test forwarders
// are `pub`. Not part of the supported public surface.
#[doc(hidden)]
pub mod tagged_ptr;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
pub(crate) mod tcache;

#[doc(hidden)]
pub use heap_core::HeapCore;
// RAD-4 (Phase 4, E3a): the overflow-retry diagnostic counters — see their
// doc comments in `heap_core.rs` for the full rationale. `#[doc(hidden)]
// pub` (not stable API) so `tests/remote_fanin.rs` can read them, mirroring
// the existing `DBG_LARGE_XTHREAD_RECLAIMED` test-only export pattern.
#[cfg(feature = "alloc-xthread")]
#[doc(hidden)]
pub use heap_core::{DBG_RING_PUSH_RETRIED, DBG_RING_PUSH_RETRY_EXHAUSTED};
// 0.3.x task #132: the reclaim counter moved to the shared
// `alloc_core::deferred_large` module (both public faces bump the SAME
// counter now); re-exported here for backward compatibility with existing
// `sefer_alloc::registry::DBG_LARGE_XTHREAD_RECLAIMED` call sites/tests.
#[cfg(feature = "alloc-xthread")]
#[doc(hidden)]
pub use crate::alloc_core::deferred_large::DBG_LARGE_XTHREAD_RECLAIMED;
#[doc(hidden)]
pub use heap_registry::heaps_claimed_high_water;
#[cfg(feature = "alloc-decommit")]
#[doc(hidden)]
pub use heap_registry::large_cache_hits_total;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
#[doc(hidden)]
pub use heap_registry::tcache_hits_total;
#[doc(hidden)]
pub use heap_registry::HeapRegistry;
#[doc(hidden)]
pub use heap_slot::HeapSlot;
