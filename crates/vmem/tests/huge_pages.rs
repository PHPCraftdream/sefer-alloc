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

use aligned_vmem::reserve_aligned_huge;
#[cfg(target_os = "linux")]
use aligned_vmem::{try_reserve_aligned_huge, VmemError, PAGE};

const MIB: usize = 1024 * 1024;

#[test]
fn reserve_aligned_huge_ordinary_page_sized_request_succeeds() {
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
/// This test verifies that `reserve_aligned_huge(2 MiB, 2 MiB)` succeeds
/// and is properly aligned. The kernel guarantees an anonymous MAP_HUGETLB
/// mapping starts at a huge-page-aligned address, so the exact-size mmap
/// satisfies the alignment contract without over-reserving.
///
/// Whether huge pages are actually granted depends on host configuration
/// (`/proc/sys/vm/nr_hugepages`); the crate's best-effort fallback means
/// this must succeed either way, so the test only asserts on the contract-
/// violation guard not firing (not on the actual huge-page grant).
#[test]
#[cfg(target_os = "linux")]
fn reserve_aligned_huge_exact_size_for_2mib_align() {
    let size = LINUX_HUGE_PAGE_SIZE;
    match try_reserve_aligned_huge(size, LINUX_HUGE_PAGE_SIZE) {
        Ok(r) => {
            let base = r.as_ptr();
            assert_eq!(base as usize % LINUX_HUGE_PAGE_SIZE, 0);
            // The memory must be writable.
            unsafe {
                base.write(0xEF);
                assert_eq!(base.read(), 0xEF);
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
/// so `reserve_aligned_huge(4 MiB, 4 MiB)` can now attempt the single-call
/// path. This test is extended to also verify the widened condition works
/// (the actual huge-page grant still requires size to be a multiple of the
/// large-page minimum and the process to have SeLockMemoryPrivilege).
///
/// Note: the V-6 alignment check (task #921) is unobservable on a conforming
/// Windows host and is NOT regression-tested by this or any test in this crate —
/// it guards against `WIN_ALLOCATION_GRANULARITY` being wrong, a condition that
/// cannot be constructed without a fake/mocked allocator backend.
#[test]
#[cfg(windows)]
fn reserve_aligned_huge_64k_single_call_path() {
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

/// II-3 (2026-08-16 audit finding): Windows 4 MiB-aligned 4 MiB huge reservation.
///
/// This test verifies that `reserve_aligned_huge(4 MiB, 4 MiB)` can now attempt
/// the single-call fast path on Windows (the fast-path condition was widened
/// from `align <= 64 KiB` to `align <= GetLargePageMinimum()` when requesting
/// large pages). Whether large pages are actually granted depends on:
/// 1. The process has SeLockMemoryPrivilege granted AND enabled
/// 2. The size is a multiple of the system's large-page minimum (2 MiB on x86_64)
///
/// This test verifies the shape works (reserves and falls back correctly) without
/// asserting on the actual huge-page grant (which depends on host configuration).
#[test]
#[cfg(windows)]
fn reserve_aligned_huge_4mib_single_call_path_widened() {
    const SIZE: usize = 4 * 1024 * 1024; // 4 MiB
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

    // is_huge() depends on whether the system actually granted large pages.
    // We don't assert on it here; the test verifies the shape works, not
    // the actual grant (which is host-configured).
}
