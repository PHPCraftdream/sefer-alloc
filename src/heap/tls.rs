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
/// Lazily bootstraps the heap on first call.
///
/// # Non-panicking contract (0.3.x task #132)
///
/// Returns `None` -- NEVER panics -- in any of these cases:
/// - the TLS destructor has already run (thread is shutting down and the TLS
///   slot is inaccessible);
/// - a reentrant call (e.g. `f` itself calls `with_heap` again, or a `Drop`
///   impl running inside `f` allocates and re-enters `with_heap` while the
///   outer borrow is still held);
/// - the primordial segment reservation fails (OOM at startup -- unrecoverable
///   for an allocator, but we propagate gracefully).
///
/// This used to `HEAP.with(...)` + `borrow_mut()`, which panics on either the
/// teardown or reentrant case -- a footgun for the public `Heap`/`with_heap`
/// API surface: a `Drop` impl that allocates via `with_heap` during thread
/// teardown, or that (directly or transitively) re-enters `with_heap` while
/// already inside one, would panic instead of degrading gracefully. Every
/// caller already handles `None` (the signature has always returned
/// `Option<R>`, legal on primordial OOM), so callers need NO changes to
/// benefit from the stricter no-panic contract.
///
/// Implemented as a thin public wrapper over the same no-panic
/// `try_with`/`try_borrow_mut` mechanics as the crate-internal
/// [`with_heap_try`] (kept as a `pub(crate)` alias so existing internal call
/// sites are unaffected).
pub fn with_heap<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut Heap) -> R,
{
    with_heap_impl(f)
}

/// Execute `f` with a mutable reference to the current thread's [`Heap`], or
/// return `None` if the heap cannot be accessed right now WITHOUT panicking.
///
/// This was the **no-panic** variant for the Phase 11 `GlobalAlloc` face.
/// Phase 12.3 rewired `SeferAlloc` to route through the registry-backed
/// raw-pointer TLS ([`crate::global::tls_heap`]) instead of this `RefCell`
/// binding, so `with_heap_try` is no longer on the malloc path. 0.3.x task
/// #132 unified this with the public [`with_heap`], which is now the SAME
/// no-panic implementation ([`with_heap_impl`]) -- this alias is kept so
/// existing `pub(crate)` call sites are unaffected.
///
/// Returns `None` if the heap cannot be accessed (TLS destroyed, reentrant
/// borrow, or primordial OOM). The caller MUST handle `None` by returning null
/// -- never by panicking.
#[cfg(feature = "alloc-global")]
#[allow(dead_code)]
pub(crate) fn with_heap_try<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut Heap) -> R,
{
    with_heap_impl(f)
}

/// Shared no-panic implementation for [`with_heap`] and [`with_heap_try`]
/// (0.3.x task #132 -- unified so the two public/`pub(crate)` entry points
/// never drift apart).
fn with_heap_impl<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut Heap) -> R,
{
    // `try_with` returns `Result<R, AccessError>`; `AccessError` means the
    // thread-local's destructor has already run (thread shutdown). We treat
    // this as "no heap available" and return None -- the caller returns null
    // (or propagates `None`). This is the no-panic contract: `.ok()?`
    // converts Err(AccessError) to None without panicking.
    HEAP.try_with(|cell| {
        // `try_borrow_mut` returns `Err` on a reentrant borrow (e.g. a
        // nested `with_heap` call, or a `Drop` impl running inside `f` that
        // allocates and re-enters). Returning `None` here -- rather than
        // `RefCell::borrow_mut`'s panic -- is the fix for the public API
        // footgun task #132 closes.
        let mut borrow = cell.try_borrow_mut().ok()?;
        if borrow.is_none() {
            *borrow = Some(Heap::new()?);
        }
        let heap = borrow.as_mut()?;
        Some(f(heap))
    })
    .ok()?
}
