use crate::decommit_outcome::DecommitOutcome;
use crate::error::VmemError;
#[cfg(aligned_vmem_mock)]
use crate::mock;
#[cfg(not(aligned_vmem_mock))]
use crate::os::{decommit_pages_impl, DecommitKind};
use crate::page_size::{page_size_or_poison, PAGE_SIZE_QUERY_FAILED};

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
/// `start` and `end` must be multiples of [`page_size()`](crate::page_size::page_size) and within the span.
/// A no-op if the range is empty AND page-aligned — and a VIOLATED range
/// (`start > end`, or an endpoint not a multiple of [`page_size()`](crate::page_size::page_size) — which
/// includes an empty MISALIGNED range such as `decommit(ptr, 1, 1)`) is a
/// silent no-op in a release build; see "Contract violations, by build
/// profile" below for the debug-build tripwire and the fallible
/// [`try_decommit`] form.
///
/// # Safety
///
/// - `base` must be the [`as_ptr`](crate::Reservation::as_ptr) of a live
///   reservation the caller owns.
/// - **`end <= reservation.len()`** (the reservation's usable span, in
///   bytes) — this is a MANDATORY precondition of the pointer arithmetic
///   this function performs internally (`base.add(start)` /
///   `base.add(end)`), not merely a functional/behavioral preference. This
///   requirement is stated here explicitly (task #1213/L2) rather than left
///   to the summary line above ("within the span") — for an `unsafe fn`,
///   a bounds requirement that determines whether pointer arithmetic is
///   even defined belongs inside `# Safety` itself, restated in full, not
///   referenced from an adjacent paragraph a caller auditing only this
///   section could miss. Passing `end > reservation.len()` is undefined
///   behavior (out-of-bounds pointer arithmetic), distinct from — and a
///   strictly worse violation than — the `page_size()`-multiple contract
///   below, which is merely a silent no-op on violation, never UB.
/// - `[base+start, base+end)` must contain no data the caller still needs —
///   its contents are discarded.
///
/// **Contract violations, by build profile (task #1051):** this entry point
/// is intentionally infallible — the `()` return carries no write-permitting
/// sentinel to misuse — so a violated range (`start > end`, or an endpoint
/// not a multiple of [`page_size()`](crate::page_size::page_size)) is a silent no-op in a RELEASE build:
/// no OS call is made and nothing is recorded. In a DEBUG build the same
/// violation trips the `debug_assert!` below before anything happens, so a
/// consumer's own test fails at the mistake instead of quietly decommitting
/// nothing and leaving the memory resident; zero cost in release.
/// [`try_decommit`] is the fallible form for callers who need the violation
/// reported: it returns `Err` on every profile and never trips the tripwire.
///
/// **A poisoned page-size query is a DIFFERENT case and never panics, on
/// any profile (task #1145/#1139, sharpened task #1173/L1):** if the
/// one-time OS page-size query itself failed (see
/// [`page_size()`](crate::page_size::page_size)'s "If the one-time OS query
/// fails"), this function fails closed silently — no `debug_assert!`, no
/// tripwire — because the caller's arguments are not at fault and the
/// crate-wide poison contract promises an unconditional no-op here, matching
/// [`decommit_lazy`](crate::api::decommit_lazy)'s no-tripwire design and the
/// README's "never panics" list. This is distinct from the range-contract
/// tripwire immediately above, which fires only in debug builds and only for
/// a violated range under a HEALTHY page-size query.
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
/// Documented per the `madvise(2)` man page cited above, and — since task
/// #1152 (F1) — empirically exercised by this crate's own CI: the
/// `aligned-vmem-hugetlb-real` job (`.github/workflows/ci.yml`) configures a
/// real `nr_hugepages` pool and hard-asserts (via a dedicated
/// path-activation oracle) that `reserve_aligned_huge` actually received a
/// `MAP_HUGETLB` grant rather than silently falling back to ordinary pages.
/// Under that real grant, the job runs
/// `huge_aligned_range_takes_the_real_backend_path_not_the_skip_path` and
/// `huge_decommit_attempts_increments_on_huge_reservation`
/// (`tests/decommit_capability.rs`), which drive a huge-page-eligible
/// `[start, end)` through [`Reservation::decommit`](crate::Reservation::decommit)'s eligible-huge
/// branch — the same `decommit_pages_impl`/`MADV_DONTNEED` backend call this
/// free function itself makes. **What that job proves, stated precisely
/// (task #1160/F1 correction of an earlier overclaim; strengthened task
/// #1164):** the eligible-range case genuinely REACHES the real
/// `madvise(2)`/`MADV_DONTNEED` backend call under a real `MAP_HUGETLB`
/// grant, rather than silently taking the Rust-level skip path — AND, since
/// task #1164's `ci_hugetlb_real_pool_kernel_actually_accepts_eligible_madvise`
/// (`tests/decommit_capability.rs`), the kernel's own syscall-level response
/// is also asserted: under `bench-internals`, `libc_madvise`
/// (`src/os/unix.rs`) records whether the syscall returned `0` or `-1`, and
/// that job hard-asserts it returned `0` for this eligible-range call. It
/// does NOT prove the kernel actually returned the physical backing, or that
/// a subsequent access re-faults zeroed memory — no test on this path reads
/// memory content back, so whether `MADV_DONTNEED`'s *effect* (as opposed to
/// its *return code*) was honoured by the kernel for this eligible-range/
/// post-5.18 case remains reasoned from the man page, not independently
/// observed. On builds WITHOUT `bench-internals`, `libc_madvise` still
/// discards the return value entirely (task #719) — the kernel-response
/// proof above is scoped to the one CI job that enables the counters. It
/// also does not call this free function's own entry point directly (no
/// test invokes `decommit` outside a `Reservation` method), so this
/// function's own unconditional-syscall behavior on an INELIGIBLE range
/// (still a no-op by kernel contract, not by Rust-level skip) remains
/// reasoned-from-spec, not independently exercised under a real pool.
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
    // round the length up across live data — fail closed instead. Silent on
    // EVERY profile, deliberately, by design (task #1145/#1139, `4cba9c1`):
    // that commit's own message records "Rejected: panicking (the README's
    // 'never panics' list stays at three)", and the crate-wide poison
    // contract documented in `page_size()`'s rustdoc and the README's
    // "If the one-time OS query fails" section states unconditionally that
    // `decommit`/`decommit_lazy` become no-ops, with no build-profile
    // qualifier — unlike the range-contract tripwire below, which IS
    // profile-qualified and documented as such. A `debug_assert!(false, ..)`
    // here used to contradict that design decision (task #1173/L1): it made
    // every debug-build caller of `decommit`/`Reservation::decommit` panic
    // under a poisoned page size regardless of how well-formed the caller's
    // OWN range was, silently promoted from a documented no-op into a crash
    // no `# Panics` section on this function (there isn't one) ever
    // disclosed. Use `try_decommit`/`try_page_size` to observe this state
    // instead — see `page_size()`'s "If the one-time OS query fails".
    if ps == PAGE_SIZE_QUERY_FAILED {
        return;
    }
    // A contract violation here is silent BY SIGNATURE — this function returns
    // `()` and has nowhere to report one. In a debug build say so loudly, so a
    // consumer's own test fails at the mistake rather than quietly decommitting
    // nothing and leaving the memory resident. Zero cost in release.
    debug_assert!(
        decommit_range_is_well_formed(start, end, ps),
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
    //
    // task #1180 (PUB-R2 phase 2): `decommit_pages_impl` now reports the
    // backend's own accept/refuse outcome. This function stays infallible BY
    // SIGNATURE (see its own doc and `# Safety`'s "Contract violations"
    // section) — the outcome is deliberately discarded here, exactly as it
    // always was before this task (which only changed WHERE the discard
    // happens, from inside `libc_madvise`/`winapi_virtual_decommit`
    // unconditionally, to here). Use [`try_decommit`] to observe it.
    let _ = unsafe { decommit_pages_impl(base, start, end, DecommitKind::Eager) };
}

/// Whether `[start, end)` is a well-formed decommit range: `start <= end` and
/// both endpoints are multiples of `ps`.
///
/// An EMPTY range (`start == end`, page-aligned) is well-formed — it is a
/// deliberate no-op, not a mistake. That distinction is why this predicate
/// exists separately from the `start >= end` early-return in [`decommit`]:
/// the early return conflates "nothing to do" with "you got the arguments
/// wrong", and only the second deserves a diagnostic.
///
/// **Takes `ps` as a parameter instead of reading [`page_size_or_poison`]
/// itself (task #1213/L1 — corrected doc-vs-code drift: this comment
/// previously claimed both callers already read `page_size_or_poison()`
/// once and the predicate reading it again "makes no observable
/// difference," which was true only of the debug-only `decommit`/
/// `debug_assert!` call site, never of `try_decommit`, which read
/// `page_size_or_poison()` once at its own top, then AGAIN inside this
/// predicate, in every build profile including release — two atomic loads
/// per call on the exact population `dispatch_try_decommit`'s own doc
/// above already optimized down to one, for the opposite reason).** Every
/// caller now takes its own `page_size_or_poison()` snapshot ONCE and
/// passes it in here — this predicate performs no atomic load of its own.
/// The fail-closed property (task #1156, finding F16) is unchanged: a
/// caller MUST pass [`page_size_or_poison`]'s raw value, never the masked
/// public [`page_size()`](crate::page_size::page_size) (which silently
/// substitutes [`PAGE`](crate::PAGE), 4 KiB, for "unknown" in the degraded
/// state) — every current caller pre-checks
/// `ps == PAGE_SIZE_QUERY_FAILED` and returns before reaching this
/// predicate, but if a future caller ever reached it without that
/// pre-check, passing the unmasked value still fails closed by
/// construction: `is_multiple_of(usize::MAX)` is true only for `0` or
/// `usize::MAX`, so any ordinary non-empty range is rejected here too, not
/// just by the callers' own pre-checks — the trap task #1139's design note
/// ("the arithmetic is suspenders, so forgetting a check cannot reopen the
/// hole") means to rule out.
#[must_use]
fn decommit_range_is_well_formed(start: usize, end: usize, ps: usize) -> bool {
    start <= end && start.is_multiple_of(ps) && end.is_multiple_of(ps)
}

/// Task #1180 (PUB-R2 phase 2), poached finding P2: the single private
/// dispatch point for every `try_decommit`-shaped caller — the free
/// [`try_decommit`] AND [`Reservation::try_decommit`](crate::Reservation::try_decommit) — issuing exactly
/// ONE `page_size_or_poison()` snapshot (`ps`, taken here and nowhere else in
/// either caller) and calling the real backend exactly once when a call is
/// warranted.
///
/// Before this task, `Reservation::try_decommit` re-validated the range
/// itself (its own `page_size_or_poison()` load) and then, on the non-huge/
/// eligible-huge path, forwarded to the free `try_decommit`, which validated
/// AGAIN (a second `page_size_or_poison()` load) before finally calling
/// [`decommit`] a third time removed from the original caller. Three relaxed
/// atomic loads and two redundant validations for one logical operation —
/// cheap next to the syscall when one is actually issued, but wasted work on
/// every EMPTY/INVALID/SKIPPED call, which is exactly the population that
/// never reaches a syscall to amortize it against. This function is now the
/// only place that reads `page_size_or_poison()` on the `try_decommit`
/// dispatch path and the only place that calls the backend, called by BOTH
/// public entry points after each does its OWN validation (the free function
/// has no bounds/huge concept to check first; the method's bounds check and
/// huge-skip decision must run before this is even reached, since a skip
/// must never touch the backend at all) — so the total atomic-load count for
/// ANY call through either entry point is now exactly one, not two or three.
///
/// Returns `DecommitOutcome::Advised` / `DecommitOutcome::Refused(_)` for a
/// call to the SELECTED backend — on the native backend (no
/// `aligned_vmem_mock` cfg) that is a genuinely-issued syscall, mapped
/// straight from `decommit_pages_impl`'s own `Result`; under the
/// `aligned_vmem_mock` cfg no syscall runs at all — the mock backend records
/// the call into its call log and this function unconditionally returns
/// `Advised` without ever calling `decommit_pages_impl` (see
/// [`DecommitOutcome::Advised`]'s own doc for why that simulated-vs-real
/// distinction does not need a separate `Skipped`/third variant here: the
/// call itself DID happen, from this crate's point of view — only the
/// backend it reached differs). Never returns `Skipped` — that variant is
/// produced by the CALLERS (the free function's own empty-range
/// short-circuit, and `Reservation::try_decommit`'s huge-skip branch), never
/// by this function, which is reached only when a call has already been
/// decided.
///
/// # Safety
///
/// Same contract as [`decommit`]: `base` must be the usable base of a live
/// reservation owned by the caller, and `[base+start, base+end)` — already
/// validated well-formed and NON-EMPTY by the caller — must lie within its
/// usable span.
pub(crate) unsafe fn dispatch_try_decommit(
    base: *mut u8,
    start: usize,
    end: usize,
) -> DecommitOutcome {
    #[cfg(aligned_vmem_mock)]
    {
        mock::record(mock::Call::Decommit {
            base: base.addr(),
            start,
            end,
        });
        // The mock backend never touches the OS (see the module-level doc in
        // `mock.rs`), so there is no real syscall outcome to report — treat a
        // recorded mock call as accepted, matching the pre-#1180 `Ok(())`
        // this function's callers gave under `mock`.
        DecommitOutcome::Advised
    }
    #[cfg(not(aligned_vmem_mock))]
    {
        // SAFETY: forwarded from this function's own `# Safety` contract.
        match unsafe { decommit_pages_impl(base, start, end, DecommitKind::Eager) } {
            Ok(()) => DecommitOutcome::Advised,
            Err(e) => DecommitOutcome::Refused(e),
        }
    }
}

/// Fallible [`decommit`]: the same operation, with a channel for the one thing
/// `decommit` cannot report — **and, since task #1180 (PUB-R2 phase 2), a
/// channel for the OS's own accept/refuse answer too**, not just argument
/// validity.
///
/// Of this crate's state-changing primitives, `decommit`/[`decommit_lazy`](crate::api::decommit_lazy) were
/// the only pair with no fallible twin — and also the only ones that do nothing
/// at all on a contract violation. The worst two properties met in one place:
/// silent AND unreportable. This closes the first half.
///
/// # Errors
///
/// [`VmemError::invalid_argument`] if `start > end`, or either endpoint is not
/// a multiple of the runtime [`page_size()`](crate::page_size::page_size). An empty page-aligned range
/// (`start == end`) is a well-formed no-op and returns `Ok(DecommitOutcome::Skipped)`.
///
/// [`VmemError::os_refusal_unknown_code`] if the one-time OS page-size query
/// itself failed — the caller's arguments are not at fault; see
/// [`page_size()`](crate::page_size::page_size)'s "If the one-time OS query fails" paragraph.
///
/// Note what is deliberately NOT reported as an `Err` (the outer `Result`
/// keeps reporting only caller-contract validity, exactly as before this
/// task): the OS refusing or ignoring the request is `Ok(DecommitOutcome::Refused(_))`,
/// not `Err`. `decommit` is best-effort by nature — on Darwin and the BSDs
/// `MADV_DONTNEED` is advisory, and on a huge-page reservation eligibility
/// depends on the platform, the requested range, and (on Linux/Android) the
/// running kernel — see [`decommit`]'s "Huge-page granularity" section above
/// for the exact split. Promoting an OS refusal to `Err` would conflate "your
/// arguments were rejected" with "the platform declined to honor a
/// well-formed request", which is exactly the ambiguity
/// [`DecommitOutcome`] exists to separate. Use
/// [`Reservation::decommit_reclaims_and_zeroes`](crate::Reservation::decommit_reclaims_and_zeroes) to learn what the platform
/// actually does; [`DecommitOutcome::Advised`]/[`DecommitOutcome::Refused`] tell
/// you what THIS call did, not what it accomplished (see that type's own doc).
///
/// This free function always either short-circuits on an empty range
/// (`Ok(DecommitOutcome::Skipped)`) or forwards to the real backend — it has
/// no [`Reservation::is_huge`](crate::Reservation::is_huge) to consult, so unlike
/// [`Reservation::try_decommit`](crate::Reservation::try_decommit) it can never produce `Skipped` for a
/// non-empty range; every non-empty well-formed range here becomes either
/// `Advised` or `Refused`.
///
/// # Safety
///
/// Identical to [`decommit`]: `base` must be the usable base of a live
/// reservation owned by the caller, and `[base+start, base+end)` must lie
/// within its usable span.
pub unsafe fn try_decommit(
    base: *mut u8,
    start: usize,
    end: usize,
) -> Result<DecommitOutcome, VmemError> {
    // Failed OS page-size query: fail closed, reported as an OS-side no-code
    // failure — the caller's arguments are NOT at fault, so this must not
    // read as `invalid_argument` (see `page_size`'s "If the one-time OS
    // query fails" paragraph).
    //
    // Single snapshot (task #1213/L1): taken once here and passed into
    // `decommit_range_is_well_formed` below, instead of that predicate
    // re-reading `page_size_or_poison()` a second time — this used to be
    // two atomic loads per call, in every build profile, for a value that
    // cannot have changed between them (the query result is fixed for the
    // process lifetime).
    let ps = page_size_or_poison();
    if ps == PAGE_SIZE_QUERY_FAILED {
        return Err(VmemError::os_refusal_unknown_code());
    }
    if !decommit_range_is_well_formed(start, end, ps) {
        return Err(VmemError::invalid_argument());
    }
    if start == end {
        return Ok(DecommitOutcome::Skipped);
    }
    // SAFETY: forwarded from this function's own `# Safety` contract, which is
    // identical to `decommit`'s; the range was just validated well-formed and
    // non-empty above.
    Ok(unsafe { dispatch_try_decommit(base, start, end) })
}
