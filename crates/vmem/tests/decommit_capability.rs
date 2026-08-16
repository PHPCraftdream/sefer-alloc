//! Tests for the `decommit_reclaims_and_zeroes()` capability API and diagnostic
//! counter (findings R4-3 and R4-4).
//!
//! Finding II-16 (tests for public constructors/accessors without coverage) applied:
//! every public API surface added in this wave must have a test that would fail if the
//! implementation were broken or deleted.

use aligned_vmem::{
    reserve_aligned, reserve_aligned_huge, reset_bench_internals_counters, Reservation,
};
use std::sync::Mutex;

// Serial guard for bench-internals tests that read/write global counters
// (mirrors the pattern from tests/smoke.rs — see the comment block there for rationale).
static SERIAL: Mutex<()> = Mutex::new(());

const MIB: usize = 1024 * 1024;

/// Test that `decommit_reclaims_and_zeroes()` returns the correct compile-time
/// constant for the current platform (finding R4-3, II-16).
///
/// What breaks if this test is deleted: the cfg-based contract of
/// `decommit_reclaims_and_zeroes()` could drift from actual platform behavior
/// without any regression guard. A future maintainer adding a new target could
/// erroneously return `true` on a platform where decommit is advisory-only,
/// or `false` on a platform where it actually guarantees reclaim+zero-fill.
#[test]
fn decommit_reclaims_and_zeroes_matches_platform_cfg() {
    // On Linux and Windows, decommit is guaranteed to reclaim and zero-fill
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
    assert!(
        Reservation::decommit_reclaims_and_zeroes(),
        "Linux and Windows should guarantee reclaim+zero-fill semantics"
    );

    // On Darwin and BSD family, decommit is advisory-only with no guarantee
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    assert!(
        !Reservation::decommit_reclaims_and_zeroes(),
        "Darwin and BSD should NOT guarantee reclaim+zero-fill semantics"
    );
}

/// Test that `huge_decommit_attempts()` counter increments when decommit is called
/// on a huge-page reservation (finding R4-4, II-16).
///
/// What breaks if this test is deleted: the counter increment could be removed
/// or moved to the wrong place (e.g., outside the `is_huge()` check) without any
/// regression guard. The counter is the only observability mechanism for the
/// "decommit silently fails on huge reservations" problem; without this test,
/// that observability could vanish unnoticed.
#[test]
#[cfg(feature = "bench-internals")]
fn huge_decommit_attempts_increments_on_huge_reservation() {
    use aligned_vmem::huge_decommit_attempts;

    let _guard = SERIAL.lock();

    // Reset counters to get a clean baseline
    reset_bench_internals_counters();
    let baseline = huge_decommit_attempts();

    // Try to reserve with huge pages. This may fall back to ordinary pages
    // if huge pages are not available (no hugetlb pool, unprivileged, etc.),
    // but the counter should still increment if `is_huge()` reports true.
    let size = 2 * MIB; // Linux huge-page size
    let huge_r = reserve_aligned_huge(size, size);

    if let Some(ref reservation) = huge_r {
        if reservation.is_huge() {
            reservation.decommit(0, size);

            // Counter should have incremented by exactly 1
            assert_eq!(
                huge_decommit_attempts(),
                baseline + 1,
                "huge_decommit_attempts should increment by 1 for a decommit on a huge reservation"
            );

            // Test decommit_lazy as well — same counter path
            reset_bench_internals_counters();
            let baseline2 = huge_decommit_attempts();

            reservation.decommit_lazy(0, size);

            assert_eq!(
                huge_decommit_attempts(),
                baseline2 + 1,
                "huge_decommit_attempts should also increment by 1 for decommit_lazy on a huge reservation"
            );
        } else {
            // Fallback to ordinary pages — counter should NOT increment
            reservation.decommit(0, size);

            assert_eq!(
                huge_decommit_attempts(),
                baseline,
                "huge_decommit_attempts should NOT increment when is_huge() == false (fallback to ordinary pages)"
            );
        }
    } else {
        // Reservation failed entirely — no decommit was called, counter should stay at baseline
        assert_eq!(
            huge_decommit_attempts(),
            baseline,
            "huge_decommit_attempts should stay at baseline when reservation fails"
        );
    }
}

/// Test that `huge_decommit_attempts()` does NOT increment for ordinary reservations
/// (finding R4-4, II-16).
///
/// What breaks if this test is deleted: the counter increment could be moved outside
/// the `is_huge()` guard, causing it to increment for ALL decommits, not just
/// huge-page ones. This would break the counter's contract as an upper bound for
/// estimating the true huge-page incompatibility rate.
#[test]
#[cfg(feature = "bench-internals")]
fn huge_decommit_attempts_does_not_increment_on_ordinary_reservation() {
    use aligned_vmem::huge_decommit_attempts;

    let _guard = SERIAL.lock();

    reset_bench_internals_counters();
    let baseline = huge_decommit_attempts();

    // Reserve with ordinary pages — should never report as huge
    let ordinary_r = reserve_aligned(4 * MIB, 4 * MIB).expect("reserve 4 MiB");
    assert!(
        !ordinary_r.is_huge(),
        "ordinary reservation should never report as huge"
    );

    ordinary_r.decommit(0, ordinary_r.len());

    assert_eq!(
        huge_decommit_attempts(),
        baseline,
        "huge_decommit_attempts should NOT increment for ordinary reservations"
    );

    // Test decommit_lazy as well
    ordinary_r.decommit_lazy(0, ordinary_r.len());

    assert_eq!(
        huge_decommit_attempts(),
        baseline,
        "huge_decommit_attempts should NOT increment for decommit_lazy on ordinary reservations"
    );
}
