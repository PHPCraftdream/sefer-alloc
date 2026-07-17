//! `proc-memstat` — same-instant self-probe of a process's own memory.
//!
//! One call — [`snapshot`] — returns a [`MemStat`] carrying three memory
//! figures read as close to the same instant as the OS permits:
//!
//! - **`rss`** — resident set size: the physical memory currently backing the
//!   process's pages (what "top" shows as RES / working set).
//! - **`commit`** — commit charge: memory *charged against the system commit
//!   limit*, whether or not it has been faulted in yet. This is a **separate
//!   axis** from RSS — a `VirtualAlloc(MEM_COMMIT)` (or an over-committing
//!   reservation) shows up here even while it is demand-zero and therefore
//!   invisible to RSS. Commit charge is rarely surfaced by existing Rust
//!   crates, and it is the metric that catches commit-heavy designs RSS hides.
//! - **`peak_rss`** — the high-water mark of RSS, where the OS exposes it
//!   (`Some`), or `None` where it does not.
//!
//! **All fields are in bytes.** (Note: the Linux `/proc` and the KiB-oriented
//! callers should convert at the boundary — this crate deals only in bytes.)
//!
//! # Why not `sysinfo`?
//!
//! `sysinfo` is a heavy, whole-system crate with many dependencies. This crate
//! does one narrow thing — *my own* process, three counters, one struct, zero
//! dependencies — and it surfaces **commit charge**, which the whole-system
//! crates almost never do.
//!
//! # Platform matrix
//!
//! | Platform | `rss` | `commit` | `peak_rss` |
//! |----------|-------|----------|------------|
//! | Linux    | `/proc/self/statm` resident × page size | `/proc/self/statm` size (total VM) × page size | `/proc/self/status` `VmHWM` (`Some`) |
//! | Windows  | `K32GetProcessMemoryInfo` `WorkingSetSize` | `PagefileUsage` | `PeakWorkingSetSize` (`Some`) |
//! | macOS    | `task_info(MACH_TASK_BASIC_INFO)` `resident_size` | `virtual_size` | `resident_size_max` (`Some`) |
//! | other    | `0` | `0` | `None` |
//!
//! Linux's overcommit accounting is not identical to Windows' commit-charge
//! model; `commit` there is "total program size" (virtual memory), the nearest
//! Linux analogue. On unknown targets the crate reports honest zeros rather
//! than a fabricated number.
//!
//! Runnable form of the examples: `tests/monotonicity.rs`.

// This crate's entire purpose is the `unsafe` OS FFI that reads the calling
// process's own memory counters (Windows `K32GetProcessMemoryInfo`, macOS
// `task_info`; Linux/other are pure safe `/proc` parsing). The public API is
// safe — every `unsafe` block is confined to the platform modules below and
// carries a `// SAFETY:` proof; every `unsafe fn` carries a `# Safety` note.
#![allow(unsafe_code)]
#![deny(missing_docs)]

/// A same-instant snapshot of the calling process's own memory usage, in
/// **bytes**.
///
/// Produced by [`snapshot`]. The three fields are read from one OS query (or,
/// on Linux, two adjacent `/proc` reads) so that comparing `commit` against
/// `rss` is apples-to-apples: both describe the same moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemStat {
    /// Resident set size in bytes — physical memory currently backing the
    /// process (Windows `WorkingSetSize`, Linux `statm` resident, macOS
    /// `resident_size`). `0` on unknown platforms.
    pub rss: u64,
    /// Commit charge in bytes — memory charged against the system commit
    /// limit, whether or not faulted in yet (Windows `PagefileUsage`, Linux
    /// `statm` total program size, macOS `virtual_size`). A **separate axis**
    /// from `rss`. `0` on unknown platforms.
    pub commit: u64,
    /// Peak (high-water) resident set size in bytes, where the OS exposes it
    /// (Windows `PeakWorkingSetSize`, Linux `/proc/self/status` `VmHWM`, macOS
    /// `resident_size_max`); `None` on platforms without a peak-RSS counter.
    pub peak_rss: Option<u64>,
}

/// Read the calling process's current memory counters as one [`MemStat`]
/// (bytes), from a single OS query so `rss` and `commit` describe the same
/// instant.
///
/// On any read failure, or on an unknown target, the affected field falls back
/// to `0` (or `peak_rss` to `None`) rather than panicking — a probe must never
/// take the process down.
#[must_use]
pub fn snapshot() -> MemStat {
    platform::snapshot()
}

// ---------------------------------------------------------------------------
// Linux — /proc parsing (pure safe code, no FFI).
// ---------------------------------------------------------------------------
#[cfg(all(target_os = "linux", not(miri)))]
mod platform {
    use super::MemStat;

    /// Page size in bytes. `sysconf(_SC_PAGESIZE)` would need libc; 4 KiB is
    /// correct on every x86_64/aarch64-4k Linux host this crate is exercised
    /// on, and `/proc/self/statm` is expressed in pages. Documented as a rough
    /// probe, not a precise accountant on exotic 16k/64k-page kernels.
    const PAGE_SIZE: u64 = 4096;

    pub(super) fn snapshot() -> MemStat {
        let (rss_pages, size_pages) = read_statm();
        MemStat {
            rss: rss_pages * PAGE_SIZE,
            commit: size_pages * PAGE_SIZE,
            peak_rss: read_vmhwm_bytes(),
        }
    }

    /// `(resident_pages, size_pages)` from `/proc/self/statm`
    /// ("size resident shared text lib data dt", in pages): field 1 is
    /// resident, field 0 is total program size (virtual memory).
    fn read_statm() -> (u64, u64) {
        let statm = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
        let mut it = statm.split_whitespace();
        let size = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let resident = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        (resident, size)
    }

    /// Peak RSS from `/proc/self/status`'s `VmHWM:` line (reported in KiB),
    /// converted to bytes. `None` if the field is absent/unreadable.
    fn read_vmhwm_bytes() -> Option<u64> {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmHWM:") {
                // Format: "VmHWM:\t   1234 kB"
                let kib: u64 = rest.split_whitespace().next()?.parse().ok()?;
                return Some(kib * 1024);
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Windows — K32GetProcessMemoryInfo.
// ---------------------------------------------------------------------------
#[cfg(all(windows, not(miri)))]
mod platform {
    use super::MemStat;

    /// `PROCESS_MEMORY_COUNTERS` (the base, non-`_EX` variant). Declared
    /// locally so this crate needs no `windows-sys`/`winapi` dependency; `std`
    /// already links `kernel32`, which exports the `K32`-prefixed
    /// `GetProcessMemoryInfo`.
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(
            process: isize,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    pub(super) fn snapshot() -> MemStat {
        // SAFETY: `counters` is a valid, sufficiently-sized, mutable
        // out-parameter zero-initialised with its `cb` field set to the
        // struct size, exactly as `GetProcessMemoryInfo` documents;
        // `GetCurrentProcess` returns a pseudo-handle that needs no close.
        // On failure (`ok == 0`) we do not read the (untouched) counters.
        unsafe {
            let mut counters: ProcessMemoryCounters = core::mem::zeroed();
            counters.cb = core::mem::size_of::<ProcessMemoryCounters>() as u32;
            let ok = K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb);
            if ok == 0 {
                MemStat::default()
            } else {
                MemStat {
                    rss: counters.working_set_size as u64,
                    commit: counters.pagefile_usage as u64,
                    peak_rss: Some(counters.peak_working_set_size as u64),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// macOS — task_info(MACH_TASK_BASIC_INFO).
// ---------------------------------------------------------------------------
#[cfg(all(target_os = "macos", not(miri)))]
mod platform {
    use super::MemStat;

    // `mach_task_basic_info` (flavor `MACH_TASK_BASIC_INFO`). The count is
    // expressed in `natural_t` (u32) units of the struct.
    const MACH_TASK_BASIC_INFO: u32 = 20;

    #[repr(C)]
    struct MachTaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time: [i32; 2],
        system_time: [i32; 2],
        policy: i32,
        suspend_count: i32,
    }

    extern "C" {
        fn mach_task_self() -> u32;
        fn task_info(
            target_task: u32,
            flavor: u32,
            task_info_out: *mut i32,
            task_info_out_count: *mut u32,
        ) -> i32;
    }

    pub(super) fn snapshot() -> MemStat {
        const COUNT: u32 =
            (core::mem::size_of::<MachTaskBasicInfo>() / core::mem::size_of::<i32>()) as u32;
        // SAFETY: `info` is a valid, mutable out-parameter of exactly `COUNT`
        // `i32` units; `count` is initialised to that capacity as `task_info`
        // requires. `mach_task_self` returns the caller's task port (no
        // ownership transfer / no deallocation needed here). On any non-zero
        // (error) return we ignore the untouched `info`.
        unsafe {
            let mut info: MachTaskBasicInfo = core::mem::zeroed();
            let mut count: u32 = COUNT;
            let kr = task_info(
                mach_task_self(),
                MACH_TASK_BASIC_INFO,
                (&mut info as *mut MachTaskBasicInfo).cast::<i32>(),
                &mut count,
            );
            if kr != 0 {
                MemStat::default()
            } else {
                MemStat {
                    rss: info.resident_size,
                    // `virtual_size` is the honest macOS analogue of commit
                    // charge (total mapped virtual memory); macOS has no
                    // Windows-style separate commit-charge counter.
                    commit: info.virtual_size,
                    peak_rss: Some(info.resident_size_max),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Stub — miri and unknown targets: honest zeros.
// ---------------------------------------------------------------------------
#[cfg(any(miri, not(any(target_os = "linux", windows, target_os = "macos"))))]
mod platform {
    use super::MemStat;

    pub(super) fn snapshot() -> MemStat {
        // No cheap, dependency-free self-memory read on this target (or under
        // miri, which has no real OS memory accounting). Report honest zeros /
        // `None` rather than a fabricated figure.
        MemStat::default()
    }
}
