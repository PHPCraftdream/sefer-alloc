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
