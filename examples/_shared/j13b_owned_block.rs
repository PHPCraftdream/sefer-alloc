//! Exact-layout ownership for a bounded judge allocation; no allocator metadata access.
use std::alloc::{GlobalAlloc, Layout};
use std::mem::MaybeUninit;
use std::ptr::NonNull;

/// Uniquely owns bytes and borrows its allocator until destruction.
///
/// Only `&mut self` can expose mutable storage or reallocate it. The payload
/// contains bytes, not a generic borrowed T; NonNull covariance adds no payload
/// lifetime. The allocator reference and explicit Drop provide drop checking,
/// so no additional PhantomData is needed. No Clone, raw-pointer getter or Sync
/// implementation permits a second payload owner or concurrent shared access.
pub(crate) struct OwnedBlock<'a, A: GlobalAlloc + Sync> {
    pointer: NonNull<u8>,
    layout: Layout,
    allocator: &'a A,
}

// SAFETY: this unique byte owner has no escaped payload alias during a move.
// Its borrowed allocator remains alive and A: Sync permits calls from the
// destination thread. GlobalAlloc's contract permits freeing/reallocating these
// owned bytes through the same allocator there. NonNull provenance is retained;
// no address reconstruction occurs. Shared access is intentionally not Sync.
unsafe impl<A: GlobalAlloc + Sync> Send for OwnedBlock<'_, A> {}

impl<'a, A: GlobalAlloc + Sync> OwnedBlock<'a, A> {
    pub(crate) const MAX_BYTES: usize = 2 * 1024 * 1024 * 1024;

    /// Bounds requested payload bytes, not mapping span, accepted advice or RSS.
    pub(crate) fn checked_layout(bytes: usize) -> Result<Layout, &'static str> {
        if bytes == 0 || bytes > Self::MAX_BYTES {
            return Err("requested bytes outside bounded judge range");
        }
        Layout::from_size_align(bytes, 8).map_err(|_| "invalid judge allocation layout")
    }

    #[inline(never)]
    pub(crate) fn allocate(allocator: &'a A, bytes: usize) -> Result<Self, &'static str> {
        let layout = Self::checked_layout(bytes)?;
        // SAFETY: checked_layout ensures a valid nonzero layout. GlobalAlloc
        // supplies uniquely owned storage or null; no contents are read here.
        let pointer = unsafe { allocator.alloc(layout) };
        let pointer = NonNull::new(pointer).ok_or("allocation failed")?;
        Ok(Self {
            pointer,
            layout,
            allocator,
        })
    }

    pub(crate) fn requested_bytes(&self) -> usize {
        self.layout.size()
    }

    /// Borrows the owned storage exclusively without assuming initialization.
    pub(crate) fn slots(&mut self) -> &mut [MaybeUninit<u8>] {
        // SAFETY: the nonnull allocation is live for layout.size() bytes in one
        // allocation, within isize::MAX and aligned for MaybeUninit<u8>. Every
        // bit pattern, including uninitialized storage, is valid for that type.
        // Only this &mut self creates a view; its lifetime prevents reallocation,
        // transfer or destruction while the exclusive reborrow remains usable.
        unsafe { std::slice::from_raw_parts_mut(self.pointer.as_ptr().cast(), self.layout.size()) }
    }

    /// On failure the original allocation, layout and initialized prefix survive.
    #[inline(never)]
    pub(crate) fn reallocate(&mut self, bytes: usize) -> Result<(), &'static str> {
        let next_layout = Self::checked_layout(bytes)?;
        // SAFETY: pointer is this allocator's unique live allocation with the
        // exact current layout. The validated new size is nonzero, keeps the
        // same alignment and fits Layout's isize bound. No payload view is live.
        let next = unsafe {
            self.allocator
                .realloc(self.pointer.as_ptr(), self.layout, bytes)
        };
        let next = NonNull::new(next).ok_or("reallocation failed")?;
        // Both assignments are infallible; no user code runs between them.
        self.pointer = next;
        self.layout = next_layout;
        Ok(())
    }
}

impl<A: GlobalAlloc + Sync> Drop for OwnedBlock<'_, A> {
    fn drop(&mut self) {
        // SAFETY: this remains the sole owner of the allocator's live pointer
        // and its exact current layout, including after a failed realloc.
        // All payload borrows have ended; Drop consumes ownership exactly once.
        unsafe { self.allocator.dealloc(self.pointer.as_ptr(), self.layout) };
    }
}
