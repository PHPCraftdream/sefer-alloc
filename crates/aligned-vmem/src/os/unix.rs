use core::ptr::NonNull;
#[cfg(feature = "bench-internals")]
use core::sync::atomic::Ordering;

#[cfg(all(
    feature = "bench-internals",
    any(
        all(
            any(target_os = "linux", target_os = "android"),
            feature = "huge-pages"
        ),
        target_pointer_width = "32"
    )
))]
use crate::bench_internals::{UNIX_EXACT_RESERVE_ATTEMPTS, UNIX_EXACT_RESERVE_HITS};
#[cfg(feature = "bench-internals")]
use crate::bench_internals::{UNIX_MADVISE_ATTEMPTS, UNIX_MADVISE_SUCCESSES, UNIX_MUNMAP_FAILURES};
use crate::error::VmemError;
use crate::os::{align_up_addr, DecommitKind};

#[cfg(all(unix, not(miri)))]
pub(crate) fn reserve_aligned_raw(
    size: usize,
    align: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    unix_reserve(size, align, false).map(|(base, reservation, reservation_len, _granted_huge)| {
        (base, reservation, reservation_len)
    })
}

/// Unix reservation shared by the eager and huge paths. When `huge` is `true`
/// the exact-size fast path and over-reserve fallback both request
/// `MAP_HUGETLB` (Linux/Android) and fall back to ordinary pages if the huge
/// mapping fails.
///
/// Returns `(base, reservation, reservation_len, granted_huge)` where
/// `granted_huge` is `true` iff `huge` was `true` and the huge-page request
/// actually succeeded.
///
/// task #713: every `Err` here carries a [`VmemError`] captured IMMEDIATELY
/// after the syscall that produced it, before any cleanup `munmap` call that
/// could clobber `errno` — a fit-computation failure (not a real OS refusal)
/// maps to [`VmemError::invalid_argument`] rather than a stale/irrelevant
/// error code. The exact-size fast path's own failure cause is discarded
/// (this function always falls through to the over-reserve attempt
/// regardless of why the fast path failed — unchanged control flow from
/// before this task); the final returned error is always the over-reserve
/// attempt's own.
///
/// task #714 (rust-intel audit MEDIUM §F1): on Linux and Android, `huge`
/// requests `MAP_HUGETLB`, and the Linux kernel's `mmap(2)` "Huge TLB
/// mappings" section requires
/// BOTH `munmap(2)`'s `addr` and `length` to be multiples of the huge page
/// size (`man 2 mmap`: "the length ... must also be huge page aligned" for
/// `MAP_HUGETLB`; the kernel additionally guarantees an anonymous
/// `MAP_HUGETLB` mapping with `addr == NULL` starts at a huge-page-aligned
/// address). A non-huge-aligned `size` makes `over = size + align`
/// non-huge-aligned too, so the whole-mapping `munmap` in
/// `release_reservation` would fail `EINVAL`, leaking the ENTIRE mapping
/// (plus its pinned physical huge pages) on every affected reservation AND
/// on every subsequent [`release`].
///
/// REASONED-FROM-SPEC, NOT empirically verified (per this task's own
/// instruction: no hugetlb-configured host is in this project's CI). Fixed
/// by requiring `size` AND `align` to both be multiples of
/// [`LINUX_HUGE_PAGE_SIZE`] before attempting a Linux/Android huge-page
/// reservation
/// at all — with both huge-page-aligned, `over = size + align` is also
/// huge-page-aligned, so the whole-mapping `munmap` calls this function makes
/// (release_reservation unmaps the entire `reservation_len` span) are
/// provably conformant. The head offset `align_up_addr(region_addr, align) -
/// region_addr` is NOT zero in general for `align > LINUX_HUGE_PAGE_SIZE`
/// (e.g. with `align = 4 MiB` and a kernel-guaranteed 2-MiB-aligned region,
/// the offset is 2 MiB), but this no longer matters for correctness because
/// the entire over-reserve mapping is kept and released as a single unit
/// rather than being trimmed with head/tail munmap calls (task #842).
/// A caller that does not supply huge-page-aligned `size`/`align` gets a
/// clean, documented [`VmemError::invalid_argument`] instead of a silent leak.
#[cfg(all(unix, not(miri)))]
fn unix_reserve(
    size: usize,
    align: usize,
    huge: bool,
) -> Result<(NonNull<u8>, NonNull<u8>, usize, bool), VmemError> {
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        feature = "huge-pages"
    ))]
    if huge
        && (!size.is_multiple_of(LINUX_HUGE_PAGE_SIZE)
            || !align.is_multiple_of(LINUX_HUGE_PAGE_SIZE))
    {
        return Err(VmemError::invalid_argument());
    }
    // II-4 (2026-08-16 audit finding): Linux/Android exact-size huge-page fast path
    // for `align == LINUX_HUGE_PAGE_SIZE` (2 MiB). The kernel guarantees an
    // anonymous MAP_HUGETLB mapping with addr == NULL starts at a huge-page-
    // aligned address (see the doc comment above), so an exact-size mmap
    // satisfies the alignment contract with zero over-reserve when
    // `align == LINUX_HUGE_PAGE_SIZE`. This avoids charging `size + align`
    // against the scarce hugetlb pool.
    //
    // This block handles ONLY the genuine-huge-page-granted case and returns
    // early on success; on any failure (mmap refusal, or -- defensively --
    // the kernel's alignment guarantee not holding) it falls through to the
    // general over-reserve path below instead of re-implementing its own
    // ordinary-page fallback. That path already correctly tracks
    // `granted_huge = false` on a huge-to-ordinary fallback (see its own
    // `libc_mmap(over, huge)` retry below) -- duplicating that logic here
    // previously produced a real bug (caught by zero-trust review): an
    // earlier version of this block retried `libc_mmap(size, false)` inline
    // and unconditionally returned `granted_huge = true` even when that
    // ordinary-page retry was the one that actually succeeded, which is
    // both a false `is_huge()` claim (the same class of bug task #943/W-1
    // fixed for Windows) and paired with an alignment guarantee that does
    // NOT hold for a plain (non-`MAP_HUGETLB`) mapping.
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        feature = "huge-pages"
    ))]
    if huge && align == LINUX_HUGE_PAGE_SIZE {
        // Track exact-size attempt (bench-internals oracle for II-4 fast path).
        #[cfg(feature = "bench-internals")]
        UNIX_EXACT_RESERVE_ATTEMPTS.fetch_add(1, Ordering::Relaxed);

        // SAFETY: anonymous private MAP_HUGETLB mapping of exactly `size`
        // bytes. The error cause is captured inside `libc_mmap` at the
        // failing syscall; this fast path discards it — any failure falls
        // through to the general over-reserve path below, which performs its
        // own mmap and error reporting.
        if let Ok(p) = unsafe { libc_mmap(size, true) } {
            let region_addr = p.addr();
            // Real runtime check (not debug_assert!): the kernel's huge-page-
            // alignment guarantee is unverified behavior this crate is
            // trusting, not self-evident arithmetic -- release builds are
            // exactly where an unverified assumption matters (same reasoning
            // as the Windows H2C6 / Unix U1 checks elsewhere in this file).
            if region_addr.is_multiple_of(align) {
                // Track exact-size hit (bench-internals oracle for II-4 fast path).
                #[cfg(feature = "bench-internals")]
                UNIX_EXACT_RESERVE_HITS.fetch_add(1, Ordering::Relaxed);

                // SAFETY: non-null and just verified aligned.
                let base = unsafe { NonNull::new_unchecked(p as *mut u8) };
                return Ok((base, base, size, true));
            }
            // Kernel's alignment guarantee did not hold (should not happen in
            // practice); release this mapping and fall through below.
            // SAFETY: `p` was returned by `mmap` above and not yet handed to
            // a caller; releasing before falling through prevents a leak.
            unsafe { libc_munmap(p.cast(), size) };
        }
        // Huge mmap failed, or its alignment guarantee didn't hold: fall
        // through to the general over-reserve path below.
    }
    // 32-bit only: try exact-size mmap first for address-space economy.
    // On 64-bit this is a net syscall loss (the fast path costs 1 syscall on a
    // hit, 3 on a miss, vs a flat 1 syscall for the over-reserve path), and
    // address-space economy is not a concern on 64-bit.
    //
    // R3-1/R4-2 fix: skip 32-bit generic exact path when huge-page exact-size fast path
    // was already tried above (the `huge && align == LINUX_HUGE_PAGE_SIZE`
    // fast-path block at the top of this function — named, not line-numbered,
    // per the task #908/V2C1 convention). Without this check, a 32-bit host
    // with hugetlb pool == 0 would call `try_reserve_aligned_exact(size, align, huge=true)`
    // after the specialized huge-exact path just failed with the same MAP_HUGETLB call,
    // causing `UNIX_EXACT_RESERVE_ATTEMPTS` to be incremented twice for one logical reserve.
    #[cfg(target_pointer_width = "32")]
    {
        #[cfg(all(
            any(target_os = "linux", target_os = "android"),
            feature = "huge-pages"
        ))]
        let huge_exact_already_tried = huge && align == LINUX_HUGE_PAGE_SIZE;
        #[cfg(not(all(
            any(target_os = "linux", target_os = "android"),
            feature = "huge-pages"
        )))]
        let huge_exact_already_tried = false;

        if !huge_exact_already_tried {
            if let Ok((base, reservation, reservation_len, granted_huge)) =
                try_reserve_aligned_exact(size, align, huge)
            {
                return Ok((base, reservation, reservation_len, granted_huge));
            }
        }
    }
    let over = size
        .checked_add(align)
        .ok_or_else(VmemError::invalid_argument)?;
    // Track whether huge pages were actually granted; assigned in each branch
    // below (a bare pointer, not a tuple, must be the unsafe block's own tail
    // expression -- `region_ptr` is used as a raw pointer immediately after).
    let granted_huge;
    let region_ptr = unsafe {
        // SAFETY: `mmap(NULL, over, RW, PRIVATE|ANON, -1, 0)` — anonymous
        // private mapping; the kernel chooses the address or returns
        // MAP_FAILED (mapped to `Err` by `libc_mmap`, which captures the
        // cause at the failing syscall itself — task #713's immediate-capture
        // discipline, enforced inside `libc_mmap` since task #1068/F2, so no
        // call site can capture after an intervening syscall).
        match libc_mmap(over, huge) {
            Ok(p) => {
                granted_huge = HUGE_SUPPORTED && huge; // Huge pages requested and actually supported
                p
            }
            Err(e) => {
                // Retry without huge pages if the huge request was the cause.
                if !huge {
                    return Err(e);
                }
                // Set before the retry: if the retry refuses, the `?` below
                // returns from this function and `granted_huge` is dead; if
                // it succeeds, the reservation fell back from the requested
                // huge pages to ordinary ones.
                granted_huge = false;
                // SAFETY: same call, ordinary pages. The `?` is pure
                // propagation: the retry's own refusal is the proximate cause
                // of the reservation failure, so its error (captured inside
                // `libc_mmap` at the failing syscall) is the one reported —
                // unchanged from the pre-#1068 behavior, which read the
                // retry's errno last.
                libc_mmap(over, false)?
            }
        }
    };
    // task #717: `.addr()`/`.with_addr()` (strict-provenance) replace the
    // previous `as usize` / `as *mut u8` round-trip — see win_reserve_commit's
    // matching comment above for the full reasoning.
    let region_addr = region_ptr.addr();
    let fits = align_up_addr(region_addr, align).and_then(|a| {
        let tail_start = a.checked_add(size)?;
        let region_end = region_addr.checked_add(over)?;
        (tail_start <= region_end).then_some(a)
    });
    let base_addr = match fits {
        Some(a) => a,
        None => {
            // Not an OS refusal — an internal fit-computation failure (should
            // not occur given `over = size + align`); do not read errno here.
            // SAFETY: `region_ptr` was returned by `mmap` above; releasing the
            // whole `over`-byte mapping before handing to a caller is sound.
            unsafe { libc_munmap(region_ptr.cast(), over) };
            return Err(VmemError::invalid_argument());
        }
    };
    // Symmetry with `try_reserve_aligned_exact`'s unconditional alignment check:
    // that check is a runtime check (not debug_assert) because it guards against
    // an unverified `_SC_PAGESIZE` constant producing a misaligned base on BSDs.
    // Here, the arithmetic `align_up_addr(region_addr, align)` is self-evidently
    // correct with no unverified-constant dependency, so a debug_assert is sufficient.
    debug_assert!(
        base_addr.is_multiple_of(align),
        "over-reserve base_addr must be align-aligned"
    );
    // SAFETY: `base_addr >= region_addr` and `align`-aligned; `with_addr`
    // carries `region_ptr`'s provenance (valid for the whole `over`-byte
    // mapping) to the computed address.
    let base = unsafe { NonNull::new_unchecked(region_ptr.with_addr(base_addr).cast::<u8>()) };
    // Keep the entire over-reserve mapping as the reservation, exactly as
    // the Windows backend does. This removes the `munmap` trim calls and
    // makes V1's alignment bug structurally impossible: there is exactly one
    // `munmap` at `region_ptr` (provably page-aligned by `mmap`'s contract)
    // instead of two at potentially misaligned offsets. Cost: up to `align`
    // bytes of untouched VA held for the reservation's lifetime (no RSS).
    //
    // task #1069 (audit F9 half 1 — documented, deliberately NOT changed):
    // that "(no RSS)" price is the ORDINARY-pages case only. When a huge
    // request is GRANTED through this tail (every granted `align >
    // LINUX_HUGE_PAGE_SIZE` request — the II-4 fast path above covers only
    // `align == LINUX_HUGE_PAGE_SIZE` — plus an `align == 2 MiB` request
    // whose exact-size attempt missed), the cost is not merely VA:
    // `libc_mmap` passes no `MAP_NORESERVE`, and the Linux kernel reserves hugetlb
    // pool pages for a private `MAP_HUGETLB` mapping's entire length at
    // mmap time, so the whole `over`-byte span — including the slack — is
    // charged against the bounded `nr_hugepages` pool for the
    // reservation's lifetime. The slack (head + tail combined) is exactly
    // `align` bytes regardless of where the kernel placed the region
    // (`over - size`), so a `size == align == 4 MiB` request charges
    // 4 pool pages per segment for 2 needed — 2×, II-4's own "up to 2x"
    // arithmetic (the F9 audit's "roughly 33% pool overhead" headline does
    // not match: the slack is 50% of the charge in that shape).
    // REASONED-FROM-SPEC (documented kernel hugetlb reservation semantics;
    // no hugetlb-configured host in this project's CI to verify or measure
    // on — docs/CORRECTNESS_OPEN_ITEMS.md item 59). A head/tail trim WOULD
    // be munmap-conformant in this specific case (the guard at the top of
    // this function forces `size`/`align` to 2 MiB multiples and the
    // kernel guarantees a 2 MiB-aligned base, so every trim boundary is
    // huge-page-aligned), but re-adding trims would partially reverse task
    // #842's deliberate one-munmap soundness design for a win that cannot
    // be measured on any host available to this project — recorded with
    // that revisit trigger in docs/perf/OPEN_ITEMS.md item 48 instead.
    Ok((
        base,
        // SAFETY: `region_ptr` is confirmed non-null above (both the `p` and
        // `p2` paths null-check before reaching here); this cast preserves
        // `region_ptr`'s own provenance (valid for the whole `over`-byte
        // mapping), unlike `base` above which is `with_addr`'d to a
        // sub-range of it.
        unsafe { NonNull::new_unchecked(region_ptr.cast::<u8>()) },
        over,
        granted_huge,
    ))
}

/// 1-syscall exact-size mmap fast path. `huge` requests
/// `MAP_HUGETLB`.
///
/// Returns `(base, reservation, reservation_len, granted_huge)` where
/// `granted_huge` is `true` iff `huge` was `true` and the mapping succeeded.
///
/// task #713 (capture enforced inside `libc_mmap` since task #1068/F2): a
/// genuine `mmap` failure captures its [`VmemError`]
/// IMMEDIATELY, before returning. An alignment miss (the exact address `mmap`
/// handed back doesn't satisfy `align`) is NOT an OS refusal — it maps to
/// [`VmemError::invalid_argument`] instead of a stale/irrelevant error code.
/// Either way, [`unix_reserve`] discards this function's own error and always
/// falls through to the over-reserve path on any failure (unchanged control
/// flow from before this task) — the error value here is not observable by
/// any public caller today, but is still captured correctly for future
/// callers / diagnostics.
///
/// NOTE: This function is only used on 32-bit targets for address-space
/// economy. On 64-bit, `unix_reserve` over-reserves (1 syscall) for every
/// ordinary request — the 32-bit-only exact-size fast path is a net syscall
/// loss at all hit rates — with ONE exception: the exact-size `MAP_HUGETLB`
/// attempt inside `unix_reserve` itself (Linux AND Android, `huge-pages`
/// feature, `align == LINUX_HUGE_PAGE_SIZE`) is NOT gated on pointer width,
/// so it still fires on 64-bit. See `api/reserve.rs` and the crate-level
/// docs for that documented exception. (Task #1081/F10a corrected the
/// earlier "always" wording, which contradicted them.)
#[cfg(all(unix, not(miri), target_pointer_width = "32"))]
fn try_reserve_aligned_exact(
    size: usize,
    align: usize,
    huge: bool,
) -> Result<(NonNull<u8>, NonNull<u8>, usize, bool), VmemError> {
    #[cfg(feature = "bench-internals")]
    UNIX_EXACT_RESERVE_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: anonymous private mapping of exactly `size` bytes. The error
    // cause is captured inside `libc_mmap` at the failing syscall (task #713
    // immediate-capture, enforced there since task #1068/F2); the `?` below
    // is pure propagation of that already-captured cause.
    let region_ptr = unsafe { libc_mmap(size, huge) }?;
    // task #776 (F8): `.addr()` reads the address without exposing
    // provenance, completing the strict-provenance discipline task #717
    // applied to `unix_reserve`'s slow path -- this is the Unix FAST path
    // (tried first by every reservation), so it is the higher-traffic site.
    let region_addr = region_ptr.addr();
    // task #897 (finding U1): this check is UNCONDITIONAL -- it used to be
    // guarded by `align > page_size() &&`, reasoned as follows: when
    // `align <= page_size()`, `mmap` always returns page-aligned addresses,
    // so the check below is provably false and can be skipped. That
    // reasoning is correct only if `page_size()` is <= the REAL OS page
    // size. `page_size()` accepts any power-of-two value >= `PAGE` read
    // from `sysconf(_SC_PAGESIZE)` (see its own guard comment above,
    // "a plausible-looking power-of-two answer to a DIFFERENT question") --
    // a wrong `_SC_PAGESIZE` constant that happens to return a power-of-two
    // ABOVE the real page size would be silently accepted and cached
    // process-wide (docs/CORRECTNESS_OPEN_ITEMS.md item 43's still-open BSD
    // half: FreeBSD/DragonFly/NetBSD/OpenBSD constants are REASONED-FROM-SPEC,
    // not hardware-verified). If that ever happens, every `align` in
    // `(real_page_size, page_size()]` would silently skip this check, and
    // `try_reserve_aligned_exact` could return a base that is NOT aligned to
    // `align` -- violating `Reservation::as_ptr()`'s documented alignment
    // guarantee with no error and no diagnostic. The `align > page_size()`
    // conjunct measured zero syscalls saved (measurement #849: 480/480 hits
    // in page-size mode, i.e. it only ever removed a dead branch, never
    // avoided real work), so there is no performance cost to always running
    // the check. Deliberately a real runtime check, not a `debug_assert!`:
    // release builds are exactly where an unverified `_SC_PAGESIZE` constant
    // would matter (CLAUDE.md's R26-4 rule: `debug_assert!` compiles out of
    // `--release`).
    if !region_addr.is_multiple_of(align) {
        // SAFETY: `region_ptr` was just mapped with length `size`; unmap once.
        unsafe { libc_munmap(region_ptr.cast(), size) };
        return Err(VmemError::invalid_argument());
    }
    #[cfg(feature = "bench-internals")]
    UNIX_EXACT_RESERVE_HITS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: non-null and proven `align`-aligned.
    let base = unsafe { NonNull::new_unchecked(region_ptr as *mut u8) };
    // `granted_huge` reflects what was actually requested AND what the OS supports.
    // On Unix that is neither Linux nor Android the `huge` flag is silently ignored, so we report false.
    // This is correct because `MAP_HUGETLB` fails the WHOLE `mmap` call when
    // 2 MiB hugetlb pages are unavailable (an all-or-nothing kernel behavior),
    // so "the caller asked for huge and mmap succeeded" implies a grant.
    Ok((base, base, size, HUGE_SUPPORTED && huge))
}

#[cfg(all(unix, not(miri)))]
pub(crate) unsafe fn release_reservation(
    reservation: NonNull<u8>,
    reservation_len: usize,
    _align: usize,
) {
    // SAFETY: on unix `reservation` is the start of the remaining mapping of length
    // `reservation_len`; `libc_munmap` requires the address/len to be a live mmap'd
    // region being unmapped once, which this call satisfies.
    unsafe { libc_munmap(reservation.as_ptr(), reservation_len) };
}

#[cfg(all(unix, not(miri)))]
// mock (task #646/F8): bypassed by the recording backend, unused when `mock`
// alone is enabled without a real decommit call site reachable.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
pub(crate) unsafe fn decommit_pages_impl(
    base: *mut u8,
    start: usize,
    end: usize,
    kind: DecommitKind,
) {
    // task #957 (NUM-1): guard the `end - start` subtraction below against an
    // inverted range (caller contract violation) so a debug build panics with
    // an attributable message rather than the subtraction silently wrapping.
    debug_assert!(
        start <= end,
        "decommit_pages_impl: start ({start}) must be <= end ({end})"
    );
    let len = end - start;
    // task #719: missing SAFETY comment (the Windows sibling has one for the
    // identical `base.add(start)` offset above its own commit call).
    // SAFETY: caller guarantees `[base+start, +len)` is within a live
    // reservation owned by them, so the offset stays in-bounds.
    let addr = unsafe { base.add(start) };
    match kind {
        // SAFETY: caller guarantees `[base+start, +len)` is within a live
        // mapping; `madvise` touches only kernel page-state.
        DecommitKind::Eager => unsafe { libc_madvise(addr, len, MADV_DONTNEED) },
        DecommitKind::Lazy => unsafe { libc_madvise(addr, len, madv_free_advice()) },
    }
}

#[cfg(all(unix, not(miri)))]
// mock (task #646/F8): see decommit_pages_impl above.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
pub(crate) unsafe fn recommit_pages_impl(
    _base: *mut u8,
    _start: usize,
    _end: usize,
) -> Result<(), VmemError> {
    // On Linux and Android, re-access after MADV_DONTNEED is implicit — the
    // kernel actually unmaps the physical pages, so the next write re-faults
    // a fresh zero page. No syscall, cannot fail, on Linux and Android.
    //
    // CAVEAT (confirmed as a real, failing-test-level gap by this crate's
    // first real-macOS CI run, 2026-08-13 -- the underlying hazard was
    // already known repo-wide since Round 9, see
    // docs/CORRECTNESS_OPEN_ITEMS.md item 48): this does
    // NOT hold on the Darwin family (macOS/iOS/tvOS/watchOS). `madvise(MADV_DONTNEED)`
    // on Darwin is advisory only for anonymous memory and does not reliably
    // unmap/zero the pages, so a `decommit` + `recommit` roundtrip on Darwin
    // can observe the OLD data still resident — `decommit`'s "return physical
    // backing to the OS" promise is silently unmet on Darwin, the same shape
    // as the already-documented huge-page no-op above `decommit`'s own doc
    // comment. No fix implemented here (a real fix needs re-`mmap`(MAP_FIXED)
    // over the range on Darwin, a bigger change deserving its own review round); this
    // comment and the test scoping below are the honest interim state.
    Ok(())
}

#[cfg(all(unix, not(miri), feature = "lazy-commit"))]
// mock (task #646/F8): `try_commit_range`'s real-path branch is compiled out
// under `mock`, so this never gets called.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
pub(crate) unsafe fn commit_range_impl(
    _base: *mut u8,
    _start: usize,
    _end: usize,
) -> Result<(), VmemError> {
    // Unix: pages are already accessible (eager mmap). Always succeeds.
    Ok(())
}

#[cfg(all(unix, not(miri), feature = "lazy-commit"))]
// mock (task #646/F8): `try_reserve_aligned_lazy`'s real-path branch is
// compiled out under `mock`, so this never gets called.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
pub(crate) fn reserve_aligned_lazy_raw(
    size: usize,
    align: usize,
    _initial_commit: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize), VmemError> {
    reserve_aligned_raw(size, align)
}

#[cfg(all(unix, not(miri), feature = "huge-pages"))]
pub(crate) fn reserve_aligned_huge_raw(
    size: usize,
    align: usize,
) -> Result<(NonNull<u8>, NonNull<u8>, usize, bool), VmemError> {
    unix_reserve(size, align, true)
}

/// Select the lazy-decommit `madvise` advice for this platform.
/// Linux/Android: `MADV_FREE`; macOS/iOS: `MADV_FREE_REUSABLE`; FreeBSD/DragonFly:
/// `MADV_FREE` (5); NetBSD/OpenBSD: `MADV_FREE` (6). tvOS/watchOS are routed
/// to the `MADV_DONTNEED` fallback below (the `cfg` arms below match only
/// `any(target_os = "macos", target_os = "ios")`, not tvOS/watchOS — see
/// [`decommit_lazy`]'s own public doc, which documents this fallback as
/// current behavior). `MADV_FREE_REUSABLE` would be a plausible future
/// widening for tvOS/watchOS (same XNU kernel as macOS/iOS, so the numeric
/// advice value is shared), not current behavior — REASONED-FROM-SPEC only,
/// not verified on those targets, and not what this function actually does
/// today. REASONED-FROM-SPEC for all BSD constants — this crate has no BSD
/// CI runner to empirically verify on; values are from each OS's own
/// `sys/mman.h` headers (FreeBSD/DragonFly: 5, NetBSD/OpenBSD: 6).
#[cfg(all(unix, not(miri)))]
// mock (task #646/F8): only caller is decommit_pages_impl above, itself
// unused under `mock`.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
#[inline]
fn madv_free_advice() -> i32 {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        MADV_FREE
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        MADV_FREE_REUSABLE
    }
    #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
    {
        MADV_FREE_BSD_5
    }
    #[cfg(any(target_os = "netbsd", target_os = "openbsd"))]
    {
        MADV_FREE_BSD_6
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
    )))]
    {
        MADV_DONTNEED
    }
}

#[cfg(all(unix, not(miri)))]
const PROT_READ: i32 = 0x1;
#[cfg(all(unix, not(miri)))]
const PROT_WRITE: i32 = 0x2;
#[cfg(all(unix, not(miri)))]
const MAP_PRIVATE: i32 = 0x02;
// task #893 (T6, round-7 review): `MAP_ANON`/`MAP_HUGETLB` below are gated on
// `target_os = "linux"` or `target_os = "android"`, but their numeric VALUES are `target_arch`-
// dependent, not just OS-dependent — the same class of non-portability the
// `_SC_PAGESIZE` table below (task #714) documents at length. `0x20` /
// `0x40000` are `asm-generic/mman-common.h`'s values, correct on every
// mainstream Linux architecture this crate targets (x86, x86_64, aarch64,
// arm, riscv, powerpc — every Linux architecture that uses
// `asm-generic/mman-common.h`, which is all of them except MIPS, Alpha,
// PA-RISC and Xtensa; NOT an exhaustive tier-1/tier-2 roster — s390x and
// loongarch64 are tier-2 and also use `asm-generic/mman-common.h`, just not
// named in this parenthetical -- round 7, task #895/TC9). They are WRONG on MIPS:
// `arch/mips/include/uapi/asm/mman.h` defines `MAP_ANONYMOUS = 0x0800` and
// `MAP_HUGETLB = 0x80000`; `0x20` on MIPS is actually the IRIX-compat
// `MAP_RENAME`, which Linux ignores — so on `mips*-unknown-linux-*` (tier 3;
// no such target is installed here or in this project's CI) `libc_mmap`
// would issue `mmap(..., MAP_PRIVATE, -1, 0)` with no anonymous flag set,
// `fd = -1` causes `EBADF`, `mmap` returns `MAP_FAILED`, and every
// `reserve_aligned` call fails closed with no diagnostic pointing at the
// wrong constant (Alpha, PA-RISC and Xtensa diverge similarly, but none of
// those is a current Rust target at all). Android's bionic libc runs on the
// Linux kernel, so the Linux values apply directly. REASONED-FROM-SPEC, not executed —
// no MIPS target is available to verify this against real hardware.
#[cfg(all(unix, not(miri), any(target_os = "linux", target_os = "android")))]
const MAP_ANON: i32 = 0x20;
#[cfg(all(
    unix,
    not(miri),
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly",
    )
))]
const MAP_ANON: i32 = 0x1000;

/// task #918 (finding H2C7): Compile-time error for Unix targets without a
/// `MAP_ANON` definition. Several real `cfg(unix)` targets (e.g. illumos
/// `x86_64-unknown-illumos`, Solaris `x86_64-pc-solaris`) set `unix` but do
/// NOT match either of the two `MAP_ANON` cfg arms above (Linux/Android or
/// Darwin/BSD). (Android's `aarch64-linux-android` was on this example list
/// before task #944/U-2 wired it into the Linux/Android arm — it matches that
/// arm now, so it is no longer an example.) Without this guard,
/// `libc_mmap` fails with a bare `error[E0425]: cannot find value MAP_ANON
/// in this scope` — fails closed (no unsoundness), but with an unattributable
/// compiler error rather than a clear diagnostic naming the actual reason
/// (unsupported target) and pointing at how to add support.
///
/// Adding support for a new Unix target requires:
/// 1. Confirming the target's `MAP_ANON` (or `MAP_ANONYMOUS`) constant value
///    from its libc headers or OS documentation.
/// 2. Adding a new `#[cfg(...)]` arm for that `target_os` with the correct
///    constant value, following the pattern of the two existing arms above.
#[cfg(all(
    unix,
    not(miri),
    not(any(target_os = "linux", target_os = "android")),
    not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly",
    ))
))]
compile_error!(
    "aligned-vmem does not currently support this Unix target because its \
     `MAP_ANON` constant is not defined. Adding support requires confirming \
     the target's `MAP_ANON` (or `MAP_ANONYMOUS`) value and adding a new \
     `#[cfg(...)]` arm for this `target_os` following the pattern in the file. \
     See the comment above this compile_error! for details."
);

// task #1017 (finding R4-1): Compile-time error for MIPS targets.
//
// MIPS (both `mips` and `mips64`) uses different `MAP_ANON`/`MAP_HUGETLB`
// constant values than the `asm-generic/mman-common.h` values this crate
// hardcodes for Linux: MIPS defines `MAP_ANONYMOUS = 0x0800` and
// `MAP_HUGETLB = 0x80000`, while this crate uses `0x20` and `0x40000`
// respectively (the `asm-generic` values). With the wrong constants,
// every `reserve_aligned` call fails closed at runtime with `EBADF`
// (invalid file descriptor) because `libc_mmap` issues `mmap(..., MAP_PRIVATE, -1, 0)`
// with no anonymous flag properly set, but the failure is silent (no diagnostic
// points to the constant error). Rather than compile a buildable-but-broken crate,
// we fail compilation with a clear diagnostic. This is a release decision:
// adding support requires adding a `#[cfg(any(target_arch = "mips", target_arch = "mips64"))]`
// arm with the correct MIPS-specific constant values. See `docs/CORRECTNESS_OPEN_ITEMS.md`.
#[cfg(all(unix, not(miri), any(target_arch = "mips", target_arch = "mips64")))]
compile_error!(
    "aligned-vmem does not support MIPS: MAP_ANON/MAP_HUGETLB constant values \
     differ from the values this crate hardcodes, causing every reservation to \
     fail with EBADF at runtime with no diagnostic. See docs/CORRECTNESS_OPEN_ITEMS.md \
     for the release decision record."
);

/// Linux/Android `MAP_HUGETLB` (request huge pages at mmap time).
///
/// Same architecture caveat as `MAP_ANON` above: `0x40000` is correct on
/// x86/x86_64/aarch64/arm/riscv/powerpc, wrong on MIPS (`0x80000` per
/// `arch/mips/include/uapi/asm/mman.h`) — see the note above `MAP_ANON` for
/// the full rationale and failure mode. Android's bionic libc runs on the
/// Linux kernel, so the Linux value applies directly; Android is covered by
/// the same cfg arm (`any(target_os = "linux", target_os = "android")`).
#[cfg(all(
    unix,
    not(miri),
    any(target_os = "linux", target_os = "android"),
    feature = "huge-pages"
))]
const MAP_HUGETLB: i32 = 0x40000;

/// Linux/Android `MAP_HUGE_2MB` (request 2 MiB huge pages at mmap time).
///
/// This flag explicitly requests the 2 MiB huge page size, overriding the
/// system's configured default huge-page size (set via the kernel boot
/// parameter `default_hugepagesz=`). Without this flag, plain `MAP_HUGETLB`
/// requests the system's default huge page size, which can be 1 GiB on
/// HPC/database-tuned hosts — a mismatch that would cause a silent leak
/// because this crate's `LINUX_HUGE_PAGE_SIZE` constant and validation logic
/// assume 2 MiB. The value is taken from the Linux kernel's
/// `include/uapi/linux/mman.h`: `MAP_HUGE_2MB = (21 << MAP_HUGE_SHIFT)` where
/// `MAP_HUGE_SHIFT = 26`. (REASONED-FROM-SPEC: this fix has NOT been empirically
/// verified on a real host with a non-2-MiB default `default_hugepagesz`, because
/// no such host exists in this project's CI.)
///
/// Note: The `MAP_HUGE_*` size encoding (the bits above `MAP_HUGE_SHIFT`)
/// was introduced in Linux 3.8 (2013); on older kernels these bits are not
/// interpreted by the `MAP_HUGETLB` path and the system's default huge-page size
/// is used instead. Android's bionic libc runs on the Linux kernel, so the
/// Linux value applies directly; Android is covered by the same cfg arm
/// (`any(target_os = "linux", target_os = "android")`).
#[cfg(all(
    unix,
    not(miri),
    any(target_os = "linux", target_os = "android"),
    feature = "huge-pages"
))]
const MAP_HUGE_2MB: i32 = 21 << 26;

/// task #852 (W2): `HUGE_SUPPORTED` is true only on Linux or Android with the
/// `huge-pages` feature enabled. Unix that is neither Linux nor Android
/// (macOS, iOS, BSD, etc.) does NOT
/// support `MAP_HUGETLB` — the `libc_mmap` function silently ignores the `huge`
/// parameter on those platforms. This constant is used to ensure `granted_huge`
/// reports the ACTUAL grant, not just the request. Android's bionic libc runs on
/// the Linux kernel, so Android is covered by the same cfg arm
/// (`any(target_os = "linux", target_os = "android")`)
/// and gets huge-page support when the feature is enabled.
#[cfg(all(
    unix,
    not(miri),
    any(target_os = "linux", target_os = "android"),
    feature = "huge-pages"
))]
const HUGE_SUPPORTED: bool = true;
#[cfg(all(
    unix,
    not(miri),
    not(all(
        any(target_os = "linux", target_os = "android"),
        feature = "huge-pages"
    ))
))]
const HUGE_SUPPORTED: bool = false;

/// task #909: the huge-page size (2 MiB, named `LINUX_HUGE_PAGE_SIZE` after
/// its origin) this crate explicitly requests via
/// `MAP_HUGE_2MB` (not the system's configured default). Before this fix,
/// the crate used plain `MAP_HUGETLB` and relied on a now-falsified premise
/// that "the default is always 2 MiB on mainstream x86_64/aarch64 Linux" —
/// that premise was proved wrong by task #909's independent review finding H1,
/// which showed the default is independently configurable via the kernel boot
/// parameter `default_hugepagesz=` and can be 1 GiB on HPC/database-tuned hosts.
/// The fix (task #909) makes the crate explicitly request 2 MiB huge pages via
/// `MAP_HUGE_2MB`, making this constant correct by construction — if 2 MiB
/// huge pages are not configured/available on the host, the mmap call fails
/// cleanly (returns null → the crate's normal OOM/error path), rather than
/// silently succeeding with a mismatched size and leaking on munmap. `unix_reserve`
/// requires `size`/`align` to be multiples of this before attempting a huge-page
/// reservation at all, so every `munmap` it can still reach is provably
/// huge-page-aligned (see `unix_reserve`'s own doc for the full reasoning) —
/// REASONED-FROM-SPEC, not empirically verified on a real hugetlb-configured host
/// with a non-2-MiB default `default_hugepagesz` (none is in this project's CI).
/// Android's bionic libc runs on the Linux kernel, so the Linux value applies
/// directly; Android is covered by the same cfg arm
/// (`any(target_os = "linux", target_os = "android")`).
#[cfg(all(
    unix,
    not(miri),
    any(target_os = "linux", target_os = "android"),
    feature = "huge-pages"
))]
const LINUX_HUGE_PAGE_SIZE: usize = 2 * 1024 * 1024;
#[cfg(all(unix, not(miri)))]
const MAP_FAILED: usize = usize::MAX;
#[cfg(all(unix, not(miri)))]
// mock (task #646/F8): only consumed by decommit_pages_impl / madv_free_advice
// above, both unused under `mock`.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
const MADV_DONTNEED: i32 = 4;
/// Linux/Android `MADV_FREE` (lazy reclaim under pressure).
#[cfg(all(unix, not(miri), any(target_os = "linux", target_os = "android")))]
// mock (task #646/F8): see MADV_DONTNEED above.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
const MADV_FREE: i32 = 8;
/// macOS/iOS `MADV_FREE_REUSABLE` (lazy reclaim; page reusable).
#[cfg(all(unix, not(miri), any(target_os = "macos", target_os = "ios")))]
const MADV_FREE_REUSABLE: i32 = 7;
/// FreeBSD/DragonFly `MADV_FREE` (lazy reclaim; advisory).
/// REASONED-FROM-SPEC — no BSD CI runner to empirically verify on; value from
/// `sys/mman.h` (both OSes use 5).
#[cfg(all(unix, not(miri), any(target_os = "freebsd", target_os = "dragonfly")))]
// mock (task #646/F8): see MADV_DONTNEED above.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
const MADV_FREE_BSD_5: i32 = 5;
/// NetBSD/OpenBSD `MADV_FREE` (lazy reclaim; advisory).
/// REASONED-FROM-SPEC — no BSD CI runner to empirically verify on; value from
/// `sys/mman.h` (both OSes use 6).
#[cfg(all(unix, not(miri), any(target_os = "netbsd", target_os = "openbsd")))]
// mock (task #646/F8): see MADV_DONTNEED above.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
const MADV_FREE_BSD_6: i32 = 6;
// task #714 (rust-intel audit MEDIUM §F1): `_SC_PAGESIZE`'s numeric value is
// NOT portable across the `unix` family — it is an index into each OS's own
// `sysconf(3)` name table, not a POSIX-wide constant. The prior version of
// this constant used the Darwin value (29) for macOS/iOS and the Linux value
// (30) for every OTHER `unix` target, INCLUDING all four BSDs this crate's
// own `MAP_ANON` cfg list above already supports — on FreeBSD/DragonFly/
// NetBSD/OpenBSD, `sysconf(30)` queries an unrelated `sysconf` parameter, not
// the page size, and if that unrelated value happened to be a power of two
// it would silently pass `page_size()`'s validation guard and poison the
// decommit-offset rounding callers are told to base on `page_size()`
// (see `UNIX_EXACT_RESERVE_HITS`'s own doc comment above for the counter
// this hazard would have silently corrupted).
//
// REASONED-FROM-SPEC, NOT empirically verified (per this task's own
// instruction: none of the four BSDs run in this project's CI) — values
// cited from each OS's own `sys/unistd.h` `_SC_*` name table:
// - FreeBSD `sys/unistd.h`: `_SC_PAGESIZE` = 47 (`_SC_PAGE_SIZE` is a
//   `#define` alias for the same value). DragonFly BSD forked from FreeBSD
//   and has NOT renumbered this table; DragonFly's own `sys/unistd.h`
//   confirms the same 47.
// - NetBSD `sys/unistd.h`: `_SC_PAGESIZE` = 28 (`_SC_PAGE_SIZE` aliases it).
// - OpenBSD `sys/unistd.h`: `_SC_PAGESIZE` = 28 (same table position as
//   NetBSD; OpenBSD forked from NetBSD's 4.4BSD-derived `unistd.h`).
//
// task #951 (independent review finding 2.1): task #944/U-2 wired Android
// (bionic libc) into `MAP_ANON`/`MAP_HUGETLB`/`HUGE_SUPPORTED`/
// `LINUX_HUGE_PAGE_SIZE`/`MADV_FREE`/`libc_mmap`/`unix_reserve`'s huge
// guard, but missed this table — Android fell through to the `not(any(...))` fallback below and
// silently got the glibc value 30. Bionic does NOT share glibc's
// `confname.h` numbering (same portability hazard the BSD comment above
// already documents for a different OS family). EMPIRICALLY VERIFIED for
// this task (not merely reasoned-from-spec, unlike the BSD entries above,
// none of which run in this project's CI): fetched
// https://android.googlesource.com/platform/bionic/+/refs/heads/main/libc/include/bits/sysconf.h
// directly (AOSP `main` branch, `platform/bionic` repo) on 2026-08-15,
// which defines `#define _SC_PAGESIZE 0x0027` (= 39) and
// `#define _SC_PAGE_SIZE 0x0028` (= 40 — a DIFFERENT slot from
// `_SC_PAGESIZE` on bionic, unlike every other OS in this table where the
// two names alias the same value; callers must use `_SC_PAGESIZE`'s 39,
// not 40). glibc's value 30 lands on an unrelated `_SC_XOPEN_*`-range slot
// under bionic's table, which is why the wrong value silently poisons
// `page_size()`'s fallback path (returns a non-power-of-two or an
// unrelated small integer, fails `validate_page_size_impl`, and falls
// back to `PAGE` = 4096) instead of tripping the BSD-style "wrong but
// power-of-two" poison scenario the task #714 comment above warns about.
#[cfg(all(unix, not(miri)))]
pub(crate) const _SC_PAGESIZE: i32 = {
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos"
    ))]
    {
        29
    }
    #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
    {
        47
    }
    #[cfg(any(target_os = "netbsd", target_os = "openbsd"))]
    {
        28
    }
    #[cfg(target_os = "android")]
    {
        39
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "android",
    )))]
    {
        // glibc/musl Linux use 30 for _SC_PAGESIZE / _SC_PAGE_SIZE. This is
        // NOT a safe generalization to "most other unices" — that framing
        // has already been wrong twice (the four BSDs above, task #714;
        // Android/bionic above, task #951) — so treat 30 as the
        // glibc/musl-Linux value specifically, not a portable unix default.
        30
    }
};

// task #719, #914: `offset`'s type is conditionally-typed based on the
// target's actual `off_t` width. POSIX `off_t`'s width is platform-dependent:
// - 32-bit Linux (glibc) and Android (bionic) default to a 32-bit `off_t` without
//   `_FILE_OFFSET_BITS=64`. Since this crate's `mmap` is ALWAYS called with
//   offset=0 (anonymous mappings only, no file descriptor), a 32-bit `off_t`
//   is correct on these targets -- there is no code path where a wrong-width
//   `off_t` could silently truncate a real value.
// - 32-bit Linux with musl uses a 64-bit `off_t` unconditionally (musl defines
//   `off_t` as 64-bit on every architecture; `mmap64` is an alias of `mmap`).
// - Every other Unix target (64-bit pointer width, OR 32-bit BSD/Darwin) uses
//   a 64-bit `off_t`. BSDs and macOS define `off_t` as a 64-bit `int64_t` even
//   in 32-bit builds; this includes x86_64-unknown-{freebsd,netbsd,openbsd},
//   i686-unknown-{freebsd,openbsd}, and x86_64-apple-darwin.
// - Android's bionic is REASONED-FROM-SPEC: not empirically verified in this
//   project's CI, but bionic inherits the same 32-bit-off_t-on-32-bit-default
//   behavior as glibc on 32-bit targets without large-file-support flags.
//
// The two-arm `OffT` type alias below classifies targets as follows:
// - The `i32` arm matches 32-bit glibc and bionic targets only.
// - The `i64` catch-all arm matches everything else, including 32-bit musl,
//   all 64-bit targets, and all 32-bit BSD/Darwin targets.
#[cfg(all(
    unix,
    not(miri),
    target_pointer_width = "32",
    any(target_os = "linux", target_os = "android"),
    not(target_env = "musl")
))]
type OffT = i32;

#[cfg(all(
    unix,
    not(miri),
    not(all(
        target_pointer_width = "32",
        any(target_os = "linux", target_os = "android"),
        not(target_env = "musl")
    ))
))]
type OffT = i64;

#[cfg(all(unix, not(miri)))]
extern "C" {
    fn mmap(
        addr: *mut core::ffi::c_void,
        length: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: OffT,
    ) -> *mut core::ffi::c_void;
    fn munmap(addr: *mut core::ffi::c_void, length: usize) -> i32;
    fn madvise(addr: *mut core::ffi::c_void, length: usize, advice: i32) -> i32;
    pub(crate) fn sysconf(name: i32) -> core::ffi::c_long;
}

/// Raw `mmap` wrapper. `Ok(p)` is a successful mapping at a usable address;
/// `Err` carries the cause captured AT the failing syscall: `last_os_error()`
/// for a genuine `MAP_FAILED` refusal, or
/// [`VmemError::os_refusal_unknown_code`] for the R7-11 address-zero grant
/// this crate rejects (no syscall failed there, so no real code exists).
#[cfg(all(unix, not(miri)))]
unsafe fn libc_mmap(len: usize, huge: bool) -> Result<*mut core::ffi::c_void, VmemError> {
    #[cfg_attr(
        not(all(
            any(target_os = "linux", target_os = "android"),
            feature = "huge-pages"
        )),
        allow(unused_mut)
    )]
    let mut flags = MAP_PRIVATE | MAP_ANON;
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        feature = "huge-pages"
    ))]
    if huge {
        flags |= MAP_HUGETLB | MAP_HUGE_2MB;
    }
    let _ = huge; // silence unused on non-linux / no huge-pages builds

    // SAFETY: anonymous private mapping; kernel chooses the address.
    let p = unsafe {
        mmap(
            core::ptr::null_mut(),
            len,
            PROT_READ | PROT_WRITE,
            flags,
            -1,
            0,
        )
    };
    // task #776 (F8): `.addr()` for the same reason as the fast-path fix
    // above -- a comparison-only read, not a round-trip.
    if p.addr() == MAP_FAILED {
        // task #713, enforced here since task #1068/F2: capture errno AT the
        // failing syscall, before any other FFI call (including cleanup) can
        // clobber it. Returning it inside the Result keeps every caller on
        // the immediate-capture discipline by construction — no call site can
        // accidentally capture after an intervening syscall anymore.
        return Err(VmemError::last_os_error());
    }
    // R7-11: POSIX does not guarantee that `mmap(NULL, ...)` never returns address zero.
    // If the kernel returns a successful mapping at address zero, we must unmap it
    // to avoid leaking it before returning an error. The crate does not support
    // address-zero mappings (no caller expects or can safely use them).
    //
    // task #1068/F2: on THIS sub-path no syscall failed — `mmap` succeeded and
    // the `munmap` is cleanup — so there is no valid errno to capture. A
    // caller-side `last_os_error()` after the cleanup would report whatever
    // stale code the `munmap` (or earlier unrelated code) left behind: a
    // fabricated OS cause, exactly the class task #713's immediate-capture
    // discipline exists to eliminate. Report the no-real-code refusal
    // sentinel instead.
    if p.cast::<u8>().is_null() {
        unsafe { libc_munmap(p.cast(), len) };
        return Err(VmemError::os_refusal_unknown_code());
    }
    Ok(p)
}

#[cfg(all(unix, not(miri)))]
unsafe fn libc_munmap(addr: *mut u8, len: usize) {
    // SAFETY: caller guarantees `[addr, addr+len)` was mmap'd and is unmapped once.
    // task #719: the `-1`/`EINVAL` return is deliberately discarded, not
    // merely overlooked — every caller of this function already establishes
    // page/huge-page alignment before calling (task #714 closed the one case
    // where that was NOT true, a silent whole-mapping leak); given that
    // precondition, a genuine `munmap` failure here would indicate a bug in
    // this crate's own alignment bookkeeping, not a recoverable runtime
    // condition the caller could act on (`release`/`decommit`'s public
    // signatures are infallible `()`/`bool`, so there is no channel to
    // surface it through even if we wanted to). The failure mode on error is
    // a leaked mapping, never memory unsafety — the memory stays validly
    // mapped, just not returned to the OS.
    //
    // task P2-6 (2026-08-16 audit finding): increment the failure counter
    // under `bench-internals` so at least diagnostic visibility exists. The
    // counter is gated on the feature and the increment is a single relaxed
    // fetch_add — zero overhead when the feature is off.
    let ret = unsafe { munmap(addr as *mut core::ffi::c_void, len) };
    #[cfg(feature = "bench-internals")]
    if ret != 0 {
        UNIX_MUNMAP_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    #[cfg(not(feature = "bench-internals"))]
    let _ = ret;
}

#[cfg(all(unix, not(miri)))]
// mock (task #646/F8): only caller is decommit_pages_impl above, itself
// unused under `mock`.
#[cfg_attr(aligned_vmem_mock, allow(dead_code))]
unsafe fn libc_madvise(addr: *mut u8, len: usize, advice: i32) {
    // SAFETY: caller guarantees `[addr, addr+len)` is within a live mmap region.
    // task #719: the return value is deliberately discarded -- `madvise`
    // failing here means the OS did not reclaim the pages (the mapping stays
    // exactly as valid and readable/writable as before the call), not a
    // memory-safety concern; [`decommit`]/[`decommit_lazy`]'s own public
    // contracts already document decommit as an OS-cooperative hint whose
    // failure mode is "the physical pages were not actually returned", never
    // a dangling/invalid mapping.
    //
    // task #882: under `bench-internals` ONLY, also record whether the
    // syscall itself succeeded (returned `0`) or failed (returned `-1`) into
    // `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` -- see those statics'
    // docs for why (settling item 48's H1-vs-H2 root-cause question). The
    // discard above is unconditional and unchanged for every non-bench build;
    // this is a read of the same return value the plain build throws away,
    // not a new syscall or a change to what `libc_madvise` returns to its
    // caller (still `()` either way), so it is zero-cost when the feature is
    // off.
    // SAFETY: `madvise` is safe for any address/len within a live mmap region; advice is a valid constant.
    let ret = unsafe { madvise(addr as *mut core::ffi::c_void, len, advice) };
    #[cfg(feature = "bench-internals")]
    {
        UNIX_MADVISE_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
        if ret == 0 {
            UNIX_MADVISE_SUCCESSES.fetch_add(1, Ordering::Relaxed);
        }
    }
    #[cfg(not(feature = "bench-internals"))]
    let _ = ret;
}
