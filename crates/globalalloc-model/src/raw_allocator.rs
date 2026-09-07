//! The allocator seam: the minimal raw-allocator surface [`drive`](crate::drive) is generic
//! over, plus the blanket impl forwarding every [`GlobalAlloc`] implementor.

use core::alloc::{GlobalAlloc, Layout};

/// The four `GlobalAlloc`-shaped operations used by [`drive`](crate::drive).
///
/// A blanket implementation covers every [`GlobalAlloc`]. An owned allocator
/// engine can implement this trait directly when testing through the installed
/// global allocator would mix driver bookkeeping into the allocator's state.
///
/// # Safety
///
/// This trait is unsafe to implement because [`drive`](crate::drive) accesses
/// returned raw memory. The guarantees below are the minimum needed to make
/// those accesses defined. Alignment, zero values, prefix values, and
/// non-overlap remain behavioral oracles where the driver can inspect them
/// before an unsafe access.
///
/// ## Guarantees the implementor must provide
///
/// Every non-null returned extent lies within one allocated object, with
/// provenance permitting the documented accesses. Initialized bytes remain
/// initialized for its live lifetime; the allocator must not race the caller's
/// reads or writes. Behavioral value corruption may still be diagnosed.
///
/// - [`alloc`](RawAllocator::alloc) returns null or an extent of
///   `layout.size()` bytes that the caller may initialize by writing. Once
///   initialized, those bytes may be read. The extent remains allocated and
///   valid until a matching [`dealloc`](RawAllocator::dealloc), or until a
///   matching [`realloc`](RawAllocator::realloc) returns non-null and consumes
///   it.
/// - [`alloc_zeroed`](RawAllocator::alloc_zeroed) has the same lifetime and
///   extent guarantee, and all returned bytes are initialized. Whether their
///   values are actually zero is an oracle checked by [`drive`](crate::drive).
/// - [`realloc`](RawAllocator::realloc) returning null leaves the old extent
///   allocated, valid, and unmodified. A non-null return consumes the old
///   pointer and yields a new `new_size`-byte extent, allocated with
///   `old_layout` adjusted to `new_size`. Because callers must initialize the
///   first `min(old_layout.size(), new_size)` old bytes before calling, the
///   corresponding returned prefix must remain initialized and may be read.
///   Whether those returned values equal the old values is the prefix oracle.
///   The new extent remains valid until its own matching `dealloc` or a later
///   non-null `realloc`.
///
/// A broken implementor may fail the behavioral oracles, but it must not break
/// these extent, liveness, or initialization guarantees. Such a break can make
/// the driver itself execute undefined behavior and therefore cannot be
/// promised as a clean test failure.
///
/// ## Guarantees the implementor may rely on from callers
///
/// - Every layout passed to `alloc` or `alloc_zeroed` has non-zero size and is
///   otherwise admissible under `Layout`. A `realloc` request has non-zero
///   `new_size`, and rounding it up to `old_layout.align()` does not exceed
///   `isize::MAX`.
/// - `dealloc` receives a currently live block returned by this allocator and
///   the exact layout currently associated with it. The only exception is the
///   M2 driver's one immediate repetition of the just-completed matching
///   `dealloc`, with no intervening allocator call. That exception is
///   authorized only when [`Config::double_free`](crate::Config::double_free)
///   contains a [`DoubleFreeOk`](crate::DoubleFreeOk) token whose safety
///   contract the caller has upheld. All other deallocations require a live
///   block.
/// - `realloc` receives a currently live block and its exact current layout;
///   the block has not already been freed or consumed. Its first
///   `min(old_layout.size(), new_size)` bytes are initialized. `drive`
///   satisfies this by filling and verifying every block before reallocation.
///
/// These size, layout, liveness, and realloc-prefix requirements are common to
/// every implementation, including the blanket `GlobalAlloc` implementation;
/// they are not a stronger contract introduced only at that forwarding seam.
///
/// # What the oracles check
///
/// Subject to the safety contract above, [`drive`](crate::drive) checks:
///
/// - non-null results from `alloc` and `alloc_zeroed` (the driver treats null
///   as a test failure even though `GlobalAlloc` permits it),
/// - alignment of each non-null return,
/// - zero values from `alloc_zeroed`,
/// - preservation of the initialized realloc prefix,
/// - no overlap among simultaneously live extents, and
/// - fill-byte read-back at documented observation points.
pub unsafe trait RawAllocator {
    /// Allocate `layout.size()` bytes; return null on failure.
    ///
    /// # Safety
    ///
    /// `layout.size()` must be non-zero. See the
    /// [trait-level contract](RawAllocator#safety).
    unsafe fn alloc(&self, layout: Layout) -> *mut u8;

    /// Allocate initialized, zeroed `layout.size()` bytes; return null on failure.
    ///
    /// # Safety
    ///
    /// `layout.size()` must be non-zero. See the
    /// [trait-level contract](RawAllocator#safety). Initializedness is required
    /// for a non-null return; the zero values are checked as an oracle.
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8;

    /// Deallocate the block at `ptr` with its matching `layout`.
    ///
    /// # Safety
    ///
    /// `ptr` must denote a currently live block returned by this allocator,
    /// and `layout` must be its exact current layout. The sole exception is the
    /// authorized M2 call: one immediate repetition of the just-completed
    /// matching deallocation, issued by [`drive`](crate::drive) only when its
    /// [`DoubleFreeOk`](crate::DoubleFreeOk) contract is satisfied. See the
    /// [trait-level caller obligations](RawAllocator#guarantees-the-implementor-may-rely-on-from-callers).
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout);

    /// Resize the live block at `ptr` to `new_size` bytes.
    ///
    /// # Safety
    ///
    /// `ptr` must denote a currently live block returned by this allocator,
    /// `old_layout` must be its exact current layout, and `new_size` must be
    /// non-zero with an alignment round-up no greater than `isize::MAX`. The
    /// first `min(old_layout.size(), new_size)` bytes of the old block must be
    /// initialized. A non-null return consumes `ptr`; a null return leaves it
    /// live and unchanged. See the [trait-level contract](RawAllocator#safety).
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8;
}

/// # Safety
///
/// The trait-level caller obligations match the corresponding `GlobalAlloc`
/// preconditions: non-zero allocation sizes, a currently allocated matching
/// block for `dealloc` and `realloc`, and a valid non-zero `new_size` for
/// `realloc`. `RawAllocator` additionally requires the old realloc prefix to
/// be initialized before the call. `GlobalAlloc::realloc` preserves the old
/// prefix values, so forwarding also preserves that initialized prefix.
/// [`drive`](crate::drive) satisfies these conditions by validating and
/// clamping operation sizes, filling live blocks, and never enabling the M2
/// repeated deallocation for a blanket-reached allocator. Any other unsafe
/// caller of this implementation must uphold the same conditions.
unsafe impl<A: GlobalAlloc> RawAllocator for A {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller upholds the common non-zero-layout requirement.
        unsafe { GlobalAlloc::alloc(self, layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller upholds the common non-zero-layout requirement.
        unsafe { GlobalAlloc::alloc_zeroed(self, layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller provides the currently allocated matching block;
        // the blanket implementation is never used for the M2 exception.
        unsafe { GlobalAlloc::dealloc(self, ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller provides a live matching block and a valid,
        // non-zero new size whose alignment round-up fits in `isize`.
        unsafe { GlobalAlloc::realloc(self, ptr, old_layout, new_size) }
    }
}
