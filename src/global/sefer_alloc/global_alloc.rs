// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see `src/lib.rs`);
// this is the documented alloc-face seam. `allow` lifts the crate-level
// deny for this file only -- `unsafe` anywhere else in the crate is a hard
// error. The ONLY `unsafe` here is the `unsafe impl GlobalAlloc` (the trait
// is `unsafe`) plus the `// SAFETY:`-annotated pointer handoff to HeapCore.
#![allow(unsafe_code)]

#[cfg(all(feature = "internals", feature = "bench-internals"))]
use crate::global::fallback;
use core::alloc::{GlobalAlloc, Layout};

use crate::global::tls_heap::{current_for_dealloc, CurrentHeapForDealloc};

use crate::global::tls_heap::CurrentHeap;

use super::SeferAlloc;

#[cfg(feature = "exact-object-proto")]
use crate::global::exact_object::ExactNarrow;

#[cold]
#[inline(never)]
fn exit_escape_alloc(layout: Layout, zeroed: bool) -> *mut u8 {
    // SAFETY: the caller supplied a valid nonzero GlobalAlloc layout.
    // System owns the escaped block; it leaks at exit.
    unsafe {
        if zeroed {
            std::alloc::System.alloc_zeroed(layout)
        } else {
            std::alloc::System.alloc(layout)
        }
    }
}

/// Exit-path `realloc` for a block this allocator can no longer route:
/// fresh `System` storage plus a copy of `min(old, new)` bytes; the old
/// block is never handed to `System.dealloc` and leaks at exit.
#[cold]
#[inline(never)]
fn exit_escape_realloc(ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
    if new_size == 0 {
        return core::ptr::null_mut();
    }
    let Ok(layout) = Layout::from_size_align(new_size, old_layout.align()) else {
        return core::ptr::null_mut();
    };
    // SAFETY: nonzero validated layout with the old alignment. The caller
    // retains a live old allocation of old_layout.size() readable bytes; a
    // successful fresh System allocation cannot overlap it. Never hand an
    // unknown-origin pointer to System.realloc/dealloc. Both the old storage
    // and the escaped storage are intentionally leaked at exit.
    unsafe {
        let new_ptr = std::alloc::System.alloc(layout);
        if !new_ptr.is_null() {
            core::ptr::copy_nonoverlapping(ptr, new_ptr, old_layout.size().min(new_size));
        }
        new_ptr
    }
}

// SAFETY (the trait obligation): `GlobalAlloc` requires that `alloc`/
// `alloc_zeroed`/`realloc` return valid memory for the requested `Layout`
// (or null on failure), and that `dealloc` receives a pointer previously
// returned by an allocating method. We delegate to `HeapCore::alloc`/
// `dealloc`/`realloc`/`alloc_zeroed`, which uphold M1 (validity), M3 (no
// overlap), and M4 (alignment/size fidelity) -- verified by the Phase 8/9
// differential proptests and miri. `HeapCore` returns null on OOM (never
// panics -- the substrate panic sites were hardened in Phase 11). If the
// TLS heap is unavailable (thread teardown), `current_heap()` returns the
// process-global fallback heap (never null); `dealloc` on the fallback is
// sound under the fallback's spinlock. M10 (never-null for serviceable
// requests) is upheld: the only null return is true OOM. During process
// exit a cold null return is instead served by a `System` escape
// (`exit_escape_alloc`/`exit_escape_realloc`) that leaks until the process
// ends.
unsafe impl GlobalAlloc for SeferAlloc {
    #[inline(always)]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        #[cfg(feature = "exact-object-proto")]
        if ExactNarrow::is_narrow(layout) {
            return ExactNarrow::alloc(layout, false);
        }
        let ptr = match self.current_heap() {
            CurrentHeap::Fallback => match self.with_fallback_heap(|h| h.alloc(layout)) {
                Some(ptr) => ptr,
                None => core::ptr::null_mut(),
            },
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in
            // a registry slot. `current_heap` returned it for THIS thread;
            // the single-writer invariant (the CAS-won slot owner) makes
            // `&mut` access exclusive. `HeapCore::alloc` upholds the
            // GlobalAlloc contract (returns valid memory or null).
            CurrentHeap::Own(heap) => unsafe { (*heap).alloc(layout) },
        };
        if ptr.is_null() && crate::global::process_exit_in_progress() {
            // Cold null path only: the route is unavailable because a lock
            // holder was terminated by `ExitProcess`. Owner decision: escape
            // to System, never abort (docs/reviews/2026-10-11, question 7).
            return exit_escape_alloc(layout, false);
        }
        ptr
    }

    #[inline(always)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        // Prototype: every pointer is looked up by exact address first.
        // SAFETY: `ptr`/`layout` are this method's own contract pair.
        #[cfg(feature = "exact-object-proto")]
        if unsafe { ExactNarrow::try_dealloc(ptr, layout) } {
            return;
        }
        match current_for_dealloc() {
            CurrentHeapForDealloc::Own(heap) => {
                // SAFETY: `heap` is non-null and points to a live `HeapCore`
                // in a registry slot this thread owns (single-writer
                // invariant).
                unsafe { (*heap).dealloc(ptr, layout) };
            }
            CurrentHeapForDealloc::ForeignNoBind => {
                // SAFETY: the GlobalAlloc caller transfers its unique current
                // allocation. Lookup and publication use only independent sidecars.
                unsafe { crate::registry::HeapCore::publish_foreign(ptr, layout) };
            }
        }
    }

    // #1987: `#[inline(always)]`, matching `alloc`/`dealloc` above. All four
    // `GlobalAlloc` methods are the same shape — a tiny tagged dispatch over
    // `current_heap()` — and `alloc_zeroed` is the calloc-shaped entry
    // (`vec![0; n]`, `Box::new([0; N])`, `__rust_alloc_zeroed`) that real
    // workloads hit nearly as often as `alloc`. The previous split (two
    // `inline(always)`, two `inline`) predated the `sefer_alloc.rs` ->
    // `sefer_alloc/` file split (marker counts were identical on both sides
    // of it) and had no recorded rationale; uniform `inline(always)` is the
    // choice, so a future reader does not have to guess which half was
    // deliberate.
    #[inline(always)]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        #[cfg(feature = "exact-object-proto")]
        if ExactNarrow::is_narrow(layout) {
            return ExactNarrow::alloc(layout, true);
        }
        let ptr = match self.current_heap() {
            CurrentHeap::Fallback => match self.with_fallback_heap(|h| h.alloc_zeroed(layout)) {
                Some(ptr) => ptr,
                None => core::ptr::null_mut(),
            },
            // SAFETY: as in `alloc`.
            CurrentHeap::Own(heap) => unsafe { (*heap).alloc_zeroed(layout) },
        };
        if ptr.is_null() && crate::global::process_exit_in_progress() {
            return exit_escape_alloc(layout, true);
        }
        ptr
    }

    // #1987: `#[inline(always)]` — see `alloc_zeroed` above for the rationale
    // (all four `GlobalAlloc` methods share one dispatch shape and now one
    // inlining policy).
    #[inline(always)]
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return core::ptr::null_mut();
        }
        // Prototype: a narrow old or new class always moves (no in-place promise).
        #[cfg(feature = "exact-object-proto")]
        if let Ok(new_layout) = Layout::from_size_align(new_size, old_layout.align()) {
            if ExactNarrow::is_narrow(old_layout) || ExactNarrow::is_narrow(new_layout) {
                // SAFETY: this fn's GlobalAlloc contract; the new block is distinct.
                unsafe {
                    let new_ptr = self.alloc(new_layout);
                    if !new_ptr.is_null() {
                        core::ptr::copy_nonoverlapping(
                            ptr,
                            new_ptr,
                            old_layout.size().min(new_size),
                        );
                        self.dealloc(ptr, old_layout);
                    }
                    return new_ptr;
                }
            }
        }
        let new_ptr = match self.current_heap() {
            // SAFETY: `ptr`/`old_layout` are the caller-bound GlobalAlloc
            // contract pair (this whole fn is `unsafe fn realloc`); the
            // closure forwards them to the fallback heap's `HeapCore::realloc`.
            CurrentHeap::Fallback => {
                match self.with_fallback_heap(|h| unsafe { h.realloc(ptr, old_layout, new_size) }) {
                    Some(ptr) => ptr,
                    None => core::ptr::null_mut(),
                }
            }
            // SAFETY: as in `alloc`. `realloc` takes the C2 in-place fast path
            // for a same-class / compatible resize of an own-thread block, and
            // otherwise falls back to alloc-new + copy + dealloc-old, leaving
            // the old allocation intact on OOM.
            CurrentHeap::Own(heap) => unsafe { (*heap).realloc(ptr, old_layout, new_size) },
        };
        if new_ptr.is_null() && crate::global::process_exit_in_progress() {
            // Also covers `Some(null)` from a live fallback lock whose inner
            // route lookup failed during exit (dead shard-lock holder).
            return exit_escape_realloc(ptr, old_layout, new_size);
        }
        new_ptr
    }
}

#[cfg(all(feature = "internals", feature = "bench-internals"))]
impl SeferAlloc {
    /// Exercise the production dealloc path while this thread already holds
    /// the fallback lock.
    ///
    /// # Safety
    /// `ptr` must be a live allocation produced by this allocator with
    /// `layout`, and must not be freed again.
    #[doc(hidden)]
    pub unsafe fn dbg_dealloc_while_fallback_lock_held(ptr: *mut u8, layout: Layout) {
        let allocator = Self::new();
        let _ = fallback::with_heap(|_| {
            // SAFETY: upheld by this hook's caller contract above.
            unsafe { allocator.dealloc(ptr, layout) };
        });
    }
}
