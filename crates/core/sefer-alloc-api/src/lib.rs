#![deny(unsafe_code)]

use std::alloc::{AllocError, Allocator, GlobalAlloc, Layout};
use std::ptr::{self, NonNull};

use sefer_alloc::SeferAlloc;

/// An allocator handle backed by SeferAlloc's routed global allocation path.
///
/// Handles are intentionally neither cloneable nor copyable. Panics raised by
/// SeferAlloc during a global allocation operation invoke the process panic hook
/// when the callback panics, before unwinding reaches the private
/// `unsafe extern "C"` boundary. Unwinding reaching that boundary aborts rather
/// than crossing it. The hook may itself allocate, and that is an inherited risk.
pub struct SeferAllocator {
    inner: SeferAlloc,
}

impl SeferAllocator {
    /// Creates an allocator handle without initializing a thread heap.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            inner: SeferAlloc::new(),
        }
    }
}

impl Default for SeferAllocator {
    fn default() -> Self {
        Self::new()
    }
}

type Operation = unsafe fn(*mut (), *mut u8, Layout, Layout) -> *mut u8;

struct Call {
    operation: Operation,
    context: *mut (),
    ptr: *mut u8,
    old: Layout,
    new: Layout,
}

/// Synchronously invokes one GlobalAlloc operation across a non-unwind ABI.
///
/// # Safety
/// `call` must point to a live, uniquely borrowed `Call` for the duration of
/// this invocation. Its callback must obey the GlobalAlloc contract and may not
/// retain either the call or context pointer.
#[allow(unsafe_code)]
unsafe extern "C" fn trampoline(call: *mut Call) -> *mut u8 {
    // SAFETY: caller supplies a live stack Call and this synchronous function accesses it once.
    let call = unsafe { &mut *call };
    // SAFETY: callback and context are constructed together by the private caller.
    unsafe { (call.operation)(call.context, call.ptr, call.old, call.new) }
}

/// Invokes `GlobalAlloc::alloc` on the borrowed allocator.
///
/// # Safety
/// `context` must point to a live `SeferAlloc` for this synchronous call.
#[allow(unsafe_code)]
unsafe fn call_alloc(context: *mut (), _: *mut u8, _: Layout, layout: Layout) -> *mut u8 {
    // SAFETY: context points to the SeferAlloc borrowed for this synchronous call.
    unsafe { (&*context.cast::<SeferAlloc>()).alloc(layout) }
}

/// Invokes `GlobalAlloc::alloc_zeroed` on the borrowed allocator.
///
/// # Safety
/// `context` must point to a live `SeferAlloc` for this synchronous call.
#[allow(unsafe_code)]
unsafe fn call_zeroed(context: *mut (), _: *mut u8, _: Layout, layout: Layout) -> *mut u8 {
    // SAFETY: context points to the SeferAlloc borrowed for this synchronous call.
    unsafe { (&*context.cast::<SeferAlloc>()).alloc_zeroed(layout) }
}

/// Invokes `GlobalAlloc::dealloc` on the borrowed allocator.
///
/// # Safety
/// `context` must point to a live `SeferAlloc`; `ptr` and `layout` must be a
/// valid live GlobalAlloc allocation pair owned by the caller.
#[allow(unsafe_code)]
unsafe fn call_free(context: *mut (), ptr: *mut u8, layout: Layout, _: Layout) -> *mut u8 {
    // SAFETY: pointer/layout are the live block and exact layout provided by Allocator.
    unsafe { (&*context.cast::<SeferAlloc>()).dealloc(ptr, layout) };
    ptr
}

#[allow(unsafe_code)]
fn invoke(
    allocator: &SeferAlloc,
    operation: Operation,
    ptr: *mut u8,
    old: Layout,
    new: Layout,
) -> *mut u8 {
    let mut call = Call {
        operation,
        context: (allocator as *const SeferAlloc).cast_mut().cast(),
        ptr,
        old,
        new,
    };
    // SAFETY: trampoline is unsafe because it calls the callback through the
    // abort-on-unwind boundary; call stays live and exclusively borrowed until
    // synchronous return, and the callback cannot retain its address or context.
    unsafe { trampoline(&mut call) }
}

// SAFETY: all nonzero GlobalAlloc calls use the synchronous abort-on-unwind
// trampoline. Blocks route through process-wide SeferAlloc and remain releasable
// after the allocating thread exits. Zero-sized layouts use aligned dangling
// pointers without entering GlobalAlloc.
#[allow(unsafe_code)]
unsafe impl Allocator for SeferAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        if layout.size() == 0 {
            let Some(address) = std::num::NonZeroUsize::new(layout.align()) else {
                return Err(AllocError);
            };
            let ptr = NonNull::without_provenance(address);
            return Ok(NonNull::slice_from_raw_parts(ptr, 0));
        }
        let ptr = invoke(&self.inner, call_alloc, ptr::null_mut(), layout, layout);
        let ptr = NonNull::new(ptr).ok_or(AllocError)?;
        Ok(NonNull::slice_from_raw_parts(ptr, layout.size()))
    }

    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        if layout.size() == 0 {
            return self.allocate(layout);
        }
        let ptr = invoke(&self.inner, call_zeroed, ptr::null_mut(), layout, layout);
        let ptr = NonNull::new(ptr).ok_or(AllocError)?;
        Ok(NonNull::slice_from_raw_parts(ptr, layout.size()))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() != 0 {
            let _ = invoke(&self.inner, call_free, ptr.as_ptr(), layout, layout);
        }
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        // SAFETY: trait caller supplies the live allocation and grow-compatible layouts.
        unsafe { resize(self, ptr, old, new, false) }
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        // SAFETY: trait caller supplies the live allocation and grow-compatible layouts.
        unsafe { resize(self, ptr, old, new, true) }
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        // SAFETY: trait caller supplies the live allocation and shrink-compatible layouts.
        unsafe { resize(self, ptr, old, new, false) }
    }
}

/// Replaces an allocation transactionally, preserving the old block on failure.
///
/// # Safety
/// `ptr` and `old` must describe a live allocation from `allocator`; `new` must
/// have the size direction required by the requested resize operation.
#[allow(unsafe_code)]
unsafe fn resize(
    allocator: &SeferAllocator,
    ptr: NonNull<u8>,
    old: Layout,
    new: Layout,
    zero_tail: bool,
) -> Result<NonNull<[u8]>, AllocError> {
    if new.size() == 0 {
        // No valid GlobalAlloc zero-size call: return an aligned dangling pointer.
        let Some(address) = std::num::NonZeroUsize::new(new.align()) else {
            return Err(AllocError);
        };
        let dangling = NonNull::without_provenance(address);
        // SAFETY: ptr/layout are the caller's live allocation pair.
        unsafe { allocator.deallocate(ptr, old) };
        return Ok(NonNull::slice_from_raw_parts(dangling, 0));
    }

    let result = if zero_tail {
        allocator.allocate_zeroed(new)?
    } else {
        allocator.allocate(new)?
    };
    let destination = result.cast::<u8>().as_ptr();
    let copied = old.size().min(new.size());
    // SAFETY: successful allocation is distinct from the still-live old block;
    // both ranges cover copied bytes and do not overlap.
    unsafe { ptr::copy_nonoverlapping(ptr.as_ptr(), destination, copied) };
    if zero_tail && new.size() > copied {
        // SAFETY: destination's newly allocated range covers the zeroed tail.
        unsafe { destination.add(copied).write_bytes(0, new.size() - copied) };
    }
    // SAFETY: old block is freed only after successful replacement allocation.
    unsafe { allocator.deallocate(ptr, old) };
    Ok(result)
}

#[cfg(feature = "panic-test")]
#[doc(hidden)]
/// Panics inside the private operation callback to exercise the ABI boundary.
pub fn panic_boundary_probe() {
    /// # Safety
    /// This callback is invoked synchronously with an unused null context and
    /// valid layouts; it never returns because it deliberately panics.
    #[allow(unsafe_code)]
    unsafe fn panic_call(_: *mut (), _: *mut u8, _: Layout, _: Layout) -> *mut u8 {
        panic!("probe")
    }

    let layout = Layout::from_size_align(1, 1).expect("valid probe layout");
    let _ = invoke(
        &SeferAlloc::new(),
        panic_call,
        ptr::null_mut(),
        layout,
        layout,
    );
}
