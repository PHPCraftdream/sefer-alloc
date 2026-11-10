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

    /// TH-1 follow-up: the huge-page threshold in force (published or default; `0` = never).
    #[doc(hidden)]
    #[cfg(all(feature = "bench-internals", not(feature = "numa-aware")))]
    #[must_use]
    pub fn dbg_huge_page_after_segments() -> u64 {
        crate::alloc_core::small::alloc_core_small::huge_page_after()
    }

    /// J13b: the process large-THP tuple in force (published or default).
    #[doc(hidden)]
    #[cfg(all(
        feature = "bench-internals",
        feature = "alloc-decommit",
        not(feature = "numa-aware")
    ))]
    #[must_use]
    pub fn dbg_thp_tuple() -> crate::alloc_core::policy::large_thp_runtime::ThpTuple {
        crate::alloc_core::policy::large_thp_runtime::ThpRuntime::current()
    }

    /// J13b: large-THP tuple publications that conflicted with the published tuple.
    #[doc(hidden)]
    #[cfg(all(
        feature = "bench-internals",
        feature = "alloc-decommit",
        not(feature = "numa-aware")
    ))]
    #[must_use]
    pub fn dbg_thp_tuple_conflicts() -> u64 {
        crate::alloc_core::policy::large_thp_runtime::ThpRuntime::conflicts()
    }

    /// J13b: per-request-size-group Auto control-selection and residency
    /// telemetry. Fields are selections, premise failures, valid observations,
    /// invalidated remote-reclaim, invalidated realloc, errors, resident
    /// pages, and sampled pages.
    #[doc(hidden)]
    #[cfg(all(
        feature = "bench-internals",
        feature = "alloc-decommit",
        not(feature = "numa-aware")
    ))]
    #[must_use]
    pub fn dbg_thp_control_telemetry() -> [[u64; 8]; 6] {
        crate::alloc_core::policy::large_thp_runtime::ThpRuntime::control_telemetry()
    }

    /// J13b: per-group e/m/share plus independent applied/dropped counters.
    /// Each pair is coherent; whole-array closure requires worker quiescence.
    #[doc(hidden)]
    #[cfg(all(
        feature = "bench-internals",
        feature = "alloc-decommit",
        not(feature = "numa-aware")
    ))]
    #[must_use]
    pub fn dbg_thp_learning_telemetry() -> [[u64; 5]; 6] {
        crate::alloc_core::policy::large_thp_runtime::ThpRuntime::learning_telemetry()
    }

    /// J13b: the current process-global control ordinal (one Relaxed load).
    /// Boundary/totality check only: ticket uniqueness and per-cohort
    /// placement come from the ticket log's actual records, never from the
    /// difference of two probe reads.
    #[doc(hidden)]
    #[cfg(all(
        feature = "bench-internals",
        feature = "alloc-decommit",
        not(feature = "numa-aware")
    ))]
    #[must_use]
    pub fn dbg_thp_control_ordinal() -> u64 {
        crate::alloc_core::policy::large_thp_runtime::ThpRuntime::control_ordinal_probe()
    }

    /// J13b: the runtime selector's decision for a raw ordinal. Pure
    /// diagnostic passthrough; lets tests census the per-cohort decisions of
    /// an observed ordinal window.
    #[doc(hidden)]
    #[cfg(all(
        feature = "bench-internals",
        feature = "alloc-decommit",
        not(feature = "numa-aware")
    ))]
    #[must_use]
    pub fn dbg_thp_control_decision_at(ordinal: u64) -> bool {
        crate::alloc_core::policy::large_thp_runtime::ThpRuntime::control_decision_for_ordinal(
            ordinal,
        )
    }

    /// J13b: reset the bench-internals diagnostic control-ticket log cursor.
    #[doc(hidden)]
    #[cfg(all(
        feature = "bench-internals",
        feature = "alloc-decommit",
        not(feature = "numa-aware")
    ))]
    pub fn dbg_thp_control_ticket_log_reset() {
        crate::alloc_core::policy::large_thp_runtime::ThpRuntime::ticket_log_reset();
    }

    /// J13b: snapshot the diagnostic control-ticket log, FAIL-CLOSED:
    /// returns `None` unless the window is fully supported capture — the
    /// raw cursor is within the 128-entry cap (no dropped records) and
    /// every consumed entry carries the valid bit (no top-truncated
    /// ticket). Entries are packed `ordinal << 2 | selected << 1 | valid`.
    /// Capture is coherent only around a quiescent scenario (reset before
    /// spawn, snapshot after join).
    #[doc(hidden)]
    #[cfg(all(
        feature = "bench-internals",
        feature = "alloc-decommit",
        not(feature = "numa-aware")
    ))]
    #[must_use]
    pub fn dbg_thp_control_ticket_log() -> Option<([u64; 128], u64)> {
        let (entries, cursor) =
            crate::alloc_core::policy::large_thp_runtime::ThpRuntime::ticket_log_snapshot();
        if crate::alloc_core::policy::large_thp_runtime::ThpRuntime::ticket_log_capture_supported(
            &entries, cursor,
        ) {
            Some((entries, cursor))
        } else {
            None
        }
    }

    /// J13b: the production packer for one diagnostic ticket record — the
    /// exact value `record_control_ticket` stores (`ordinal << 2 |
    /// selected << 1 | valid` for ordinals below 2^62; the explicit invalid
    /// marker 0 for anything else). Pure value function over its arguments;
    /// lets the capture-bounds test bind to production packing instead of
    /// duplicating the formula.
    #[doc(hidden)]
    #[cfg(all(
        feature = "bench-internals",
        feature = "alloc-decommit",
        not(feature = "numa-aware")
    ))]
    #[must_use]
    pub fn dbg_thp_control_ticket_log_pack(ordinal: u64, selected: bool) -> u64 {
        crate::alloc_core::policy::large_thp_runtime::ThpRuntime::ticket_log_pack(ordinal, selected)
    }

    /// J13b: the production capture-validity gate over a raw snapshot —
    /// false iff the raw cursor exceeds the 128-entry cap or any consumed
    /// entry lacks the valid bit. Pure value function over caller-provided
    /// arrays; the exact predicate `dbg_thp_control_ticket_log` applies.
    #[doc(hidden)]
    #[cfg(all(
        feature = "bench-internals",
        feature = "alloc-decommit",
        not(feature = "numa-aware")
    ))]
    #[must_use]
    pub fn dbg_thp_control_ticket_log_capture_supported(entries: &[u64; 128], cursor: u64) -> bool {
        crate::alloc_core::policy::large_thp_runtime::ThpRuntime::ticket_log_capture_supported(
            entries, cursor,
        )
    }

    /// J13b: huge-page advice bytes currently charged to the budget ledger.
    #[doc(hidden)]
    #[cfg(all(
        feature = "bench-internals",
        feature = "alloc-decommit",
        not(feature = "numa-aware")
    ))]
    #[must_use]
    pub fn dbg_thp_ledger_used() -> usize {
        crate::alloc_core::policy::large_thp_runtime::ThpRuntime::ledger().used()
    }

    /// J13b: budget bytes charged over the process lifetime.
    #[doc(hidden)]
    #[cfg(all(
        feature = "bench-internals",
        feature = "alloc-decommit",
        not(feature = "numa-aware")
    ))]
    #[must_use]
    pub fn dbg_thp_charged_total() -> u64 {
        crate::alloc_core::policy::large_thp_runtime::ThpRuntime::charged_total()
    }

    /// J13b: budget bytes given back over the process lifetime.
    #[doc(hidden)]
    #[cfg(all(
        feature = "bench-internals",
        feature = "alloc-decommit",
        not(feature = "numa-aware")
    ))]
    #[must_use]
    pub fn dbg_thp_returned_total() -> u64 {
        crate::alloc_core::policy::large_thp_runtime::ThpRuntime::returned_total()
    }

    /// J13b: bytes the kernel accepted large-THP advice for, in every mode.
    #[doc(hidden)]
    #[cfg(all(
        feature = "bench-internals",
        feature = "alloc-decommit",
        not(feature = "numa-aware")
    ))]
    #[must_use]
    pub fn dbg_thp_advised_bytes_total() -> u64 {
        crate::alloc_core::policy::large_thp_runtime::ThpRuntime::advised_bytes_total()
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
