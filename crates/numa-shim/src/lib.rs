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
//! | `vmem-integration` | Enables [`reserve_on_node`], which uses the `aligned-vmem` crate for the reservation step. Windows path uses `VirtualAllocExNuma`; Linux reserves then calls `mbind`. |
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

/// Re-exported so callers of [`reserve_on_node`] can name the return type as
/// `numa_shim::Reservation` without adding a direct `aligned-vmem`
/// dependency of their own. This re-export makes the intentional semver
/// coupling between the two sibling crates visible in `numa-shim`'s own
/// public API (item 46, `docs/CORRECTNESS_OPEN_ITEMS.md`) rather than
/// leaving it implicit — see [`reserve_on_node`]'s own doc section on the
/// coupling for the full rationale.
#[cfg(feature = "vmem-integration")]
pub use aligned_vmem::Reservation;

/// Test-only mock state replacing platform NUMA syscalls.  Records every
/// invocation into a thread-local buffer so unit tests can assert the
/// wrapping logic is correct on any target (including macOS and miri,
/// where real NUMA syscalls are absent).
///
/// Enabled by feature `mock`.  When enabled, the public NUMA functions
/// dispatch into this module instead of the platform implementations.
///
/// The recording log is capped at [`mock::CALLS_CAP`] entries (task
/// #726/#778) — see [`mock::drain`]'s own doc for what that means for a
/// caller driving more calls than the cap without an intervening drain.
///
/// # Cargo feature-unification hazard (task #726)
///
/// `mock` is a NON-ADDITIVE, backend-REPLACING feature, and Cargo unifies
/// features across a build's WHOLE dependency graph, not per edge. If ANY
/// crate anywhere downstream of your build enables `numa-shim/mock`
/// (including a sibling workspace member's own `[dev-dependencies]`), every
/// OTHER consumer in that SAME build silently gets the mock backend too —
/// with no compile error and no warning. **Enable this feature only in a
/// leaf test/dev target — never in a library's own `[dependencies]`, and
/// never in a shared `[dev-dependencies]` entry reachable from more than one
/// build target in the same workspace.** See the `mock` feature's own doc
/// comment in `Cargo.toml` for the full reasoning and the stronger
/// alternative (a `--cfg` flag instead of a Cargo feature) considered and
/// deferred for this crate's first publish, matching `aligned-vmem`'s own
/// task #715 decision for its identical `mock` feature.
#[cfg(feature = "mock")]
pub mod mock {
    use core::cell::RefCell;

    /// Maximum number of calls `CALLS` retains before `record()` stops
    /// pushing.
    ///
    /// task #726 (rust-intel audit §B14): under the documented
    /// sefer-alloc-as-global `numa-aware-mock` scenario (this module's own
    /// R11-5 note on `record`), every allocation calls `current_node()` →
    /// `record()` → `Vec::push` with nothing ever draining the log in that
    /// scenario — an unbounded insert-only Vec growing linearly with
    /// allocation count per thread. Once `CALLS` holds this many entries,
    /// `record()` stops pushing (oldest entries are kept, matching a
    /// call-log's usual "what happened first" debugging value) rather than
    /// growing forever; direct mock tests that `drain()` promptly never
    /// approach this cap.
    ///
    /// task #778 (round-closing review, F7): made `pub` so a downstream
    /// test driving a large number of mocked calls can assert against this
    /// exact value instead of hardcoding a mirror of it (as
    /// `tests/mock_dispatch.rs`'s own `calls_log_is_capped_not_unbounded`
    /// now does).
    pub const CALLS_CAP: usize = 4096;

    /// One recorded invocation of a public NUMA function.
    #[non_exhaustive]
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum MockCall {
        /// `current_node()` was called; the inner value is what was returned.
        ///
        /// task #778 (round-closing review, F13): unlike [`BindRange`] and
        /// [`ReserveOnNode`] below, this tuple variant deliberately does NOT
        /// carry `#[non_exhaustive]` -- `current_node()`'s signature is
        /// `fn() -> Option<u32>`, a single scalar return with no plausible
        /// second field to grow into (unlike `bind_range`/`reserve_on_node`,
        /// which each take multiple arguments a future API revision could
        /// add to). Marking it would force `tests/mock_dispatch.rs`'s two
        /// `assert_eq!(calls, vec![MockCall::CurrentNode(n)])` equality-
        /// oracle sites into weaker `matches!` form for no real growth path
        /// this shape needs to reserve. This variant's single-field layout
        /// is considered frozen.
        ///
        /// [`BindRange`]: MockCall::BindRange
        /// [`ReserveOnNode`]: MockCall::ReserveOnNode
        CurrentNode(u32),
        /// `bind_range(base, len, node)` was called (past the short-circuit).
        #[non_exhaustive]
        BindRange {
            /// Base address passed to `bind_range`, as `usize`.
            base: usize,
            /// Length in bytes passed to `bind_range`.
            len: usize,
            /// NUMA node id passed to `bind_range`.
            node: u32,
        },
        /// `reserve_on_node(size, align, node)` was called.
        #[non_exhaustive]
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
        ///
        /// task #726 (rust-intel audit §A3): was `pub`, committing this
        /// thread-local's internal representation (`RefCell<Vec<MockCall>>`)
        /// to the crate's semver surface even though the intended API is the
        /// encapsulating pair [`drain`]/[`set_current_node`] — no code
        /// anywhere in this workspace (including this crate's own tests)
        /// touched `CALLS`/`CURRENT_NODE_SLOT` directly. Narrowed to
        /// `pub(crate)`; external consumers keep `drain()`/`set_current_node()`
        /// as the only surface.
        pub(crate) static CALLS: RefCell<Vec<MockCall>> = const { RefCell::new(Vec::new()) };
        /// Value returned by `current_node()` under the mock.  Default 0.
        pub(crate) static CURRENT_NODE_SLOT: RefCell<u32> = const { RefCell::new(0) };
    }

    /// Drain every recorded call since the last drain (or test start).
    ///
    /// task #778 (round-closing review, F7): truthful only up to
    /// [`CALLS_CAP`] entries — past that, `record()` has already stopped
    /// pushing (see `CALLS_CAP`'s own doc), so a caller that drives more
    /// than `CALLS_CAP` calls without an intervening `drain()` gets a
    /// silently truncated (oldest-first) prefix here, not the full set.
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
                // task #726 (rust-intel audit §B14): cap the log so an
                // unbounded numa-aware-mock allocation scenario (see this
                // fn's own R11-5 note above) cannot grow this Vec forever.
                if b.len() < CALLS_CAP {
                    b.push(call);
                }
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
///
/// On a single-node Linux system where sysfs NUMA files are absent, this
/// function returns `Some(0)` (all CPUs are on node 0).
///
/// **On Linux, `None` for "CPU index cannot be mapped to a NUMA node via
/// sysfs" is currently UNREACHABLE** (task #722, rust-intel audit §F2, an
/// earlier revision of this doc claimed that case existed): every sysfs
/// read/permission failure, every truncated cpumap read, and every CPU
/// whose real node is `>= 64` all collapse into the SAME `Some(0)` fallback
/// as a genuinely topology-absent single-node system — the code does not
/// currently distinguish "no NUMA topology exists" from "topology exists
/// but this CPU's real node could not be determined." A caller cannot tell
/// these apart from the return value alone; `Some(0)` from this function on
/// Linux does not guarantee the calling thread is actually on node 0.
///
/// **First-call cost on Linux** (task #778, round-closing review, F12): the
/// VERY FIRST call to this function on a real Linux host performs up to 64
/// `open`/`read`/`close` syscall triples (one per candidate NUMA node) to
/// populate a process-lifetime topology cache; every subsequent call is a
/// pure in-memory bit-test with no syscalls at all. For a crate whose
/// selling point is "zero dependencies, `forbid(unsafe_code)`-friendly for
/// consumers," this first-call cost is a contract-level fact a caller on a
/// latency-sensitive cold path should know about — most callers should call
/// this once early (e.g. at startup) rather than assuming every call is
/// equally cheap.
#[must_use]
pub fn current_node() -> Option<u32> {
    #[cfg(feature = "mock")]
    {
        let n = mock::current_node_slot();
        mock::record(mock::MockCall::CurrentNode(n));
        // task #722 (rust-intel audit §F2): this used to unconditionally
        // wrap the scripted slot in `Some`, so `set_current_node(NO_NODE)`
        // (`u32::MAX`) produced `Some(NO_NODE)` -- violating this function's
        // own documented "returns `Option`, never the sentinel" guarantee,
        // and making every consumer's `None` branch impossible to exercise
        // under `mock`, the very feature that exists so CI can assert this
        // wrapping logic. Mirrored the real dispatch's remapping below.
        if n == NO_NODE {
            None
        } else {
            Some(n)
        }
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
/// **`node >= 64` is silently skipped** (task #722, rust-intel audit §F1):
/// the Linux implementation encodes the nodemask in a single `u64`, so it
/// can only address node IDs 0..63. `mbind(2)` itself supports node counts
/// up to `MAX_NUMNODES` (commonly 1024 on real kernels), so a caller on a
/// system with 64+ NUMA nodes binding to one of the high nodes gets a
/// silent no-op rather than an error or a wider nodemask.
///
/// task #725 (rust-intel audit §B5) / task #778 (round-closing review, F6):
/// this doc previously stated the `[base, base+len)` precondition
/// UNCONDITIONALLY, even though the function itself never touches `base`
/// when the node/len short-circuit fires below. Every current test call
/// site relies on exactly that short-circuit rather than a genuinely valid
/// reservation (`tests/mock_dispatch.rs` passes a dummy `0x1000 as *mut
/// u8`; `tests/smoke.rs`'s `bind_range_no_node_is_noop` /
/// `bind_range_zero_len_is_noop` pass stack-array pointers under
/// `mock`/`NO_NODE`/`len == 0`) — under the old wording those call sites
/// were UB-by-contract despite being harmless in practice, and a future
/// edit reordering the short-circuit after a real platform call would have
/// silently turned them into real UB with no doc contradiction to catch it.
/// task #725 fixed the unconditional framing; task #778/F6 fixed a residual
/// the same audit finding also named but #725 left open: `tests/smoke.rs`'s
/// `bind_range_on_owned_memory_does_not_panic` is the crate's ONE test call
/// site that reaches a real platform backend on a real host (`node =
/// current_node().unwrap_or(0)`, typically `Some(0)`, so NOT short-
/// circuited) against a `Vec<u8>` heap buffer — not an "OS reservation" in
/// the literal sense the `# Safety` section below used to require verbatim,
/// though harmless in practice (`mbind(MPOL_PREFERRED)` only sets kernel
/// page-policy metadata). The contract below now says "valid mapped range,"
/// which a heap buffer genuinely satisfies, closing that residual too.
///
/// # Safety
///
/// When `node != NO_NODE` and `len != 0`, `[base, base + len)` must be a
/// valid MAPPED RANGE (an OS reservation, a heap allocation, or any other
/// live mapping) owned exclusively by the caller for the duration of the
/// call. `mbind(2)`'s `MPOL_PREFERRED` policy applies at PAGE granularity,
/// so any other data sharing a page with `[base, base+len)` (e.g. a heap
/// allocator's neighboring objects) is affected by the same policy change —
/// harmless for a policy hint, but worth knowing when reasoning about which
/// bytes this call actually touches at the OS level. When either condition
/// (`node`/`len`) is false, the function returns immediately without
/// touching `base` at all (see the short-circuit at the top of the body) —
/// `base` need not be valid, dereferenceable, or even non-dangling in that
/// case, and any address value is permitted. The function never reads or
/// writes payload bytes in either case — it only passes the range to
/// `mbind(2)` (Linux) or a platform no-op, both of which set/ignore kernel
/// page-policy metadata, never payload memory.
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
///
/// # Semver coupling with `aligned-vmem`
///
/// This function's return type is `aligned-vmem`'s own [`Reservation`]
/// (re-exported here as [`numa_shim::Reservation`](crate::Reservation)), not
/// a `numa-shim`-owned wrapper. That is an intentional, accepted coupling
/// (item 46, `docs/CORRECTNESS_OPEN_ITEMS.md`), not an oversight: `numa-shim`
/// and `aligned-vmem` are sibling crates in this workspace, released
/// together, so a semver-major bump in `aligned-vmem`'s `Reservation` shape
/// forces a coordinated `numa-shim` bump in the same release — a cost
/// already paid by the shared release process, not an extra one. The
/// alternative (a `numa-shim`-owned newtype) was considered and rejected:
/// wrapping `Reservation` would need either full API forwarding (permanent
/// boilerplate that drifts on every `aligned-vmem` change) or an
/// `into_inner()` escape hatch that re-exposes the same type in a public
/// signature anyway, reproducing the coupling it was meant to remove.
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
// Linux cpumap parsing (task #721): extracted from the Linux-only `platform`
// module below into a target-INDEPENDENT module. These five functions are
// pure byte-slice parsing with no syscalls and no OS dependency whatsoever
// -- gating them inside `#[cfg(target_os = "linux")]` was an accident of
// code organization, not a genuine platform requirement, and it meant the
// crate's own most intricate parsing logic (the most-significant-word-first
// cpumap bitmask format) could ONLY be exercised on a real Linux host. This
// crate's own `mock` feature bypasses the whole `platform` module rather
// than exercising it, so before this change there was no way to run these
// functions on ANY host this project's CI or this session actually has.
// Moving them here (still `#[doc(hidden)]`, not part of this crate's public
// API — see the established "doc-hidden test-only forwarders" pattern in
// `CLAUDE.md`) lets `tests/cpumap_parser.rs` exercise the real parsing logic
// directly, on every target, closing the round-closing audit's §D1a finding
// for this half of its "zero behavioral oracles" claim.
// ---------------------------------------------------------------------------
#[doc(hidden)]
pub mod cpumap {
    /// Write `/sys/devices/system/node/nodeN/cpumap\0` into `buf` and return
    /// the nul-terminated slice. Avoids heap allocation.
    pub fn format_sysfs_path(buf: &mut [u8; 64], node: u32) -> &[u8] {
        const PREFIX: &[u8] = b"/sys/devices/system/node/node";
        const SUFFIX: &[u8] = b"/cpumap\0";
        let mut pos = 0usize;
        for &b in PREFIX {
            buf[pos] = b;
            pos += 1;
        }
        // task #727 (rust-intel audit §B7): `tmp` sized `[u8; 10]` -- the
        // maximum decimal digit count for ANY `u32` (`u32::MAX` =
        // 4294967295, 10 digits) -- rather than the previous `[u8; 4]`,
        // which panicked (`tmp[digits]` out of bounds) for `node >= 10000`.
        // Unreachable today (the only caller, `topology()` below, iterates
        // `0u32..64`), but this is a `#[doc(hidden)] pub` function reachable
        // by `tests/cpumap_parser.rs` with an arbitrary `node`, and the old
        // doc comment ("up to 3 digits for node < 1000") already disagreed
        // with the old buffer size (4 bytes = up to 3 digits + none-needed
        // slack, not 4 full digits) -- sizing for the real signature (`u32`)
        // removes the latent panic instead of just re-stating the caller's
        // unstated bound.
        let mut tmp = [0u8; 10];
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

    /// Parse a Linux cpumap bitmask string and test whether `cpu_idx` is set.
    ///
    /// Format: comma-separated hex 32-bit words, most-significant first,
    /// optional trailing newline. Example: `"00000000,00000003\n"` means
    /// CPUs 0 and 1 are in this node.
    pub fn parse_contains_cpu(data: &[u8], cpu_idx: u32) -> bool {
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

    /// Trim trailing `\n`/`\r`/` ` bytes.
    pub fn trim_end(data: &[u8]) -> &[u8] {
        let mut end = data.len();
        while end > 0 && (data[end - 1] == b'\n' || data[end - 1] == b'\r' || data[end - 1] == b' ')
        {
            end -= 1;
        }
        &data[..end]
    }

    /// Return the `n`-th token (0-indexed) delimited by `sep`.
    pub fn nth_token(data: &[u8], n: usize, sep: u8) -> Option<&[u8]> {
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

    /// Parse a hex string (no `0x` prefix) as `u32`. Returns `None` on error,
    /// including a token longer than 8 hex digits (would silently overflow
    /// `u32`; see task #727 below).
    pub fn parse_hex_u32(s: &[u8]) -> Option<u32> {
        if s.is_empty() {
            return None;
        }
        // task #727 (rust-intel audit §B26): previously absent, so a token
        // longer than 8 hex digits silently WRAPPED (`wrapping_shl` drops
        // the most-significant nibbles) instead of failing like every other
        // malformed input this parser rejects (empty token, invalid digit).
        // Real sysfs cpumap words are fixed 8 hex chars, so this had no live
        // impact, but a silently-wrong value for oversized input is
        // inconsistent with the rest of this parser's fail-closed behavior.
        if s.len() > 8 {
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

    /// Per-node raw cpumap byte capacity for the cached [`Topology`].
    ///
    /// task #777 (rust-intel audit round-closing review, finding F1):
    /// bounds each node's cached cpumap to a FIXED size instead of a
    /// heap-allocated `Vec` (see `Topology`'s own doc for why). 1024 bytes
    /// of comma-separated 8-hex-digit words covers `1024 / 9 * 32 ≈ 3640`
    /// CPUs on a SINGLE node -- comfortably past the per-node CPU count of
    /// any currently-shipping NUMA system (real multi-socket hosts run at
    /// most a few hundred CPUs per node; the crate's own separate 64-node
    /// ceiling, documented at `bind_range_impl_linux`'s `node >= 64` guard,
    /// already bounds total system-wide CPU count far below what 3640/node
    /// could ever combine with in practice). This is a DEVIATION from
    /// task #720's original 4 KiB read buffer (chosen there to cover
    /// ~14,500 CPUs on a SINGLE node, a bound this cache does not need to
    /// match since it exists on the stack/static, not the heap, and 64
    /// nodes x 4 KiB would be 256 KiB of static storage). A node whose
    /// cpumap file is wider than this buffer is treated identically to a
    /// read failure -- `false`/not-cached, same as before.
    const NODE_CPUMAP_BUF_LEN: usize = 1024;

    /// Boot-static cpu→node topology, parsed via sysfs ONCE and cached for
    /// the life of the process (task #723, rust-intel audit §E5: each
    /// `current_node()` call previously re-derived the mapping via up to 64
    /// open/read/close syscall triples -- expensive on an ALLOCATION path,
    /// since `current_node()` is re-entered from sefer-alloc's `numa-aware`
    /// feature). CPU-hotplug changes after the first call are NOT
    /// reflected -- acceptable for an `MPOL_PREFERRED` hint, itself already
    /// a soft, best-effort preference the kernel can override under memory
    /// pressure.
    ///
    /// task #778 (round-closing review, F12): the caveat above covers
    /// hotplug AFTER the first call; it does not cover the narrower window
    /// this cache also opens: `topology()` reads 64 sysfs files
    /// SEQUENTIALLY, so a hotplug event landing mid-scan can freeze a TORN
    /// snapshot for the rest of the process's lifetime (a CPU that existed
    /// only after the scan passed its node's file permanently falls back to
    /// the `Some(0)` single-node answer, where the OLD un-cached per-call
    /// derivation would have self-corrected on the very next call). Still
    /// acceptable for the same reason as the broader hotplug caveat above
    /// (a soft `MPOL_PREFERRED` hint), but worth naming as its own distinct
    /// property rather than folding it into the "after the first call"
    /// wording, which reads as covering only post-scan hotplug.
    ///
    /// task #777 (rust-intel audit round-closing review, finding F1, HIGH):
    /// task #723's original design cached `Vec<Vec<u8>>` -- ~65 heap
    /// allocations inside the `OnceLock::get_or_init` initializer below.
    /// `current_node()` is reachable from `AllocCore::alloc` (via
    /// `current_node_cached` on a cache miss, inside `reserve_small_segment`
    /// / `alloc_large_slow`), and the parent `sefer-alloc` crate's own `M5`
    /// invariant declares that entire path allocation-free/reentrancy-free
    /// specifically so it never re-enters the global allocator. Under a real
    /// `#[global_allocator] = SeferAlloc` + `numa-aware` deployment on
    /// Linux, the FIRST allocation needing a NUMA lookup would have
    /// triggered `topology()`'s heap allocation, which re-enters
    /// `GlobalAlloc::alloc`, which re-enters `current_node()`, which
    /// re-enters `OnceLock::get_or_init` on the SAME `TOPOLOGY` cell mid-
    /// initialization -- documented by `std::sync::OnceLock` as "an error
    /// to reentrantly initialize the cell from `f`... current implementation
    /// deadlocks" -- confirmed by the reviewing agent in a standalone
    /// scratch reproduction on this exact toolchain. Fixed by making the
    /// cache allocation-free: `Topology` below holds only fixed-size
    /// stack/static data (`[[u8; NODE_CPUMAP_BUF_LEN]; 64]` +
    /// `[usize; 64]`), so populating it touches no `Vec`/`Box`/heap at all,
    /// and the reentrancy hazard is structurally removed rather than
    /// guarded against.
    struct Topology {
        /// Node `i`'s cpumap byte length (0 if unreadable or oversized).
        len: [usize; 64],
        /// Node `i`'s raw cpumap file bytes, `buf[i][..len[i]]` valid.
        buf: [[u8; NODE_CPUMAP_BUF_LEN]; 64],
    }

    static TOPOLOGY: std::sync::OnceLock<Topology> = std::sync::OnceLock::new();

    fn topology() -> &'static Topology {
        TOPOLOGY.get_or_init(|| {
            let mut t = Topology {
                len: [0; 64],
                buf: [[0u8; NODE_CPUMAP_BUF_LEN]; 64],
            };
            for node in 0u32..64 {
                let mut path = [0u8; 64];
                let path_str = crate::cpumap::format_sysfs_path(&mut path, node);
                if let Some(n) = read_cpumap_into(path_str, &mut t.buf[node as usize]) {
                    t.len[node as usize] = n;
                }
            }
            t
        })
    }

    /// Map a CPU index to its NUMA node using the cached boot-static
    /// topology (`topology()` above). Pure in-memory bit-testing after the
    /// topology's one-time syscall-driven populate.
    ///
    /// Returns `0` when sysfs NUMA topology files are absent (single-node
    /// system where the kernel didn't compile NUMA support).
    fn cpu_to_numa_node(cpu_idx: u32) -> u32 {
        let topo = topology();
        for node in 0usize..64 {
            let len = topo.len[node];
            if len > 0 && crate::cpumap::parse_contains_cpu(&topo.buf[node][..len], cpu_idx) {
                return node as u32;
            }
        }
        // Single-node system or NUMA sysfs not present: treat as node 0.
        // mbind to node 0 on a single-node machine is a no-op from the OS
        // perspective (pages are already on node 0).
        0
    }

    /// Open the cpumap file at `path` and read its complete contents into
    /// the caller-supplied fixed buffer `out`, returning the byte count.
    ///
    /// The cpumap file format: `"00000000,00000001\n"` — comma-separated
    /// hex 32-bit words, most-significant word first; each word covers 32 CPUs.
    /// Parsing itself is delegated to `crate::cpumap` (task #721) -- a
    /// target-independent module so the parser can be exercised by
    /// `tests/cpumap_parser.rs` on every target, not just real Linux.
    ///
    /// Returns `None` on open/read failure, or if the file is wider than
    /// `out` (task #720, rust-intel audit §C4: a truncated read must never
    /// be silently treated as complete -- the most-significant-word-first
    /// `word_count`/`left_index` arithmetic would misalign on a prefix and
    /// return a WRONG node rather than failing loudly; task #777 preserves
    /// this fail-closed behavior while replacing the original heap `Vec`
    /// destination with `out: &mut [u8; NODE_CPUMAP_BUF_LEN]`, a fixed
    /// buffer that lives inside `Topology`'s own storage -- no allocation
    /// anywhere in this function).
    fn read_cpumap_into(path: &[u8], out: &mut [u8; NODE_CPUMAP_BUF_LEN]) -> Option<usize> {
        // SAFETY: `path` is a valid nul-terminated C string constructed above.
        // `open` is a POSIX syscall; we check for -1 on error.
        let fd = unsafe { libc_open(path.as_ptr() as *const core::ffi::c_char, 0) };
        if fd < 0 {
            return None;
        }
        let mut total = 0usize;
        loop {
            if total >= NODE_CPUMAP_BUF_LEN {
                // SAFETY: `fd` was opened by us and must be closed exactly once.
                unsafe { libc_close(fd) };
                return None;
            }
            // SAFETY: `out[total..]` is a valid writable sub-slice of `out`
            // (length `NODE_CPUMAP_BUF_LEN - total > 0`, checked above);
            // `fd` was returned by the successful `open` call above and not
            // yet closed.
            let n = unsafe {
                libc_read(
                    fd,
                    out[total..].as_mut_ptr() as *mut core::ffi::c_void,
                    NODE_CPUMAP_BUF_LEN - total,
                )
            };
            if n < 0 {
                // SAFETY: same as above.
                unsafe { libc_close(fd) };
                return None;
            }
            if n == 0 {
                break; // EOF: `out[..total]` holds the complete file.
            }
            total += n as usize;
        }
        // SAFETY: `fd` was opened by us and must be closed exactly once.
        unsafe { libc_close(fd) };
        if total == 0 {
            return None;
        }
        Some(total)
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
    // task #722 (rust-intel audit §F1): DEVIATION -- this nodemask is a
    // single u64, so nodes >= 64 are silently skipped rather than bound;
    // real `mbind(2)` supports node counts up to `MAX_NUMNODES` (commonly
    // 1024). Documented on `bind_range`'s own public rustdoc.
    if node == NO_NODE || node >= 64 {
        return;
    }
    // 64-bit nodemask with bit `node` set.
    let nodemask: u64 = 1u64 << node;
    // task #697 (rust-intel audit §F1): `maxnode` is NOT simply "number of
    // bits in the mask" -- the kernel's `get_nodes()` (mm/mempolicy.c)
    // decrements `maxnode` internally before computing which bits are
    // addressable, so `maxnode = 64` only covers bits 0..62, silently
    // dropping bit 63 (`bind_range(node = 63)` would pass an effectively
    // empty nodemask and `MPOL_PREFERRED` would silently degrade to local
    // allocation, with mbind's own errors already discarded by design, so
    // nothing would surface the miss). `libnuma` compensates for this exact
    // kernel quirk by always passing bitmask-size + 1; mirrored here.
    let maxnode: u64 = 65;
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
        // task #722 (rust-intel audit §F1): corrected -- the previous
        // comment here said this API "returns 0 on single-node or error",
        // conflating the BOOL return (`ok`) with the OUT-parameter
        // (`node`). The actual contract: `ok == 0` (Win32 `FALSE`) means the
        // call FAILED outright; `node == 0` on a SUCCESSFUL call is the
        // genuine single-node-system answer, not an error signal. Separately
        // (and NOT previously handled at all): Microsoft's own docs for
        // `GetNumaProcessorNodeEx` state the OUT node number is set to
        // `MAXUSHORT` (`u16::MAX`) when the given processor does not exist,
        // while the call STILL reports success -- that sentinel is checked
        // below, after the `ok` check.
        // SAFETY: `proc_num` was filled by `GetCurrentProcessorNumberEx`
        // above and is a valid `PROCESSOR_NUMBER`; `node` is a valid `u16`
        // out-pointer.
        let ok = unsafe { GetNumaProcessorNodeEx(&proc_num, &mut node) };
        if ok == 0 || node == u16::MAX {
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
    /// Strategy (mirrors `aligned-vmem`'s own Windows reservation,
    /// `win_reserve_commit` in `crates/aligned-vmem/src/lib.rs`): over-reserve
    /// `size + align` bytes as ADDRESS SPACE ONLY (`MEM_RESERVE`, no
    /// `MEM_COMMIT`), find the aligned chunk inside, then commit only the
    /// caller-requested `size` bytes at that aligned sub-range (`MEM_COMMIT`,
    /// still via `VirtualAllocExNuma` for API-site uniformity, though the
    /// NUMA preference is already fixed by the reserve call above — see the
    /// task #778/F2 note below). The WHOLE `over`-byte reservation is then
    /// adopted into an `aligned_vmem::Reservation` via
    /// [`aligned_vmem::Reservation::from_raw_parts`]; its `Drop` / release
    /// path will `VirtualFree(MEM_RELEASE)` the entire span exactly once.
    ///
    /// task #724 (rust-intel audit): the previous version committed the
    /// FULL `over = size + align` bytes in one `MEM_RESERVE | MEM_COMMIT`
    /// call -- up to double the commit-charge of the byte range the caller
    /// actually asked for and can use (e.g. `align == size` commits `2 *
    /// size`), silently contradicting this function's own doc claim that it
    /// "mirrors aligned-vmem's own Windows reservation" (aligned-vmem's
    /// `win_reserve_commit` has always reserved `over` but committed only
    /// `commit_len <= size`). Fixed to the same two-call reserve-then-commit
    /// shape.
    ///
    /// task #778 (round-closing review, F2, MEDIUM): the mechanism note
    /// above and this function's two `// SAFETY:` comments originally stated
    /// the `node` argument "has no effect" on the `MEM_RESERVE` call and
    /// "takes effect" on the `MEM_COMMIT` call — the EXACT INVERSE of
    /// Microsoft's documented `VirtualAllocExNuma` contract. Per the
    /// Win32 API reference, `nndPreferred` is "used only when allocating a
    /// NEW VA region (either committed or reserved)... ignored when the API
    /// is used to commit pages in a region that already exists" — so `node`
    /// takes effect on the `MEM_RESERVE` call (a new VA region) and is
    /// IGNORED on the `MEM_COMMIT` call (into the region the reserve call
    /// already created). Separately, no `VirtualAllocExNuma` call "actually
    /// allocates physical pages" at all — per the same reference, physical
    /// pages are allocated ON DEMAND at first touch, regardless of which
    /// call reserved/committed the range. The net shipped behavior was
    /// still correct (the preference IS recorded, by the reserve call, and
    /// the commit charge IS halved) — but purely because `node` happened to
    /// be passed on the reserve call too, which the ORIGINAL comments framed
    /// as a harmless no-op kept only "for API uniformity." A reader who
    /// trusted that framing would have every reason to drop `node` from the
    /// (documented-as-inert) reserve call and keep it only on the
    /// (documented-as-load-bearing) commit call — silently disabling Windows
    /// NUMA binding entirely, with no error from either call. Comments
    /// corrected to state the true mechanism.
    ///
    /// Returns `None` on contract violation (`align` not a power of two `>= PAGE`,
    /// `size` zero or not a multiple of `PAGE`) or when the OS refuses the
    /// reservation or the commit (OOM / no memory on the requested node).
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
        // MEM_RESERVE, PAGE_READWRITE, node)` reserves (but does not commit)
        // `over` bytes of address space, returning the base or NULL on
        // refusal. task #778 (F2): `node` IS load-bearing on this call --
        // per Microsoft's documented `nndPreferred` contract, the NUMA
        // preference is recorded when allocating a NEW VA region (reserved
        // or committed), which this call is; it is the ONLY call in this
        // function where `node` has any effect (see the corrected mechanism
        // note on this function's own rustdoc above).
        let raw = unsafe {
            VirtualAllocExNuma(
                GetCurrentProcess(),
                core::ptr::null_mut(),
                over,
                MEM_RESERVE,
                PAGE_READWRITE,
                node,
            )
        };
        if raw.is_null() {
            return None;
        }
        let raw_u = raw as usize;
        // Checked alignment arithmetic: `raw_u` is a Win32-returned base (page-
        // aligned, so overflow needs an allocation near the top of the address
        // space), but do not rely on that silently wrapping if it ever happens —
        // return None rather than hand from_raw_parts a wrapped base.
        let rounded = raw_u.checked_add(align - 1)?;
        let base_u = rounded & !(align - 1);
        let base = base_u as *mut u8;

        // SAFETY: `VirtualAllocExNuma(.., base, size, MEM_COMMIT, ..,
        // node)` commits exactly the caller-requested `size` bytes at the
        // aligned sub-range within the just-reserved `over`-byte region
        // (`base + size <= raw + over` by construction: `base <= raw +
        // align - 1` rounds down to `raw + align`, and `over = size +
        // align`). task #778 (F2): `node` has NO effect on this call --
        // per Microsoft's documented `nndPreferred` contract, the NUMA
        // preference is ignored when committing pages into a region that
        // already exists (the `MEM_RESERVE` call above already created it);
        // passed through for API-site uniformity only, not because it does
        // anything here. Physical pages are not allocated by EITHER call --
        // Windows allocates them on demand at first touch, regardless of
        // which call reserved/committed the range. NULL indicates commit-
        // charge exhaustion; the reservation is released and `None` returned.
        let committed = unsafe {
            VirtualAllocExNuma(
                GetCurrentProcess(),
                base.cast(),
                size,
                MEM_COMMIT,
                PAGE_READWRITE,
                node,
            )
        };
        if committed.is_null() {
            // SAFETY: `raw` was returned by the `MEM_RESERVE` call above and
            // has not been handed to any caller yet; releasing before
            // returning `None` cannot double-free.
            if unsafe { VirtualFree(raw, 0, MEM_RELEASE) } == 0 {
                // Double-failure: the commit failed AND the cleanup release failed.
                // Nothing more can be done here — the reservation may leak until
                // process exit. Returning None is still correct: the caller never
                // received an owning handle, so handing one out now would risk a
                // double-release. (Matched to this file's style: other "nothing more
                // we can do" cleanup paths release silently; this one is at least
                // counted, not silently ignored.)
                // Note: No logging here — this file's existing cleanup paths for
                // unrecoverable errors are silent (see the sibling MEM_RESERVE
                // failure branches above); matching that established pattern
                // rather than introducing a new one for this single site.
            }
            return None;
        }

        // Win32 contract: committing into an already-reserved region returns the
        // base address of that region subrange, i.e. exactly `base`. Documenting
        // and checking (debug builds) the assumption from_raw_parts relies on.
        debug_assert_eq!(committed.cast::<u8>(), base, "VirtualAllocExNuma(MEM_COMMIT) must return the requested base for an already-reserved region");

        // SAFETY of from_raw_parts:
        // - `base` is non-null, valid for `size` bytes (it's inside the
        //   `over`-byte reservation since `align <= over - size`), aligned
        //   to `align` (by construction above), and its `size`-byte range
        //   was just committed above. Win32 contract: `committed == base` for
        //   commit into an already-reserved region, verified by debug_assert above.
        // - `raw` is the start of the OS reservation, non-null.
        // - `over = size + align` is the full reservation length, multiple of PAGE.
        // - `align` was just used to align `base` — same value.
        // - The reservation will be released exactly once when the returned
        //   handle's `Drop` fires (or via `release` after `into_parts`).
        // - The reservation was created with `MEM_RESERVE` and the `size`-byte
        //   sub-range separately committed with `MEM_COMMIT` →
        //   `VirtualFree(MEM_RELEASE)` on the WHOLE `over`-byte span will
        //   accept it (matches aligned-vmem's own `win_reserve_commit` shape).
        let r = unsafe {
            aligned_vmem::Reservation::from_raw_parts(
                base,
                size,
                raw as *mut u8,
                over,
                align,
                false, // ordinary VirtualAllocExNuma pages -- MEM_LARGE_PAGES is never requested here
            )
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

    // task #726 (rust-intel audit §B25): `ProcessorNumber` is passed by
    // pointer to `GetCurrentProcessorNumberEx`/`GetNumaProcessorNodeEx` --
    // the hand-written mirror currently matches the real `PROCESSOR_NUMBER`
    // layout (size 4, align 2, offsets 0/2/3), but nothing pinned that
    // before this assertion, so a future field edit (reordering, adding a
    // field) would silently corrupt the out-parameter write these two FFI
    // calls make into it, with no compile-time signal.
    const _: () = {
        assert!(core::mem::size_of::<ProcessorNumber>() == 4);
        assert!(core::mem::align_of::<ProcessorNumber>() == 2);
        assert!(core::mem::offset_of!(ProcessorNumber, group) == 0);
        assert!(core::mem::offset_of!(ProcessorNumber, number) == 2);
        assert!(core::mem::offset_of!(ProcessorNumber, reserved) == 3);
    };

    extern "system" {
        fn GetCurrentProcessorNumberEx(proc_number: *mut ProcessorNumber);
        fn GetNumaProcessorNodeEx(processor: *const ProcessorNumber, node_number: *mut u16) -> i32;
    }

    // `VirtualAllocExNuma` is the load-bearing call: it is the ONLY way to
    // bind a reservation to a NUMA node on Windows (`VirtualAlloc` chooses
    // the node by kernel heuristic; there is no `mbind`-equivalent for
    // post-reservation binding). Declared locally to avoid pulling
    // `windows-sys` / `winapi` just for one syscall.
    #[cfg(feature = "vmem-integration")]
    extern "system" {
        // task #778 (round-closing review, F9): moved here from the
        // always-compiled extern block above -- its only two call sites
        // (`reserve_aligned_numa`) are already `vmem-integration`-gated, so
        // leaving it in the unconditional block made `cargo clippy
        // --all-targets -- -D warnings` fail on this crate's DEFAULT
        // feature set (what `cargo add numa-shim` produces) with "function
        // `GetCurrentProcess` is never used" -- every downstream Windows
        // consumer's default build saw this warning, and no CI job for this
        // crate runs clippy at all to catch it either way.
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn VirtualAllocExNuma(
            h_process: *mut core::ffi::c_void,
            lp_address: *mut core::ffi::c_void,
            dw_size: usize,
            fl_allocation_type: u32,
            fl_protect: u32,
            nnd_preferred: u32,
        ) -> *mut core::ffi::c_void;
        // task #724: needed to release a reservation whose commit step
        // failed, before any `aligned_vmem::Reservation` (whose own `Drop`
        // would otherwise own that release) has been constructed.
        fn VirtualFree(
            lp_address: *mut core::ffi::c_void,
            dw_size: usize,
            dw_free_type: u32,
        ) -> i32;
    }

    #[cfg(feature = "vmem-integration")]
    const MEM_RESERVE: u32 = 0x0000_2000;
    #[cfg(feature = "vmem-integration")]
    const MEM_COMMIT: u32 = 0x0000_1000;
    #[cfg(feature = "vmem-integration")]
    const MEM_RELEASE: u32 = 0x0000_8000;
    #[cfg(feature = "vmem-integration")]
    const PAGE_READWRITE: u32 = 0x04;
}

// ---- macOS stub -----------------------------------------------------------
// `not(miri)` is required here (matching the other three sibling
// `mod platform` blocks above -- Linux, Windows, and the generic
// fallback -- task #778/F11: line numbers drift with every edit to this
// file, so this is described by role instead of a citation that goes
// stale silently): without it, this block and the separate
// `#[cfg(miri)] mod platform` block below (any-OS-under-miri stub) BOTH
// satisfy their cfg simultaneously when running miri on macOS
// (`target_os = "macos"` is true AND `miri` is true), causing `mod platform`
// to be defined twice (E0428). No CI job caught this because every miri job
// runs on `ubuntu-latest` and the macOS job (`numa-shim-macos`) runs plain
// `cargo test`, never miri — the two conditions never crossed until an
// explicit macOS+miri CI job was added. If you ever touch this block or the
// `#[cfg(miri)]` block below, keep them mutually exclusive.
#[cfg(all(target_os = "macos", not(miri)))]
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
