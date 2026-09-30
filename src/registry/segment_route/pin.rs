use super::directory::EntryHandle;
use super::RouteKind;

/// Counted descriptor and sidecar capability, not a reservation credit.
/// No allocator-origin root is accessible through a producer pin.
pub struct RoutePin {
    pub(super) entry: EntryHandle,
}

impl RoutePin {
    pub fn owner(&self) -> usize {
        self.entry.owner()
    }
    pub fn kind(&self) -> RouteKind {
        self.entry.kind()
    }
    pub fn incarnation(&self) -> u64 {
        self.entry.incarnation()
    }
    /// Capacity validation reads only the independent immutable descriptor.
    pub fn contains_payload(&self, ptr: *mut u8, size: usize) -> bool {
        self.entry.contains_payload(ptr.addr(), size)
    }
    /// Read-only terminal-publication oracle; does not grant owner mutation.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn pending_for_test(&self, ptr: *mut u8) -> bool {
        if let Some(state) = self.entry.large_state() {
            state.pending_for_test()
        } else {
            let offset = (ptr.addr() & (crate::alloc_core::os::SEGMENT - 1)) as u32;
            self.entry
                .small_sidecar()
                .is_some_and(|sidecar| sidecar.pending_for_test(offset))
        }
    }
    /// Terminal publication consumes the producer capability. Drop then
    /// touches only the independently allocated descriptor/sidecar.
    ///
    /// # Safety
    /// This pin and `offset` identify the caller's current issued Small or
    /// Primordial allocation. The caller transfers its unique ownership once
    /// and must not access or free that allocation after successful publication.
    /// Address lookup alone does not establish this ownership.
    #[allow(unsafe_code)]
    pub unsafe fn publish_small(self, offset: u32) -> bool {
        self.entry
            .small_sidecar()
            .is_some_and(|sidecar| sidecar.publish(offset))
    }
    /// Terminal LIVE(g) -> PENDING(g) CAS, consuming this pin.
    ///
    /// # Safety
    /// This pin identifies the caller's current issued Large allocation.
    /// The caller transfers its unique ownership once and must not access or
    /// free that allocation after successful publication.
    #[allow(unsafe_code)]
    pub unsafe fn publish_large(self) -> bool {
        self.entry
            .large_state()
            .is_some_and(|state| state.publish_pending())
    }
}
