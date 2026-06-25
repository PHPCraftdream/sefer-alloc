//! Phase 11 -- the `malloc` face: [`SeferMalloc`], an `unsafe impl GlobalAlloc`
//! over the Phase 9/10 per-thread `Heap` substrate.
//!
//! This is the **drop-in face** -- the campaign's victory deliverable. One
//! substrate (the segment-backed, self-hosted, per-thread-heap allocator),
//! two faces: the `Handle` face (typed, generational, relocatable) and this
//! `malloc` face (raw `*mut u8`, drop-in `#[global_allocator]` replacement).
//!
//! ## What this module IS
//!
//! - The `unsafe impl GlobalAlloc` -- the trait is `unsafe`, so this is a
//!   documented `unsafe` seam. Every method carries a `// SAFETY:` proof.
//! - Routing: `alloc`/`dealloc`/`realloc`/`alloc_zeroed` delegate to the
//!   per-thread `Heap` via the **no-panic** TLS binding `with_heap_try`
//!   (returns `None` instead of panicking during TLS teardown; on `None` we
//!   return null / no-op).
//!
//! ## M5 (reentrancy-freedom) -- how it is upheld
//!
//! The whole point (§4 M5, §8 of `MALLOC_PLAN.md`): when WE are the global
//! allocator, ANY use of `Vec`/`Box`/`HashSet`/`std::alloc`/`format!` on the
//! alloc path would recurse infinitely. This module contains NONE of those.
//! The TLS access (`with_heap_try`) is a plain thread-local load with a `const`
//! initialiser (no allocation). `Heap::new()` bootstraps via the OS aperture
//! (`mmap`/`VirtualAlloc`) -- never `std::alloc`. The `Heap`'s alloc/dealloc
//! paths are pure safe integer arithmetic + the `node` seam (intrusive pointer
//! r/w). No `std` collection is reachable from here.
//!
//! ## No-panic -- how it is upheld
//!
//! A panic in `#[global_allocator]` aborts the process (§8 of `MALLOC_PLAN.md`).
//! Every entry point here returns null on failure (OOM, TLS unavailable) and
//! NEVER panics:
//! - `alloc`: `with_heap_try` returns `None` → we return null. `Heap::alloc`
//!   returns null on OOM.
//! - `dealloc`: `with_heap_try` returns `None` → no-op (the block leaks; safe
//!   during thread shutdown).
//! - `realloc`: delegates to `alloc` + copy + `dealloc`, all null-returning.
//! - `alloc_zeroed`: `alloc` + zero-fill.
//!
//! The substrate panic sites (`.expect` in `alloc_small`, `assert!` in
//! `register`, `assert!(len > 0)` in `Segment::reserve`) were hardened to
//! graceful null-returning branches in Phase 11.

// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see `src/lib.rs`);
// this is the documented malloc-face seam. `allow` lifts the crate-level `deny`
// for this file only -- `unsafe` anywhere else in the crate is a hard error.
// The ONLY `unsafe` here is the `unsafe impl GlobalAlloc` (the trait is
// `unsafe`) plus the `// SAFETY:`-annotated pointer handoff to the Heap.
#![allow(unsafe_code)]

use core::alloc::{GlobalAlloc, Layout};

use crate::heap::{with_heap_try, Heap};

/// The drop-in `GlobalAlloc` face over the `sefer-alloc` segment substrate.
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
/// Each thread gets its own [`Heap`] (lazily bootstrapped on first allocation
/// via a `const`-initialised TLS slot -- no allocation on the TLS path).
/// `alloc`/`dealloc`/`realloc`/`alloc_zeroed` route through the per-thread
/// heap's lock-free free-list pop/push (the Phase 9 hot path). With
/// `alloc-xthread`, cross-thread `dealloc` routes through the Phase 10 Treiber
/// stack.
///
/// This is the **malloc face** of one substrate; the **handle face**
/// (`Region<T>` / `Handle<T>`) is the typed, generational view over the same
/// governed memory. See `docs/MALLOC_PLAN.md` §3 "The two faces".
pub struct SeferMalloc;

impl SeferMalloc {
    /// Construct the allocator. This is a zero-cost `const` constructor -- the
    /// per-thread heaps are lazily bootstrapped on first use (not here), so
    /// this can be used in `static` initialisers without any allocation or OS
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
// `alloc_zeroed`/`realloc` return valid memory for the requested `Layout` (or
// null on failure), and that `dealloc` receives a pointer previously returned
// by an allocating method. We delegate to `Heap::alloc`/`dealloc`/`realloc`/
// `alloc_zeroed`, which uphold M1 (validity), M3 (no overlap), and M4
// (alignment/size fidelity) -- verified by the Phase 8/9 differential
// proptests and miri. The `Heap` returns null on OOM (never panics -- the
// substrate panic sites were hardened in Phase 11). If the TLS heap is
// unavailable (`with_heap_try` returns `None` -- thread shutdown), `alloc`
// returns null and `dealloc` is a no-op (the block leaks safely; sound because
// the substrate never reuses memory across heaps without the Phase 10 Treiber
// protocol, and a thread in shutdown is exiting).
unsafe impl GlobalAlloc for SeferMalloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller (Rust's allocator infrastructure) guarantees
        // `layout` has non-zero size and valid alignment. We route through the
        // per-thread Heap, which is reentrancy-free (M5: no Vec/Box/std::alloc
        // on the path) and no-panic (returns null on OOM).
        with_heap_try(|heap: &mut Heap| heap.alloc(layout)).unwrap_or(core::ptr::null_mut())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller guarantees `ptr` was returned by a prior `alloc`
        // on this allocator with `layout` and not yet deallocated. We route
        // through the per-thread Heap. If the TLS heap is unavailable (thread
        // shutdown), this is a no-op -- the block is leaked, which is sound
        // (no corruption, no UAF; the memory stays mapped under alloc-xthread's
        // abandonment-leak, or is freed by AllocCore::drop under plain alloc).
        if ptr.is_null() {
            return;
        }
        let _ = with_heap_try(|heap: &mut Heap| heap.dealloc(ptr, layout));
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: same contract as `alloc`; the returned memory is zero-filled.
        // `Heap::alloc_zeroed` allocates then zeroes via the `node` seam.
        with_heap_try(|heap: &mut Heap| heap.alloc_zeroed(layout)).unwrap_or(core::ptr::null_mut())
    }

    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` is a valid prior allocation of
        // `old_layout` not yet deallocated. `Heap::realloc` performs
        // alloc-new + copy + dealloc-old, returning a valid new pointer (or
        // null on OOM, leaving the old allocation intact). If the TLS heap is
        // unavailable, we return null (the old block leaks safely).
        if ptr.is_null() {
            return core::ptr::null_mut();
        }
        with_heap_try(|heap: &mut Heap| heap.realloc(ptr, old_layout, new_size))
            .unwrap_or(core::ptr::null_mut())
    }
}
