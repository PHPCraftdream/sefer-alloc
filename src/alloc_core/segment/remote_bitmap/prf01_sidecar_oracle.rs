//! Compile-time-only discovery observations; never consulted by allocator policy.
#[cfg(all(r18_sidecar_scan_bench, r19_sidecar_summary_oracle))]
compile_error!("PRF01 oracle must not use R18 runtime prefilter mode");
use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(all(
    feature = "bench-internals",
    any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
))]
static PENDING_WORDS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
static PENDING_WORDS_EXCHANGED: AtomicU64 = AtomicU64::new(0);
#[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
static NONEMPTY_CUTS: AtomicU64 = AtomicU64::new(0);
#[cfg(all(
    feature = "bench-internals",
    any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
))]
static DISCOVERY_RECORDS_RECLAIMED: AtomicU64 = AtomicU64::new(0);
#[cfg(all(
    feature = "bench-internals",
    any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
))]
static ROUTED_SCANS: AtomicU64 = AtomicU64::new(0);
#[cfg(all(
    feature = "bench-internals",
    any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
))]
static SUMMARY_WORDS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(all(
    feature = "bench-internals",
    any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
))]
static SUMMARY_HINT_LOADS: AtomicU64 = AtomicU64::new(0);
#[cfg(all(
    feature = "bench-internals",
    any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
))]
static SUMMARY_HINT_RMW: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
static STRICT_WORDS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
static STRICT_RECORDS_CONSUMED: AtomicU64 = AtomicU64::new(0);

/// Bounded oracle processes take deltas, never reset shared counters.
pub(crate) struct Prf01SidecarOracle;

impl Prf01SidecarOracle {
    #[cfg(all(
        feature = "bench-internals",
        any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
    ))]
    pub(crate) fn constructed() {
        ROUTED_SCANS.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(all(
        feature = "bench-internals",
        any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
    ))]
    pub(crate) fn completed(visited: u64, nonempty: u64, reclaimed: u64) {
        PENDING_WORDS_VISITED.fetch_add(visited, Ordering::Relaxed);
        #[cfg(r19_sidecar_summary_oracle)]
        {
            PENDING_WORDS_EXCHANGED.fetch_add(visited, Ordering::Relaxed);
            NONEMPTY_CUTS.fetch_add(nonempty, Ordering::Relaxed);
        }
        #[cfg(not(r19_sidecar_summary_oracle))]
        let _ = nonempty;
        DISCOVERY_RECORDS_RECLAIMED.fetch_add(reclaimed, Ordering::Relaxed);
    }

    #[cfg(all(
        feature = "bench-internals",
        any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
    ))]
    pub(crate) fn summary_word_visited() {
        SUMMARY_WORDS_VISITED.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(all(
        feature = "bench-internals",
        any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
    ))]
    pub(crate) fn summary_hint_load() {
        SUMMARY_HINT_LOADS.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(all(
        feature = "bench-internals",
        any(r19_sidecar_summary_oracle, r20_prf01_c3_oracle)
    ))]
    pub(crate) fn summary_hint_rmw() {
        SUMMARY_HINT_RMW.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(all(feature = "bench-internals", r20_prf01_c3_oracle))]
    pub(crate) fn c3_snapshot(quiescent_missing: u64) -> [u64; 7] {
        [
            PENDING_WORDS_VISITED.load(Ordering::Relaxed),
            SUMMARY_WORDS_VISITED.load(Ordering::Relaxed),
            SUMMARY_HINT_LOADS.load(Ordering::Relaxed),
            SUMMARY_HINT_RMW.load(Ordering::Relaxed),
            DISCOVERY_RECORDS_RECLAIMED.load(Ordering::Relaxed),
            quiescent_missing,
            core::mem::size_of::<crate::registry::segment_route::SmallSidecar>() as u64,
        ]
    }

    #[cfg(feature = "bench-internals")]
    pub(crate) fn strict_visited(visited: u64, records: u64) {
        STRICT_WORDS_VISITED.fetch_add(visited, Ordering::Relaxed);
        STRICT_RECORDS_CONSUMED.fetch_add(records, Ordering::Relaxed);
    }

    #[cfg(feature = "bench-internals")]
    pub(crate) fn strict_snapshot() -> (u64, u64) {
        (
            STRICT_WORDS_VISITED.load(Ordering::Relaxed),
            STRICT_RECORDS_CONSUMED.load(Ordering::Relaxed),
        )
    }

    /// Fixed-route counters remain available for R19 oracle builds.
    #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
    pub(crate) fn snapshot() -> [u64; 8] {
        [
            PENDING_WORDS_VISITED.load(Ordering::Relaxed),
            PENDING_WORDS_EXCHANGED.load(Ordering::Relaxed),
            NONEMPTY_CUTS.load(Ordering::Relaxed),
            DISCOVERY_RECORDS_RECLAIMED.load(Ordering::Relaxed),
            ROUTED_SCANS.load(Ordering::Relaxed),
            SUMMARY_WORDS_VISITED.load(Ordering::Relaxed),
            SUMMARY_HINT_RMW.load(Ordering::Relaxed),
            SUMMARY_HINT_LOADS.load(Ordering::Relaxed),
        ]
    }
}
