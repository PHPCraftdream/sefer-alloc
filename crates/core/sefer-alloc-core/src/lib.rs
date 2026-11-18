//! `sefer-alloc-core`: the small/medium and large/huge allocation paths, the
//! `AllocCore` hub and its diagnostics of `sefer-alloc`.
//!
//! Plan step P2.2 (docs/plans/2026-11-15-workspace-restructure-plan.md):
//! `src/alloc_core` moved wholesale from the root crate; the module tree,
//! file names and internal `crate::alloc_core::…` paths are unchanged (the
//! Miri/evidence pins on `platform/node.rs` and `mem/advise_fresh_large.rs`
//! keep their exact relative paths). The root `sefer-alloc` crate re-exports
//! this module tree at `sefer_alloc::alloc_core` under the `alloc-core`
//! feature. Cross-crate consumers (registry/global call sites) normalize
//! their visibility/import paths as part of P2.2.
//!
//! Without the `alloc-core` feature the crate compiles empty, mirroring the
//! root crate's feature-gated module layout.
//!
//! No dependency points back to `sefer-alloc`.

// Root-crate parity: the module tree compiles only under `alloc-core`; in
// that configuration the crate root flips to `deny(unsafe_code)` with the
// confined-unsafe seams carrying their own `#![allow(unsafe_code)]`
// attributes. Without it, the crate is `forbid(unsafe_code)` empty.
#![cfg_attr(not(feature = "alloc-core"), forbid(unsafe_code))]
#![cfg_attr(feature = "alloc-core", deny(unsafe_code))]
#![cfg_attr(docsrs, feature(doc_cfg))]

// R2-17 rationale: mirrors the pointer-width gate in the root `src/lib.rs`
// (identical message) — the allocator core relies on the same 64-bit layout
// premise, and compiling this crate before the root crate means its gate
// must fire on its own on 32-bit targets.
#[cfg(all(feature = "alloc-core", not(target_pointer_width = "64")))]
compile_error!(
    "sefer-alloc: allocator features require a 64-bit target (R2-17 support \
     decision): `alloc-core` and every feature built on it (`alloc-global`, \
     `production`, ...) require metadata/magazine layout bounds, AtomicU64 and \
     pointer-provenance rules that have NOT been audited on 32-bit pointer \
     widths. Rebuild for a \
     64-bit target, or drop the allocator features (the region-only \
     `Region<T>` surface still works on 32-bit). See README.md \
     \"Target support\"."
);

#[cfg(feature = "alloc-core")]
#[doc(hidden)]
pub mod alloc_core;
