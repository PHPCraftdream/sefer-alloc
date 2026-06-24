//! [`ByteAllocator`] — a `Sync` wrapper over [`ByteRegion`] that implements
//! [`GlobalAlloc`](std::alloc::GlobalAlloc) by delegating to a
//! `Mutex<ByteRegion>`.
//!
//! This is the experimental `unsafe impl GlobalAlloc` of Phase 4. Its
//! *intelligence* (size classes, free lists) lives in the safe Cartographer
//! ([`ByteRegion`]); the only `unsafe` here is the [`GlobalAlloc`] trait
//! obligation (the trait is `unsafe`) plus the unavoidable raw-pointer
//! handoff. Every `unsafe` block below carries a `// SAFETY:` comment.
//!
//! ## Honest scope
//!
//! This is research, not a production allocator. It is NOT installed as the
//! crate's `#[global_allocator]` (that would replace the allocator for the whole
//! test binary — dangerous); callers opt in by constructing a
//! [`ByteAllocator`] and installing it themselves if they wish. See
//! `docs/PLAN.md` and `docs/BYTE_BENCH.md`.

// The crate is `#![deny(unsafe_code)]` with `byte` on (see `src/lib.rs`); this
// is the documented confined-unsafe module for the `GlobalAlloc` wrapper. The
// `unsafe` here is the trait obligation plus the raw-pointer handoff to
// `ByteRegion`'s aperture. `allow` lifts the crate-level `deny` for this file
// only.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::Mutex;

use crate::byte::byte_region::ByteRegion;

/// A thread-safe (`Sync`) wrapper over a [`ByteRegion`] that implements
/// [`GlobalAlloc`].
///
/// All allocations are serialised through a single [`Mutex<ByteRegion>`]; this
/// is correct (no data races) but not lock-free. It exists to honour the design
/// descent to the `GlobalAlloc` boundary, not to compete with `mimalloc`.
///
/// Construct it with [`new`](Self::new) and either call its methods directly
/// (via `GlobalAlloc`) or install it as a `#[global_allocator]` if you accept
/// the research-flagged scope.
pub struct ByteAllocator {
    inner: Mutex<ByteRegion>,
}

// SAFETY: `ByteRegion` is NOT auto-`Send`/`Sync` — its `large: HashSet<*mut u8>`
// holds raw pointers, which are `!Send`/`!Sync` by default, so neither the
// region nor `Mutex<ByteRegion>` derives the auto traits. We assert them
// manually on the wrapper because the impl is genuinely sound:
//  - ALL access to the region (and to those raw pointers) goes through the
//    inner `Mutex`, which serialises every entry point — no two threads ever
//    touch the bookkeeping concurrently (no data race).
//  - The pointers in `large` are only ever used as set keys and handed back to
//    `std::alloc::dealloc`/`realloc` under the lock; they are never dereferenced
//    by the allocator, and the memory they name is process-global (valid on any
//    thread). Moving the allocator to another thread (`Send`) or sharing `&` it
//    (`Sync`) is therefore sound — this is the standard property of any
//    allocator (memory allocated on one thread may be freed on another).
// The manual impls are required because `unsafe impl GlobalAlloc` needs `Sync`.
unsafe impl Send for ByteAllocator {}
unsafe impl Sync for ByteAllocator {}

impl ByteAllocator {
    /// Creates a new empty allocator backed by a fresh [`ByteRegion`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ByteRegion::new()),
        }
    }

    /// Returns the number of backing chunks currently held (mirrors
    /// [`ByteRegion::chunk_count`] through the mutex). Exposed
    /// (`#[doc(hidden)]`) for tests that assert bounded growth under churn.
    #[doc(hidden)]
    #[must_use]
    pub fn chunk_count(&self) -> usize {
        self.inner
            .lock()
            .expect("byte allocator mutex not poisoned")
            .chunk_count()
    }
}

impl Default for ByteAllocator {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY (the trait obligation): the `GlobalAlloc` contract requires that the
// implementor uphold the documented safety invariants of each method — chiefly
// that `alloc`/`alloc_zeroed`/`realloc` return valid memory for the requested
// `Layout` (or null), and that `dealloc` receives exactly a pointer previously
// returned by an allocating method along with its original `Layout`.
// `ByteRegion` upholds these: it either carves from pinned, owned chunks or
// delegates to `std::alloc`. We lock the mutex on every entry point so there is
// exactly one thread operating on the region at a time (no data race). The
// pointer handoff is the single irreducible `*mut u8` aperture documented in
// `byte_region`.
unsafe impl GlobalAlloc for ByteAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: we hold no raw pointers across the lock; `ByteRegion::alloc`
        // returns a pointer into the region's own pinned memory (or a
        // system-allocated pointer recorded for dealloc). The mutex guarantees
        // exclusive access to the region's mutable free-list state.
        let mut region = self.inner.lock().expect("byte allocator mutex not poisoned");
        region.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller guarantees `ptr` was returned by a prior `alloc`
        // on this allocator with `layout` (the `GlobalAlloc` contract), and has
        // not been deallocated already. `ByteRegion::dealloc` routes it to the
        // correct backend (system allocator for large, class free list
        // otherwise). The mutex serialises access.
        let mut region = self.inner.lock().expect("byte allocator mutex not poisoned");
        region.dealloc(ptr, layout);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: same as `alloc` — the mutex serialises, and `ByteRegion`
        // returns valid memory (or null) for the layout.
        let mut region = self.inner.lock().expect("byte allocator mutex not poisoned");
        region.alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` is a valid prior allocation of
        // `old_layout` not yet deallocated (`GlobalAlloc` contract).
        // `ByteRegion::realloc` either delegates to the system allocator (for
        // large) or performs alloc + copy + dealloc, returning a valid new
        // pointer (or null on OOM). The mutex serialises access.
        let mut region = self.inner.lock().expect("byte allocator mutex not poisoned");
        region.realloc(ptr, old_layout, new_size)
    }
}
