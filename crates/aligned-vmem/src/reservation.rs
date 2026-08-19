use core::ptr::NonNull;
#[cfg(feature = "bench-internals")]
use core::sync::atomic::Ordering;

#[cfg(feature = "lazy-commit")]
use crate::api::{commit_range, try_commit_range};
use crate::api::{decommit, decommit_lazy, recommit, try_decommit, try_recommit};
#[cfg(feature = "bench-internals")]
use crate::bench_internals::HUGE_DECOMMIT_ATTEMPTS;
use crate::error::VmemError;
use crate::os::release_reservation;
use crate::page::PAGE;
use crate::page_size::{page_size_or_poison, PAGE_SIZE_QUERY_FAILED};
use crate::reservation_full_parts::ReservationFullParts;
use crate::reservation_parts::ReservationParts;

/// An owning handle to one aligned span of anonymous virtual memory.
///
/// `as_ptr()` is non-null, aligned to the `align` requested at reservation, and
/// valid for `len()` bytes for the lifetime of this handle **with the following
/// exceptions**:
///
/// - **Decommitted ranges**: Ranges that the caller has decommitted (via the
///   free functions or the safe methods) and not yet recommitted have
///   platform-specific behavior:
///   - **Windows**: pages are unmapped until `recommit`; access before `recommit`
///     crashes with `STATUS_ACCESS_VIOLATION`.
///   - **Linux (eager `decommit`)**: pages are zeroed on next access via `MADV_DONTNEED`.
///   - **Linux (lazy `decommit_lazy`)**: pages keep old contents until kernel
///     reclaims them under pressure; writes before reclamation cancel the free.
///   - **Darwin/BSD**: pages keep old contents; `MADV_DONTNEED` is advisory-only
///     and does not reliably zero.
///   - **Huge reservations, `decommit_lazy` (both layers), and `decommit`/
///     `try_decommit` on Windows or on a non-huge-page-aligned range on
///     Linux/Android**: old contents remain. The safe methods
///     [`Reservation::decommit`]/[`Reservation::decommit_lazy`] skip the
///     backend call outright in this case (they can consult `is_huge()` and,
///     for `decommit`, the requested range); the free functions cannot
///     consult `is_huge()`, so they still issue the syscall — which the OS
///     then refuses or ignores. Same observable outcome, different mechanism;
///     do not read "no-op" as "no syscall" for the free functions.
///   - **Huge reservations, eager `decommit`/`try_decommit`, Linux/Android
///     kernel >= 5.18, range aligned to the huge page size (2 MiB) at both
///     endpoints (task #1140)**: this is the ONE huge-page case where decommit
///     actually works — pages ARE zeroed on next access via `MADV_DONTNEED`,
///     same as the ordinary eager-Linux case above. Both the safe method and
///     the free function issue the real syscall here; they agree. See
///     [`Reservation::decommit`]'s own doc for the exact eligibility rule.
///
/// - **Lazy reservations on Windows (feature `lazy-commit`)**: When created via
///   `reserve_aligned_lazy`, only the `initial_commit` prefix is committed at
///   reservation time. The tail `[initial_commit, len())` must be committed via
///   `commit_range` before it becomes writable. Writing to the uncommitted tail
///   results in an access violation.
///
/// The span is **not** initialised. Dropping the handle returns the whole
/// underlying OS reservation to the OS exactly once.
///
/// For a self-hosted allocator that records `(reservation, reservation_len)` in
/// its own metadata rather than keeping a `Vec<Reservation>`, use
/// [`into_parts`](Self::into_parts) to take the raw reservation (suppressing the
/// `Drop`) and release it later with [`release`](crate::api::release).
///
/// `Reservation` is `Send` (the span is owned exclusively) but not `Sync`
/// (writes through the raw pointer are unsynchronised — that is the caller's
/// concern).
pub struct Reservation {
    pub(crate) base: NonNull<u8>,
    pub(crate) len: usize,
    pub(crate) reservation: NonNull<u8>,
    pub(crate) reservation_len: usize,
    /// The alignment requested at reservation time. Carried so the `Drop` /
    /// [`release`](crate::api::release) path can reconstruct the exact `Layout` under miri (the
    /// native `munmap` / `VirtualFree` paths ignore it). See [`into_parts`].
    pub(crate) align: usize,
    /// Whether OS large/huge pages were actually granted for this reservation.
    /// True if `reserve_aligned_huge` succeeded in obtaining large pages on
    /// Linux (`MAP_HUGETLB`) or Windows (`MEM_LARGE_PAGES` when the OS grants
    /// the request). False if the request fell back to ordinary pages.
    ///
    /// This flag is the "best-effort" observable: a caller can detect whether
    /// the huge-page feature actually engaged, rather than receiving only an
    /// indistinguishable `Ok(Reservation)` on every fallback path.
    ///
    /// **Windows limitation (task #848 single-call fast path):** on Windows,
    /// this flag is `true` only when ALL of the following hold:
    /// 1. The fast-path condition `align <= GetLargePageMinimum()` is satisfied
    ///    (typically `align <= 2 MiB` on x86_64)
    /// 2. `size` is a multiple of the system's large-page minimum
    /// 3. The calling process has `SeLockMemoryPrivilege` granted AND has
    ///    **enabled** it via `AdjustTokenPrivileges` (the crate does not do
    ///    this for you — a process with the privilege granted but not
    ///    enabled fails exactly like an unprivileged one and silently falls
    ///    back to ordinary pages)
    ///
    /// NOTE: The widened fast-path condition (II-3, 2026-08-16 audit finding) expanded
    /// the single-call ATTEMPT window from `align <= 64 KiB` to `align <= GetLargePageMinimum()`,
    /// but on an unprivileged host the actual paths that SUCCEED (pass the post-call alignment
    /// check) are typically still limited. When large pages are NOT granted (unprivileged),
    /// `VirtualAlloc`'s alignment guarantee is only 64 KiB; in practice it typically does NOT
    /// happen to land on the requested alignment, so the post-call check fails and the fast
    /// path falls through to the two-call path. Practically, this means `is_huge == true` only
    /// for shapes where large pages are actually granted, which requires all three conditions
    /// above to hold.
    ///
    /// If any of these conditions fail, the function falls back to ordinary
    /// pages and this flag is `false`. On Windows, large pages (`MEM_LARGE_PAGES`)
    /// are only ever requested and possibly granted via the single-call fast path;
    /// the two-call path never requests large pages, so
    /// `granted_huge` is always `false` for a reservation that takes it.
    pub(crate) granted_huge: bool,
}

impl core::fmt::Debug for Reservation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Reservation")
            .field("base", &self.base.as_ptr())
            .field("len", &self.len)
            .field("reservation", &self.reservation.as_ptr())
            .field("reservation_len", &self.reservation_len)
            .field("align", &self.align)
            .field("granted_huge", &self.granted_huge)
            .finish()
    }
}

impl Reservation {
    /// The aligned usable base of this span. Non-null, aligned to the `align`
    /// requested at reservation.
    ///
    /// **Validity scope:** Valid for [`len()`](Self::len) bytes, with the
    /// following exceptions:
    ///
    /// - **Decommitted ranges:** Ranges decommitted via the free functions or
    ///   safe methods and not yet recommitted have platform-specific behavior:
    ///   - **Windows**: pages are unmapped until `recommit`; access before
    ///     `recommit` crashes with `STATUS_ACCESS_VIOLATION`.
    ///   - **Linux (eager `decommit`)**: pages are zeroed on next access via
    ///     `MADV_DONTNEED`.
    ///   - **Linux (lazy `decommit_lazy`)**: pages keep old contents until kernel
    ///     reclaims them under pressure; writes before reclamation cancel the free.
    ///   - **Darwin/BSD**: pages keep old contents; `MADV_DONTNEED` is
    ///     advisory-only and does not reliably zero.
    ///   - **Huge reservations, `decommit_lazy` (both layers), and `decommit`/
    ///     `try_decommit` on Windows or on a non-huge-page-aligned range on
    ///     Linux/Android**: old contents remain. The safe
    ///     methods [`Self::decommit`]/[`Self::decommit_lazy`] skip the backend
    ///     call outright in this case (they can consult [`Self::is_huge`] and,
    ///     for `decommit`, the requested range); the free
    ///     functions cannot consult [`Self::is_huge`], so they still issue the
    ///     syscall — which the OS then refuses or ignores. Same observable
    ///     outcome, different mechanism; do not read "no-op" as "no syscall"
    ///     for the free functions.
    ///   - **Huge reservations, eager `decommit`/`try_decommit`, Linux/Android
    ///     kernel >= 5.18, range aligned to the huge page size (2 MiB) at both
    ///     endpoints (task #1140):** the one huge-page case where decommit
    ///     actually works — pages ARE zeroed on next access via
    ///     `MADV_DONTNEED`. Both layers issue the real syscall here and agree.
    ///     See [`Self::decommit`]'s own doc for the exact eligibility rule.
    ///
    /// - **Lazy reservations on Windows (feature `lazy-commit`):** When created
    ///   via `reserve_aligned_lazy`, only the `initial_commit` prefix is
    ///   committed at reservation time. The tail `[initial_commit, len())` must
    ///   be committed via `commit_range` before it becomes writable. Writing
    ///   to the uncommitted tail results in an access violation.
    ///
    /// Returns `*mut u8` (rather than the std convention of `*const T` from
    /// `&self`) because a raw pointer carries no borrow obligation in this
    /// crate's model, and the span is exclusively owned by this `Reservation`
    /// handle. The mutability reflects ownership, not mutability of the
    /// borrow itself.
    #[must_use]
    #[inline]
    pub fn as_ptr(&self) -> *mut u8 {
        self.base.as_ptr()
    }

    /// The number of usable bytes at [`as_ptr`](Self::as_ptr).
    #[must_use]
    #[inline]
    #[allow(clippy::len_without_is_empty)]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// The start of the underlying OS reservation (may sit below
    /// [`as_ptr`](Self::as_ptr) because the reservation is over-reserved
    /// to achieve alignment and the full mapping is kept).
    #[must_use]
    #[inline]
    pub fn reservation_ptr(&self) -> *mut u8 {
        self.reservation.as_ptr()
    }

    /// The **requested/logical** span length of this reservation.
    ///
    /// **This value is NOT necessarily the actual OS reservation size** — at least
    /// three paths under-report the true VA span the OS mapped:
    ///
    /// - **Windows single-call fast path** (`align <= 64 KiB`): this returns
    ///   `commit_len` (which equals `size`), not the rounded-up VA reservation
    ///   size. Windows rounds VA reservations up to the 64 KiB allocation
    ///   granularity internally, so `reserve_aligned(4096, 4096)` reports
    ///   `reservation_len() == 4096` while actually consuming 64 KiB of address
    ///   space.
    /// - **Windows two-call path's fast-reserve sub-path** (`align <= 64 KiB`
    ///   via `reserve_aligned_lazy`): when the candidate `VirtualAlloc(NULL,
    ///   size, MEM_RESERVE)` happens to be aligned, this returns `size` directly,
    ///   not the rounded-up 64 KiB granularity. The underlying reservation still
    ///   consumes a 64 KiB-granular region.
    /// - **Any page-rounding `mmap` where the OS page size exceeds the requested
    ///   granularity** — e.g. Apple-Silicon macOS's 16 KiB pages, or 64 KiB on
    ///   some Linux configurations (see [`MIN_PAGE`](crate::min_page::MIN_PAGE)'s doc above): `mmap` rounds
    ///   `length` up to the page size, so `reserve_aligned(PAGE, PAGE)` on a 16
    ///   KiB-page host actually maps a full 16 KiB page while this returns
    ///   `4096`.
    ///
    /// Both cases are harmless for correctness (`VirtualFree(base, 0,
    /// MEM_RELEASE)` ignores the length argument; `munmap` rounds its length
    /// argument up to the page size the same way `mmap` did, so `release`
    /// still unmaps the whole underlying mapping) — but the return value is
    /// not a portable measure of the true reservation size.
    #[must_use]
    #[inline]
    pub const fn reservation_len(&self) -> usize {
        self.reservation_len
    }

    // Historical note (task #848, #921): the Windows single-call fast path
    // (align <= WIN_ALLOCATION_GRANULARITY, typically 64 KiB; widens to
    // GetLargePageMinimum() when requesting large pages) and the two-call
    // path's fast-reserve sub-path (align <= WIN_ALLOCATION_GRANULARITY
    // via reserve_aligned_lazy) are the primary under-report cases for
    // this method; the page-rounding mmap case is the third. These are
    // documented in the method's rustdoc above without task-number references.

    /// The alignment requested at reservation time.
    #[must_use]
    #[inline]
    pub const fn align(&self) -> usize {
        self.align
    }

    /// Whether OS large/huge pages were actually granted for this reservation.
    ///
    /// Returns `true` if the reservation successfully obtained large/huge pages
    /// from the OS (Linux `MAP_HUGETLB` or Windows `MEM_LARGE_PAGES`), and `false`
    /// if it fell back to ordinary pages or was not a huge-page request.
    ///
    /// This is the "best-effort" observable: a caller using `reserve_aligned_huge`
    /// can now detect whether the huge-page feature actually engaged, rather than
    /// receiving only an indistinguishable `Ok(Reservation)` on every fallback.
    ///
    /// **Windows limitation (task #848 single-call fast path):** on Windows,
    /// this returns `true` only when ALL of the following hold:
    /// 1. The fast-path condition `align <= GetLargePageMinimum()` is satisfied
    ///    (typically `align <= 2 MiB` on x86_64)
    /// 2. `size` is a multiple of the system's large-page minimum
    /// 3. The calling process has `SeLockMemoryPrivilege` granted AND has
    ///    **enabled** it via `AdjustTokenPrivileges` (the crate does not do
    ///    this for you — a process with the privilege granted but not
    ///    enabled fails exactly like an unprivileged one and silently falls
    ///    back to ordinary pages)
    ///
    /// NOTE: The widened fast-path condition (II-3, 2026-08-16 audit finding) expanded
    /// the single-call ATTEMPT window from `align <= 64 KiB` to `align <= GetLargePageMinimum()`,
    /// but on an unprivileged host the actual paths that SUCCEED (pass the post-call alignment
    /// check) are typically still limited. When large pages are NOT granted (unprivileged),
    /// `VirtualAlloc`'s alignment guarantee is only 64 KiB; in practice it typically does NOT
    /// happen to land on the requested alignment, so the post-call check fails and the fast
    /// path falls through to the two-call path. Practically, this means `is_huge() == true` only
    /// for shapes where large pages are actually granted, which requires all three conditions
    /// above to hold.
    ///
    /// If any of these conditions fail, the function falls back to ordinary pages
    /// and this flag is `false`. On Windows, large pages (`MEM_LARGE_PAGES`)
    /// are only ever requested and possibly granted via the single-call fast path;
    /// the two-call path never requests large pages, so
    /// `is_huge()` is always `false` for a reservation that takes it. See
    /// [`reserve_aligned_huge`](crate::api::reserve_aligned_huge)'s rustdoc for details.
    ///
    /// **Note:** reservations adopted via [`from_raw_parts`](Self::from_raw_parts)
    /// report whatever `granted_huge` value the caller passed to that constructor,
    /// which the caller is responsible for getting right (see that constructor's
    /// `# Safety` section).
    ///
    /// **This method has no `huge-pages` feature gate — [`Self::decommit`]'s
    /// eligible-forward behavior does (task #1156, finding F10).** `is_huge()`
    /// reports `true`/`false` identically regardless of which features are
    /// enabled; whether a `true` result also gets you a real Linux/Android
    /// kernel >= 5.18 decommit forward instead of a guaranteed skip depends
    /// on the `huge-pages` feature being enabled too. See [`Self::decommit`]'s
    /// doc for the full explanation — this asymmetry matters most for
    /// [`from_raw_parts`](Self::from_raw_parts) callers, since that
    /// constructor is also unconditionally compiled.
    #[must_use]
    #[inline]
    pub const fn is_huge(&self) -> bool {
        self.granted_huge
    }

    /// Returns `true` if the current platform's **ordinary native backend** guarantees
    /// that eager [`Self::decommit`] returns physical backing to the OS and zero-fills
    /// on next access, `false` otherwise.
    ///
    /// **Scope:** this is a platform-level query about the ordinary native backend's
    /// contract. It does NOT apply to:
    /// - **huge-page reservations** (those with [`Self::is_huge`] == `true`) — decommit
    ///   silently fails there (see the free [`decommit`] function's rustdoc for details).
    /// - **miri** — under miri, the backend is a no-op that doesn't model RSS or reclaim.
    /// - **the `aligned_vmem_mock` cfg** (`RUSTFLAGS="--cfg aligned_vmem_mock"`) — the
    ///   recording mock backend's decommit logs the call WITHOUT touching the OS, so it
    ///   reclaims nothing and zeroes nothing (task #1066). Excluded for the same reason
    ///   the sibling capability query `lazy_commit_is_honored()` (feature `lazy-commit`)
    ///   excludes it: this family answers for the backend actually linked into the
    ///   compilation, and the miri bullet above is already that same substituted-backend
    ///   category rather than a platform property.
    ///
    /// For an instance-level query that accounts for huge pages, use
    /// [`Self::can_decommit_reclaim_and_zero`].
    ///
    /// Platform behavior (ordinary native backend only, eager decommit path):
    /// - **Linux (all targets)**: returns `true`. `MADV_DONTNEED` unmaps physical pages
    ///   and re-faults fresh zero pages on next access.
    /// - **Windows**: returns `true`. `MEM_DECOMMIT` unmaps physical pages and
    ///   re-faults fresh zero pages on next access.
    /// - **Darwin family (macOS/iOS/tvOS/watchOS)**: returns `false`. `MADV_DONTNEED`
    ///   is advisory-only for anonymous memory and does not reliably unmap/zero pages.
    ///   A decommit+recommit roundtrip can observe old data still resident.
    /// - **BSD family (FreeBSD/DragonFly/NetBSD/OpenBSD)**: returns `false`. Same
    ///   advisory-only caveat as Darwin for eager decommit. (Note: lazy decommit
    ///   via [`decommit_lazy`] DOES reclaim on BSD via `MADV_FREE`, even though
    ///   eager decommit does not.)
    ///
    /// This is a compile-time constant per platform: the return value is the same
    /// for all calls within a single compilation unit, determined by the target
    /// OS triple, whether miri is active, and whether the `aligned_vmem_mock` recording
    /// backend is compiled in. It provides programmatic access to the
    /// platform-specific guarantee that [`Self::decommit`]'s rustdoc describes in prose.
    #[must_use]
    #[inline]
    pub const fn decommit_reclaims_and_zeroes() -> bool {
        cfg!(not(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_os = "watchos",
            target_os = "freebsd",
            target_os = "dragonfly",
            target_os = "netbsd",
            target_os = "openbsd",
            miri,
            aligned_vmem_mock
        )))
    }

    /// Returns `true` if eager [`Self::decommit`] on **this specific reservation**
    /// guarantees reclaim+zero-fill semantics, `false` otherwise.
    ///
    /// This is an **advisory** capability query. It is computed from
    /// compile-time platform capability and the reservation's huge-page status only,
    /// and does **not** issue any runtime syscall or observe whether a prior `decommit`
    /// call actually succeeded. Specifically:
    ///
    /// - On Linux/Windows (native — not miri, not the `aligned_vmem_mock` cfg), `true` means the platform guarantees that
    ///   `decommit` will return physical backing and zero-fill on next access via
    ///   `MADV_DONTNEED` / `MEM_DECOMMIT`. Backend syscall failures (e.g. rare kernel
    ///   failures) are silently discarded and not reflected in this query's return value.
    /// - On Darwin/BSDs, under miri, or under the `aligned_vmem_mock` cfg, `false` means
    ///   decommit is advisory-only (Darwin/BSDs) or a recorded no-op (miri, mock) with no
    ///   reclaim or zero-fill guarantee.
    /// - On huge-page reservations, `false` — **this bool is CONSERVATIVE and is NOT
    ///   range-aware** (task #1140): on Windows it is unconditionally correct (large-page
    ///   decommit never works there). On Linux/Android with kernel >= 5.18, it
    ///   UNDER-reports: [`Self::decommit`]/[`Self::try_decommit`] DO issue a real
    ///   `MADV_DONTNEED` for a `[start, end)` range that is itself huge-page-size-aligned
    ///   (2 MiB) at both endpoints — see those methods' own doc comments — but this
    ///   instance-level query has no `start`/`end` parameters to judge that per-call, so it
    ///   answers `false` for EVERY range on a huge reservation, including the ranges that
    ///   actually do work. Call [`Self::decommit`]/[`Self::try_decommit`] directly and judge
    ///   by their return value / the `bench-internals`
    ///   `huge_decommit_attempts` counter (not an intra-doc link: `bench-internals` is excluded from the published docs.rs feature set)
    ///   if you need to distinguish "this exact range worked" from "this bool said no."
    ///
    /// A `true` return is therefore a statement about the **platform and reservation type**,
    /// not a guarantee that a specific `decommit` call actually released memory or zeroed
    /// pages — OS errors in that path are unobservable through this API by design
    /// (the same contract as the infallible `decommit` method itself). A `false` return is
    /// similarly not a guarantee that no range on this reservation can ever be decommitted
    /// (see the huge-page bullet above).
    ///
    /// This query combines:
    /// - the platform-level guarantee (see [`Self::decommit_reclaims_and_zeroes`]), and
    /// - the reservation's huge-page status (via [`Self::is_huge`]).
    ///
    /// Returns `false` if EITHER condition fails:
    /// - the platform doesn't guarantee reclaim+zero-fill (Darwin/BSDs, miri, or the
    ///   `aligned_vmem_mock` cfg), or
    /// - this reservation uses huge pages — **conservatively**: on Windows this is
    ///   always correct (huge-page decommit is a genuine no-op there), but on
    ///   Linux/Android >= 5.18 a huge-page-size-aligned range CAN actually
    ///   decommit (see [`Self::decommit`]'s doc and the bullet on this fact
    ///   above); this bool has no range to judge, so it answers `false`
    ///   unconditionally for a huge reservation regardless of platform.
    ///
    /// Use this when you have an actual `Reservation` and need to know whether decommit
    /// will work on it **for an ordinary (non-huge) reservation, or to conservatively rule
    /// out a huge one**. Use the associated function [`Self::decommit_reclaims_and_zeroes`]
    /// when you only care about platform capability without a reservation instance. For a
    /// huge reservation on Linux/Android, this bool cannot tell you whether a SPECIFIC
    /// `[start, end)` will work — call [`Self::decommit`]/[`Self::try_decommit`] and judge
    /// by outcome instead (see the huge-page bullet above).
    ///
    /// # Example
    ///
    /// Ordinary reservation: decommit works on Linux/Windows (except miri):
    /// ```text
    /// let ordinary = reserve_aligned(1024 * 1024, 4096).expect("reserve");
    /// // On Linux/Windows (native): ordinary.can_decommit_reclaim_and_zero() == true
    /// // On Darwin/BSD, under miri, or under `aligned_vmem_mock`:
    /// // ordinary.can_decommit_reclaim_and_zero() == false
    /// ```
    ///
    /// Huge-page reservation: this bool is always false, but on Linux/Android
    /// >= 5.18 that does NOT mean `decommit` itself is a no-op for every range:
    /// ```text
    /// let huge = reserve_aligned_huge(2 * 1024 * 1024, 2 * 1024 * 1024);
    /// if let Some(ref reservation) = huge {
    ///     if reservation.is_huge() {
    ///         // The bool is always false, regardless of platform — conservative,
    ///         // not "decommit never works" (see the doc above this example).
    ///         assert!(!reservation.can_decommit_reclaim_and_zero());
    ///     }
    /// }
    /// ```
    /// NOTE: On Linux/Android with the `huge-pages` feature enabled, the
    /// arguments must be multiples of the huge page size (2 MiB); the example
    /// above uses 2 MiB for both size and align to avoid rejection. On other
    /// platforms, the function is a best-effort no-op and any size/align will
    /// succeed (falling back to ordinary pages).
    ///
    /// See the tests in `tests/decommit_capability.rs` for runnable coverage of both cases.
    #[must_use]
    #[inline]
    pub fn can_decommit_reclaim_and_zero(&self) -> bool {
        Self::decommit_reclaims_and_zeroes() && !self.is_huge()
    }

    /// Consume the handle WITHOUT releasing the OS reservation, returning the
    /// `(reservation_ptr, reservation_len, align)` the caller must later hand to
    /// [`release`](crate::api::release) exactly once. Use this when your allocator records the
    /// reservation in its own self-hosted metadata instead of relying on
    /// `Drop`.
    ///
    /// `align` is the alignment originally requested; the native release paths
    /// ignore it, but it is required for the miri fallback to reconstruct the
    /// exact `Layout`. A self-hosting allocator that always uses one alignment
    /// can pass that constant to [`release`](crate::api::release) instead of storing this value.
    ///
    /// **Warning:** This method returns a raw tuple. Consider using
    /// [`into_reservation_parts`](Self::into_reservation_parts) instead, which
    /// returns a named struct that prevents accidentally swapping `len` and `align`.
    #[must_use]
    pub fn into_parts(self) -> (*mut u8, usize, usize) {
        let parts = (self.reservation.as_ptr(), self.reservation_len, self.align);
        core::mem::forget(self);
        parts
    }

    /// Consume the handle WITHOUT releasing the OS reservation, returning the
    /// [`ReservationParts`] struct the caller must later hand to [`release_parts`](crate::api::release_parts)
    /// exactly once. Use this when your allocator records the reservation in its
    /// own self-hosted metadata instead of relying on `Drop`.
    ///
    /// This method is the typed, named alternative to [`into_parts`](Self::into_parts);
    /// it prevents the footgun of accidentally swapping `len` and `align`, which
    /// would be undefined behavior on the native backend and cause leaks or crashes
    /// on the Unix backend.
    ///
    /// **WARNING:** This method discards `base`, `len`, and `granted_huge`. To
    /// reconstruct a full `Reservation` via [`from_raw_parts`](Self::from_raw_parts),
    /// you MUST preserve these three fields separately alongside the returned
    /// `ReservationParts`. If you omit `granted_huge`, the reconstructed reservation
    /// will incorrectly report `is_huge() == false` even if the original used huge
    /// pages, which can lead to incorrect decommit-availability decisions.
    ///
    /// For backwards compatibility with code that already uses the tuple form,
    /// you can call [`ReservationParts::as_tuple`] to get a raw tuple.
    #[must_use]
    pub fn into_reservation_parts(self) -> ReservationParts {
        let parts = ReservationParts {
            ptr: self.reservation.as_ptr(),
            len: self.reservation_len,
            align: self.align,
        };
        // Same suppression as `into_parts` -- without this, `self` would run
        // its normal `Drop` (which now also releases the OS reservation) at
        // the end of this function, and the returned `ReservationParts`
        // would describe already-freed memory: a guaranteed double-free the
        // moment the caller follows this method's own contract and passes
        // it to `release_parts`.
        core::mem::forget(self);
        parts
    }

    /// Consume the handle WITHOUT releasing the OS reservation, returning a
    /// full [`ReservationFullParts`] struct containing all six fields needed to
    /// reconstruct the original `Reservation` via [`from_raw_parts`](Self::from_raw_parts).
    ///
    /// This is the lossless round-trip alternative to [`into_reservation_parts`](Self::into_reservation_parts):
    /// it preserves `base`, `len`, and `granted_huge` in addition to the underlying
    /// reservation metadata, eliminating the risk of silent huge-page status loss
    /// or usable-span information loss.
    ///
    /// Use this when you need to temporarily extract all reservation state for
    /// later reconstruction, such as in a custom allocator that hands off
    /// reservations between components within the same process.
    ///
    /// **IMPORTANT:** `ReservationFullParts` is a plain struct with no `Drop`
    /// implementation — dropping or forgetting it does NOT release the underlying
    /// OS reservation. The reservation will leak until you reconstruct it via
    /// `into_reservation()` and drop the resulting `Reservation`, or release it
    /// manually via [`release`](crate::api::release) (using the `reservation`, `reservation_len`, and
    /// `align` fields from `ReservationFullParts`). If you only need manual
    /// release and don't require preserving `base`, `len`, and `granted_huge`,
    /// prefer [`into_reservation_parts`](Self::into_reservation_parts) instead,
    /// which provides the `release_parts` function.
    #[must_use]
    pub fn into_full_parts(self) -> ReservationFullParts {
        let parts = ReservationFullParts {
            base: self.base.as_ptr(),
            len: self.len,
            reservation: self.reservation.as_ptr(),
            reservation_len: self.reservation_len,
            align: self.align,
            granted_huge: self.granted_huge,
        };
        core::mem::forget(self);
        parts
    }

    /// Decommit pages `[start, end)` within this reservation.
    ///
    /// This is the safe, bounds-checked alternative to the free [`decommit`]
    /// function for callers already holding a `Reservation`. It delegates to
    /// the underlying implementation with `self.as_ptr()` as base and
    /// automatically ensures `[start, end)` is within the reservation's usable span.
    ///
    /// Takes `&mut self` (task #1113): OS-state mutation requires exclusive
    /// access, so a shared `&Reservation` can reach none of the seven state
    /// mutators. This is what structurally seals the `LazyReservation`
    /// watermark (finding H1, task #1104): a leaked `&Reservation` is now
    /// read-only by construction, not by policing.
    ///
    /// **Programmatically check platform guarantees:** use
    /// [`Self::decommit_reclaims_and_zeroes`] to query whether the current
    /// platform guarantees reclaim+zero-fill semantics.
    ///
    /// Hint the OS to return the physical backing of `[start, end)` while keeping the
    /// address-space reservation alive. On Linux and Windows this is guaranteed to
    /// return physical backing and zero-fill on next access (Linux `MADV_DONTNEED`,
    /// Windows `MEM_DECOMMIT`). On the Darwin family (macOS/iOS/tvOS/watchOS) and the
    /// four BSDs (FreeBSD/DragonFly/NetBSD/OpenBSD), this is a best-effort hint with no
    /// zero-fill or reclaim guarantee — the physical pages may remain resident and
    /// old data may be observed after a decommit+recommit roundtrip.
    ///
    /// `start` and `end` must be multiples of the runtime page size ([`page_size()`](crate::page_size::page_size)).
    /// A no-op if the range is out of bounds (`end > self.len()`); an empty
    /// range is a no-op only when page-ALIGNED — an empty MISALIGNED range
    /// (`start == end`, endpoints not page multiples, e.g. `decommit(1, 1)`)
    /// is a contract violation like any other, with the SAME profile split
    /// as every other violated range: a silent no-op in a RELEASE build
    /// (the forwarded free function returns at `start >= end` once the
    /// `debug_assert!` is compiled out) and a tripwire panic in a DEBUG
    /// build — EXCEPT on a huge-page reservation, where a NON-huge-aligned
    /// (or inverted) range never reaches the forward at all, so it is a
    /// silent no-op on EVERY profile there and the debug tripwire never
    /// fires (see `# Panics`; task #1084/M2 wrote the split into `# Panics`,
    /// task #1097/L4 qualified this summary line to match, task #1108 added
    /// the huge exception that the paragraph below and `# Panics` both
    /// already stated but this sentence did not; task #1140 narrowed the
    /// huge exception to "non-huge-aligned or inverted" — see below).
    ///
    /// **Contract violations, by build profile (task #1051, narrowed task
    /// #1140):** this method forwards to the free [`decommit`] function
    /// UNFILTERED whenever it forwards at all, so a violated range
    /// (`start > end`, or an endpoint not a multiple of
    /// [`page_size()`](crate::page_size::page_size)) follows that function's
    /// documented profile split exactly on a NON-huge reservation — a silent
    /// no-op in a RELEASE build (no OS call, nothing recorded), a tripwire
    /// panic in a DEBUG build. On a HUGE-page reservation
    /// ([`Self::is_huge`] == `true`), whether this method forwards at all
    /// now depends on the range (task #1140, Linux/Android kernel >= 5.18
    /// only): a WELL-FORMED range that is ALSO aligned to the huge page size
    /// (2 MiB) at both endpoints forwards to the real backend exactly like a
    /// non-huge reservation would (and can therefore reach that same debug
    /// tripwire, only for a range that manages to be simultaneously
    /// huge-aligned AND page-size-misaligned — impossible in practice since
    /// 2 MiB is already page-size-aligned on every supported page size, so
    /// this case cannot actually occur); every OTHER range on a huge
    /// reservation (not huge-aligned, or `start > end`) never reaches the
    /// forward and is a silent no-op on every profile (see `# Panics`).
    /// [`Self::try_decommit`] is the fallible form: it reports a violated
    /// range as `Err` on every profile — including huge reservations (task
    /// #1084/M3) — and never trips the tripwire.
    ///
    /// See [`decommit`] for platform divergence notes (Windows crashes on write
    /// before recommit, Linux and Android do not), huge-page incompatibility,
    /// and Darwin zero-fill caveats. Under the `bench-internals` feature, the
    /// `huge_decommit_attempts` counter (not an intra-doc link: `bench-internals` is excluded from the published docs.rs feature set) is incremented when decommit
    /// is called on a huge-page reservation with a range that is NOT eligible
    /// for the Linux/Android >= 5.18 huge-aligned real-call path (i.e. the
    /// counter tracks calls that hit the silent-no-op path, not every call on
    /// a huge reservation — task #1140 narrowed this from "every huge-reservation
    /// call" to "every huge-reservation call that is actually skipped").
    ///
    /// **The Linux/Android kernel >= 5.18 eligible-forward path itself
    /// requires the `huge-pages` feature (task #1156, finding F10) —
    /// [`Self::is_huge`] does NOT.** The eligibility check this method
    /// consults (`linux_huge_range_is_madvise_eligible`) is compiled only
    /// under `#[cfg(all(not(miri), not(aligned_vmem_mock),
    /// any(target_os = "linux", target_os = "android"),
    /// feature = "huge-pages"))]`; without that
    /// feature enabled, EVERY range on a huge-flagged reservation takes the
    /// silent-no-op early-exit above, unconditionally, on every platform —
    /// there is no huge-aligned-range exception without the feature.
    /// **[`Self::from_raw_parts`] no longer creates a mismatch here (task
    /// #1172/M1-hybrid, closing finding M2):** that constructor now
    /// `assert!`s that `granted_huge: true` requires the `huge-pages` feature
    /// to be enabled in THIS crate, so a reservation with `is_huge() == true`
    /// cannot exist without `huge-pages` — the scenario this paragraph used
    /// to describe (an adopted `MAP_HUGETLB` reservation reporting
    /// `is_huge() == true` while `decommit`/`try_decommit` silently and
    /// permanently skip the backend regardless of range or kernel version,
    /// because the CONSUMER's Cargo feature set diverged from what the flag
    /// promised) can no longer arise: without `huge-pages`, adopting such a
    /// reservation panics at construction instead of silently under-serving
    /// it later. **[`decommit_lazy`] is NOT a
    /// workaround** (task #1172, correcting the advice this paragraph used
    /// to give): [`Self::decommit_lazy`] skips its backend call
    /// UNCONDITIONALLY for every huge-flagged reservation, on every
    /// platform, regardless of feature flags or kernel version — it has no
    /// Linux >= 5.18 huge-aligned carve-out at all (see its own doc). Routing
    /// around a `huge-pages`-gated no-op into a permanent no-op is not a
    /// substitute. If you cannot enable `huge-pages`, either accept the
    /// no-op (RSS will not drop for this reservation) or track the huge-page
    /// state yourself and avoid relying on either decommit path for it.
    ///
    /// # Panics
    ///
    /// DEBUG builds only, and only for a contract-violating range (`start >
    /// end`, or an endpoint not a multiple of the runtime
    /// [`page_size()`](crate::page_size::page_size)) that actually reaches the
    /// forwarded free [`decommit`]: unconditionally true on a NON-huge
    /// reservation, or — since task #1140 — on a huge reservation whenever the
    /// range happens to be huge-page-size-aligned at both endpoints (in
    /// practice this can only be a WELL-FORMED range, since a huge-page-size
    /// multiple is always also a `page_size()` multiple, so the tripwire is
    /// not actually reachable through the huge-aligned path — this bullet
    /// exists to be precise about the forwarding rule, not because a real
    /// input triggers it). That includes an EMPTY MISALIGNED range such as
    /// `decommit(1, 1)` — emptiness is NOT a pre-check (task #1084, finding
    /// M2, rewrote this section, which previously claimed "empty and
    /// out-of-bounds ranges are checked by this method first and never
    /// panic"; only the out-of-bounds half of that sentence was true). The
    /// two classes that never panic on any profile: out-of-bounds
    /// (`end > self.len()`), the one range class this method itself
    /// pre-checks, and an empty PAGE-ALIGNED range (`start == end`, both
    /// endpoints multiples of `page_size()`), which forwards as
    /// well-formed. On a huge-page reservation ([`Self::is_huge`] == `true`)
    /// a range that is NOT huge-page-size-aligned at both endpoints (or is
    /// inverted, `start > end`) never reaches the tripwire: it is a silent
    /// no-op there on every profile, same as before task #1140. RELEASE
    /// builds silently skip a violated range regardless of huge-page status.
    /// This is the free function's own documented panic surface reached
    /// through the safe method, not a new one (task #1079 added this
    /// `# Panics` section to a doc that previously promised "the same
    /// silent-skip behavior as the free `decommit` function" with no profile
    /// qualifier; task #1084 corrected its empty-range claim; task #1140
    /// narrowed the huge-page exception).
    pub fn decommit(&mut self, start: usize, end: usize) {
        // Bounds check: the range must be within the reservation's usable span.
        if end > self.len() {
            return;
        }
        // Huge-page reservations (finding R6-7, revised task #1140): on Windows,
        // decommit NEVER works — `VirtualFree(MEM_DECOMMIT)` unconditionally fails
        // on a large-page region, full stop, so the backend call is skipped
        // unconditionally there. On Linux/Android, that used to be believed true
        // unconditionally too, but it is not: Linux 5.18+ added `MADV_DONTNEED`
        // support for HugeTLB mappings, gated on the address/length both being
        // aligned to the mapping's huge page size (`man 2 madvise`). This
        // reservation's `base` is always huge-page-aligned by construction
        // whenever `is_huge()` (see `linux_huge_range_is_madvise_eligible`'s own
        // doc), so only `[start, end)` needs checking. See `try_decommit`'s doc
        // for why an eligible-but-malformed range is still handled by validation,
        // not by this eligibility check.
        //
        // The `if` itself is UNCONDITIONAL and only the diagnostic increment is
        // feature-gated. Putting the whole block (and therefore the `return`)
        // behind `#[cfg(feature = "bench-internals")]` would confine the
        // optimisation to diagnostic builds and leave the useless syscall in
        // every production build — the exact inverse of the point — while also
        // making an observable behaviour (syscall issued or not) depend on a
        // feature flag. Caught at review of task #1040's delegated diff, which
        // had exactly that shape.
        if self.is_huge() {
            #[cfg(all(
                not(miri),
                not(aligned_vmem_mock),
                any(target_os = "linux", target_os = "android"),
                feature = "huge-pages"
            ))]
            if crate::os::linux_huge_range_is_madvise_eligible(start, end) {
                // SAFETY: `self.as_ptr()` is a valid reservation base, and
                // we've just verified `[start, end)` is within `self.len()`;
                // the free function's own contract is validated inside it.
                // The huge-decommit-eligibility check above is this method's
                // own addition on top of that contract.
                unsafe { decommit(self.as_ptr(), start, end) };
                return;
            }
            // Counts calls that hit this early-exit path; the increment is a
            // single relaxed fetch_add and compiles out when the feature is off.
            #[cfg(feature = "bench-internals")]
            HUGE_DECOMMIT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
            return;
        }
        // SAFETY: `self.as_ptr()` is a valid reservation base, and we've just
        // verified `[start, end)` is within `self.len()`. The free function's
        // own contract (multiples of page_size(), etc.) is validated inside it.
        unsafe { decommit(self.as_ptr(), start, end) };
    }

    /// Fallible [`Self::decommit`]: `Ok(())` on success (or a well-formed
    /// no-op — an empty page-aligned range), `Err(VmemError::invalid_argument())`
    /// if the offsets violated the contract (misaligned, `start > end`, or
    /// `end > self.len()`) — on EVERY reservation kind, huge included
    /// (task #1084/M3: the huge-page skip used to sit ahead of validation
    /// and answer `Ok(())` for a malformed range on a huge reservation,
    /// disagreeing with both this promise and the free [`try_decommit`]'s
    /// validate-first order). Never panics on any build profile: the
    /// violation is rejected here, before the eager path's tripwire can
    /// see it.
    ///
    /// This is the safe, bounds-checked alternative to the free [`try_decommit`]
    /// function for callers already holding a `Reservation` — and the form to
    /// reach for when [`Self::decommit`]'s DEBUG-build tripwire is itself
    /// unwelcome. Until task #1079 this was the one fallible pair with no
    /// safe-method twin: `recommit`/`try_recommit` and `commit_range`/
    /// `try_commit_range` already existed at both layers, and
    /// [`Self::decommit`]'s forwarded tripwire message ("Use try_decommit
    /// for the fallible form") pointed safe-API callers straight at an
    /// `unsafe fn` with a raw-pointer signature.
    ///
    /// Note what is deliberately NOT an error (mirroring the free
    /// [`try_decommit`]): the OS refusing or ignoring the request, and —
    /// FOR A WELL-FORMED RANGE — a huge-page reservation: on Windows, or on a
    /// Linux/Android range that is NOT huge-page-size-aligned at both
    /// endpoints, this method skips the backend call entirely, same as
    /// [`Self::decommit`], incrementing the same `bench-internals`
    /// `huge_decommit_attempts` counter (not an intra-doc link: `bench-internals` is excluded from the published docs.rs feature set) and returning `Ok(())`.
    /// On Linux/Android kernel >= 5.18 (task #1140), a well-formed range that
    /// IS huge-page-size-aligned at both endpoints instead forwards to the
    /// real backend (same as a non-huge reservation) and returns whatever
    /// that call reports — still `Ok(())` on any OS-level refusal, per the
    /// "best-effort" note below, but now backed by a real attempt rather than
    /// a guaranteed skip. Either way, `Ok(())` never distinguishes "skipped"
    /// from "attempted" — see [`Self::decommit`]'s doc for how to tell them
    /// apart if needed. A malformed range is `Err` even on a huge
    /// reservation: validation runs before the skip/forward decision, so
    /// neither the counter nor the real backend ever sees a malformed range
    /// (task #1084/M3).
    /// Decommit is best-effort by nature; use
    /// [`Self::decommit_reclaims_and_zeroes`] to learn what the platform
    /// actually does.
    ///
    /// **The Linux/Android >= 5.18 eligible-forward path requires the
    /// `huge-pages` feature (task #1156, finding F10); [`Self::is_huge`]
    /// does not — it is a pure query, always compiled.** Without
    /// `huge-pages` enabled, this method always takes the
    /// skip-and-return-`Ok(())` path on a huge-flagged reservation, on every
    /// platform, regardless of range or kernel version. **A huge-flagged
    /// reservation cannot even be constructed without `huge-pages`, as of
    /// task #1172/M1-hybrid** — [`Self::from_raw_parts`] now requires the
    /// feature to accept `granted_huge: true` — see [`Self::decommit`]'s doc
    /// and [`Self::from_raw_parts`]'s "Correctness contract" section for the
    /// full explanation.
    pub fn try_decommit(&mut self, start: usize, end: usize) -> Result<(), VmemError> {
        // Bounds check: the range must be within the reservation's usable span.
        if end > self.len() {
            return Err(VmemError::invalid_argument());
        }
        // Range-contract validation BEFORE the huge-page skip (task #1084,
        // finding M3). The huge early-return below used to sit ahead of ALL
        // validation, so on a reservation with `is_huge() == true` a
        // malformed range — the exact input a caller uses the fallible form
        // to detect — was answered `Ok(())`, contradicting both this
        // method's own `Err` contract and the free `try_decommit`'s
        // validate-first order. The three conditions mirror the free
        // function's private `decommit_range_is_well_formed`
        // (`api/decommit.rs`), which re-checks after the forward, so the
        // two layers cannot drift apart silently —
        // `method_try_decommit_reports_malformed_range_on_huge_flagged_
        // reservation` and `method_try_decommit_reports_violations_and_
        // never_panics` (tests/reservation_decommit_contract.rs) pin the
        // agreement, on huge-flagged and ordinary reservations
        // respectively.
        let ps = page_size_or_poison();
        // Failed OS page-size query: fail closed with the OS-side no-code
        // error, BEFORE the huge skip and the range validation — mirroring
        // the free `try_decommit` (NOT `invalid_argument`; the caller's
        // arguments are not at fault). See `page_size`'s "If the one-time
        // OS query fails" paragraph.
        if ps == PAGE_SIZE_QUERY_FAILED {
            return Err(VmemError::os_refusal_unknown_code());
        }
        if start > end || !start.is_multiple_of(ps) || !end.is_multiple_of(ps) {
            return Err(VmemError::invalid_argument());
        }
        // Huge-page reservations (finding R6-7, revised task #1140): a
        // WELL-FORMED range that reaches this point (validated above) is still
        // not guaranteed to actually decommit anything — see `Self::decommit`'s
        // doc comment for the Windows-vs-Linux/Android split this mirrors.
        // `Ok(())` is the honest answer either way: whether the backend call is
        // skipped (no-op) or actually issued (Linux/Android 5.18+, huge-aligned
        // range), this method's contract is "the range was well-formed", not
        // "the OS actually reclaimed anything" — the free `try_decommit`
        // deliberately does not report OS refusal/ignore as an error, and this
        // decision applies equally to "the OS was never even asked" (Windows,
        // or a page-size-but-not-huge-size-granular range on Linux/Android) and
        // "the OS was asked and may have silently declined" (any ordinary
        // reservation). Pinned in the contract, not left implicit: this method
        // is fallible only on MALFORMED input, never on best-effort OS outcome.
        if self.is_huge() {
            #[cfg(all(
                not(miri),
                not(aligned_vmem_mock),
                any(target_os = "linux", target_os = "android"),
                feature = "huge-pages"
            ))]
            if crate::os::linux_huge_range_is_madvise_eligible(start, end) {
                // SAFETY: `self.as_ptr()` is a valid reservation base, and
                // `[start, end)` was just validated as well-formed and in-span.
                return unsafe { try_decommit(self.as_ptr(), start, end) };
            }
            // Same reasoning and same cfg placement rule as `Self::decommit`
            // above — the `if`/`return` are unconditional, only the counter is
            // gated.
            #[cfg(feature = "bench-internals")]
            HUGE_DECOMMIT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        // SAFETY: `self.as_ptr()` is a valid reservation base, and we've just
        // verified `[start, end)` is within `self.len()`. The free function
        // re-validates the page-size contract itself and reports a violation
        // as `Err` — which is exactly why this method never needs to panic.
        unsafe { try_decommit(self.as_ptr(), start, end) }
    }

    /// Lazy decommit variant: hint the OS it MAY reclaim `[start, end)` under memory
    /// pressure, cheaper than [`Self::decommit`] (Linux `MADV_FREE`, macOS/iOS
    /// `MADV_FREE_REUSABLE`, FreeBSD/DragonFly `MADV_FREE`, NetBSD/OpenBSD
    /// `MADV_FREE`, other Unix (including tvOS/watchOS) falls back to `MADV_DONTNEED`;
    /// Windows falls back to the eager [`Self::decommit`] path, which has no lazy equivalent).
    ///
    /// This is the safe, bounds-checked alternative to the free [`decommit_lazy`]
    /// function for callers already holding a `Reservation`. It delegates to the
    /// underlying implementation with `self.as_ptr()` as base and automatically
    /// ensures `[start, end)` is within the reservation's usable span.
    ///
    /// `start` and `end` must be multiples of the runtime page size
    /// ([`page_size()`](crate::page_size::page_size)); an empty or
    /// out-of-bounds (`end > self.len()`) range is a no-op, and a VIOLATED
    /// range (`start > end`, or a misaligned endpoint) is a silent no-op on
    /// EVERY build profile — the deliberate eager/lazy asymmetry settled by
    /// task #1072: the eager [`Self::decommit`] trips a debug-build
    /// tripwire, this lazy variant has none on any profile.
    ///
    /// See [`decommit_lazy`] for the platform-specific cost inversion on macOS/iOS
    /// (this variant actually drops RSS immediately there, unlike the eager path)
    /// and other caveats. Under the `bench-internals` feature, the
    /// `huge_decommit_attempts` counter (not an intra-doc link: `bench-internals` is excluded from the published docs.rs feature set) is incremented when decommit is called
    /// on a huge-page reservation (same logic as `Self::decommit`).
    pub fn decommit_lazy(&mut self, start: usize, end: usize) {
        // Bounds check: the range must be within the reservation's usable span.
        if end > self.len() {
            return;
        }
        // Huge-page reservations: skip the backend call entirely (finding R6-7).
        // Same reasoning, same cfg placement rule as `Self::decommit` above —
        // the `if`/`return` are unconditional, only the counter is gated.
        //
        // Deliberately NOT extended to match `Self::decommit`'s task #1140
        // Linux-5.18+ carve-out: `man 2 madvise` documents `MADV_DONTNEED`
        // gaining HugeTLB support in 5.18, but says nothing of the kind for
        // `MADV_FREE` (the backend `decommit_lazy` uses) — the lazy advice
        // family is a different kernel code path with its own support
        // history, and this crate does not assume one advice value's support
        // change implies another's without a citation. Windows large pages
        // remain a `MEM_DECOMMIT` no-op unconditionally either way (there is
        // no lazy/eager split on Windows — see this method's own rustdoc).
        if self.is_huge() {
            #[cfg(feature = "bench-internals")]
            HUGE_DECOMMIT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
            return;
        }
        // SAFETY: same safety argument as `decommit` above.
        unsafe { decommit_lazy(self.as_ptr(), start, end) };
    }

    /// Recommit pages `[start, end)` previously passed to [`Self::decommit`].
    ///
    /// This is the safe, bounds-checked alternative to the free [`recommit`]
    /// function for callers already holding a `Reservation`. It delegates to
    /// the underlying implementation with `self.as_ptr()` as base and automatically
    /// ensures `[start, end)` is within the reservation's usable span.
    ///
    /// Returns `true` if the range is now committed (or the call was a well-formed
    /// no-op — an empty PAGE-ALIGNED range, `start == end`), and `false` if the
    /// OS refused to
    /// commit the pages (commit-charge exhaustion / true OOM) OR the offsets
    /// violated the contract below. On `false` the caller MUST NOT write into
    /// `[start, end)`. Never panics. For the cause use [`Self::try_recommit`].
    ///
    /// `start` and `end` must be multiples of the runtime page size ([`page_size()`](crate::page_size::page_size)).
    /// A well-formed no-op (an empty PAGE-ALIGNED range, `start == end`)
    /// returns `true`; any other contract violation (misaligned, or
    /// `start > end`, or `end > self.len()`) returns `false`.
    #[must_use]
    pub fn recommit(&mut self, start: usize, end: usize) -> bool {
        // Bounds check: the range must be within the reservation's usable span.
        if end > self.len() {
            return false;
        }
        // SAFETY: `self.as_ptr()` is a valid reservation base, and we've just
        // verified `[start, end)` is within `self.len()`. The free function's
        // own contract (multiples of page_size(), etc.) is validated inside it.
        unsafe { recommit(self.as_ptr(), start, end) }
    }

    /// Fallible [`Self::recommit`]: `Ok(())` if the range is now committed
    /// (or was a well-formed no-op), `Err(VmemError::invalid_argument())` if the
    /// offsets violated the contract (misaligned, or `start > end`, or `end > self.len()`),
    /// `Err(VmemError)` carrying the OS cause on genuine commit failure.
    ///
    /// This is the safe, bounds-checked alternative to the free [`try_recommit`]
    /// function for callers already holding a `Reservation`.
    pub fn try_recommit(&mut self, start: usize, end: usize) -> Result<(), VmemError> {
        // Bounds check: the range must be within the reservation's usable span.
        if end > self.len() {
            return Err(VmemError::invalid_argument());
        }
        // SAFETY: same safety argument as `recommit` above.
        unsafe { try_recommit(self.as_ptr(), start, end) }
    }

    /// Commit pages `[start, end)` within this reservation.
    ///
    /// This is the safe, bounds-checked alternative to the free [`commit_range`]
    /// function for callers already holding a `Reservation`. It delegates to
    /// the underlying implementation with `self.as_ptr()` as base and automatically
    /// ensures `[start, end)` is within the reservation's usable span.
    ///
    /// After a [`reserve_aligned_lazy`](crate::api::reserve_aligned_lazy) call that left some pages reserved-but-uncommitted,
    /// `commit_range` commits exactly the requested sub-range so it becomes writable.
    ///
    /// Returns `true` if the range is now committed, `false` if the OS refused
    /// (commit-charge exhaustion / true OOM) OR the offsets violated the contract
    /// above. On `false` the caller MUST NOT write into the range. Never panics.
    /// For the cause use [`Self::try_commit_range`].
    ///
    /// `start` and `end` must be multiples of the runtime page size ([`page_size()`](crate::page_size::page_size)).
    /// A well-formed no-op (an empty PAGE-ALIGNED range, `start == end`)
    /// returns `true`; any other contract violation (misaligned, or
    /// `start > end`, or `end > self.len()`) returns `false`.
    #[must_use]
    #[cfg(feature = "lazy-commit")]
    #[cfg_attr(docsrs, doc(cfg(feature = "lazy-commit")))]
    pub fn commit_range(&mut self, start: usize, end: usize) -> bool {
        // Bounds check: the range must be within the reservation's usable span.
        if end > self.len() {
            return false;
        }
        // SAFETY: same safety argument as `recommit` above.
        unsafe { commit_range(self.as_ptr(), start, end) }
    }

    /// Fallible [`Self::commit_range`]: `Ok(())` on success (or was a well-formed no-op),
    /// `Err(VmemError::invalid_argument())` if the offsets violated the contract
    /// (misaligned, or `start > end`, or `end > self.len()`), `Err(VmemError)` carrying
    /// the OS cause on genuine commit failure.
    ///
    /// This is the safe, bounds-checked alternative to the free [`try_commit_range`]
    /// function for callers already holding a `Reservation`.
    #[cfg(feature = "lazy-commit")]
    #[cfg_attr(docsrs, doc(cfg(feature = "lazy-commit")))]
    pub fn try_commit_range(&mut self, start: usize, end: usize) -> Result<(), VmemError> {
        // Bounds check: the range must be within the reservation's usable span.
        if end > self.len() {
            return Err(VmemError::invalid_argument());
        }
        // SAFETY: same safety argument as `recommit` above.
        unsafe { try_commit_range(self.as_ptr(), start, end) }
    }

    /// Wrap a pre-existing OS reservation (e.g. one obtained from
    /// `VirtualAllocExNuma` or another platform-specific allocator that
    /// `reserve_aligned` does not call directly) in a [`Reservation`] handle.
    ///
    /// The handle then participates in the normal RAII lifecycle: on `Drop`
    /// (or via [`release`](crate::api::release)) the underlying reservation is returned to the OS
    /// using the platform-appropriate release routine
    /// (`VirtualFree(MEM_RELEASE)` on Windows, `munmap` on Unix,
    /// `std::alloc::dealloc` on miri).
    ///
    /// This is **not** the inverse of [`into_parts`](Self::into_parts): that
    /// method returns only 3 of the 6 fields this constructor requires
    /// (`reservation_ptr, reservation_len, align`), discarding `base`, `len`,
    /// and `granted_huge` entirely. [`into_parts`](Self::into_parts)'s true structural complement
    /// is [`release`](crate::api::release), whose signature is exactly the 3-tuple `into_parts`
    /// returns — that is the intended matched pair for "take ownership out of
    /// RAII, then give it back to the OS manually". `from_raw_parts` is a
    /// separate, more general constructor for the cross-crate handoff pattern:
    /// a sibling crate (`numa-shim` on Windows) issues a platform-specific
    /// reservation call that `aligned-vmem` itself does not wrap, then adopts
    /// the result via this constructor — it needs `base`/`len` too because the
    /// adopted reservation's usable span need not start at the OS reservation's
    /// own base (this crate over-reserves `size + align` and keeps the full
    /// mapping whenever the exact-size fast path misses, or on Windows when
    /// `align > 64 KiB`, which is exactly that shape).
    ///
    /// # Safety
    ///
    /// This section covers ONLY memory-safety preconditions: liveness,
    /// exclusive ownership, pointer provenance, and exact-once release. A
    /// violation here is undefined behaviour. Functional/behavioral
    /// requirements — whether `granted_huge` accurately describes the
    /// mapping, and Windows commit-state compatibility — are NOT memory-
    /// safety preconditions and live in the "Correctness contract" section
    /// below instead (task #1172/M3: this section used to mix both kinds
    /// together, which made it impossible to state honestly that this
    /// crate's own integration tests deliberately violate some of the
    /// mixed-in conditions while remaining sound — see that section's
    /// opening paragraph for why that is not a contradiction).
    ///
    /// All six values must describe a **live, exclusively-owned OS
    /// reservation** compatible with `aligned-vmem`'s release path:
    ///
    /// - `base` is the *aligned usable* start; non-null, valid for `len` bytes,
    ///   aligned to `align`. For correct `decommit`/`decommit_lazy` behavior,
    ///   `base` must also be aligned to the runtime [`page_size()`](crate::page_size::page_size) (not just
    ///   the compile-time [`PAGE`]). On systems with non-4 KiB pages (e.g., 16 KiB on
    ///   Apple Silicon), passing a 4 KiB-aligned `base` will cause `decommit`,
    ///   `decommit_lazy`, or `munmap` calls to fail silently or return `EINVAL`.
    ///   **This alignment to page_size() is NOT checked by the constructor's
    ///   `assert!`** — it is the caller's responsibility to ensure it.
    /// - `len` is the usable span size, a non-zero multiple of [`PAGE`].
    /// - `reservation` is the *underlying OS reservation* start (often equal
    ///   to `base`, but may be lower because the reservation is over-reserved
    ///   to achieve alignment and the full mapping is kept). For correct OS
    ///   release behavior, it must be aligned to the runtime [`page_size()`](crate::page_size::page_size).
    ///   **This alignment to page_size() is NOT checked by the constructor's
    ///   `assert!`** — it is the caller's responsibility to ensure it.
    /// - Under miri specifically, `reservation` — NOT `base` — MUST be the exact
    ///   pointer returned by a `std::alloc::alloc` call, and that call's `Layout`
    ///   must equal `Layout::from_size_align(reservation_len, align)`. The miri
    ///   `release_reservation` reconstructs precisely that `Layout` and hands
    ///   `reservation` to `std::alloc::dealloc`, which requires the pointer to be
    ///   the one `alloc` returned and the layout to match exactly; anything else
    ///   is undefined behaviour, not a leak.
    ///
    ///   The distinction between `reservation` and `base` is load-bearing here
    ///   and is why this bullet names one and not the other: they are SEPARATE
    ///   parameters, and `base` MAY sit at a non-zero offset inside the region
    ///   `reservation` points at whenever the caller obtained that region with
    ///   extra slack to satisfy alignment. Satisfying the provenance
    ///   requirement at `base` while `reservation` points somewhere else is
    ///   exactly the mistake this wording exists to prevent. (This crate's own
    ///   miri backend returns `base == reservation`, so the distinction never
    ///   bites internally — which is what makes it easy to get wrong for a
    ///   caller-supplied pair.)
    ///
    ///   This requirement is specific to the miri backend; the Windows and Unix
    ///   backends release by address and do not track allocator provenance.
    ///   It complements — and does not restate — the `reservation_len`
    ///   precision rule below: that one governs the SIZE, this one governs
    ///   WHICH POINTER and WHERE THE MEMORY CAME FROM.
    /// - `reservation_len` must cover the underlying OS mapping/allocation.
    ///   The required PRECISION differs per backend, and is spelled out here
    ///   because the two halves of this rule used to contradict each other
    ///   (task #1035, finding F9: this bullet said an undersized value "leaks
    ///   memory (Unix)", while the "Important" note below said under-reporting
    ///   on a large-page host is "harmless for correctness" — both about Unix):
    ///   - **Native Unix, ORDINARY (non-huge) mapping:** `release` passes this
    ///     value straight to `munmap`, which ROUNDS THE LENGTH UP to a whole
    ///     page. A value short of the true mapping by less than one runtime
    ///     page therefore still unmaps the whole mapping and is harmless —
    ///     that is exactly the case the "Important" note below describes, and
    ///     it is the case this crate itself produces on a host whose page
    ///     size exceeds [`PAGE`]. What DOES leak is a value short by a whole
    ///     page or more: those trailing pages stay mapped for the life of the
    ///     process.
    ///   - **Native Unix, `granted_huge == true` (task #1172/M1, HugeTLB
    ///     exception to the paragraph above):** the "rounds up, so a
    ///     less-than-one-page shortfall is harmless" reasoning does NOT
    ///     transfer to a HugeTLB mapping. Linux's — and Android's, which
    ///     shares that kernel interface and is covered by the same `#[cfg]`
    ///     gate on the assert below — `mmap(2)` "Huge TLB
    ///     mappings" section requires BOTH `munmap(2)`'s `addr` and `length`
    ///     to be multiples of the huge page size — an undersized
    ///     `reservation_len` that is not huge-page-size-aligned gets `EINVAL`
    ///     from `munmap`, not a rounded-up unmap, and the ENTIRE mapping
    ///     (plus its pinned physical huge pages) leaks for the life of the
    ///     process. This crate's own `reserve_aligned_huge` path already
    ///     documents and upholds this requirement (see
    ///     `crates/aligned-vmem/src/os/unix.rs`'s huge-page-alignment
    ///     comments); `from_raw_parts` did not previously carry the same
    ///     requirement for an ADOPTED huge mapping. See the "2 MiB-multiple
    ///     requirement" bullet in "Correctness contract" below for the
    ///     checked form of this requirement.
    ///   - **miri:** `release` reconstructs a `Layout` from
    ///     `reservation_len`/`align` and hands it to `std::alloc::dealloc`,
    ///     which requires the EXACT size the allocation was made with — no
    ///     rounding, and a mismatch is undefined behaviour rather than a leak.
    ///     The rounding case above cannot arise here: under `cfg(miri)`
    ///     `query_os_page_size()` returns [`PAGE`] unconditionally, so
    ///     `page_size() == PAGE` and there is no larger runtime page to round
    ///     up to. The exact-size requirement is unqualified under miri.
    ///   - **Windows:** `VirtualFree(MEM_RELEASE)` ignores the length
    ///     entirely, so the value is advisory — reporting whatever
    ///     `Reservation::reservation_len` would report for an equivalent
    ///     reservation is sufficient.
    ///
    ///   **Important:** On hosts where the OS page size exceeds [`PAGE`]
    ///   (e.g., 16 KiB on Apple Silicon macOS, 64 KiB on some Linux
    ///   configurations), `reservation_len` may under-report the actual OS
    ///   mapping size — `mmap` rounds its length argument up to the page size,
    ///   so `reserve_aligned(PAGE, PAGE)` actually maps a full 16 KiB page
    ///   while `reservation_len()` returns `4096`. This is harmless for
    ///   correctness on an ORDINARY mapping (`munmap` rounds its length
    ///   argument up the same way; `VirtualFree(MEM_RELEASE)` ignores the
    ///   length on Windows) — it does NOT apply to a `granted_huge == true`
    ///   mapping on Linux or Android, per the HugeTLB bullet above — but it
    ///   means
    ///   `reservation_len` is a **logical** length, not a measure of the true
    ///   OS reservation size. It must be a non-zero multiple of [`PAGE`] with
    ///   `reservation_len >= len + (base - reservation)`.
    /// - `align` is a power of two `>= PAGE` and matches the alignment the OS
    ///   reservation was created with.
    /// - `granted_huge` itself carries NO memory-safety precondition: it is
    ///   stored and read back verbatim by every unsafe operation this
    ///   constructor, `Drop`, and `release_reservation` perform, and branched
    ///   on by neither. Its accuracy requirement, its interaction with
    ///   `reservation_len`'s HugeTLB exception above, and Windows commit-state
    ///   compatibility are all functional requirements — see "Correctness
    ///   contract" below.
    ///
    /// The reservation must be released **exactly once** — by dropping this
    /// handle, or by extracting via `into_parts` and calling [`release`](crate::api::release)
    /// manually. Constructing two `Reservation` handles over the same OS
    /// reservation is undefined behaviour (double release).
    ///
    /// # Correctness contract
    ///
    /// These requirements are NOT memory-safety preconditions — violating one
    /// changes observable *behavior* (a query result, a dispatch decision, or
    /// an OS-level no-op/leak) but never causes undefined behavior by itself.
    /// This crate's own integration tests deliberately violate the
    /// `granted_huge`-accuracy requirement below (see
    /// `tests/reservation_decommit_contract.rs`'s
    /// `method_try_decommit_reports_malformed_range_on_huge_flagged_reservation`
    /// and `tests/decommit_capability.rs`'s
    /// `simulated_huge_flag_drives_the_same_branch_dispatch_on_any_host`) to
    /// exercise huge-page branch dispatch without a real hugetlb-configured
    /// host — both tests' own SAFETY comments enumerate every reader of the
    /// flag and confirm none is memory-safety-relevant. That is a deliberate,
    /// reviewed use of this contract's slack, not a bug in either the tests
    /// or this documentation; production callers must still pass a truthful
    /// `granted_huge`, because the CONSEQUENCE of getting it wrong (below) is
    /// real even though it is not UB.
    ///
    /// - **`granted_huge` accuracy.** MUST accurately reflect whether the OS
    ///   actually granted huge pages for this reservation. Pass `true` only
    ///   if the reservation was obtained via a huge-page allocation (e.g.
    ///   `reserve_aligned_huge`) and the OS confirmed the grant (via
    ///   `Reservation::is_huge()` or equivalent platform-specific detection).
    ///   **Consequence of a wrong value:** `Reservation::is_huge()` reports
    ///   the wrong value, and any decommit-availability decision made from
    ///   that wrong result is wrong (on huge pages, `decommit` is a silent
    ///   no-op — RSS does not drop and reads return the old data). This
    ///   changes DISPATCH and query results, never memory safety. If you
    ///   cannot determine whether the OS granted huge pages, you MUST pass
    ///   `false` and use `reserve_aligned` instead.
    ///
    /// - **2 MiB-multiple requirement, Linux/Android, `granted_huge == true`
    ///   (task #1172/M1-hybrid).** On `target_os = "linux"` or `"android"`,
    ///   when `granted_huge` is `true`, this constructor additionally
    ///   `assert!`s that ALL FIVE of `len`, `reservation_len`, `reservation`,
    ///   `base`, and the offset `base - reservation` are multiples of 2 MiB
    ///   (this crate's one supported HugeTLB granularity, `MAP_HUGE_2MB`; see
    ///   `crates/aligned-vmem/src/os/unix.rs`'s `LINUX_HUGE_PAGE_SIZE`).
    ///   **Consequence of a violation:** an immediate, loud, attributable
    ///   panic at the call site — not deferred to `Drop`, and not a silent
    ///   leak — because a non-2-MiB-aligned `reservation`/`reservation_len`
    ///   on a real HugeTLB mapping would otherwise make the eventual
    ///   `munmap` fail `EINVAL` and leak the entire mapping (see the
    ///   `reservation_len` bullet in `# Safety` above). This assert narrows
    ///   what `granted_huge == true` is allowed to MEAN through this
    ///   constructor to "the mapping is in this crate's own 2 MiB HugeTLB
    ///   format" — it does not by itself prove the memory is really
    ///   `MAP_HUGETLB`-backed (that remains a `# Safety` precondition the
    ///   assert cannot check), only that its shape is consistent with being
    ///   so. **Open question, not yet answered by the crate owner:** whether
    ///   `aligned-vmem` should ever support adopting a HugeTLB mapping at a
    ///   granularity other than 2 MiB (e.g. 1 GiB) — see
    ///   `docs/CORRECTNESS_OPEN_ITEMS.md` item 90's OPEN QUESTION block. Under
    ///   today's default answer (no), this assert is exactly the crate's
    ///   contract, not a temporary narrowing.
    ///
    /// - **`huge-pages` feature required to pass `granted_huge: true` (task
    ///   #1172/M1-hybrid, closing finding M2 as a consequence).** Passing
    ///   `granted_huge: true` when this crate is built WITHOUT the
    ///   `huge-pages` feature is itself a contract violation and `assert!`s
    ///   immediately, for the same "loud at the call site, not silently
    ///   divergent later" reason as the 2 MiB bullet above. Before this
    ///   requirement, a caller who adopted a `MAP_HUGETLB` mapping through
    ///   their own crate but did not separately enable `huge-pages` in THIS
    ///   crate's `Cargo.toml` got `is_huge() == true` (accurately reflecting
    ///   what they passed) while `decommit`/`try_decommit` silently,
    ///   unconditionally skipped the backend call regardless of range or
    ///   kernel version (finding M2: the SAME live mapping served or skipped
    ///   decommit depending on the CONSUMER's Cargo feature set, invisible at
    ///   the call site). Requiring the feature to accept the flag at all
    ///   means there is no longer a huge-flagged ADOPTED reservation without
    ///   `huge-pages` enabled, so the divergent-behavior scenario cannot
    ///   arise — see [`Self::decommit`]'s doc for the full eligible-forward
    ///   explanation this closes the gap in.
    ///
    /// - **Windows commit state.** On Windows, the reservation's commit state
    ///   (which pages are committed vs. reserved-only) must be compatible
    ///   with the `granted_huge` value:
    ///
    ///   - If `granted_huge == false`, the reservation may be in any valid
    ///     commit state: fully committed (created via `reserve_aligned` or
    ///     the single-call Windows fast path), partially committed (created
    ///     via the two-call `reserve_aligned_lazy` path), or reserved-only
    ///     (not a common pattern but valid).
    ///
    ///   - If `granted_huge == true`, the reservation MUST have been created
    ///     with `MEM_RESERVE | MEM_COMMIT | MEM_LARGE_PAGES` in a single call
    ///     (the only way Windows grants large pages). The crate itself only
    ///     produces such reservations via its `reserve_aligned_huge`
    ///     single-call fast path. The crate's own two-call
    ///     `reserve_aligned_lazy` path (which issues
    ///     `VirtualAlloc(MEM_RESERVE)` followed by `VirtualAlloc(MEM_COMMIT)`)
    ///     is incompatible with `granted_huge == true`, because `MEM_COMMIT`
    ///     cannot be combined with `MEM_LARGE_PAGES` on a pre-reserved region
    ///     — MSDN requires all three flags in a single call.
    ///
    ///   **Consequence of a violation:** `Reservation::is_huge()` reports a
    ///   value inconsistent with the reservation's actual commit state —
    ///   the same DISPATCH/query-result consequence as the accuracy bullet
    ///   above, not a new failure mode. If you adopted a reservation from
    ///   another source and cannot determine whether it was created with the
    ///   one-call large-page path, you MUST pass `granted_huge == false`.
    #[must_use]
    pub unsafe fn from_raw_parts(
        base: *mut u8,
        len: usize,
        reservation: *mut u8,
        reservation_len: usize,
        align: usize,
        granted_huge: bool,
    ) -> Self {
        // Historical notes (task #719, #776, #916):
        //
        // - task #719: validate the documented `align`/`reservation_len` contract
        //   HERE, at the unsafe call site, rather than leaving it to surface later
        //   as a panic inside `Drop::drop` (via the miri backend's
        //   `Layout::from_size_align(reservation_len, align).expect(...)` in
        //   `release_reservation`) -- a panic reachable from `Drop` is far more
        //   dangerous than one at construction time: if this `Reservation` is ever
        //   dropped while ANOTHER panic is already unwinding the stack, Rust
        //   aborts the whole process on the second panic. Every other construction
        //   path in this crate already produces a valid `(align, reservation_len)`
        //   pair by construction (validated at each public entry point), so this
        //   check is specific to the caller-supplied values `from_raw_parts`
        //   accepts. Violating the documented contract is already undefined
        //   behaviour per this function's own `# Safety` section; panicking
        //   immediately here converts a silently-deferred hazard into a loud,
        //   attributable failure at the actual point of misuse.
        //
        // - task #776 (F2 revision -- round-closing review finding F7): the
        //   original check validated only `align`, but `Layout::from_size_align`
        //   also fails when `reservation_len` overflows `isize::MAX` once rounded
        //   up to `align` -- an `align`-only check left that half of the SAME
        //   Drop-reachable-panic hazard open (e.g. `from_raw_parts(b, PAGE, r,
        //   usize::MAX, PAGE)` still constructed successfully and still panicked
        //   inside `Drop` under miri). The explicit `reservation_len != 0 &&
        //   reservation_len.is_multiple_of(PAGE)` checks enforce the documented
        //   nonzero/page-multiple invariants, while `Layout::from_size_align(...).
        //   is_ok()` catches overflow cases.
        //
        // - task #916 (H2C3): the comment above previously claimed these checks
        //   "cover all documented contract violations immediately at the call
        //   site" -- this was false. Four documented invariants were uncheckable
        //   from the arguments alone (pointer validity, liveness, exclusivity,
        //   and exact-once release), but three MORE were cheaply checkable and
        //   were NOT checked:
        //   - `len` must be a non-zero multiple of `PAGE` (documented, not checked)
        //   - `base` must be aligned to `align` (documented, not checked)
        //   - `reservation <= base` (documented, now checked below via `base_addr >= res_addr`)
        //   - `reservation_len >= len + (base - reservation)` (documented, not checked)
        //   All four are now checked explicitly below, leaving only the genuinely
        //   uncheckable invariants (pointer validity, liveness, exclusivity) as
        //   unchecked caller responsibilities.
        let base_nn = NonNull::new(base).expect("from_raw_parts: base must be non-null");
        let res_nn =
            NonNull::new(reservation).expect("from_raw_parts: reservation must be non-null");
        let base_addr = base.addr();
        let res_addr = reservation.addr();
        assert!(
            align.is_power_of_two()
                && align >= PAGE
                && reservation_len != 0
                && reservation_len.is_multiple_of(PAGE)
                && len != 0
                && len.is_multiple_of(PAGE)
                && base_addr >= res_addr
                && base_addr.is_multiple_of(align)
                && len
                    .checked_add(base_addr - res_addr)
                    .is_some_and(|required| reservation_len >= required)
                && std::alloc::Layout::from_size_align(reservation_len, align).is_ok(),
            "Reservation::from_raw_parts: \
             align must be a power of two >= PAGE; \
             reservation_len must be non-zero and a multiple of PAGE; \
             len must be non-zero and a multiple of PAGE; \
             base must be >= reservation; \
             base must be aligned to align; \
             reservation_len must be >= len + (base - reservation); \
             (reservation_len, align) must form a valid Layout; \
             NOTE: alignment to runtime page_size() is NOT checked — \
             caller must ensure base/reservation are page_size()-aligned; \
             got align={align}, reservation_len={reservation_len}, len={len}, \
             base={base:?}, reservation={reservation:?}"
        );
        // Task #1172 (M1-hybrid, item 90 of docs/CORRECTNESS_OPEN_ITEMS.md):
        // narrow what `granted_huge == true` is allowed to MEAN through this
        // constructor. Two independent checks, both loud-at-the-call-site
        // for the same task #719 reason as the block above (a Drop-reachable
        // panic is far more dangerous than one here):
        //
        // 1. `huge-pages` feature required to accept `granted_huge: true` at
        //    all (closes finding M2 as a consequence: with no feature there
        //    are no huge-flagged ADOPTED reservations, so the "same live
        //    mapping served or skipped depending on the CONSUMER's feature
        //    set" divergence cannot arise).
        #[cfg(not(feature = "huge-pages"))]
        assert!(
            !granted_huge,
            "Reservation::from_raw_parts: granted_huge=true requires this crate's \
             `huge-pages` feature to be enabled. Without it, this constructor would \
             accept an adopted huge-flagged reservation whose eligible-forward \
             decommit path is compiled out, making `is_huge()` report true while \
             decommit/try_decommit silently skip on every call regardless of range \
             or kernel version -- see Reservation::decommit's rustdoc. Enable \
             `huge-pages`, or pass granted_huge: false."
        );
        // 2. On Linux/Android, when `granted_huge` is true, all FIVE of
        //    `len`, `reservation_len`, `reservation`, `base`, and the offset
        //    `base - reservation` must be 2 MiB multiples (this crate's one
        //    supported HugeTLB granularity). Linux's `mmap(2)`/`munmap(2)`
        //    require both the address and the length of a `MAP_HUGETLB`
        //    mapping's release call to be huge-page-size-aligned; violating
        //    this on a REAL HugeTLB mapping would make the eventual `munmap`
        //    fail EINVAL and leak the entire mapping (see the
        //    `reservation_len` HugeTLB bullet in `# Safety` above). This
        //    assert cannot by itself prove the memory really is
        //    `MAP_HUGETLB`-backed (a `# Safety` precondition it has no way
        //    to check) -- only that its shape is consistent with being so.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        if granted_huge {
            // 2 MiB: this crate's one supported HugeTLB granularity
            // (`MAP_HUGE_2MB`), matching `os::unix::LINUX_HUGE_PAGE_SIZE`.
            // Not imported directly: that constant is private to a module
            // compiled only under `#[cfg(all(unix, not(miri)))]` plus
            // `huge-pages`, narrower than this assert's own gate (Linux/
            // Android, any feature set, so the `huge-pages`-off panic above
            // fires first and with a clearer message on that combination).
            const HUGE_PAGE_SIZE_2MIB: usize = 2 * 1024 * 1024;
            assert!(
                len.is_multiple_of(HUGE_PAGE_SIZE_2MIB)
                    && reservation_len.is_multiple_of(HUGE_PAGE_SIZE_2MIB)
                    && res_addr.is_multiple_of(HUGE_PAGE_SIZE_2MIB)
                    && base_addr.is_multiple_of(HUGE_PAGE_SIZE_2MIB)
                    && (base_addr - res_addr).is_multiple_of(HUGE_PAGE_SIZE_2MIB),
                "Reservation::from_raw_parts: granted_huge=true on Linux/Android \
                 requires len, reservation_len, reservation, base, and the offset \
                 (base - reservation) to ALL be multiples of 2 MiB (this crate's \
                 supported HugeTLB granularity) -- a non-2-MiB-aligned reservation \
                 or reservation_len would make munmap(2) fail EINVAL and leak the \
                 entire mapping on release; \
                 got len={len}, reservation_len={reservation_len}, \
                 base={base:?}, reservation={reservation:?}, \
                 offset={}",
                base_addr - res_addr
            );
        }
        Self {
            base: base_nn,
            len,
            reservation: res_nn,
            reservation_len,
            align,
            granted_huge,
        }
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        // Record the release for mock observers (RAII path visibility).
        #[cfg(aligned_vmem_mock)]
        crate::mock::record(crate::mock::Call::Release {
            reservation: self.reservation.as_ptr().addr(),
            reservation_len: self.reservation_len,
        });
        // SAFETY: every safe constructor of `Self` (`reserve_aligned` and
        // friends) upholds `from_raw_parts`'s `# Safety` contract by
        // construction, so that contract covers `self.reservation` /
        // `self.reservation_len` / `self.align` regardless of which
        // constructor built this handle: `self.reservation` describes a
        // live OS reservation valid for `self.reservation_len` bytes,
        // release-compatible with the platform backend below (an exact
        // `std::alloc::alloc` provenance/`Layout` match under miri; a live
        // `mmap`'d region on Unix; a `VirtualAlloc(MEM_RESERVE)` region on
        // Windows), and this handle owns it exclusively (no aliasing —
        // `Reservation` is `Send` but not `Sync`). Dropping returns the
        // entire reservation to the OS exactly once.
        unsafe { release_reservation(self.reservation, self.reservation_len, self.align) };
    }
}

// SAFETY (Send): a `Reservation` owns its OS reservation exclusively; moving it
// to another thread moves ownership of every byte, leaving no aliasing on the
// origin thread. The memory is plain uninitialised bytes (no `Rc`/`Cell`/TLS
// affinity).
unsafe impl Send for Reservation {}
