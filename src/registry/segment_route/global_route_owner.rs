use super::RouteRegistration;
use core::{cell::Cell, marker::PhantomData, mem::ManuallyDrop, ptr::NonNull};

/// One global-directory owner count, with a null niche and no allocation.
/// The opaque non-generic pointer has no lifetime variance; Cell keeps this
/// owner !Sync and NonNull keeps it !Send. No reservation borrow is exposed.
/// Only a registration from the immortal global directory can create it.
pub(crate) struct GlobalRouteOwner {
    pointer: NonNull<()>,
    owner_only: PhantomData<Cell<()>>,
}

impl GlobalRouteOwner {
    /// # Safety
    /// pointer must own a live global Entry count transferred from EntryHandle.
    #[allow(unsafe_code)]
    pub(super) unsafe fn new(pointer: NonNull<()>) -> Self {
        Self {
            pointer,
            owner_only: PhantomData,
        }
    }

    #[inline(always)]
    #[allow(unsafe_code)]
    fn with_entry<R>(&self, f: impl FnOnce(&super::directory::EntryHandle) -> R) -> R {
        // SAFETY: self retains its live owner count throughout the temporary
        // borrow. ManuallyDrop prevents decrementing it; R cannot borrow this
        // local view. No directory lookup or reservation access is performed.
        let view = ManuallyDrop::new(unsafe {
            super::directory::EntryHandle::from_owner_pointer(self.pointer)
        });
        f(&view)
    }

    #[inline(always)]
    pub(crate) fn matches_identity(&self, base: *mut u8, payload: *mut u8) -> bool {
        self.with_entry(|entry| entry.root() == base && entry.payload_addr() == payload.addr())
    }

    #[inline(always)]
    pub(crate) fn with_large_state<R>(&self, f: impl FnOnce(&super::LargeState) -> R) -> R {
        self.with_entry(|entry| f(entry.large_state().unwrap_or_else(|| std::process::abort())))
    }

    #[inline(always)]
    #[allow(unsafe_code)]
    pub(crate) fn into_registration(self) -> RouteRegistration<'static> {
        let this = ManuallyDrop::new(self);
        // SAFETY: this suppresses owner Drop and transfers its count once.
        unsafe { RouteRegistration::from_global_owner(this.pointer) }
    }
}

impl Drop for GlobalRouteOwner {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: self still owns the transferred count. Registration Drop
        // unlinks before releasing it and intentionally retains it on failure.
        drop(unsafe { RouteRegistration::from_global_owner(self.pointer) });
    }
}
