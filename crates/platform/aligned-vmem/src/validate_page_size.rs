#[cfg(feature = "bench-internals")]
use super::page_size::validate_page_size_impl;

/// Validate a queried OS page size, falling back to PAGE if the value is invalid.
///
/// This function is pure and has no OS dependencies, making it directly testable.
/// It guards against:
/// - A queried value of 0
/// - A non-power-of-two value
/// - A value smaller than PAGE (4 KiB), which indicates `query_os_page_size()`
///   read the wrong sysconf(3) parameter entirely (e.g., a wrong `_SC_PAGESIZE`
///   constant on an untested target).
///
/// The OS page size is never smaller than PAGE on any target this crate supports,
/// so a queried value below it indicates a broken query. A hostile/broken value
/// would otherwise corrupt every rounding computation downstream, so we fall back
/// to the safe default PAGE.
#[cfg(feature = "bench-internals")]
#[cfg_attr(docsrs, doc(cfg(feature = "bench-internals")))]
#[inline]
#[must_use]
pub fn validate_page_size(queried: usize) -> usize {
    validate_page_size_impl(queried)
}
