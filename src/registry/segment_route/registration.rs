use core::cell::Cell;
use core::marker::PhantomData;
use core::mem::ManuallyDrop;

use super::directory::{EntryHandle, RouteDirectory};
use super::{LargeState, SmallSidecar};

/// Owner-held admission handle. Drop unlinks first, then releases its counted
/// descriptor reference without waiting for producers already holding pins.
pub struct RouteRegistration<'a> {
    pub(super) directory: &'a RouteDirectory,
    pub(super) entry: ManuallyDrop<EntryHandle>,
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
    /// Owner-only, fallible preparation before an issue transaction mutates
    /// anything. False means preparation failed; no side effect is kept.
    pub fn prepare_small(&self, offset: u32, class: u8) -> bool {
        self.small_sidecar()
            .is_some_and(|sidecar| sidecar.prepare(offset, class))
    }
    /// Owner-only class issue before allocation handoff; false means this is
    /// not a valid Small issue.
    pub fn issue_small(&self, offset: u32, class: u8) -> bool {
        self.small_sidecar()
            .is_some_and(|sidecar| sidecar.issue(offset, class))
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn class_at_global_address_for_test(address: usize) -> Option<u8> {
        let key = core::ptr::without_provenance_mut::<u8>(address);
        let pin = RouteDirectory::global().lookup(key)?;
        let offset = u32::try_from(address & (crate::alloc_core::os::SEGMENT - 1)).ok()?;
        pin.entry.small_sidecar()?.class_at_for_test(offset)
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
    /// # Safety
    /// Drop the counted owner exactly once, only after successful unlink;
    /// failed unlink must retain the descriptor and sidecar owner count.
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        if self.directory.remove(&self.entry) {
            // SAFETY: successful unlink released the shard guard before this
            // sole owner-count drop. ManuallyDrop suppresses automatic cleanup.
            unsafe { ManuallyDrop::drop(&mut self.entry) };
        }
        // Failed unlink leaves a non-owning shard pointer: retain its owner
        // count intentionally, including its independent sidecar, until exit.
    }
}
