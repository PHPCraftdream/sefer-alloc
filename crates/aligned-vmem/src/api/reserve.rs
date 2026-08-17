use crate::error::VmemError;
#[cfg(aligned_vmem_mock)]
use crate::mock;
use crate::os::reserve_aligned_raw;
use crate::Reservation;

use super::internal::{finish_reservation, validate_size_align, RawReservation};

/// Reserve `size` bytes of anonymous virtual memory whose base is aligned to
/// `align`.
///
/// - `align` must be a power of two `>=` [`PAGE`](crate::page::PAGE).
/// - `size` must be a non-zero multiple of [`PAGE`](crate::page::PAGE).
///
/// On 32-bit Unix, first tries an ordinary exact-size `mmap` and checks
/// whether the kernel happened to place it at an `align`-aligned address
/// (fast path; hit rate depends on the OS's placement heuristics, not on any
/// hint this crate passes); on a miss (wrong alignment), over-reserves
/// `size + align` bytes and keeps the full mapping. On 64-bit Unix, the fast
/// path is compiled out (`target_pointer_width = "32"` — see task #944,
/// finding P-1), with ONE exception: on Linux AND Android, with the `huge-pages`
/// feature on, a request for `align == LINUX_HUGE_PAGE_SIZE` (2 MiB) huge pages
/// takes an exact-size `MAP_HUGETLB` attempt first, which when it succeeds
/// reserves exactly `size`. That exception is gated on
/// `any(target_os = "linux", target_os = "android")` + `feature = "huge-pages"`,
/// NOT on pointer width — which is both why it still fires on 64-bit and why
/// calling it Linux-only would be wrong. When it does not apply, a 64-bit Unix
/// reservation over-reserves `size + align`
/// bytes in one `mmap` call. On Windows, uses one syscall (fast path
/// for `align <= 64 KiB`, over-reserving nothing — base == region) or two
/// syscalls (over-reserving `size + align` and keeping the full mapping). The `Reservation::reservation_ptr` / `reservation_len` fields
/// expose the full reservation; `Reservation::as_ptr` / `len` expose the
/// aligned usable span.
///
/// **Cost on 32-bit Unix fast-path miss:** the reservation holds `size + align`
/// bytes of virtual address space for its lifetime (measured hit rate: 34.4% at
/// 64 KiB align, 46.7% at 1 MiB, 56.7% at 4 MiB — commit `35d51e6`, task #849;
/// measured on WSL2/Linux, x86_64; 30-run aggregate; scope: 32-bit only — the
/// hit rate is kernel- and ASLR-dependent and is not expected to transfer to
/// other Unix platforms). **On 64-bit Unix these numbers do not apply**: the
/// fast path never runs, so every reservation pays the "miss" cost of
/// `size + align` bytes held for the reservation's lifetime, unconditionally.
///
/// Returns `None` on a contract violation or if the OS refuses the reservation
/// (OOM) — never panics, so it is safe to call from inside a `GlobalAlloc`
/// implementation. For the failure cause use [`try_reserve_aligned`].
#[must_use]
pub fn reserve_aligned(size: usize, align: usize) -> Option<Reservation> {
    try_reserve_aligned(size, align).ok()
}
/// Fallible [`reserve_aligned`]: returns a [`VmemError`] carrying the OS cause
/// (`errno` / `GetLastError`) on failure instead of a bare `None`.
///
/// A contract violation (bad `size`/`align`) returns
/// [`VmemError::invalid_argument`] without touching the OS.
pub fn try_reserve_aligned(size: usize, align: usize) -> Result<Reservation, VmemError> {
    validate_size_align(size, align)?;
    // Mock fault-injection: honour a scripted reserve failure first.
    #[cfg(aligned_vmem_mock)]
    if let Some(e) = mock::take_reserve_fault() {
        mock::record(mock::Call::Reserve { size, align });
        return Err(e);
    }
    #[cfg(aligned_vmem_mock)]
    mock::record(mock::Call::Reserve { size, align });

    // task #713: `reserve_aligned_raw` now captures its own `VmemError`
    // immediately at the point of failure (before any cleanup FFI); this
    // just propagates it rather than re-deriving a possibly-stale one here.
    finish_reservation(
        size,
        align,
        reserve_aligned_raw(size, align).map(|(base, reservation, reservation_len)| {
            RawReservation {
                base,
                reservation,
                reservation_len,
                granted_huge: false,
            }
        }),
    )
}
