use crate::error::VmemError;
use crate::page_size::{page_size_or_poison, PAGE_SIZE_QUERY_FAILED};

/// Fallible [`page_size`](crate::page_size::page_size): the same cached
/// one-time OS page-size query, with a channel for the one thing `page_size`
/// cannot report — the query itself having failed.
///
/// [`page_size`](crate::page_size::page_size) is deliberately infallible: it
/// returns the conservative [`MIN_PAGE`](crate::MIN_PAGE) floor when the
/// one-time OS query produced an unusable answer, and the crate fails every
/// page-granular state operation closed from then on (see `page_size`'s own
/// "If the one-time OS query fails" paragraph). This twin is the upfront
/// detector: a caller that wants to choose a strategy at startup — rather
/// than discover the degraded state through `try_decommit` errors — asks
/// here once.
///
/// # Errors
///
/// [`VmemError::os_refusal_unknown_code`] if the OS page-size query failed
/// (not observed on any supported platform; see `page_size`'s rustdoc for
/// why). The error is an OS-side no-code failure, NOT
/// [`VmemError::invalid_argument`] — the caller did nothing wrong.
///
/// On success, the returned value is identical to `page_size()`'s: a power
/// of two `>= MIN_PAGE`, stable for the process lifetime.
pub fn try_page_size() -> Result<usize, VmemError> {
    let v = page_size_or_poison();
    if v == PAGE_SIZE_QUERY_FAILED {
        Err(VmemError::os_refusal_unknown_code())
    } else {
        Ok(v)
    }
}
