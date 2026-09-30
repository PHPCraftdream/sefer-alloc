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
// requests) is upheld: the only null return is true OOM.
unsafe impl GlobalAlloc for SeferAlloc {
    #[inline(always)]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.current_heap() {
            // Fallback path (TLS torn down, registry exhausted, or true
            // fallback OOM): route through the fallback's spinlock-guarded
            // `with_heap`. `with_heap` returns `None` only on true OOM → we
            // surface null.
            CurrentHeap::Fallback => self
                .with_fallback_heap(|h| h.alloc(layout))
                .unwrap_or(core::ptr::null_mut()),
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in
            // a registry slot. `current_heap` returned it for THIS thread;
            // the single-writer invariant (the CAS-won slot owner) makes
            // `&mut` access exclusive. `HeapCore::alloc` upholds the
            // GlobalAlloc contract (returns valid memory or null).
            CurrentHeap::Own(heap) => unsafe { (*heap).alloc(layout) },
        }
    }

    #[inline(always)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
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
        match self.current_heap() {
            CurrentHeap::Fallback => self
                .with_fallback_heap(|h| h.alloc_zeroed(layout))
                .unwrap_or(core::ptr::null_mut()),
            // SAFETY: as in `alloc`.
            CurrentHeap::Own(heap) => unsafe { (*heap).alloc_zeroed(layout) },
        }
    }

    // #1987: `#[inline(always)]` — see `alloc_zeroed` above for the rationale
    // (all four `GlobalAlloc` methods share one dispatch shape and now one
    // inlining policy).
    #[inline(always)]
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return core::ptr::null_mut();
        }
        match self.current_heap() {
            // SAFETY: `ptr`/`old_layout` are the caller-bound GlobalAlloc
            // contract pair (this whole fn is `unsafe fn realloc`); the
            // closure forwards them to the fallback heap's `HeapCore::realloc`.
            CurrentHeap::Fallback => self
                .with_fallback_heap(|h| unsafe { h.realloc(ptr, old_layout, new_size) })
                .unwrap_or(core::ptr::null_mut()),
            // SAFETY: as in `alloc`. `realloc` takes the C2 in-place fast path
            // for a same-class / compatible resize of an own-thread block, and
            // otherwise falls back to alloc-new + copy + dealloc-old, leaving
            // the old allocation intact on OOM.
            CurrentHeap::Own(heap) => unsafe { (*heap).realloc(ptr, old_layout, new_size) },
        }
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
