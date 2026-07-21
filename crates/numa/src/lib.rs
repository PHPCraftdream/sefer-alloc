//! `numa-shim` — dependency-free NUMA detection and binding.
//!
//! **Key selling point:** zero C library dependencies.
//! - Linux: `mbind(2)` via raw `syscall(2)` (no libnuma, no hwloc).
//! - Linux node detection: reads `/sys/devices/system/node/nodeN/cpumap` directly
//!   via `open`/`read`/`close` from the C runtime (always present in glibc/musl).
//! - Windows: `VirtualAllocExNuma` for NUMA-preferred reservations;
//!   `GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx` for detection.
//! - macOS / miri: no-op (no public NUMA API on macOS; miri has no real OS topology).
//!
//! This is rare in the Rust ecosystem — typical NUMA crates bind to `libnuma` or
//! `hwloc`, pulling in heavy C dependencies. `numa-shim` has **zero non-system
//! dependencies** in its default configuration.
//!
//! ## Usage
//!
//! ```text
//! use numa_shim::{current_node, NO_NODE};
//!
//! match current_node() {
//!     Some(node) => println!("Running on NUMA node {node}"),
//!     None       => println!("NUMA unavailable or single-node host"),
//! }
//! ```
//!
//! Runnable form: `tests/smoke.rs`.
//!
//! ## Feature flags
//!
//! | Flag | Effect |
//! |------|--------|
//! | `vmem-integration` | Enables [`reserve_on_node`], which uses [`aligned-vmem`] for the reservation step. Windows path uses `VirtualAllocExNuma`; Linux reserves then calls `mbind`. |
//!
//! ## Platform matrix
//!
//! | Platform | [`current_node`] | [`bind_range`] | [`reserve_on_node`] (feature) |
//! |----------|-----------------|----------------|-------------------------------|
//! | Linux x86_64/aarch64 (non-miri) | sched_getcpu + sysfs cpumap | `mbind(2)` via syscall | mmap then mbind |
//! | Linux other arch (non-miri) | sched_getcpu + sysfs cpumap | no-op | mmap (no mbind) |
//! | Windows (non-miri) | `GetCurrentProcessorNumberEx` | no-op (use `reserve_on_node`) | `VirtualAllocExNuma` (direct, via `Reservation::from_raw_parts`) |
//! | macOS | `None` | no-op | `reserve_aligned` (no binding) |
//! | miri | `None` | no-op | `reserve_aligned` (no binding) |
//! | other | `None` | no-op | `reserve_aligned` (no binding) |

// This crate intentionally contains unsafe OS FFI code.
// The public API is safe; all unsafe is confined to platform modules and
// clearly documented with // SAFETY: proof comments.
#![allow(unsafe_code)]
#![deny(missing_docs)]

/// Sentinel value meaning "no NUMA node / feature disabled / unsupported
/// platform". This constant is useful when interfacing with APIs that return
/// a raw `u32` node index and need a "not available" sentinel.
///
/// [`current_node`] returns `None` instead of this sentinel; `NO_NODE` is
/// provided for interop with code that uses the sentinel pattern.
pub const NO_NODE: u32 = u32::MAX;

/// Test-only mock state replacing platform NUMA syscalls.  Records every
/// invocation into a thread-local buffer so unit tests can assert the
/// wrapping logic is correct on any target (including macOS and miri,
/// where real NUMA syscalls are absent).
///
/// Enabled by feature `mock`.  When enabled, the public NUMA functions
/// dispatch into this module instead of the platform implementations.
#[cfg(feature = "mock")]
pub mod mock {
    use core::cell::RefCell;

    /// One recorded invocation of a public NUMA function.
    #[non_exhaustive]
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum MockCall {
        /// `current_node()` was called; the inner value is what was returned.
        CurrentNode(u32),
        /// `bind_range(base, len, node)` was called (past the short-circuit).
        BindRange {
            /// Base address passed to `bind_range`, as `usize`.
            base: usize,
            /// Length in bytes passed to `bind_range`.
            len: usize,
            /// NUMA node id passed to `bind_range`.
            node: u32,
        },
        /// `reserve_on_node(size, align, node)` was called.
        ReserveOnNode {
            /// Requested reservation size in bytes.
            size: usize,
            /// Required alignment in bytes.
            align: usize,
            /// NUMA node id passed to `reserve_on_node`.
            node: u32,
        },
    }

    std::thread_local! {
        /// Calls recorded since the last `drain()`.
        pub static CALLS: RefCell<Vec<MockCall>> = const { RefCell::new(Vec::new()) };
        /// Value returned by `current_node()` under the mock.  Default 0.
        pub static CURRENT_NODE_SLOT: RefCell<u32> = const { RefCell::new(0) };
    }

    /// Drain every recorded call since the last drain (or test start).
    pub fn drain() -> Vec<MockCall> {
        CALLS.with(|c| c.borrow_mut().drain(..).collect())
    }

    /// Set the value the next `current_node()` call will return.
    pub fn set_current_node(node: u32) {
        CURRENT_NODE_SLOT.with(|c| *c.borrow_mut() = node);
    }

    /// Internal: read the scripted current_node value.
    pub(crate) fn current_node_slot() -> u32 {
        CURRENT_NODE_SLOT.with(|c| *c.borrow())
    }

    /// Internal: record a call.
    ///
    /// R11-5: reentrancy-safe. The `Vec::push` inside the borrow guard
    /// allocates via the global allocator; if the global allocator IS
    /// sefer-alloc under `numa-aware-mock` (which `--all-features` enables),
    /// that allocation re-enters `current_node()` → `record()`, which would
    /// deadlock on a plain `borrow_mut()` (already borrowed). `try_with` +
    /// `try_borrow_mut` silently drops the recording on re-entry — the
    /// RETURNED value (from `current_node_slot`) is unaffected; only the
    /// call-log entry for the re-entrant call is lost, which is acceptable
    /// because tests that inspect the call log never run under a
    /// sefer-alloc-as-global scenario.
    pub(crate) fn record(call: MockCall) {
        let _ = CALLS.try_with(|c| {
            if let Ok(mut b) = c.try_borrow_mut() {
                b.push(call);
            }
        });
    }
}

/// Return the NUMA node id of the calling thread, or `None` if not
/// determinable.
///
/// Returns `None` when:
/// - The platform does not provide a NUMA API (macOS, miri, unsupported OS).
/// - The OS API returns an error (e.g. single-NUMA host with disabled NUMA
///   support in the kernel).
/// - The CPU index cannot be mapped to a NUMA node via sysfs.
///
/// On a single-node Linux system where sysfs NUMA files are absent, this
/// function returns `Some(0)` (all CPUs are on node 0).
#[must_use]
pub fn current_node() -> Option<u32> {
    #[cfg(feature = "mock")]
    {
        let n = mock::current_node_slot();
        mock::record(mock::MockCall::CurrentNode(n));
        Some(n)
    }
    #[cfg(not(feature = "mock"))]
    {
        let raw = platform::current_node_impl();
        if raw == NO_NODE {
            None
        } else {
            Some(raw)
        }
    }
}

/// Bind the virtual-memory range `[base, base + len)` to NUMA node `node`.
///
/// On Linux (x86_64 and aarch64): issues `mbind(2)` via the `syscall(2)`
/// libc wrapper with `MPOL_PREFERRED` (soft preference — the kernel falls
/// back to any node on memory pressure). This steers physical page allocation
/// to `node` at the first page-fault after the call.
///
/// On Windows: no-op. Windows has no post-reserve NUMA binding API; use
/// [`reserve_on_node`] (with the `vmem-integration` feature) to bind at
/// reservation time via `VirtualAllocExNuma`.
///
/// On macOS / miri / other: no-op.
///
/// The function silently ignores OS errors (e.g. `EINVAL` on a single-node
/// kernel): the allocation is always valid regardless of whether binding
/// succeeded.
///
/// # Safety
///
/// `[base, base + len)` must be a valid OS reservation owned exclusively by
/// the caller for the duration of the call. The function never reads or writes
/// payload bytes — it only passes the range to `mbind(2)` which sets kernel
/// page-policy metadata.
pub unsafe fn bind_range(base: *mut u8, len: usize, node: u32) {
    if node == NO_NODE || len == 0 {
        return;
    }
    #[cfg(feature = "mock")]
    {
        mock::record(mock::MockCall::BindRange {
            base: base as usize,
            len,
            node,
        });
    }
    #[cfg(not(feature = "mock"))]
    {
        // SAFETY: caller guarantees [base, base+len) is a valid OS reservation.
        platform::bind_range_impl(base, len, node);
    }
}

/// Reserve `size` bytes of anonymous virtual memory with a NUMA preference for
/// `node`, aligned to `align`.
///
/// Requires the `vmem-integration` feature.
///
/// - Linux: reserves via [`aligned_vmem::reserve_aligned`] then calls
///   [`bind_range`] before the first page-fault.
/// - Windows: calls `VirtualAllocExNuma` directly (the only way to get NUMA
///   binding on Windows is at reservation time).
/// - macOS / miri / other: falls back to [`aligned_vmem::reserve_aligned`]
///   without NUMA binding.
///
/// Returns `None` on OOM or if `size`/`align` violate [`aligned_vmem`]
/// contracts (size non-zero, align a power-of-two `>=` page size, size a
/// multiple of page size).
///
/// When `node` is `NO_NODE` (or [`None`] from [`current_node`]) the call
/// behaves like plain [`aligned_vmem::reserve_aligned`].
#[cfg(feature = "vmem-integration")]
#[must_use]
pub fn reserve_on_node(size: usize, align: usize, node: u32) -> Option<aligned_vmem::Reservation> {
    #[cfg(feature = "mock")]
    {
        mock::record(mock::MockCall::ReserveOnNode { size, align, node });
        // Still chain to aligned_vmem so the test can verify the Reservation works.
        let r = aligned_vmem::reserve_aligned(size, align)?;
        if node != NO_NODE {
            let base = r.as_ptr();
            let len = r.len();
            mock::record(mock::MockCall::BindRange {
                base: base as usize,
                len,
                node,
            });
        }
        Some(r)
    }
    #[cfg(not(feature = "mock"))]
    {
        platform::reserve_on_node_impl(size, align, node)
    }
}

// ---------------------------------------------------------------------------
// Per-platform implementations
// ---------------------------------------------------------------------------

// ---- Linux (real hardware, not miri) --------------------------------------
#[cfg(all(target_os = "linux", not(miri)))]
// Under `mock`, the public API dispatches to the recording mock instead of
// these platform impls, so every symbol here is (expectedly) unused. `mock`
// exists precisely to bypass the real syscalls; the platform code still must
// compile. Suppress dead-code only in that combination.
#[cfg_attr(feature = "mock", allow(dead_code))]
mod platform {
    use super::{bind_range_impl_linux, NO_NODE};

    pub(super) fn current_node_impl() -> u32 {
        // SAFETY: `sched_getcpu` is a POSIX function that returns the CPU index
        // of the calling thread, or -1 on error. No pointer arguments.
        let cpu = unsafe { libc_sched_getcpu() };
        if cpu < 0 {
            return NO_NODE;
        }
        cpu_to_numa_node(cpu as u32)
    }

    pub(super) fn bind_range_impl(base: *mut u8, len: usize, node: u32) {
        // SAFETY: caller of bind_range is `unsafe fn` and guarantees
        // `[base, base+len)` is a live OS reservation owned by it. mbind only
        // sets kernel page-policy metadata, never reads/writes payload bytes.
        unsafe { bind_range_impl_linux(base, len, node) };
    }

    #[cfg(feature = "vmem-integration")]
    pub(super) fn reserve_on_node_impl(
        size: usize,
        align: usize,
        node: u32,
    ) -> Option<aligned_vmem::Reservation> {
        // Reserve via aligned-vmem, then bind with mbind before first page access.
        let r = aligned_vmem::reserve_aligned(size, align)?;
        if node != NO_NODE {
            let base = r.as_ptr();
            let len = r.len();
            // SAFETY: `r` is a valid live OS reservation we own; `base` and
            // `len` come from the freshly-created Reservation. mbind only sets
            // kernel page-policy metadata, never reads/writes payload bytes.
            unsafe { bind_range_impl_linux(base, len, node) };
        }
        Some(r)
    }

    /// Map a CPU index to its NUMA node by reading
    /// `/sys/devices/system/node/nodeN/cpumap` for each node N.
    ///
    /// Returns `0` when sysfs NUMA topology files are absent (single-node
    /// system where the kernel didn't compile NUMA support).
    fn cpu_to_numa_node(cpu_idx: u32) -> u32 {
        // Try up to 64 NUMA nodes (reasonable upper bound for current hardware).
        for node in 0u32..64 {
            if node_contains_cpu(node, cpu_idx) {
                return node;
            }
        }
        // Single-node system or NUMA sysfs not present: treat as node 0.
        // mbind to node 0 on a single-node machine is a no-op from the OS
        // perspective (pages are already on node 0).
        0
    }

    /// Return `true` if `node` lists `cpu_idx` in its cpumap.
    ///
    /// Reads `/sys/devices/system/node/nodeN/cpumap`, a comma-separated list
    /// of hex 32-bit words (most-significant word first) encoding a CPU bitmask.
    fn node_contains_cpu(node: u32, cpu_idx: u32) -> bool {
        let mut path = [0u8; 64];
        let path_str = format_sysfs_path(&mut path, node);
        read_cpumap_contains_cpu(path_str, cpu_idx)
    }

    /// Write `/sys/devices/system/node/nodeN/cpumap\0` into `buf` and return
    /// the nul-terminated slice. Avoids heap allocation.
    fn format_sysfs_path(buf: &mut [u8; 64], node: u32) -> &[u8] {
        const PREFIX: &[u8] = b"/sys/devices/system/node/node";
        const SUFFIX: &[u8] = b"/cpumap\0";
        let mut pos = 0usize;
        for &b in PREFIX {
            buf[pos] = b;
            pos += 1;
        }
        // Write decimal digits of `node` (up to 3 digits for node < 1000).
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
            // Written in reverse; fix ordering.
            tmp[..digits].reverse();
        }
        for &d in tmp.iter().take(digits) {
            buf[pos] = d;
            pos += 1;
        }
        for &b in SUFFIX {
            buf[pos] = b;
            pos += 1;
        }
        &buf[..pos]
    }

    /// Open the cpumap file at `path` and check if `cpu_idx` bit is set.
    ///
    /// The cpumap file format: `"00000000,00000001\n"` — comma-separated
    /// hex 32-bit words, most-significant word first; each word covers 32 CPUs.
    fn read_cpumap_contains_cpu(path: &[u8], cpu_idx: u32) -> bool {
        // SAFETY: `path` is a valid nul-terminated C string constructed above.
        // `open` is a POSIX syscall; we check for -1 on error.
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

    /// Parse a Linux cpumap bitmask string and test whether `cpu_idx` is set.
    ///
    /// Format: comma-separated hex 32-bit words, most-significant first,
    /// optional trailing newline. Example: `"00000000,00000003\n"` means
    /// CPUs 0 and 1 are in this node.
    fn parse_cpumap_contains_cpu(data: &[u8], cpu_idx: u32) -> bool {
        let data = trim_end(data);
        let word_count = data.iter().filter(|&&b| b == b',').count() + 1;
        let target_word = (cpu_idx / 32) as usize;
        let bit_in_word = cpu_idx % 32;
        if target_word >= word_count {
            return false;
        }
        // The leftmost word covers the highest CPU indices.
        let left_index = word_count - 1 - target_word;
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
        while end > 0 && (data[end - 1] == b'\n' || data[end - 1] == b'\r' || data[end - 1] == b' ')
        {
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
        // Last token (no trailing separator).
        if idx == n {
            Some(&data[start..])
        } else {
            None
        }
    }

    /// Parse a hex string (no `0x` prefix) as `u32`. Returns `None` on error.
    fn parse_hex_u32(s: &[u8]) -> Option<u32> {
        if s.is_empty() {
            return None;
        }
        let mut val: u32 = 0;
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

    // -- Raw Linux FFI (no libc crate dependency) ----------------------------

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

    // Thin private wrappers so every call site has its own // SAFETY: comment.
    unsafe fn libc_sched_getcpu() -> core::ffi::c_int {
        // SAFETY: no pointer args; returns current CPU index or -1.
        sched_getcpu()
    }
    unsafe fn libc_open(
        path: *const core::ffi::c_char,
        flags: core::ffi::c_int,
    ) -> core::ffi::c_int {
        // SAFETY: caller must supply a valid nul-terminated path.
        open(path, flags)
    }
    unsafe fn libc_read(
        fd: core::ffi::c_int,
        buf: *mut core::ffi::c_void,
        count: usize,
    ) -> core::ffi::c_long {
        // SAFETY: caller must supply a valid fd and a writable buffer of `count` bytes.
        read(fd, buf, count)
    }
    unsafe fn libc_close(fd: core::ffi::c_int) {
        // SAFETY: caller must supply a valid, open fd that is closed exactly once.
        let _ = close(fd);
    }
}

// ---------------------------------------------------------------------------
// Linux mbind: factored out of `platform` so both bind_range_impl and
// reserve_on_node_impl (under vmem-integration) can call it.
// ---------------------------------------------------------------------------

/// Bind `[base, base+len)` to NUMA node `node` via `mbind(2)`.
///
/// Uses `syscall(SYS_MBIND, …)` — avoids a hard dependency on `libnuma`.
/// OS errors (e.g. `EINVAL` on a single-node kernel) are silently discarded.
#[cfg(all(
    target_os = "linux",
    not(miri),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
// Reached only from the platform module, which is itself unused under `mock`.
#[cfg_attr(feature = "mock", allow(dead_code))]
unsafe fn bind_range_impl_linux(base: *mut u8, len: usize, node: u32) {
    if node == NO_NODE || node >= 64 {
        return;
    }
    // 64-bit nodemask with bit `node` set.
    let nodemask: u64 = 1u64 << node;
    let maxnode: u64 = 64;
    // SAFETY: `base` is the start of a live OS reservation (caller's contract).
    // `mbind` only sets kernel page-policy metadata; it never accesses payload
    // bytes. Errors are silently discarded — the allocation is correct regardless.
    libc_mbind(
        base as *mut core::ffi::c_void,
        len as u64,
        MPOL_PREFERRED,
        &nodemask as *const u64,
        maxnode,
        0,
    );
}

/// No-op on Linux architectures without a known `SYS_MBIND` number.
#[cfg(all(
    target_os = "linux",
    not(miri),
    not(any(target_arch = "x86_64", target_arch = "aarch64"))
))]
#[cfg_attr(feature = "mock", allow(dead_code))]
unsafe fn bind_range_impl_linux(_base: *mut u8, _len: usize, _node: u32) {
    // mbind syscall number unknown for this arch; binding is skipped silently.
}

/// `MPOL_PREFERRED`: soft preferred-node policy; kernel falls back on pressure.
#[cfg(all(target_os = "linux", not(miri)))]
#[cfg_attr(feature = "mock", allow(dead_code))]
const MPOL_PREFERRED: i32 = 1;

/// Syscall number for `mbind(2)` on x86_64.
#[cfg(all(target_os = "linux", not(miri), target_arch = "x86_64"))]
#[cfg_attr(feature = "mock", allow(dead_code))]
const SYS_MBIND: i64 = 237;

/// Syscall number for `mbind(2)` on aarch64.
#[cfg(all(target_os = "linux", not(miri), target_arch = "aarch64"))]
#[cfg_attr(feature = "mock", allow(dead_code))]
const SYS_MBIND: i64 = 235;

// `syscall(2)` from glibc/musl — always present, does not require libnuma.
#[cfg(all(
    target_os = "linux",
    not(miri),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
extern "C" {
    fn syscall(number: i64, ...) -> i64;
}

#[cfg(all(
    target_os = "linux",
    not(miri),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[cfg_attr(feature = "mock", allow(dead_code))]
unsafe fn libc_mbind(
    addr: *mut core::ffi::c_void,
    len: u64,
    mode: i32,
    nodemask: *const u64,
    maxnode: u64,
    flags: u32,
) -> i64 {
    // SAFETY: SYS_MBIND is the correct syscall number for this architecture.
    // `addr` is a live mapping; `nodemask` points to a valid stack-allocated u64.
    // Errors are ignored by the caller.
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

// ---------------------------------------------------------------------------
// Windows platform module
// ---------------------------------------------------------------------------
#[cfg(all(windows, not(miri)))]
// Under `mock`, the public API dispatches to the recording mock instead of
// these platform impls, so every symbol here is (expectedly) unused. `mock`
// exists precisely to bypass the real syscalls; the platform code still must
// compile. Suppress dead-code only in that combination.
#[cfg_attr(feature = "mock", allow(dead_code))]
mod platform {
    use super::NO_NODE;

    pub(super) fn current_node_impl() -> u32 {
        let mut proc_num = ProcessorNumber {
            group: 0,
            number: 0,
            reserved: 0,
        };
        // SAFETY: `proc_num` is a valid zeroed `PROCESSOR_NUMBER`; this API
        // fills it in and never fails (documented to always succeed).
        unsafe { GetCurrentProcessorNumberEx(&mut proc_num) };

        let mut node: u16 = 0;
        // SAFETY: `proc_num` was filled by `GetCurrentProcessorNumberEx`;
        // `GetNumaProcessorNodeEx` maps it to a NUMA node (returns 0 on
        // single-node or error, which we remap to NO_NODE).
        let ok = unsafe { GetNumaProcessorNodeEx(&proc_num, &mut node) };
        if ok == 0 {
            return NO_NODE;
        }
        node as u32
    }

    /// On Windows there is no post-reserve NUMA binding API equivalent to
    /// Linux `mbind(2)`. Binding must happen at reservation time via
    /// `VirtualAllocExNuma`. This function is intentionally a no-op.
    pub(super) fn bind_range_impl(_base: *mut u8, _len: usize, _node: u32) {
        // no-op: Windows has no post-mmap NUMA rebind. Use reserve_on_node.
    }

    #[cfg(feature = "vmem-integration")]
    pub(super) fn reserve_on_node_impl(
        size: usize,
        align: usize,
        node: u32,
    ) -> Option<aligned_vmem::Reservation> {
        if node == NO_NODE {
            // No NUMA preference: fall back to ordinary aligned-vmem reserve.
            return aligned_vmem::reserve_aligned(size, align);
        }
        reserve_aligned_numa(size, align, node)
    }

    /// Reserve `size` bytes aligned to `align` with a NUMA preference for `node`
    /// via `VirtualAllocExNuma` directly. This is the **only** way to bind
    /// memory to a NUMA node on Windows — there is no post-reservation
    /// equivalent to Linux `mbind(2)`.
    ///
    /// Strategy (mirrors `aligned-vmem`'s own Windows reservation): over-reserve
    /// `size + align` bytes via `VirtualAllocExNuma`, find the aligned chunk
    /// inside, and adopt the WHOLE reservation into an `aligned_vmem::Reservation`
    /// via [`aligned_vmem::Reservation::from_raw_parts`]. The handle's `Drop` /
    /// release path will `VirtualFree(MEM_RELEASE)` the entire over-reserved
    /// span exactly once.
    ///
    /// Returns `None` on contract violation (`align` not a power of two `>= PAGE`,
    /// `size` zero or not a multiple of `PAGE`) or when the OS refuses the
    /// reservation (OOM / no memory on the requested node).
    #[cfg(feature = "vmem-integration")]
    fn reserve_aligned_numa(
        size: usize,
        align: usize,
        node: u32,
    ) -> Option<aligned_vmem::Reservation> {
        use aligned_vmem::PAGE;
        if size == 0 || !align.is_power_of_two() || align < PAGE || !size.is_multiple_of(PAGE) {
            return None;
        }
        let over = size.checked_add(align)?;

        // SAFETY: `VirtualAllocExNuma(GetCurrentProcess(), NULL, over,
        // MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE, node)` reserves+commits
        // `over` bytes on (preferred) `node`, returning the base or NULL.
        // We treat NULL as OOM and bail.
        let raw = unsafe {
            VirtualAllocExNuma(
                GetCurrentProcess(),
                core::ptr::null_mut(),
                over,
                MEM_RESERVE | MEM_COMMIT,
                PAGE_READWRITE,
                node,
            )
        };
        if raw.is_null() {
            return None;
        }
        let raw_u = raw as usize;
        let base_u = (raw_u + align - 1) & !(align - 1);
        let base = base_u as *mut u8;

        // SAFETY of from_raw_parts:
        // - `base` is non-null, valid for `size` bytes (it's inside the
        //   `over`-byte reservation since `align <= over - size`), aligned
        //   to `align` (by construction above).
        // - `raw` is the start of the OS reservation, non-null.
        // - `over = size + align` is the full reservation length, multiple of PAGE.
        // - `align` was just used to align `base` — same value.
        // - The reservation will be released exactly once when the returned
        //   handle's `Drop` fires (or via `release` after `into_parts`).
        // - The reservation was created with `MEM_RESERVE | MEM_COMMIT` →
        //   `VirtualFree(MEM_RELEASE)` will accept it.
        let r = unsafe {
            aligned_vmem::Reservation::from_raw_parts(base, size, raw as *mut u8, over, align)
        };
        Some(r)
    }

    /// Mirrors `PROCESSOR_NUMBER` from the Windows SDK.
    #[repr(C)]
    struct ProcessorNumber {
        group: u16,
        number: u8,
        reserved: u8,
    }

    extern "system" {
        fn GetCurrentProcessorNumberEx(proc_number: *mut ProcessorNumber);
        fn GetNumaProcessorNodeEx(processor: *const ProcessorNumber, node_number: *mut u16) -> i32;
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
    }

    // `VirtualAllocExNuma` is the load-bearing call: it is the ONLY way to
    // bind a reservation to a NUMA node on Windows (`VirtualAlloc` chooses
    // the node by kernel heuristic; there is no `mbind`-equivalent for
    // post-reservation binding). Declared locally to avoid pulling
    // `windows-sys` / `winapi` just for one syscall.
    #[cfg(feature = "vmem-integration")]
    extern "system" {
        fn VirtualAllocExNuma(
            h_process: *mut core::ffi::c_void,
            lp_address: *mut core::ffi::c_void,
            dw_size: usize,
            fl_allocation_type: u32,
            fl_protect: u32,
            nnd_preferred: u32,
        ) -> *mut core::ffi::c_void;
    }

    #[cfg(feature = "vmem-integration")]
    const MEM_RESERVE: u32 = 0x0000_2000;
    #[cfg(feature = "vmem-integration")]
    const MEM_COMMIT: u32 = 0x0000_1000;
    #[cfg(feature = "vmem-integration")]
    const PAGE_READWRITE: u32 = 0x04;
}

// ---- macOS stub -----------------------------------------------------------
#[cfg(target_os = "macos")]
#[cfg_attr(feature = "mock", allow(dead_code))]
mod platform {
    use super::NO_NODE;

    /// macOS has no public NUMA API. Always returns `NO_NODE`.
    pub(super) fn current_node_impl() -> u32 {
        NO_NODE
    }

    /// No-op: macOS has no NUMA binding API.
    pub(super) fn bind_range_impl(_base: *mut u8, _len: usize, _node: u32) {}

    #[cfg(feature = "vmem-integration")]
    pub(super) fn reserve_on_node_impl(
        size: usize,
        align: usize,
        _node: u32,
    ) -> Option<aligned_vmem::Reservation> {
        // macOS: no NUMA API; plain reserve.
        aligned_vmem::reserve_aligned(size, align)
    }
}

// ---- miri stub (any OS under miri) ----------------------------------------
#[cfg(miri)]
#[cfg_attr(feature = "mock", allow(dead_code))]
mod platform {
    use super::NO_NODE;

    /// Under miri NUMA detection is not meaningful. Always returns `NO_NODE`.
    pub(super) fn current_node_impl() -> u32 {
        NO_NODE
    }

    /// No-op under miri.
    pub(super) fn bind_range_impl(_base: *mut u8, _len: usize, _node: u32) {}

    #[cfg(feature = "vmem-integration")]
    pub(super) fn reserve_on_node_impl(
        size: usize,
        align: usize,
        _node: u32,
    ) -> Option<aligned_vmem::Reservation> {
        aligned_vmem::reserve_aligned(size, align)
    }
}

// ---- Fallback: unsupported platform (e.g. FreeBSD, other Unix) ------------
#[cfg(not(any(target_os = "linux", windows, target_os = "macos", miri,)))]
#[cfg_attr(feature = "mock", allow(dead_code))]
mod platform {
    use super::NO_NODE;

    /// Unsupported platform: always returns `NO_NODE`.
    pub(super) fn current_node_impl() -> u32 {
        NO_NODE
    }

    /// No-op on unsupported platforms.
    pub(super) fn bind_range_impl(_base: *mut u8, _len: usize, _node: u32) {}

    #[cfg(feature = "vmem-integration")]
    pub(super) fn reserve_on_node_impl(
        size: usize,
        align: usize,
        _node: u32,
    ) -> Option<aligned_vmem::Reservation> {
        aligned_vmem::reserve_aligned(size, align)
    }
}
