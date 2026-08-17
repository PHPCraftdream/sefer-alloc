use super::huge::HUGE_DECOMMIT_ATTEMPTS;
use super::unix::{
    UNIX_EXACT_RESERVE_ATTEMPTS, UNIX_EXACT_RESERVE_HITS, UNIX_MADVISE_ATTEMPTS,
    UNIX_MADVISE_SUCCESSES, UNIX_MUNMAP_FAILURES,
};
use super::windows::{
    WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES, WINDOWS_LARGE_PAGE_PLAIN_FALLBACK_SUCCESSES,
    WINDOWS_LARGE_PAGE_RETRY_FAILURES, WINDOWS_RESERVE_COMMIT_SINGLE_CALLS,
    WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS, WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS,
    WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES, WINDOWS_VIRTUALFREE_RELEASE_FAILURES,
};
use core::sync::atomic::Ordering;

/// `bench-internals`: reset all fourteen counters (`UNIX_EXACT_RESERVE_ATTEMPTS`,
/// `UNIX_EXACT_RESERVE_HITS`, `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS`,
/// `WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS`,
/// `WINDOWS_LARGE_PAGE_RETRY_FAILURES`,
/// `WINDOWS_LARGE_PAGE_PLAIN_FALLBACK_SUCCESSES`,
/// `UNIX_MADVISE_ATTEMPTS`,
/// `UNIX_MADVISE_SUCCESSES`, `UNIX_MUNMAP_FAILURES`,
/// `WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS`,
/// `WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES`,
/// `WINDOWS_VIRTUALFREE_RELEASE_FAILURES`, `HUGE_DECOMMIT_ATTEMPTS`,
/// and `WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES` -- all private storage, read via their
/// respective accessor functions above) to 0.
/// Test/bench hook only —
/// lets a measurement window start from a clean count instead of accumulating
/// across the whole process lifetime, mirroring sefer-alloc's established
/// `dbg_reset_*` convention.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
pub fn reset_bench_internals_counters() {
    UNIX_EXACT_RESERVE_ATTEMPTS.store(0, Ordering::Relaxed);
    UNIX_EXACT_RESERVE_HITS.store(0, Ordering::Relaxed);
    WINDOWS_RESERVE_COMMIT_SINGLE_CALLS.store(0, Ordering::Relaxed);
    WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS.store(0, Ordering::Relaxed);
    WINDOWS_LARGE_PAGE_RETRY_FAILURES.store(0, Ordering::Relaxed);
    WINDOWS_LARGE_PAGE_PLAIN_FALLBACK_SUCCESSES.store(0, Ordering::Relaxed);
    WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES.store(0, Ordering::Relaxed);
    UNIX_MADVISE_ATTEMPTS.store(0, Ordering::Relaxed);
    UNIX_MADVISE_SUCCESSES.store(0, Ordering::Relaxed);
    UNIX_MUNMAP_FAILURES.store(0, Ordering::Relaxed);
    WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS.store(0, Ordering::Relaxed);
    WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES.store(0, Ordering::Relaxed);
    WINDOWS_VIRTUALFREE_RELEASE_FAILURES.store(0, Ordering::Relaxed);
    HUGE_DECOMMIT_ATTEMPTS.store(0, Ordering::Relaxed);
}
