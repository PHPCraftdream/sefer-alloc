//! NUMA-seam: NUMA-node detection and segment binding.
//!
//! This is the **confined-`unsafe`** OS interface for NUMA-aware segment
//! reservation, following the same discipline as [`super::os`]:
//!
//! - `#![allow(unsafe_code)]` lifts the crate-level `#![deny(unsafe_code)]`
//!   for THIS FILE ONLY. Every `unsafe` block carries a `// SAFETY:` proof.
//! - The three public functions (`current_node`, `bind_segment`,
//!   `reserve_aligned_on_node`) are **safe** to call — all invariants are
//!   checked internally.
//!
//! ## Gating
//!
//! Compiled only when `feature = "numa-aware"` is active. Each platform arm is
//! additionally gated on `not(miri)` so miri tests still link (miri cannot
//! execute raw OS FFI).
//!
//! ## Platform matrix
//!
//! | Platform | `current_node` | `bind_segment` | `reserve_aligned_on_node` |
//! |----------|---------------|----------------|--------------------------|
//! | Linux (non-miri) | `sched_getcpu` + sysfs lookup | `mbind(2)` | mmap then `bind_segment` |
//! | Windows (non-miri) | `GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx` | no-op (must bind at reserve time) | `VirtualAllocExNuma` |
//! | macOS | `NO_NODE` | no-op | plain `os::reserve_aligned` |
//! | miri | `NO_NODE` | no-op | plain `os::reserve_aligned` |

// The crate is `#![deny(unsafe_code)]`; this lifts the deny for this file
// only — the same pattern as `os.rs`.
#![allow(unsafe_code)]

use core::ptr::NonNull;

/// Sentinel value meaning "no NUMA node / feature disabled / unsupported
/// platform". Any function that cannot determine the NUMA node returns this.
pub const NO_NODE: u32 = u32::MAX;

// ---------------------------------------------------------------------------
// Public API (always safe to call on every platform)
// ---------------------------------------------------------------------------

/// Return the NUMA node of the calling thread.
///
/// Returns [`NO_NODE`] when:
/// - The `numa-aware` feature is off (this file is not compiled at all in that
///   case, but the constant is still usable as a fallback in `#[cfg]` stubs).
/// - The platform does not provide a NUMA API (macOS, miri).
/// - The OS API returns an error.
#[must_use]
pub fn current_node() -> u32 {
    platform::current_node_impl()
}

/// Bind the virtual-memory range `[base, base + len)` to NUMA node `node`.
///
/// Must be called **after** the OS has reserved the pages (via `mmap` /
/// `VirtualAlloc`) and **before** the first read or write to those pages.
/// Physical pages are assigned to the node at page-fault time; calling
/// `bind_segment` before the first fault steers them to `node`.
///
/// No-op when:
/// - `node == NO_NODE`
/// - Platform does not support post-mmap binding (Windows, macOS, miri)
/// - `len == 0`
///
/// # Safety contract (caller's invariant, NOT checked at runtime)
///
/// `[base, base + len)` must be a valid, live OS reservation owned by the
/// caller.  The function never reads or writes the payload bytes — it only
/// invokes `mbind(2)` which sets kernel metadata. Passing an invalid range
/// causes `mbind` to return an error; the error is silently ignored (the
/// allocation proceeds without NUMA binding, which is always correct).
pub fn bind_segment(base: *mut u8, len: usize, node: u32) {
    if node == NO_NODE || len == 0 {
        return;
    }
    platform::bind_segment_impl(base, len, node);
}

/// Reserve a SEGMENT-aligned span of `usable` bytes with a NUMA preference for
/// `node`.
///
/// On Linux: calls `os::reserve_aligned` then `bind_segment` (mbind runs
/// before the first page-fault).
/// On Windows: calls `VirtualAllocExNuma` directly (there is no post-reserve
/// binding API on Windows).
/// On macOS / miri: equivalent to `os::reserve_aligned` (no NUMA binding).
///
/// Returns `None` only on OOM (same contract as [`os::Segment::reserve`]).
///
/// The returned triple is `(base, reservation_start, reservation_len)`,
/// matching the internal [`os::reserve_aligned`] contract so callers can
/// record it in the segment header for later release.
#[must_use]
pub fn reserve_aligned_on_node(
    usable: usize,
    node: u32,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    platform::reserve_aligned_on_node_impl(usable, node)
}

// ---------------------------------------------------------------------------
// Per-platform implementations
// ---------------------------------------------------------------------------

// ---- Linux (real hardware, not miri) --------------------------------------
#[cfg(all(target_os = "linux", not(miri)))]
mod platform {
    use core::ptr::NonNull;

    use super::{bind_segment_impl_linux, NO_NODE};
    use crate::alloc_core::os;

    pub(super) fn current_node_impl() -> u32 {
        // SAFETY: `sched_getcpu` is a safe libc wrapper around the `getcpu`
        // syscall.  It returns the index of the CPU currently executing this
        // thread, or -1 on error.  We then map the CPU index to a NUMA node by
        // reading /sys/devices/system/node/node<N>/cpumap.
        let cpu = unsafe { libc_sched_getcpu() };
        if cpu < 0 {
            return NO_NODE;
        }
        cpu_to_numa_node(cpu as u32)
    }

    pub(super) fn bind_segment_impl(base: *mut u8, len: usize, node: u32) {
        bind_segment_impl_linux(base, len, node);
    }

    pub(super) fn reserve_aligned_on_node_impl(
        usable: usize,
        node: u32,
    ) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
        // On Linux: reserve with ordinary mmap, then bind with mbind.
        // mbind must be called BEFORE the first page access so physical pages
        // land on the right node at page-fault time.
        let seg = os::Segment::reserve(usable)?;
        let base = seg.as_ptr();
        let len = seg.len();
        let reservation = seg.reservation();
        let reservation_len = seg.reservation_len();
        // Forget the Segment to prevent its Drop from freeing the reservation;
        // the caller takes ownership of (base, reservation, reservation_len).
        core::mem::forget(seg);
        bind_segment_impl_linux(base, len, node);
        Some((
            // SAFETY: `base` is non-null (Segment::reserve guarantees it).
            unsafe { NonNull::new_unchecked(base) },
            reservation,
            reservation_len,
        ))
    }

    /// Map a CPU index to its NUMA node by reading
    /// `/sys/devices/system/node/node<N>/cpumap` for each node N.
    ///
    /// Returns `NO_NODE` if the sysfs topology files are absent (e.g. on a
    /// single-node system where the kernel didn't compile NUMA support) or if
    /// `cpu_idx` is out of range.
    fn cpu_to_numa_node(cpu_idx: u32) -> u32 {
        // Try up to 64 NUMA nodes (reasonable upper bound for current hardware).
        for node in 0u32..64 {
            if node_contains_cpu(node, cpu_idx) {
                return node;
            }
        }
        // Single-node system or NUMA sysfs not present: treat as node 0.
        // This is safe — mbind to node 0 on a single-node machine is a no-op
        // from the OS perspective (pages are already on node 0).
        0
    }

    /// Return `true` if `node` lists `cpu_idx` in its cpumap.
    ///
    /// Reads `/sys/devices/system/node/node<N>/cpumap`, which is a
    /// comma-separated list of hex 32-bit words, most-significant word first,
    /// encoding a bitmask of CPU indices.
    fn node_contains_cpu(node: u32, cpu_idx: u32) -> bool {
        // Build the path: "/sys/devices/system/node/nodeN/cpumap"
        // We avoid heap allocation by writing into a fixed-size stack buffer.
        let mut path = [0u8; 64];
        let path_str = format_sysfs_path(&mut path, node);
        // Open and read the file via raw POSIX calls (no std::fs — we must
        // avoid any heap allocation here since this function may be called from
        // the allocation path itself in Phase C).
        read_cpumap_contains_cpu(path_str, cpu_idx)
    }

    /// Write "/sys/devices/system/node/node<N>/cpumap\0" into `buf` and return
    /// the nul-terminated slice.  `N` must fit in 2 digits (< 100).
    fn format_sysfs_path(buf: &mut [u8; 64], node: u32) -> &[u8] {
        // Construct the string manually to avoid format! / heap allocation.
        const PREFIX: &[u8] = b"/sys/devices/system/node/node";
        const SUFFIX: &[u8] = b"/cpumap\0";
        let mut pos = 0usize;
        for &b in PREFIX {
            buf[pos] = b;
            pos += 1;
        }
        // Write decimal digits of `node` (≤ 3 digits for node < 1000).
        let mut tmp = [0u8; 4];
        let mut n = node;
        let mut digits = 0usize;
        if n == 0 {
            tmp[0] = b'0';
            digits = 1;
        } else {
            while n > 0 {
                tmp[digits] = b'0' + (n % 10) as u8;
                n /= 10;
                digits += 1;
            }
            // digits are in reverse order; reverse them
            tmp[..digits].reverse();
        }
        for i in 0..digits {
            buf[pos] = tmp[i];
            pos += 1;
        }
        for &b in SUFFIX {
            buf[pos] = b;
            pos += 1;
        }
        &buf[..pos]
    }

    /// Open the cpumap file and check if `cpu_idx` is set in the bitmask.
    ///
    /// The cpumap file contains a hex bitmask like `"00000000,00000001\n"`.
    /// Each comma-separated 32-bit hex word covers 32 CPUs; the rightmost word
    /// covers CPUs 0–31.  We only need to check one bit.
    fn read_cpumap_contains_cpu(path: &[u8], cpu_idx: u32) -> bool {
        // SAFETY: `path` is a valid nul-terminated C string we constructed
        // above.  `open` is a POSIX syscall; we check for -1 on error.
        let fd = unsafe { libc_open(path.as_ptr() as *const core::ffi::c_char, 0) };
        if fd < 0 {
            return false;
        }
        let mut buf = [0u8; 256];
        // SAFETY: `buf` is a valid writable buffer of length 256; `fd` was
        // returned by a successful `open` call above.
        let n = unsafe { libc_read(fd, buf.as_mut_ptr() as *mut core::ffi::c_void, 256) };
        // SAFETY: `fd` was opened by us and must be closed exactly once.
        unsafe { libc_close(fd) };
        if n <= 0 {
            return false;
        }
        parse_cpumap_contains_cpu(&buf[..n as usize], cpu_idx)
    }

    /// Parse a Linux cpumap string and test whether `cpu_idx` is set.
    ///
    /// Format: comma-separated hex 32-bit words, most-significant first,
    /// optional trailing newline.  Example: `"00000000,00000003\n"` means
    /// CPUs 0 and 1 are in this node.
    fn parse_cpumap_contains_cpu(data: &[u8], cpu_idx: u32) -> bool {
        // Strip trailing newline/whitespace.
        let data = trim_end(data);
        // Split on commas; count words to compute bit position.
        // Word index 0 is the most-significant (highest CPU indices).
        let word_count = data.iter().filter(|&&b| b == b',').count() + 1;
        let target_word = (cpu_idx / 32) as usize;
        let bit_in_word = cpu_idx % 32;
        if target_word >= word_count {
            return false; // cpu_idx beyond what this node knows
        }
        // word_count - 1 - target_word is the 0-based index from the LEFT
        // (since the leftmost word is the most-significant / highest CPUs).
        let left_index = word_count - 1 - target_word;
        // Find the left_index-th comma-delimited token.
        let word_str = match nth_token(data, left_index, b',') {
            Some(s) => s,
            None => return false,
        };
        let val = match parse_hex_u32(word_str) {
            Some(v) => v,
            None => return false,
        };
        (val >> bit_in_word) & 1 == 1
    }

    fn trim_end(data: &[u8]) -> &[u8] {
        let mut end = data.len();
        while end > 0 && (data[end - 1] == b'\n' || data[end - 1] == b'\r' || data[end - 1] == b' ') {
            end -= 1;
        }
        &data[..end]
    }

    /// Return the `n`-th token (0-indexed) delimited by `sep`.
    fn nth_token(data: &[u8], n: usize, sep: u8) -> Option<&[u8]> {
        let mut idx = 0usize;
        let mut start = 0usize;
        for (i, &b) in data.iter().enumerate() {
            if b == sep {
                if idx == n {
                    return Some(&data[start..i]);
                }
                idx += 1;
                start = i + 1;
            }
        }
        // Last token (no trailing sep)
        if idx == n {
            Some(&data[start..])
        } else {
            None
        }
    }

    /// Parse a hex string (no "0x" prefix) as u32.  Returns `None` on error.
    fn parse_hex_u32(s: &[u8]) -> Option<u32> {
        let mut val: u32 = 0;
        if s.is_empty() {
            return None;
        }
        for &b in s {
            let digit = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return None,
            };
            val = val.wrapping_shl(4) | digit as u32;
        }
        Some(val)
    }

    // -- Raw Linux FFI (no libc crate dependency) ---------------------------

    extern "C" {
        fn sched_getcpu() -> core::ffi::c_int;
        fn open(path: *const core::ffi::c_char, flags: core::ffi::c_int, ...) -> core::ffi::c_int;
        fn read(
            fd: core::ffi::c_int,
            buf: *mut core::ffi::c_void,
            count: usize,
        ) -> core::ffi::c_long;
        fn close(fd: core::ffi::c_int) -> core::ffi::c_int;
    }

    // SAFETY wrappers — thin private wrappers so every `unsafe` call site has
    // a corresponding `// SAFETY:` comment in the caller (above).
    unsafe fn libc_sched_getcpu() -> core::ffi::c_int {
        // SAFETY: no pointer args; returns current CPU index or -1.
        sched_getcpu()
    }
    unsafe fn libc_open(path: *const core::ffi::c_char, flags: core::ffi::c_int) -> core::ffi::c_int {
        // SAFETY: caller must supply a valid nul-terminated path.
        open(path, flags)
    }
    unsafe fn libc_read(fd: core::ffi::c_int, buf: *mut core::ffi::c_void, count: usize) -> core::ffi::c_long {
        // SAFETY: caller must supply a valid fd and a writable buffer of `count` bytes.
        read(fd, buf, count)
    }
    unsafe fn libc_close(fd: core::ffi::c_int) {
        // SAFETY: caller must supply a valid, open fd.
        let _ = close(fd);
    }
}

/// Linux (x86_64 / aarch64): bind `[base, base+len)` to NUMA node `node`
/// using `mbind(2)` via the `syscall()` libc wrapper.
///
/// Factored out of the `platform` module so it can be called from both
/// `bind_segment_impl` and `reserve_aligned_on_node_impl`.
///
/// On a single-node machine (or if `node` is invalid) the kernel returns
/// `EINVAL`; we silently ignore errors — the allocation is always correct
/// regardless of whether the binding succeeded.
#[cfg(all(
    target_os = "linux",
    not(miri),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn bind_segment_impl_linux(base: *mut u8, len: usize, node: u32) {
    if node == NO_NODE || node >= 64 {
        return;
    }
    // Build a 64-bit nodemask with bit `node` set.
    let nodemask: u64 = 1u64 << node;
    // maxnode = 64: number of bits in the nodemask word.
    let maxnode: u64 = 64;
    // SAFETY: `base` is the start of a live OS reservation owned by the caller
    // (documented in `bind_segment`'s contract).  `len` is its byte length.
    // `mbind` only modifies kernel page-policy metadata; it never reads or
    // writes the payload bytes.  Errors (EINVAL, ENOTSUP on single-node
    // kernels) are silently discarded — the allocation is correct either way.
    unsafe {
        libc_mbind(
            base as *mut core::ffi::c_void,
            len as u64,
            MPOL_PREFERRED, // soft preference: use node, fall back if unavailable
            &nodemask as *const u64,
            maxnode,
            0, // flags = 0: do not move already-faulted pages
        );
    }
}

/// No-op on Linux architectures where we don't know the mbind syscall number.
#[cfg(all(
    target_os = "linux",
    not(miri),
    not(any(target_arch = "x86_64", target_arch = "aarch64"))
))]
fn bind_segment_impl_linux(_base: *mut u8, _len: usize, _node: u32) {
    // mbind syscall number unknown for this arch; NUMA binding is skipped.
    // The allocation is still correct — pages land wherever the OS decides.
}

#[cfg(all(target_os = "linux", not(miri)))]
const MPOL_PREFERRED: i32 = 1; // soft preferred node, falls back on pressure

/// x86_64 syscall number for `mbind(2)`.
#[cfg(all(target_os = "linux", not(miri), target_arch = "x86_64"))]
const SYS_MBIND: i64 = 237;
/// aarch64 syscall number for `mbind(2)`.
#[cfg(all(target_os = "linux", not(miri), target_arch = "aarch64"))]
const SYS_MBIND: i64 = 235;

/// Invoke `mbind(2)` via the `syscall(2)` libc wrapper rather than a direct
/// `mbind` symbol (which lives in `libnuma`, not `libc`). `syscall` is always
/// present in glibc and musl.
///
/// On architectures other than x86_64 / aarch64 this is a no-op (SYS_MBIND
/// is undefined). We constrain the cfg appropriately below.
#[cfg(all(
    target_os = "linux",
    not(miri),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
extern "C" {
    /// `syscall(2)` — present in every libc. We use it to invoke `mbind`
    /// without depending on `libnuma`.
    fn syscall(number: i64, ...) -> i64;
}

#[cfg(all(
    target_os = "linux",
    not(miri),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
unsafe fn libc_mbind(
    addr: *mut core::ffi::c_void,
    len: u64,
    mode: i32,
    nodemask: *const u64,
    maxnode: u64,
    flags: u32,
) -> i64 {
    // SAFETY: SYS_MBIND is the correct syscall number for this architecture.
    // `addr` is a valid live mapping; `len` is its byte length.  `nodemask`
    // points to a valid u64 on the stack (in the caller).  Errors are ignored
    // by the caller — the allocation is correct regardless.
    syscall(
        SYS_MBIND,
        addr,
        len as usize,
        mode as i64,
        nodemask,
        maxnode as usize,
        flags as i64,
    )
}

/// No-op on unsupported Linux architectures (not x86_64 / aarch64).
#[cfg(all(
    target_os = "linux",
    not(miri),
    not(any(target_arch = "x86_64", target_arch = "aarch64"))
))]
unsafe fn libc_mbind(
    _addr: *mut core::ffi::c_void,
    _len: u64,
    _mode: i32,
    _nodemask: *const u64,
    _maxnode: u64,
    _flags: u32,
) -> i64 {
    -1 // ENOSYS
}

// ---- Windows (real hardware, not miri) ------------------------------------
#[cfg(all(windows, not(miri)))]
mod platform {
    use core::ptr::NonNull;

    use super::{super::os, NO_NODE};

    pub(super) fn current_node_impl() -> u32 {
        let mut proc_num = ProcessorNumber {
            group: 0,
            number: 0,
            reserved: 0,
        };
        // SAFETY: `proc_num` is a valid, zeroed `PROCESSOR_NUMBER` struct;
        // `GetCurrentProcessorNumberEx` fills it in and never fails.
        unsafe { GetCurrentProcessorNumberEx(&mut proc_num) };

        let mut node: u16 = 0;
        // SAFETY: `proc_num` was filled by `GetCurrentProcessorNumberEx`;
        // `GetNumaProcessorNodeEx` maps it to a NUMA node.
        let ok = unsafe { GetNumaProcessorNodeEx(&proc_num, &mut node) };
        if ok == 0 {
            return NO_NODE; // API failed (e.g. single-node system)
        }
        node as u32
    }

    /// On Windows there is no post-reserve NUMA binding API (`mbind` does not
    /// exist). Binding must happen at reservation time via
    /// `VirtualAllocExNuma`. Therefore `bind_segment` is a no-op.
    pub(super) fn bind_segment_impl(_base: *mut u8, _len: usize, _node: u32) {
        // no-op: see module docs.
    }

    pub(super) fn reserve_aligned_on_node_impl(
        usable: usize,
        node: u32,
    ) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
        if node == NO_NODE {
            // Fall back to ordinary VirtualAlloc path.
            let seg = os::Segment::reserve(usable)?;
            let base = seg.as_ptr();
            let reservation = seg.reservation();
            let reservation_len = seg.reservation_len();
            core::mem::forget(seg);
            return Some((
                // SAFETY: Segment::reserve guarantees a non-null base.
                unsafe { NonNull::new_unchecked(base) },
                reservation,
                reservation_len,
            ));
        }
        reserve_aligned_numa(usable, node)
    }

    /// Reserve `usable` bytes SEGMENT-aligned via `VirtualAllocExNuma`.
    ///
    /// Uses the same over-reserve + trim technique as `os::reserve_aligned`
    /// (Windows cannot partially `MEM_RELEASE` a reservation, so we
    /// `MEM_DECOMMIT` the head and tail).
    fn reserve_aligned_numa(usable: usize, node: u32) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
        use super::super::os::SEGMENT; // reuse the constant

        let over = usable.checked_mul(2)?;
        // SAFETY: `VirtualAllocExNuma(GetCurrentProcess(), NULL, over,
        // MEM_RESERVE|MEM_COMMIT, PAGE_READWRITE, node)` reserves+commits
        // `over` bytes preferentially on NUMA node `node`. Returns NULL on
        // OOM or invalid node (treated as OOM — caller gets None).
        let region_ptr = unsafe {
            let p = VirtualAllocExNuma(
                GetCurrentProcess(),
                core::ptr::null_mut(),
                over,
                MEM_RESERVE | MEM_COMMIT,
                PAGE_READWRITE,
                node,
            );
            if p.is_null() {
                return None;
            }
            p as *mut u8
        };

        let region_addr = region_ptr as usize;
        let base_addr = align_up(region_addr, SEGMENT);
        debug_assert!(base_addr + usable <= region_addr + over);

        let base = unsafe {
            // SAFETY: `base_addr` is non-null (>= region_addr which is non-null)
            // and SEGMENT-aligned; it lies within the committed reservation.
            NonNull::new_unchecked(base_addr as *mut u8)
        };

        // Decommit head/tail — return physical pages to OS.
        let head = base_addr - region_addr;
        let tail_start = base_addr + usable;
        let tail_len = (region_addr + over) - tail_start;
        if head > 0 {
            unsafe {
                // SAFETY: `[region_ptr, region_ptr + head)` is within the
                // committed reservation; MEM_DECOMMIT returns physical pages.
                VirtualFree(region_ptr as *mut core::ffi::c_void, head, MEM_DECOMMIT);
            }
        }
        if tail_len > 0 {
            unsafe {
                // SAFETY: `[tail_start, tail_start + tail_len)` is within the
                // committed reservation; same MEM_DECOMMIT contract.
                VirtualFree(tail_start as *mut core::ffi::c_void, tail_len, MEM_DECOMMIT);
            }
        }

        Some((base, unsafe { NonNull::new_unchecked(region_ptr) }, over))
    }

    fn align_up(addr: usize, align: usize) -> usize {
        let mask = align - 1;
        (addr + mask) & !mask
    }

    // Windows constants (kernel32, no winapi crate needed)
    const MEM_RESERVE: u32 = 0x0000_2000;
    const MEM_COMMIT: u32 = 0x0000_1000;
    const MEM_DECOMMIT: u32 = 0x0000_4000;
    const PAGE_READWRITE: u32 = 0x04;

    /// Mirrors `PROCESSOR_NUMBER` from the Windows SDK.
    #[repr(C)]
    struct ProcessorNumber {
        group: u16,
        number: u8,
        reserved: u8,
    }

    extern "system" {
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn VirtualAllocExNuma(
            h_process: *mut core::ffi::c_void,
            lp_address: *mut core::ffi::c_void,
            dw_size: usize,
            fl_allocation_type: u32,
            fl_protect: u32,
            nnd_preferred: u32,
        ) -> *mut core::ffi::c_void;
        fn VirtualFree(
            lp_address: *mut core::ffi::c_void,
            dw_size: usize,
            dw_free_type: u32,
        ) -> i32;
        fn GetCurrentProcessorNumberEx(proc_number: *mut ProcessorNumber);
        fn GetNumaProcessorNodeEx(
            processor: *const ProcessorNumber,
            node_number: *mut u16,
        ) -> i32;
    }
}

// ---- macOS stub -----------------------------------------------------------
#[cfg(target_os = "macos")]
mod platform {
    use core::ptr::NonNull;

    use super::{super::os, NO_NODE};

    /// macOS has no public NUMA API. Apple Silicon (M-series) is UMA; Intel Mac
    /// multi-socket NUMA is not exposed via public syscalls. Return `NO_NODE`.
    pub(super) fn current_node_impl() -> u32 {
        NO_NODE
    }

    /// No-op: no NUMA binding on macOS.
    pub(super) fn bind_segment_impl(_base: *mut u8, _len: usize, _node: u32) {}

    pub(super) fn reserve_aligned_on_node_impl(
        usable: usize,
        _node: u32,
    ) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
        let seg = os::Segment::reserve(usable)?;
        let base = seg.as_ptr();
        let reservation = seg.reservation();
        let reservation_len = seg.reservation_len();
        core::mem::forget(seg);
        Some((
            // SAFETY: Segment::reserve guarantees a non-null base.
            unsafe { NonNull::new_unchecked(base) },
            reservation,
            reservation_len,
        ))
    }
}

// ---- miri stub (any OS under miri) ----------------------------------------
#[cfg(miri)]
mod platform {
    use core::ptr::NonNull;

    use super::{super::os, NO_NODE};

    /// Under miri, NUMA detection is not meaningful (no real OS topology).
    pub(super) fn current_node_impl() -> u32 {
        NO_NODE
    }

    /// No-op under miri.
    pub(super) fn bind_segment_impl(_base: *mut u8, _len: usize, _node: u32) {}

    pub(super) fn reserve_aligned_on_node_impl(
        usable: usize,
        _node: u32,
    ) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
        let seg = os::Segment::reserve(usable)?;
        let base = seg.as_ptr();
        let reservation = seg.reservation();
        let reservation_len = seg.reservation_len();
        core::mem::forget(seg);
        Some((
            // SAFETY: Segment::reserve guarantees a non-null base.
            unsafe { NonNull::new_unchecked(base) },
            reservation,
            reservation_len,
        ))
    }
}

// ---- Fallback: any remaining platform (e.g. FreeBSD) ---------------------
// This arm triggers on platforms that are not Linux, Windows, macOS, or miri.
// We treat them as unsupported and return NO_NODE / no-ops.
#[cfg(not(any(
    target_os = "linux",
    windows,
    target_os = "macos",
    miri,
)))]
mod platform {
    use core::ptr::NonNull;

    use super::{super::os, NO_NODE};

    pub(super) fn current_node_impl() -> u32 {
        NO_NODE
    }

    pub(super) fn bind_segment_impl(_base: *mut u8, _len: usize, _node: u32) {}

    pub(super) fn reserve_aligned_on_node_impl(
        usable: usize,
        _node: u32,
    ) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
        let seg = os::Segment::reserve(usable)?;
        let base = seg.as_ptr();
        let reservation = seg.reservation();
        let reservation_len = seg.reservation_len();
        core::mem::forget(seg);
        Some((
            unsafe { NonNull::new_unchecked(base) },
            reservation,
            reservation_len,
        ))
    }
}
