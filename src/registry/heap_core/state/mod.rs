//! `HeapCore` state submodules (decls only): ownership/binding machinery
//! (`ownership`) and the tcache/magazine lifecycle (`tcache`,
//! `tcache_flush`). Re-export policy lives in the parent `heap_core/mod.rs`.

#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
pub(crate) mod depot;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
mod depot_paths;
mod ownership;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
pub(crate) mod tcache;
mod tcache_flush;
