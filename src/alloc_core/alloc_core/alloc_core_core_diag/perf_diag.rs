//! `internals`-gated performance-counter accessors of [`AllocCore`] — the
//! `directory_stats` counters, the OPT-H / R34-23 realloc-oracle counters,
//! and the R29-5 promotion distribution (mechanical split of the former
//! flat `alloc_core_core_diag.rs`; pure code movement, no behavior
//! changed).

use crate::alloc_core::alloc_core::AllocCore;
use crate::alloc_core::directory_stats;

/// Sol-F1 (task #563): `internals`-gated — every other `dbg_*` diagnostic
/// hook in this file. See this file's module doc for the full rationale.
#[cfg(feature = "internals")]
impl AllocCore {
    // ── R7-A0: directory diagnostic counter accessors ───────────────────────
    //
    // Process-wide counters (Relaxed loads -- diagnostic only, no ordering).
    // Storage is always compiled; per-event increments are `alloc-stats`-gated.
    // Reads 0 when the increment was not compiled in. See
    // `directory_stats.rs` for the counter inventory.

    /// R7-A0: process-wide count of directory lookup hits (A3). Reads 0 until
    /// A3 wires the increment.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_hits() -> u64 {
        directory_stats::DIRECTORY_HITS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R7-A0: process-wide count of stale directory hits (A3). Reads 0 until
    /// A3 wires the increment.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_stale_hits() -> u64 {
        directory_stats::DIRECTORY_STALE_HITS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R7-A0: process-wide count of directory fallback scans (A3). Reads 0
    /// until A3 wires the increment.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_fallback_scans() -> u64 {
        directory_stats::DIRECTORY_FALLBACK_SCANS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R7-A0: process-wide count of directory bitmap words examined (A3).
    /// Reads 0 until A3 wires the increment.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_words_examined() -> u64 {
        directory_stats::DIRECTORY_WORDS_EXAMINED.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R7-A0: process-wide count of dirty segments drained (A4). Reads 0
    /// until A4 wires the increment.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_dirty_segments_drained() -> u64 {
        directory_stats::DIRTY_SEGMENTS_DRAINED.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R9-6 (class-aware dirty routing judge): process-wide count of
    /// `drain_dirty_segments` visits where the segment's ring, once drained in
    /// response to a `find_segment_with_free_impl(class_idx)` call, produced
    /// ZERO reclaimed blocks of the sought `class_idx` — i.e. wasted work from
    /// THAT caller's perspective that per-(segment,class) dirty routing would
    /// have avoided. The denominator is `dbg_dirty_segments_drained()`. The
    /// ratio wasted/total directly characterises the O(D) vs O(D_class) gap
    /// the review flagged. Diagnostic only; reads 0 unless `alloc-stats` is on.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_wasted_dirty_drains() -> u64 {
        directory_stats::WASTED_DIRTY_DRAINS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// STAGE-1 DIAGNOSTIC ONLY (R21-2, task #351): process-wide count of
    /// cross-class Small/Primordial grow attempts that reach OPT-H's
    /// (proposed, not-yet-implemented — see
    /// `docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md`) precondition-1 check
    /// in [`crate::alloc_core::AllocCore::realloc_inplace_fast_path_known_base`].
    /// This is the Stage-1 hit-rate DENOMINATOR; pairs with
    /// [`dbg_opt_h_hits`](Self::dbg_opt_h_hits). Relaxed load — diagnostic
    /// only. Reads 0 unless `alloc-stats` is on (the increment site is
    /// gated); the accessor is always compiled so callers need no `#[cfg]`.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_opt_h_attempts() -> u64 {
        crate::alloc_core::alloc_core::counters::OPT_H_ATTEMPTS
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// STAGE-1 DIAGNOSTIC ONLY (R21-2, task #351): process-wide count of
    /// cross-class Small/Primordial grow attempts (counted by
    /// [`dbg_opt_h_attempts`](Self::dbg_opt_h_attempts)) where ALL SIX of
    /// OPT-H's preconditions additionally held (design §2.1). This is the
    /// Stage-1 hit-rate NUMERATOR. Relaxed load — diagnostic only. Reads 0
    /// unless `alloc-stats` is on; the accessor is always compiled so callers
    /// need no `#[cfg]`.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_opt_h_hits() -> u64 {
        crate::alloc_core::alloc_core::counters::OPT_H_HITS
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R34-23 (task #542) path-activation oracle: process-wide count of
    /// Large→Large in-place realloc grows that succeeded via OPT-G
    /// (committed-span OR `large-reserved-capacity` reserved-VA path). A
    /// non-zero delta for a Large-growth workload proves the in-place path
    /// actually fired — not just that the config resolved. Relaxed load —
    /// diagnostic only. Reads 0 unless `alloc-stats` is on; the accessor is
    /// always compiled so callers need no `#[cfg]`.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_reloc_inplace_large_count() -> u64 {
        crate::alloc_core::alloc_core::counters::RELOC_INPLACE_LARGE_CALLS
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R34-23 (task #542) path-activation oracle: process-wide count of
    /// Small/Primordial same-class in-place reallocs that succeeded via OPT-F.
    /// Relaxed load — diagnostic only. Reads 0 unless `alloc-stats` is on.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_reloc_inplace_small_count() -> u64 {
        crate::alloc_core::alloc_core::counters::RELOC_INPLACE_SMALL_CALLS
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R34-23 (task #542) path-activation oracle: process-wide count of
    /// reallocs where the in-place fast paths DECLINED (forced the move leg:
    /// alloc-new + copy + dealloc-old). By construction
    /// `inplace_large + inplace_small + decline == total_fast_path_calls`.
    /// Relaxed load — diagnostic only. Reads 0 unless `alloc-stats` is on.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_reloc_fastpath_decline_count() -> u64 {
        crate::alloc_core::alloc_core::counters::RELOC_FASTPATH_DECLINE_CALLS
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// STAGE-1 DIAGNOSTIC ONLY (R29-5, task #436): process-wide count of
    /// successful medium→Large realloc promotions (`try_promote_to_large`
    /// returning `Some`) since process start — the numerator of the
    /// promotion-frequency question
    /// (`docs/perf/R29_5_PROMOTION_FREQUENCY_GATE.md`). Relaxed load —
    /// diagnostic only. Reads 0 unless `bench-internals` is on (the increment
    /// site is gated); the accessor is always compiled so callers need no
    /// `#[cfg]`. Pairs with
    /// [`dbg_promotion_bytes_sum`](Self::dbg_promotion_bytes_sum) and the
    /// [`dbg_promotion_bytes_hist`](Self::dbg_promotion_bytes_hist)
    /// distribution.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_promotion_count() -> u64 {
        crate::alloc_core::alloc_core::counters::PROMOTION_COUNT
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// STAGE-1 DIAGNOSTIC ONLY (R29-5): cumulative bytes copied across all
    /// promotions counted by [`dbg_promotion_count`](Self::dbg_promotion_count).
    /// `sum / count` is the mean copied bytes per promotion. Relaxed. Reads 0
    /// unless `bench-internals` is on.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_promotion_bytes_sum() -> u64 {
        crate::alloc_core::alloc_core::counters::PROMOTION_BYTES_SUM
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// STAGE-1 DIAGNOSTIC ONLY (R29-5): smallest `old_layout.size()` ever
    /// copied by a single promotion. Relaxed. Returns 0 if no promotion has
    /// occurred (the underlying static is initialised to `u64::MAX`, mapped to
    /// 0 here) so an idle workload reads a non-misleading 0 rather than
    /// `u64::MAX`. Reads 0 unless `bench-internals` is on.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_promotion_bytes_min() -> u64 {
        let v = crate::alloc_core::alloc_core::counters::PROMOTION_BYTES_MIN
            .load(core::sync::atomic::Ordering::Relaxed);
        if v == u64::MAX {
            0
        } else {
            v
        }
    }

    /// STAGE-1 DIAGNOSTIC ONLY (R29-5): largest `old_layout.size()` ever
    /// copied by a single promotion. Relaxed. Reads 0 unless `bench-internals`
    /// is on (or if no promotion has occurred).
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_promotion_bytes_max() -> u64 {
        crate::alloc_core::alloc_core::counters::PROMOTION_BYTES_MAX
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// STAGE-1 DIAGNOSTIC ONLY (R29-5): per-bucket histogram of bytes copied
    /// per promotion (one increment per event in exactly one bucket). The
    /// returned array is indexed by the bucket layout documented on
    /// [`PROMOTION_BYTES_HIST`](crate::alloc_core::alloc_core::counters::PROMOTION_BYTES_HIST):
    /// `[0]=<4KiB [1]=4-16KiB [2]=16-64KiB [3]=64-128KiB [4]=128-256KiB
    /// [5]=256-512KiB [6]=512-1024KiB [7]=>=1MiB`. Relaxed loads. Reads all-0
    /// unless `bench-internals` is on.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_promotion_bytes_hist() -> [u64; 8] {
        let h = &crate::alloc_core::alloc_core::counters::PROMOTION_BYTES_HIST;
        [
            h[0].load(core::sync::atomic::Ordering::Relaxed),
            h[1].load(core::sync::atomic::Ordering::Relaxed),
            h[2].load(core::sync::atomic::Ordering::Relaxed),
            h[3].load(core::sync::atomic::Ordering::Relaxed),
            h[4].load(core::sync::atomic::Ordering::Relaxed),
            h[5].load(core::sync::atomic::Ordering::Relaxed),
            h[6].load(core::sync::atomic::Ordering::Relaxed),
            h[7].load(core::sync::atomic::Ordering::Relaxed),
        ]
    }

    /// R7-A0: process-wide count of slots examined by
    /// `find_segment_with_free_impl` (the linear scan). This is the primary
    /// scan-cost counter -- it is LIVE in A0 (incremented per slot visited
    /// under `alloc-stats`).
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_full_scan_slots_examined() -> u64 {
        directory_stats::FULL_SCAN_SLOTS_EXAMINED.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R8-2 (task #215): process-wide count of genuine directory misses where
    /// the directory was TRUSTED authoritative and the O(S) linear-scan
    /// fallback was SKIPPED. Reads 0 until R8-2 wires the increment.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_authoritative_miss() -> u64 {
        directory_stats::DIRECTORY_AUTHORITATIVE_MISS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R8-2 (task #215): process-wide count of periodic re-validation full
    /// scans that found a segment the directory had missed and repaired its
    /// bit in-place. Expected to stay 0 in normal operation; a nonzero value
    /// is a canary for a directory-tracking bug. Reads 0 until R8-2 wires the
    /// increment.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_miss_self_heal() -> u64 {
        directory_stats::DIRECTORY_MISS_SELF_HEAL.load(core::sync::atomic::Ordering::Relaxed)
    }
}
