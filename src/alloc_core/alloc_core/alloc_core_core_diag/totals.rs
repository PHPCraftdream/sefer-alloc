//! The always-compiled, `AllocStats`-backing diagnostic accessors of
//! [`AllocCore`](crate::alloc_core::alloc_core::AllocCore) (Sol-F1):
//! `foreign_or_unroutable_frees`, `segments_reserved_total`,
//! `segments_released_total` as `pub(crate)` production methods, plus
//! R6-T38's `internals`-gated `dbg_*` test aliases. They no longer sit on
//! the public `AllocCore` surface — see the group module doc for the
//! rationale (mechanical split of the former flat `alloc_core_core_diag.rs`;
//! pure code movement, no behavior changed).

use crate::alloc_core::alloc_core::AllocCore;

use crate::alloc_core::alloc_core::counters::FOREIGN_OR_UNROUTABLE_FREES;

/// Sol-F1 (task #563): NOT `internals`-gated — these three back the stable,
/// always-available `AllocStats::stats()` (`src/global/sefer_alloc.rs`),
/// which calls them unconditionally under plain `--features production`.
/// See this file's module doc for the full rationale.
impl AllocCore {
    /// DIAGNOSTIC: process-wide dropped frees: no matching live address route,
    /// or rejected terminal publication. Not an exhaustive invalid-free or
    /// layout-mismatch detector; callers still owe a live allocation and exact
    /// layout. Backs
    /// [`AllocStats::foreign_or_unroutable_frees`](crate::AllocStats::foreign_or_unroutable_frees);
    /// see [`FOREIGN_OR_UNROUTABLE_FREES`]. A relaxed load. Reads `0` under a
    /// bare `alloc-core` build unless `alloc-stats` is on.
    // R6-T16: backs AllocStats::stats() (alloc-global) and the internals
    // dbg aliases below; suppressed where both are absent.
    #[cfg_attr(
        not(any(feature = "alloc-global", feature = "internals")),
        allow(dead_code)
    )]
    #[cfg(feature = "alloc-core")]
    pub(crate) fn foreign_or_unroutable_frees() -> u64 {
        FOREIGN_OR_UNROUTABLE_FREES.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// DIAGNOSTIC (task E1): process-wide count of successful OS segment
    /// reservations since process start (every `os::Segment::reserve`
    /// success plus NUMA-pinned reservations). Monotonic, relaxed — pairs
    /// with `AllocCore::segments_released_total`; the difference is
    /// the current process-wide live segment count. Always compiled (not
    /// feature-gated) — every build reserves segments via `os::Segment::reserve`.
    // R6-T16: see `foreign_or_unroutable_frees`'s note.
    #[cfg_attr(
        not(any(feature = "alloc-global", feature = "internals")),
        allow(dead_code)
    )]
    #[must_use]
    pub(crate) fn segments_reserved_total() -> u64 {
        crate::alloc_core::os::segments_reserved_total()
    }

    /// DIAGNOSTIC (task E1): process-wide count of successful OS segment
    /// releases since process start. Monotonic, relaxed. See
    /// `AllocCore::segments_reserved_total`.
    // R6-T16: see `foreign_or_unroutable_frees`'s note.
    #[cfg_attr(
        not(any(feature = "alloc-global", feature = "internals")),
        allow(dead_code)
    )]
    #[must_use]
    pub(crate) fn segments_released_total() -> u64 {
        crate::alloc_core::os::segments_released_total()
    }
}

/// R6-T38: `internals`-gated test aliases of the crate-only stats accessors
/// above. `tests/` reads the process-wide counters through these `dbg_*`
/// names; the production accessors they forward to are `pub(crate)`.
#[cfg(feature = "internals")]
impl AllocCore {
    /// Test alias of [`Self::foreign_or_unroutable_frees`].
    #[doc(hidden)]
    #[cfg(feature = "alloc-core")]
    pub fn dbg_foreign_or_unroutable_frees() -> u64 {
        Self::foreign_or_unroutable_frees()
    }

    /// Test alias of the internal local-reject counter (R6-T55): LOCAL no-op
    /// rejections — interior Large frees and `small_free_guard` `RejectNoOp`s
    /// — that deliberately never enter the public
    /// `foreign_or_unroutable_frees`. See `counters::LOCAL_REJECT_FREES`.
    #[doc(hidden)]
    #[cfg(feature = "alloc-core")]
    #[must_use]
    pub fn dbg_local_reject_frees() -> u64 {
        crate::alloc_core::LOCAL_REJECT_FREES.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// TH-1: small-segment reservations of this process so far.
    #[doc(hidden)]
    #[cfg(all(feature = "bench-internals", not(feature = "numa-aware")))]
    #[must_use]
    pub fn dbg_small_segments_reserved() -> u64 {
        crate::alloc_core::small::alloc_core_small::SMALL_SEGMENTS_RESERVED
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// TH-1: small segments the transparent-huge-page hint was issued for.
    #[doc(hidden)]
    #[cfg(all(feature = "bench-internals", not(feature = "numa-aware")))]
    #[must_use]
    pub fn dbg_thp_hinted_small_segments() -> u64 {
        crate::alloc_core::small::alloc_core_small::THP_HINTED_SMALL_SEGMENTS
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Test alias of [`Self::segments_reserved_total`].
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_segments_reserved_total() -> u64 {
        Self::segments_reserved_total()
    }

    /// Test alias of [`Self::segments_released_total`].
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_segments_released_total() -> u64 {
        Self::segments_released_total()
    }
}
