//! Re-exports only (per repo convention): the public/`pub(crate)` surface of
//! the `heap_core` subtree, kept name-compatible with the pre-reorg flat
//! files (`heap_core.rs`, `heap_core_alloc.rs`, `heap_core_ownership.rs`,
//! `heap_core_tcache.rs`, `tcache.rs`). Decls + re-exports; no logic.

mod alloc;
mod core;
mod diag;
// Plain `mod` (reorg step 3): the flat `heap_core_diag.rs` sibling whose
// `HARDENED_LARGE_NOOP_COUNT` load needed this widened has itself moved into
// this tree (`diag/`), so nothing outside the subtree paths through `free`
// any more.
mod free;
mod state;

pub use self::core::HeapCore;

// 0.3.x (task C1 → #133 → W3): the per-heap magazine-hit counter type,
// cfg'd exactly like its definition in `core` (consumed by
// `state::tcache_flush`).
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
pub(crate) use self::core::TcacheHitCounter;

// RAD-4 (Phase 4, E3a): the retry-spin budget. `heap_core_xthread` (now the
// `heap_core_xthread/` directory under `registry/`) reaches it via
// `super::heap_core::`; the moved file's `pub(super)` would have confined it
// to this subtree, so it is widened to `pub(crate)` in `core.rs` and
// re-exported here. The two cfg'd consts are mutually exclusive (miri vs
// native); this single `use` binds whichever exists.
#[cfg(feature = "alloc-xthread")]
pub(crate) use self::core::RING_PUSH_RETRY_SPINS;

// RAD-4 (Phase 4, E3a): the overflow-retry diagnostic counters, re-exported
// so `registry`'s `pub use heap_core::{..}` and the flat sibling
// `heap_core_xthread` keep resolving. `#[doc(hidden)] pub` per the
// test-only-export pattern (see `registry/mod.rs`).
#[cfg(feature = "alloc-xthread")]
#[doc(hidden)]
pub use self::core::{DBG_RING_PUSH_RETRIED, DBG_RING_PUSH_RETRY_EXHAUSTED};
