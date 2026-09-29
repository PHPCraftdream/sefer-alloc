//! The always-compiled, `AllocStats`-backing diagnostic accessors of
//! [`AllocCore`] (Sol-F1): `dbg_foreign_or_unroutable_frees`,
//! `dbg_segments_reserved_total`, `dbg_segments_released_total`. NOT
//! `internals`-gated — see the group module doc for the rationale
//! (mechanical split of the former flat `alloc_core_core_diag.rs`; pure code
//! movement, no behavior changed).

use crate::alloc_core::alloc_core::AllocCore;

use crate::alloc_core::alloc_core::counters::FOREIGN_OR_UNROUTABLE_FREES;

/// Sol-F1 (task #563): NOT `internals`-gated — these three back the stable,
/// always-available `AllocStats::stats()` (`src/global/sefer_alloc.rs`),
/// which calls them unconditionally under plain `--features production`.
/// See this file's module doc for the full rationale.
impl AllocCore {
    /// DIAGNOSTIC: process-wide count of `dealloc` calls that dropped a free
    /// (foreign pointer, already-released segment, layout mismatch). Backs
    /// [`AllocStats::foreign_or_unroutable_frees`](crate::AllocStats::foreign_or_unroutable_frees);
    /// see [`FOREIGN_OR_UNROUTABLE_FREES`]. A relaxed load. Reads `0` under a
    /// bare `alloc-core` build unless `alloc-stats` is on.
    #[doc(hidden)]
    #[cfg(feature = "alloc-core")]
    pub fn dbg_foreign_or_unroutable_frees() -> u64 {
        FOREIGN_OR_UNROUTABLE_FREES.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// DIAGNOSTIC (task E1): process-wide count of successful OS segment
    /// reservations since process start (every `os::Segment::reserve`
    /// success plus NUMA-pinned reservations). Monotonic, relaxed — pairs
    /// with [`AllocCore::dbg_segments_released_total`]; the difference is
    /// the current process-wide live segment count. Always compiled (not
    /// feature-gated) — every build reserves segments via `os::Segment::reserve`.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_segments_reserved_total() -> u64 {
        crate::alloc_core::os::segments_reserved_total()
    }

    /// DIAGNOSTIC (task E1): process-wide count of successful OS segment
    /// releases since process start. Monotonic, relaxed. See
    /// [`AllocCore::dbg_segments_reserved_total`].
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_segments_released_total() -> u64 {
        crate::alloc_core::os::segments_released_total()
    }
}
