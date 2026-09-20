//! Free-side (`dealloc`/`realloc`/batched `dealloc`) `impl HeapCore` blocks
//! — the mechanical split of the former flat `heap_core_free.rs` +
//! `heap_core_dealloc_batch.rs`. Decls only; re-export policy lives in the
//! parent `heap_core/mod.rs`.

mod dealloc_batch;
// Reverted to `pub(crate)` (reorg step 3): the moved `diag/diag_probes.rs`
// sibling (inside `heap_core`, but NOT inside `free`) still item-paths
// `heap_core::free::dealloc::HARDENED_LARGE_NOOP_COUNT`, and a private
// `dealloc` is nameable only from `free`'s own descendants — the sibling
// cannot reach it at any path spelling. The `free` module itself was still
// tightened to a plain `mod` in `heap_core/mod.rs` (nothing outside the
// `heap_core` subtree paths through it). Everything else in the split is
// reached via `HeapCore` method resolution, not module paths.
pub(crate) mod dealloc;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
mod dealloc_own_base;
mod realloc;
