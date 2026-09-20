//! The large/huge allocation path — `alloc_large` and its slow/reclaim path
//! (`alloc_core_large`), the per-shard large-cache decay/eviction cluster
//! (`alloc_core_large_cache`), the experimental `large-cache-extended`
//! sidecar (`large_cache_extended`), and the cross-thread deferred-free
//! Treiber stack (`deferred_large`).
//!
//! All children are re-exported by `alloc_core` (at their original
//! visibility/cfg parity) so every one stays reachable at its existing
//! `alloc_core::<name>` module path. `pub(super)` on the two private ones:
//! that is exactly the visibility domain the original flat root `mod`s had
//! (the whole `crate::alloc_core` subtree), and a bare private `mod` here
//! would not be nameable by the parent's parity re-exports.

pub(super) mod alloc_core_large;
#[cfg(feature = "alloc-decommit")]
pub(super) mod alloc_core_large_cache;
/// The cross-thread deferred-free Treiber stack for Large/huge segments
/// (task A1, extracted for #132). Used by the allocator face
/// (`registry::heap_core::HeapCore`) and any direct `AllocCore` user so the
/// double-push-guarded push/drain logic is not duplicated.
///
/// `pub` (not `pub(crate)`) only because `alloc_core` itself is
/// `#[doc(hidden)]` (see `lib.rs`): `DBG_LARGE_XTHREAD_RECLAIMED` is
/// re-exported (via `registry`) as a `#[doc(hidden)]` test-only diagnostic.
#[doc(hidden)]
pub mod deferred_large;
/// R13-7 (task #277, EXPERIMENTAL `large-cache-extended`): the lazily-
/// materialised sidecar that widens the large-segment free-cache beyond the
/// fixed 8 base slots. See the module doc for the full design. A named
/// `unsafe` seam (single documented reason: dereferencing the
/// `leak_zeroed_pages`-published sidecar pointer, owner-only, no
/// `OncePtrCell` needed).
#[cfg(feature = "large-cache-extended")]
pub(crate) mod large_cache_extended;
