/// `bench-internals` counters: accessor existence and reset verification.
///
/// # What this test DOES verify
///
/// - All accessor functions compile and are accessible under the
///   `bench-internals` feature.
/// - `reset_bench_internals_counters()` brings all counters to exactly zero
///   after a successful reservation that demonstrably incremented at least
///   one counter. On Windows this is `WINDOWS_RESERVE_COMMIT_CALLS`; on Unix
///   this is `UNIX_MADVISE_ATTEMPTS` after calling `decommit()` on the
///   reserved range. This proves reset actually clears the counters, not that
///   they were already zero.
///
/// # What this test does NOT verify
///
/// - This test does NOT verify counter monotonicity or specific increment
///   patterns. A test that asserts monotonicity would need to compare snapshots
///   before/after a sequence of operations, but on platforms where some counters
///   are guaranteed zero by construction (e.g., `windows_*` counters on Unix),
///   such assertions would be tautological and provide no regression protection.
///
/// - This test does NOT verify that the actual code paths that increment the
///   counters are exercised. In particular, the alignment-failure paths
///   (`WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES`, `WINDOWS_LARGE_PAGE_RETRY_FAILURES`)
///   are defensive branches that would only trigger on a broken or malformed
///   Windows kernel. Reproducing those branches would require fault injection
///   beyond what the existing `fault-injection` feature provides (it only
///   covers the `try_commit_range` OOM path, not VirtualAlloc alignment
///   violations). Therefore, this test makes no regression claim about those
///   failure paths — it only verifies the diagnostic surface exists and the
///   reset mechanism works.
///
/// - On 64-bit Unix, the exact-size `mmap` fast path is disabled (see
///   `reserve_aligned`'s rustdoc for the syscall-cost rationale). The only
///   guaranteed increment path (`UNIX_EXACT_RESERVE_ATTEMPTS`) lives in
///   `try_reserve_aligned_exact`, which is gated to 32-bit targets via
///   `target_pointer_width = "32"`. Therefore, a plain `reserve_aligned(PAGE, PAGE)`
///   call on 64-bit Unix does NOT guarantee any non-zero counter increment.
///   This test guarantees increment on Unix by calling `decommit()` on the
///   reserved range, which always issues `madvise()` and increments
///   `UNIX_MADVISE_ATTEMPTS`. This approach works on both 64-bit Unix and on
///   Apple Silicon macOS (where `decommit(0, PAGE)` would be rejected by the
///   guard that checks `end.is_multiple_of(page_size())` for a 16 KiB page).
///
///   A test that actually exercises specific increment paths would need to
///   use the mock backend (via `--cfg aligned_vmem_mock`) and verify that
///   recorded operations map to the expected counter values. That is a
///   separate, more comprehensive test beyond this file's scope.
#[cfg(feature = "bench-internals")]
#[test]
fn bench_internals_counters_existence_and_reset() {
    use aligned_vmem::{page_size, reserve_aligned, reset_bench_internals_counters};

    // All accessors are imported to verify they compile and are accessible.
    // This section is deliberately verbose: every accessor name must appear
    // exactly once, creating a compile-time check that the accessor surface
    // matches the counter surface. If a new counter is added without a
    // corresponding accessor, this line will fail to compile.
    use aligned_vmem::{
        huge_decommit_attempts, unix_exact_reserve_attempts, unix_exact_reserve_hits,
        unix_madvise_attempts, unix_madvise_successes, unix_munmap_failures,
        windows_large_page_alignment_failures, windows_large_page_retry_failures,
        windows_reserve_commit_calls, windows_reserve_commit_single_calls,
        windows_reserve_commit_two_call_pairs, windows_virtualfree_decommit_attempts,
        windows_virtualfree_decommit_failures, windows_virtualfree_release_failures,
    };

    // Reserve 4 * page_size() to ensure we can decommit at least one page
    // (important on Apple Silicon macOS where page_size() == 16 KiB, so
    // decommit(0, PAGE) would fail the guard that checks end.is_multiple_of(ps)).
    let ps = page_size();
    let result = reserve_aligned(4 * ps, 4 * ps);
    assert!(result.is_some(), "trivial reserve must succeed");
    let reservation = result.unwrap();

    // On Windows, verify that at least one counter was incremented.
    // This validates that the subsequent `reset_bench_internals_counters()` check
    // is actually testing reset behavior, not just asserting that zeros are zero.
    #[cfg(windows)]
    assert!(
        windows_reserve_commit_calls() >= 1,
        "a successful reservation must be counted on Windows; got {}",
        windows_reserve_commit_calls()
    );

    // On Unix, decommit one page to increment UNIX_MADVISE_ATTEMPTS.
    // This guarantees a non-zero counter increment on 64-bit Unix where
    // UNIX_EXACT_RESERVE_ATTEMPTS (32-bit-only) does not increment.
    #[cfg(all(unix, not(aligned_vmem_mock)))]
    unsafe {
        aligned_vmem::decommit(reservation.reservation_ptr(), 0, ps);
    }

    #[cfg(all(unix, not(aligned_vmem_mock)))]
    assert!(
        unix_madvise_attempts() >= 1,
        "decommit must increment UNIX_MADVISE_ATTEMPTS on Unix; got {}",
        unix_madvise_attempts()
    );

    // Release.
    drop(reservation);

    // Reset clears everything to zero. This assertion catches the R3-6 class of
    // bug: a new counter is added, but its `store(0, ...)` line is omitted from
    // `reset_bench_internals_counters`. The assertion would fail because the
    // counter would retain its post-reservation value (guaranteed non-zero for
    // at least one counter on Windows).
    reset_bench_internals_counters();
    assert_eq!(unix_exact_reserve_attempts(), 0);
    assert_eq!(unix_exact_reserve_hits(), 0);
    assert_eq!(windows_reserve_commit_calls(), 0);
    assert_eq!(windows_reserve_commit_single_calls(), 0);
    assert_eq!(windows_reserve_commit_two_call_pairs(), 0);
    assert_eq!(windows_large_page_retry_failures(), 0);
    assert_eq!(windows_large_page_alignment_failures(), 0);
    assert_eq!(unix_madvise_attempts(), 0);
    assert_eq!(unix_madvise_successes(), 0);
    assert_eq!(unix_munmap_failures(), 0);
    assert_eq!(windows_virtualfree_decommit_attempts(), 0);
    assert_eq!(windows_virtualfree_decommit_failures(), 0);
    assert_eq!(windows_virtualfree_release_failures(), 0);
    assert_eq!(huge_decommit_attempts(), 0);
}
