//! Compile-time-only discovery observations; never consulted by allocator policy.
#[cfg(all(r18_sidecar_scan_bench, r19_sidecar_summary_oracle))]
compile_error!("PRF01 oracle must not use R18 runtime prefilter mode");
use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
static PENDING_WORDS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
static PENDING_WORDS_EXCHANGED: AtomicU64 = AtomicU64::new(0);
#[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
static NONEMPTY_CUTS: AtomicU64 = AtomicU64::new(0);
#[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
static DISCOVERY_RECORDS_RECLAIMED: AtomicU64 = AtomicU64::new(0);
#[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
static ROUTED_SCANS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
static STRICT_WORDS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
static STRICT_RECORDS_CONSUMED: AtomicU64 = AtomicU64::new(0);

/// Bounded oracle processes take deltas, never reset shared counters.
/// One aggregate update per completed discovery scan, not TLS on each word.
pub(crate) struct Prf01SidecarOracle;

impl Prf01SidecarOracle {
    #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
    pub(crate) fn constructed() {
        ROUTED_SCANS.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
    pub(crate) fn completed(visited: u64, nonempty: u64, reclaimed: u64) {
        PENDING_WORDS_VISITED.fetch_add(visited, Ordering::Relaxed);
        PENDING_WORDS_EXCHANGED.fetch_add(visited, Ordering::Relaxed);
        NONEMPTY_CUTS.fetch_add(nonempty, Ordering::Relaxed);
        DISCOVERY_RECORDS_RECLAIMED.fetch_add(reclaimed, Ordering::Relaxed);
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

    #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
    pub(crate) fn snapshot() -> [u64; 8] {
        [
            PENDING_WORDS_VISITED.load(Ordering::Relaxed),
            PENDING_WORDS_EXCHANGED.load(Ordering::Relaxed),
            NONEMPTY_CUTS.load(Ordering::Relaxed),
            DISCOVERY_RECORDS_RECLAIMED.load(Ordering::Relaxed),
            ROUTED_SCANS.load(Ordering::Relaxed),
            // A has no summary algorithm: these are genuine constant-zero APIs.
            // They are not fallbacks for compiled-out observations.
            0, // summary_words_visited
            0, // summary_rmw
            0, // summary_load
        ]
    }
}
