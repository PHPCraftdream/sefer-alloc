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

    pub fn activate_large(&self) -> bool {
        self.large_state().is_some_and(LargeState::activate)
    }

    pub fn claim_large(&self) -> bool {
        self.large_state().is_some_and(LargeState::claim)
    }

    pub fn cache_large(&self) -> bool {
        self.large_state().is_some_and(LargeState::cache)
    }

    pub fn reissue_large(&self) -> bool {
        self.large_state().is_some_and(LargeState::reissue)
    }

    pub fn retire_cached_large(&self) -> bool {
        self.large_state().is_some_and(LargeState::retire_cached)
    }
}

impl Drop for RouteRegistration<'_> {
    fn drop(&mut self) {
        self.directory.remove(&self.entry);
    }
}
