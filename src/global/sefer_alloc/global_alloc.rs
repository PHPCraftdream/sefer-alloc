// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see `src/lib.rs`);
// this is the documented alloc-face seam. `allow` lifts the crate-level deny
// for this file only. Unsafe operations include raw pointer handoff to
// `HeapCore`, `System` allocation, realloc pointer copies, and the unsafe
// diagnostic hook below; each block has a local `// SAFETY:` justification.
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

/// Outlined `CurrentHeap::Fallback` arm of [`GlobalAlloc::alloc`] so the hot
/// own-heap path stays a leaf: every non-hit arm is a tail call with ≤ 4
/// register arguments (Win64). Behaviour is byte-identical to the former
/// inline match (including the shared null + process-exit escape check that
/// followed the match — it stays here, on the cold arm, unchanged).
#[inline(never)]
fn alloc_via_fallback(sefer: &SeferAlloc, layout: Layout) -> *mut u8 {
    let ptr = match sefer.with_fallback_heap(|h| h.alloc(layout)) {
        Some(ptr) => ptr,
        None => core::ptr::null_mut(),
    };
    if ptr.is_null() && crate::global::process_exit_in_progress() {
        // Cold null path only: the route is unavailable because a lock
        // holder was terminated by `ExitProcess`. Owner decision: escape
        // to System, never abort (docs/reviews/2026-10-11, question 7).
        // SAFETY: `layout` is the caller's valid GlobalAlloc layout.
        return exit_escape_alloc(layout, false);
    }
    ptr
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

#[cfg(feature = "fastbin")]
#[cold]
#[inline(never)]
fn alloc_null_slow(layout: Layout) -> *mut u8 {
    if crate::global::process_exit_in_progress() {
        exit_escape_alloc(layout, false)
    } else {
        core::ptr::null_mut()
    }
}

/// # Safety
/// heap — живой HeapCore текущего owner; class получен из того же layout.
#[cfg(feature = "fastbin")]
#[cold]
#[inline(never)]
unsafe fn alloc_bound_small_slow(
    heap: *mut crate::registry::HeapCore,
    layout: Layout,
    class: usize,
) -> *mut u8 {
    // SAFETY: owner/layout/class сохранены синхронным вызывающим путём.
    let ptr = unsafe { (*heap).alloc_with_class(layout, Some(class)) };
    if ptr.is_null() {
        alloc_null_slow(layout)
    } else {
        ptr
    }
}

/// # Safety
/// heap — живой HeapCore текущего owner; layout классифицирован как Large.
#[cfg(feature = "fastbin")]
#[cold]
#[inline(never)]
unsafe fn alloc_bound_large_slow(heap: *mut crate::registry::HeapCore, layout: Layout) -> *mut u8 {
    // SAFETY: owner/layout сохранены; None — прежний Large результат classifier.
    let ptr = unsafe { (*heap).alloc_with_class(layout, None) };
    if ptr.is_null() {
        alloc_null_slow(layout)
    } else {
        ptr
    }
}

#[cfg(feature = "fastbin")]
#[cold]
#[inline(never)]
fn alloc_unbound_slow(sefer: &SeferAlloc, layout: Layout) -> *mut u8 {
    let ptr = match sefer.current_heap() {
        CurrentHeap::Fallback => return alloc_via_fallback(sefer, layout),
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

// SAFETY: M4 (`docs/INVARIANTS.md`): `CurrentHeap::Own` calls `HeapCore`; locked
// `CurrentHeap::Fallback` calls the fallback heap. Alloc, zeroed alloc, and
// realloc obey GlobalAlloc layout and failure contracts; null can result from
// checked geometry, `MAX_SEGMENTS` capacity, route registration, reservation
// or commit, `System` class-leaf/spill metadata, or fallback failure. Dealloc
// uses `current_for_dealloc`: Own calls `HeapCore`, ForeignNoBind calls
// `HeapCore::publish_foreign` without binding. `exact-object-proto` narrow
// alloc returns directly before ordinary routing and process-exit escape,
// including when that result is null. Otherwise alloc/zeroed alloc may use
// `exit_escape_alloc` on process-exit null; realloc uses `exit_escape_realloc`.
// Both obtain System storage that intentionally leaks at exit. Exit realloc
// copies `min(old, new)` bytes and retains/leaks the old block; ordinary
// failures still return null. The fatal abort policy is in `sefer_alloc/mod.rs`;
// other paths may panic, so this is not a no-panic guarantee.
unsafe impl GlobalAlloc for SeferAlloc {
    #[inline(always)]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        #[cfg(feature = "exact-object-proto")]
        if ExactNarrow::is_narrow(layout) {
            return ExactNarrow::alloc(layout, false);
        }
        #[cfg(feature = "fastbin")]
        {
            if let Some(heap) = crate::global::tls_heap::current_for_trim() {
                use crate::alloc_core::size_classes::{SizeClasses, MIN_BLOCK};
                let class = SizeClasses::class_for(layout.size().max(MIN_BLOCK), layout.align());
                if let Some(c) = class {
                    // SAFETY: пассивный TLS lookup дал живой exclusive owner HeapCore.
                    if let Some(ptr) = unsafe { (*heap).try_alloc_magazine_class(c) } {
                        return if ptr.is_null() {
                            alloc_null_slow(layout)
                        } else {
                            ptr
                        };
                    }
                    // SAFETY: тот же owner/class/layout; miss не мутировал magazine.
                    return unsafe { alloc_bound_small_slow(heap, layout, c) };
                }
                // SAFETY: тот же owner/layout; classifier вернул None.
                return unsafe { alloc_bound_large_slow(heap, layout) };
            }
            return alloc_unbound_slow(self, layout);
        }
        #[cfg(not(feature = "fastbin"))]
        {
            let ptr = match self.current_heap() {
                CurrentHeap::Fallback => return alloc_via_fallback(self, layout),
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
    }

    #[inline(always)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // X-null: no hot null check. Null's memo/own_cache key is 0 and never
        // matches a stored segment base or `LAST_EMPTY_KEY`, so null always
        // misses into `publish_foreign`, whose first line null-checks before
        // any counter or action (same observable no-op).
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
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub unsafe fn dbg_dealloc_while_fallback_lock_held(ptr: *mut u8, layout: Layout) {
        let allocator = Self::new();
        let _ = fallback::with_heap(|_| {
            // SAFETY: upheld by this hook's caller contract above.
            unsafe { allocator.dealloc(ptr, layout) };
        });
    }
}
