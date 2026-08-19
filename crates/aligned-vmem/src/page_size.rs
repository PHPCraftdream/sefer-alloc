use core::sync::atomic::{AtomicUsize, Ordering};

use super::page::PAGE;
#[cfg(all(unix, not(miri)))]
use crate::os::{sysconf, _SC_PAGESIZE};
#[cfg(all(windows, not(miri)))]
use crate::os::{GetSystemInfo, SystemInfo, WIN_ALLOCATION_GRANULARITY};

/// Cache for [`page_size`]. Three states:
///
/// - `0` — not yet queried: the next call queries the OS and stores one of the
///   other two states.
/// - [`PAGE_SIZE_QUERY_FAILED`] (`usize::MAX`) — queried, and the OS answer was
///   unusable (an error return, zero, not a power of two, or below [`PAGE`]).
///   Cached like a success so the degraded state is stable for the process
///   lifetime and the hot path stays one relaxed load.
/// - any other value — the validated OS page size (a power of two `>= PAGE`).
///
/// `pub(crate)` so the `page_size_override` test seam can store into it.
pub(crate) static PAGE_SIZE_CACHE: AtomicUsize = AtomicUsize::new(0);

/// The "queried and the answer was unusable" cache state (see
/// [`PAGE_SIZE_CACHE`]). `usize::MAX` is not a power of two, so it can never
/// collide with a real validated page size — and, deliberately, no real
/// offset is a multiple of it except `0`, so even a page-multiple validator
/// that reads it RAW (without an explicit poison check) rejects every
/// non-empty range: the poison fails closed by arithmetic, not by policing.
pub(crate) const PAGE_SIZE_QUERY_FAILED: usize = usize::MAX;

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

/// Raw three-state page-size accessor: the validated OS page size, or
/// [`PAGE_SIZE_QUERY_FAILED`] when the one-time OS query produced an unusable
/// answer. Never returns `0` (the cold path resolves the not-yet-queried
/// state before returning).
///
/// This is what every VALIDATOR in this crate consults — the poison must be
/// visible to validation, which is exactly what the public [`page_size`]
/// deliberately hides (it maps the poison to the conservative [`PAGE`] floor
/// so its "power of two" contract holds). Hot path: one relaxed load and a
/// zero test, identical to the pre-poison `page_size` body.
#[inline]
pub(crate) fn page_size_or_poison() -> usize {
    let cached = PAGE_SIZE_CACHE.load(Ordering::Relaxed);
    if cached != 0 {
        return cached;
    }
    init_page_size_cache()
}

/// Cold path of [`page_size_or_poison`]: query the OS once and cache either
/// the validated answer or the poison. Racing threads may both query; the
/// query is idempotent and both store the same value.
#[cold]
#[inline(never)]
fn init_page_size_cache() -> usize {
    let queried = query_os_page_size();
    // `validate_page_size_impl` returns its input exactly when the input is a
    // valid page size (every invalid input maps to PAGE; the one overlap —
    // the valid input PAGE mapping to itself — is benign), so
    // "output == input" IS the validity test, shared verbatim with the
    // `page_size_override` seam's acceptance rule.
    let value = if validate_page_size_impl(queried) == queried {
        queried
    } else {
        // Do NOT fold a failed query to PAGE: on a host whose real page is
        // larger (16 KiB Apple Silicon, 64 KiB aarch64 Linux), a believed
        // 4 KiB page would let decommit ranges pass validation that the OS
        // then rounds UP to the real page — discarding live data outside the
        // requested range. Poison instead: every page-granular state
        // operation fails closed until the process exits (`try_*` forms
        // report it; see `try_page_size`).
        PAGE_SIZE_QUERY_FAILED
    };
    PAGE_SIZE_CACHE.store(value, Ordering::Relaxed);
    value
}

/// Return the OS page size in bytes, querying the OS once and caching the
/// result.
///
/// Uses `sysconf(_SC_PAGESIZE)` on Unix and `GetSystemInfo` on Windows; under
/// miri it returns [`PAGE`] (4 KiB) by design (there is no real OS page). The
/// value is cached in a process-wide atomic after the first call, so repeated
/// calls are a single relaxed load. The returned value is always a power of
/// two and at least [`PAGE`].
///
/// **Correctness:** on Apple Silicon macOS the page size is 16 KiB, and on
/// some Linux configurations 64 KiB. Use this value (not [`PAGE`]) to round
/// decommit offsets: `decommit`/`decommit_lazy` validate BOTH endpoints
/// against `page_size()` before reaching the OS, and a misaligned endpoint is
/// a crate-level fail-closed skip (the call returns without any effect).
/// That crate-level validation is the load-bearing guard — do not rely on the
/// OS to reject a misaligned range for you. The kernels' own behavior is
/// asymmetric and platform-divergent: Linux `madvise(2)` rejects the call
/// only when the ADDRESS is misaligned, and rounds a misaligned LENGTH **up**
/// to the real page (touching memory past the requested range); Windows
/// `VirtualFree(MEM_DECOMMIT)` rejects nothing and widens the range in BOTH
/// directions (it decommits every page containing any byte of the range).
///
/// **If the one-time OS query fails** (`sysconf` returning an error, or a
/// nonsensical answer — not observed on any supported platform: on
/// Linux/macOS/Windows the page size comes from process-startup data that
/// cannot fail to exist), this function still returns [`MIN_PAGE`](crate::MIN_PAGE)
/// (= [`PAGE`], 4 KiB) so it stays infallible — but the crate records the
/// failure and every page-granular STATE operation fails closed for the
/// process lifetime: `decommit`/`decommit_lazy` become no-ops,
/// `recommit`/`commit_range` return `false`, the `try_*` forms and the lazy
/// reservation constructor report an OS-side no-code error (see
/// [`VmemError::os_refusal_unknown_code`](crate::VmemError::os_refusal_unknown_code)),
/// and [`try_page_size`](crate::try_page_size) returns `Err`. Reserving,
/// using, and releasing memory are unaffected — they never depend on the
/// runtime page size. Rationale: with the real page size unknown, a
/// decommit granularity guess could make the OS round a length up across
/// live data; refusing to decommit loses nothing but an optimization,
/// while guessing risks silent data loss.
#[must_use]
#[inline]
pub fn page_size() -> usize {
    let v = page_size_or_poison();
    if v == PAGE_SIZE_QUERY_FAILED {
        // Conservative display value: the documented "power of two >= PAGE"
        // property must hold even in the degraded state, and PAGE is the
        // crate-wide validation floor. The poison itself never escapes the
        // crate through this function.
        PAGE
    } else {
        v
    }
}

/// One-time raw OS page-size query, before validation. Routed through the
/// `aligned_vmem_page_size_override` test seam when that cfg is on, so a test
/// can simulate a failed query (or a larger-page host) on any hardware; the
/// production build compiles the seam out entirely.
pub(crate) fn query_os_page_size() -> usize {
    #[cfg(aligned_vmem_page_size_override)]
    if let Some(simulated) = crate::page_size_query_override::armed_query_result() {
        return simulated;
    }
    query_os_page_size_real()
}

#[cfg(all(unix, not(miri)))]
pub(crate) fn query_os_page_size_real() -> usize {
    // SAFETY: `sysconf(_SC_PAGESIZE)` takes an integer name and returns a
    // `c_long` (the page size, or -1 on error). No pointers involved.
    let v = unsafe { sysconf(_SC_PAGESIZE) };
    // An error (or nonsense) return maps to 0, which `init_page_size_cache`
    // classifies as a FAILED query (poison), never as a 4 KiB answer.
    if v <= 0 {
        0
    } else {
        v as usize
    }
}

#[cfg(all(windows, not(miri)))]
pub(crate) fn query_os_page_size_real() -> usize {
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
    //
    // A zero/garbage `dw_page_size` needs no explicit check here:
    // `init_page_size_cache` validates the raw answer and classifies anything
    // unusable as a failed query (poison).
    info.dw_page_size as usize
}

#[cfg(miri)]
pub(crate) fn query_os_page_size_real() -> usize {
    // Miri has no real OS page; use the crate's constant granularity.
    PAGE
}
