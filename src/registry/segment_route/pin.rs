use super::directory::EntryHandle;
use super::{LargePublisher, RouteKind, SmallPublisher};

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
    pub fn small_publisher(&self) -> Option<SmallPublisher<'_>> {
        self.entry
            .small_sidecar()
            .map(|sidecar| SmallPublisher { sidecar })
    }
    pub fn large_publisher(&self) -> Option<LargePublisher<'_>> {
        self.entry
            .large_state()
            .map(|state| LargePublisher { state })
    }
}
