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
    /// Terminal publication consumes the producer capability. Drop then
    /// touches only the independently allocated descriptor/sidecar.
    pub fn publish_small(self, offset: u32) -> bool {
        self.entry
            .small_sidecar()
            .is_some_and(|sidecar| sidecar.publish(offset))
    }
    /// Terminal LIVE(g) -> PENDING(g) CAS, consuming this pin.
    pub fn publish_large(self) -> bool {
        self.entry
            .large_state()
            .is_some_and(|state| state.publish_pending())
    }
}
