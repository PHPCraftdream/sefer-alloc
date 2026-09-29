use core::cell::Cell;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicPtr, Ordering};

use super::RouteDirectory;

/// Owner-held admission handle; dropping closes admission without waiting
/// for an existing pin. The reservation belongs to the owner, not this handle.
pub struct RouteRegistration<'a> {
    pub(super) directory: &'a RouteDirectory,
    pub(super) key: usize,
    pub(super) index: usize,
    pub(super) incarnation: u64,
    pub(super) root: AtomicPtr<u8>,
    pub(super) owner_only: PhantomData<Cell<()>>,
}

impl RouteRegistration<'_> {
    /// Original allocator-origin root, never reconstructed from lookup key.
    pub fn root(&self) -> *mut u8 {
        self.root.load(Ordering::Relaxed)
    }

    pub fn incarnation(&self) -> u64 {
        self.incarnation
    }
}

impl Drop for RouteRegistration<'_> {
    fn drop(&mut self) {
        self.directory
            .remove(self.key, self.index, self.incarnation);
    }
}
