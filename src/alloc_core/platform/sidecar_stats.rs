//! R2-12: process-wide owner-sidecar reservation/release accounting — the
//! acceptance oracle for the sidecar ownership fix (`platform::sidecar`).
//!
//! ## Why this exists (R2-12)
//!
//! Before R2-12, the owner-only sidecars (`SegmentDirectory`,
//! `LargeCacheExtension`) were backed by `aligned_vmem::leak_zeroed_pages`
//! spans that were NEVER released — defensible only for the bounded
//! population of process-lifetime registry heaps, but an unbounded VM leak
//! under repeated standalone `AllocCore::new`/drop churn. R2-12 made each
//! sidecar own its `aligned_vmem::Reservation` (the `sidecar::
//! AccountedSidecar` token, released with the owning `AllocCore`). These
//! counters are the fix's acceptance oracle: across a churn lifecycle,
//! releases must equal reservations per sidecar kind, so after the last drop
//! every owner-scoped span is accounted released and only the
//! explicitly-sanctioned process-global spans (registry-slot-lifetime
//! sidecars, the cross-thread-published `PerClassDirty`/
//! `HeapOverflowSidecar`) remain reserved.
//!
//! ## The three breakouts
//!
//! - `directory_*` — the `SegmentDirectory` sidecar under non-`numa-aware`
//!   builds (single-bucket bitmap span).
//! - `numa_directory_*` — the SAME `SegmentDirectory` sidecar under
//!   `numa-aware`, whose span is multiplied by `NODE_BITMAPS` (= `MAX_NODES
//!   + 1`) per-node buckets. Compile-time disjoint with `directory_*`: a
//!   given build routes every directory materialisation into exactly one of
//!   the two, so both flavors are independently observable across the
//!   feature matrix (plain/`production` rows exercise `directory_*`,
//!   `--all-features` exercises `numa_directory_*`).
//! - `large_cache_extension_*` — the `LargeCacheExtension` sidecar
//!   (`large-cache-extended`).
//!
//! Mirrors `segment_directory::directory_stats`' discipline: storage is
//! ALWAYS compiled under `alloc-core` so the `dbg_*` read accessor has a
//! stable definition; the per-event INCREMENTS are gated behind `internals`
//! (the R34-3 test-hook gate — a `production` build compiles no bookkeeping
//! at all, byte-identical behavior). Reads without increments return 0.

// Feature-matrix deadness, one allow for the whole file (each item is inert
// in SOME configuration without being absent — e.g. the counters are never
// read without `internals`, the record fns have no callers in a build with
// neither `alloc-segment-directory` nor `large-cache-extended`, the
// directory counters cannot increment without `alloc-segment-directory`):
// the same rationale as the `#[cfg_attr(not(feature = ...),
// allow(dead_code))]` pattern on `sidecar.rs`'s reserve/deref fns, applied
// file-level because the live/dead axis here is a cross-product of three
// independent features.
#![allow(dead_code)]

use core::sync::atomic::{AtomicU64, Ordering};

use super::sidecar::SidecarKind;

/// `SegmentDirectory` sidecar reservations (non-`numa-aware` builds).
pub(crate) static DIRECTORY_SIDECAR_RESERVATIONS: AtomicU64 = AtomicU64::new(0);

/// `SegmentDirectory` sidecar releases (non-`numa-aware` builds). After the
/// R2-12 fix this always catches up to `DIRECTORY_SIDECAR_RESERVATIONS` once
/// every materialising core has dropped; a permanently lagging value under
/// churn is the leak this oracle exists to catch.
pub(crate) static DIRECTORY_SIDECAR_RELEASES: AtomicU64 = AtomicU64::new(0);

/// `SegmentDirectory` sidecar reservations under `numa-aware` (the
/// `NODE_BITMAPS`-multiplied span).
pub(crate) static NUMA_DIRECTORY_SIDECAR_RESERVATIONS: AtomicU64 = AtomicU64::new(0);

/// `numa-aware` `SegmentDirectory` sidecar releases — same catch-up
/// invariant as [`DIRECTORY_SIDECAR_RELEASES`].
pub(crate) static NUMA_DIRECTORY_SIDECAR_RELEASES: AtomicU64 = AtomicU64::new(0);

/// `LargeCacheExtension` sidecar reservations (`large-cache-extended`).
pub(crate) static LARGE_CACHE_EXTENSION_SIDECAR_RESERVATIONS: AtomicU64 = AtomicU64::new(0);

/// `LargeCacheExtension` sidecar releases — same catch-up invariant as
/// [`LARGE_CACHE_EXTENSION_SIDECAR_RESERVATIONS`].
pub(crate) static LARGE_CACHE_EXTENSION_SIDECAR_RELEASES: AtomicU64 = AtomicU64::new(0);

/// Point-in-time snapshot of the process-wide owner-sidecar accounting (see
/// the module doc for the three breakouts). Reserve/release pairs are
/// per-sidecar-kind; the R2-12 invariant is `releases == reservations` for
/// every kind once all materialising cores have dropped.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SidecarReservationStats {
    /// `SegmentDirectory` sidecar reservations (non-`numa-aware` builds).
    pub directory_reservations: u64,
    /// `SegmentDirectory` sidecar releases (non-`numa-aware` builds).
    pub directory_releases: u64,
    /// `SegmentDirectory` sidecar reservations (`numa-aware` builds).
    pub numa_directory_reservations: u64,
    /// `SegmentDirectory` sidecar releases (`numa-aware` builds).
    pub numa_directory_releases: u64,
    /// `LargeCacheExtension` sidecar reservations.
    pub large_cache_extension_reservations: u64,
    /// `LargeCacheExtension` sidecar releases.
    pub large_cache_extension_releases: u64,
}

/// Record one successful sidecar span reservation of `kind`. Called by
/// `sidecar::AccountedSidecar::reserve` — i.e. only after the OS reservation
/// actually succeeded; an OOM (`None`) records nothing. The increment is
/// compiled only under `internals` (see the module doc); without the feature
/// this is a no-op and production builds carry zero bookkeeping.
pub(crate) fn record_sidecar_reservation(kind: SidecarKind) {
    #[cfg(feature = "internals")]
    match kind {
        SidecarKind::Directory => {
            DIRECTORY_SIDECAR_RESERVATIONS.fetch_add(1, Ordering::Relaxed);
        }
        SidecarKind::NumaDirectory => {
            NUMA_DIRECTORY_SIDECAR_RESERVATIONS.fetch_add(1, Ordering::Relaxed);
        }
        SidecarKind::LargeCacheExtension => {
            LARGE_CACHE_EXTENSION_SIDECAR_RESERVATIONS.fetch_add(1, Ordering::Relaxed);
        }
    }
    #[cfg(not(feature = "internals"))]
    {
        let _ = kind;
    }
}

/// Record one sidecar span release of `kind`. Called by
/// `sidecar::AccountedSidecar`'s `Drop` — i.e. exactly when the owning
/// `AllocCore` (or whatever holds the token) drops it, just before the inner
/// `aligned_vmem::Reservation` performs the actual OS release. Same
/// `internals` gating as [`record_sidecar_reservation`].
pub(crate) fn record_sidecar_release(kind: SidecarKind) {
    #[cfg(feature = "internals")]
    match kind {
        SidecarKind::Directory => {
            DIRECTORY_SIDECAR_RELEASES.fetch_add(1, Ordering::Relaxed);
        }
        SidecarKind::NumaDirectory => {
            NUMA_DIRECTORY_SIDECAR_RELEASES.fetch_add(1, Ordering::Relaxed);
        }
        SidecarKind::LargeCacheExtension => {
            LARGE_CACHE_EXTENSION_SIDECAR_RELEASES.fetch_add(1, Ordering::Relaxed);
        }
    }
    #[cfg(not(feature = "internals"))]
    {
        let _ = kind;
    }
}

/// R2-12 TEST/diagnostic-only: snapshot the process-wide owner-sidecar
/// reservation/release accounting. Pure observer — reads six counters, never
/// mutates allocator state, no raw pointers. `internals`-gated (the same
/// R34-3 test-hook gate as the rest of the accounting increments).
#[doc(hidden)]
#[cfg(feature = "internals")]
#[must_use]
pub fn dbg_sidecar_reservation_stats() -> SidecarReservationStats {
    SidecarReservationStats {
        directory_reservations: DIRECTORY_SIDECAR_RESERVATIONS.load(Ordering::Relaxed),
        directory_releases: DIRECTORY_SIDECAR_RELEASES.load(Ordering::Relaxed),
        numa_directory_reservations: NUMA_DIRECTORY_SIDECAR_RESERVATIONS.load(Ordering::Relaxed),
        numa_directory_releases: NUMA_DIRECTORY_SIDECAR_RELEASES.load(Ordering::Relaxed),
        large_cache_extension_reservations: LARGE_CACHE_EXTENSION_SIDECAR_RESERVATIONS
            .load(Ordering::Relaxed),
        large_cache_extension_releases: LARGE_CACHE_EXTENSION_SIDECAR_RELEASES
            .load(Ordering::Relaxed),
    }
}
