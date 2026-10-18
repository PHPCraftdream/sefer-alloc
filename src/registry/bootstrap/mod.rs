//! ## Current registry and chunk protocol
//!
//! `Registry` is a const-initialised static with one
//! [`once_ptr_cell::OncePtrCell`] per chunk. The cell owns the
//! `UNINIT → INITIALIZING → READY` publication protocol; the winning
//! initializer obtains zeroed pages with `aligned_vmem::leak_zeroed_pages`,
//! initializes the chunk in place, and publishes it through the cell. The
//! leaked pages and initialized slots remain valid for the process lifetime.
//! A failed chunk reservation is returned by the fallible slot accessor so a
//! claim may use the existing fallback policy; the infallible accessor is for
//! indices whose chunks are already known to exist.
//!
//! `Registry` also tracks the minted-index count, scan cursor, advisory reuse
//! hint, and saturation version. Reusable slots are selected by the bitmap
//! (`FREE_SLOTS`), with the hint serving only as a candidate; claim and
//! maintenance ownership is established by slot-state CAS, not by the hint.
//!
//! `Release`/`Acquire` publication in `OncePtrCell` makes the initialized
//! chunk visible before slot resolution. Bootstrap uses OS-zeroed pages and
//! does not allocate through the installed global allocator. Foreign publication
//! uses independent sidecars; terminal metadata in allocation headers is separate.

// R1-07 (src review round 1): this file is `mod.rs` — decls and
// path-preserving re-exports only, per the "mod.rs — reexports only, no
// code" rule — so it carries NO `#![allow(unsafe_code)]` of its own; a
// blanket allow here used to silently widen to every descendant, including
// `loom_shim` (a `#[cfg(loom)]`-only module that was invisible to both this
// crate's README/`src/lib.rs` unsafe inventory and this file's own
// seam-narrative comment, until R1-07 fixed both). Each child file that
// actually contains `unsafe` now carries its OWN tier-1 `#![allow(unsafe_code)]`
// with its own `// SAFETY:` proof at every block:
//  - [`registry`] — casts the leaked `aligned_vmem::leak_zeroed_pages`
//    reservation to `*mut RegistryChunk` and dereferences the pointer the
//    cell publishes (`p.as_ref()`), sound because the cell's `Release`
//    publish establishes happens-before.
//  - [`ensure`] — the per-chunk `ensure_chunk`/`ensure_chunk_slow` CAS/
//    reserve/publish/spin protocol that drives `registry`'s dereference
//    above (the state-machine ITSELF is `once_ptr_cell::OncePtrCell`,
//    CRATE-P3-extracted; this file's own `unsafe` is the pointer cast/deref
//    around calling it).
//  - [`loom_shim`] (`--cfg loom` only) — `unsafe impl Send`/`Sync` for its
//    const-capable `OncePtrCell` stand-in, plus `NonNull::new_unchecked`
//    calls proved by the sentinel-address check immediately above each site.
// [`chunk`] has no `unsafe` of its own and carries no allow — its layout
// constants are computed with safe `core::mem::size_of`/`align_of`.

// Structural reorg step 5: the flat `bootstrap.rs` became this directory.
// `mod.rs` is decls + path-preserving re-exports only (per the
// one-export-per-file rule); the code lives in the child files:
//
// - [`registry`] — the `Registry` struct, `MAX_HEAPS`, `static REGISTRY`, and
//   its slot-resolution / dbg accessor methods.
// - [`ensure`] — the process-global `ensure()` accessor, the per-chunk
//   materialisation slow path, and the test-only dbg hooks.
// - [`chunk`] — `RegistryChunk` (the former `registry_chunk.rs`, moved
//   verbatim; the module path becomes `bootstrap::chunk`).
// - [`loom_shim`] — the `#[cfg(loom)]` const-capable `OncePtrCell` stand-in
//   needed by the static initializer.
mod chunk;
mod ensure;
mod free_slot_hint;
#[cfg(loom)]
pub(crate) mod loom_shim;
mod registry;
pub(crate) mod saturation;

// Re-exports preserving the flat file's item paths (`bootstrap::X` — consumed
// by registry heaps and integration tests):
#[cfg(feature = "internals")]
pub use ensure::count_for_test;
#[cfg(feature = "internals")]
pub use ensure::dbg_num_chunks;
#[cfg(feature = "internals")]
pub use ensure::dbg_rollback_chunk_sentinel_reenterable;
#[cfg(feature = "internals")]
pub use ensure::dbg_set_inject_chunk_oom;
#[cfg(feature = "internals")]
pub use ensure::dbg_slot_or_none;
pub use ensure::ensure;
#[cfg(feature = "internals")]
pub use free_slot_hint::FreeSlotHint;
pub(crate) use free_slot_hint::FREE_SLOTS;
/// Re-exported so a consumer of [`dbg_rollback_chunk_sentinel_reenterable`]
/// can match on its result without depending on `once-ptr-cell` directly
/// (it is an OPTIONAL dependency of this crate). The probe's result type is
/// pure data with no atomics, so it is loom-agnostic and the same enum
/// serves both the real cell and the `#[cfg(loom)]` shim.
#[cfg(feature = "internals")]
pub use once_ptr_cell::RollbackProbe;
pub use registry::{Registry, MAX_HEAPS};
