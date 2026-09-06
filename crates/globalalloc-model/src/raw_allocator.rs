//! The allocator seam: the minimal raw-allocator surface [`drive`](crate::drive) is generic
//! over, plus the blanket impl forwarding every [`GlobalAlloc`] implementor.

use core::alloc::{GlobalAlloc, Layout};

/// A minimal raw-allocator surface: the four `GlobalAlloc` methods over
/// [`Layout`]. The differential [`drive`](crate::drive) loop is generic over this trait.
///
/// A blanket impl covers every [`GlobalAlloc`]; a plain owned allocator with the
/// same four methods can implement it directly. An implementor backed by a
/// `GlobalAlloc` carries `GlobalAlloc`'s stricter preconditions (see the
/// blanket impl below); `drive` upholds them by construction.
///
/// # Safety
///
/// This trait is `unsafe` to implement. An implementor must behave as a correct
/// allocator so the oracles are testing the allocator, not papering over a
/// broken trait impl:
///
/// - [`alloc`](RawAllocator::alloc) / [`alloc_zeroed`](RawAllocator::alloc_zeroed)
///   return either null (allocation failed) or a pointer valid for reads and
///   writes over `layout.size()` bytes and aligned to `layout.align()`. A
///   non-null pointer from `alloc_zeroed` points at `layout.size()` zero bytes.
/// - [`dealloc`](RawAllocator::dealloc) is called only with a pointer previously
///   returned by a matching `alloc`/`alloc_zeroed`/`realloc` on `self` with the
///   same `layout`. When
///   [`Config::double_free`](crate::Config::double_free)
///   is set the harness deliberately
///   frees the SAME pointer a second time (the M2 no-op oracle) — enable it only
///   for an allocator whose contract makes that a safe no-op.
/// - [`realloc`](RawAllocator::realloc) either returns null (leaving the old
///   block live and valid) or a pointer valid for `new_size` bytes whose first
///   `min(old_size, new_size)` bytes equal the old block's, consuming the old
///   pointer on a non-null return.
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
    /// See the [trait-level contract](RawAllocator#safety).
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout);

    /// Resize the block `ptr` (allocated with `old_layout`) to `new_size` bytes.
    ///
    /// # Safety
    /// See the [trait-level contract](RawAllocator#safety).
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8;
}

// SAFETY (impl-level, applies to every forwarding line below): `GlobalAlloc`'s
// contract is STRICTLY STRONGER than `RawAllocator`'s — a non-zero `Layout`
// size, a non-zero `realloc` `new_size`, and no `isize` overflow after the
// alignment round-up. Forwarding is sound only because this crate's sole
// generic caller, `drive`, clamps every op into that stricter range before
// calling (see `drive`'s P0-1 clamping). The per-method notes below reference
// this impl-level reasoning rather than restating it.
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
