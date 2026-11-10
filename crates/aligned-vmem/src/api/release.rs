use core::ptr::NonNull;

use crate::error::VmemError;
#[cfg(feature = "fault-injection")]
use crate::fault_injection;
#[cfg(aligned_vmem_mock)]
use crate::mock;
use crate::os::release_reservation;
use crate::os::release_reservation_checked;
use crate::page::PAGE;
use crate::reservation::Reservation;
use crate::reservation_parts::ReservationParts;

/// Validate the original release Layout without adding constructor arithmetic.
fn validate_release_size_align(len: usize, align: usize) -> Result<(), VmemError> {
    if len != 0
        && len.is_multiple_of(PAGE)
        && align.is_power_of_two()
        && align >= PAGE
        && std::alloc::Layout::from_size_align(len, align).is_ok()
    {
        Ok(())
    } else {
        Err(VmemError::invalid_argument())
    }
}

#[cfg(feature = "fault-injection")]
#[doc(hidden)]
/// Test-only forwarder for pure release geometry boundary checks.
pub fn release_size_align_is_valid_for_test(len: usize, align: usize) -> bool {
    validate_release_size_align(len, align).is_ok()
}

/// Release a whole OS reservation obtained from [`Reservation::into_parts`](crate::Reservation::into_parts).
///
/// # Safety
///
/// `reservation`, `reservation_len` and `align` must be the three values
/// returned by [`Reservation::into_parts`](crate::Reservation::into_parts) (or describe an
/// equivalent live, exclusively owned raw reservation as specified by
/// [`Reservation::from_raw_parts`](crate::Reservation::from_raw_parts)), and the
/// reservation must be released **exactly once**. Under miri, `reservation`
/// must be the original `System.alloc` pointer, with exactly the
/// `Layout::from_size_align(reservation_len, align)` used to allocate it;
/// `std::alloc::alloc` is not sufficient evidence of this origin when a
/// different global allocator is installed.
/// The native (`munmap` / `VirtualFree`) paths ignore `align`.
///
/// For a form that reports the native release call's own outcome instead of
/// discarding it, see [`try_release`].
///
/// If `reservation` is null, this function returns early and does nothing
/// (the call is a no-op). The mock recorder is also skipped in this case,
/// so a `mock`-based test's expected call log may desync if it expects a
/// record for a null pointer.
///
/// # Panics
///
/// Panics if `reservation` is non-null and `(reservation_len, align)` violates
/// the documented contract above: `reservation_len` must be non-zero and a
/// multiple of [`PAGE`], `align` must be a power of two `>= PAGE`, and the
/// pair must form a valid [`std::alloc::Layout`]. The assert runs before
/// `mock::record`, so under the `aligned_vmem_mock` cfg a contract-violating
/// call panics before it is ever recorded in the mock call log — it does not
/// appear as a `Release` entry.
///
/// A null `reservation` is unaffected by this: it remains the documented
/// no-op above and is not a panic path.
pub unsafe fn release(reservation: *mut u8, reservation_len: usize, align: usize) {
    // Historical note (task #947/G-1): before this assert existed, this doc
    // comment used to claim "the native (`munmap`/`VirtualFree`) paths ignore
    // `align`" — which was true in the sense that a contract-violating call
    // would silently "succeed" (no crash, no error) on those native backends;
    // only the `miri` fallback path (which reconstructs a `Layout` from
    // `reservation_len`/`align` to call back into `std::alloc`) would panic on
    // the same bad input, with a bare, uninformative `.expect()` message. That
    // divergence is now closed: this function validates the contract up front
    // and panics with a descriptive message on **every** backend, not only
    // under `miri`. The assert runs before `mock::record`, so under the
    // `aligned_vmem_mock` cfg a contract-violating call panics before it is
    // ever recorded in the mock call log — it does not appear as a `Release`
    // entry.
    //
    // The checked invariants are a subset of `from_raw_parts`'s checks because
    // `release` receives only `(reservation_len, align)` (not the full
    // `(base, len, reservation, reservation_len, align)` tuple), so the bounds
    // between `base` and `reservation` are uncheckable here — we validate what
    // we can and keep the same informative message style.
    if reservation.is_null() {
        return;
    }
    assert!(
        reservation_len != 0
            && reservation_len.is_multiple_of(PAGE)
            && align.is_power_of_two()
            && align >= PAGE
            && std::alloc::Layout::from_size_align(reservation_len, align).is_ok(),
        "release: \
         reservation_len must be non-zero and a multiple of PAGE; \
         align must be a power of two >= PAGE; \
         (reservation_len, align) must form a valid Layout; \
         got reservation_len={reservation_len}, align={align}"
    );

    let nn = NonNull::new(reservation).expect("checked non-null above");
    #[cfg(aligned_vmem_mock)]
    mock::record(mock::Call::Release {
        reservation: reservation.addr(),
        reservation_len,
    });
    // SAFETY: forwarded from the caller's contract above.
    unsafe { release_reservation(nn, reservation_len, align) };
}

/// Release a reservation obtained from [`Reservation::into_reservation_parts`](crate::Reservation::into_reservation_parts).
///
/// This is the typed alternative to [`release`]: it takes a [`ReservationParts`]
/// struct instead of raw parameters, preventing accidental swapping of `len` and
/// `align` (which would cause undefined behavior on the native backend and leaks
/// or crashes on Unix).
///
/// For backwards compatibility with code that uses the raw tuple form, you can
/// convert a `ReservationParts` to a tuple via [`ReservationParts::as_tuple`] and
/// call [`release`].
///
/// For a form that reports the native release call's own outcome instead of
/// discarding it, see [`try_release_parts`].
///
/// # Safety
///
/// `parts.ptr` must be a reservation obtained from [`Reservation::into_reservation_parts`](crate::Reservation::into_reservation_parts)
/// (or the raw [`Reservation::into_parts`](crate::Reservation::into_parts)) and must be live. The reservation must be released
/// exactly once.
pub unsafe fn release_parts(parts: ReservationParts) {
    let ReservationParts {
        ptr: reservation,
        len: reservation_len,
        align,
    } = parts;
    // Delegate to the existing release function.
    unsafe { release(reservation, reservation_len, align) };
}

/// Private single dispatch point for the checked release surface: both
/// `try_release` and `try_release_parts` (and, through them,
/// `Reservation::try_release`) funnel through here, mirroring
/// `dispatch_try_decommit`'s role on the decommit side.
///
/// Order of operations: the pre-syscall fault hook is checked first, followed
/// by the mock record; the backend-result surrogate then supplies a refusal
/// at the backend result seam without fabricating a native OS error. Native
/// failures retain the error captured immediately by the OS backend.
///
/// Unlike the commit/decommit hooks' consult sites, this call site is
/// compiled under EVERY cfg — release is core API and the `aligned_vmem_mock`
/// cfg does NOT replace the release backend (the real backend call still
/// runs under mock) — so no `allow(dead_code)` cfg_attr is needed on the
/// fault-injection consult or anywhere in this function.
///
/// # Safety
///
/// Forwarded caller contract: `reservation`, `reservation_len` and `align`
/// must describe a live, exclusively owned raw reservation compatible with
/// `Reservation::into_parts`/`from_raw_parts`, released exactly once.
pub(crate) unsafe fn dispatch_try_release(
    reservation: NonNull<u8>,
    reservation_len: usize,
    align: usize,
) -> Result<(), VmemError> {
    #[cfg(feature = "fault-injection")]
    {
        if fault_injection::should_fail_release() {
            // task #713: SIMULATED failure — no syscall ran, so there is no
            // real OS code to report; `last_os_error()` here would fabricate
            // a stale cause from an unrelated prior call.
            return Err(VmemError::os_refusal_unknown_code());
        }
    }
    // The mock record happens only when the backend call is actually issued:
    // an injected pre-syscall fault-injection failure returns above without
    // recording anything.
    #[cfg(aligned_vmem_mock)]
    mock::record(mock::Call::Release {
        reservation: reservation.as_ptr().addr(),
        reservation_len,
    });
    // SAFETY: live unique reservation, original geometry, one consuming attempt.
    unsafe { release_reservation_checked(reservation, reservation_len, align) }
}

/// Checked (fallible) twin of [`release`]: the same release, but the `Result`
/// reports the REAL native outcome of the OS release call instead of silently
/// discarding it. On Windows, `Ok` means `VirtualFree(MEM_RELEASE)` returned
/// nonzero; on every unix target (Linux, Android, the Darwin family, the
/// BSDs), `Ok` means `munmap(2)` returned `0` and `Err` is its `-1` return
/// with the `errno` captured immediately at the failing call. Under miri,
/// `Ok` is terminal release within miri's model (`System.dealloc` has no
/// failure mode to report). Under the `aligned_vmem_mock` cfg the record is
/// written and the REAL platform backend still runs — mock replaces
/// decommit/recommit/commit_range but NOT release.
///
/// On `Err` the reservation's release state is UNPROVEN: fully, partially, or
/// not released are all possible and indistinguishable through this API.
///
/// # Safety
///
/// `reservation`, `reservation_len` and `align` must be the three values
/// returned by [`Reservation::into_parts`](crate::Reservation::into_parts) (or describe an
/// equivalent live, exclusively owned raw reservation as specified by
/// [`Reservation::from_raw_parts`](crate::Reservation::from_raw_parts)), and the
/// reservation must be released **exactly once**. Under miri, `reservation`
/// must be the original `System.alloc` pointer, with exactly the
/// `Layout::from_size_align(reservation_len, align)` used to allocate it.
/// The native (`munmap` / `VirtualFree`) paths ignore `align`.
///
/// **Exact-once-even-on-`Err`:** the call consumes the release decision — on
/// `Err` the caller MUST NOT call [`release`], [`try_release`], or
/// [`try_release_parts`] again on the same reservation (retrying risks a
/// double release if the failed attempt actually took effect), MUST NOT treat
/// the pointer as live for any use, and MUST conservatively retain any
/// outstanding accounting charge. The crate never returns the reservation to
/// a usable owning state and never retries internally.
///
/// If `reservation` is null, this function returns `Ok(())` early and does
/// nothing (the call is a documented no-op, same early return as
/// [`release`]). The mock recorder is also skipped in this case, so a mock
/// test's expected call log may desync if it expects a record for a null
/// pointer.
///
/// # Errors
///
/// [`VmemError::invalid_argument()`] for a contract violation — a non-null
/// `reservation` with zero/misaligned `reservation_len` or a bad `align`
/// (documented divergence from the infallible [`release`], which panics: the
/// fallible form reports, never trips a tripwire).
///
/// [`VmemError::os_refusal_unknown_code()`] if an armed `fault-injection`
/// release hook fired (a simulated failure — no syscall ran, so no real OS
/// code to report).
///
/// A real backend refusal carries the OS cause captured immediately at the
/// failing call: `errno` on Unix, `GetLastError` on Windows — surfaced via
/// the error's `os_code()`.
pub unsafe fn try_release(
    reservation: *mut u8,
    reservation_len: usize,
    align: usize,
) -> Result<(), VmemError> {
    if reservation.is_null() {
        return Ok(());
    }
    // Divergence from `release` (which asserts): the try_* convention across
    // this crate is that the fallible form reports contract violations as
    // `Err(VmemError::invalid_argument())` and never trips a tripwire.
    validate_release_size_align(reservation_len, align)?;
    let nn = NonNull::new(reservation).expect("checked non-null above");
    // SAFETY: caller contract is live and uniquely owned, with original geometry.
    // This invokes a single consuming attempt; Err permits no retry or live reuse.
    unsafe { dispatch_try_release(nn, reservation_len, align) }
}

/// Checked (fallible) twin of [`release_parts`]: the typed release that
/// reports the REAL native outcome of the OS release call instead of
/// discarding it. See [`try_release`] for the exact per-platform meaning of
/// `Ok`/`Err` and the exact-once-even-on-`Err` rule; the `Ok`/`Err` semantics
/// are identical — only the parameter shape differs (a [`ReservationParts`]
/// struct instead of the raw triple, preventing accidental swapping of `len`
/// and `align`).
///
/// # Safety
///
/// `parts.ptr` must be a reservation obtained from
/// [`Reservation::into_reservation_parts`](crate::Reservation::into_reservation_parts)
/// (or the raw [`Reservation::into_parts`](crate::Reservation::into_parts)) and must be live.
/// The reservation must be released exactly once — on `Err` as well: the
/// caller MUST NOT retry the release and MUST conservatively retain any
/// outstanding accounting charge (the release state on `Err` is unproven).
///
/// # Errors
///
/// On the backend-result surrogate, the checked platform backend skips native release and returns the explicit no-code refusal; it does not establish kernel refusal behavior.
pub unsafe fn try_release_parts(parts: ReservationParts) -> Result<(), VmemError> {
    let ReservationParts {
        ptr: reservation,
        len: reservation_len,
        align,
    } = parts;
    // SAFETY: `parts` came from a live uniquely owned reservation and carries
    // its original length/alignment. This is its single consuming release
    // attempt; on Err no retry is permitted and no live owning handle remains.
    unsafe { try_release(reservation, reservation_len, align) }
}

impl Reservation {
    /// Consuming checked release of this reservation: the safe, fallible
    /// counterpart of the infallible `Drop`-backed release and of
    /// [`release_parts`], reporting the REAL native outcome of the OS release
    /// call (see [`try_release`] for the exact per-platform meaning of `Ok`
    /// and `Err`).
    ///
    /// Ownership is consumed REGARDLESS of outcome: on `Err` the handle is
    /// gone and the reservation's release is unproven — the same conservative
    /// retention contract as [`try_release`] applies (never retry, never treat
    /// the pointer as live, retain any outstanding accounting charge). The
    /// method never returns the handle or a live pointer, and `Drop` does not
    /// run afterwards because the handle was consumed.
    pub fn try_release(self) -> Result<(), VmemError> {
        let parts = self.into_reservation_parts();
        // SAFETY: the parts came from a live exclusively-owned handle consumed
        // above; `into_reservation_parts` suppresses `Drop`, so no double
        // release can occur.
        unsafe { try_release_parts(parts) }
    }
}
