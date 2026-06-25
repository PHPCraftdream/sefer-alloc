//! Thread-local heap binding -- the TLS routing truth for Phase 9.
//!
//! Each thread lazily initialises its own [`Heap`] on first use via
//! `std::thread_local!`. The heap is released when the thread exits (the TLS
//! destructor drops the `RefCell<Option<Heap>>`). This is allocation-free
//! init: `std::thread_local!` with a `const` initialiser does NOT call the
//! global allocator (it uses a linker-provided TLS slot on all major
//! platforms).
//!
//! **M5 reentrancy-freedom:** the TLS access is a plain thread-local load (no
//! lock, no alloc). The `Heap::new()` bootstrap calls the OS aperture
//! (`mmap`/`VirtualAlloc`) but never the global allocator. So the TLS init
//! path is reentrancy-free.

use std::cell::RefCell;

use super::heap::Heap;

thread_local! {
    /// The per-thread heap. `None` until first use; `Some` for the thread's
    /// lifetime. The `RefCell` is needed because `thread_local!` hands out
    /// shared references (`&`), but `Heap::alloc`/`dealloc` need `&mut`.
    /// The `RefCell` borrow is uncontended (single-thread, single-owner)
    /// and never panics (we never hold a borrow across a yield point).
    static HEAP: RefCell<Option<Heap>> = const { RefCell::new(None) };
}

/// Execute `f` with a mutable reference to the current thread's [`Heap`].
///
/// Lazily bootstraps the heap on first call. Returns `None` only if the
/// primordial segment reservation fails (OOM at startup -- unrecoverable for
/// an allocator, but we propagate gracefully).
///
/// # Panics
///
/// Panics if the TLS destructor has already run (thread is shutting down and
/// the TLS slot is poisoned). This is the standard `thread_local!` behaviour
/// and is acceptable: a thread that outlives its TLS is already in an
/// exceptional state.
pub fn with_heap<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut Heap) -> R,
{
    HEAP.with(|cell| {
        let mut borrow = cell.borrow_mut();
        if borrow.is_none() {
            *borrow = Some(Heap::new()?);
        }
        let heap = borrow.as_mut().unwrap();
        Some(f(heap))
    })
}

/// Execute `f` with a mutable reference to the current thread's [`Heap`], or
/// return `None` if the heap cannot be accessed right now WITHOUT panicking.
///
/// This is the **no-panic** variant for the `GlobalAlloc` face (Phase 11,
/// `alloc-global`). A panic inside `#[global_allocator]` aborts the whole
/// process, so the malloc face must never panic. The two situations where
/// `with_heap` would panic are handled gracefully here:
///
/// 1. **TLS slot destroyed (thread shutdown):** `try_with` returns `Err` when
///    the thread-local's destructor has already run (the thread is exiting and
///    its TLS slot has been reclaimed). We return `None` -- the caller (the
///    `GlobalAlloc` impl) returns null, signalling OOM. This is correct: a
///    thread that has outlived its TLS heap cannot serve allocations, and
///    returning null lets `std` fall back gracefully (most allocation requests
///    during shutdown are best-effort).
/// 2. **Reentrant borrow:** `RefCell::try_borrow_mut` returns `Err` if the
///    cell is already borrowed. Under M5 (reentrancy-freedom) this never
///    happens on the alloc path (no alloc call reaches back into the heap),
///    but we guard defensively rather than panicking.
///
/// Returns `None` if the heap cannot be accessed (TLS destroyed, reentrant
/// borrow, or primordial OOM). The caller MUST handle `None` by returning null
/// -- never by panicking.
#[cfg(feature = "alloc-global")]
pub(crate) fn with_heap_try<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut Heap) -> R,
{
    // `try_with` returns `Result<R, AccessError>`; `AccessError` means the
    // thread-local's destructor has already run (thread shutdown). We treat
    // this as "no heap available" and return None -- the caller (the
    // GlobalAlloc impl) returns null. This is the no-panic contract: `.ok()?`
    // converts Err(AccessError) to None without panicking.
    HEAP.try_with(|cell| {
        // `try_borrow_mut` returns `Err` only on a reentrant borrow. Under M5
        // (reentrancy-freedom) this is impossible on the alloc path, but we
        // guard defensively (no panic).
        let mut borrow = cell.try_borrow_mut().ok()?;
        if borrow.is_none() {
            *borrow = Some(Heap::new()?);
        }
        let heap = borrow.as_mut()?;
        Some(f(heap))
    })
    .ok()?
}
