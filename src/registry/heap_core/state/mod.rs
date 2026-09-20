//! `HeapCore` state submodules (decls only): ownership/binding machinery
//! (`ownership`) and the tcache/magazine lifecycle (`tcache`,
//! `tcache_flush`). Re-export policy lives in the parent `heap_core/mod.rs`.

mod ownership;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
pub(crate) mod tcache;
mod tcache_flush;
