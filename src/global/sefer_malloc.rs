//! Phase 12.3 -- the `malloc` face: [`SeferMalloc`], an `unsafe impl GlobalAlloc`
//! over the global heap registry (Phase 12.2) via raw-pointer TLS (Phase 12.3).
//!
//! This is the **drop-in face** -- the campaign's victory deliverable. One
//! substrate (the segment-backed, self-hosted, registry-resident heap
//! allocator), two faces: the `Handle` face (typed, generational,
//! relocatable) and this `malloc` face (raw `*mut u8`, drop-in
//! `#[global_allocator]` replacement).
//!
//! ## Phase 12.3 rewiring
//!
//! Previously (Phase 11) this face routed through
//! `RefCell<Option<Heap>>` via `with_heap_try`. That binding ABORTED under
//! libtest's reentrant harness: `RefCell::try_borrow_mut` returns `Err` on
//! a reentrant borrow → the malloc face returned null → std aborted.
//!
//! Phase 12.3 replaces that with [`tls_heap::current`](super::tls_heap::current):
//! a raw `Cell<*mut HeapCore>` TLS cache (no borrow state to fail) over the
//! global [`HeapRegistry`](crate::registry::HeapRegistry). The heap lives in
//! a registry slot (not in TLS); thread exit abandons + recycles the slot
//! (not drops the heap). The malloc face is therefore **reentrancy-safe**
//! (M5) and **never-null** (M10): [`current`] returns a non-null pointer in
//! every case (cached slot, fresh claim, or the process-global fallback
//! heap).
//!
//! ## M5 (reentrancy-freedom) -- how it is upheld
//!
//! The whole point (§4 M5, §8 of `MALLOC_PLAN.md`): when WE are the global
//! allocator, ANY use of `Vec`/`Box`/`HashSet`/`std::alloc`/`format!` on the
//! alloc path would recurse infinitely. This module contains NONE of those.
//! `current()` is a plain thread-local load + null check. `bind_slow` claims
//! a registry slot (which bootstraps via the OS aperture, never `std::alloc`);
//! the only `std::alloc` touch is the `Box<AtomicPtr<u8>>` TFS handle under
//! `alloc-xthread`, installed on the bind path (outside the registry
//! bootstrap). The `HeapCore` alloc/dealloc paths are pure safe integer
//! arithmetic + the `node` seam (intrusive pointer r/w). No `std` collection
//! is reachable from here.
//!
//! ## No-panic -- how it is upheld
//!
//! A panic in `#[global_allocator]` aborts the process (§8 of `MALLOC_PLAN.md`).
//! Every entry point here returns null on failure and NEVER panics:
//! - `alloc`: `current()` → `&mut HeapCore` → `HeapCore::alloc` (returns
//!   null on OOM). If `current()` itself yields the fallback (TLS teardown),
//!   the fallback's `with_heap` returns `None` only on true OOM → null.
//! - `dealloc`: `current()` → `HeapCore::dealloc`. If TLS is torn down, the
//!   fallback's `with_heap` deallocs under the spinlock; a torn-down-TLS
//!   dealloc still routes correctly (the segment's owner routes via the
//!   header). On any failure this is a no-op (the block is leaked safely).
//! - `realloc`: `alloc` + copy + `dealloc`, all null-returning.
//! - `alloc_zeroed`: `alloc` + zero-fill.
//!
//! [`current`]: super::tls_heap::current

// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see `src/lib.rs`);
// this is the documented malloc-face seam. `allow` lifts the crate-level
// deny for this file only -- `unsafe` anywhere else in the crate is a hard
// error. The ONLY `unsafe` here is the `unsafe impl GlobalAlloc` (the trait
// is `unsafe`) plus the `// SAFETY:`-annotated pointer handoff to HeapCore.
#![allow(unsafe_code)]

use core::alloc::{GlobalAlloc, Layout};

use super::fallback;
use super::tls_heap::{current_for_alloc, CurrentHeap};

/// The drop-in `GlobalAlloc` face over the `sefer-alloc` segment substrate,
/// routed through the global heap registry (Phase 12.2) via raw-pointer TLS
/// (Phase 12.3).
///
/// Install it as your process's global allocator:
///
/// ```no_run
/// # #[cfg(feature = "alloc-global")]
/// # {
/// use sefer_alloc::SeferMalloc;
///
/// #[global_allocator]
/// static A: SeferMalloc = SeferMalloc::new();
/// # }
/// ```
///
/// Each thread gets its own heap slot in the global registry (lazily claimed
/// on first allocation via the raw-pointer TLS binding -- no `RefCell`, no
/// reentrant-borrow failure). `alloc`/`dealloc`/`realloc`/`alloc_zeroed`
/// route through the per-thread heap's segment-centric `BinTable` free lists
/// (the Phase 12.1 hot path). With `alloc-xthread`, cross-thread `dealloc`
/// routes through the Phase 10 Treiber stack, now stamped from the
/// registry-resident heap (12.3 owner stamping).
///
/// Thread exit abandons the heap's segments back to the registry (a no-op
/// stub in 12.3 -- segments leak until 12.4 adoption; bounded and sound) and
/// recycles the slot for reuse. A primordial fallback heap (§2.3) serves
/// the pre-TLS / post-teardown windows, so the face is **never-null for a
/// serviceable request** (M10).
///
/// This is the **malloc face** of one substrate; the **handle face**
/// (`Region<T>` / `Handle<T>`) is the typed, generational view over the same
/// governed memory. See `docs/MALLOC_PLAN.md` §3 "The two faces".
pub struct SeferMalloc;

impl SeferMalloc {
    /// Construct the allocator. This is a zero-cost `const` constructor -- the
    /// per-thread heaps are lazily claimed on first use (not here), so this
    /// can be used in `static` initialisers without any allocation or OS
    /// calls.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for SeferMalloc {
    fn default() -> Self {
        Self::new()
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
// TLS heap is unavailable (thread teardown), `current()` returns the
// process-global fallback heap (never null); `dealloc` on the fallback is
// sound under the fallback's spinlock. M10 (never-null for serviceable
// requests) is upheld: the only null return is true OOM.
unsafe impl GlobalAlloc for SeferMalloc {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match current_for_alloc() {
            // Fallback path (TLS torn down, registry exhausted, or true
            // fallback OOM): route through the fallback's spinlock-guarded
            // `with_heap`. `with_heap` returns `None` only on true OOM → we
            // surface null.
            CurrentHeap::Fallback => {
                fallback::with_heap(|h| h.alloc(layout)).unwrap_or(core::ptr::null_mut())
            }
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in
            // a registry slot. `current_for_alloc` returned it for THIS
            // thread; the single-writer invariant (the CAS-won slot owner)
            // makes `&mut` access exclusive. `HeapCore::alloc` upholds the
            // GlobalAlloc contract (returns valid memory or null).
            CurrentHeap::Own(heap) => unsafe { (*heap).alloc(layout) },
        }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        match current_for_alloc() {
            CurrentHeap::Fallback => {
                // Fallback path: dealloc under the spinlock. A failure here
                // (true OOM at fallback init) is a safe no-op — the block
                // is leaked, never corrupted.
                let _ = fallback::with_heap(|h| h.dealloc(ptr, layout));
            }
            // SAFETY: as above; `dealloc` is a safe no-op on a
            // foreign/dangling pointer (M2 defence-in-depth), so even if
            // `ptr` was allocated on a different thread's heap, this routes
            // correctly (own-thread → BinTable; cross-thread → TFS under
            // `alloc-xthread`).
            CurrentHeap::Own(heap) => unsafe { (*heap).dealloc(ptr, layout) },
        }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        match current_for_alloc() {
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
        match current_for_alloc() {
            CurrentHeap::Fallback => fallback::with_heap(|h| h.realloc(ptr, old_layout, new_size))
                .unwrap_or(core::ptr::null_mut()),
            // SAFETY: as in `alloc`; `realloc` is alloc-new + copy +
            // dealloc-old, leaving the old allocation intact on OOM.
            CurrentHeap::Own(heap) => unsafe { (*heap).realloc(ptr, old_layout, new_size) },
        }
    }
}
