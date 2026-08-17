use core::ptr::NonNull;

use crate::page::PAGE;

use super::reserve::reserve_aligned;

/// Reserve `size` bytes of **zero-initialised** anonymous virtual memory and
/// **leak** it for the process lifetime, returning the base pointer.
///
/// Folds the leaked-zeroed-sidecar pattern (used by allocators for pre-main
/// bookkeeping structures that must not route through the very allocator they
/// implement) into one helper:
///
/// - `size` is rounded up to a multiple of [`PAGE`] internally (any non-zero
///   `size` is accepted; a zero `size` returns `None`). On some platforms
///   (e.g. macOS with 16 KiB pages, or 64 KiB Windows allocation granularity),
///   the OS may round further beyond `PAGE`, so the actual granularity
///   consumed can exceed `PAGE`.
/// - the span is guaranteed all-zero on every backend, INCLUDING the miri
///   fallback (`std::alloc` does not zero; this helper zeroes explicitly under
///   miri), so the returned memory is a valid all-zero initial state.
/// - the reservation is `mem::forget`-leaked: it lives for the process lifetime
///   and is never released.
///
/// Returns `None` on OOM or a zero `size`. The returned pointer is non-null,
/// [`PAGE`]-aligned, and valid for the rounded-up size for the whole process
/// lifetime. Because the reservation is leaked, the returned pointer may be
/// safely turned into a `&'static` by the caller (subject to the caller's own
/// aliasing discipline).
#[must_use]
pub fn leak_zeroed_pages(size: usize) -> Option<NonNull<u8>> {
    if size == 0 {
        return None;
    }
    let rounded = size.checked_add(PAGE - 1)? & !(PAGE - 1);
    let reservation = reserve_aligned(rounded, PAGE)?;
    let base = reservation.as_ptr();

    // Under miri, `reserve_aligned` falls back to `std::alloc`, which does NOT
    // zero the bytes; every real OS backend hands back zeroed pages. Zero
    // explicitly under miri so the all-zero initial-state guarantee holds on
    // every backend.
    #[cfg(miri)]
    // SAFETY: `base` is a fresh, exclusively-owned reservation of `rounded`
    // bytes; nothing else references it yet, so writing zeros is sound.
    unsafe {
        core::ptr::write_bytes(base, 0, rounded);
    }

    // Leak: the sidecar lives for the process lifetime, never released.
    core::mem::forget(reservation);

    // SAFETY: `base` is the non-null `as_ptr` of a successful reservation.
    Some(unsafe { NonNull::new_unchecked(base) })
}
