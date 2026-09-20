//! The small/medium allocation path — the `alloc_small`/`dealloc_small`/
//! carve hot cluster ([`alloc_core_small`]), the directory-accelerated
//! segment lookup that serves it, the small-segment reserve, the magazine
//! (tcache) batch ops ([`alloc_core_small_magazine`]), cross-thread reclaim
//! ([`alloc_core_small_reclaim`]), small-path diagnostics
//! ([`alloc_core_small_diag`]), the `alloc-decommit` empty-segment pool +
//! decommit machinery ([`alloc_core_small_pool`]), and the measurement-only
//! `ReservedSmallSegment` handle ([`reserved_small_segment`]).
//!
//! All children are re-exported by `alloc_core` (at their original
//! visibility/cfg parity) so every one stays reachable at its existing
//! `alloc_core::<name>` module path.

/// The small-object alloc/dealloc/carve hot cluster of [`AllocCore`] — the
/// former flat `alloc_core_small.rs`, mechanically split into `mod.rs` +
/// `find_segment.rs` + `dealloc.rs` + `reserve.rs` + `directory.rs`.
pub(super) mod alloc_core_small;
/// Small-path diagnostic accessors (`dbg_*`) over the segment substrate.
pub(super) mod alloc_core_small_diag;
/// The magazine (tcache) batch fill/flush ops of the small path.
pub(super) mod alloc_core_small_magazine;
/// Mechanism-2 empty-small-segment pool + M6 decommit cluster. The
/// declaration keeps the original root `mod`'s `alloc-decommit` gate (not
/// only the root re-export): the file's `use` block is not feature-gated and
/// every item in it is, so compiling it under `alloc-decommit`-off configs
/// would only produce unused-import warnings — an intrinsic gate, same
/// discipline as `platform::numa`/`platform::dirty_by_class`.
#[cfg(feature = "alloc-decommit")]
pub(super) mod alloc_core_small_pool;
/// Cross-thread (ring-drain) small-path reclaim — `reclaim_offset` and its
/// `hardened` generation-checked variant.
pub(super) mod alloc_core_small_reclaim;
/// RAD-5 (plan Phase 5-E4), verdict GO — the typed, non-forgeable,
/// move-consumed handle for the `dbg_decomp_reserve_and_keep`/
/// `dbg_decomp_release` measurement hook pair.
///
/// `pub` (not `pub(crate)`) for the root's `pub use` (E0365); the module's
/// own items keep their gates, and the root re-export carries the original
/// `all(alloc-decommit, bench-internals)` cfg.
pub mod reserved_small_segment;
