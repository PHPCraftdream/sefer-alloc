use core::ptr::NonNull;
#[cfg(feature = "bench-internals")]
use core::sync::atomic::Ordering;

#[cfg(feature = "bench-internals")]
use crate::bench_internals::{
    WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES, WINDOWS_LARGE_PAGE_PLAIN_FALLBACK_SUCCESSES,
    WINDOWS_LARGE_PAGE_RETRY_FAILURES, WINDOWS_RESERVE_COMMIT_SINGLE_CALLS,
    WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS, WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS,
    WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES, WINDOWS_VIRTUALFREE_RELEASE_ATTEMPTS,
    WINDOWS_VIRTUALFREE_RELEASE_FAILURES,
};
use crate::error::VmemError;
use crate::os::{align_up_addr, DecommitKind};

#[cfg(all(windows, not(miri)))]
pub(crate) fn reserve_aligned_raw(
    size: usize,
    align: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    win_reserve_commit(size, align, size, 0).map(
        |(base, reservation, reservation_len, _granted_huge)| (base, reservation, reservation_len),
    )
}

/// Windows over-reserve + commit helper shared by the eager, lazy and huge
/// paths. Takes two execution paths:
///
/// **Single-call fast path** (`align <= GetLargePageMinimum() && commit_len == size` for
/// large-page requests, `align <= WIN_ALLOCATION_GRANULARITY && commit_len == size` for
/// ordinary requests): reserves and commits `commit_len` bytes in one `VirtualAlloc` call with
/// `MEM_RESERVE | MEM_COMMIT | extra_commit_flags` (e.g., `MEM_LARGE_PAGES`).
/// If the initial call fails with `extra_commit_flags != 0`, it retries without
/// the extra flags (ordinary-page fallback). Returns `(base, base, commit_len, huge_granted)`
/// — the fourth element indicates whether the huge-page request actually succeeded
/// (true only when `extra_commit_flags` was nonzero AND the initial attempt succeeded
/// without falling back to ordinary pages).
///
/// **Two-call path** (all other cases): reserves address space in a first call,
/// then commits `commit_len` bytes with plain `MEM_COMMIT` (no extra flags applied).
/// The reserve size is conditional: when `align <= WIN_ALLOCATION_GRANULARITY`,
/// the fast-reserve optimization attempts to reserve exactly `size` bytes and
/// uses it if the result happens to already satisfy alignment; otherwise, it
/// reserves `size + align` bytes to guarantee an aligned base can be found.
/// Returns `(base, region, over, false)` — the fourth element is always `false`
/// because the two-call path never requests `MEM_LARGE_PAGES` (Windows rejects
/// it on pre-reserved regions anyway). On commit failure the whole reservation
/// is released and `Err` returned.
///
/// task #713: every `Err` here carries a [`VmemError`] captured IMMEDIATELY
/// after the syscall that produced it, before any cleanup FFI call that could
/// clobber `GetLastError` — a fit-computation failure (not a real OS refusal)
/// maps to [`VmemError::invalid_argument`] rather than a stale/irrelevant
/// error code.
#[cfg(all(windows, not(miri)))]
fn win_reserve_commit(
    size: usize,
    align: usize,
    commit_len: usize,
    extra_commit_flags: u32,
) -> Result<(NonNull<u8>, NonNull<u8>, usize, bool), VmemError> {
    // V21 (task #848): for align <= 64 KiB, use a single combined
    // VirtualAlloc(NULL, size, MEM_RESERVE | MEM_COMMIT [| extra_flags])
    // call instead of two calls. VirtualAlloc(NULL, ...) already returns
    // a base aligned to WIN_ALLOCATION_GRANULARITY (64 KiB on all supported
    // Windows targets), so the alignment contract is satisfied by construction.
    //
    // II-3 (2026-08-16 audit finding): when requesting large pages
    // (extra_commit_flags includes MEM_LARGE_PAGES), widen the fast-path
    // condition to use GetLargePageMinimum() instead of WIN_ALLOCATION_GRANULARITY.
    // A granted large-page allocation is naturally aligned to at least the
    // large-page minimum (typically 2 MiB on Windows), so it will already
    // satisfy alignments up to that minimum. The unconditional alignment check
    // below guarantees correctness even if large pages are not granted.
    //
    // NOTE: GetLargePageMinimum() returns 0 on systems/CPU that do not support
    // large pages at all (Microsoft documentation). Since align >= PAGE > 0 always,
    // the comparison align <= 0 is always false for a positive align, meaning the
    // fast path becomes unreachable on such hosts. This is a safe degenerate case:
    // we fall through correctly to the two-call path, same as when align > threshold.
    // No special-case code is needed; the existing threshold comparison handles it.
    //
    // Historical note: a ~4.6 µs / ~33% reduction claim was made in the original
    // V21 commit, inherited from pre-#848 measurement (R32_13) of the OLD two-call
    // path. That claim has NOT been re-measured for the current single-call fast
    // path code. The claim should be treated as an unverified hypothesis, not a
    // validated benchmark result.
    //
    // `commit_len == size` is REQUIRED, not just an optimization detail: a
    // single VirtualAlloc(.., MEM_RESERVE | MEM_COMMIT, ..) call reserves AND
    // commits the SAME byte range -- there is no way to reserve `size` bytes
    // while committing only a smaller `commit_len` in one call. The lazy-commit
    // path (`reserve_aligned_lazy` -> `commit_range` later) calls this function
    // with `commit_len < size` by design (reserve the full span up front,
    // commit incrementally). Taking the single-call path there would silently
    // shrink the actual reservation to `commit_len` bytes, breaking every
    // later `commit_range` call past that point -- confirmed concretely: a
    // targeted repro (align=4 KiB, size=64 KiB, initial_commit=4 KiB) showed
    // the returned `reservation_len` was only 4096, not 65536, and the
    // follow-up `commit_range` past `initial_commit` failed. Guarding on
    // `commit_len == size` keeps the fast path to exactly the case it's sound
    // for (the eager `reserve_aligned`/`reserve_aligned_huge` callers, which
    // always pass `commit_len == size`) and routes the lazy-commit caller
    // through the unchanged two-call path below.
    let fast_path_align_threshold = if extra_commit_flags != 0 {
        // When requesting large pages, the threshold is the large-page minimum.
        // See the GetLargePageMinimum()==0 degenerate-case note above this function.
        unsafe { GetLargePageMinimum() }
    } else {
        WIN_ALLOCATION_GRANULARITY
    };
    if align <= fast_path_align_threshold && commit_len == size {
        // Single-call path: reserve+commit together.
        // Track whether huge pages were actually granted; initialized from the
        // request flag, but may be cleared if the retry fallback succeeds.
        let mut huge_granted = extra_commit_flags != 0;
        let base = unsafe {
            // SAFETY: `VirtualAlloc(NULL, commit_len, MEM_RESERVE | MEM_COMMIT
            // | extra_commit_flags, PAGE_READWRITE)` reserves and commits in one
            // syscall, returning the base or NULL on OOM/refusal. NULL is checked
            // below.
            let p = VirtualAlloc(
                core::ptr::null_mut(),
                commit_len,
                MEM_RESERVE | MEM_COMMIT | extra_commit_flags,
                PAGE_READWRITE,
            );
            match NonNull::new(p as *mut u8) {
                Some(n) => n,
                None => {
                    if extra_commit_flags != 0 {
                        // Best-effort retry: try without extra_commit_flags (e.g.
                        // MEM_LARGE_PAGES). This matches the two-call path's fallback
                        // behavior. On success, `huge_granted` is cleared because the
                        // retry succeeded with ordinary pages, not the original large-page
                        // request.
                        // SAFETY: fresh anonymous reserve+commit at a kernel-chosen
                        // address; NULL is checked below.
                        let plain = VirtualAlloc(
                            core::ptr::null_mut(),
                            commit_len,
                            MEM_RESERVE | MEM_COMMIT,
                            PAGE_READWRITE,
                        );
                        match NonNull::new(plain as *mut u8) {
                            Some(n) => {
                                huge_granted = false; // Fallback to ordinary pages
                                #[cfg(feature = "bench-internals")]
                                WINDOWS_LARGE_PAGE_PLAIN_FALLBACK_SUCCESSES
                                    .fetch_add(1, Ordering::Relaxed);
                                n
                            }
                            None => {
                                #[cfg(feature = "bench-internals")]
                                WINDOWS_LARGE_PAGE_RETRY_FAILURES.fetch_add(1, Ordering::Relaxed);
                                return Err(VmemError::last_os_error());
                            }
                        }
                    } else {
                        return Err(VmemError::last_os_error());
                    }
                }
            }
        };
        // task #917 (finding H2C6, Windows analogue of Unix task #897/finding U1):
        // this check is UNCONDITIONAL. The fast-path's premise (VirtualAlloc(NULL, ...)
        // returns a base aligned to WIN_ALLOCATION_GRANULARITY) is REASONED from Microsoft
        // documentation but never verified at the point of use. The only verification is
        // a debug_assert in query_os_page_size() which compiles out of --release and
        // is not even called by this fast path (it lives on the cold decommit path).
        // If WIN_ALLOCATION_GRANULARITY were wrong (unlikely but theoretically possible
        // on a future Windows version), this check would catch it and fall through to
        // the two-call path, guaranteeing the documented alignment contract. Deliberately
        // a real runtime check, not a debug_assert: release builds are exactly where
        // an unverified constant matters (CLAUDE.md's R26-4 rule: debug_assert compiles
        // out of --release).
        // task #921/V-6: this check applies to BOTH the initial allocation AND any
        // retry fallback - we never return without verifying alignment, even on the
        // retry path that strips extra_commit_flags (e.g. MEM_LARGE_PAGES).
        if !base.as_ptr().addr().is_multiple_of(align) {
            // SAFETY: `base` was just allocated with VirtualAlloc(MEM_RESERVE | MEM_COMMIT)
            // and has not been released yet; releasing before handing to a caller prevents
            // a leak.
            unsafe { winapi_virtual_release(base.as_ptr()) };
            #[cfg(feature = "bench-internals")]
            if extra_commit_flags != 0 {
                // Track alignment failures: allocation succeeded but base is misaligned.
                WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES.fetch_add(1, Ordering::Relaxed);
            }
            // Fall through to the two-call path below.
        } else {
            #[cfg(feature = "bench-internals")]
            WINDOWS_RESERVE_COMMIT_SINGLE_CALLS.fetch_add(1, Ordering::Relaxed);
            // Single-call path: base == region (no over-reserve).
            // Return (base, base, commit_len, huge_granted).
            // NOTE: huge_granted reflects which VirtualAlloc call actually succeeded:
            // if the retry fallback was taken, huge_granted is false (ordinary pages);
            // otherwise it is true only when extra_commit_flags (e.g. MEM_LARGE_PAGES)
            // was requested AND the initial attempt succeeded. We do not query the
            // actual grant at runtime, but this correctly tracks the observable
            // difference between "large-page request succeeded" vs "ordinary-page
            // fallback".
            return Ok((base, base, commit_len, huge_granted));
        }
    }

    // Two-call path (align > WIN_ALLOCATION_GRANULARITY for ordinary requests,
    // align > GetLargePageMinimum() for large-page requests, or a partial initial commit,
    // or single-call alignment check failed).
    // task #921/V-32: when align <= WIN_ALLOCATION_GRANULARITY, try a fast-reserve
    // path: VirtualAlloc(NULL, size, MEM_RESERVE, ...) may return a base already
    // aligned to the requested alignment, avoiding the size+align over-reserve overhead.
    // If it's not aligned, we release it and fall through to the over-reserve path.
    let (region, over) = if align <= WIN_ALLOCATION_GRANULARITY {
        let candidate = unsafe {
            // SAFETY: `VirtualAlloc(NULL, size, MEM_RESERVE, PAGE_READWRITE)`
            // reserves (but does not commit) `size` bytes of address space,
            // returning the base or NULL on OOM/refusal. NULL is checked below.
            let p = winapi_virtual_reserve(size);
            match NonNull::new(p as *mut u8) {
                Some(n) => n,
                // Nothing was reserved; no cleanup needed, so capturing here is
                // already the immediate-capture the task requires.
                None => return Err(VmemError::last_os_error()),
            }
        };
        let candidate_ptr = candidate.as_ptr();
        // Check if the reserved region happens to already be aligned to `align`.
        // VirtualAlloc(NULL, ...) returns a base aligned to WIN_ALLOCATION_GRANULARITY
        // (64 KiB), so this check often succeeds for `align <= 64 KiB` cases.
        if candidate_ptr.addr().is_multiple_of(align) {
            // Fast-reserve succeeded: use `size` directly, no over-reserve needed.
            // The aligned base equals the region base (no offset).
            (candidate, size)
        } else {
            // Aligned candidate won't work; release it and fall through to the
            // size+align over-reserve path below.
            // SAFETY: `candidate` was just reserved with `MEM_RESERVE` and has not
            // been released yet; releasing before falling back prevents a leak.
            unsafe { winapi_virtual_release(candidate_ptr) };
            // Continue to the over = size + align path.
            let over = size
                .checked_add(align)
                .ok_or_else(VmemError::invalid_argument)?;
            let region = unsafe {
                // SAFETY: same as the reserve call above, for `over` bytes.
                let p = winapi_virtual_reserve(over);
                match NonNull::new(p as *mut u8) {
                    Some(n) => n,
                    None => return Err(VmemError::last_os_error()),
                }
            };
            (region, over)
        }
    } else {
        let over = size
            .checked_add(align)
            .ok_or_else(VmemError::invalid_argument)?;
        let region = unsafe {
            // SAFETY: `VirtualAlloc(NULL, over, MEM_RESERVE, PAGE_READWRITE)`
            // reserves (but does not commit) `over` bytes of address space,
            // returning the base or NULL on OOM/refusal. NULL is checked below.
            let p = winapi_virtual_reserve(over);
            match NonNull::new(p as *mut u8) {
                Some(n) => n,
                // Nothing was reserved; no cleanup needed, so capturing here is
                // already the immediate-capture the task requires.
                None => return Err(VmemError::last_os_error()),
            }
        };
        (region, over)
    };
    let region_ptr = region.as_ptr();
    // task #717: `.addr()` reads the address without exposing provenance
    // (strict-provenance-legal); the paired `.with_addr()` below reconstructs
    // `base` carrying `region_ptr`'s OWN provenance (valid for the whole
    // `over`-byte reservation) at the computed aligned address, instead of
    // the previous `base_addr as *mut u8` cast, which manufactured a pointer
    // with no established provenance at all (contradicted the README's
    // documented "no exposed-address `as usize` round-trips" guarantee).
    let region_addr = region_ptr.addr();
    let fits = align_up_addr(region_addr, align).and_then(|a| {
        let end = a.checked_add(size)?;
        let region_end = region_addr.checked_add(over)?;
        (end <= region_end).then_some(a)
    });
    let base_addr = match fits {
        Some(a) => a,
        None => {
            // Not an OS refusal — an internal fit-computation failure (should
            // not occur given `over = size + align`); do not read errno here.
            // SAFETY: `region` was returned by the `MEM_RESERVE` call above and
            // has not been released yet; releasing before handing to a caller
            // cannot double-free.
            unsafe { winapi_virtual_release(region_ptr) };
            return Err(VmemError::invalid_argument());
        }
    };
    // SAFETY: `base_addr >= region_addr`, within the reserved region, aligned;
    // `region_ptr.with_addr` carries `region_ptr`'s provenance to the new
    // address, so `base` is a valid derived pointer into the live reservation.
    let base = unsafe { NonNull::new_unchecked(region_ptr.with_addr(base_addr)) };
    // SAFETY: `[base_addr, base_addr+commit_len)` is within the just-reserved
    // region (`commit_len <= size`, validated by callers); `MEM_COMMIT` commits
    // exactly this aligned sub-range. NULL indicates commit-charge exhaustion.
    let committed =
        unsafe { VirtualAlloc(base.as_ptr().cast(), commit_len, MEM_COMMIT, PAGE_READWRITE) };
    if committed.is_null() {
        // Capture immediately after the failing commit, before cleanup.
        let err = VmemError::last_os_error();
        // SAFETY: `region` reserved above, not yet handed out — release once.
        unsafe { winapi_virtual_release(region_ptr) };
        return Err(err);
    }
    #[cfg(feature = "bench-internals")]
    WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS.fetch_add(1, Ordering::Relaxed);
    // task #921/V-7: the two-call path never requests MEM_LARGE_PAGES (always plain
    // MEM_COMMIT), so granted_huge is always false here. Only the single-call fast path
    // (align <= GetLargePageMinimum() for large-page requests, align <= WIN_ALLOCATION_GRANULARITY
    // otherwise) can grant huge pages.
    // NOTE: MEM_LARGE_PAGES on a pre-reserved (not pre-committed-with-large-pages) region
    // is empirically always rejected by Windows, so requesting it would be a guaranteed
    // wasted syscall anyway.
    Ok((base, region, over, false))
}

#[cfg(all(windows, not(miri)))]
pub(crate) unsafe fn release_reservation(
    reservation: NonNull<u8>,
    _reservation_len: usize,
    _align: usize,
) {
    // SAFETY: `reservation` is the base of a live `MEM_RESERVE` region — the
    // crate's own reserve path produces one (with an inner aligned sub-range
    // separately committed), and an adopted one is required to be one by
    // `Reservation::from_raw_parts`'s `# Safety` contract. `VirtualFree(.., 0,
    // MEM_RELEASE)` releases the entire MEM_RESERVE region regardless of
    // commit state, so the adopted case's unknown commit state is irrelevant
    // here.
    unsafe { winapi_virtual_release(reservation.as_ptr()) };
}

#[cfg(all(windows, not(miri)))]
// mock (task #646/F8): bypassed by the recording backend, unused when `mock`
// alone is enabled without a real decommit call site reachable.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
pub(crate) unsafe fn decommit_pages_impl(
    base: *mut u8,
    start: usize,
    end: usize,
    _kind: DecommitKind,
) {
    // task #957 (NUM-1): guard the `end - start` subtraction below against an
    // inverted range (caller contract violation) so a debug build panics with
    // an attributable message rather than the subtraction silently wrapping.
    debug_assert!(
        start <= end,
        "decommit_pages_impl: start ({start}) must be <= end ({end})"
    );
    let len = end - start;
    // Windows has no lazy `MADV_FREE` equivalent — both eager and lazy map to
    // `MEM_DECOMMIT`.
    // SAFETY: caller guarantees `[base+start, +len)` is within a MEM_RESERVEd
    // region (not necessarily committed); `MEM_DECOMMIT` returns the physical pages,
    // and decommitting an already-uncommitted sub-range is a defined safe no-op.
    let addr = unsafe { base.add(start) };
    unsafe { winapi_virtual_decommit(addr, len) };
}

#[cfg(all(windows, not(miri)))]
// mock (task #646/F8): see decommit_pages_impl above.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
pub(crate) unsafe fn recommit_pages_impl(
    base: *mut u8,
    start: usize,
    end: usize,
) -> Result<(), VmemError> {
    // task #957 (NUM-1): guard the `end - start` subtraction below against an
    // inverted range (caller contract violation) so a debug build panics with
    // an attributable message rather than the subtraction silently wrapping.
    debug_assert!(
        start <= end,
        "recommit_pages_impl: start ({start}) must be <= end ({end})"
    );
    let len = end - start;
    // SAFETY: caller guarantees `[base+start, +len)` is within a reservation
    // owned by them; `MEM_COMMIT` re-commits the physical pages. NULL indicates
    // commit-charge exhaustion.
    let addr = unsafe { base.add(start) };
    let committed = unsafe {
        VirtualAlloc(
            addr as *mut core::ffi::c_void,
            len,
            MEM_COMMIT,
            PAGE_READWRITE,
        )
    };
    if committed.is_null() {
        Err(VmemError::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(all(windows, not(miri), feature = "lazy-commit"))]
// mock (task #646/F8): see decommit_pages_impl above; `try_commit_range`'s
// real-path branch is compiled out under `mock`, so this never gets called.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
pub(crate) unsafe fn commit_range_impl(
    base: *mut u8,
    start: usize,
    end: usize,
) -> Result<(), VmemError> {
    // Same MEM_COMMIT call as recommit (idempotent on Windows).
    // SAFETY: forwarded from the caller's contract.
    unsafe { recommit_pages_impl(base, start, end) }
}

#[cfg(all(windows, not(miri), feature = "lazy-commit"))]
// mock (task #646/F8): `try_reserve_aligned_lazy`'s real-path branch is
// compiled out under `mock`, so this never gets called.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
pub(crate) fn reserve_aligned_lazy_raw(
    size: usize,
    align: usize,
    initial_commit: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    win_reserve_commit(size, align, initial_commit, 0).map(
        |(base, reservation, reservation_len, _granted_huge)| (base, reservation, reservation_len),
    )
}

#[cfg(all(windows, not(miri), feature = "huge-pages"))]
pub(crate) fn reserve_aligned_huge_raw(
    size: usize,
    align: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize, bool), VmemError> {
    // Windows large pages work via the single-call fast path (task #848):
    // MEM_LARGE_PAGES is issued in a combined MEM_RESERVE | MEM_COMMIT call.
    // The fast-path condition is widened (2026-08-16 audit finding II-3) to
    // attempt the single-call path for any `align` up to GetLargePageMinimum()
    // (typically 2 MiB), not just the 64 KiB WIN_ALLOCATION_GRANULARITY. A
    // granted large-page allocation is naturally aligned to at least the
    // large-page minimum, so it satisfies alignments up to that threshold.
    // The unconditional post-call alignment check guarantees correctness
    // even if large pages are not granted (the allocation then uses ordinary
    // pages, which have the 64 KiB WIN_ALLOCATION_GRANULARITY guarantee).
    //
    // Even when the fast-path condition is satisfied, large-page allocation
    // requires:
    // 1. size is a multiple of the system's large-page minimum
    // 2. The process has SeLockMemoryPrivilege granted AND has enabled it
    //    via AdjustTokenPrivileges (this crate does not do this for you --
    //    granted-but-not-enabled fails exactly like unprivileged)
    // If either fails, the allocation falls back to ordinary pages and
    // granted_huge is false.
    //
    // This widening narrows (but does not eliminate) the platform gap versus
    // Linux's `align >= 2 MiB` requirement: the overlap is now in the 2 MiB
    // neighborhood (where both platforms CAN attempt a huge grant, though
    // Windows still needs privilege to actually succeed), not at 4 MiB
    // (which exceeds GetLargePageMinimum() and can never be huge on Windows).
    win_reserve_commit(size, align, size, MEM_LARGE_PAGES)
}

#[cfg(all(windows, not(miri)))]
extern "system" {
    fn VirtualAlloc(
        lp_address: *mut core::ffi::c_void,
        dw_size: usize,
        fl_allocation_type: u32,
        fl_protect: u32,
    ) -> *mut core::ffi::c_void;
    fn VirtualFree(lp_address: *mut core::ffi::c_void, dw_size: usize, dw_free_type: u32) -> i32;
    pub(crate) fn GetSystemInfo(lp_system_info: *mut SystemInfo);
    fn GetLargePageMinimum() -> usize;
}

/// Mirrors the Windows `SYSTEM_INFO` struct — only `dwPageSize` is read.
///
/// `Default` is all-zeroes (null for the two address fields);
/// `GetSystemInfo` overwrites the fields it defines.
#[cfg(all(windows, not(miri)))]
#[repr(C)]
#[derive(Default)]
pub(crate) struct SystemInfo {
    w_processor_architecture: u16,
    w_reserved: u16,
    pub(crate) dw_page_size: u32,
    lp_minimum_application_address: *mut core::ffi::c_void,
    lp_maximum_application_address: *mut core::ffi::c_void,
    dw_active_processor_mask: usize,
    dw_number_of_processors: u32,
    dw_processor_type: u32,
    pub(crate) dw_allocation_granularity: u32,
    w_processor_level: u16,
    w_processor_revision: u16,
}

#[cfg(all(windows, not(miri)))]
const MEM_COMMIT: u32 = 0x0000_1000;
#[cfg(all(windows, not(miri)))]
const MEM_RESERVE: u32 = 0x0000_2000;
#[cfg(all(windows, not(miri)))]
pub(crate) const WIN_ALLOCATION_GRANULARITY: usize = 65536; // 64 KiB - VirtualAlloc alignment guarantee
#[cfg(all(windows, not(miri)))]
// mock (task #646/F8): only consumed by winapi_virtual_decommit below, which
// itself is unused under `mock`.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
const MEM_DECOMMIT: u32 = 0x0000_4000;
#[cfg(all(windows, not(miri)))]
const MEM_RELEASE: u32 = 0x0000_8000;
#[cfg(all(windows, not(miri), feature = "huge-pages"))]
const MEM_LARGE_PAGES: u32 = 0x2000_0000;
#[cfg(all(windows, not(miri)))]
const PAGE_READWRITE: u32 = 0x04;

#[cfg(all(windows, not(miri)))]
unsafe fn winapi_virtual_reserve(over: usize) -> *mut core::ffi::c_void {
    // SAFETY: `VirtualAlloc` with `MEM_RESERVE` only reserves address space without
    // commit; null base is documented for this usage and safe for any valid size.
    unsafe { VirtualAlloc(core::ptr::null_mut(), over, MEM_RESERVE, PAGE_READWRITE) }
}

#[cfg(all(windows, not(miri)))]
// mock (task #646/F8): see decommit_pages_impl above.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
unsafe fn winapi_virtual_decommit(addr: *mut u8, len: usize) {
    // SAFETY: caller guarantees `[addr, addr+len)` is within a MEM_RESERVEd region;
    // decommitting an already-uncommitted sub-range is a defined safe no-op per the Windows API contract.
    // task #921/V-8: the return value is deliberately discarded. A failure here would
    // indicate a bug in this crate's own bookkeeping (not a recoverable external condition),
    // and the failure mode is a leak, never unsafety. The failure is known to be reachable
    // in practice (e.g. the huge-page decommit case documented in `decommit`'s rustdoc), so
    // this is not a theoretical concern.
    //
    // task P2-6 (2026-08-16 audit finding): increment the failure counter
    // under `bench-internals` so at least diagnostic visibility exists. The
    // counter is gated on the feature and the increment is a single relaxed
    // fetch_add — zero overhead when the feature is off.
    //
    // Finding C-12 (2026-08-16 audit): add an attempts counter mirroring the
    // Unix `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` pair, letting tests
    // distinguish "genuinely succeeded" from "never attempted".
    #[cfg(feature = "bench-internals")]
    WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: `VirtualFree` with `MEM_DECOMMIT` is safe for any address/len within a `MEM_RESERVE`d region;
    // decommitting an already-uncommitted sub-range is a defined safe no-op per the Windows API contract.
    let ret = unsafe { VirtualFree(addr as *mut core::ffi::c_void, len, MEM_DECOMMIT) };
    #[cfg(feature = "bench-internals")]
    if ret == 0 {
        WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    #[cfg(not(feature = "bench-internals"))]
    let _ = ret;
}

#[cfg(all(windows, not(miri)))]
unsafe fn winapi_virtual_release(addr: *mut u8) {
    // SAFETY: caller guarantees `addr` is the base of a `MEM_RESERVE` region;
    // `MEM_RELEASE` + size 0 releases the entire reservation.
    // task #921/V-8: the return value is deliberately discarded. A failure here would
    // indicate a bug in this crate's own bookkeeping (not a recoverable external condition),
    // and the failure mode is a leak, never unsafety (the mapping stays valid, just not
    // returned to the OS).
    //
    // task R4-7 (2026-08-16 audit finding): increment the failure counter
    // under `bench-internals` so at least diagnostic visibility exists. The
    // counter is gated on the feature and the increment is a single relaxed
    // fetch_add — zero overhead when the feature is off.
    //
    // task #1189 (coverage gap C2): also increment the attempts counter
    // BEFORE the syscall, mirroring `winapi_virtual_decommit`'s
    // attempts/failures pairing above and Unix `libc_munmap`'s identical
    // fix. Without this, `WINDOWS_VIRTUALFREE_RELEASE_FAILURES` alone
    // cannot distinguish "release ran and succeeded" from "the call site
    // was removed and never ran" -- both read as zero failures. See
    // `WINDOWS_VIRTUALFREE_RELEASE_ATTEMPTS`'s own doc for the full
    // reasoning.
    #[cfg(feature = "bench-internals")]
    WINDOWS_VIRTUALFREE_RELEASE_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: `VirtualFree` with `MEM_RELEASE` and size 0 is safe for the base of a `MEM_RESERVE` region.
    let ret = unsafe { VirtualFree(addr as *mut core::ffi::c_void, 0, MEM_RELEASE) };
    #[cfg(feature = "bench-internals")]
    if ret == 0 {
        WINDOWS_VIRTUALFREE_RELEASE_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    #[cfg(not(feature = "bench-internals"))]
    let _ = ret;
}
