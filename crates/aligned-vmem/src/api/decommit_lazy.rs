#[cfg(aligned_vmem_mock)]
use crate::mock;
use crate::os::{decommit_pages_impl, DecommitKind};
use crate::page_size::page_size;

/// Lazy decommit variant: hint the OS it MAY reclaim `[base+start, base+end)`
/// under memory pressure, cheaper than [`decommit`](crate::api::decommit) (Linux `MADV_FREE`,
/// macOS/iOS `MADV_FREE_REUSABLE`, FreeBSD/DragonFly `MADV_FREE`,
/// NetBSD/OpenBSD `MADV_FREE`, other Unix (including tvOS/watchOS) falls
/// back to `MADV_DONTNEED`; Windows falls back to the eager [`decommit`](crate::api::decommit)
/// path, which has no lazy equivalent).
///
/// Unlike [`decommit`](crate::api::decommit), on Linux the pages are NOT necessarily zeroed on next
/// access if the kernel has not yet reclaimed them (a write before reclamation
/// keeps the old contents and cancels the free) — so this is appropriate only
/// for memory whose contents the caller no longer needs but has not yet
/// overwritten. Cheaper reclaim; the kernel takes pages only under pressure.
/// **This benign-re-fault story is Linux-only: on Windows this call is the
/// eager [`decommit`](crate::api::decommit) path (see the summary above), where a write into the
/// range before [`recommit`](crate::api::recommit) is a hard `STATUS_ACCESS_VIOLATION` crash, not a
/// re-fault** — see [`decommit`](crate::api::decommit)'s platform-divergence paragraph above for the
/// incident this already caused.
///
/// **On macOS/iOS specifically, the cost ordering above is INVERTED, on the
/// RSS axis only** — see [`decommit`](crate::api::decommit)'s Darwin caveat: eager `decommit`'s
/// `MADV_DONTNEED` is a no-op there (drops nothing), while this lazy variant's
/// `MADV_FREE_REUSABLE` DOES drop the physical footprint immediately (not just
/// "under pressure"). Neither call zero-fills on next access on macOS/iOS —
/// that half of the non-guarantee is unchanged from the eager path. On
/// tvOS/watchOS this function falls back to the same `MADV_DONTNEED` as
/// [`decommit`](crate::api::decommit) (see the "other Unix" case in the summary above — the arm
/// that excludes macOS/iOS specifically, not "other Unix" in a general
/// sense), so there it IS a true no-op like the eager path, on both axes.
/// This tvOS/watchOS fallback is this crate's current `madv_free_advice` cfg
/// coverage (REASONED-FROM-SPEC, not verified on tvOS/watchOS hardware or a
/// tvOS/watchOS build target -- neither is available to this crate's CI):
/// `MADV_FREE_REUSABLE`'s numeric value is defined by XNU, the kernel all
/// four Darwin targets share, so it MAY work identically there too; but
/// tvOS/watchOS's userspace sandbox restrictions are unverified for this
/// specific advice value, so this is a plausible widening candidate, not an
/// established fact (see `madv_free_advice`'s doc and
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
/// item 48's S9 note, which must agree with this wording -- keep both in
/// sync if either changes).
///
/// **No fallible form:** this entry point is intentionally infallible, for the
/// same safety rationale as [`decommit`](crate::api::decommit). The `()` return carries no
/// write-permitting sentinel, so silently skipping on a contract violation is
/// safe. A `try_decommit_lazy` could be added as a future additive API decision.
///
/// `start`/`end` contract and safety are identical to [`decommit`](crate::api::decommit).
///
/// # Safety
///
/// Same as [`decommit`](crate::api::decommit).
pub unsafe fn decommit_lazy(base: *mut u8, start: usize, end: usize) {
    let ps = page_size();
    if start >= end || !start.is_multiple_of(ps) || !end.is_multiple_of(ps) {
        return;
    }
    #[cfg(aligned_vmem_mock)]
    mock::record(mock::Call::DecommitLazy {
        base: base.addr(),
        start,
        end,
    });
    #[cfg(not(aligned_vmem_mock))]
    // SAFETY: forwarded from the caller's contract; the per-OS routine touches
    // only kernel page-state, never the bytes.
    unsafe {
        decommit_pages_impl(base, start, end, DecommitKind::Lazy)
    };
}
