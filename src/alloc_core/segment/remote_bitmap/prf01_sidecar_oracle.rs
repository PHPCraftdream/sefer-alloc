//! Compile-time-only discovery observations; never consulted by allocator policy.
#[cfg(r18_sidecar_scan_bench)]
compile_error!("PRF01 oracle must not use R18 runtime prefilter mode");
use core::sync::atomic::{AtomicU64, Ordering};

static PENDING_WORDS_VISITED: AtomicU64 = AtomicU64::new(0);
static PENDING_WORDS_EXCHANGED: AtomicU64 = AtomicU64::new(0);
static NONEMPTY_CUTS: AtomicU64 = AtomicU64::new(0);
static DISCOVERY_RECORDS_RECLAIMED: AtomicU64 = AtomicU64::new(0);
static SUMMARY_WORDS_VISITED: AtomicU64 = AtomicU64::new(0);
static SUMMARY_RMW: AtomicU64 = AtomicU64::new(0);
static SUMMARY_LOAD: AtomicU64 = AtomicU64::new(0);
static ROUTED_SCANS: AtomicU64 = AtomicU64::new(0);

/// Bounded oracle processes take deltas, never reset shared counters.
/// One aggregate update per completed discovery scan, not TLS on each word.
pub(crate) struct Prf01SidecarOracle;

impl Prf01SidecarOracle {
    pub(crate) fn constructed() {
        ROUTED_SCANS.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn published(rmw: u64, loads: u64) {
        SUMMARY_RMW.fetch_add(rmw, Ordering::Relaxed);
        SUMMARY_LOAD.fetch_add(loads, Ordering::Relaxed);
    }

    pub(crate) fn completed(visited: u64, nonempty: u64, reclaimed: u64, summary: u64) {
        SUMMARY_WORDS_VISITED.fetch_add(summary, Ordering::Relaxed);
        PENDING_WORDS_VISITED.fetch_add(visited, Ordering::Relaxed);
        PENDING_WORDS_EXCHANGED.fetch_add(visited, Ordering::Relaxed);
        NONEMPTY_CUTS.fetch_add(nonempty, Ordering::Relaxed);
        DISCOVERY_RECORDS_RECLAIMED.fetch_add(reclaimed, Ordering::Relaxed);
    }

    pub(crate) fn snapshot() -> [u64; 8] {
        [
            PENDING_WORDS_VISITED.load(Ordering::Relaxed),
            PENDING_WORDS_EXCHANGED.load(Ordering::Relaxed),
            NONEMPTY_CUTS.load(Ordering::Relaxed),
            DISCOVERY_RECORDS_RECLAIMED.load(Ordering::Relaxed),
            ROUTED_SCANS.load(Ordering::Relaxed),
            SUMMARY_WORDS_VISITED.load(Ordering::Relaxed),
            SUMMARY_RMW.load(Ordering::Relaxed),
            SUMMARY_LOAD.load(Ordering::Relaxed),
        ]
    }
}
