//! `proc-memstat` — single-read self-probe of a process's own memory.
//!
//! One call — [`snapshot`] — returns a [`MemStat`] carrying four memory
//! figures gathered from ONE source in one go:
//!
//! - **`rss`** — resident set size: the physical memory currently backing the
//!   process's pages (what "top" shows as RES / working set).
//! - **`commit_charge`** — memory *charged against the system commit limit*,
//!   whether or not it has been faulted in yet. This is a **separate axis**
//!   from RSS — a `VirtualAlloc(MEM_COMMIT)` shows up here even while it is
//!   demand-zero and therefore invisible to RSS. `None` where the platform
//!   API used here does not expose this counter.
//! - **`virtual_size`** — the size of the process's virtual ADDRESS SPACE.
//!   Deliberately a different field from `commit_charge`, not a synonym: a
//!   large read-only file mapping raises `virtual_size` while costing nothing
//!   in Linux overcommit accounting, and reserved-but-uncommitted address
//!   space likewise is not memory anything has been charged for. `None` where
//!   the platform API used here does not expose it.
//! - **`peak_rss`** — the high-water mark of RSS, where the OS exposes it
//!   (`Some`), or `None` where it does not.
//!
//! **All fields are in bytes.** (Note: the Linux `/proc` and the KiB-oriented
//! callers should convert at the boundary — this crate deals only in bytes.)
//!
//! # Why not `sysinfo`?
//!
//! `sysinfo` is a heavy, whole-system crate with many dependencies. This crate
//! does one narrow thing — *my own* process, a handful of counters, one
//! struct, zero dependencies — and it surfaces **commit charge**, which the
//! whole-system crates almost never do.
//!
//! # Platform matrix
//!
//! | Platform | `rss` | `virtual_size` | `commit_charge` | `peak_rss` |
//! |----------|-------|----------------|-----------------|------------|
//! | Linux    | `/proc/self/status` `VmRSS` | `VmSize` (`Some`) | `None` | `VmHWM` (`Some`) |
//! | Windows  | `K32GetProcessMemoryInfo` `WorkingSetSize` | `None` | `PagefileUsage` (`Some`) | `PeakWorkingSetSize` (`Some`) |
//! | macOS    | `task_info(MACH_TASK_BASIC_INFO)` `resident_size` | `virtual_size` (`Some`) | `None` | `resident_size_max` (`Some`) |
//! | other    | `0` | `None` | `None` | `None` |
//!
//! **Why the `commit_charge` column is `None` on two of the three platforms.**
//! Windows `PagefileUsage` is documented commit charge. Linux `VmSize` and
//! macOS `virtual_size` are the size of the virtual address space — Apple's
//! `task_info.h` names that field "virtual memory size" outright — and Linux
//! overcharges nothing for a read-only file mapping that nonetheless inflates
//! `VmSize`. Reporting either under the name "commit charge" would invert the
//! meaning of a reserve/commit/decommit measurement: retained address space
//! would read as retained commit. A shared unit (bytes) is not a shared
//! measured quantity, so this crate reports each counter under its own name
//! and leaves the other absent rather than substituting a "nearest analogue".
//! Neither `PROCESS_MEMORY_COUNTERS` (the Windows struct read here) nor this
//! Mach flavor's counterpart carries the other platform's field, which is why
//! the absences are structural rather than merely unimplemented.
//!
//! All three Linux fields come from `/proc/self/status`, whose
//! values are reported in kB regardless of the kernel's base page size — unlike
//! `/proc/self/statm` (expressed in pages), this needs no page-size query and
//! stays correct on 16 KiB/64 KiB-page kernels (aarch64, ppc64, etc.). On
//! unknown targets the crate reports honest zeros rather than a fabricated
//! number.
//!
//! Runnable form of the examples: `tests/monotonicity.rs`.

// This crate's entire purpose is the `unsafe` OS FFI that reads the calling
// process's own memory counters (Windows `K32GetProcessMemoryInfo`, macOS
// `task_info`; Linux/other are pure safe `/proc` parsing). The public API is
// safe — every `unsafe` block is confined to the platform modules below and
// carries a `// SAFETY:` proof; every `unsafe fn` carries a `# Safety` note.
#![allow(unsafe_code)]
#![deny(missing_docs)]

/// A best-effort observation of the calling process's own memory usage, in
/// **bytes**.
///
/// Produced by [`snapshot`] from one source — on Linux, one
/// `/proc/self/status` read. That narrows the window between the figures; it
/// does NOT make them atomic, and this type does not claim it does (review
/// P3-1). The kernel documents RSS accounting as asynchronous and possibly
/// inexact, and `task_mem` — the procfs code producing these very lines —
/// reads the anon/file/shmem totals and `total_vm` in separate operations,
/// with its own comment permitting inconsistent snapshots. At the `std` level
/// a file read is not one syscall either.
///
/// So: comparing two fields of one `MemStat` is far better than two separate
/// `snapshot()` calls, and is the intended use — but a comparison that would
/// be WRONG if the fields were microseconds apart needs a stronger mechanism
/// than this crate offers.
///
/// One relation that looks like same-instant evidence but is not: on current
/// Linux `peak_rss >= rss` holds because both derive from a shared
/// `total_rss`. That is correct, and it is not proof the other fields belong
/// to the same moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemStat {
    /// Resident set size in bytes — physical memory currently backing the
    /// process (Windows `WorkingSetSize`, Linux `/proc/self/status` `VmRSS`,
    /// macOS `resident_size`). `0` on unknown platforms.
    pub rss: u64,
    /// Size of the process's virtual ADDRESS SPACE in bytes (Linux
    /// `/proc/self/status` `VmSize`, macOS `virtual_size`); `None` where the
    /// API this crate uses does not expose it, which includes Windows —
    /// `PROCESS_MEMORY_COUNTERS` carries no virtual-size field.
    ///
    /// This is **not** commit charge and must not be read as one: address
    /// space can be mapped without anything being charged for it (a
    /// read-only file mapping is the standard example), and it can be
    /// reserved without being committed at all.
    pub virtual_size: Option<u64>,
    /// Memory charged against the system commit limit, in bytes, whether or
    /// not it has been faulted in yet (Windows `PagefileUsage`); `None` where
    /// the API this crate uses does not expose it, which is both Linux and
    /// macOS — neither `/proc/self/status` nor `MACH_TASK_BASIC_INFO` carries
    /// a commit-charge counter, and [`virtual_size`](Self::virtual_size) is a
    /// different quantity, not a stand-in for this one.
    ///
    /// A **separate axis** from `rss`: committed-but-untouched memory appears
    /// here and not in `rss`.
    pub commit_charge: Option<u64>,
    /// Peak (high-water) resident set size in bytes, where the OS exposes it
    /// (Windows `PeakWorkingSetSize`, Linux `/proc/self/status` `VmHWM`, macOS
    /// `resident_size_max`); `None` on platforms without a peak-RSS counter.
    pub peak_rss: Option<u64>,
}

impl MemStat {
    /// [`commit_charge`](Self::commit_charge) where the platform provides it,
    /// otherwise [`virtual_size`](Self::virtual_size), otherwise `0` — that
    /// is, the single figure this crate reported under the name `commit`
    /// before those two were split into separate fields.
    ///
    /// **The two are not the same quantity.** Read the field docs above: one
    /// is memory charged against the commit limit, the other is address
    /// space, and on any given platform this returns whichever of them that
    /// OS happens to expose. It is therefore a PLATFORM-DEPENDENT reading,
    /// and a caller that publishes the number owes its readers the platform
    /// alongside it — otherwise a Linux `virtual_size` and a Windows
    /// `commit_charge` end up compared as if they measured the same thing,
    /// which is exactly the confusion the split exists to prevent.
    ///
    /// Provided so existing measurement harnesses keep producing byte-
    /// identical numbers across the split. New code should read the fields
    /// directly and say which one it read.
    #[must_use]
    pub fn charged_or_reserved_bytes(&self) -> u64 {
        self.commit_charge.or(self.virtual_size).unwrap_or(0)
    }
}

/// Why a [`try_snapshot`] call could not produce a reading.
///
/// Exists because a failed read and a genuinely tiny process are otherwise
/// indistinguishable through [`snapshot`], which reports zeros either way
/// (review P4-1): a before/after pair whose second read failed looks exactly
/// like a complete release of memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SnapshotError {
    /// This target has no self-memory source this crate can read — the
    /// `other` row of the platform matrix, and every target under miri.
    ///
    /// Not a failure of anything: it will not start working on a retry, so
    /// callers should treat it as "this measurement is unavailable here",
    /// not as an error to report.
    Unsupported,
    /// The platform query itself failed: `/proc/self/status` could not be
    /// read, or `K32GetProcessMemoryInfo` / `task_info` returned an error.
    Os,
    /// The source was read, but a required field was absent or not a plain
    /// ASCII integer — a `/proc/self/status` without `VmRSS`, for instance.
    ///
    /// Distinguished from [`Os`](Self::Os) because it points at the CONTENT
    /// rather than at the access: a caller that sees this is looking at a
    /// procfs whose shape this crate does not understand.
    Malformed,
}

impl core::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            Self::Unsupported => "no self-memory source on this target",
            Self::Os => "the platform memory query failed",
            Self::Malformed => "the platform reported memory data in an unexpected shape",
        };
        f.write_str(s)
    }
}

impl std::error::Error for SnapshotError {}

/// Read the calling process's current memory counters as one [`MemStat`]
/// (bytes), reporting WHY on failure.
///
/// Prefer this over [`snapshot`] whenever the difference between "memory was
/// released" and "the reading failed" matters — comparing a before/after pair
/// is exactly that case. Returning an error does not panic and does not stop
/// the measured process; it just declines to invent a number.
pub fn try_snapshot() -> Result<MemStat, SnapshotError> {
    platform::try_snapshot()
}

/// Read the calling process's current memory counters as one [`MemStat`]
/// (bytes), from a single OS query.
///
/// Best-effort: on any read failure, or on an unknown target, the whole
/// reading falls back to [`MemStat::default`] — `rss: 0` and `None` for the
/// optional fields — rather than panicking.
///
/// **That fallback is indistinguishable from a genuinely near-zero process.**
/// Use [`try_snapshot`] when that distinction matters.
///
/// # What "never takes the process down" does and does not mean
///
/// It means this function does not panic when the platform read fails: a
/// probe that cannot measure still returns.
///
/// It does NOT mean the call is free of side effects on the memory it is
/// measuring, and on Linux specifically it is not (review P4-2). That backend
/// reads `/proc/self/status` through `std::fs::read`, which ALLOCATES a fresh
/// buffer per call. Two consequences worth stating rather than leaving to be
/// discovered:
///
/// - Calling this from inside a global-allocator hook can RE-ENTER the
///   allocator. Whether that is safe is a property of the allocator, not of
///   this crate; if it is not reentrant, this call is not safe to make there.
/// - Under memory exhaustion the allocation itself can fail, and Rust's
///   allocation-failure path is not something this function can intercept.
///
/// Windows and macOS carry neither caveat: those backends fill a stack struct
/// and allocate nothing.
#[must_use]
pub fn snapshot() -> MemStat {
    try_snapshot().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Linux — /proc parsing (pure safe code, no FFI).
// ---------------------------------------------------------------------------
// The byte-level field parser lives in its own dependency-free file so
// `tests/status_parse.rs` can pull it in with `#[path]` and cover it on any
// host, /proc or not, without this crate exposing it publicly (review P2-3).
#[cfg(all(target_os = "linux", not(miri)))]
mod status_parse;

#[cfg(all(target_os = "linux", not(miri)))]
mod platform {
    use super::status_parse::read_kib_field;
    use super::{MemStat, SnapshotError};

    pub(super) fn try_snapshot() -> Result<MemStat, SnapshotError> {
        // `read` (bytes), NOT `read_to_string`: a non-UTF-8 task name must not
        // be able to zero out the numeric fields — see `status_parse`.
        let status = std::fs::read("/proc/self/status").map_err(|_| SnapshotError::Os)?;
        // VmRSS is the one field with no `Option` to express absence, so a
        // procfs without it is Malformed rather than a silent zero.
        let rss = read_kib_field(&status, b"VmRSS:").ok_or(SnapshotError::Malformed)?;
        Ok(MemStat {
            rss: rss * 1024,
            virtual_size: read_kib_field(&status, b"VmSize:").map(|kib| kib * 1024),
            // `/proc/self/status` exposes no commit-charge counter; `VmSize`
            // above is address space, a different quantity (see `MemStat`).
            commit_charge: None,
            peak_rss: read_kib_field(&status, b"VmHWM:").map(|kib| kib * 1024),
        })
    }
}

// ---------------------------------------------------------------------------
// Windows — K32GetProcessMemoryInfo.
// ---------------------------------------------------------------------------
#[cfg(all(windows, not(miri)))]
mod platform {
    use super::{MemStat, SnapshotError};

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

    pub(super) fn try_snapshot() -> Result<MemStat, SnapshotError> {
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
                Err(SnapshotError::Os)
            } else {
                Ok(MemStat {
                    rss: counters.working_set_size as u64,
                    // `PROCESS_MEMORY_COUNTERS` has no virtual-size field;
                    // obtaining one needs a different API entirely.
                    virtual_size: None,
                    commit_charge: Some(counters.pagefile_usage as u64),
                    peak_rss: Some(counters.peak_working_set_size as u64),
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// macOS — task_info(MACH_TASK_BASIC_INFO).
// ---------------------------------------------------------------------------
#[cfg(all(target_os = "macos", not(miri)))]
mod platform {
    use super::{MemStat, SnapshotError};

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
        // `mach_task_self()` is NOT a C function: the public Mach API defines
        // it as a macro over this cached global — `#define mach_task_self()
        // mach_task_self_` (Apple libsyscall, libsyscall/mach/mach/mach_init.h,
        // which declares only `extern mach_port_t mach_task_self_`) — so there
        // is no `_mach_task_self` function symbol to link against. Bind the
        // data symbol itself, the same shape rust-lang/libc uses
        // (`pub static mut mach_task_self_: mach_port_t` in
        // src/unix/bsd/apple/mod.rs; `mach_port_t` = `c_uint` = u32).
        static mut mach_task_self_: u32;
        fn task_info(
            target_task: u32,
            flavor: u32,
            task_info_out: *mut i32,
            task_info_out_count: *mut u32,
        ) -> i32;
    }

    /// The calling task's own task-port name — the value the public
    /// `mach_task_self()` C macro expands to (a variable read, not a call;
    /// see the extern block above). The name refers to a cached reference to
    /// the caller's OWN task held by libSystem, not a newly acquired send
    /// right, so it must never be `mach_port_deallocate`d.
    fn mach_task_self() -> u32 {
        // SAFETY: `mach_task_self_` is a `mach_port_t` global exported by
        // libSystem (which std links on macOS), written only by
        // `mach_init_doit()` (`mach_task_self_ = task_self_trap()`,
        // libsyscall/Mach-side mach_init.c) during `libSystem_initializer` —
        // before any user code runs in the process image — so this read can
        // neither race with a write nor observe uninitialised memory. It is a
        // plain `u32` load of a port NAME copied by value: it acquires no Mach
        // right and transfers no ownership.
        unsafe { mach_task_self_ }
    }

    pub(super) fn try_snapshot() -> Result<MemStat, SnapshotError> {
        const COUNT: u32 =
            (core::mem::size_of::<MachTaskBasicInfo>() / core::mem::size_of::<i32>()) as u32;
        // SAFETY: `info` is a valid, mutable out-parameter of exactly `COUNT`
        // `i32` units; `count` is initialised to that capacity as `task_info`
        // requires. The `target_task` argument is the bare self-port name from
        // `mach_task_self` (see the helper above) — `task_info` consumes no
        // Mach right, so nothing needs deallocating afterwards. On any
        // non-zero (error) return we ignore the untouched `info`.
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
                Err(SnapshotError::Os)
            } else {
                Ok(MemStat {
                    rss: info.resident_size,
                    virtual_size: Some(info.virtual_size),
                    // Apple names this flavor's field "virtual memory size";
                    // it is address space, not commit charge, and this flavor
                    // exposes no commit-charge counter at all.
                    commit_charge: None,
                    peak_rss: Some(info.resident_size_max),
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Stub — miri and unknown targets: honest zeros.
// ---------------------------------------------------------------------------
#[cfg(any(miri, not(any(target_os = "linux", windows, target_os = "macos"))))]
mod platform {
    use super::{MemStat, SnapshotError};

    pub(super) fn try_snapshot() -> Result<MemStat, SnapshotError> {
        // No cheap, dependency-free self-memory read on this target (or under
        // miri, which has no real OS memory accounting). Say so, rather than
        // returning a fabricated figure a caller cannot tell apart from a real
        // near-zero reading. `snapshot()` still maps this to honest zeros for
        // callers that want the best-effort shape.
        Err(SnapshotError::Unsupported)
    }
}
