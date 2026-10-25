//! `HeapCore` state submodules (decls only): ownership/binding machinery
//! (`ownership`) and the tcache/magazine lifecycle (`tcache`,
//! `tcache_flush`). Re-export policy lives in the parent `heap_core/mod.rs`.

mod ownership;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
pub(crate) mod tcache;
mod tcache_flush;
// B4 (task b4-provenance): heap-private shadow depot simulation —
// measurement-only, compiled out entirely without `bench-internals`.
#[cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "bench-internals"
))]
pub(crate) mod depot_shadow;
