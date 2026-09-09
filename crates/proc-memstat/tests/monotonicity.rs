//! Behavioural tests for `proc-memstat`: the two axes (RSS vs commit) must
//! move as documented, and `peak_rss` must be monotonic non-decreasing.
//!
//! **Every scenario that moves process memory runs in a FRESH PROCESS**
//! (review P3-2). RSS and commit charge are process-wide counters, libtest
//! runs tests in parallel by default, and there is no way to atomically pair
//! a before/after snapshot with the allocation that must sit between them:
//! a neighbour test's touched-but-not-yet-freed 16 MiB trips the Windows
//! "rss must NOT grow" bound, and a neighbour freeing between the two reads
//! cancels the growth the rss scenarios expect — both with no defect in the
//! snapshot itself. Each parent test therefore re-executes this very test
//! binary with `--exact <test>` plus a marker env var; the child runs the
//! scenario alone against its own clean counters, and the parent fails with
//! the child's captured output if the child fails. A static mutex
//! additionally serialises the children, so a memory-quota'd host never
//! carries two scenarios at once. Read-only contract coverage (which
//! counters exist per platform, and that Linux reports bytes, not KiB)
//! lives in `tests/platform_contract.rs`.

use proc_memstat::snapshot;

/// Marker env var, set ONLY on the freshly-spawned scenario process: the
/// marked process runs its scenario body directly instead of spawning yet
/// another copy of itself.
#[cfg(all(any(target_os = "linux", windows, target_os = "macos"), not(miri)))]
const SCENARIO_CHILD: &str = "PROC_MEMSTAT_MONOTONICITY_CHILD_PROCESS";

/// Run `scenario` — a test of THIS binary — alone in a fresh process, and
/// fail this test (with the child's captured output) if that child fails.
#[cfg(all(any(target_os = "linux", windows, target_os = "macos"), not(miri)))]
fn run_scenario_alone(scenario: &'static str) {
    use std::sync::Mutex;

    // Children are serialised so a memory-quota'd host never carries two
    // scenarios at once. Poisoning is irrelevant — the lock only serialises.
    static SPAWN_LOCK: Mutex<()> = Mutex::new(());
    let _serialise = SPAWN_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let exe = std::env::current_exe().expect("current_exe resolves this test binary");
    let out = std::process::Command::new(&exe)
        .args(["--exact", scenario])
        .env(SCENARIO_CHILD, "1")
        .output()
        .unwrap_or_else(|e| panic!("failed to re-spawn this binary for scenario {scenario}: {e}"));
    assert!(
        out.status.success(),
        "fresh-process scenario {scenario} failed ({status}):\n\
         --- child stdout ---\n{stdout}\n\
         --- child stderr ---\n{stderr}",
        status = out.status,
        stdout = String::from_utf8_lossy(&out.stdout),
        stderr = String::from_utf8_lossy(&out.stderr),
    );
}

// ---------------------------------------------------------------------------
// Scenario bodies — child-side only (the stub target never spawns them), so
// each may assert the real-platform contract outright.
// ---------------------------------------------------------------------------

/// Child body of `touching_memory_grows_rss`.
#[cfg(all(any(target_os = "linux", windows, target_os = "macos"), not(miri)))]
fn scenario_touching_memory_grows_rss() {
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

    assert!(
        after.rss > before.rss,
        "rss must grow after touching {N} bytes: before={} after={}",
        before.rss,
        after.rss
    );
}

/// Child body of `peak_rss_is_monotonic`.
#[cfg(all(any(target_os = "linux", windows, target_os = "macos"), not(miri)))]
fn scenario_peak_rss_is_monotonic() {
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

    // The scenario only ever runs on real platforms (the stub target never
    // spawns it), so a `None` peak here is a broken backend, NOT a stub —
    // the old `(None, None)` arm accepted exactly that failure (review P3-3).
    let before_peak = before
        .peak_rss
        .expect("a real platform must report peak_rss");
    let after_peak = after
        .peak_rss
        .expect("a real platform must report peak_rss");
    assert!(
        after_peak >= before_peak,
        "peak_rss must be non-decreasing: before={before_peak} after={after_peak}"
    );
}

/// Child body of `committing_without_touching_grows_commit_not_rss`
/// (Windows-only: this needs the OS's distinct commit-charge accounting).
#[cfg(all(windows, not(miri)))]
fn scenario_committing_without_touching() {
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
    // returning. VirtualAlloc failure is an assert (not a skip): a fresh
    // process has no legitimate reason to refuse 64 MiB of commit. Should any
    // assertion below fail, the whole scenario PROCESS dies and Windows
    // reclaims the committed region with it — nothing leaks into sibling
    // tests.
    /// Owns the committed region: releases it on drop, so an assertion
    /// failure between the alloc and the release cannot leak it (review
    /// P4-3). The scenario process dying would also reclaim the region, but
    /// that makes the cleanup a property of process teardown rather than of
    /// this code — a guard says it locally and keeps holding if the body is
    /// ever reused somewhere that does not exit.
    struct CommittedRegion(*mut core::ffi::c_void);
    impl Drop for CommittedRegion {
        fn drop(&mut self) {
            // SAFETY: `self.0` came from the `VirtualAlloc` below and is
            // released exactly once, here. Size 0 is the documented
            // MEM_RELEASE contract.
            let freed = unsafe { VirtualFree(self.0, 0, MEM_RELEASE) };
            if freed == 0 {
                // Deliberately not a panic (on the unwind path a panicking
                // Drop aborts the process and would replace the real
                // assertion message with a far less useful one) — and
                // deliberately not `eprintln!` either: it panics if writing
                // to stderr fails, and that panic inside `drop` during an
                // unwind aborts the process just the same. A raw `write_all`
                // with the `Result` discarded is the same best-effort
                // diagnostic without the panic path.
                let _ = std::io::Write::write_all(
                    &mut std::io::stderr(),
                    b"warning: VirtualFree(MEM_RELEASE) failed during cleanup\n",
                );
            }
        }
    }

    unsafe {
        let p = VirtualAlloc(
            core::ptr::null_mut(),
            N,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );
        assert!(!p.is_null(), "VirtualAlloc(MEM_COMMIT) failed");
        // From here on the region is owned; every exit path releases it.
        let region = CommittedRegion(p);

        let after = snapshot();

        // Commit charge grew by ~N (allow slack for concurrent activity).
        // Windows is the one platform where `commit_charge` is `Some`; a
        // `None` here would mean the backend stopped reporting the counter,
        // which is a failure, not a skip.
        let before_commit = before
            .commit_charge
            .expect("Windows must report commit_charge (PagefileUsage)");
        let after_commit = after
            .commit_charge
            .expect("Windows must report commit_charge (PagefileUsage)");
        assert!(
            after_commit >= before_commit + (N as u64) / 2,
            "commit_charge must grow by ~{N} after MEM_COMMIT: before={before_commit} after={after_commit}"
        );
        // RSS did NOT grow by anything like N — the pages were never touched,
        // so at most incidental noise moved it. Assert it did not grow by even
        // a quarter of the committed span. In the fresh scenario process there
        // are no neighbour tests to move RSS (review P3-2) — only this
        // process's own noise, which is far below N/4.
        assert!(
            after.rss < before.rss + (N as u64) / 4,
            "rss must NOT grow from untouched commit: before={} after={} (N={N})",
            before.rss,
            after.rss
        );

        // Explicit, checked release on the happy path — a failure here is a
        // real defect and must be loud, unlike the Drop fallback above.
        let raw = region.0;
        core::mem::forget(region);
        let freed = VirtualFree(raw, 0, MEM_RELEASE);
        assert!(freed != 0, "VirtualFree(MEM_RELEASE) failed");
    }
}

// ---------------------------------------------------------------------------
// Tests. Read-only ones run in-process; memory-moving ones dispatch a child.
// ---------------------------------------------------------------------------

/// A `snapshot()` on any platform must not panic and must be internally
/// consistent (peak_rss, when present, is at least the live rss).
///
/// Read-only — moves no process memory, so unlike the scenarios it needs no
/// fresh-process isolation. That `peak_rss` must be PRESENT on every real
/// platform (this `if let` alone survives a backend that stopped producing
/// it, and a KiB-for-bytes scale error) is asserted separately by
/// `tests/platform_contract.rs` (review P3-3).
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
#[cfg(all(any(target_os = "linux", windows, target_os = "macos"), not(miri)))]
#[test]
fn touching_memory_grows_rss() {
    if std::env::var_os(SCENARIO_CHILD).is_some() {
        scenario_touching_memory_grows_rss();
    } else {
        run_scenario_alone("touching_memory_grows_rss");
    }
}

/// `peak_rss` is monotonic non-decreasing across two snapshots that straddle
/// a large touch. Real platforms expose a peak counter; there, a `None` is a
/// broken backend and must fail (see the scenario body), not skip.
#[cfg(all(any(target_os = "linux", windows, target_os = "macos"), not(miri)))]
#[test]
fn peak_rss_is_monotonic() {
    if std::env::var_os(SCENARIO_CHILD).is_some() {
        scenario_peak_rss_is_monotonic();
    } else {
        run_scenario_alone("peak_rss_is_monotonic");
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
    if std::env::var_os(SCENARIO_CHILD).is_some() {
        scenario_committing_without_touching();
    } else {
        run_scenario_alone("committing_without_touching_grows_commit_not_rss");
    }
}
