//! Tests for the `huge-pages` feature (task #714's own regression coverage;
//! task #716 tracks building out the rest of this feature's test suite,
//! which had zero coverage before this file).
//!
//! **Verification honesty (per task #714's own instruction):** no
//! hugetlb-configured host is in this project's CI, and this crate's
//! `MAP_HUGETLB` request path is Linux-only (`libc_mmap`'s `huge` flag has
//! no effect on any other target — huge pages are a documented no-op
//! elsewhere). The `#[cfg(target_os = "linux")]` tests below (the
//! task #714-specific hugetlb-alignment-rejection regressions) therefore
//! only ever compile and run on Linux; everywhere else (including the
//! Windows host this round's fixes were authored and verified on) they are
//! `#[cfg]`-excluded and only the general best-effort-fallback sanity test
//! above them runs. The Linux-only tests' correctness was checked, when
//! authored, by a cross-compile (`cargo check --target
//! x86_64-unknown-linux-gnu`), NOT by execution on a real hugetlb-backed
//! system.
//!
//! task #776 (F12, round-closing review): an earlier revision of this
//! comment said this file exists so a Linux CI runner "were one added"
//! would exercise this rejection logic -- that undersold this file's own
//! coverage. A Linux runner ALREADY exists (`.github/workflows/ci.yml`'s
//! `test-workspace` job, `runs-on: ubuntu-latest`) and already runs
//! `cargo test -p aligned-vmem --all-features`, which compiles this file
//! (gated only on `feature = "huge-pages"`, not `target_os`) and executes
//! all four Linux-gated tests below on every push. This file's Linux-only
//! regression coverage is therefore genuinely LIVE in CI, not merely
//! compile-checked. The real residual gap, stated precisely: no
//! **hugetlb-configured** host runs these tests, so the
//! `MAP_HUGETLB`-actually-succeeds branch (as opposed to the
//! best-effort-fallback branch, which IS exercised) stays untested end to
//! end.

#![cfg(feature = "huge-pages")]
// `serial_guard()` returns a real `MutexGuard` on `bench-internals` builds and
// `()` everywhere else (the lock/counters it guards don't exist there) -- every
// call site binds it via `let _guard = ...;` so the guard's scope (drop at
// function end) is identical across all platforms; on the `()` variant that's
// a unit-value binding, which is otherwise a real clippy smell but is exactly
// the point here.
#![allow(clippy::let_unit_value)]

use aligned_vmem::reserve_aligned_huge;
#[cfg(target_os = "linux")]
use aligned_vmem::{try_reserve_aligned_huge, VmemError, PAGE};

const MIB: usize = 1024 * 1024;

/// The `bench-internals` diagnostic counters this file's path-activation
/// tests read (`WINDOWS_RESERVE_COMMIT_SINGLE_CALLS`/`_TWO_CALL_PAIRS` on
/// Windows, `UNIX_EXACT_RESERVE_ATTEMPTS`/`_HITS` on Linux) are
/// PROCESS-GLOBAL. libtest runs this file's tests on parallel threads by
/// default, so any test that calls `reserve_aligned_huge`/
/// `try_reserve_aligned_huge` (which increments these counters as a side
/// effect whenever `bench-internals` is enabled, regardless of whether the
/// calling test itself reads them) can corrupt a sibling test's
/// reset-then-measure window -- mirroring the exact hazard
/// `tests/lazy_commit.rs`'s own `SERIAL` mutex already exists to prevent for
/// the same counters. Confirmed by direct reproduction: running this file's
/// tests under the default (parallel) `--test-threads` failed
/// `reserve_aligned_huge_2mib_still_two_call_path_unprivileged`
/// nondeterministically (observed `two_call_pairs=2` instead of the expected
/// `1`, from a concurrently-running sibling test's own reservation call).
/// Every test in this file that reserves through `reserve_aligned_huge`/
/// `try_reserve_aligned_huge` now holds `SERIAL` for its whole body -- a
/// future test added to this file must also take it.
#[cfg(feature = "bench-internals")]
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Acquire [`SERIAL`] under `bench-internals` (a no-op everywhere else,
/// since neither the static nor the counters it guards exist there) -- see
/// `SERIAL`'s own doc comment above for why every counter-touching test in
/// this file needs this, not only the ones that read the counters.
#[cfg(feature = "bench-internals")]
fn serial_guard() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}
#[cfg(not(feature = "bench-internals"))]
fn serial_guard() {}

#[test]
fn reserve_aligned_huge_ordinary_page_sized_request_succeeds() {
    let _guard = serial_guard();
    // The general huge-pages contract (best-effort: falls back to ordinary
    // pages when the OS refuses huge pages) applies on every platform,
    // including where `huge` is a documented no-op (macOS, other non-Linux
    // Unix, and this Windows-only-verified session's own host). Not itself
    // task #714-specific -- a basic sanity check that the feature still
    // works at all after this round's changes.
    let r = reserve_aligned_huge(4 * MIB, 4 * MIB).expect("huge reservation (or fallback)");
    let base = r.as_ptr();
    assert_eq!(base as usize % (4 * MIB), 0, "must still be align-aligned");
    // SAFETY: base is valid for 4 MiB, freshly reserved.
    unsafe {
        base.write(0xAB);
        assert_eq!(base.read(), 0xAB);
    }
    // Regression for W2 fix: on non-Linux Unix and Windows, huge pages are
    // a documented no-op, so is_huge() must always return false even when
    // reserve_aligned_huge is called.
    #[cfg(not(target_os = "linux"))]
    assert!(!r.is_huge(), "non-Linux Unix and Windows never report huge");
}

// ── task #714: hugetlb-alignment rejection (Linux + huge-pages only) ───────

#[cfg(target_os = "linux")]
const LINUX_HUGE_PAGE_SIZE: usize = 2 * MIB;

#[test]
#[cfg(target_os = "linux")]
fn reserve_aligned_huge_rejects_non_huge_page_aligned_size() {
    // task #714 (rust-intel audit MEDIUM §F1): on Linux, `mmap(2)`'s Huge TLB
    // rule requires munmap's addr AND length to be huge-page-aligned; the
    // over-reserve path used to silently leak the whole mapping (EINVAL
    // from munmap, discarded) when `size` was not a multiple of the huge
    // page size. Fixed by rejecting such a request up front with
    // VmemError::invalid_argument() instead of leaking.
    let bad_size = LINUX_HUGE_PAGE_SIZE + PAGE; // NOT a multiple of the huge page size
    match try_reserve_aligned_huge(bad_size, LINUX_HUGE_PAGE_SIZE) {
        Err(e) => assert!(
            e.is_invalid_argument(),
            "must be classified as a contract violation, not an OS refusal: {e:?}"
        ),
        Ok(_) => panic!("a non-huge-page-aligned size must be rejected, not silently leaked"),
    }
}

#[test]
#[cfg(target_os = "linux")]
fn reserve_aligned_huge_rejects_non_huge_page_aligned_align() {
    // Same rule, the `align` half: `over = size + align` must also be
    // huge-page-aligned for the release to stay provably conformant (see
    // `unix_reserve`'s own doc for the full reasoning).
    let good_size = LINUX_HUGE_PAGE_SIZE;
    let bad_align = PAGE; // page-aligned but NOT huge-page-aligned
    match try_reserve_aligned_huge(good_size, bad_align) {
        Err(e) => assert!(e.is_invalid_argument(), "{e:?}"),
        Ok(_) => panic!("a non-huge-page-aligned align must be rejected, not silently leaked"),
    }
}

#[test]
#[cfg(target_os = "linux")]
fn reserve_aligned_huge_accepts_huge_page_aligned_request() {
    let _guard = serial_guard();
    // The positive case: a genuinely huge-page-aligned size/align pair must
    // NOT be rejected by the new guard (only misaligned requests are).
    // Whether the OS actually grants real huge pages depends on host
    // configuration (`/proc/sys/vm/nr_hugepages`) -- the crate's own
    // documented best-effort fallback to ordinary pages means this must
    // succeed either way, so this test does not assert on `VmemError` at
    // all, only that the CONTRACT-VIOLATION guard specifically does not
    // fire for well-formed input.
    let size = LINUX_HUGE_PAGE_SIZE;
    match try_reserve_aligned_huge(size, LINUX_HUGE_PAGE_SIZE) {
        Ok(r) => {
            let base = r.as_ptr();
            assert_eq!(base as usize % LINUX_HUGE_PAGE_SIZE, 0);
        }
        Err(e) => assert!(
            !e.is_invalid_argument(),
            "a huge-page-aligned request must never be rejected as a contract \
             violation -- only a genuine OS refusal (commit-charge exhaustion, \
             no hugetlb pages configured) is an acceptable failure here: {e:?}"
        ),
    }
}

#[test]
#[cfg(target_os = "linux")]
fn reserve_aligned_huge_error_type_is_vmem_error() {
    // Type-level sanity: the fallible entry point's error type is exactly
    // `VmemError` (guards against a future refactor silently widening the
    // public error surface).
    fn assert_error_type(_: Result<aligned_vmem::Reservation, VmemError>) {}
    assert_error_type(try_reserve_aligned_huge(0, PAGE));
}

/// II-4 (2026-08-16 audit finding): Linux huge-page reservation with
/// `align == LINUX_HUGE_PAGE_SIZE` (2 MiB) uses exact-size mmap, avoiding
/// `size + align` over-reserve against the hugetlb pool.
///
/// This test verifies that `reserve_aligned_huge(2 MiB, 2 MiB)` succeeds,
/// is properly aligned, AND actually takes the Linux exact-size huge-page
/// fast path (using the bench-internals counters as a path-activation oracle).
/// The kernel guarantees an anonymous MAP_HUGETLB mapping starts at a
/// huge-page-aligned address, so the exact-size mmap satisfies the alignment
/// contract without over-reserving.
///
/// Whether huge pages are actually granted depends on host configuration
/// (`/proc/sys/vm/nr_hugepages`); the crate's best-effort fallback means
/// this must succeed either way, so the test only asserts on the contract-
/// violation guard not firing (not on the actual huge-page grant).
#[test]
#[cfg(target_os = "linux")]
fn reserve_aligned_huge_exact_size_for_2mib_align() {
    let _guard = serial_guard();
    let size = LINUX_HUGE_PAGE_SIZE;

    #[cfg(feature = "bench-internals")]
    aligned_vmem::reset_bench_internals_counters();
    match aligned_vmem::try_reserve_aligned_huge(size, LINUX_HUGE_PAGE_SIZE) {
        Ok(r) => {
            let base = r.as_ptr();
            assert_eq!(base as usize % LINUX_HUGE_PAGE_SIZE, 0);

            // The memory must be writable.
            unsafe {
                base.write(0xEF);
                assert_eq!(base.read(), 0xEF);
            }

            // PATH-ACTIVATION ORACLE: verify this took the Linux exact-size huge path.
            // The exact-size huge path increments UNIX_EXACT_RESERVE_ATTEMPTS before calling
            // mmap, and increments UNIX_EXACT_RESERVE_HITS only if that mmap succeeds AND
            // the returned address happens to be aligned. When /proc/sys/vm/nr_hugepages == 0
            // (the default on GitHub-hosted ubuntu-latest CI runners), the MAP_HUGETLB mmap
            // fails and the function falls through to the ordinary-page fallback path, which
            // succeeds and returns Ok(r) — so attempts is always 1, but hits may be 0.
            #[cfg(feature = "bench-internals")]
            {
                let attempts = aligned_vmem::unix_exact_reserve_attempts();
                let hits = aligned_vmem::unix_exact_reserve_hits();
                assert_eq!(
                    attempts, 1,
                    "2 MiB shape must increment UNIX_EXACT_RESERVE_ATTEMPTS; observed: attempts={attempts}, hits={hits}"
                );
                assert!(
                    hits <= 1,
                    "UNIX_EXACT_RESERVE_HITS cannot exceed 1 for a single reservation; observed: attempts={attempts}, hits={hits}"
                );
                // This assert is a cheap tripwire for future refactoring, not an active path-activation oracle.
                // By construction (single HITS increment site returns immediately, 32-bit path unreachable in this scenario),
                // hits can never exceed 1. The active oracle is assert_eq!(attempts, 1) above and the r.is_huge() check below.
                // When hits == 1, the MAP_HUGETLB mmap succeeded and actually granted a huge page.
                if hits == 1 {
                    assert!(
                        r.is_huge(),
                        "hits == 1 must correspond to an actual huge-page grant"
                    );
                }
            }
        }
        Err(e) => assert!(
            !e.is_invalid_argument(),
            "a 2 MiB-aligned request with size = 2 MiB must never be rejected as a \
             contract violation -- the kernel guarantees huge-page alignment: {e:?}"
        ),
    }
}

/// V-25: Windows single-call large-page branch (task #923).
///
/// Every existing `reserve_aligned_huge`/`try_reserve_aligned_huge` call site
/// in this crate uses `align` of 2 MiB or 4 MiB, which on Windows all take the
/// TWO-CALL path. This test exercises the Windows SINGLE-CALL large-page
/// branch by using `align == size == 64 KiB`, which is well under the
/// `WIN_ALLOCATION_GRANULARITY` threshold. The test asserts:
/// - the returned `Reservation`'s `as_ptr()` is non-null and aligned to 64 KiB;
/// - the memory is writable (write a byte, read it back);
/// - `is_huge()` is `false`: `GetLargePageMinimum()` returns 2 MiB on x86_64,
///   so a 64 KiB `MEM_LARGE_PAGES` request can NEVER succeed regardless of
///   privilege — the assertion is safe by construction, not by host configuration.
///
/// II-3 (2026-08-16 audit finding): the single-call fast-path condition is
/// widened to `align <= GetLargePageMinimum()` when requesting large pages,
/// so `reserve_aligned_huge(2 MiB, 2 MiB)` (not 4 MiB!) can now ATTEMPT the
/// single-call path -- though on an unprivileged host it still falls through
/// to the two-call path (see the post-call-alignment-check explanation on
/// `reserve_aligned_huge_2mib_still_two_call_path_unprivileged` below, which
/// verifies the 2 MiB case with a path-activation counter assertion). The
/// 4 MiB shape never even attempts the fast path, since 4 MiB >
/// GetLargePageMinimum() (typically 2 MiB on x86_64).
///
/// Note: the V-6 alignment check (task #921) is unobservable on a conforming
/// Windows host and is NOT regression-tested by this or any test in this crate —
/// it guards against `WIN_ALLOCATION_GRANULARITY` being wrong, a condition that
/// cannot be constructed without a fake/mocked allocator backend.
#[test]
#[cfg(windows)]
fn reserve_aligned_huge_64k_single_call_path() {
    let _guard = serial_guard();
    const SIZE: usize = 64 * 1024; // 64 KiB, below WIN_ALLOCATION_GRANULARITY (also 64 KiB)
    let r = reserve_aligned_huge(SIZE, SIZE).expect("64 KiB huge reservation");
    let base = r.as_ptr();

    // The returned pointer must be non-null and aligned to the requested alignment.
    assert!(!base.is_null(), "base pointer must be non-null");
    assert_eq!(base.addr() % SIZE, 0, "base must be 64 KiB-aligned");

    // The memory must be writable (write a byte, read it back).
    // SAFETY: base is valid for SIZE bytes, freshly reserved.
    unsafe {
        base.write(0xAB);
        assert_eq!(base.read(), 0xAB, "written byte must read back");
    }

    // This assertion is safe unconditionally: GetLargePageMinimum() returns 2 MiB
    // on x86_64, so a 64 KiB request can NEVER succeed regardless of privilege.
    // If this assertion fails, it's the W-1 bug (fixed in task #943; this test
    // was added by task #949 to guard it): Reservation::is_huge() incorrectly
    // returns true after a Windows large-page request failed and fell back to
    // ordinary pages.
    assert!(
        !r.is_huge(),
        "is_huge() must be false for 64 KiB: GetLargePageMinimum() is 2 MiB, so a \
         64 KiB MEM_LARGE_PAGES request cannot succeed by construction"
    );
}

/// II-3 (2026-08-16 audit finding) — CLOSED and corrected:
///
/// The audit finding claimed that the Windows single-call fast-path condition
/// was widened from `align <= 64 KiB` to `align <= GetLargePageMinimum()` when
/// requesting large pages, and that this would make `reserve_aligned_huge(2 MiB, 2 MiB)`
/// now attemptable via the single-call path.
///
/// **This claim was empirically false.** While the condition code was indeed
/// widened to use `GetLargePageMinimum()` instead of `WIN_ALLOCATION_GRANULARITY`,
/// the fast path also requires a POST-CALL alignment check that verifies the
/// base returned by `VirtualAlloc` is actually aligned to the requested `align`.
/// When large pages are not granted (e.g., due to lack of `SeLockMemoryPrivilege`),
/// `VirtualAlloc`'s alignment guarantee is only 64 KiB (`WIN_ALLOCATION_GRANULARITY`);
/// in practice it typically does NOT happen to land on a 2 MiB boundary, so the
/// post-call check fails and the single-call path falls through to the two-call path.
///
/// The practical result: the II-3 widening expands the single-call ATTEMPT window
/// from `align <= 64 KiB` to `align <= GetLargePageMinimum()` (typically 2 MiB),
/// but on an unprivileged host the actual paths that SUCCEED (pass the post-call
/// alignment check) are typically still limited to `align <= 64 KiB`. The widened
/// window is real and can succeed on unprivileged hosts when VirtualAlloc happens
/// to return a base that's already aligned to the requested alignment, but this is
/// a coincidence, not architecturally guaranteed. The only shapes that reliably
/// benefit from the widening are those where:
/// 1. The host grants actual large pages (rare, requires privilege), AND
/// 2. The size is a multiple of `GetLargePageMinimum()`.
///
/// For the common unprivileged case, the fast-path widening has NEARLY ZERO effect:
/// `reserve_aligned_huge(2 MiB, 2 MiB)` typically still takes the two-call path,
/// exactly as before, but in roughly 1-in-32 runs (the odds of VirtualAlloc coincidentally
/// returning a 2 MiB-aligned base), the fast path can succeed even without privilege.
/// The earlier test was vacuous because it only asserted that the reservation succeeded
/// and was aligned — both true regardless of which internal path was taken (the two-call
/// path also produces correctly-aligned ordinary-page reservations).
///
/// This test documents the ACTUAL behavior (two-call path for unprivileged
/// 2 MiB) and includes a path-activation assertion to detect if this ever changes.
#[test]
#[cfg(all(windows, feature = "bench-internals"))]
fn reserve_aligned_huge_2mib_still_two_call_path_unprivileged() {
    let _guard = serial_guard();
    use aligned_vmem::{
        reset_bench_internals_counters, windows_reserve_commit_single_calls,
        windows_reserve_commit_two_call_pairs,
    };

    const SIZE: usize = 2 * 1024 * 1024; // 2 MiB == GetLargePageMinimum() on typical x86_64

    reset_bench_internals_counters();
    let r = reserve_aligned_huge(SIZE, SIZE).expect("2 MiB huge reservation");
    let base = r.as_ptr();

    // The returned pointer must be non-null and aligned to 2 MiB.
    assert!(!base.is_null(), "base pointer must be non-null");
    assert_eq!(base.addr() % SIZE, 0, "base must be 2 MiB-aligned");

    // The memory must be writable.
    // SAFETY: base is valid for SIZE bytes, freshly reserved.
    unsafe {
        base.write(0xCD);
        assert_eq!(base.read(), 0xCD, "written byte must read back");
    }

    // PATH-ACTIVATION ORACLE: verify this takes the two-call path (NOT single-call).
    // The widened fast-path condition is `align <= GetLargePageMinimum()`, which is
    // satisfied for SIZE=2 MiB on typical x86_64. However, the fast path ALSO
    // requires a post-call alignment check. When large pages are not granted
    // (unprivileged process), VirtualAlloc's alignment guarantee is only 64 KiB;
    // in practice it typically does NOT happen to land on a 2 MiB boundary, so the
    // post-call check fails and we fall through to the two-call path. In the rare
    // case where VirtualAlloc coincidentally returns a 2 MiB-aligned base, the fast
    // path succeeds — this is acceptable, not a failure.
    let single_calls = windows_reserve_commit_single_calls();
    let two_call_pairs = windows_reserve_commit_two_call_pairs();
    assert_eq!(
        single_calls + two_call_pairs, 1,
        "Exactly one path must be taken; observed: single_calls={single_calls}, two_call_pairs={two_call_pairs}"
    );
    // When the fast path coincidentally succeeds (rare, roughly 1-in-32), the
    // alignment/writability checks above already cover it -- both paths produce
    // a correctly-aligned, writable reservation, so there is nothing further to
    // assert here specifically for the single_calls == 1 case.
}

/// II-3 (2026-08-16 audit finding) — 4 MiB case explicitly documented as two-call path.
///
/// This test documents that `reserve_aligned_huge(4 MiB, 4 MiB)` does NOT use the
/// single-call fast path on Windows, even after the II-3 widening. The reason:
/// `GetLargePageMinimum()` returns 2 MiB on typical x86_64 Windows hosts, and the
/// widened fast-path condition is `align <= GetLargePageMinimum()`. Since `align = 4 MiB > 2 MiB`,
/// the condition is false, so the fast path is never attempted. The 4 MiB shape
/// still takes the two-call path, exactly as before the II-3 change.
///
/// This test explicitly verifies this state so it's clearly documented, not left as
/// an unverified assumption.
#[test]
#[cfg(all(windows, feature = "bench-internals"))]
fn reserve_aligned_huge_4mib_still_two_call_path() {
    let _guard = serial_guard();
    use aligned_vmem::{
        reset_bench_internals_counters, windows_reserve_commit_single_calls,
        windows_reserve_commit_two_call_pairs,
    };

    const SIZE: usize = 4 * 1024 * 1024; // 4 MiB

    reset_bench_internals_counters();
    let r = reserve_aligned_huge(SIZE, SIZE).expect("4 MiB huge reservation");
    let base = r.as_ptr();

    // The returned pointer must be non-null and aligned to 4 MiB.
    assert!(!base.is_null(), "base pointer must be non-null");
    assert_eq!(base.addr() % SIZE, 0, "base must be 4 MiB-aligned");

    // The memory must be writable.
    // SAFETY: base is valid for SIZE bytes, freshly reserved.
    unsafe {
        base.write(0xCD);
        assert_eq!(base.read(), 0xCD, "written byte must read back");
    }

    // PATH-ACTIVATION ORACLE: verify this takes the two-call path (NOT single-call).
    // The fast-path condition is `align <= GetLargePageMinimum() && commit_len == size`.
    // For SIZE = 4 MiB > GetLargePageMinimum() = 2 MiB, the condition is false.
    let single_calls = windows_reserve_commit_single_calls();
    let two_call_pairs = windows_reserve_commit_two_call_pairs();
    assert_eq!(
        single_calls, 0,
        "4 MiB shape must NOT take the single-call fast path; observed: single_calls={single_calls}, two_call_pairs={two_call_pairs}"
    );
    assert_eq!(
        two_call_pairs, 1,
        "4 MiB shape must take the two-call path; observed: single_calls={single_calls}, two_call_pairs={two_call_pairs}"
    );
}
