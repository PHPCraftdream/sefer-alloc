use core::sync::atomic::{AtomicUsize, Ordering};

use super::page::PAGE;
#[cfg(all(unix, not(miri)))]
use crate::os::{sysconf, _SC_PAGESIZE};
#[cfg(all(windows, not(miri)))]
use crate::os::{GetSystemInfo, SystemInfo, WIN_ALLOCATION_GRANULARITY};

/// Cache for [`page_size`]. `0` means "not yet queried"; a real page size is
/// always a non-zero power of two so `0` is an unambiguous sentinel.
static PAGE_SIZE_CACHE: AtomicUsize = AtomicUsize::new(0);

/// Internal implementation of page size validation.
#[inline]
#[must_use]
pub(crate) fn validate_page_size_impl(queried: usize) -> usize {
    if queried >= PAGE && queried.is_power_of_two() {
        queried
    } else {
        PAGE
    }
}

/// Return the OS page size in bytes, querying the OS once and caching the
/// result.
///
/// Uses `sysconf(_SC_PAGESIZE)` on Unix and `GetSystemInfo` on Windows; under
/// miri (or if the OS query returns a nonsensical value) it falls back to
/// [`PAGE`] (4 KiB). The value is cached in a process-wide atomic after the
/// first call, so repeated calls are a single relaxed load.
///
/// **Correctness:** on Apple Silicon macOS the page size is 16 KiB, and on some
/// Linux configurations 64 KiB. A caller that decommits at 4 KiB-but-not-page
/// multiples gets a crate-level silent skip (the call returns without any
/// effect), because `decommit`/`decommit_lazy` validate against `page_size()`
/// before reaching the OS. Even at the OS level, madvise(2) rejects the entire
/// call (all-or-nothing) when `addr` is not a multiple of the real page size.
/// Use this value (not [`PAGE`]) to round decommit offsets.
#[must_use]
#[inline]
pub fn page_size() -> usize {
    let cached = PAGE_SIZE_CACHE.load(Ordering::Relaxed);
    if cached != 0 {
        return cached;
    }
    let queried = query_os_page_size();
    let value = validate_page_size_impl(queried);
    PAGE_SIZE_CACHE.store(value, Ordering::Relaxed);
    value
}

#[cfg(all(unix, not(miri)))]
fn query_os_page_size() -> usize {
    // SAFETY: `sysconf(_SC_PAGESIZE)` takes an integer name and returns a
    // `c_long` (the page size, or -1 on error). No pointers involved.
    let v = unsafe { sysconf(_SC_PAGESIZE) };
    if v <= 0 {
        0
    } else {
        v as usize
    }
}

#[cfg(all(windows, not(miri)))]
fn query_os_page_size() -> usize {
    // SAFETY: `GetSystemInfo` fills the caller-provided `SYSTEM_INFO`; the
    // struct is stack-allocated and fully written by the call.
    let mut info = SystemInfo::default();
    unsafe { GetSystemInfo(&mut info) };
    debug_assert!(
        info.dw_allocation_granularity as usize >= WIN_ALLOCATION_GRANULARITY,
        "OS-reported allocation granularity ({}) is smaller than the hardcoded constant ({}); \
         this would break the single-call fast path's alignment guarantee",
        info.dw_allocation_granularity,
        WIN_ALLOCATION_GRANULARITY
    );
    // NOTE: This debug_assert fires only when `query_os_page_size()` is called,
    // which happens on the cold path (decommit/decommit_lazy) — since task #897
    // removed the `align > page_size() &&` conjunct, the reserve fast path no
    // longer consults `page_size()` at all. It does NOT fire on the Windows
    // single-call reservation fast path, which uses `WIN_ALLOCATION_GRANULARITY`
    // directly.
    info.dw_page_size as usize
}

#[cfg(miri)]
fn query_os_page_size() -> usize {
    // Miri has no real OS page; use the crate's constant granularity.
    PAGE
}
