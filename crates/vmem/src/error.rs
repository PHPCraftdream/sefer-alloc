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
/// - [`os_code`](Self::os_code) is `Some(code)` for a genuine OS refusal, where
///   `code` is `errno` (Unix) or `GetLastError` (Windows).
/// - [`os_code`](Self::os_code) is `None` for [`VmemError::invalid_argument`] —
///   a contract violation (e.g. non-power-of-two `align`, zero `size`) detected
///   before any syscall.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VmemError {
    /// Raw OS error code, or `0` when this is an invalid-argument error.
    code: u32,
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
            code: 0,
            invalid_arg: true,
        }
    }

    /// Wrap a raw OS error code (`errno` / `GetLastError`).
    #[must_use]
    #[inline]
    pub const fn from_os_code(code: u32) -> Self {
        Self {
            code,
            invalid_arg: false,
        }
    }

    /// The raw OS error code, or `None` for [`invalid_argument`](Self::invalid_argument).
    #[must_use]
    #[inline]
    pub const fn os_code(&self) -> Option<u32> {
        if self.invalid_arg {
            None
        } else {
            Some(self.code)
        }
    }

    /// `true` if this is a caller contract violation rather than an OS refusal.
    #[must_use]
    #[inline]
    pub const fn is_invalid_argument(&self) -> bool {
        self.invalid_arg
    }

    /// Capture the current thread's last OS error (`errno` / `GetLastError`).
    #[must_use]
    pub fn last_os_error() -> Self {
        Self::from_os_code(last_os_error_code())
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
            write!(f, "OS virtual-memory error (code {})", self.code)
        }
    }
}

impl std::error::Error for VmemError {}

#[cfg(all(unix, not(miri)))]
fn last_os_error_code() -> u32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32
}

#[cfg(all(windows, not(miri)))]
fn last_os_error_code() -> u32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32
}

#[cfg(miri)]
fn last_os_error_code() -> u32 {
    0
}
