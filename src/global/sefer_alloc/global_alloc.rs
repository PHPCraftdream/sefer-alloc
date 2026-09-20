// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see `src/lib.rs`);
// this is the documented alloc-face seam. `allow` lifts the crate-level
// deny for this file only -- `unsafe` anywhere else in the crate is a hard
// error. The ONLY `unsafe` here is the `unsafe impl GlobalAlloc` (the trait
// is `unsafe`) plus the `// SAFETY:`-annotated pointer handoff to HeapCore.
#![allow(unsafe_code)]

use core::alloc::{GlobalAlloc, Layout};

use crate::global::fallback;
use crate::global::tls_heap::CurrentHeap;
#[cfg(feature = "alloc-xthread")]
use crate::global::tls_heap::{current_for_dealloc, CurrentHeapForDealloc};

use super::SeferAlloc;

// SAFETY (the trait obligation): `GlobalAlloc` requires that `alloc`/
// `alloc_zeroed`/`realloc` return valid memory for the requested `Layout`
// (or null on failure), and that `dealloc` receives a pointer previously
// returned by an allocating method. We delegate to `HeapCore::alloc`/
// `dealloc`/`realloc`/`alloc_zeroed`, which uphold M1 (validity), M3 (no
// overlap), and M4 (alignment/size fidelity) -- verified by the Phase 8/9
// differential proptests and miri. `HeapCore` returns null on OOM (never
// panics -- the substrate panic sites were hardened in Phase 11). If the
// TLS heap is unavailable (thread teardown), `current()` returns the
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
            CurrentHeap::Fallback => {
                fallback::with_heap(|h| h.alloc(layout)).unwrap_or(core::ptr::null_mut())
            }
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
        // R6-OPT-P0-1: under `alloc-xthread`, resolve via the DEALLOC-ONLY
        // `current_for_dealloc` — NOT `self.current_heap()` — so a thread
        // whose TLS is null (never allocated anything itself) or `TORN`
        // (already exited) does not pay to claim a registry slot or take the
        // fallback spinlock just to free one foreign pointer. See
        // `tls_heap::current_for_dealloc`'s doc comment for the full
        // rationale, and the "TORN + fallback-owned" trade-off note below.
        #[cfg(feature = "alloc-xthread")]
        {
            match current_for_dealloc() {
                CurrentHeapForDealloc::Own(heap) => {
                    // SAFETY: as the `Own` arm below — `heap` is non-null and
                    // points to a live `HeapCore` in a registry slot this
                    // thread owns (single-writer invariant).
                    unsafe { (*heap).dealloc(ptr, layout) };
                }
                CurrentHeapForDealloc::ForeignNoBind => {
                    // This thread never bound a heap, or its heap's slot was
                    // already recycled (TORN), or its TLS is torn down. Every
                    // valid pointer reaching `dealloc` here is foreign BY
                    // CONSTRUCTION (see `current_for_dealloc`'s doc comment):
                    // route it directly through the heap-instance-independent
                    // cross-thread routing tail, WITHOUT claiming a registry
                    // slot and WITHOUT constructing or dereferencing any
                    // `*mut HeapCore` at all.
                    //
                    // Deliberate, documented trade-off (verified sound — see
                    // `HeapCore::dealloc_foreign_routing`'s doc comment and
                    // the R6-OPT-P0-1 task report): for the TORN case
                    // specifically, the OLD code routed through
                    // `fallback::with_heap`, which checked the FALLBACK
                    // heap's OWN `contains_base` FIRST — so a pointer that
                    // genuinely belongs to the fallback's own segments took
                    // the direct free path under the lock. This shortcut has
                    // no fallback `HeapCore` instance to consult, so it
                    // ALWAYS treats a TORN thread's dealloc as foreign-by-
                    // header, pushing onto whatever ring the header says
                    // owns it — for a fallback-owned pointer, that means the
                    // fallback's OWN ring instead of a direct free. This is
                    // NOT a correctness bug: pushing to a ring is safe for
                    // ANY live segment regardless of caller identity (see
                    // `dealloc_foreign_routing`'s doc comment), and the
                    // fallback drains its own ring lazily on its next
                    // `with_heap` call exactly like any other segment's
                    // owner — it is a narrow efficiency trade-off in an
                    // already-rare corner case (TORN AND fallback-owned),
                    // traded for removing the claim/lock cost in the
                    // overwhelmingly common case this task targets.
                    //
                    // SAFETY: `ptr`/`layout` are the caller-bound
                    // `GlobalAlloc::dealloc` contract pair (this whole fn is
                    // `unsafe fn dealloc`); `dealloc_foreign_routing` applies
                    // the SAME null-base and magic-mismatch guards
                    // `dealloc_foreign_slow` already uses before touching any
                    // segment memory, so a LIVE-but-foreign `ptr` (the case
                    // this arm exists for) is routed or rejected without
                    // faulting. This is NOT a blanket "safe on any
                    // dangling/garbage pointer" claim — identical scope to the
                    // `not(alloc-xthread)` arm below: a pointer into an
                    // already-RELEASED, unmapped segment faults on the header
                    // read in either path, and excluding that case is the
                    // caller's baseline `GlobalAlloc` obligation, not
                    // something these guards relax.
                    let base = crate::alloc_core::os::segment_base_of_ptr(ptr);
                    crate::registry::HeapCore::dealloc_foreign_routing(ptr, base, layout, None);
                }
            }
        }
        #[cfg(not(feature = "alloc-xthread"))]
        {
            // Without `alloc-xthread` there is no heap-instance-independent
            // routing concept (no owner stamp, no per-segment
            // `RemoteFreeRing`) — fall back to the OLD behavior: resolve via
            // `current_heap()` (bind/fallback as before). Do not attempt the
            // P0-1 shortcut when cross-thread routing does not exist.
            match self.current_heap() {
                CurrentHeap::Fallback => {
                    // Fallback path: dealloc under the spinlock. A failure
                    // here (true OOM at fallback init) is a safe no-op — the
                    // block is leaked, never corrupted.
                    //
                    // SAFETY: `ptr`/`layout` are the caller-bound GlobalAlloc
                    // contract pair (this whole fn is `unsafe fn dealloc`);
                    // the closure forwards them to the fallback heap's
                    // `HeapCore::dealloc`.
                    let _ = fallback::with_heap(|h| unsafe { h.dealloc(ptr, layout) });
                }
                // SAFETY: as above. For a LIVE/MAPPED pointer this routes
                // correctly regardless of which thread allocated it
                // (own-thread only, without `alloc-xthread`), and the M2
                // double-free guard makes a repeated free of a still-mapped
                // block a no-op. This is NOT a blanket "safe on any
                // foreign/dangling pointer" claim: a dangling pointer into an
                // already-RELEASED, unmapped segment is fundamentally UB —
                // not calling `dealloc` on an already-freed pointer is the
                // caller's baseline `GlobalAlloc` obligation (a basic trait
                // contract, not something M2 relaxes); M2 hardens the
                // live-block case, it does not extend the contract to
                // released memory.
                CurrentHeap::Own(heap) => unsafe { (*heap).dealloc(ptr, layout) },
            }
        }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        match self.current_heap() {
            CurrentHeap::Fallback => {
                fallback::with_heap(|h| h.alloc_zeroed(layout)).unwrap_or(core::ptr::null_mut())
            }
            // SAFETY: as in `alloc`.
            CurrentHeap::Own(heap) => unsafe { (*heap).alloc_zeroed(layout) },
        }
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return core::ptr::null_mut();
        }
        match self.current_heap() {
            // SAFETY: `ptr`/`old_layout` are the caller-bound GlobalAlloc
            // contract pair (this whole fn is `unsafe fn realloc`); the
            // closure forwards them to the fallback heap's `HeapCore::realloc`.
            CurrentHeap::Fallback => {
                fallback::with_heap(|h| unsafe { h.realloc(ptr, old_layout, new_size) })
                    .unwrap_or(core::ptr::null_mut())
            }
            // SAFETY: as in `alloc`. `realloc` takes the C2 in-place fast path
            // for a same-class / compatible resize of an own-thread block, and
            // otherwise falls back to alloc-new + copy + dealloc-old, leaving
            // the old allocation intact on OOM.
            CurrentHeap::Own(heap) => unsafe { (*heap).realloc(ptr, old_layout, new_size) },
        }
    }
}
