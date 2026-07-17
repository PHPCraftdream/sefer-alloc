//! Behavioural tests for `proc-memstat`: the two axes (RSS vs commit) must
//! move as documented, and `peak_rss` must be monotonic non-decreasing.
//!
//! Some legs are inherently platform-specific and are gated + explained where
//! they cannot be made deterministic everywhere.

use proc_memstat::snapshot;

/// A `snapshot()` on any platform must not panic and must be internally
/// consistent (peak_rss, when present, is at least the live rss).
#[test]
fn snapshot_is_consistent() {
    let m = snapshot();
    if let Some(peak) = m.peak_rss {
        // A high-water mark cannot be below the current resident set.
        assert!(
            peak >= m.rss,
            "peak_rss ({peak}) must be >= live rss ({})",
            m.rss
        );
    }
}

/// Touching freshly-allocated memory makes it resident → `rss` grows.
///
/// On the stub target (miri / unknown OS) `snapshot()` reports zeros, so the
/// growth cannot be observed; the assertion is gated to real Linux/Windows/
/// macOS reads. Elsewhere it is a real monotonicity check.
#[test]
fn touching_memory_grows_rss() {
    let before = snapshot();

    // Allocate and TOUCH ~16 MiB so the OS must back it with physical pages.
    // 16 MiB (not 1) to stay comfortably above working-set trimming / probe
    // granularity noise on all three real platforms.
    const N: usize = 16 * 1024 * 1024;
    let mut buf: Vec<u8> = vec![0u8; N];
    // Write one byte per 4 KiB page so every page is faulted in, then read it
    // back through a black-box so nothing is optimised away.
    let mut acc: u64 = 0;
    let mut i = 0;
    while i < N {
        buf[i] = (i as u8) | 1;
        acc = acc.wrapping_add(u64::from(buf[i]));
        i += 4096;
    }
    std::hint::black_box(&buf);
    std::hint::black_box(acc);

    let after = snapshot();

    #[cfg(all(any(target_os = "linux", windows, target_os = "macos"), not(miri)))]
    {
        assert!(
            after.rss > before.rss,
            "rss must grow after touching {N} bytes: before={} after={}",
            before.rss,
            after.rss
        );
    }
    #[cfg(any(miri, not(any(target_os = "linux", windows, target_os = "macos"))))]
    {
        // Stub target: snapshot reports zeros; growth is unobservable here.
        let _ = (before, after);
    }
}

/// `peak_rss` is monotonic non-decreasing across two snapshots that straddle a
/// large touch. On the stub target `peak_rss` is `None`; the check is gated to
/// platforms that expose a peak counter (all three real ones).
#[test]
fn peak_rss_is_monotonic() {
    let before = snapshot();

    const N: usize = 16 * 1024 * 1024;
    let mut buf: Vec<u8> = vec![0u8; N];
    let mut i = 0;
    while i < N {
        buf[i] = 0xA5;
        i += 4096;
    }
    std::hint::black_box(&buf);

    let after = snapshot();

    match (before.peak_rss, after.peak_rss) {
        (Some(b), Some(a)) => assert!(
            a >= b,
            "peak_rss must be non-decreasing: before={b} after={a}"
        ),
        (None, None) => { /* stub target: no peak counter, nothing to assert */ }
        (b, a) => panic!("peak_rss availability changed mid-run: {b:?} -> {a:?}"),
    }
}

/// Reserve+commit WITHOUT touching → `commit` grows while `rss` does NOT.
///
/// This is the entire reason commit charge is a separate axis: on Windows a
/// `VirtualAlloc(MEM_COMMIT)` charges the commit limit immediately, but the
/// demand-zero pages are not resident until first access — so `commit` moves
/// and `rss` does not. Windows-only: it needs the OS's distinct commit-charge
/// accounting (Linux overcommit + macOS `virtual_size` do not give the same
/// clean "committed but not resident" guarantee without an actual mapping
/// syscall, which would reintroduce an FFI dependency into the test).
#[cfg(all(windows, not(miri)))]
#[test]
fn committing_without_touching_grows_commit_not_rss() {
    // Locally-declared VirtualAlloc/VirtualFree FFI — the test crate may hold
    // unsafe; this exercises the commit-vs-rss distinction the library exists
    // to surface.
    const MEM_COMMIT: u32 = 0x1000;
    const MEM_RESERVE: u32 = 0x2000;
    const MEM_RELEASE: u32 = 0x8000;
    const PAGE_READWRITE: u32 = 0x04;

    extern "system" {
        fn VirtualAlloc(
            addr: *mut core::ffi::c_void,
            size: usize,
            alloc_type: u32,
            protect: u32,
        ) -> *mut core::ffi::c_void;
        fn VirtualFree(addr: *mut core::ffi::c_void, size: usize, free_type: u32) -> i32;
    }

    const N: usize = 64 * 1024 * 1024; // 64 MiB committed, never touched.

    let before = snapshot();

    // SAFETY: reserve+commit N bytes of anonymous VA; we never dereference the
    // returned pointer (that is the point — no page is faulted in), and we
    // release it with MEM_RELEASE (size 0 per the VirtualFree contract) before
    // returning. On failure (`p.is_null()`) we skip the assertions.
    unsafe {
        let p = VirtualAlloc(
            core::ptr::null_mut(),
            N,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );
        assert!(!p.is_null(), "VirtualAlloc(MEM_COMMIT) failed");

        let after = snapshot();

        // Commit charge grew by ~N (allow slack for concurrent activity).
        assert!(
            after.commit >= before.commit + (N as u64) / 2,
            "commit must grow by ~{N} after MEM_COMMIT: before={} after={}",
            before.commit,
            after.commit
        );
        // RSS did NOT grow by anything like N — the pages were never touched,
        // so at most incidental noise moved it. Assert it did not grow by even
        // a quarter of the committed span.
        assert!(
            after.rss < before.rss + (N as u64) / 4,
            "rss must NOT grow from untouched commit: before={} after={} (N={N})",
            before.rss,
            after.rss
        );

        let freed = VirtualFree(p, 0, MEM_RELEASE);
        assert!(freed != 0, "VirtualFree(MEM_RELEASE) failed");
    }
}
