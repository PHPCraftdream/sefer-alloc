use crate::error::VmemError;
#[cfg(aligned_vmem_mock)]
use crate::mock;
#[cfg(not(aligned_vmem_mock))]
use crate::os::{decommit_pages_impl, DecommitKind};
use crate::page_size::{page_size, page_size_or_poison, PAGE_SIZE_QUERY_FAILED};

/// Decommit pages `[base + start, base + end)`: hint the OS to return
/// their physical backing while keeping the address-space reservation alive.
///
/// **Programmatically check platform guarantees:** use
/// [`Reservation::decommit_reclaims_and_zeroes`](crate::Reservation::decommit_reclaims_and_zeroes) to query whether the current
/// platform guarantees reclaim+zero-fill semantics. Returns `true` on Linux/Windows,
/// `false` on Darwin/BSD where decommit is advisory-only.
///
/// **Platform behavior:**
/// - On Linux and Windows this is guaranteed to return physical backing and
///   zero-fill on next access (Linux `MADV_DONTNEED`, Windows `MEM_DECOMMIT`).
/// - On the Darwin family (macOS/iOS/tvOS/watchOS) and the four BSDs
///   (FreeBSD/DragonFly/NetBSD/OpenBSD), this is a best-effort hint with no
///   zero-fill or reclaim guarantee — the physical pages may remain resident and
///   old data may be observed after a decommit+recommit roundtrip.
///   See [`Reservation::decommit_reclaims_and_zeroes`](crate::Reservation::decommit_reclaims_and_zeroes).
///
/// `start` and `end` must be multiples of [`page_size()`] and within the span.
/// A no-op if the range is empty AND page-aligned — and a VIOLATED range
/// (`start > end`, or an endpoint not a multiple of [`page_size()`] — which
/// includes an empty MISALIGNED range such as `decommit(ptr, 1, 1)`) is a
/// silent no-op in a release build; see "Contract violations, by build
/// profile" below for the debug-build tripwire and the fallible
/// [`try_decommit`] form.
///
/// # Safety
///
/// `base` must be the [`as_ptr`](crate::Reservation::as_ptr) of a live reservation,
/// and `[base+start, base+end)` must contain no data the caller still needs —
/// its contents are discarded.
///
/// **Contract violations, by build profile (task #1051):** this entry point
/// is intentionally infallible — the `()` return carries no write-permitting
/// sentinel to misuse — so a violated range (`start > end`, or an endpoint
/// not a multiple of [`page_size()`]) is a silent no-op in a RELEASE build:
/// no OS call is made and nothing is recorded. In a DEBUG build the same
/// violation trips the `debug_assert!` below before anything happens, so a
/// consumer's own test fails at the mistake instead of quietly decommitting
/// nothing and leaving the memory resident; zero cost in release.
/// [`try_decommit`] is the fallible form for callers who need the violation
/// reported: it returns `Err` on every profile and never trips the tripwire.
///
/// **Platform divergence, not just a data-loss concern:** on Windows,
/// `MEM_DECOMMIT` genuinely unmaps the pages, so a **write to `[base+start,
/// base+end)` before [`recommit`](crate::api::recommit) is a hard `STATUS_ACCESS_VIOLATION`
/// crash**, not a soft re-fault. On Linux, `MADV_DONTNEED` keeps the mapping
/// resident and transparently re-faults a fresh zero page on next write, so
/// the same code that is safe on Linux can crash on Windows. This exact
/// divergence already crashed an in-repo consumer that assumed the Linux
/// semantics — see
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
/// item 6 (filed 2026-07-30) for the incident record and status.
///
/// **Huge-page granularity (task #843 V4/finding R4-4, corrected task #1140):**
/// on huge-page reservations (those returned by
/// [`reserve_aligned_huge`](crate::api::reserve_aligned_huge) with [`Reservation::is_huge`](crate::Reservation::is_huge) == `true`),
/// **on Windows, decommit does not work at all**: `VirtualFree` with
/// `MEM_DECOMMIT` unconditionally fails on large-page regions.
///
/// **On Linux/Android, whether decommit works depends on the requested range and
/// the running kernel**, not on whether the mapping is huge — `madvise(2)`
/// documents that `MADV_DONTNEED` gained HugeTLB support in Linux 5.18, with
/// the same requirement it already has for ordinary mappings: `[base+start,
/// base+end)` must be aligned to the mapping's huge page size (2 MiB on this
/// crate's supported targets) at BOTH endpoints. This crate's own Linux/Android
/// `huge-pages` contract already requires `reserve_aligned_huge`'s `size`/`align`
/// to be multiples of that same 2 MiB, so a huge-aligned `[start, end)` is not a
/// hypothetical — decommitting an entire huge reservation, or any 2-MiB-granular
/// sub-range of it, is exactly such a range. A `page_size()`-granular (e.g. 4
/// KiB) but NOT 2-MiB-granular offset still gets `EINVAL` from the kernel and
/// does nothing — **this free function issues the syscall regardless of
/// eligibility** (unlike [`Reservation::decommit`](crate::Reservation::decommit), which can consult
/// [`Reservation::is_huge`](crate::Reservation::is_huge) and the requested range to skip the
/// ineligible case before the syscall — see that method's doc for the exact
/// split), so an ineligible range here is a wasted syscall that the kernel
/// itself turns into a no-op, not a Rust-level skip. On a pre-5.18 kernel,
/// EVERY range is ineligible regardless of alignment (the capability did not
/// exist yet), so decommit is unconditionally a no-op there, matching the
/// prior (task #843) documented behavior exactly. Either way — ineligible
/// range, or eligible range on a pre-5.18 kernel — the effect is
/// indistinguishable from a silent no-op: the caller's RSS does not decrease,
/// and subsequent reads return the old (stale) data rather than zeroed pages.
///
/// REASONED-FROM-SPEC per the `madvise(2)` man page cited above; NOT
/// empirically verified by this crate's own CI, which has no
/// hugetlb-configured Linux runner (see `Reservation::decommit`'s doc for the
/// exact CI row this would need).
///
/// **Diagnostic visibility:** under the `bench-internals` feature, the
/// `huge_decommit_attempts` counter (not an intra-doc link: `bench-internals` is excluded from the published docs.rs feature set) is incremented each time
/// [`Reservation::decommit`](crate::Reservation::decommit)/[`Reservation::try_decommit`](crate::Reservation::try_decommit) skip the
/// backend call on a huge-page reservation — it is NOT incremented by calls
/// through this free function (which has no `is_huge()` to consult and always
/// issues the syscall) or by an eligible Linux/Android >= 5.18 huge-aligned
/// call through the safe methods (those forward to the real backend instead
/// of skipping). Use [`reserve_aligned`](crate::api::reserve_aligned) instead of
/// [`reserve_aligned_huge`](crate::api::reserve_aligned_huge) if you need decommit to work
/// unconditionally, regardless of range shape or kernel version.
///
/// **Darwin zero-fill gap (confirmed as a real, failing-test-level gap by
/// this crate's first real-macOS CI run, 2026-08-13 — the underlying hazard
/// was already known repo-wide since Round 9, see
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
/// item 48):** `MADV_DONTNEED` on Darwin and the four BSDs (FreeBSD/DragonFly/
/// NetBSD/OpenBSD) is advisory-only for anonymous memory — unlike Linux, it does
/// not reliably unmap the physical pages, so a decommit + [`recommit`](crate::api::recommit) roundtrip
/// on these OS families (macOS/iOS/tvOS/watchOS — all share XNU and the same
/// `MADV_DONTNEED` semantics, not just macOS — plus the four BSDs which use
/// identical `MADV_DONTNEED` semantics) can observe the OLD data still resident
/// instead of a fresh zero page. This is the same "indistinguishable
/// from a silent no-op" shape as the huge-page case above, but for ORDINARY
/// (non-huge) reservations on Darwin and the BSDs specifically. See
/// <https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>
/// for the open item; no fix is implemented
/// yet (the real fix needs re-`mmap`(`MAP_FIXED`) over the range, a larger
/// change deserving its own review round). Note: this caveat applies only to
/// the EAGER `decommit` path (which uses `MADV_DONTNEED` on all Unix); the
/// lazy `decommit_lazy` path uses `MADV_FREE`-family advice on Darwin/BSDs and
/// DOES free pages on those platforms.
pub unsafe fn decommit(base: *mut u8, start: usize, end: usize) {
    let ps = page_size_or_poison();
    // Failed OS page-size query (never observed on a supported platform):
    // with the real page unknown, ANY granularity guess could make the OS
    // round the length up across live data — fail closed instead. Silent in
    // release BY SIGNATURE (same rationale as the range tripwire below);
    // loud in debug with the accurate cause, not a misleading
    // "you misaligned your range" message.
    if ps == PAGE_SIZE_QUERY_FAILED {
        debug_assert!(
            false,
            "aligned-vmem: the OS page-size query failed, so decommit is \
             disabled for this process (fail-closed); the call does nothing. \
             Use try_decommit / try_page_size to observe this state."
        );
        return;
    }
    // A contract violation here is silent BY SIGNATURE — this function returns
    // `()` and has nowhere to report one. In a debug build say so loudly, so a
    // consumer's own test fails at the mistake rather than quietly decommitting
    // nothing and leaving the memory resident. Zero cost in release.
    debug_assert!(
        decommit_range_is_well_formed(start, end),
        "aligned-vmem: decommit({start}, {end}) violates the range contract \
         (start > end, or an endpoint is not a multiple of page_size()); the \
         call does nothing. Use try_decommit for the fallible form."
    );
    if start >= end || !start.is_multiple_of(ps) || !end.is_multiple_of(ps) {
        return;
    }
    #[cfg(aligned_vmem_mock)]
    mock::record(mock::Call::Decommit {
        base: base.addr(),
        start,
        end,
    });
    #[cfg(not(aligned_vmem_mock))]
    // SAFETY: forwarded from the caller's contract; the per-OS routine touches
    // only kernel page-state, never the bytes.
    unsafe {
        decommit_pages_impl(base, start, end, DecommitKind::Eager)
    };
}

/// Whether `[start, end)` is a well-formed decommit range: `start <= end` and
/// both endpoints are multiples of the runtime [`page_size()`].
///
/// An EMPTY range (`start == end`, page-aligned) is well-formed — it is a
/// deliberate no-op, not a mistake. That distinction is why this predicate
/// exists separately from the `start >= end` early-return in [`decommit`]:
/// the early return conflates "nothing to do" with "you got the arguments
/// wrong", and only the second deserves a diagnostic.
#[must_use]
fn decommit_range_is_well_formed(start: usize, end: usize) -> bool {
    let ps = page_size();
    start <= end && start.is_multiple_of(ps) && end.is_multiple_of(ps)
}

/// Fallible [`decommit`]: the same operation, with a channel for the one thing
/// `decommit` cannot report.
///
/// Of this crate's state-changing primitives, `decommit`/[`decommit_lazy`](crate::api::decommit_lazy) were
/// the only pair with no fallible twin — and also the only ones that do nothing
/// at all on a contract violation. The worst two properties met in one place:
/// silent AND unreportable. This closes the first half.
///
/// # Errors
///
/// [`VmemError::invalid_argument`] if `start > end`, or either endpoint is not
/// a multiple of the runtime [`page_size()`]. An empty page-aligned range
/// (`start == end`) is a well-formed no-op and returns `Ok(())`.
///
/// Note what is deliberately NOT an error: the OS refusing or ignoring the
/// request. `decommit` is best-effort by nature — on Darwin and the BSDs
/// `MADV_DONTNEED` is advisory, and on a huge-page reservation the operation is
/// incompatible outright. Reporting those as `Err` would promise a portable
/// guarantee the platforms do not give. Use
/// [`Reservation::decommit_reclaims_and_zeroes`](crate::Reservation::decommit_reclaims_and_zeroes) to learn what the platform
/// actually does.
///
/// # Safety
///
/// Identical to [`decommit`]: `base` must be the usable base of a live
/// reservation owned by the caller, and `[base+start, base+end)` must lie
/// within its usable span.
pub unsafe fn try_decommit(base: *mut u8, start: usize, end: usize) -> Result<(), VmemError> {
    // Failed OS page-size query: fail closed, reported as an OS-side no-code
    // failure — the caller's arguments are NOT at fault, so this must not
    // read as `invalid_argument` (see `page_size`'s "If the one-time OS
    // query fails" paragraph).
    if page_size_or_poison() == PAGE_SIZE_QUERY_FAILED {
        return Err(VmemError::os_refusal_unknown_code());
    }
    if !decommit_range_is_well_formed(start, end) {
        return Err(VmemError::invalid_argument());
    }
    if start == end {
        return Ok(());
    }
    // SAFETY: forwarded from this function's own `# Safety` contract, which is
    // identical to `decommit`'s.
    unsafe { decommit(base, start, end) };
    Ok(())
}
