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
///   detected before any syscall — **and also** for an OS refusal whose code
///   is unavailable (under miri, where no real `errno`/`GetLastError` exists
///   to read, or the rare case where the platform's own `raw_os_error()`
///   itself returns `None`). Use [`is_invalid_argument`](Self::is_invalid_argument)
///   to tell the two `None` cases apart — task #712/#713 (2026-08-09): an
///   earlier version of this type stored the raw code as a bare `u32`
///   defaulting to `0` when unavailable, making "no OS code available"
///   indistinguishable from a genuine `code 0` / `ERROR_SUCCESS` — `os_code()`
///   reported `Some(0)` for both. Storing `Option<u32>` closes that gap at the
///   type level.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VmemError {
    /// Raw OS error code, or `None` when this is an invalid-argument error OR
    /// a genuine OS refusal whose code is unavailable.
    code: Option<u32>,
    /// `true` when the error is a caller contract violation (no OS involved).
    invalid_arg: bool,
}

impl VmemError {
    /// A caller-contract-violation error: the arguments were rejected before
    /// any OS call (e.g. `align` not a power of two, `size` not a page
    /// multiple, `size == 0`).
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

    /// A genuine OS refusal whose specific error code is unavailable — under
    /// miri (no real `errno`/`GetLastError` exists to read), or the rare case
    /// where the platform's own `raw_os_error()` itself returns `None`.
    /// Distinct from [`invalid_argument`](Self::invalid_argument):
    /// `is_invalid_argument()` is `false` here — the OS (or its miri stand-in)
    /// genuinely refused the operation, the cause is simply unknown.
    #[must_use]
    #[inline]
    pub const fn os_refusal_unknown_code() -> Self {
        Self {
            code: None,
            invalid_arg: false,
        }
    }

    /// The raw OS error code. `None` for
    /// [`invalid_argument`](Self::invalid_argument) OR for a genuine OS
    /// refusal whose code is unavailable
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
            f.write_str("invalid argument (size/align contract violation)")
        } else {
            match self.code {
                Some(code) => write!(f, "OS virtual-memory error (code {code})"),
                None => f.write_str("OS virtual-memory error (unknown OS error code)"),
            }
        }
    }
}

impl std::error::Error for VmemError {}

impl From<VmemError> for std::io::Error {
    fn from(e: VmemError) -> Self {
        match e.os_code() {
            Some(code) => std::io::Error::from_raw_os_error(code as i32),
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
