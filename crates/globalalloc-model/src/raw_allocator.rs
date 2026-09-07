//! The allocator seam: the minimal raw-allocator surface [`drive`](crate::drive) is generic
//! over, plus the blanket impl forwarding every [`GlobalAlloc`] implementor.

use core::alloc::{GlobalAlloc, Layout};

/// A minimal raw-allocator surface: the four `GlobalAlloc` methods over
/// [`Layout`]. The differential [`drive`](crate::drive) loop is generic over this trait.
///
/// A blanket impl covers every [`GlobalAlloc`]; a plain owned allocator with the
/// same four methods can implement it directly. An implementor backed by a
/// `GlobalAlloc` carries `GlobalAlloc`'s stricter preconditions — spelled out
/// in the caller-obligations half of the safety contract below;
/// [`drive`](crate::drive) upholds them by construction.
///
/// # Safety
///
/// This trait is `unsafe` to implement. The implementor must guarantee
/// exactly the first list below and nothing more — it is the minimal
/// contract `drive`'s soundness rests on. A broken allocator that violates
/// the *behavioral* oracles further down is precisely what this crate
/// exists to detect, and detecting one must not itself be undefined
/// behavior.
///
/// ## Guarantees the implementor must provide
///
/// - [`alloc`](RawAllocator::alloc) / [`alloc_zeroed`](RawAllocator::alloc_zeroed)
///   return either null (allocation failed) or a pointer valid for reads
///   and writes of `layout.size()` bytes, inside one live allocation that
///   [`dealloc`](RawAllocator::dealloc) can reclaim later with that same
///   `layout`.
/// - [`realloc`](RawAllocator::realloc) either returns null (leaving the old
///   block live and valid) or a pointer valid for reads and writes of
///   `new_size` bytes, inside one live allocation reclaimable by `dealloc`
///   with the old layout adjusted to `new_size`, consuming the old pointer
///   on a non-null return.
///
/// ## Guarantees the implementor may rely on from callers
///
/// - [`dealloc`](RawAllocator::dealloc) is called only with a pointer previously
///   returned by a matching `alloc`/`alloc_zeroed`/`realloc` on `self` with the
///   same `layout`. When
///   [`Config::double_free`](crate::Config::double_free)
///   is set the harness deliberately
///   frees the SAME pointer a second time (the M2 no-op oracle) — enable it only
///   for an allocator whose contract makes that a safe no-op.
/// - [`realloc`](RawAllocator::realloc) is called only with a pointer previously
///   returned by a matching `alloc`/`alloc_zeroed`/`realloc` on `self` with
///   `old_layout`, and not yet freed or consumed.
/// - A caller going through the blanket [`GlobalAlloc`] impl must additionally
///   uphold `GlobalAlloc`'s stricter preconditions (non-zero `Layout` size,
///   non-zero `realloc` `new_size`, no `isize` overflow after round-up) —
///   stated normatively once, in that impl's `# Safety` note on this page.
///   [`drive`](crate::drive) upholds them by construction (it clamps every op
///   into that range before calling); any other caller of this trait through
///   that impl carries the obligation itself.
///
/// # What the oracles check
///
/// Everything beyond that minimal contract is an *oracle*, not a safety
/// obligation. `drive` verifies these behaviorally and panics on a
/// violation; an implementor MAY return values that fail them (the
/// crate's own negative-oracle suite does exactly that):
///
/// - **Alignment:** a non-null pointer is aligned to `layout.align()`.
/// - **Zeroing:** a non-null pointer from `alloc_zeroed` points at
///   `layout.size()` zero bytes.
/// - **Prefix preservation:** `realloc`'s non-null result's first
///   `min(old_size, new_size)` bytes equal the old block's contents.
/// - **No overlap:** two simultaneously-live blocks never share a byte.
pub unsafe trait RawAllocator {
    /// Allocate `layout.size()` bytes at `layout.align()`; null on failure.
    ///
    /// # Safety
    /// See the [trait-level contract](RawAllocator#safety).
    unsafe fn alloc(&self, layout: Layout) -> *mut u8;

    /// Allocate zeroed `layout.size()` bytes at `layout.align()`; null on failure.
    ///
    /// # Safety
    /// See the [trait-level contract](RawAllocator#safety).
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8;

    /// Free the block `ptr` that was allocated with `layout`.
    ///
    /// # Safety
    /// `ptr` must be a pointer previously returned by a matching
    /// `alloc`/`alloc_zeroed`/`realloc` on `self` with the same `layout`
    /// (see the [trait-level contract](RawAllocator#safety), [caller
    /// obligations](RawAllocator#guarantees-the-implementor-may-rely-on-from-callers)).
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout);

    /// Resize the block `ptr` (allocated with `old_layout`) to `new_size` bytes.
    ///
    /// # Safety
    /// `ptr` must be a pointer previously returned by a matching
    /// `alloc`/`alloc_zeroed`/`realloc` on `self` with `old_layout`, and not
    /// yet freed or consumed (see the
    /// [trait-level contract](RawAllocator#safety), caller obligations).
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8;
}

/// # Safety
///
/// `GlobalAlloc`'s contract is STRICTLY STRONGER than `RawAllocator`'s — a
/// non-zero `Layout` size, a non-zero `realloc` `new_size`, and no `isize`
/// overflow after the alignment round-up. Forwarding is sound only because
/// this crate's sole generic caller, [`drive`](crate::drive), clamps every
/// op into that stricter range before calling (see `drive`'s totality
/// guarantee). A caller outside `drive` going through this impl carries
/// those preconditions itself — see the caller-obligations half of the
/// trait's `# Safety` contract.
unsafe impl<A: GlobalAlloc> RawAllocator for A {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: see the impl-level note above — `drive` guarantees the
        // stricter `GlobalAlloc::alloc` preconditions hold here.
        unsafe { GlobalAlloc::alloc(self, layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: see the impl-level note above — `drive` guarantees the
        // stricter `GlobalAlloc::alloc_zeroed` preconditions hold here.
        unsafe { GlobalAlloc::alloc_zeroed(self, layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: see the impl-level note above — `drive` guarantees the
        // stricter `GlobalAlloc::dealloc` preconditions hold here.
        unsafe { GlobalAlloc::dealloc(self, ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: see the impl-level note above — `drive` guarantees the
        // stricter `GlobalAlloc::realloc` preconditions hold here.
        unsafe { GlobalAlloc::realloc(self, ptr, old_layout, new_size) }
    }
}
