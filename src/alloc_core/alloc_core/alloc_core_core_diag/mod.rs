//! Core/general diagnostics for [`AllocCore`] (mechanical split of
//! `alloc_core.rs`, task R6-CQ-7a).
//!
//! This file holds the `impl AllocCore { .. }` block for the `dbg_*`
//! diagnostic/test-only hooks that are NOT specific to the small-allocator
//! subsystem (see `alloc_core_small_diag.rs` for that cluster): segment
//! reservation/release counters, NUMA node lookup, page-map/layout-class
//! introspection, segment-id/kind-byte read+corrupt hooks, and the
//! table/registry teardown test seams (`dbg_unregister`/`dbg_recycle`).
//! Pure code-movement sibling of `alloc_core.rs`; no behavior changed.
//!
//! Sol-F1 (task #563, release-readiness review finding F1): `internals`
//! (see its own doc comment in `Cargo.toml`) gates the `alloc_core` /
//! `global` / `registry` MODULE PATHS, but `AllocCore` itself is
//! additionally re-exported at the crate root unconditionally (`pub use
//! alloc_core::AllocCore` in `src/lib.rs`, gated only on `alloc-core`).
//! Gating the module path alone does NOT hide `AllocCore`'s own INHERENT
//! `dbg_*` methods — those stay reachable as `sefer_alloc::AllocCore::dbg_*`
//! regardless of `internals`, since Rust's module-privacy rules only affect
//! how a TYPE is NAMED/reached, not the visibility of already-`pub` inherent
//! methods on a type that is itself reachable another way. This file
//! therefore gates its own `impl AllocCore` blocks directly with
//! `#[cfg(feature = "internals")]`, split into two blocks:
//!
//! 1. An UNGATED block holding exactly the three `dbg_*` accessors
//!    [`AllocCore::dbg_foreign_or_unroutable_frees`],
//!    [`AllocCore::dbg_segments_reserved_total`],
//!    [`AllocCore::dbg_segments_released_total`] — these back
//!    `AllocStats::stats()` (`src/global/sefer_alloc.rs`), a stable,
//!    always-available public API method that is NOT `internals`-gated;
//!    gating these three would break `stats()` under plain `production`.
//! 2. An `internals`-gated block holding every other `dbg_*` hook in this
//!    file — none of the rest has a caller outside `tests/`/`benches/`/
//!    `examples/`, all of which already require `internals` (either via
//!    `required-features` on the target, or — for the ~106 `tests/` files,
//!    which cannot carry `required-features` since they are not `[[test]]`
//!    Cargo.toml targets — via the `internals`-feature command line CI/local
//!    convention `internals`'s own Cargo.toml doc comment documents).
//
// Mechanical split (pure code movement, no behavior changed): the impl
// blocks of the former flat file now live in the sibling files below,
// grouped by subsystem. Each `internals`-gated sibling keeps the same
// `#[cfg(feature = "internals")]` on its `impl AllocCore` block that the
// single gated block carried here, and its `mod` declaration carries the
// same gate so non-`internals` builds do not parse the (fully gated) file.
#[cfg(feature = "internals")]
mod directory_diag;
#[cfg(feature = "internals")]
mod header_diag;
#[cfg(feature = "internals")]
mod perf_diag;
#[cfg(feature = "internals")]
mod table_diag;
/// The always-compiled `AllocStats` accessors — NOT `internals`-gated.
mod totals;
#[cfg(feature = "internals")]
mod vmem;
