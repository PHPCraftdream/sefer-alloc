//! Tests for the `decommit_reclaims_and_zeroes()` capability API and diagnostic
//! counter (findings R4-3 and R4-4).
//!
//! Finding II-16 (tests for public constructors/accessors without coverage) applied:
//! every public API surface added in this wave must have a test that would fail if the
//! implementation were broken or deleted.

// Only `Reservation` is unconditionally available: `reserve_aligned_huge` is gated
// behind `huge-pages` and `reset_bench_internals_counters` behind `bench-internals`,
// so both are imported inside the gated tests that use them. A top-level unconditional
// `use` of either breaks the default-feature CI row
// (`cargo clippy -p aligned-vmem --all-targets -- -D warnings`) with E0432.
use aligned_vmem::Reservation;

// Serial guard for bench-internals tests that read/write global counters
// (mirrors the pattern from tests/smoke.rs — see the comment block there for rationale).
#[cfg(feature = "bench-internals")]
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

const MIB: usize = 1024 * 1024;

/// Test that `decommit_reclaims_and_zeroes()` returns the correct compile-time
/// constant for the current platform (finding R4-3, II-16).
///
/// What breaks if this test is deleted: the cfg-based contract of
/// `decommit_reclaims_and_zeroes()` could drift from actual platform behavior
/// without any regression guard. A future maintainer adding a new target could
/// erroneously return `true` on a platform where decommit is advisory-only,
/// or `false` on a platform where it actually guarantees reclaim+zero-fill.
/// The mock arm (task #1066) is the counterfactual for the cfg fix: dropping
/// `aligned_vmem_mock` from the exclusion list would silently regress the
/// query to `true` under the mock with no red test.
#[test]
fn decommit_reclaims_and_zeroes_matches_platform_cfg() {
    // On Linux and Windows (native backend), decommit is guaranteed to reclaim
    // and zero-fill
    #[cfg(all(
        any(target_os = "linux", target_os = "android", target_os = "windows"),
        not(miri),
        not(aligned_vmem_mock)
    ))]
    assert!(
        Reservation::decommit_reclaims_and_zeroes(),
        "Linux and Windows (native) should guarantee reclaim+zero-fill semantics"
    );

    // Under the `aligned_vmem_mock` cfg the recording backend's decommit never
    // touches the OS, so the capability must answer `false` even on a
    // Linux/Windows host (task #1066) — the same substitution the sibling query
    // `lazy_commit_is_honored()` already excludes.
    #[cfg(all(
        any(target_os = "linux", target_os = "android", target_os = "windows"),
        aligned_vmem_mock
    ))]
    assert!(
        !Reservation::decommit_reclaims_and_zeroes(),
        "under `--cfg aligned_vmem_mock`, decommit records without touching the OS: \
         the capability query must answer false"
    );

    // Under miri, the capability is false even on Linux/Windows targets
    #[cfg(all(
        any(target_os = "linux", target_os = "android", target_os = "windows"),
        miri
    ))]
    assert!(
        !Reservation::decommit_reclaims_and_zeroes(),
        "Under miri, capability should be false even on Linux/Windows targets"
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

/// Test that `can_decommit_reclaim_and_zero()` returns `false` for huge-page reservations
/// and equals the platform query for ordinary (fallback) reservations.
///
/// What breaks if this test is deleted:
/// - The instance-level query could incorrectly return `true` for huge-page reservations,
///   leading callers to believe decommit will work when it actually silently fails (R5-1, finding 3).
/// - The documented relationship (instance query = platform query && !is_huge) could be broken
///   for ordinary reservations, because this is the ONLY test that checks it on a real
///   reservation obtained via `reserve_aligned_huge` (which may fall back to ordinary pages).
///
/// Counterfactual for huge case: if the implementation returns `Self::decommit_reclaims_and_zeroes()`
/// (removing `&& !self.is_huge()`), this test fails on any host where `is_huge() == true`.
///
/// NOTE (updated task #1160/F4): The huge-page success path (the `if reservation.is_huge()`
/// branch) is NOT exercised on STANDARD CI runners — `ubuntu-latest` runners have no configured
/// hugetlb pool (`/proc/sys/vm/nr_hugepages` defaults to 0), and `windows-latest` runners lack
/// `SeLockMemoryPrivilege` — see item 59b in `docs/CORRECTNESS_OPEN_ITEMS.md` (the Windows half,
/// still fully open). The Linux half is narrower than it used to be: the dedicated
/// `aligned-vmem-hugetlb-real` CI job (`.github/workflows/ci.yml`) DOES configure a real
/// `nr_hugepages` pool and DOES run this test file (`--test decommit_capability`), so under
/// THAT job `reservation.is_huge()` is `true` here and the huge-page branch below executes for
/// real — see item 59a in `docs/CORRECTNESS_OPEN_ITEMS.md` for what that job proves and what it
/// still does not (the kernel-response question). On every OTHER runner (including the general
/// `test-workspace`/`aligned-vmem-gates` jobs, and Windows everywhere),
/// `reserve_aligned_huge` falls back to ordinary pages, so only the ordinary (fallback) case
/// below executes there. The test remains valuable as documentation of the contract and for the
/// rare host where huge pages are actually available.
///
/// Counterfactual for ordinary case: if the implementation returns `true` unconditionally
/// or writes `||` instead of `&&`, this test fails on Linux/Windows (where platform query returns `true`
/// but the instance query should return `false` for huge pages, and `true` for ordinary fallback).
#[test]
#[cfg(feature = "huge-pages")]
fn can_decommit_reclaim_and_zero_returns_false_for_huge_reservations() {
    use aligned_vmem::reserve_aligned_huge;

    // Try to reserve with huge pages. This may fall back to ordinary pages
    // if huge pages are not available (no hugetlb pool, unprivileged, etc.).
    let size = 2 * MIB; // Linux huge-page size
    let reservation = reserve_aligned_huge(size, size).expect("huge reservation (or fallback)");

    if reservation.is_huge() {
        // HUGE CASE: decommit never works, even on platforms where the native
        // backend guarantees it for ordinary reservations.
        assert!(
            !reservation.can_decommit_reclaim_and_zero(),
            "instance query must return false for huge-page reservations"
        );
    } else {
        // ORDINARY FALLBACK CASE: when huge pages are not available, the reservation
        // is ordinary and the instance query should equal the platform query.
        assert_eq!(
            reservation.can_decommit_reclaim_and_zero(),
            Reservation::decommit_reclaims_and_zeroes(),
            "instance query should equal platform query for ordinary (fallback) reservations"
        );
    }
}

/// Test that `can_decommit_reclaim_and_zero()` returns the platform capability
/// for ordinary (non-huge) reservations.
///
/// What breaks if this test is deleted: the instance-level query could diverge from
/// the platform-level query for ordinary reservations, breaking the documented
/// relationship (instance query = platform query && !is_huge). The mock arm
/// (task #1066) pins the instance query to the platform query's new `false`
/// answer under the `aligned_vmem_mock` cfg.
#[test]
fn can_decommit_reclaim_and_zero_matches_platform_for_ordinary_reservations() {
    use aligned_vmem::reserve_aligned;

    let ordinary_r = reserve_aligned(4 * MIB, 4 * MIB).expect("reserve 4 MiB");
    assert!(
        !ordinary_r.is_huge(),
        "ordinary reservation should never report as huge"
    );

    // For ordinary reservations, the instance query should match the platform query
    #[cfg(not(miri))]
    {
        #[cfg(all(
            any(target_os = "linux", target_os = "android", target_os = "windows"),
            not(aligned_vmem_mock)
        ))]
        assert!(
            ordinary_r.can_decommit_reclaim_and_zero(),
            "ordinary reservation on Linux/Windows should support reclaim+zero-fill"
        );

        // Under the mock cfg the platform query is false, so the instance query
        // must be too for this ordinary (non-huge) reservation (task #1066).
        #[cfg(all(
            any(target_os = "linux", target_os = "android", target_os = "windows"),
            aligned_vmem_mock
        ))]
        assert!(
            !ordinary_r.can_decommit_reclaim_and_zero(),
            "under `--cfg aligned_vmem_mock`, even ordinary reservations must report false"
        );

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
            !ordinary_r.can_decommit_reclaim_and_zero(),
            "ordinary reservation on Darwin/BSD should NOT support reclaim+zero-fill"
        );
    }

    // Under miri, even ordinary reservations should report false
    #[cfg(miri)]
    assert!(
        !ordinary_r.can_decommit_reclaim_and_zero(),
        "under miri, even ordinary reservations should report false"
    );
}

/// Test that `huge_decommit_attempts()` counter increments when decommit is called
/// on a huge-page reservation with a range that is NOT eligible for the
/// Linux/Android >= 5.18 real-backend path (finding R4-4, II-16; range
/// narrowed task #1140 — see the regression note below).
///
/// What breaks if this test is deleted: the counter increment could be removed
/// or moved to the wrong place (e.g., outside the `is_huge()` check) without any
/// regression guard. The counter is the only observability mechanism for the
/// "decommit silently fails on huge reservations" problem; without this test,
/// that observability could vanish unnoticed.
///
/// **Regression note (task #1140, discovered during this task's own
/// verification):** this test originally called `reservation.decommit(0,
/// size)` with `size == 2 MiB == LINUX_HUGE_PAGE_SIZE` inside the
/// `is_huge()` arm, then asserted the skip counter incremented. Since task
/// #1140, that exact call is now an ELIGIBLE range on a sufficiently recent
/// Linux/Android kernel (it takes the real `MADV_DONTNEED` backend path
/// instead of skipping), so the counter would correctly stay at `baseline`
/// instead of reaching `baseline + 1` — a false failure of correct behavior
/// on any real hugetlb-pool host running such a kernel. No CI runner in this
/// repo configures such a pool today, so this specific failure has never been
/// observed in CI (the `if` arm has never executed there), but it WAS
/// reproduced for real on a manually-provisioned WSL2/Linux kernel 6.18 host
/// with the huge flag genuinely synthesized via `from_raw_parts` (see
/// `simulated_huge_flag_drives_the_same_branch_dispatch_on_any_host` below)
/// during this task's own verification pass. The `decommit(0, ps)` probe
/// below (page-aligned, in-bounds, but never a 2-MiB multiple) stays on the
/// skip path unconditionally, closing the gap before a real hugetlb runner
/// ever exercises it. The eligible-range real-backend case has its own
/// dedicated coverage:
/// `huge_aligned_range_takes_the_real_backend_path_not_the_skip_path` below.
#[test]
#[cfg(all(feature = "bench-internals", feature = "huge-pages"))]
fn huge_decommit_attempts_increments_on_huge_reservation() {
    use aligned_vmem::{
        huge_decommit_attempts, page_size, reserve_aligned_huge, reset_bench_internals_counters,
    };

    let _guard = SERIAL.lock();

    // Reset counters to get a clean baseline
    reset_bench_internals_counters();
    let baseline = huge_decommit_attempts();

    // Try to reserve with huge pages. This may fall back to ordinary pages
    // if huge pages are not available (no hugetlb pool, unprivileged, etc.),
    // but the counter should still increment if `is_huge()` reports true.
    let size = 2 * MIB; // Linux huge-page size
    let mut reservation = reserve_aligned_huge(size, size).expect("huge reservation (or fallback)");
    let ps = page_size();

    if reservation.is_huge() {
        // `ps` (not `size`): a non-2-MiB-aligned but still well-formed range,
        // guaranteed to take the skip path on every platform/kernel (task
        // #1140) — see this test's own doc comment for why `size` would not.
        reservation.decommit(0, ps);

        // Counter should have incremented by exactly 1
        assert_eq!(
            huge_decommit_attempts(),
            baseline + 1,
            "huge_decommit_attempts should increment by 1 for a non-2-MiB-aligned \
             decommit call on a huge reservation"
        );

        // Test decommit_lazy as well — same counter path. Unlike eager
        // `decommit`, `decommit_lazy` was NOT extended by task #1140 (see
        // that method's own doc comment: `MADV_FREE` has no documented
        // HugeTLB support), so it always takes the skip path regardless of
        // range — the full `size` span is fine to use here.
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

        // Test decommit_lazy as well
        reservation.decommit_lazy(0, size);

        assert_eq!(
            huge_decommit_attempts(),
            baseline,
            "huge_decommit_attempts should NOT increment for decommit_lazy when is_huge() == false"
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
    use aligned_vmem::{huge_decommit_attempts, reserve_aligned, reset_bench_internals_counters};

    let _guard = SERIAL.lock();

    reset_bench_internals_counters();
    let baseline = huge_decommit_attempts();

    // Reserve with ordinary pages — should never report as huge
    let mut ordinary_r = reserve_aligned(4 * MIB, 4 * MIB).expect("reserve 4 MiB");
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

/// Task #1140: on Linux/Android kernel >= 5.18, a huge-page-aligned range on a
/// GENUINELY huge reservation must take the REAL backend path (`MADV_DONTNEED`
/// actually issued), not the silent-skip path — the reverse of what
/// `huge_decommit_attempts_increments_on_huge_reservation` above pins for the
/// pre-#1140 behavior. This test asserts exactly the opposite counter
/// direction from that test, on the same `is_huge()` branch, for a range that
/// is additionally 2-MiB-aligned at both endpoints.
///
/// **Counterfactual:** reverting the `reservation.rs` change from task #1140
/// (restoring the unconditional `if self.is_huge() { ...; return; }` skip)
/// makes this test fail: `huge_decommit_attempts()` would increment by 1 for
/// EVERY call in this test, including the 2-MiB-aligned one, so the first
/// assertion below (`baseline` unchanged after the aligned call) would read
/// `baseline + 1` instead and fail.
///
/// **Execution honesty (per this task's own brief; updated task #1160/F4):**
/// this assertion only actually exercises the new code path when
/// `reservation.is_huge()` is `true`, which requires a REAL hugetlb pool
/// (`/proc/sys/vm/nr_hugepages > 0`). At the time this test was authored, no
/// runner in this repo's CI configured one (see item 59 in
/// `docs/CORRECTNESS_OPEN_ITEMS.md`, and the same caveat on
/// `can_decommit_reclaim_and_zero_returns_false_for_huge_reservations`
/// above). **That has changed:** the `aligned-vmem-hugetlb-real` CI job
/// (`.github/workflows/ci.yml`) now configures a real `nr_hugepages` pool
/// and runs this exact test (this function is one of its five
/// hard-asserted sentinels), so under THAT job `reservation.is_huge()` is
/// `true` and this test genuinely exercises the real-backend branch —
/// proving the crate's dispatch reaches the real
/// `madvise(2)`/`MADV_DONTNEED` call, not that the kernel honoured it (see
/// `decommit`'s own rustdoc for that distinction). On every OTHER host this
/// crate's CI runs on (including the general `test-workspace` job, and the
/// Windows host this test was originally authored on), `reserve_aligned_huge`
/// still falls back to ordinary pages, `is_huge()` is `false`, and this test
/// exercises only the `else` arm (already covered by the ordinary-reservation
/// tests above) — it is NOT a false pass there either, it is an honest skip
/// of the new-behavior assertion, matching this file's own pre-existing
/// pattern for the same structural reason.
#[test]
#[cfg(all(feature = "bench-internals", feature = "huge-pages"))]
fn huge_aligned_range_takes_the_real_backend_path_not_the_skip_path() {
    use aligned_vmem::{
        huge_decommit_attempts, reserve_aligned_huge, reset_bench_internals_counters,
    };

    let _guard = SERIAL.lock();

    let size = 2 * MIB; // exactly one huge page: 2-MiB-aligned at both endpoints by construction.
    let mut reservation = reserve_aligned_huge(size, size).expect("huge reservation (or fallback)");

    if !reservation.is_huge() {
        // Honest skip: see the doc comment above for why this is expected on
        // every host without a real hugetlb pool, including this task's own
        // Windows authoring host.
        return;
    }

    // The full reservation span [0, size) is exactly one 2-MiB huge page:
    // both endpoints are multiples of 2 MiB by construction (size == 2 MiB).
    reset_bench_internals_counters();
    let baseline = huge_decommit_attempts();

    reservation.decommit(0, size);

    assert_eq!(
        huge_decommit_attempts(),
        baseline,
        "a 2-MiB-aligned range on a genuinely huge reservation must NOT hit the \
         skip-counter path on Linux/Android kernel >= 5.18 — it must forward to \
         the real MADV_DONTNEED backend instead (task #1140)"
    );

    // A `page_size()`-granular but NOT 2-MiB-granular sub-range (e.g. the
    // first 4 KiB) is NOT huge-aligned and must still take the skip path,
    // exactly like the pre-#1140 behavior.
    let ps = aligned_vmem::page_size();
    if ps < size {
        reset_bench_internals_counters();
        let baseline2 = huge_decommit_attempts();

        reservation.decommit(0, ps);

        assert_eq!(
            huge_decommit_attempts(),
            baseline2 + 1,
            "a page_size()-granular but non-2-MiB-granular range on a huge \
             reservation must still take the silent-skip path (EINVAL territory)"
        );
    }
}

/// Task #1140, HOST-INDEPENDENT branch-dispatch proof: the two tests above
/// (`huge_aligned_range_takes_the_real_backend_path_not_the_skip_path` and its
/// sibling in `huge_decommit_attempts_increments_on_huge_reservation`) can only
/// exercise their real assertions on a host with a configured hugetlb pool —
/// on every other host, including this task's own Windows authoring host, they
/// silently no-op. This test proves the SAME `is_huge()`-plus-range-alignment
/// BRANCH DISPATCH logic in `Reservation::decommit`/`try_decommit` without
/// needing a real hugetlb pool, by fabricating `is_huge() == true` over an
/// ORDINARY (non-`MAP_HUGETLB`) mapping via the documented
/// `into_full_parts`/`from_raw_parts` round-trip.
///
/// **Why this is a sound (not unsound) use of `from_raw_parts`:** the
/// constructor's own `# Safety` section requires `granted_huge` to accurately
/// reflect the OS grant, and this test deliberately violates that for
/// `is_huge()`'s OBSERVABLE VALUE — but `granted_huge` has NO effect on any
/// unsafe operation `from_raw_parts`/`Drop`/`release_reservation` performs
/// (verified by reading every `granted_huge` use site in
/// `src/reservation.rs`/`src/os/unix.rs`/`src/os/windows.rs`: it is stored and
/// read back verbatim, never branched on by any pointer-unsafe code path —
/// `munmap`/`VirtualFree` care only about `reservation`/`reservation_len`/
/// `align`, never about `granted_huge`). The ONLY consumers of `is_huge()`
/// are `Reservation::decommit`/`try_decommit`'s branch dispatch (exactly what
/// this test exercises) and the two capability-query methods (not exercised
/// here) — both operate purely on already-validated, in-bounds byte ranges of
/// a live mapping, so fabricating this one bool cannot cause memory unsafety.
///
/// **What this test DOES prove:** the Rust-level decision of "does
/// `Reservation::decommit`'s huge branch call the real backend, or take the
/// silent-skip + counter-increment path" is driven by `is_huge()` AND
/// `linux_huge_range_is_madvise_eligible`'s range check — exactly the new
/// logic task #1140 added — regardless of whether the underlying mapping is
/// truly `MAP_HUGETLB`.
///
/// **What this test does NOT prove:** that a REAL `MAP_HUGETLB` mapping's
/// `madvise(MADV_DONTNEED)` call actually succeeds/zeroes on a real
/// Linux >= 5.18 kernel — an ordinary (non-hugetlb) anonymous mapping accepts
/// `MADV_DONTNEED` at ANY granularity on EVERY Linux kernel version (this is
/// not new to 5.18; only `MAP_HUGETLB` mappings had the granularity
/// restriction this task's fix is about), so this test's underlying syscall
/// always succeeds regardless of host kernel version — it is not evidence for
/// the kernel-version-gated HugeTLB claim itself, only for the branch-dispatch
/// logic around it. That claim remains REASONED-FROM-SPEC per `man 2 madvise`,
/// as stated throughout this task's doc changes and its own final report.
#[test]
#[cfg(all(feature = "bench-internals", feature = "huge-pages"))]
fn simulated_huge_flag_drives_the_same_branch_dispatch_on_any_host() {
    use aligned_vmem::{
        huge_decommit_attempts, reserve_aligned, reset_bench_internals_counters, Reservation,
        ReservationFullParts,
    };

    let _guard = SERIAL.lock();

    let size = 2 * MIB;
    let ordinary = reserve_aligned(size, size).expect("reserve 2 MiB ordinary");
    assert!(!ordinary.is_huge(), "sanity: ordinary reservation");

    let mut parts: ReservationFullParts = ordinary.into_full_parts();
    assert!(
        !parts.granted_huge,
        "sanity: parts carry the real (false) flag"
    );
    // Fabricate the huge flag — see the doc comment above for why this is a
    // sound test-only use of `from_raw_parts`'s unsafe contract.
    parts.granted_huge = true;
    // SAFETY: `parts` came from a real, live `into_full_parts()` call on a
    // reservation this test still exclusively owns (not yet dropped/released);
    // only `granted_huge` was mutated, and — per the doc comment above — no
    // unsafe operation this crate performs branches on that field. `base`,
    // `len`, `reservation`, `reservation_len`, `align` are all exactly the
    // values the real reservation produced, so every OTHER `from_raw_parts`
    // invariant holds unchanged.
    let mut simulated_huge: Reservation = unsafe { parts.into_reservation() };
    assert!(
        simulated_huge.is_huge(),
        "sanity: the fabricated flag round-tripped"
    );

    reset_bench_internals_counters();
    let baseline = huge_decommit_attempts();

    // Whole-span range: size == 2 MiB, so [0, size) is 2-MiB-aligned at both
    // endpoints — eligible under `linux_huge_range_is_madvise_eligible`.
    simulated_huge.decommit(0, size);

    #[cfg(all(
        not(miri),
        not(aligned_vmem_mock),
        any(target_os = "linux", target_os = "android")
    ))]
    assert_eq!(
        huge_decommit_attempts(),
        baseline,
        "on Linux/Android (native, non-mock), a 2-MiB-aligned range on a \
         (simulated) huge reservation must take the real-backend path, not \
         the skip-counter path"
    );
    // On every other platform (Windows, Darwin/BSD, miri, aligned_vmem_mock),
    // task #1140 made no behavior change: the huge branch always takes the
    // skip path regardless of range.
    #[cfg(not(all(
        not(miri),
        not(aligned_vmem_mock),
        any(target_os = "linux", target_os = "android")
    )))]
    assert_eq!(
        huge_decommit_attempts(),
        baseline + 1,
        "on non-Linux/Android platforms, every range on a huge reservation \
         still takes the silent-skip path unconditionally (unchanged by task #1140)"
    );

    // Prevent Drop from trying to munmap/VirtualFree a region this reservation
    // no longer exclusively owns any special claim over — it does, in fact,
    // still exclusively own the real mapping (only the `granted_huge` VALUE
    // was fabricated, not the underlying memory), so a normal drop is correct
    // and releases the real mapping exactly once. No `into_parts`/`forget`
    // dance is needed here; `simulated_huge` drops normally at end of scope.
}

/// Task #1140: `Reservation::try_decommit`'s validate-before-huge-skip
/// ordering (task #1084/M3) must still hold when the huge-aligned real-call
/// path is reachable — an INVERTED range that happens to be huge-page-aligned
/// at both endpoints (`start = 2 * size, end = size` for a `size`-byte
/// reservation, both multiples of `LINUX_HUGE_PAGE_SIZE`) must be rejected as
/// `Err` by the bounds check before ever reaching the eligibility check, on
/// every platform and every `is_huge()` value — this test does not require a
/// real hugetlb pool because the bounds check (`end > self.len()`) rejects it
/// unconditionally, before `is_huge()` is even consulted.
///
/// **Counterfactual:** if the bounds/validation checks in `try_decommit` were
/// ever reordered to run AFTER the huge-eligibility check (the exact bug
/// class task #1084/M3 already fixed once for the huge-skip branch), this
/// range could reach `linux_huge_range_is_madvise_eligible(2*size, size)` —
/// which itself now rejects `start > end` (see that function's own doc) — but
/// a caller relying on the OUTER bounds check firing first would see a
/// different error path. This test pins the outer bounds check as the first
/// gate regardless of internal eligibility-check details.
#[test]
#[cfg(feature = "huge-pages")]
fn try_decommit_rejects_out_of_bounds_huge_aligned_range_before_any_eligibility_check() {
    use aligned_vmem::reserve_aligned_huge;

    let size = 2 * MIB;
    let mut reservation = reserve_aligned_huge(size, size).expect("huge reservation (or fallback)");

    // start = 2*size, end = size: both are multiples of size (== LINUX_HUGE_PAGE_SIZE
    // when huge pages are granted), but end > reservation.len() == size, and
    // start > end — a doubly-invalid range regardless of is_huge().
    let out = reservation.try_decommit(2 * size, size);
    assert!(
        out.is_err(),
        "an out-of-bounds, inverted range must be rejected regardless of huge-page \
         status or endpoint alignment"
    );
}

/// Task #1152 (F1): path-activation oracle for the `aligned-vmem-hugetlb-real`
/// CI job (`.github/workflows/ci.yml`).
///
/// Every huge-decommit test above (`huge_decommit_attempts_increments_on_huge_reservation`,
/// `huge_aligned_range_takes_the_real_backend_path_not_the_skip_path`) is
/// deliberately host-adaptive: `if reservation.is_huge() { <real assertions> }
/// else { return; }`. That is the right shape for a test that must also pass
/// on a host with no hugetlb pool (every dev machine, `windows-latest`,
/// `ubuntu-latest` without the pool configured) — but it means NONE of them
/// can, by themselves, prove that a CI job which claims to grant a real
/// hugetlb pool actually did. A job that configures `nr_hugepages=64` but
/// whose `MAP_HUGETLB` mmap still silently fails (cgroup limit, NUMA
/// placement, a future runner-image change) would still show every one of
/// those tests as `ok`, because they all take the documented ordinary-page
/// fallback branch — the exact "green and dead" failure mode CLAUDE.md's
/// R30-8 rule (path-activation oracle) exists to close.
///
/// This test is the oracle: gated behind an env var
/// (`ALIGNED_VMEM_REQUIRE_REAL_HUGETLB=1`) that only the
/// `aligned-vmem-hugetlb-real` job sets, it refuses the fallback outcome
/// outright. Unset (every other environment, including a developer's own
/// machine and every other CI job), it no-ops immediately — it is not a
/// general-purpose "prove huge pages work" test, only a tripwire for the one
/// job that is supposed to guarantee a real pool.
///
/// **Counterfactual:** if the runner's hugetlb pool silently stops actually
/// backing `MAP_HUGETLB` allocations (while `/proc/sys/vm/nr_hugepages`
/// still reads back a nonzero value, so the job's own pool-configuration
/// step keeps passing), `reserve_aligned_huge` falls back to ordinary pages,
/// `is_huge()` is `false`, and this test's `assert!` fires — turning the job
/// red instead of green-and-silent. Validated by forcing the same assertion
/// against a real fallback outcome on a non-hugetlb host (this task's own
/// verification; see the task's final report for how, since this repository
/// has no way to grant a real hugetlb pool on Windows or in this sandbox).
#[test]
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    feature = "huge-pages"
))]
fn ci_hugetlb_real_pool_oracle_refuses_ordinary_page_fallback() {
    use aligned_vmem::reserve_aligned_huge;

    if std::env::var("ALIGNED_VMEM_REQUIRE_REAL_HUGETLB").as_deref() != Ok("1") {
        // Not running inside the `aligned-vmem-hugetlb-real` job: the
        // ordinary-page fallback is expected and acceptable everywhere else.
        return;
    }

    let size = 2 * MIB; // Linux/Android huge-page size.
    let reservation = reserve_aligned_huge(size, size)
        .expect("huge reservation request must succeed (real grant or fallback)");

    assert!(
        reservation.is_huge(),
        "ALIGNED_VMEM_REQUIRE_REAL_HUGETLB=1 is set (this must only be true inside the \
         `aligned-vmem-hugetlb-real` CI job), so a `reserve_aligned_huge` request took the \
         ordinary-page fallback instead of a real MAP_HUGETLB grant. That job's whole purpose is \
         to prove the crate's huge-decommit path executes against a REAL hugetlb pool — a \
         silent fallback here means every other test in this run that checks is_huge() before its \
         real assertions also silently skipped them, and the job would otherwise report success \
         while proving nothing. Check `/proc/sys/vm/nr_hugepages` \
         and whether the runner's cgroup/NUMA configuration still permits MAP_HUGETLB."
    );
}
