//! [`VmemError`] — the failure cause carried by the `try_*` API.
//!
//! Every fallible entry point ([`crate::try_reserve_aligned`],
//! [`crate::try_recommit`], …) returns `Result<_, VmemError>`. The error either
//! carries the raw OS error code (`errno` on Unix, `GetLastError` on Windows)
//! captured at the point of failure, or a sentinel for a caller contract
//! violation (bad `size`/`align`) that never reached the OS.

use core::fmt;

/// The cause of a virtual-memory operation failure.
///
/// - [`os_code`](Self::os_code) is `Some(code)` for a genuine OS refusal with
///   a known cause, where `code` is `errno` (Unix) or `GetLastError`
///   (Windows).
/// - [`os_code`](Self::os_code) is `None` for [`VmemError::invalid_argument`]
///   — a contract violation (e.g. non-power-of-two `align`, zero `size`)
///   detected before any syscall — **and also** for a no-code failure on
///   the OS side (see [`os_refusal_unknown_code`](Self::os_refusal_unknown_code)).
///   Use [`is_invalid_argument`](Self::is_invalid_argument)
///   to tell the two `None` cases apart — task #712/#713 (2026-08-09): an
///   earlier version of this type stored the raw code as a bare `u32`
///   defaulting to `0` when unavailable, making "no OS code available"
///   indistinguishable from a genuine `code 0` / `ERROR_SUCCESS` — `os_code()`
///   reported `Some(0)` for both. Storing `Option<u32>` closes that gap at the
///   type level.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VmemError {
    /// Raw OS error code, or `None` when this is an invalid-argument error OR
    /// a no-code failure on the OS side.
    code: Option<u32>,
    /// `true` when the error is a caller contract violation (no OS involved).
    invalid_arg: bool,
}

impl VmemError {
    /// A caller-contract-violation error: the arguments were rejected before
    /// any OS call. This covers MORE than the `size`/`align` contract, which is
    /// why neither this doc nor `Display` names that one contract specifically
    /// any more (task #1046, finding R7-7 — `Display` used to print
    /// "size/align contract violation" for every one of these). The rejected
    /// classes, enumerated from the actual call sites rather than guessed:
    ///
    /// - `size`/`align` contract: `align` not a power of two, `size` not a page
    ///   multiple, `size == 0`, or the `size + align` sum overflowing.
    /// - The `initial_commit` contract on the lazy path.
    /// - The commit/recommit RANGE contract: `start > end`, either endpoint not
    ///   a multiple of the runtime `page_size()`, or `end` past `len()`.
    /// - Huge-page alignment on the Linux/Android huge path.
    /// - An internal fit computation failing — deliberately mapped here rather
    ///   than to a stale OS error code, because no OS call refused anything.
    ///
    /// The specific cause is documented on the method that returned it; this
    /// type carries no payload naming which parameter was at fault.
    #[must_use]
    #[inline]
    pub const fn invalid_argument() -> Self {
        Self {
            code: None,
            invalid_arg: true,
        }
    }

    /// Wrap a raw OS error code (`errno` / `GetLastError`).
    #[must_use]
    #[inline]
    pub const fn from_os_code(code: u32) -> Self {
        Self {
            code: Some(code),
            invalid_arg: false,
        }
    }

    /// A no-code failure on the OS side — the operation failed without a
    /// real OS error code to report. FOUR sources — **keep this count in
    /// sync with the list below when adding one**: task #1139 added the
    /// fourth and left the count reading "Three", corrected by task #1141
    /// (task #1106/L2 — an earlier revision of this doc called ALL of them
    /// a "genuine OS refusal", which is false for the third and fourth):
    /// - under miri (no real `errno`/`GetLastError` exists to read) — a
    ///   genuine refusal by the miri stand-in;
    /// - the rare case where the platform's own `raw_os_error()` itself
    ///   returns `None` — a genuine OS refusal with an unavailable cause;
    /// - (task #1068/F2) the crate's own rejection of the kernel's R7-11
    ///   address-zero `mmap` grant on Unix — `mmap` SUCCEEDED and the crate
    ///   unmapped the grant itself, so no syscall refused anything and there
    ///   is no real code to report. This source is not a refusal by the OS
    ///   at all; it shares this sentinel because it is equally not a caller
    ///   contract violation, and the type carries no further discrimination
    ///   (crate still at 0.2.0, unpublished — a distinct kind was judged not
    ///   worth the public-API surface; see the task #1106/L2 record);
    /// - a FAILED one-time OS page-size query (never observed on a supported
    ///   platform): [`crate::try_page_size`] and every page-granular `try_*`
    ///   state operation (`try_decommit`, `try_recommit`,
    ///   `try_commit_range`, the lazy reservation constructor) report the
    ///   crate's fail-closed degraded state through this sentinel — the
    ///   caller's arguments are not at fault, and no per-call OS code
    ///   exists (the query failed once, at first use, possibly long before
    ///   the reporting call). Same no-new-kind reasoning as the third
    ///   source above.
    ///
    /// Distinct from [`invalid_argument`](Self::invalid_argument):
    /// `is_invalid_argument()` is `false` here — the failure originated on
    /// the OS side (or in the crate's response to an unusable OS grant), not
    /// in the caller's arguments.
    ///
    /// **This FOUR-source count is scoped to production causes; it
    /// deliberately excludes two TEST-ONLY construction SOURCES, spread
    /// across three TEST-ONLY construction SITES** (task #1173/L2,
    /// re-measured for this doc's own correction — task #1194 — against the
    /// actual call sites rather than re-asserted from an earlier audit's
    /// count. Counted with doc mentions EXCLUDED, because a raw
    /// `grep -rn "VmemError::os_refusal_unknown_code()"
    /// crates/aligned-vmem/src/` also matches prose like this very
    /// sentence — its total therefore changes whenever this paragraph is
    /// edited, which is exactly how task #1194's first attempt recorded a
    /// figure its own edit falsified one line later. The stable count is
    /// `grep -rn "VmemError::os_refusal_unknown_code()"
    /// crates/aligned-vmem/src/ | grep -vE ":\s*(///|//!|//)"` → 10 real
    /// construction sites; of those 10, 7 are production sites — matching
    /// the four causes below — and 3 are
    /// test-only sites): the `aligned_vmem_mock` backend's scripted
    /// commit/reserve fault injection (`crate::mock`, gated on that cfg —
    /// TWO sites, `take_reserve_fault`/`take_commit_fault`) and the
    /// real-path `fault-injection` feature's simulated commit failure
    /// (`crate::fault_injection`, ONE site in `api/commit_range.rs`) both
    /// also construct this sentinel, to simulate a no-code OS failure
    /// deterministically without touching the OS — see each module's own
    /// doc for why NEITHER SOURCE is a fifth or sixth PRODUCTION source:
    /// all three sites exist only under test-only cfgs/features and are not
    /// reachable in an ordinary build.
    #[must_use]
    #[inline]
    pub const fn os_refusal_unknown_code() -> Self {
        Self {
            code: None,
            invalid_arg: false,
        }
    }

    /// The raw OS error code. `None` for
    /// [`invalid_argument`](Self::invalid_argument) OR for a no-code failure
    /// on the OS side
    /// ([`os_refusal_unknown_code`](Self::os_refusal_unknown_code)) — use
    /// [`is_invalid_argument`](Self::is_invalid_argument) to tell those two
    /// `None` cases apart.
    #[must_use]
    #[inline]
    pub const fn os_code(&self) -> Option<u32> {
        self.code
    }

    /// `true` if this is a caller contract violation rather than an OS refusal.
    #[must_use]
    #[inline]
    pub const fn is_invalid_argument(&self) -> bool {
        self.invalid_arg
    }

    /// Capture the current thread's last OS error (`errno` / `GetLastError`).
    /// Yields [`os_refusal_unknown_code`](Self::os_refusal_unknown_code) under
    /// miri, or if the platform's own `raw_os_error()` returns `None`.
    ///
    /// **Timing contract**: call this IMMEDIATELY after the syscall whose
    /// failure it is meant to capture, before any other FFI call (including
    /// cleanup) — any intervening call may overwrite `errno`/`GetLastError`
    /// (task #713).
    #[must_use]
    pub fn last_os_error() -> Self {
        match last_os_error_code() {
            Some(code) => Self::from_os_code(code),
            None => Self::os_refusal_unknown_code(),
        }
    }
}

impl fmt::Debug for VmemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.invalid_arg {
            f.write_str("VmemError::InvalidArgument")
        } else {
            f.debug_struct("VmemError")
                .field("os_code", &self.code)
                .finish()
        }
    }
}

impl fmt::Display for VmemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.invalid_arg {
            f.write_str("invalid argument (argument contract violation)")
        } else {
            match self.code {
                Some(code) => write!(f, "OS virtual-memory error (code {code})"),
                None => f.write_str(
                    "OS virtual-memory error (unknown OS error code — either a \
                     genuine OS refusal with an unreadable cause, or the crate \
                     rejected an unusable OS grant, e.g. a granted address-zero \
                     mapping)",
                ),
            }
        }
    }
}

impl std::error::Error for VmemError {}

impl From<VmemError> for std::io::Error {
    fn from(e: VmemError) -> Self {
        match e.os_code() {
            Some(code) => {
                // Win32 GetLastError returns a u32; a code with the high bit set
                // (e.g. HRESULT 0x8007000E) becomes negative when cast to i32.
                // Use try_from to detect overflow; fall back to Unknown on
                // overflow (theoretical—no real VirtualAlloc/VirtualFree
                // failure produces such a code in practice).
                match i32::try_from(code) {
                    Ok(signed) => std::io::Error::from_raw_os_error(signed),
                    Err(_) => {
                        // Code doesn't fit in i32 (high bit set). Preserve the
                        // VmemError as io::Error::other to avoid silent
                        // misinterpretation.
                        std::io::Error::other(e)
                    }
                }
            }
            None if e.is_invalid_argument() => {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
            }
            None => std::io::Error::other(e),
        }
    }
}

#[cfg(not(miri))]
fn last_os_error_code() -> Option<u32> {
    std::io::Error::last_os_error()
        .raw_os_error()
        .map(|c| c as u32)
}

#[cfg(miri)]
fn last_os_error_code() -> Option<u32> {
    None
}
