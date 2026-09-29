use core::cell::Cell;
use core::marker::PhantomData;

use super::directory::{EntryHandle, RouteDirectory};
use super::{LargeState, SmallSidecar};

/// Owner-held admission handle. Drop unlinks first, then releases its counted
/// descriptor reference without waiting for producers already holding pins.
pub struct RouteRegistration<'a> {
    pub(super) directory: &'a RouteDirectory,
    pub(super) entry: EntryHandle,
    pub(super) owner_only: PhantomData<Cell<()>>,
}

impl RouteRegistration<'_> {
    /// Allocator-origin usable root, never reconstructed from an address key.
    pub fn root(&self) -> *mut u8 {
        self.entry.root()
    }
    pub fn incarnation(&self) -> u64 {
        self.entry.incarnation()
    }
    pub fn small_sidecar(&self) -> Option<&SmallSidecar> {
        self.entry.small_sidecar()
    }
    pub fn large_state(&self) -> Option<&LargeState> {
        self.entry.large_state()
    }

    pub fn claim_large_pending(&self) -> Option<u64> {
        self.large_state()?.claim_pending()
    }

    pub fn cache_large_consumed(&self, generation: u64) -> bool {
        self.large_state()
            .is_some_and(|state| state.cache_consumed(generation))
    }

    /// Leaves the reservation INITIALIZING until owner reset completes.
    pub fn begin_large_reuse(&self) -> Option<u64> {
        self.large_state()?.begin_reuse()
    }

    /// Owner-only. Caller guarantees layout/table reset is complete; this
    /// substrate changes only the sidecar phase, not reservation bytes.
    pub fn finish_large_reuse_after_reset(&self, generation: u64) -> bool {
        self.large_state()
            .is_some_and(|state| state.finish_reuse(generation))
    }

    pub fn release_cached_large(&self, generation: u64) -> bool {
        self.large_state()
            .is_some_and(|state| state.release_cached(generation))
    }
}

impl Drop for RouteRegistration<'_> {
    fn drop(&mut self) {
        self.directory.remove(&self.entry);
    }
}
