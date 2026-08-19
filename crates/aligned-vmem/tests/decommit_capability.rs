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
/// and runs this exact test (this function is one of its six
/// hard-asserted `test <name> ... ok` sentinels — plus two literal
/// `[oracle] ARMED: ...` marker sentinels, eight `grep -F` checks in total
/// as of task #1166's recount), so under THAT job `reservation.is_huge()` is
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
/// constructor's own "Correctness contract" section (task #1172/M3 split;
/// previously part of `# Safety` before that split) requires `granted_huge`
/// to accurately reflect the OS grant, and this test deliberately violates
/// that for `is_huge()`'s OBSERVABLE VALUE — but `granted_huge` has NO effect
/// on any unsafe operation `from_raw_parts`/`Drop`/`release_reservation`
/// performs (verified by reading every `granted_huge` use site in
/// `src/reservation.rs`/`src/os/unix.rs`/`src/os/windows.rs`: it is stored and
/// read back verbatim, never branched on by any pointer-unsafe code path —
/// `munmap`/`VirtualFree` care only about `reservation`/`reservation_len`/
/// `align`, never about `granted_huge`). Since task #1172, `from_raw_parts`
/// ALSO reads the raw `granted_huge` PARAMETER (before it becomes a field) in
/// two Correctness-contract `assert!`s (`huge-pages`-feature-required, and —
/// on Linux/Android — the 2-MiB-multiple requirement); this test's `size ==
/// 2 * MIB` base reservation and its `huge-pages` feature gate (see this
/// test's own `#[cfg]`) satisfy both, same reasoning as
/// `reservation_decommit_contract.rs`'s sibling test. Both `assert!`s are
/// safe Rust — not pointer-unsafe operations — so this does not weaken the
/// claim above. The ONLY consumers of `is_huge()` itself (the accessor, as
/// opposed to the raw parameter) are `Reservation::decommit`/`try_decommit`'s
/// branch dispatch (exactly what this test exercises) and the two
/// capability-query methods (not exercised here) — both operate purely on
/// already-validated, in-bounds byte ranges of
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
///
/// **Task #1162 — arming itself was pinned by nothing.** Before this task,
/// the ARMED (`ALIGNED_VMEM_REQUIRE_REAL_HUGETLB=1` set, real grant
/// confirmed) and UNARMED (var unset or wrong, early `return`) outcomes both
/// printed the identical libtest line `test
/// ci_hugetlb_real_pool_oracle_refuses_ordinary_page_fallback ... ok` — so a
/// future edit that dropped the env var (e.g. a bad `env:` block indent, or
/// a merge that lost the prefix) would make this oracle silently no-op on
/// every run, and nothing in ci.yml, `scripts/verify-ci-sentinels.mjs`, or
/// this test itself would go red: every huge test would quietly take its
/// `if reservation.is_huge()` fallback branch, all five sentinels the
/// `aligned-vmem-hugetlb-real` job checks would still match, and the job
/// would report success while proving zero hugetlb coverage. Closed by
/// making ARMED observably different from UNARMED in the OUTPUT itself: a
/// `println!("[oracle] ARMED: ...")` after the assert above, which only
/// executes on the real-grant path, checked by its own additional `grep -F`
/// sentinel in ci.yml. **Task #1166 correction:** this marker is observed by
/// running THIS test alone (`--exact <name> -- --nocapture`), not by adding
/// `--nocapture` to the job's shared multi-test run — `--nocapture` does not
/// change libtest's `test <name> ... ok` line FORMAT, but under the default
/// parallel runner it does NOT print that line atomically either: the
/// aggregating main thread writes `"test {name} ... "`, the outcome word,
/// and the trailing newline as three separate writes, and an unsynchronized
/// worker-thread `println!` (this marker, or the sibling oracle's) can land
/// between them and split another test's sentinel line — confirmed by a
/// 400-run counterfactual (11/400 corrupted) documented in
/// `.github/workflows/ci.yml`'s `aligned-vmem-hugetlb-real` step. This
/// checks EXECUTION, not workflow text, so it cannot be defeated by moving
/// the env var to a different syntactic position in the YAML.
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

    // Task #1162: the ARMED/unarmed outcomes above are otherwise
    // indistinguishable in CI's libtest output — both print exactly
    // `test ci_hugetlb_real_pool_oracle_refuses_ordinary_page_fallback ... ok`,
    // so nothing pinned that `ALIGNED_VMEM_REQUIRE_REAL_HUGETLB=1` was ever
    // actually set before this test ran (a future edit that drops the env
    // var prefix, e.g. by moving it into a malformed `env:` block, would
    // make this test silently no-op at the `return` above and still print
    // that same "... ok" line). This print only executes past the assert
    // above, so it is proof the real-grant branch was taken, not merely
    // that the process started with the right variable — and it is a
    // distinct, additional line `grep -F`-checked by
    // `.github/workflows/ci.yml`'s `aligned-vmem-hugetlb-real` job,
    // verified via `scripts/verify-ci-sentinels.mjs`. At task #1162 filing
    // time this required the job's shared `cargo test` invocation to pass
    // `-- --nocapture`; task #1166 found that flag was corrupting its own
    // sentinel lines under the default parallel runner (an unsynchronized
    // worker-thread `println!` landing mid-write of another test's `test
    // <name> ... ` / outcome-word / newline triple) and moved this marker's
    // observation to its own isolated `--exact <name> -- --nocapture`
    // invocation instead — see that job step's own comment in ci.yml for
    // the counterfactual. `--nocapture` itself does not change libtest's
    // `test <name> ... ok` line FORMAT, only whether output interleaves
    // with it; the five pre-existing sentinels in that job's step (at task
    // #1162 filing time) were unaffected by adding the flag.
    println!("[oracle] ARMED: real MAP_HUGETLB grant confirmed");
}

/// Task #1164 (item 59a's own next-trigger, task #1160/F5): the KERNEL-RESPONSE
/// half of item 59a — closes the one gap `ci_hugetlb_real_pool_oracle_refuses_
/// ordinary_page_fallback` above deliberately leaves open. That oracle proves
/// (a) a real `MAP_HUGETLB` grant was obtained and (b) the eligible-range
/// decommit dispatch reaches the real backend call — but `libc_madvise`
/// (`src/os/unix.rs`) discards the syscall's own return value by design (task
/// #719), so nothing observes whether the KERNEL actually accepted the call
/// (`madvise(2)` returning `0`) versus rejecting it (`-1`). This test is that
/// observation, for the one case where a rejection is a genuine defect rather
/// than a tolerated OS refusal: an eligible (huge-page-size-aligned) range,
/// decommitted eagerly, against a freshly-confirmed-real `MAP_HUGETLB` grant,
/// inside the `aligned-vmem-hugetlb-real` job specifically.
///
/// **Why a `madvise` failure HERE is treated as a red build, unlike the
/// crate's general contract (`decommit`'s own rustdoc) that OS refusal is
/// non-erroneous:** the crate's tolerance for refusal exists for conditions
/// this test's environment does not have — cgroup memory limits, memory
/// pressure, an unsupported kernel. `man 2 madvise` documents
/// `MADV_DONTNEED`-on-HugeTLB support from Linux 5.18 (see
/// `linux_huge_range_is_madvise_eligible`'s own doc comment in
/// `src/os/unix.rs`); `ubuntu-latest` (this job's `runs-on`) ships a kernel
/// far newer than that baseline. The job's own pool-configuration step
/// hard-fails (not skips) if `nr_hugepages` cannot be raised to at least 8
/// (`.github/workflows/ci.yml`), and the oracle immediately above this test
/// already hard-asserts the grant was real, not a fallback. Under those three
/// preconditions — modern kernel, a genuinely configured pool, a real grant —
/// there is no realistic legitimate-refusal path left for `MADV_DONTNEED` on
/// a huge-page-size-aligned range: a `-1` return here would mean the kernel
/// or the pool configuration silently regressed, which is exactly the class
/// of defect this job exists to surface as red rather than green-and-dead.
///
/// **Vacuous-pass analysis — every path this test could report `ok` without
/// having proven anything, and how each is closed:**
/// 1. **Env var unset (every host except this one CI job):** the same early
///    `return` as the oracle immediately above — a genuine, honest no-op, not
///    a claim of anything proven. Gated identically, for the identical
///    reason.
/// 2. **`#[cfg]` excluding this function entirely:** requires
///    `feature = "bench-internals"` (the counters this test reads do not
///    exist without it — see below), `feature = "huge-pages"` (the real
///    dispatch path does not exist without it), and
///    `target_os = "linux"`/`"android"` (the only platforms
///    `linux_huge_range_is_madvise_eligible` compiles for) — the same three
///    gates the oracle above already requires, so this test can never run
///    somewhere the oracle itself would not also run.
/// 3. **`bench-internals` off:** `unix_madvise_attempts`/`unix_madvise_successes`
///    are themselves `#[cfg(feature = "bench-internals")]`-gated re-exports
///    (`src/lib.rs`) — calling them without the feature is a compile error
///    (E0432 unresolved import), not a silent no-op. This function's own
///    `#[cfg]` includes the same feature, so the whole test (not just the
///    calls) is absent from the binary when the feature is off — it cannot
///    exist to vacuously pass. (Confirmed by this task's own
///    `--features huge-pages` clippy run below: this file compiles clean
///    with the test simply not present.)
/// 4. **The reservation falling back to ordinary pages:** hard-asserted via
///    `is_huge()` BEFORE the decommit call, with a panic message identical in
///    spirit to the oracle's own — this test does not assume the grant from
///    the oracle's run persists across test-binary process boundaries (it
///    does not; each `#[test]` fn in the same binary runs in the same
///    process but makes its OWN `reserve_aligned_huge` call), so it re-proves
///    the grant itself rather than relying on the earlier oracle having run
///    first (libtest does not guarantee ordering, and does not guarantee
///    both tests run in the same invocation at all).
/// 5. **The range being ineligible so the crate early-exits before any
///    syscall:** `size = 2 * MIB` and the full span `[0, size)` is exactly
///    one huge page — both endpoints are `LINUX_HUGE_PAGE_SIZE` multiples by
///    construction (mirrors `huge_aligned_range_takes_the_real_backend_path_
///    not_the_skip_path` above), so `linux_huge_range_is_madvise_eligible`
///    is `true` and the eager `decommit` call is guaranteed to reach
///    `libc_madvise`, not the skip-and-count path.
/// 6. **Another test perturbing the counters concurrently:** `SERIAL` (this
///    file's shared `Mutex<()>`, held for the same reason `tests/smoke.rs`'s
///    `macos_decommit_madvise_syscall_actually_succeeds` holds its own) is
///    locked for this test's entire body, and `reset_bench_internals_counters()`
///    is called AFTER acquiring the lock but BEFORE the decommit call, so the
///    baseline this test reads cannot be contaminated by a concurrently
///    running test in the same process (this file's other `#[test]` fns that
///    touch these counters all join the same `SERIAL` contract).
///
/// **Counterfactual (built OUTSIDE this repo, since a real hugetlb pool is
/// not available in this sandbox on Windows):** substituted `libc_madvise`'s
/// `UNIX_MADVISE_SUCCESSES.fetch_add` call with a no-op (simulating the
/// kernel returning `-1` on every call) in a scratch copy of `src/os/unix.rs`
/// outside this worktree, then ran an equivalent ordinary (non-huge)
/// `reserve_aligned`/`decommit`/counter-assert sequence on this Windows
/// fallback host. The assertion below (`successes > baseline_successes`)
/// failed exactly as expected, with the message below, confirming the
/// assertion is not tautological. **What this substitution does NOT
/// establish:** it does not exercise the real `MAP_HUGETLB`-plus-eligible-
/// range dispatch this test targets (Windows has no such path at all), so it
/// only proves the ASSERTION LOGIC is sound, not that this specific test body
/// would fail the same way on a real Linux hugetlb host with a genuinely
/// broken kernel/pool — that confirmation can only come from a real
/// `aligned-vmem-hugetlb-real` CI run with the counter regressed, which this
/// task did not have the means to force.
#[test]
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    feature = "huge-pages",
    feature = "bench-internals"
))]
fn ci_hugetlb_real_pool_kernel_actually_accepts_eligible_madvise() {
    use aligned_vmem::{
        reserve_aligned_huge, reset_bench_internals_counters, unix_madvise_attempts,
        unix_madvise_successes,
    };

    if std::env::var("ALIGNED_VMEM_REQUIRE_REAL_HUGETLB").as_deref() != Ok("1") {
        // Not running inside the `aligned-vmem-hugetlb-real` job: honest
        // no-op, matching `ci_hugetlb_real_pool_oracle_refuses_ordinary_
        // page_fallback` immediately above — see that test's doc comment
        // and this test's own vacuous-pass analysis point 1.
        return;
    }

    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let size = 2 * MIB; // Linux/Android huge-page size; the full span is exactly one eligible huge page.
    let mut reservation = reserve_aligned_huge(size, size)
        .expect("huge reservation request must succeed (real grant or fallback)");

    assert!(
        reservation.is_huge(),
        "ALIGNED_VMEM_REQUIRE_REAL_HUGETLB=1 is set (this must only be true inside the \
         `aligned-vmem-hugetlb-real` CI job), so a `reserve_aligned_huge` request took the \
         ordinary-page fallback instead of a real MAP_HUGETLB grant -- this test cannot \
         measure the kernel's madvise(2) response on a mapping that was never granted as \
         HugeTLB in the first place. See `ci_hugetlb_real_pool_oracle_refuses_ordinary_page_\
         fallback`'s identical assertion above for the same failure's root cause."
    );

    // Clean slate: this test's own decommit call must be the only
    // contributor to the counters it reads below (see vacuous-pass analysis
    // point 6 above).
    reset_bench_internals_counters();
    let attempts_before = unix_madvise_attempts();
    let successes_before = unix_madvise_successes();

    // The full reservation span is exactly one 2-MiB huge page: both
    // endpoints are LINUX_HUGE_PAGE_SIZE multiples by construction, so this
    // is an ELIGIBLE range (see vacuous-pass analysis point 5 above) and
    // must reach the real `libc_madvise` call, not the skip-and-count path.
    reservation.decommit(0, size);

    let attempts_after = unix_madvise_attempts();
    let successes_after = unix_madvise_successes();

    assert!(
        attempts_after > attempts_before,
        "the eligible-range eager decommit() call above must have reached the real \
         libc_madvise(2) syscall (UNIX_MADVISE_ATTEMPTS must have incremented) -- got \
         {attempts_before} -> {attempts_after}. If this fires, either the eligibility check \
         (linux_huge_range_is_madvise_eligible) regressed to reject a genuinely 2-MiB-aligned \
         range, or the huge-page dispatch in Reservation::decommit changed to skip the real \
         backend for an eligible range."
    );
    // Task #1166 (F5): strengthened from `successes_after > successes_before`
    // to an exact equality against `attempts_after`, matching item 59a's own
    // next-trigger wording (`unix_madvise_successes() == unix_madvise_attempts()
    // > 0`). The counters are reset to zero immediately above (`reset_bench_
    // internals_counters()`), so `attempts_before == successes_before == 0`
    // here and this equality is exactly the item's stated bar, not a weaker
    // stand-in for it: today's single-call-per-decommit dispatch
    // (`decommit_pages_impl`, `crates/aligned-vmem/src/os/unix.rs:423-447` --
    // `DecommitKind::Eager` reaches exactly one `libc_madvise` call, which
    // increments `UNIX_MADVISE_ATTEMPTS` by exactly 1 and `UNIX_MADVISE_
    // SUCCESSES` by 0 or 1 depending on the syscall's own return value) means
    // `attempts_after == 1` on this path, so `successes_after == attempts_after`
    // is equivalent to "the kernel accepted the call" -- but unlike a bare
    // `> successes_before` check, it additionally catches a future two-call
    // dispatch where one call succeeds and one fails (attempts +2, successes
    // +1 would satisfy the old `>` form while silently masking a partial
    // failure; it fails this equality).
    assert_eq!(
        successes_after, attempts_after,
        "OWNER DECISION (task #1164, strengthened task #1166): inside the \
         `aligned-vmem-hugetlb-real` job specifically -- a real MAP_HUGETLB grant \
         (hard-asserted above), a huge-page-size-aligned eligible range, on a modern \
         (>= 5.18) Linux kernel, with the pool hard-asserted configured -- every \
         madvise(2) MADV_DONTNEED call this decommit() reached must have returned 0 \
         (UNIX_MADVISE_SUCCESSES must equal UNIX_MADVISE_ATTEMPTS), not merely at \
         least one of them (contrast `decommit`'s own general contract, which does \
         tolerate refusal under cgroup/memory-pressure conditions this job's \
         environment does not have). Got attempts {attempts_before} -> {attempts_after}, \
         successes {successes_before} -> {successes_after}. The kernel rejected at \
         least one eligible madvise(2) call on a genuinely granted HugeTLB mapping -- \
         investigate a kernel/pool regression on this runner, do not relax this assertion."
    );

    // Task #1164: this marker only executes past BOTH the is_huge() grant
    // assertion and the kernel-acceptance assertion above -- printing it is
    // itself proof the real MAP_HUGETLB grant existed AND the kernel
    // genuinely accepted the eligible madvise(2) call (returned 0), not
    // merely that the process started with the right env var. Mirrors task
    // #1162's `[oracle] ARMED` marker pattern immediately above (same
    // reasoning: armed/unarmed must be distinguishable in the OUTPUT, not
    // just inferable from workflow text), checked by its own `grep -F`
    // sentinel in `.github/workflows/ci.yml`, resolved by
    // `scripts/verify-ci-sentinels.mjs`'s marker-sentinel category (added
    // task #1162).
    println!("[oracle] ARMED: kernel accepted eligible madvise(2) on real MAP_HUGETLB grant");
}

/// Task #1174 (item 87's next-trigger via the linked jobs, R30-8-class gap):
/// closes the one thing neither `ci_hugetlb_real_pool_oracle_refuses_
/// ordinary_page_fallback` nor `ci_hugetlb_real_pool_kernel_actually_accepts_
/// eligible_madvise` above ever checks — **memory content**. Both siblings
/// prove the kernel *dispatched* the `madvise(2)` call and *accepted* it
/// (returned 0); neither reads a single byte back. A kernel that accepts
/// `MADV_DONTNEED` on a real `MAP_HUGETLB` mapping but does not actually
/// reclaim/zero the underlying pages on next access — a real, documented
/// possibility this crate's own rustdoc distinguishes from acceptance (see
/// `Reservation`'s own doc block, "Huge reservations, eager `decommit`" —
/// zero-fill is the SEPARATE guarantee stated for that one eligible case, not
/// implied by acceptance alone) — would leave both siblings green while the
/// crate's documented postcondition for this exact case silently did not
/// hold.
///
/// This test is the write -> decommit -> read postcondition check: write a
/// non-zero pattern across a huge-page-size-aligned, madvise-eligible range,
/// `decommit()` it, then read every byte back and assert zero. This mirrors
/// `reservation_decommit_in_bounds_matches_free_function` in `tests/smoke.rs`
/// (the identical write/decommit/read-zero shape for the ORDINARY-page eager
/// case), narrowed to the one huge-page case where the crate's own doc
/// promises the same guarantee.
///
/// **What this test does NOT check (deliberately, see CLAUDE.md's
/// two-properties rule): whether huge pages were actually returned to the
/// pool / whether `HugePages_Free` increased.** That is a physical-resource
/// accounting question, not a content question, and unlike this test's
/// read-zero assertion it is not deterministic from inside one `#[test]` fn
/// in this process: `/proc/sys/vm/nr_hugepages` and `/proc/meminfo`'s
/// `HugePages_Free` are process-EXTERNAL, kernel-global counters that this
/// job's OWN earlier steps and every other test binary/target this job runs
/// (`huge_pages.rs`, `reservation_decommit_contract.rs`, the other tests in
/// THIS binary) also allocate from and release into — a `#[test]` fn cannot
/// snapshot "before" without racing every other huge-page reservation
/// already made or still live in this job, and cargo test's default
/// multi-threaded runner does not serialize across `#[test]` FILES the way
/// this file's own `SERIAL` mutex only serializes within itself. Folding a
/// noisy, shared, external counter into the SAME hard assert as this test's
/// deterministic in-process read-zero check would make an otherwise-reliable
/// oracle flaky on scheduling alone — exactly the class of defect CLAUDE.md's
/// "Tests must not be flaky" rule and the two-properties split both warn
/// against. The pool-free-count observation is instead taken as a
/// **best-effort, printed-not-asserted** measurement by the CI job itself
/// (`.github/workflows/ci.yml`'s `aligned-vmem-hugetlb-real` step, immediately
/// after this test's own `cargo test` invocation), reading
/// `/proc/meminfo`'s `HugePages_Free` before and after that invocation and
/// logging the delta — informative, not a gate, and NOT proof that pages
/// were returned (a nonzero job-level delta could come from any of this
/// job's other huge-page tests releasing their own reservations in the same
/// window, not specifically from this test's `decommit()` call).
///
/// **Vacuous-pass analysis:**
/// 1. **Env var unset:** the same honest early `return` as both siblings
///    above — gated identically, for the identical reason.
/// 2. **`#[cfg]` excluding this fn:** requires `huge-pages` (the real
///    dispatch path) and `target_os = "linux"`/`"android"` (the only
///    platforms the huge-aligned real-call path compiles for). Stated
///    precisely, because the two siblings do NOT have the same gate:
///    this is BYTE-IDENTICAL to
///    `ci_hugetlb_real_pool_oracle_refuses_ordinary_page_fallback`'s gate,
///    and a strict subset of
///    `ci_hugetlb_real_pool_kernel_actually_accepts_eligible_madvise`'s,
///    which additionally requires `feature = "bench-internals"` for the
///    counters it reads and this test does not. So this test compiles in
///    every configuration either sibling does, and in strictly more than
///    the second one — never fewer than either.
/// 3. **The reservation falling back to ordinary pages:** hard-asserted via
///    `is_huge()` BEFORE the write/decommit/read sequence — the same
///    tripwire both siblings use, and required by construction: without it
///    this test would silently validate the ordinary-page zero-fill
///    guarantee (already covered by `reservation_decommit_in_bounds_
///    matches_free_function` in `tests/smoke.rs`) and prove nothing about
///    the huge-page path.
/// 4. **The range being ineligible so the crate early-exits before any
///    syscall:** `size = 2 * MIB` and the full span `[0, size)` is exactly
///    one huge page, huge-page-size-aligned at both endpoints by
///    construction — identical reasoning to both siblings above.
/// 5. **The write pattern happening to already be zero:** written as `0xAB`
///    (never zero), so a decommit that is a complete no-op (old contents
///    left in place) would leave every byte `0xAB`, not `0`, and the read
///    loop below would fail on the very first byte.
/// 6. **Reading before the kernel has actually reclaimed the pages:** unlike
///    Darwin's advisory-only `MADV_DONTNEED`, Linux's `MADV_DONTNEED` on an
///    eligible range zero-fills synchronously with respect to the NEXT
///    access from the calling process (task #1140's own doc citation, `man 2
///    madvise`) — there is no reclaim-is-still-pending race to account for
///    here, unlike the RSS/pool-accounting axis this test deliberately does
///    not touch (point above).
/// 7. **Another test perturbing this range concurrently:** `SERIAL` (this
///    file's shared `Mutex<()>`) is held for this test's entire body, same
///    contract as both siblings and every other bench-internals-adjacent
///    test in this file.
#[test]
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    feature = "huge-pages"
))]
fn ci_hugetlb_real_pool_decommit_actually_zeroes_memory_on_reaccess() {
    use aligned_vmem::reserve_aligned_huge;

    if std::env::var("ALIGNED_VMEM_REQUIRE_REAL_HUGETLB").as_deref() != Ok("1") {
        // Not running inside the `aligned-vmem-hugetlb-real` job: honest
        // no-op, matching both oracle siblings above.
        return;
    }

    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let size = 2 * MIB; // Linux/Android huge-page size; the full span is exactly one eligible huge page.
    let mut reservation = reserve_aligned_huge(size, size)
        .expect("huge reservation request must succeed (real grant or fallback)");

    assert!(
        reservation.is_huge(),
        "ALIGNED_VMEM_REQUIRE_REAL_HUGETLB=1 is set (this must only be true inside the \
         `aligned-vmem-hugetlb-real` CI job), so a `reserve_aligned_huge` request took the \
         ordinary-page fallback instead of a real MAP_HUGETLB grant -- this test cannot measure \
         the memory-content postcondition of a decommit that was never granted as HugeTLB in the \
         first place. See `ci_hugetlb_real_pool_oracle_refuses_ordinary_page_fallback`'s identical \
         assertion above for the same failure's root cause."
    );

    let base = reservation.as_ptr();
    // Write a non-zero pattern across the whole eligible range so a no-op
    // decommit (old contents left in place, vacuous-pass analysis point 5
    // above) cannot be mistaken for a zero-fill.
    // SAFETY: `base` is non-null, page-aligned, and valid for `size` bytes
    // for the lifetime of `reservation` (the documented contract of
    // `Reservation::as_ptr`); the reservation was just granted above and has
    // not yet been decommitted, so the full range is committed and writable.
    unsafe {
        base.write_bytes(0xAB, size);
    }

    // The full reservation span is exactly one 2-MiB huge page: both
    // endpoints are LINUX_HUGE_PAGE_SIZE multiples by construction (eligible
    // range, vacuous-pass analysis point 4 above), so this reaches the real
    // `libc_madvise` call, not the skip-and-count path.
    reservation.decommit(0, size);

    // Read every byte back. On Linux, `MADV_DONTNEED` on an eligible huge
    // range zero-fills on next access (vacuous-pass analysis point 6 above) --
    // this is the crate's own documented postcondition for exactly this case
    // (`Reservation`'s doc block). A single non-zero byte anywhere in the
    // range means the kernel accepted the call (already proven by
    // `ci_hugetlb_real_pool_kernel_actually_accepts_eligible_madvise` above)
    // but did not actually reclaim the backing memory -- the gap this test
    // exists to close.
    // SAFETY: same justification as the write above; decommitted-then-
    // reaccessed memory in this exact case (huge, eager `decommit`, eligible
    // range, Linux/Android >= 5.18) is documented to read as zeroed, not to
    // fault or trap -- unlike the Windows case, which crashes on write before
    // `recommit` (a read is well-defined here by the same doc block).
    let bytes = unsafe { std::slice::from_raw_parts(base, size) };
    if let Some(offset) = bytes.iter().position(|&b| b != 0) {
        panic!(
            "byte at offset {offset} of {size} is {:#04x}, not zero, after decommit() on an \
             eligible huge-page range against a real MAP_HUGETLB grant (\
             ALIGNED_VMEM_REQUIRE_REAL_HUGETLB=1). The kernel accepted the madvise(2) call (see \
             `ci_hugetlb_real_pool_kernel_actually_accepts_eligible_madvise`), but this byte was \
             never actually reclaimed/zeroed -- investigate a kernel/pool regression on this \
             runner, do not relax this assertion.",
            bytes[offset]
        );
    }

    // Task #1174: this marker only executes past BOTH the is_huge() grant
    // assertion and the full-range read-zero loop above -- printing it is
    // itself proof the real MAP_HUGETLB grant existed AND every byte of the
    // decommitted range actually read back as zero, not merely that the
    // kernel accepted the syscall. Mirrors both siblings' `[oracle] ARMED`
    // marker pattern (armed/unarmed must be distinguishable in the OUTPUT,
    // not just inferable from workflow text), checked by its own `grep -F`
    // sentinel in `.github/workflows/ci.yml`, resolved by
    // `scripts/verify-ci-sentinels.mjs`'s marker-sentinel category.
    println!("[oracle] ARMED: decommitted range read back as all-zero on real MAP_HUGETLB grant");
}

/// Task #1189 (coverage gap C2 from
/// `docs/reviews/2026-08-19-2148-aligned-vmem-publication-audit-Сол-кодекс.md`):
/// closes the report's own named gap for the real-HugeTLB job specifically
/// -- `UNIX_MUNMAP_FAILURES` existed but no test in this job ever checked it
/// (or any release counter) around a HugeTLB reservation's `Drop`. The
/// report's own words: "the crate's comment in `unix.rs` assumes leak would
/// show up as test failure/resource exhaustion... but pool contains many
/// pages (64) and concurrent mappings are few, so absence of release is not
/// guaranteed to redden the job" -- i.e. a deleted release call site would
/// stay invisible in this job forever without a direct oracle.
///
/// This is the deterministic, in-process half of that oracle (mirroring the
/// two-properties split `ci_hugetlb_real_pool_decommit_actually_zeroes_
/// memory_on_reaccess` above already uses for the write/decommit/read
/// property vs. the shared `HugePages_Free` pool-count property): reserve
/// one real HugeTLB mapping, snapshot `unix_munmap_attempts()`/
/// `unix_munmap_failures()`, `drop()` the reservation (the ONLY `munmap`
/// call in this window -- `SERIAL` excludes every other test in this
/// binary, and this test makes exactly one reservation), then hard-assert
/// the attempts delta is exactly 1 and the failures delta is 0. Unlike
/// `HugePages_Free` (a kernel-global counter shared across this job's other
/// huge-page targets, see the sibling test's own doc for why that one stays
/// printed-not-asserted), `UNIX_MUNMAP_ATTEMPTS`/`_FAILURES` are THIS
/// PROCESS's own counters -- this test's binary (`decommit_capability`) is
/// a separate OS process from the job's other two test binaries
/// (`huge_pages`, `reservation_decommit_contract`), so nothing outside this
/// one `#[test]` fn's own `SERIAL`-guarded window can touch them. A hard
/// assert here is the CORRECT strength, not a compromise -- see
/// `UNIX_MUNMAP_ATTEMPTS`'s own doc comment
/// (`src/bench_internals/unix.rs`) for the general reasoning this specific
/// test applies.
///
/// **What this test does NOT check (same boundary the sibling test already
/// draws):** whether the huge page was actually returned to the kernel pool
/// (`HugePages_Free`) -- that is the job-level printed observation in
/// `.github/workflows/ci.yml`, unchanged by this test. This test proves the
/// RELEASE CALL ITSELF ran and succeeded, not that the physical page came
/// back to the pool -- those are the same "acceptance vs. physical
/// resource" distinction the sibling test's own doc draws for decommit.
///
/// **Vacuous-pass analysis:**
/// 1. **Env var unset:** the same honest early `return` as every sibling
///    oracle in this file.
/// 2. **`#[cfg]` excluding this fn:** requires `huge-pages` (for
///    `reserve_aligned_huge`) AND `bench-internals` (for the counters) AND
///    `target_os = "linux"`/`"android"` (the only platforms
///    `UNIX_MUNMAP_ATTEMPTS` is compiled for -- see that static's own
///    `#[cfg]`, which additionally requires `unix` and excludes `miri`,
///    both satisfied whenever `target_os = "linux"`/`"android"` holds).
///    Narrower than `ci_hugetlb_real_pool_decommit_actually_zeroes_memory_
///    on_reaccess`'s gate (that one does not need `bench-internals`), same
///    shape as `ci_hugetlb_real_pool_kernel_actually_accepts_eligible_
///    madvise`'s gate.
/// 3. **The reservation falling back to ordinary pages:** hard-asserted via
///    `is_huge()`, identical tripwire to every sibling oracle in this file
///    -- without it this test would validate the ordinary-page release
///    path (already implicitly exercised by every other test in this
///    crate's suite that drops a `Reservation`) and prove nothing new
///    about the HugeTLB-specific `munmap` alignment contract task #714
///    documents (`src/os/unix.rs`'s `unix_reserve` doc comment: `munmap`'s
///    `addr`/`length` must both be huge-page-size multiples for a
///    `MAP_HUGETLB` mapping, or the kernel returns `EINVAL` and leaks the
///    whole mapping).
/// 4. **A concurrent reservation/release in the same process touching the
///    same counters:** `SERIAL` is held for this test's entire body
///    (acquired before the reservation, released only when the function
///    returns), so no other test in THIS binary can run concurrently; no
///    other binary shares this process's counters (see the module-level
///    reasoning above).
/// 5. **The counter increment being deleted from `libc_munmap` (the actual
///    regression this test exists to catch):** would make `attempts_after
///    == attempts_before` (delta 0, not 1), failing the first assert below
///    -- confirmed by a local counterfactual on the WINDOWS sibling of this
///    same fix (`tests/smoke.rs`'s
///    `windows_virtualfree_release_is_attempted_exactly_once_and_does_not_fail`,
///    which this test mirrors): commenting out
///    `WINDOWS_VIRTUALFREE_RELEASE_ATTEMPTS.fetch_add` there reproducibly
///    turns that test red (`left: 0, right: 1`), then reverted. The Unix
///    counterfactual for THIS test could not be run locally (no HugeTLB
///    pool on this project's Windows dev host, and this fn's own env-var
///    guard makes it a no-op outside `ALIGNED_VMEM_REQUIRE_REAL_HUGETLB=1`)
///    -- the Windows counterfactual is the same code shape
///    (`#[cfg(feature = "bench-internals")] X.fetch_add(1, ...)` guarding a
///    release-path attempts counter) and is the closest available
///    verification; this test is first actually EXECUTED on the real-hugetlb
///    CI runner, not before.
#[test]
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    feature = "huge-pages",
    feature = "bench-internals"
))]
fn ci_hugetlb_real_pool_release_is_attempted_exactly_once_and_does_not_fail() {
    use aligned_vmem::{reserve_aligned_huge, unix_munmap_attempts, unix_munmap_failures};

    if std::env::var("ALIGNED_VMEM_REQUIRE_REAL_HUGETLB").as_deref() != Ok("1") {
        // Not running inside the `aligned-vmem-hugetlb-real` job: honest
        // no-op, matching every sibling oracle above.
        return;
    }

    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let size = 2 * MIB;
    let reservation = reserve_aligned_huge(size, size)
        .expect("huge reservation request must succeed (real grant or fallback)");

    assert!(
        reservation.is_huge(),
        "ALIGNED_VMEM_REQUIRE_REAL_HUGETLB=1 is set (this must only be true inside the \
         `aligned-vmem-hugetlb-real` CI job), so a `reserve_aligned_huge` request took the \
         ordinary-page fallback instead of a real MAP_HUGETLB grant -- this test cannot measure \
         the release-attempt postcondition of a HugeTLB mapping that was never actually granted. \
         See `ci_hugetlb_real_pool_oracle_refuses_ordinary_page_fallback`'s identical assertion \
         above for the same failure's root cause."
    );

    let attempts_before = unix_munmap_attempts();
    let failures_before = unix_munmap_failures();

    drop(reservation);

    let attempts_after = unix_munmap_attempts();
    let failures_after = unix_munmap_failures();
    assert_eq!(
        attempts_after,
        attempts_before + 1,
        "Drop must attempt exactly one munmap() call for a single live real-HugeTLB reservation \
         -- if this reads a delta of 0, the release call site was never reached at all (the exact \
         regression this test exists to catch; a failures-only check cannot distinguish this from \
         a genuine success)"
    );
    assert_eq!(
        failures_after, failures_before,
        "munmap() on a real MAP_HUGETLB reservation this process owns must not fail -- a nonzero \
         delta here means the whole mapping, including its pinned physical huge pages, just leaked \
         (see `unix_reserve`'s own doc comment in `src/os/unix.rs` for the EINVAL/alignment \
         contract this would indicate a violation of)"
    );

    // Marker follows the established armed/unarmed-must-differ-in-OUTPUT
    // pattern (tasks #1162/#1164/#1174): only reached past the is_huge()
    // grant tripwire AND both delta asserts above, so printing it is proof
    // the release was attempted exactly once and did not fail, not merely
    // that the test function ran.
    println!("[oracle] ARMED: real MAP_HUGETLB release attempted exactly once and did not fail");
}
